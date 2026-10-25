//! Bounded guest-call region summaries from MIR alone. Normal-return mutations
//! and every transient store are distinct; divergent calls still have stores.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    Dereference(NodeId),
    Project(Projection),
}
#[derive(Debug, Eq, PartialEq)]
struct Input {
    parameter: usize,
    steps: Vec<Step>,
    node: NodeId,
}
#[derive(Debug, Eq, PartialEq)]
enum Template {
    Empty,
    Brands(Vec<usize>),
    Record(Vec<usize>),
    Sum(Vec<Vec<usize>>),
    References(Vec<usize>),
}
#[derive(Debug, Eq, PartialEq)]
struct Output {
    destination: usize,
    value: usize,
}
#[derive(Debug, Eq, PartialEq)]
struct Requirement {
    destination: usize,
    sources: Vec<usize>,
    span: Span,
}
pub(super) struct Store {
    root: usize,
    brands: Vec<Brand>,
    span: Span,
}

/// Summary indices name this immutable analysis's input projections, not
/// runtime addresses, HIR regions, or permission to execute serialized MIR.
#[derive(Debug, Eq, PartialEq)]
pub struct Summary {
    returns: bool,
    inputs: Vec<Input>,
    values: Vec<Template>,
    outputs: Vec<Output>,
    requirements: Vec<Requirement>,
}
impl Summary {
    fn bottom() -> Self {
        Self {
            returns: false,
            inputs: Vec::new(),
            values: Vec::new(),
            outputs: Vec::new(),
            requirements: Vec::new(),
        }
    }
    pub const fn may_return(&self) -> bool {
        self.returns
    }
    pub fn mutation_count(&self) -> usize {
        self.outputs.len()
    }
    pub fn transient_store_count(&self) -> usize {
        self.requirements.len()
    }
    pub fn input_projection_count(&self) -> usize {
        self.inputs.len()
    }
    fn cost(&self) -> Result<usize> {
        let mut n = add(1, add(self.outputs.len(), self.values.len())?)?;
        for input in &self.inputs {
            n = add(n, add(1, input.steps.len())?)?;
        }
        for value in &self.values {
            n = add(n, Self::template_cost(value)?)?;
        }
        for store in &self.requirements {
            n = add(n, add(1, store.sources.len())?)?;
        }
        Ok(n)
    }
    fn template_cost(value: &Template) -> Result<usize> {
        Ok(match value {
            Template::Empty => 1,
            Template::Brands(v) | Template::Record(v) | Template::References(v) => add(1, v.len())?,
            Template::Sum(v) => v
                .iter()
                .try_fold(add(1, v.len())?, |n, v| add(n, v.len()))?,
        })
    }
    fn input(&mut self, input: Input, admit: &mut Admission<'_>) -> Result<usize> {
        for (id, old) in self.inputs.iter().enumerate() {
            admit
                .budget
                .charge(add(add(1, old.steps.len())?, input.steps.len())?)?;
            if old == &input {
                return Ok(id);
            }
        }
        admit.node()?;
        let id = self.inputs.len();
        admit.push(&mut self.inputs, input)?;
        Ok(id)
    }
    fn value(&mut self, value: Template, admit: &mut Admission<'_>) -> Result<usize> {
        for (id, old) in self.values.iter().enumerate() {
            admit.budget.charge(add(
                Self::template_cost(&value)?,
                Self::template_cost(old)?,
            )?)?;
            if old == &value {
                return Ok(id);
            }
        }
        admit.node()?;
        let id = self.values.len();
        admit.push(&mut self.values, value)?;
        Ok(id)
    }
}
pub struct Analysis<'p, 'a> {
    plan: &'p Plan<'a>,
    functions: Vec<FunctionAnalysis>,
    summaries: Vec<Summary>,
    usage: m::Usage,
    nodes: usize,
    cells: usize,
    rounds: usize,
}
impl<'p, 'a> Analysis<'p, 'a> {
    pub fn plan(&self) -> &'p Plan<'a> {
        self.plan
    }
    pub fn functions(&self) -> &[FunctionAnalysis] {
        &self.functions
    }
    pub fn summaries(&self) -> &[Summary] {
        &self.summaries
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub const fn nodes_used(&self) -> usize {
        self.nodes
    }
    pub const fn state_cells(&self) -> usize {
        self.cells
    }
    pub const fn rounds(&self) -> usize {
        self.rounds
    }
}
/// Derive a least fixed point over guest calls, then validate every reachable
/// operation using those effects. Failure/VM/artifact admission is separate.
pub fn analyze<'p, 'a>(plan: &'p Plan<'a>, limits: Limits) -> Result<Analysis<'p, 'a>> {
    let mut budget = Budget::with_usage(
        m::Limits {
            work: limits.work,
            allocations: limits.allocations,
            elements: limits.elements,
            depth: limits.depth,
            ..Default::default()
        },
        plan.usage(),
    )?;
    analyze_in(plan, limits, &mut budget)
}

pub(crate) fn analyze_in<'p, 'a>(
    plan: &'p Plan<'a>,
    limits: Limits,
    budget: &mut Budget,
) -> Result<Analysis<'p, 'a>> {
    let mut admit = Admission {
        budget,
        limits,
        nodes: 0,
        cells: 0,
    };
    let count = plan.functions.len();
    let mut summaries = admit.budget.vector(count)?;
    for _ in 0..count {
        admit.budget.charge(1)?;
        summaries.push(Summary::bottom());
    }
    let mut rounds = 0;
    loop {
        admit.budget.charge(1)?;
        rounds = add(rounds, 1)?;
        let mut next = admit.budget.vector(count)?;
        let mut changed = false;
        for index in 0..count {
            let mut local = run(plan, index, &mut admit, &summaries, false)?;
            let summary = local
                .engine
                .summarize(plan, index, &mut admit)
                .map_err(|e| at_function(plan, index, e))?;
            admit
                .budget
                .charge(add(summary.cost()?, summaries[index].cost()?)?)?;
            changed |= summary != summaries[index];
            next.push(summary);
        }
        summaries = next;
        if !changed {
            break;
        }
    }
    let mut functions = admit.budget.vector(count)?;
    for index in 0..count {
        functions.push(run(plan, index, &mut admit, &summaries, true)?);
    }
    Ok(Analysis {
        plan,
        functions,
        summaries,
        usage: m::Usage {
            work: admit.budget.work_used(),
            allocation_bytes: admit.budget.allocation_bytes(),
            elements: admit.budget.elements_used(),
        },
        nodes: admit.nodes,
        cells: admit.cells,
        rounds,
    })
}
fn at_function(plan: &Plan<'_>, index: usize, mut e: Error) -> Error {
    e.function = Some(FunctionId(index as u32));
    if e.primary.is_none() {
        e.primary = Some(
            plan.source
                .activity()
                .moved()
                .typed()
                .places()
                .declarations()
                .functions()[index]
                .span,
        );
    }
    e
}
fn run(
    plan: &Plan<'_>,
    index: usize,
    admit: &mut Admission<'_>,
    summaries: &[Summary],
    check: bool,
) -> Result<FunctionAnalysis> {
    super::function(plan, index, admit, Some(summaries), check)
        .map_err(|e| at_function(plan, index, e))
}

#[allow(clippy::too_many_arguments)]
impl Engine {
    pub fn record_store(
        &mut self,
        root: usize,
        brands: Vec<Brand>,
        span: Span,
        admit: &mut Admission<'_>,
    ) -> Result<()> {
        if !self.record || !matches!(self.roots[root].kind, RootKind::Caller { .. }) {
            return Ok(());
        }
        let value = self.brands(brands, admit)?;
        let Data::Brands(brands) = self.data(value, admit)? else {
            return Ok(());
        };
        for store in &mut self.stores {
            admit.budget.charge(add(1, store.brands.len())?)?;
            if store.root == root && store.span == span {
                for brand in brands {
                    let mut found = false;
                    for old in &store.brands {
                        admit.budget.charge(1)?;
                        if old == &brand {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        admit.push(&mut store.brands, brand)?;
                    }
                }
                return Ok(());
            }
        }
        admit.node()?;
        admit.push(&mut self.stores, Store { root, brands, span })?;
        Ok(())
    }
    pub fn capture_outputs(
        &mut self,
        plan: &Plan<'_>,
        row: &[usize],
        admit: &mut Admission<'_>,
    ) -> Result<()> {
        for (root, value) in row.iter().enumerate().take(self.roots.len()) {
            admit.budget.charge(1)?;
            if matches!(self.roots[root].kind, RootKind::Caller { .. }) {
                self.outputs[root] = if self.returns {
                    self.join(
                        plan,
                        self.outputs[root],
                        *value,
                        self.roots[root].node,
                        admit,
                        0,
                    )?
                } else {
                    *value
                };
            }
        }
        self.returns = true;
        Ok(())
    }
    fn input_for_place(
        &self,
        plan: &Plan<'_>,
        index: usize,
        place: usize,
        admit: &mut Admission<'_>,
        depth: usize,
    ) -> Result<Input> {
        admit.depth(depth)?;
        let (root, path) = self.place_parts(place, admit)?;
        let mut input = match self.roots[root].kind {
            RootKind::Slot if root < self.local_count() => {
                let declaration = &plan
                    .source
                    .activity()
                    .moved()
                    .typed()
                    .places()
                    .declarations()
                    .functions()[index];
                let mut ordinal = None;
                for (n, parameter) in declaration.parameters.iter().enumerate() {
                    admit.budget.charge(1)?;
                    if parameter.0 as usize == root {
                        ordinal = Some(n);
                        break;
                    }
                }
                Input {
                    parameter: ordinal
                        .ok_or_else(|| admit.error(ErrorKind::LocalReferenceEscape))?,
                    steps: Vec::new(),
                    node: self.roots[root].node,
                }
            }
            RootKind::Caller { input } => {
                let mut input = self.input_for_place(plan, index, input, admit, depth + 1)?;
                admit.push(&mut input.steps, Step::Dereference(self.roots[root].node))?;
                input.node = self.roots[root].node;
                input
            }
            _ => return Err(admit.error(ErrorKind::LocalReferenceEscape)),
        };
        for projection in path {
            admit.push(&mut input.steps, Step::Project(projection))?;
            input.node = projection.node;
        }
        Ok(input)
    }
    fn source(
        &self,
        plan: &Plan<'_>,
        index: usize,
        brand: Brand,
        summary: &mut Summary,
        admit: &mut Admission<'_>,
    ) -> Result<usize> {
        match brand.origin {
            Origin::Caller { place, .. } => {
                let input = self.input_for_place(plan, index, place, admit, 0)?;
                summary.input(input, admit)
            }
            Origin::Arena(_) => Err(self.error(
                plan,
                index,
                ErrorKind::Escape { incoming: brand },
                plan.source
                    .activity()
                    .moved()
                    .typed()
                    .places()
                    .declarations()
                    .functions()[index]
                    .span,
                Some(brand),
            )),
        }
    }
    fn template(
        &self,
        plan: &Plan<'_>,
        index: usize,
        value: usize,
        summary: &mut Summary,
        admit: &mut Admission<'_>,
        depth: usize,
    ) -> Result<usize> {
        admit.depth(depth)?;
        let value = match self.data(value, admit)? {
            Data::Empty => Template::Empty,
            Data::Brands(brands) => {
                let mut sources = admit.budget.vector(brands.len())?;
                for b in brands {
                    sources.push(self.source(plan, index, b, summary, admit)?);
                }
                Self::sort(&mut sources, admit)?;
                Template::Brands(sources)
            }
            Data::Record(fields) => {
                let mut out = admit.budget.vector(fields.len())?;
                for value in fields {
                    out.push(self.template(plan, index, value, summary, admit, depth + 1)?);
                }
                Template::Record(out)
            }
            Data::Sum(cases) => {
                let mut out = admit.budget.vector(cases.len())?;
                for fields in cases {
                    let mut payload = admit.budget.vector(fields.len())?;
                    for value in fields {
                        payload.push(self.template(
                            plan,
                            index,
                            value,
                            summary,
                            admit,
                            depth + 1,
                        )?);
                    }
                    out.push(payload);
                }
                Template::Sum(out)
            }
            Data::References(places) => {
                let mut out = admit.budget.vector(places.len())?;
                for place in places {
                    let (root, _) = self.place_parts(place, admit)?;
                    if !matches!(self.roots[root].kind, RootKind::Caller { .. }) {
                        return Err(admit.error(ErrorKind::LocalReferenceEscape));
                    }
                    let input = self.input_for_place(plan, index, place, admit, 0)?;
                    out.push(summary.input(input, admit)?);
                }
                Self::sort(&mut out, admit)?;
                Template::References(out)
            }
        };
        summary.value(value, admit)
    }
    fn sort(values: &mut Vec<usize>, admit: &mut Admission<'_>) -> Result<()> {
        for i in 1..values.len() {
            let mut j = i;
            while j > 0 {
                admit.budget.charge(1)?;
                if values[j - 1] <= values[j] {
                    break;
                }
                values.swap(j - 1, j);
                j -= 1;
            }
        }
        admit.budget.charge(values.len())?;
        values.dedup();
        Ok(())
    }
    fn summarize(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        admit: &mut Admission<'_>,
    ) -> Result<Summary> {
        let mut summary = Summary::bottom();
        summary.returns = self.returns;
        summary.value(Template::Empty, admit)?;
        // Seed symbolic caller inputs in a stable order before translating
        // normal outputs or recursively imported store requirements.
        for value in &self.values {
            admit.budget.charge(1)?;
            if let Data::Brands(brands) = value {
                for brand in brands {
                    admit.budget.charge(1)?;
                    if matches!(brand.origin, Origin::Caller { .. }) {
                        self.source(plan, index, *brand, &mut summary, admit)?;
                    }
                }
            }
        }
        if self.returns {
            for root in 0..self.roots.len() {
                admit.budget.charge(1)?;
                if self.wrote[root] && matches!(self.roots[root].kind, RootKind::Caller { .. }) {
                    let place = self.place(root, &[], admit)?;
                    let input = self.input_for_place(plan, index, place, admit, 0)?;
                    let destination = summary.input(input, admit)?;
                    let value =
                        self.template(plan, index, self.outputs[root], &mut summary, admit, 0)?;
                    admit.node()?;
                    admit.push(&mut summary.outputs, Output { destination, value })?;
                }
            }
        }
        for store in &self.stores {
            let input_place = match self.roots[store.root].kind {
                RootKind::Caller { input } => input,
                _ => unreachable!(),
            };
            let mut input = self.input_for_place(plan, index, input_place, admit, 0)?;
            admit.push(
                &mut input.steps,
                Step::Dereference(self.roots[store.root].node),
            )?;
            input.node = self.roots[store.root].node;
            let destination = summary.input(input, admit)?;
            let mut sources = admit.budget.vector(store.brands.len())?;
            for brand in &store.brands {
                sources.push(
                    self.source(plan, index, *brand, &mut summary, admit)
                        .map_err(|mut e| {
                            e.primary = Some(store.span);
                            e
                        })?,
                );
            }
            Self::sort(&mut sources, admit)?;
            admit.node()?;
            admit.push(
                &mut summary.requirements,
                Requirement {
                    destination,
                    sources,
                    span: store.span,
                },
            )?;
        }
        Ok(summary)
    }

    fn input_locations(
        &mut self,
        plan: &Plan<'_>,
        before: &[usize],
        arguments: &[m::Register],
        input: &Input,
        admit: &mut Admission<'_>,
        check: bool,
    ) -> Result<Vec<usize>> {
        let root = self.value_root(arguments[input.parameter]);
        let place = self.place(root, &[], admit)?;
        let mut out = admit.copy(&[place])?;
        for step in &input.steps {
            let mut next = Vec::new();
            for p in out {
                match step {
                    Step::Dereference(_) => {
                        let value = self.read_place(plan, before, p, admit)?;
                        let places = self.refs(value, admit, check)?;
                        for place in places {
                            admit.push(&mut next, place)?;
                        }
                    }
                    Step::Project(projection) => {
                        let p = self.append(p, *projection, admit)?;
                        admit.push(&mut next, p)?;
                    }
                }
            }
            out = next;
        }
        Ok(out)
    }
    fn instantiate(
        &mut self,
        summary: &Summary,
        value: usize,
        sources: &[usize],
        locations: &[Vec<usize>],
        admit: &mut Admission<'_>,
        depth: usize,
    ) -> Result<usize> {
        admit.depth(depth)?;
        match &summary.values[value] {
            Template::Empty => Ok(0),
            Template::Brands(inputs) => {
                let mut brands = Vec::new();
                for input in inputs {
                    match self.data(sources[*input], admit)? {
                        Data::Empty => {}
                        Data::Brands(v) => {
                            for b in v {
                                admit.push(&mut brands, b)?;
                            }
                        }
                        _ => return Err(admit.error(ErrorKind::InvalidShape)),
                    }
                }
                self.brands(brands, admit)
            }
            Template::Record(fields) => {
                let mut out = admit.budget.vector(fields.len())?;
                for field in fields {
                    out.push(self.instantiate(
                        summary,
                        *field,
                        sources,
                        locations,
                        admit,
                        depth + 1,
                    )?);
                }
                self.intern(Data::Record(out), admit)
            }
            Template::Sum(cases) => {
                let mut out = admit.budget.vector(cases.len())?;
                for fields in cases {
                    let mut payload = admit.budget.vector(fields.len())?;
                    for field in fields {
                        payload.push(self.instantiate(
                            summary,
                            *field,
                            sources,
                            locations,
                            admit,
                            depth + 1,
                        )?);
                    }
                    out.push(payload);
                }
                self.intern(Data::Sum(out), admit)
            }
            Template::References(inputs) => {
                let mut out = Vec::new();
                for input in inputs {
                    for p in &locations[*input] {
                        admit.push(&mut out, *p)?;
                    }
                }
                self.reference(out, admit)
            }
        }
    }
    pub fn apply_call(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        row: &mut [usize],
        callee: FunctionId,
        arguments: &[m::Register],
        summary: &Summary,
        span: Span,
        admit: &mut Admission<'_>,
        check: bool,
    ) -> Result<bool> {
        // Resolve all values AND destination/reference paths against one
        // pre-call snapshot. Publication cannot reread a rebound pointer cell.
        admit.cells(row.len())?;
        let before = admit.copy(row)?;
        let mut locations = admit.budget.vector(summary.inputs.len())?;
        let mut sources = admit.budget.vector(summary.inputs.len())?;
        for input in &summary.inputs {
            let places = self.input_locations(plan, &before, arguments, input, admit, check)?;
            let value = self.read_locations(plan, &before, &places, input.node, admit)?;
            let brands = self.collect(&before, value, admit, 0)?;
            sources.push(self.brands(brands, admit)?);
            locations.push(places);
        }
        for requirement in &summary.requirements {
            let mut brands = Vec::new();
            for source in &requirement.sources {
                if let Data::Brands(v) = self.data(sources[*source], admit)? {
                    for b in v {
                        admit.push(&mut brands, b)?;
                    }
                }
            }
            let incoming = self.brands(brands, admit)?;
            for place in &locations[requirement.destination] {
                let (root, _) = self.place_parts(*place, admit)?;
                let foreign = matches!(self.roots[root].kind, RootKind::Caller { .. });
                self.storage(
                    plan,
                    index,
                    row,
                    root,
                    incoming,
                    span,
                    admit,
                    check || foreign,
                )
                .map_err(|mut e| {
                    if let ErrorKind::Escape { incoming } = e.kind {
                        e.kind = ErrorKind::CallStore {
                            callee,
                            store: requirement.span,
                            incoming,
                        };
                        e.secondary = Some(requirement.span);
                    }
                    e
                })?;
                let brands = self.collect(&before, incoming, admit, 0)?;
                self.record_store(root, brands, requirement.span, admit)?;
            }
        }
        if !summary.returns {
            return Ok(false);
        }
        let mut values = admit.budget.vector(summary.outputs.len())?;
        for output in &summary.outputs {
            values.push(self.instantiate(summary, output.value, &sources, &locations, admit, 0)?);
        }
        for (output, value) in summary.outputs.iter().zip(values) {
            self.store(
                plan,
                index,
                row,
                &locations[output.destination],
                value,
                span,
                admit,
                check,
            )?;
        }
        Ok(true)
    }
}
