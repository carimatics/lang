use super::*;
use crate::unit::ownership::core;

// Transfer keeps the immutable plan, mutable flow row, budget and diagnostic
// mode explicit. Propagation and the final check use the identical operations.
#[allow(clippy::too_many_arguments)]
impl Engine {
    pub(super) fn error(
        &self,
        plan: &Plan<'_>,
        index: usize,
        kind: ErrorKind,
        span: Span,
        brand: Option<Brand>,
    ) -> Error {
        let locals = &plan
            .source
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .functions()[index]
            .locals;
        let secondary = brand.map(|b| match b.origin {
            Origin::Arena(l) | Origin::Caller { parameter: l, .. } => locals[l.0 as usize].span,
        });
        Error {
            kind,
            function: Some(FunctionId(index as u32)),
            primary: Some(span),
            secondary,
        }
    }
    fn valid(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        row: &[usize],
        value: usize,
        span: Span,
        admit: &mut Admission,
        check: bool,
    ) -> Result<()> {
        if !check {
            return Ok(());
        }
        let active = row.len() - plan.functions[index].locals;
        for b in self.collect(row, value, admit, 0)? {
            if let Origin::Arena(owner) = b.origin
                && (b.expired || row[active + owner.0 as usize] != 1)
            {
                return Err(self.error(plan, index, ErrorKind::ExpiredArena(owner), span, Some(b)));
            }
        }
        Ok(())
    }
    pub(super) fn storage(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        row: &[usize],
        root: usize,
        value: usize,
        span: Span,
        admit: &mut Admission,
        check: bool,
    ) -> Result<()> {
        self.valid(plan, index, row, value, span, admit, check)?;
        if !check {
            return Ok(());
        }
        let scope = row[self.roots.len() + root];
        for b in self.collect(row, value, admit, 0)? {
            if let Origin::Arena(_) = b.origin {
                let allowed = scope != usize::MAX
                    && matches!(&self.values[scope],Data::Brands(v) if v.contains(&b));
                if !allowed {
                    return Err(self.error(
                        plan,
                        index,
                        ErrorKind::Escape { incoming: b },
                        span,
                        Some(b),
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn consume(&mut self, plan: &Plan<'_>, row: &mut [usize], value: m::Register) {
        let root = self.value_root(value);
        if !plan.nodes[self.roots[root].node.0].properties.is_copy {
            row[root] = 0;
        }
    }
    fn locations(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        row: &[usize],
        id: places::PlaceId,
        admit: &mut Admission,
        check: bool,
    ) -> Result<Vec<usize>> {
        let path = &plan.functions[index].paths[id.0 as usize];
        let root = self.slot(path.root.slot).unwrap();
        let p = self.place(root, &[], admit)?;
        let mut out = admit.copy(&[p])?;
        for step in &path.steps {
            let mut next = Vec::new();
            for p in out {
                match step.kind {
                    places::ProjectionKind::Dereference => {
                        let value = self.read_place(plan, row, p, admit)?;
                        for p in self.refs(value, admit, check)? {
                            admit.push(&mut next, p)?;
                        }
                    }
                    places::ProjectionKind::Field(f) => {
                        let p = self.append(
                            p,
                            Projection {
                                kind: ProjectionKind::Field(f),
                                node: step.to,
                            },
                            admit,
                        )?;
                        admit.push(&mut next, p)?;
                    }
                }
            }
            out = next;
        }
        Ok(out)
    }
    pub(super) fn read_locations(
        &mut self,
        plan: &Plan<'_>,
        row: &[usize],
        places: &[usize],
        node: NodeId,
        admit: &mut Admission,
    ) -> Result<usize> {
        let mut out = 0;
        for p in places {
            let value = self.read_place(plan, row, *p, admit)?;
            out = self.join(plan, out, value, node, admit, 0)?;
        }
        Ok(out)
    }
    fn update(
        &mut self,
        plan: &Plan<'_>,
        row: &[usize],
        old: usize,
        node: NodeId,
        path: &[Projection],
        value: usize,
        admit: &mut Admission,
        depth: usize,
    ) -> Result<usize> {
        admit.depth(depth)?;
        let Some((step, rest)) = path.split_first() else {
            return Ok(value);
        };
        if step.kind == ProjectionKind::Element {
            let child = self.project(plan, old, *step, admit)?;
            let incoming =
                self.update(plan, row, child, step.node, rest, value, admit, depth + 1)?;
            let mut brands = self.collect(row, old, admit, depth + 1)?;
            let more = self.collect(row, incoming, admit, depth + 1)?;
            for b in more {
                admit.push(&mut brands, b)?;
            }
            return self.brands(brands, admit);
        }
        let record = matches!(plan.nodes[node.0].kind, Kind::Record(_));
        let children = Self::child_nodes(plan, node, admit)?;
        let mut cases = admit.budget.vector(children.len())?;
        for (case, nodes) in children.iter().enumerate() {
            let mut values = admit.budget.vector(nodes.len())?;
            for (field, node) in nodes.iter().copied().enumerate() {
                let p = Projection {
                    kind: if record {
                        ProjectionKind::Field(field as u32)
                    } else {
                        ProjectionKind::Case(case as u32, field as u32)
                    },
                    node,
                };
                let v = self.project(plan, old, p, admit)?;
                values.push(if p.kind == step.kind {
                    self.update(plan, row, v, node, rest, value, admit, depth + 1)?
                } else {
                    v
                });
            }
            cases.push(values);
        }
        if record {
            self.intern(Data::Record(cases.pop().unwrap()), admit)
        } else {
            self.intern(Data::Sum(cases), admit)
        }
    }
    pub(super) fn store(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        row: &mut [usize],
        places: &[usize],
        value: usize,
        span: Span,
        admit: &mut Admission,
        check: bool,
    ) -> Result<()> {
        for place in places {
            let (root, path) = self.place_parts(*place, admit)?;
            let foreign = self.record && matches!(self.roots[root].kind, RootKind::Caller { .. });
            // Foreign storage always outlives this frame's local arenas. This
            // check is independent of incomplete recursive output summaries.
            self.storage(plan, index, row, root, value, span, admit, check || foreign)?;
            if foreign {
                let brands = self.collect(row, value, admit, 0)?;
                self.record_store(root, brands, span, admit)?;
                self.wrote[root] = true;
            }
            let node = self.roots[root].node;
            let new = self.update(plan, row, row[root], node, &path, value, admit, 0)?;
            row[root] = if places.len() > 1 {
                self.join(plan, row[root], new, node, admit, 0)?
            } else {
                new
            };
        }
        Ok(())
    }
    pub(super) fn result(
        &mut self,
        row: &mut [usize],
        root: usize,
        value: usize,
        admit: &mut Admission,
    ) -> Result<()> {
        let active = row.len() - self.local_count();
        let scope = self.scope(row, active, admit)?;
        row[root] = value;
        row[self.roots.len() + root] = scope;
        Ok(())
    }
    fn case(
        &mut self,
        plan: &Plan<'_>,
        node: NodeId,
        case: u32,
        fields: Vec<usize>,
        admit: &mut Admission,
    ) -> Result<usize> {
        let children = Self::child_nodes(plan, node, admit)?;
        let mut cases = admit.budget.vector(children.len())?;
        let mut fields = Some(fields);
        for (i, child) in children.iter().enumerate() {
            admit.budget.charge(1)?;
            cases.push(if i == case as usize {
                fields.take().unwrap()
            } else {
                flow::filled(&mut admit.budget, child.len(), 0)?
            });
        }
        self.intern(Data::Sum(cases), admit)
    }
    fn payload_place(
        &mut self,
        plan: &Plan<'_>,
        row: &[usize],
        value: m::Register,
        case: u32,
        field: u32,
        payload_node: NodeId,
        admit: &mut Admission,
        check: bool,
    ) -> Result<Vec<usize>> {
        let root = self.value_root(value);
        let mut node = self.roots[root].node;
        let mut targets = if let Kind::Loan { target, .. } = plan.nodes[node.0].kind {
            node = target;
            self.refs(row[root], admit, check)?
        } else {
            let p = self.place(root, &[], admit)?;
            admit.copy(&[p])?
        };
        let node = plan.nodes[node.0]
            .case(case)
            .and_then(|v| v.get(field as usize))
            .copied()
            .or_else(|| matches!(plan.nodes[node.0].kind, Kind::Builtin(_)).then_some(payload_node))
            .ok_or_else(|| admit.error(ErrorKind::InvalidShape))?;
        for p in &mut targets {
            *p = self.append(
                *p,
                Projection {
                    kind: ProjectionKind::Case(case, field),
                    node,
                },
                admit,
            )?;
        }
        Ok(targets)
    }
    fn arena_owner(
        &self,
        plan: &Plan<'_>,
        index: usize,
        site: bodies::Site,
        admit: &mut Admission,
    ) -> Result<declarations::LocalId> {
        for fact in &plan.functions[index].arenas {
            admit.budget.charge(1)?;
            if fact.site == site {
                return Ok(declarations::LocalId(fact.owner.local));
            }
        }
        Err(admit.error(ErrorKind::InvalidShape))
    }
    fn core(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        row: &mut [usize],
        i: &bodies::Instruction,
        call: &core::Call,
        site: bodies::Site,
        admit: &mut Admission,
        check: bool,
    ) -> Result<usize> {
        use core::Operation as O;
        let output = self.value_root(i.results[0]);
        let node = self.roots[output].node;
        if let Some(extract) = call.operation.extraction() {
            let source = self.value_root(call.arguments[0]);
            let source_node = self.roots[source].node;
            let child = plan.nodes[source_node.0].case(extract.case).unwrap()[0];
            return self.project(
                plan,
                row[source],
                Projection {
                    kind: ProjectionKind::Case(extract.case, 0),
                    node: child,
                },
                admit,
            );
        }
        match call.operation {
            O::ArenaNew | O::ArenaRead | O::ArenaReplace => {
                let owner = self.arena_owner(plan, index, site, admit)?;
                let active = row.len() - self.local_count();
                if check && row[active + owner.0 as usize] != 1 {
                    return Err(self.error(
                        plan,
                        index,
                        ErrorKind::ArenaState(owner),
                        i.span,
                        None,
                    ));
                }
                for v in &call.arguments[1..] {
                    for b in self.collect(row, row[self.value_root(*v)], admit, 0)? {
                        if check && (b.expired || b.origin != Origin::Arena(owner)) {
                            return Err(self.error(
                                plan,
                                index,
                                ErrorKind::CrossArena { owner, incoming: b },
                                i.span,
                                Some(b),
                            ));
                        }
                    }
                }
                let brands = admit.copy(&[Brand {
                    origin: Origin::Arena(owner),
                    expired: false,
                }])?;
                let value = self.brands(brands, admit)?;
                match call.operation {
                    O::ArenaNew => {
                        let v = admit.copy(&[value])?;
                        self.case(plan, node, 0, v, admit)
                    }
                    O::ArenaRead => {
                        let root = self.synthetic(site);
                        let value = if plan.nodes[self.roots[root].node.0]
                            .properties
                            .contains_arena
                        {
                            value
                        } else {
                            0
                        };
                        self.result(row, root, value, admit)?;
                        let p = self.place(root, &[], admit)?;
                        let v = admit.copy(&[p])?;
                        let reference = self.reference(v, admit)?;
                        let v = admit.copy(&[reference])?;
                        self.case(plan, node, 0, v, admit)
                    }
                    _ => Ok(0),
                }
            }
            O::VecPush | O::MapInsert => {
                let targets = self.refs(row[self.value_root(call.arguments[0])], admit, check)?;
                let last = self.value_root(*call.arguments.last().unwrap());
                let incoming = self.collect(row, row[last], admit, 0)?;
                let mut previous = Vec::new();
                for p in targets {
                    let old = self.read_place(plan, row, p, admit)?;
                    let mut brands = self.collect(row, old, admit, 0)?;
                    for b in &brands {
                        admit.push(&mut previous, *b)?;
                    }
                    for b in &incoming {
                        admit.push(&mut brands, *b)?;
                    }
                    let value = self.brands(brands, admit)?;
                    self.store(plan, index, row, &[p], value, i.span, admit, check)?;
                }
                if call.operation == O::MapInsert {
                    let payload = plan.nodes[node.0].case(0).unwrap()[0];
                    let element = plan.nodes[payload.0].case(1).unwrap()[0];
                    let value = if plan.nodes[element.0].properties.contains_arena {
                        self.brands(previous, admit)?
                    } else {
                        0
                    };
                    let v = admit.copy(&[value])?;
                    let value = self.case(plan, payload, 1, v, admit)?;
                    let v = admit.copy(&[value])?;
                    self.case(plan, node, 0, v, admit)
                } else {
                    Ok(0)
                }
            }
            O::VecGet | O::VecGetMut | O::MapGet => {
                let mut targets =
                    self.refs(row[self.value_root(call.arguments[0])], admit, check)?;
                let element = plan
                    .node_for(call.type_arguments.last().unwrap().ty)
                    .unwrap();
                for p in &mut targets {
                    *p = self.append(
                        *p,
                        Projection {
                            kind: ProjectionKind::Element,
                            node: element,
                        },
                        admit,
                    )?;
                }
                let reference = self.reference(targets, admit)?;
                let v = admit.copy(&[reference])?;
                self.case(
                    plan,
                    node,
                    if call.operation == O::MapGet { 1 } else { 0 },
                    v,
                    admit,
                )
            }
            O::VecPop | O::MapRemove => {
                let brands =
                    self.collect(row, row[self.value_root(call.arguments[0])], admit, 0)?;
                let element = plan.nodes[node.0].case(1).unwrap()[0];
                let value = if plan.nodes[element.0].properties.contains_arena {
                    self.brands(brands, admit)?
                } else {
                    0
                };
                let v = admit.copy(&[value])?;
                self.case(plan, node, 1, v, admit)
            }
            O::SharedNew | O::SharedReplace => {
                if check
                    && let Some(b) = self
                        .collect(
                            row,
                            row[self.value_root(*call.arguments.last().unwrap())],
                            admit,
                            0,
                        )?
                        .first()
                        .copied()
                {
                    return Err(self.error(
                        plan,
                        index,
                        ErrorKind::SharedStorage { incoming: b },
                        i.span,
                        Some(b),
                    ));
                }
                Ok(0)
            }
            O::SharedRead | O::Allocator => {
                let root = self.synthetic(site);
                self.result(row, root, 0, admit)?;
                let p = self.place(root, &[], admit)?;
                let v = admit.copy(&[p])?;
                let reference = self.reference(v, admit)?;
                if call.operation == O::Allocator {
                    Ok(reference)
                } else {
                    let v = admit.copy(&[reference])?;
                    self.case(plan, node, 0, v, admit)
                }
            }
            _ => Ok(0),
        }
    }
    fn operation(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        row: &mut [usize],
        i: &bodies::Instruction,
        site: bodies::Site,
        admit: &mut Admission,
        check: bool,
        summaries: Option<&[calls::Summary]>,
    ) -> Result<bool> {
        use bodies::Op;
        admit.budget.charge(1)?;
        // Destruction does not dereference a token. Cleanup may deliberately
        // retire storage after revoking an arena, including failure paths.
        if let Op::Cleanup(c) = &i.op {
            for step in &c.steps {
                self.operation(
                    plan,
                    index,
                    row,
                    &step.instruction(),
                    site,
                    admit,
                    check,
                    summaries,
                )?;
            }
            return Ok(true);
        }
        if !matches!(i.op, Op::DropValue(_) | Op::DropValueIfAvailable(_)) {
            for v in i.op.operands() {
                self.valid(
                    plan,
                    index,
                    row,
                    row[self.value_root(v)],
                    i.span,
                    admit,
                    check,
                )?;
            }
        }
        let output = i.results.first().map(|v| self.value_root(*v));
        if let (
            Some(summaries),
            Op::Call {
                function,
                arguments,
            },
        ) = (summaries, &i.op)
        {
            let returns = self.apply_call(
                plan,
                index,
                row,
                *function,
                arguments,
                &summaries[function.0 as usize],
                i.span,
                admit,
                check,
            )?;
            for argument in arguments {
                self.consume(plan, row, *argument);
            }
            if returns && let Some(output) = output {
                self.result(row, output, 0, admit)?;
            }
            return Ok(returns);
        }
        if summaries.is_some()
            && let Op::Import { import, arguments } = &i.op
        {
            for argument in arguments {
                if plan.nodes[self.roots[self.value_root(*argument)].node.0]
                    .properties
                    .contains_arena
                {
                    return Err(self.error(
                        plan,
                        index,
                        ErrorKind::ImportEffectsRequired(*import),
                        i.span,
                        None,
                    ));
                }
            }
        }
        let mut result = 0;
        match &i.op {
            Op::Copy(v) | Op::Move(v) => {
                result = row[self.value_root(*v)];
                if matches!(i.op, Op::Move(_)) {
                    self.consume(plan, row, *v);
                }
            }
            Op::CopyPlace(p) | Op::MovePlace(p) => {
                let targets = self.locations(plan, index, row, *p, admit, check)?;
                let node = self.roots[output.unwrap()].node;
                result = self.read_locations(plan, row, &targets, node, admit)?;
                self.valid(plan, index, row, result, i.span, admit, check)?;
                if matches!(i.op, Op::MovePlace(_)) && !plan.nodes[node.0].properties.is_copy {
                    self.store(plan, index, row, &targets, 0, i.span, admit, false)?;
                }
            }
            Op::Initialize { local, value } => {
                let root = local.0 as usize;
                let scope = self.roots.len() + root;
                if row[scope] == usize::MAX {
                    row[scope] = self.scope(row, row.len() - self.local_count(), admit)?;
                }
                self.storage(
                    plan,
                    index,
                    row,
                    root,
                    row[self.value_root(*value)],
                    i.span,
                    admit,
                    check,
                )?;
                row[root] = row[self.value_root(*value)];
                self.consume(plan, row, *value);
            }
            Op::Replace { place, value } => {
                let targets = self.locations(plan, index, row, *place, admit, check)?;
                self.store(
                    plan,
                    index,
                    row,
                    &targets,
                    row[self.value_root(*value)],
                    i.span,
                    admit,
                    check,
                )?;
                self.consume(plan, row, *value);
            }
            Op::PreparePlace { destination, place } => {
                let p = self.locations(plan, index, row, *place, admit, check)?;
                row[self.roots.len() * 2 + destination.0 as usize] = self.reference(p, admit)?;
            }
            Op::ReplacePrepared { destination, value } => {
                let capture = self.roots.len() * 2 + destination.0 as usize;
                let targets = self.refs(row[capture], admit, check)?;
                self.store(
                    plan,
                    index,
                    row,
                    &targets,
                    row[self.value_root(*value)],
                    i.span,
                    admit,
                    check,
                )?;
                row[capture] = 0;
                self.consume(plan, row, *value);
            }
            Op::DiscardPrepared(p) => row[self.roots.len() * 2 + p.0 as usize] = 0,
            Op::Borrow { place, .. } => {
                let targets = self.locations(plan, index, row, *place, admit, check)?;
                result = self.reference(targets, admit)?;
            }
            Op::BorrowCaseField {
                value, case, field, ..
            } => {
                let Kind::Loan { target, .. } = plan.nodes[self.roots[output.unwrap()].node.0].kind
                else {
                    unreachable!()
                };
                let p =
                    self.payload_place(plan, row, *value, *case, *field, target, admit, check)?;
                result = self.reference(p, admit)?;
            }
            Op::CopyCaseField {
                value, case, field, ..
            } => {
                let p = self.payload_place(
                    plan,
                    row,
                    *value,
                    *case,
                    *field,
                    self.roots[output.unwrap()].node,
                    admit,
                    check,
                )?;
                result =
                    self.read_locations(plan, row, &p, self.roots[output.unwrap()].node, admit)?;
                self.valid(plan, index, row, result, i.span, admit, check)?;
            }
            Op::Record { fields, .. } => {
                let mut v = admit.budget.vector(fields.len())?;
                for f in fields {
                    admit.budget.charge(1)?;
                    v.push(row[self.value_root(*f)]);
                }
                result = self.intern(Data::Record(v), admit)?;
                for f in fields {
                    self.consume(plan, row, *f);
                }
            }
            Op::Variant { case, payload } => {
                let mut v = admit.budget.vector(payload.len())?;
                for p in payload {
                    admit.budget.charge(1)?;
                    v.push(row[self.value_root(*p)]);
                }
                result = self.case(plan, self.roots[output.unwrap()].node, *case, v, admit)?;
                for p in payload {
                    self.consume(plan, row, *p);
                }
            }
            Op::UnpackVariant { value, case, .. } => {
                let mut snapshot = admit.budget.vector(i.results.len())?;
                for (field, out) in i.results.iter().enumerate() {
                    let node = self.roots[self.value_root(*out)].node;
                    let value = self.project(
                        plan,
                        row[self.value_root(*value)],
                        Projection {
                            kind: ProjectionKind::Case(*case, field as u32),
                            node,
                        },
                        admit,
                    )?;
                    snapshot.push(value);
                }
                self.consume(plan, row, *value);
                for (out, value) in i.results.iter().zip(snapshot) {
                    self.result(row, self.value_root(*out), value, admit)?;
                }
                return Ok(true);
            }
            Op::CoreCall(call) => {
                result = self.core(plan, index, row, i, call, site, admit, check)?;
                for a in &call.arguments {
                    self.consume(plan, row, *a);
                }
            }
            Op::Call { arguments, .. } | Op::Import { arguments, .. } => {
                for v in arguments {
                    self.consume(plan, row, *v);
                }
            }
            Op::ArenaEnter { owner, .. } => {
                let active = row.len() - self.local_count();
                if check && row[active + owner.0 as usize] != 0 {
                    return Err(self.error(
                        plan,
                        index,
                        ErrorKind::ArenaState(*owner),
                        i.span,
                        None,
                    ));
                }
                row[active + owner.0 as usize] = 1;
            }
            Op::ArenaExit { owner } => {
                let active = row.len() - self.local_count();
                if check && row[active + owner.0 as usize] != 1 {
                    return Err(self.error(
                        plan,
                        index,
                        ErrorKind::ArenaState(*owner),
                        i.span,
                        None,
                    ));
                }
                row[active + owner.0 as usize] = 0;
                // Storage belongs to this activation too. A fresh enter must
                // not make an old local's storage scope accept new tokens.
                for value in row.iter_mut().take(self.roots.len() * 2) {
                    if *value != usize::MAX {
                        *value = self.expire(*value, *owner, admit, 0)?;
                    }
                }
                row[owner.0 as usize] = 0;
                row[self.roots.len() + owner.0 as usize] = usize::MAX;
            }
            Op::DropLocal(l) | Op::DropLocalIfInitialized(l) => {
                row[l.0 as usize] = 0;
                row[self.roots.len() + l.0 as usize] = usize::MAX;
            }
            Op::DropValue(v) | Op::DropValueIfAvailable(v) => self.consume(plan, row, *v),
            Op::EndBorrow { .. }
            | Op::I64(_)
            | Op::Bool(_)
            | Op::Unit
            | Op::Add(..)
            | Op::LessThan(..)
            | Op::EqualI64(..)
            | Op::IsCase { .. } => {}
            Op::Cleanup(_) => unreachable!(),
        }
        if let Some(out) = output {
            self.result(row, out, result, admit)?;
        }
        Ok(true)
    }
    pub fn block(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        row: &mut [usize],
        block_id: usize,
        block: &bodies::Block,
        admit: &mut Admission,
        check: bool,
        summaries: Option<&[calls::Summary]>,
    ) -> Result<bool> {
        for (order, i) in block.instructions.iter().enumerate() {
            let returns = self
                .operation(
                    plan,
                    index,
                    row,
                    i,
                    bodies::Site {
                        block: m::BlockId(block_id as u32),
                        order: order + 1,
                    },
                    admit,
                    check,
                    summaries,
                )
                .map_err(|mut e| {
                    if e.primary.is_none() {
                        e.primary = Some(i.span);
                    }
                    e
                })?;
            if !returns {
                return Ok(false);
            }
        }
        if check {
            for v in block.terminator.kind.operands() {
                let value = row[self.value_root(v)];
                if matches!(block.terminator.kind, bodies::Term::Return(_))
                    && let Some(b) = self.collect(row, value, admit, 0)?.first().copied()
                {
                    return Err(self.error(
                        plan,
                        index,
                        ErrorKind::ReturnEscape { incoming: b },
                        block.terminator.span,
                        Some(b),
                    ));
                }
                self.valid(plan, index, row, value, block.terminator.span, admit, true)?;
            }
        }
        if self.record && matches!(block.terminator.kind, bodies::Term::Return(_)) {
            self.capture_outputs(plan, row, admit)?;
        }
        Ok(true)
    }
    pub fn edge(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        row: &mut [usize],
        target: &m::Target,
        admit: &mut Admission,
    ) -> Result<()> {
        let parameters = &plan.source.activity().moved().typed().functions()[index].blocks
            [target.block.0 as usize]
            .parameters;
        let mut snapshot = admit.budget.vector(target.arguments.len())?;
        for v in &target.arguments {
            admit.budget.charge(1)?;
            snapshot.push(row[self.value_root(*v)]);
        }
        for v in &target.arguments {
            self.consume(plan, row, *v);
        }
        for (v, value) in parameters.iter().zip(snapshot) {
            self.result(row, self.value_root(*v), value, admit)?;
        }
        Ok(())
    }
}
