//! Finite summaries of normal-return values and every transient store.
//! Inputs are projections of the caller's pre-call values. Containers retain
//! the parent analysis's conservative element abstraction.

use super::*;

pub(super) struct ParameterRoot {
    root: usize,
    parameter: r::LocalId,
    dereferences: usize,
    ty: r::Type,
    incoming: Vec<BrandId>,
    wrote: bool,
    span: Option<Span>,
}
#[derive(Debug, Eq, PartialEq)]
enum Template {
    Empty,
    Brands(Vec<usize>),
    Record(r::DefinitionId, Vec<usize>),
    Case(Tag, Vec<usize>),
    Cases(Vec<usize>),
}
impl Template {
    fn cost(&self) -> usize {
        1 + match self {
            Self::Empty => 0,
            Self::Brands(v) | Self::Record(_, v) | Self::Case(_, v) | Self::Cases(v) => v.len(),
        }
    }
}
#[derive(Debug, Eq, PartialEq)]
struct Output {
    parameter: r::LocalId,
    dereferences: usize,
    value: usize,
}
#[derive(Debug, Eq, PartialEq)]
struct Requirement {
    parameter: r::LocalId,
    dereferences: usize,
    sources: Vec<usize>,
    span: Span,
}
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Summary {
    may_return: bool,
    inputs: Vec<CallerOrigin>,
    values: Vec<Template>,
    outputs: Vec<Output>,
    requirements: Vec<Requirement>,
}
impl Summary {
    pub(super) fn bottom() -> Self {
        Self {
            may_return: false,
            inputs: Vec::new(),
            values: Vec::new(),
            outputs: Vec::new(),
            requirements: Vec::new(),
        }
    }
    pub(super) fn cost(&self) -> usize {
        1 + self.inputs.iter().map(|i| i.path.len() + 1).sum::<usize>()
            + self.values.iter().map(Template::cost).sum::<usize>()
            + self.outputs.len()
            + self
                .requirements
                .iter()
                .map(|r| r.sources.len() + 1)
                .sum::<usize>()
    }
    fn intern(&mut self, node: Template, budget: &mut Budget, span: Span) -> Result<usize, Error> {
        for (index, previous) in self.values.iter().enumerate() {
            budget.charge(span, node.cost().saturating_add(previous.cost()))?;
            if *previous == node {
                return Ok(index);
            }
        }
        let id = self.values.len();
        budget.values = budget
            .values
            .checked_add(1)
            .filter(|n| *n <= budget.limits.values)
            .ok_or_else(|| budget.limit(span, "region summary value limit exceeded"))?;
        budget.push(span, &mut self.values, node)?;
        Ok(id)
    }
    fn case(
        &mut self,
        tag: Tag,
        values: Vec<usize>,
        budget: &mut Budget,
        span: Span,
    ) -> Result<usize, Error> {
        budget.charge(span, values.len())?;
        if values.iter().all(|v| *v == 0) {
            Ok(0)
        } else {
            self.intern(Template::Case(tag, values), budget, span)
        }
    }
}

impl Checker<'_, '_> {
    pub(super) fn parameter(
        &mut self,
        local: r::LocalId,
        ty: r::Type,
        depth: usize,
    ) -> Result<ValueId, Error> {
        self.parameter_value(local, ty, depth, true)
    }
    fn parameter_value(
        &mut self,
        local: r::LocalId,
        ty: r::Type,
        depth: usize,
        mutable: bool,
    ) -> Result<ValueId, Error> {
        let span = self.function.locals[local.0].name.span;
        self.context.budget.enter(span, depth)?;
        if let Some(r::TypeConstructor::Loan(kind, target)) = self.constructor(ty) {
            let mutable = mutable && kind == r::LoanKind::Exclusive;
            let value = self.parameter_value(local, target, depth + 1, mutable)?;
            let root = self.root(target, 0, value, span)?;
            if mutable && self.loan(target).is_none() && self.branded(target) {
                self.context.budget.push(
                    span,
                    &mut self.parameter_roots,
                    ParameterRoot {
                        root,
                        parameter: local,
                        dereferences: depth + 1,
                        ty: target,
                        incoming: Vec::new(),
                        wrote: false,
                        span: None,
                    },
                )?;
            }
            self.root_reference(root, span)
        } else {
            self.input_value(local, depth, &[], ty, span, depth + 1)
        }
    }
    fn input_value(
        &mut self,
        parameter: r::LocalId,
        dereferences: usize,
        path: &[Step],
        ty: r::Type,
        span: Span,
        depth: usize,
    ) -> Result<ValueId, Error> {
        self.context.budget.enter(span, depth)?;
        if !self.branded(ty) {
            return Ok(EMPTY);
        }
        if let r::Type::Nominal(id) = ty
            && let r::DefinitionKind::Record(fields) = &self.bodies.source().definitions[id.0].kind
        {
            let mut values = self.context.budget.vector(span, fields.len())?;
            for (index, field) in fields.iter().enumerate() {
                let ty = field.value_type.value_type;
                let mut child = self.context.budget.copy(span, path)?;
                self.context.budget.push(
                    span,
                    &mut child,
                    Step {
                        kind: StepKind::Field(index),
                        ty,
                    },
                )?;
                values.push(self.input_value(
                    parameter,
                    dereferences,
                    &child,
                    ty,
                    span,
                    depth + 1,
                )?);
            }
            return self.record(id, values, span);
        }
        let mut cases = Vec::new();
        if let r::Type::Nominal(id) = ty
            && let r::DefinitionKind::Enum(variants) = &self.bodies.source().definitions[id.0].kind
        {
            for (index, variant) in variants.iter().enumerate() {
                let tag = Tag::User(id, index);
                let mut values = self.context.budget.vector(span, variant.payload.len())?;
                for (i, field) in variant.payload.iter().enumerate() {
                    let mut child = self.context.budget.copy(span, path)?;
                    self.context.budget.push(
                        span,
                        &mut child,
                        Step {
                            kind: StepKind::Payload(tag, i),
                            ty: field.value_type,
                        },
                    )?;
                    values.push(self.input_value(
                        parameter,
                        dereferences,
                        &child,
                        field.value_type,
                        span,
                        depth + 1,
                    )?);
                }
                let value = self.case(tag, values, span)?;
                if value != EMPTY {
                    self.context.budget.push(span, &mut cases, value)?;
                }
            }
        } else if matches!(
            self.constructor(ty),
            Some(r::TypeConstructor::Option(_) | r::TypeConstructor::Result(..))
        ) {
            for tag in [core::Case::Some, core::Case::Ok, core::Case::Err] {
                if let Some(ty) = self.core_payload(ty, tag) {
                    let mut child = self.context.budget.copy(span, path)?;
                    self.context.budget.push(
                        span,
                        &mut child,
                        Step {
                            kind: StepKind::Payload(Tag::Core(tag), 0),
                            ty,
                        },
                    )?;
                    let value =
                        self.input_value(parameter, dereferences, &child, ty, span, depth + 1)?;
                    let value = self.payload_result(tag, value, span)?;
                    if value != EMPTY {
                        self.context.budget.push(span, &mut cases, value)?;
                    }
                }
            }
        } else {
            let mut existing = None;
            for (i, origin) in self.origins.iter().enumerate() {
                self.context.budget.charge(span, path.len() + 1)?;
                if let Some(origin) = origin
                    && origin.parameter == parameter
                    && origin.dereferences == dereferences
                    && origin.path == path
                {
                    existing = Some(BrandId(i));
                    break;
                }
            }
            let brand = if let Some(id) = existing {
                id
            } else {
                let id = BrandId(self.brands.len());
                let origin = CallerOrigin {
                    parameter,
                    dereferences,
                    path: self.context.budget.copy(span, path)?,
                };
                self.context.budget.push(
                    span,
                    &mut self.brands,
                    Brand {
                        kind: BrandKind::Caller {
                            function: self.id,
                            parameter,
                        },
                        span,
                        scope: 0,
                    },
                )?;
                self.context
                    .budget
                    .push(span, &mut self.origins, Some(origin))?;
                id
            };
            return self.singleton_brand(brand, span);
        }
        if cases.is_empty() {
            Ok(EMPTY)
        } else if cases.len() == 1 {
            Ok(cases[0])
        } else {
            self.intern(Value::Cases(cases), span)
        }
    }
    pub(super) fn capture_outputs(&mut self, span: Span) -> Result<(), Error> {
        for index in 0..self.parameter_roots.len() {
            let root = self.parameter_roots[index].root;
            let ty = self.parameter_roots[index].ty;
            let value = self.get(root);
            if self.has_exit {
                self.outputs[index] = self.join(self.outputs[index], value, ty, span, 0)?;
            } else {
                self.context.budget.push(span, &mut self.outputs, value)?;
            }
        }
        self.has_exit = true;
        Ok(())
    }
    pub(super) fn record_store(
        &mut self,
        root: usize,
        value: ValueId,
        span: Span,
    ) -> Result<(), Error> {
        if !self.requirements {
            return Ok(());
        }
        self.context
            .budget
            .charge(span, self.parameter_roots.len())?;
        if let Some(index) = self.parameter_roots.iter().position(|p| p.root == root) {
            let brands = self.collect(value, span, 0)?;
            let p = &mut self.parameter_roots[index];
            p.wrote = true;
            if p.span
                .is_none_or(|previous| span.range().start < previous.range().start)
            {
                p.span = Some(span);
            }
            self.context.budget.union(span, &mut p.incoming, &brands)?;
        }
        Ok(())
    }
    pub(super) fn mutation_facts(&mut self) -> Result<Vec<Mutation>, Error> {
        let span = self.function.span;
        let mut out = Vec::new();
        for p in &self.parameter_roots {
            self.context.budget.charge(span, 1)?;
            if p.wrote {
                self.context.budget.push(
                    span,
                    &mut out,
                    Mutation {
                        parameter: p.parameter,
                        dereferences: p.dereferences,
                        span: p.span.unwrap(),
                    },
                )?;
            }
        }
        Ok(out)
    }
    pub(super) fn summarize(&mut self) -> Result<Summary, Error> {
        let span = self.function.span;
        let mut summary = Summary::bottom();
        summary.may_return = self.has_exit;
        summary.intern(Template::Empty, &mut self.context.budget, span)?;
        let mut indices = self.context.budget.vector(span, self.origins.len())?;
        for origin in &self.origins {
            self.context.budget.charge(span, 1)?;
            if let Some(origin) = origin {
                indices.push(Some(summary.inputs.len()));
                let origin = CallerOrigin {
                    parameter: origin.parameter,
                    dereferences: origin.dereferences,
                    path: self.context.budget.copy(span, &origin.path)?,
                };
                self.context
                    .budget
                    .push(span, &mut summary.inputs, origin)?;
            } else {
                indices.push(None);
            }
        }
        for index in 0..self.parameter_roots.len() {
            let p = &self.parameter_roots[index];
            let ty = p.ty;
            let parameter = p.parameter;
            let dereferences = p.dereferences;
            if self.has_exit {
                let value =
                    self.template(self.outputs[index], ty, &indices, &mut summary, span, 0)?;
                self.context.budget.push(
                    span,
                    &mut summary.outputs,
                    Output {
                        parameter,
                        dereferences,
                        value,
                    },
                )?;
            }
            let p = &self.parameter_roots[index];
            if self.requirements && p.wrote {
                let mut sources = Vec::new();
                for brand in &p.incoming {
                    self.context.budget.charge(span, 1)?;
                    if let Some(id) = indices[brand.0] {
                        self.context.budget.push(span, &mut sources, id)?;
                    }
                }
                self.context.budget.push(
                    span,
                    &mut summary.requirements,
                    Requirement {
                        parameter,
                        dereferences,
                        sources,
                        span: p.span.unwrap(),
                    },
                )?;
            }
        }
        Ok(summary)
    }
    fn template(
        &mut self,
        value: ValueId,
        ty: r::Type,
        indices: &[Option<usize>],
        summary: &mut Summary,
        span: Span,
        depth: usize,
    ) -> Result<usize, Error> {
        self.context.budget.enter(span, depth)?;
        if !self.branded(ty) {
            return Ok(0);
        }
        if let r::Type::Nominal(id) = ty
            && let r::DefinitionKind::Record(fields) = &self.bodies.source().definitions[id.0].kind
        {
            let mut values = self.context.budget.vector(span, fields.len())?;
            for (index, field) in fields.iter().enumerate() {
                let ty = field.value_type.value_type;
                let v = self.project(
                    value,
                    Step {
                        kind: StepKind::Field(index),
                        ty,
                    },
                    span,
                    depth + 1,
                )?;
                values.push(self.template(v, ty, indices, summary, span, depth + 1)?);
            }
            if values.iter().all(|v| *v == 0) {
                return Ok(0);
            }
            return summary.intern(Template::Record(id, values), &mut self.context.budget, span);
        }
        let mut cases = Vec::new();
        if let r::Type::Nominal(id) = ty
            && let r::DefinitionKind::Enum(variants) = &self.bodies.source().definitions[id.0].kind
        {
            for (index, variant) in variants.iter().enumerate() {
                let tag = Tag::User(id, index);
                let mut values = self.context.budget.vector(span, variant.payload.len())?;
                for (i, field) in variant.payload.iter().enumerate() {
                    let ty = field.value_type;
                    let v = self.project(
                        value,
                        Step {
                            kind: StepKind::Payload(tag, i),
                            ty,
                        },
                        span,
                        depth + 1,
                    )?;
                    values.push(self.template(v, ty, indices, summary, span, depth + 1)?);
                }
                let v = summary.case(tag, values, &mut self.context.budget, span)?;
                if v != 0 {
                    self.context.budget.push(span, &mut cases, v)?;
                }
            }
        } else if matches!(
            self.constructor(ty),
            Some(r::TypeConstructor::Option(_) | r::TypeConstructor::Result(..))
        ) {
            for tag in [core::Case::Some, core::Case::Ok, core::Case::Err] {
                if let Some(ty) = self.core_payload(ty, tag) {
                    let v = self.project(
                        value,
                        Step {
                            kind: StepKind::Payload(Tag::Core(tag), 0),
                            ty,
                        },
                        span,
                        depth + 1,
                    )?;
                    let v = self.template(v, ty, indices, summary, span, depth + 1)?;
                    let mut values = self.context.budget.vector(span, 1)?;
                    values.push(v);
                    let v = summary.case(Tag::Core(tag), values, &mut self.context.budget, span)?;
                    if v != 0 {
                        self.context.budget.push(span, &mut cases, v)?;
                    }
                }
            }
        } else {
            let mut sources = Vec::new();
            for brand in self.collect(value, span, depth + 1)? {
                if let Some(id) = indices[brand.0] {
                    self.context.budget.push(span, &mut sources, id)?;
                }
            }
            return if sources.is_empty() {
                Ok(0)
            } else {
                summary.intern(Template::Brands(sources), &mut self.context.budget, span)
            };
        }
        if cases.is_empty() {
            Ok(0)
        } else if cases.len() == 1 {
            Ok(cases[0])
        } else {
            summary.intern(Template::Cases(cases), &mut self.context.budget, span)
        }
    }
    fn parameter_index(
        &mut self,
        callee: r::FunctionId,
        parameter: r::LocalId,
        span: Span,
    ) -> Result<usize, Error> {
        let parameters = &self.bodies.source().functions[callee.0].parameters;
        self.context.budget.charge(span, parameters.len())?;
        Ok(parameters.iter().position(|p| *p == parameter).unwrap())
    }
    fn call_places(
        &mut self,
        callee: r::FunctionId,
        parameter: r::LocalId,
        dereferences: usize,
        arguments: &[ValueId],
        span: Span,
        depth: usize,
    ) -> Result<Vec<usize>, Error> {
        self.context.budget.enter(span, depth)?;
        let index = self.parameter_index(callee, parameter, span)?;
        let mut value = arguments[index];
        let mut ty = self.bodies.functions()[callee.0].locals()[parameter.0];
        for level in 0..dereferences {
            self.context.budget.enter(span, depth + level)?;
            let places = self.ref_places(value, span)?;
            if level + 1 == dereferences {
                return Ok(places);
            }
            ty = self.loan(ty).unwrap();
            value = self.load_locations(&places, ty, span, depth + level + 1)?;
        }
        unreachable!("mutable summaries require a borrowed target")
    }
    fn call_input(
        &mut self,
        callee: r::FunctionId,
        input: &CallerOrigin,
        arguments: &[ValueId],
        span: Span,
        depth: usize,
    ) -> Result<ValueId, Error> {
        let index = self.parameter_index(callee, input.parameter, span)?;
        let mut value = arguments[index];
        let mut ty = self.bodies.functions()[callee.0].locals()[input.parameter.0];
        for level in 0..input.dereferences {
            self.context.budget.enter(span, depth + level)?;
            ty = self.loan(ty).unwrap();
            let places = self.ref_places(value, span)?;
            value = self.load_locations(&places, ty, span, depth + level + 1)?;
        }
        for step in &input.path {
            value = self.project(value, *step, span, depth + 1)?;
        }
        Ok(value)
    }
    fn source_brands(
        &mut self,
        sources: &[usize],
        inputs: &[ValueId],
        span: Span,
        depth: usize,
    ) -> Result<ValueId, Error> {
        let mut brands = Vec::new();
        for source in sources {
            let v = self.collect(inputs[*source], span, depth + 1)?;
            self.context.budget.union(span, &mut brands, &v)?;
        }
        self.set(brands, span)
    }
    fn instantiate(
        &mut self,
        summary: &Summary,
        id: usize,
        inputs: &[ValueId],
        span: Span,
        depth: usize,
    ) -> Result<ValueId, Error> {
        self.context.budget.enter(span, depth)?;
        match &summary.values[id] {
            Template::Empty => Ok(EMPTY),
            Template::Brands(sources) => self.source_brands(sources, inputs, span, depth + 1),
            Template::Record(definition, values) => {
                let mut out = self.context.budget.vector(span, values.len())?;
                for id in values {
                    out.push(self.instantiate(summary, *id, inputs, span, depth + 1)?);
                }
                self.record(*definition, out, span)
            }
            Template::Case(tag, values) => {
                let mut out = self.context.budget.vector(span, values.len())?;
                for id in values {
                    out.push(self.instantiate(summary, *id, inputs, span, depth + 1)?);
                }
                self.case(*tag, out, span)
            }
            Template::Cases(values) => {
                let mut out = Vec::new();
                for id in values {
                    let v = self.instantiate(summary, *id, inputs, span, depth + 1)?;
                    if v != EMPTY {
                        self.context.budget.push(span, &mut out, v)?;
                    }
                }
                if out.is_empty() {
                    Ok(EMPTY)
                } else if out.len() == 1 {
                    Ok(out[0])
                } else {
                    self.intern(Value::Cases(out), span)
                }
            }
        }
    }
    pub(super) fn apply_call(
        &mut self,
        callee: r::FunctionId,
        arguments: &[ValueId],
        span: Span,
        depth: usize,
    ) -> Result<ValueId, Error> {
        self.context.budget.enter(span, depth)?;
        let summaries = self.summaries;
        let summary = &summaries[callee.0];
        // Snapshot every input before publishing any output, including inputs
        // reached through nested reference cells or another mutable argument.
        let mut inputs = self.context.budget.vector(span, summary.inputs.len())?;
        for input in &summary.inputs {
            inputs.push(self.call_input(callee, input, arguments, span, depth + 1)?);
        }
        if self.requirements {
            for requirement in &summary.requirements {
                let value = self.source_brands(&requirement.sources, &inputs, span, depth + 1)?;
                let places = self.call_places(
                    callee,
                    requirement.parameter,
                    requirement.dereferences,
                    arguments,
                    span,
                    depth + 1,
                )?;
                self.call_origin = Some(requirement.span);
                for place in places {
                    let root = self.places[place].root;
                    self.check_store(value, self.roots[root].scope, span, depth + 1)?;
                    self.record_store(root, value, span)?;
                }
                self.call_origin = None;
            }
        }
        if !summary.may_return {
            self.diverges = true;
            return Ok(EMPTY);
        }
        let mut outputs = self.context.budget.vector(span, summary.outputs.len())?;
        for output in &summary.outputs {
            let places = self.call_places(
                callee,
                output.parameter,
                output.dereferences,
                arguments,
                span,
                depth + 1,
            )?;
            let value = self.instantiate(summary, output.value, &inputs, span, depth + 1)?;
            outputs.push((places, value));
        }
        for (places, value) in outputs {
            let weak = places.len() != 1;
            for place in places {
                let p = self.places[place];
                let path = self.path(p.path, span)?;
                let old = self.get(p.root);
                let new = self.update(old, self.roots[p.root].ty, &path, value, span, depth + 1)?;
                let new = if weak {
                    self.join(old, new, self.roots[p.root].ty, span, depth + 1)?
                } else {
                    new
                };
                self.put(p.root, new, span)?;
            }
        }
        Ok(EMPTY)
    }
}
