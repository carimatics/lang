use super::*;
use crate::unit::ownership::core;
use crate::unit::ownership::{Constructor, DefinitionKind, places as p};
use state::{has, ids, set};
use topology::{Topology, constructor};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Mode {
    Propagate,
    Check,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Access {
    Read,
    Write,
}
struct Place {
    targets: Vec<usize>,
    via: Vec<usize>,
}
struct Value {
    refs: Vec<u64>,
    dead: Vec<bool>,
}
pub(super) struct Context<'a, 'b> {
    pub typed: &'a b::TypedBodies,
    pub function: usize,
    pub topology: &'a Topology,
    pub layout: &'a state::Layout,
    pub budget: &'b mut Budget,
}
impl Context<'_, '_> {
    fn unique(&mut self, values: &mut Vec<usize>, id: usize) -> Result<(), Error> {
        self.budget.charge(values.len() + 1)?;
        if !values.contains(&id) {
            push(self.budget, values, id)?;
        }
        Ok(())
    }
    fn fail(&self, kind: ErrorKind, span: Span, id: Option<usize>) -> Error {
        Error {
            kind,
            primary: Some(span),
            secondary: id.and_then(|id| self.topology.origins[id]),
        }
    }
    fn empty(&mut self) -> Result<Place, Error> {
        Ok(Place {
            targets: self.budget.vector(0)?,
            via: self.budget.vector(0)?,
        })
    }
    fn direct(&mut self, node: usize) -> Result<Place, Error> {
        let mut place = self.empty()?;
        push(self.budget, &mut place.targets, node)?;
        Ok(place)
    }
    fn parents(&mut self, row: &[u64], id: usize, out: &mut Vec<usize>) -> Result<(), Error> {
        self.unique(out, id)?;
        for parent in ids(self.budget, row, self.layout.deps(id), self.layout.loans)? {
            self.unique(out, parent)?;
        }
        Ok(())
    }
    fn leaf_refs(
        &mut self,
        row: &[u64],
        leaf: usize,
        span: Span,
        mode: Mode,
    ) -> Result<Vec<usize>, Error> {
        let refs = ids(self.budget, row, self.layout.refs(leaf), self.layout.loans)?;
        if mode == Mode::Check {
            for id in &refs {
                self.budget.charge(1)?;
                if has(row, self.layout.dead.clone(), leaf) || !self.layout.active(row, *id) {
                    return Err(self.fail(
                        ErrorKind::EndedReference(ReferenceId(*id as u32)),
                        span,
                        Some(*id),
                    ));
                }
                for parent in ids(self.budget, row, self.layout.deps(*id), self.layout.loans)? {
                    if !self.layout.active(row, parent) {
                        return Err(self.fail(
                            ErrorKind::EndedReference(ReferenceId(parent as u32)),
                            span,
                            Some(parent),
                        ));
                    }
                }
            }
        }
        Ok(refs)
    }
    fn access(
        &mut self,
        row: &[u64],
        place: &Place,
        access: Access,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        if mode == Mode::Propagate {
            return Ok(());
        }
        for id in 0..self.layout.loans {
            self.budget.charge(place.via.len() + 1)?;
            if !self.layout.active(row, id) || place.via.contains(&id) {
                continue;
            }
            let shared_read = access == Access::Read && self.topology.kinds[id] == LoanKind::Shared;
            for parent in &place.via {
                self.budget.charge(1)?;
                if !shared_read && has(row, self.layout.deps(id), *parent) {
                    return Err(self.fail(
                        ErrorKind::FrozenParent {
                            parent: ReferenceId(*parent as u32),
                            child: ReferenceId(id as u32),
                        },
                        span,
                        Some(id),
                    ));
                }
            }
            if shared_read {
                continue;
            }
            for target in ids(self.budget, row, self.layout.targets(id), self.layout.nodes)? {
                for node in &place.targets {
                    self.budget.charge(1)?;
                    let owner = self.topology.cells[*node].owner;
                    if self.topology.cells[target].owner == owner {
                        return Err(self.fail(
                            ErrorKind::Conflict {
                                owner,
                                loan: ReferenceId(id as u32),
                            },
                            span,
                            Some(id),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    fn dereference(
        &mut self,
        row: &[u64],
        place: Place,
        span: Span,
        mode: Mode,
    ) -> Result<Place, Error> {
        self.access(row, &place, Access::Read, span, mode)?;
        let mut next = self.empty()?;
        for id in place.via {
            self.unique(&mut next.via, id)?;
        }
        for node in place.targets {
            for leaf in self.topology.cells[node].leaves.clone() {
                for id in self.leaf_refs(row, leaf, span, mode)? {
                    self.parents(row, id, &mut next.via)?;
                    for target in ids(self.budget, row, self.layout.targets(id), self.layout.nodes)?
                    {
                        self.unique(&mut next.targets, target)?;
                    }
                }
            }
        }
        if mode == Mode::Check && next.targets.is_empty() {
            return Err(self.fail(ErrorKind::MissingReference, span, None));
        }
        Ok(next)
    }
    fn place(
        &mut self,
        row: &[u64],
        id: p::PlaceId,
        span: Span,
        mode: Mode,
    ) -> Result<Place, Error> {
        let path = &self.typed.places().functions()[self.function].places[id.0 as usize];
        let (node, mut ty) = match path.root {
            p::Root::Local(l) => {
                let n = self.topology.local(l);
                (n, self.topology.cells[n].ty)
            }
            p::Root::Value(v) => {
                let n = self.topology.value(v);
                (n, self.topology.cells[n].ty)
            }
        };
        let mut place = self.direct(node)?;
        let types = self.typed.places().declarations().types();
        for projection in &path.projections {
            self.budget.charge(1)?;
            match projection.kind {
                p::ProjectionKind::Dereference => {
                    place = self.dereference(row, place, span, mode)?;
                    let Some(Constructor::Loan(_, target)) = constructor(types, ty) else {
                        unreachable!()
                    };
                    ty = target;
                }
                p::ProjectionKind::Field(field) => {
                    let Type::Nominal(id) = ty else {
                        unreachable!()
                    };
                    let DefinitionKind::Record(fields) =
                        &types.table().definitions[id.0 as usize].kind
                    else {
                        unreachable!()
                    };
                    ty = fields[field as usize].ty.ty;
                }
            }
        }
        Ok(place)
    }
    fn read_value(
        &mut self,
        row: &[u64],
        value: m::Register,
        transfer: bool,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        let node = self.topology.value(value);
        let direct = self.direct(node)?;
        let types = self.typed.places().declarations().types();
        let noncopy = !types
            .properties(self.topology.cells[node].ty)
            .unwrap()
            .is_copy;
        self.access(
            row,
            &direct,
            if transfer && noncopy {
                Access::Write
            } else {
                Access::Read
            },
            span,
            mode,
        )?;
        self.reference_value(row, node, transfer, span, mode)
    }
    fn reference_value(
        &mut self,
        row: &[u64],
        node: usize,
        transfer: bool,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        for leaf in self.topology.cells[node].leaves.clone() {
            let refs = self.leaf_refs(row, leaf, span, mode)?;
            if mode == Mode::Check
                && refs.is_empty()
                && matches!(
                    constructor(
                        self.typed.places().declarations().types(),
                        self.topology.cells[node].ty
                    ),
                    Some(Constructor::Loan(..))
                )
            {
                return Err(self.fail(ErrorKind::MissingReference, span, None));
            }
            for id in refs {
                let mut place = self.empty()?;
                place.targets = ids(self.budget, row, self.layout.targets(id), self.layout.nodes)?;
                self.parents(row, id, &mut place.via)?;
                let access = if transfer && self.topology.kinds[id] == LoanKind::Exclusive {
                    Access::Write
                } else {
                    Access::Read
                };
                self.access(row, &place, access, span, mode)?;
            }
        }
        Ok(())
    }
    fn snapshot(&mut self, row: &[u64], node: usize) -> Result<Value, Error> {
        let cell = &self.topology.cells[node];
        let mut refs = self
            .budget
            .vector(mul(cell.leaves.len(), self.layout.loan_words)?)?;
        let mut dead = self.budget.vector(cell.leaves.len())?;
        for leaf in cell.leaves.clone() {
            self.budget.charge(self.layout.loan_words + 1)?;
            refs.extend_from_slice(&row[self.layout.refs(leaf)]);
            dead.push(has(row, self.layout.dead.clone(), leaf));
        }
        Ok(Value { refs, dead })
    }
    fn clear(&mut self, row: &mut [u64], node: usize) -> Result<(), Error> {
        for leaf in self.topology.cells[node].leaves.clone() {
            self.budget.charge(self.layout.loan_words + 1)?;
            row[self.layout.refs(leaf)].fill(0);
            set(row, self.layout.dead.clone(), leaf, false);
        }
        Ok(())
    }
    fn write(
        &mut self,
        row: &mut [u64],
        node: usize,
        value: &Value,
        weak: bool,
    ) -> Result<(), Error> {
        let cell = &self.topology.cells[node];
        debug_assert_eq!(cell.leaves.len(), value.dead.len());
        for (i, leaf) in cell.leaves.clone().enumerate() {
            for (j, word) in row[self.layout.refs(leaf)].iter_mut().enumerate() {
                self.budget.charge(1)?;
                let incoming = value.refs[i * self.layout.loan_words + j];
                if weak {
                    *word |= incoming;
                } else {
                    *word = incoming;
                }
            }
            self.budget.charge(1)?;
            let dead = value.dead[i] || (weak && has(row, self.layout.dead.clone(), leaf));
            set(row, self.layout.dead.clone(), leaf, dead);
        }
        Ok(())
    }
    fn consume(&mut self, row: &mut [u64], value: m::Register) -> Result<(), Error> {
        let node = self.topology.value(value);
        if !self
            .typed
            .places()
            .declarations()
            .types()
            .properties(self.topology.cells[node].ty)
            .unwrap()
            .is_copy
        {
            self.clear(row, node)?;
        }
        Ok(())
    }
    fn reserve_argument(
        &mut self,
        row: &[u64],
        value: m::Register,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        let node = self.topology.value(value);
        let mut ty = self.topology.cells[node].ty;
        let mut place = self.direct(node)?;
        let mut writable = true;
        let mut depth = 0usize;
        while let Some(Constructor::Loan(kind, target)) =
            constructor(self.typed.places().declarations().types(), ty)
        {
            self.budget.charge(1)?;
            depth += 1;
            if depth > self.budget.limits().depth.min(256) {
                return Err(m::Error::new(m::ErrorKind::Limit {
                    kind: m::LimitKind::Depth,
                    requested: depth,
                    limit: self.budget.limits().depth.min(256),
                })
                .into());
            }
            writable &= kind == LoanKind::Exclusive;
            place = self.dereference(row, place, span, mode)?;
            self.access(
                row,
                &place,
                if writable {
                    Access::Write
                } else {
                    Access::Read
                },
                span,
                mode,
            )?;
            ty = target;
        }
        Ok(())
    }
    fn begin(
        &mut self,
        row: &mut [u64],
        id: b::LoanId,
        result: usize,
        place: Place,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        self.access(
            row,
            &place,
            if self.topology.kinds[id.0 as usize] == LoanKind::Shared {
                Access::Read
            } else {
                Access::Write
            },
            span,
            mode,
        )?;
        if mode == Mode::Check && place.targets.is_empty() {
            return Err(self.fail(ErrorKind::MissingReference, span, None));
        }
        let id = id.0 as usize;
        self.layout.activity(row, id, true);
        row[self.layout.targets(id)].fill(0);
        row[self.layout.deps(id)].fill(0);
        self.budget
            .charge(self.layout.node_words + self.layout.loan_words)?;
        for target in place.targets {
            self.budget.charge(1)?;
            set(row, self.layout.targets(id), target, true);
        }
        for parent in place.via {
            self.budget.charge(1)?;
            set(row, self.layout.deps(id), parent, true);
        }
        let node = result;
        self.clear(row, node)?;
        set(
            row,
            self.layout.refs(self.topology.cells[node].leaves.start),
            id,
            true,
        );
        Ok(())
    }
    fn end(&mut self, row: &mut [u64], id: usize, span: Span, mode: Mode) -> Result<(), Error> {
        if !self.layout.active(row, id) {
            return Ok(());
        }
        let mut invalid = self.budget.vector(self.layout.loans)?;
        for child in 0..self.layout.loans {
            self.budget.charge(1)?;
            if child == id || has(row, self.layout.deps(child), id) {
                if mode == Mode::Check && child != id && self.layout.active(row, child) {
                    return Err(self.fail(
                        ErrorKind::EndParent {
                            parent: ReferenceId(id as u32),
                            child: ReferenceId(child as u32),
                        },
                        span,
                        Some(child),
                    ));
                }
                invalid.push(child);
            }
        }
        for leaf in 0..self.topology.leaves {
            for child in &invalid {
                self.budget.charge(1)?;
                if self.layout.has_ref(row, leaf, *child) {
                    set(row, self.layout.dead.clone(), leaf, true);
                }
            }
        }
        for capture in 0..self.typed.functions()[self.function].prepared {
            self.budget.charge(1)?;
            if has(row, self.layout.capture_deps(capture), id) {
                set(row, self.layout.capture_dead.clone(), capture, true);
            }
        }
        self.layout.activity(row, id, false);
        self.budget
            .charge(self.layout.node_words + self.layout.loan_words)?;
        row[self.layout.targets(id)].fill(0);
        row[self.layout.deps(id)].fill(0);
        Ok(())
    }
    fn replacement(
        &mut self,
        row: &mut [u64],
        place: Place,
        value: m::Register,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        self.read_value(row, value, true, span, mode)?;
        self.access(row, &place, Access::Write, span, mode)?;
        let incoming = self.snapshot(row, self.topology.value(value))?;
        let weak = place.targets.len() > 1;
        for target in place.targets {
            if mode == Mode::Check
                && !incoming.dead.is_empty()
                && self.topology.cells[target].foreign
            {
                return Err(self.fail(
                    ErrorKind::CallerCellEscape(self.topology.cells[target].owner),
                    span,
                    None,
                ));
            }
            self.write(row, target, &incoming, weak)?;
        }
        self.consume(row, value)
    }
    fn payload(
        &mut self,
        row: &[u64],
        value: m::Register,
        case: u32,
        span: Span,
        mode: Mode,
    ) -> Result<Place, Error> {
        let node = self.topology.value(value);
        let mut place = self.direct(node)?;
        if matches!(
            constructor(
                self.typed.places().declarations().types(),
                self.topology.cells[node].ty
            ),
            Some(Constructor::Loan(..))
        ) {
            place = self.dereference(row, place, span, mode)?;
        }
        for node in &mut place.targets {
            if let Some(child) = self.topology.cells[*node]
                .children
                .get(case as usize)
                .copied()
                .flatten()
            {
                *node = child;
            }
        }
        Ok(place)
    }
    fn operation(&mut self, row: &mut [u64], i: &b::Instruction, mode: Mode) -> Result<(), Error> {
        use b::Op;
        self.budget.charge(1)?;
        let s = i.span;
        // A repeated SSA definition is also a storage write. References into
        // that storage must end before the next dynamic definition.
        for result in &i.results {
            let destination = self.direct(self.topology.value(*result))?;
            self.access(row, &destination, Access::Write, s, mode)?;
        }
        match &i.op {
            Op::Cleanup(cleanup) => {
                for loans in [true, false] {
                    for step in &cleanup.steps {
                        self.budget.charge(1)?;
                        if matches!(step.target, b::CleanupTarget::Loan(_)) == loans {
                            self.operation(row, &step.instruction(), mode)?;
                        }
                    }
                }
            }
            Op::Borrow { loan, place, .. } => {
                let place = self.place(row, *place, s, mode)?;
                self.begin(
                    row,
                    *loan,
                    self.topology.value(i.results[0]),
                    place,
                    s,
                    mode,
                )?;
            }
            Op::BorrowCaseField {
                loan, value, case, ..
            } => {
                let place = self.payload(row, *value, *case, s, mode)?;
                self.begin(
                    row,
                    *loan,
                    self.topology.value(i.results[0]),
                    place,
                    s,
                    mode,
                )?;
            }
            Op::EndBorrow { loan, .. } => self.end(row, loan.0 as usize, s, mode)?,
            Op::CoreCall(call) => {
                for value in &call.arguments {
                    self.read_value(row, *value, true, s, mode)?;
                    self.reserve_argument(row, *value, s, mode)?;
                }
                let output = self.topology.value(i.results[0]);
                self.clear(row, output)?;
                match call.operation.signature().result_binding {
                    core::ResultBinding::InstanceAllocator => {
                        let place = self.direct(self.topology.instance_allocator.unwrap())?;
                        self.begin(row, call.result_loan.unwrap(), output, place, s, mode)?;
                    }
                    core::ResultBinding::ProvisionalLoan {
                        parameter, success, ..
                    } => {
                        let place = self.direct(self.topology.value(call.arguments[parameter]))?;
                        let place = self.dereference(row, place, s, mode)?;
                        let result =
                            self.topology.cells[output].children[success as usize].unwrap();
                        self.begin(row, call.result_loan.unwrap(), result, place, s, mode)?;
                    }
                    _ => {}
                }
                for value in &call.arguments {
                    self.consume(row, *value)?;
                }
            }
            Op::Copy(v) | Op::Move(v) => {
                let transfer = matches!(i.op, Op::Move(_));
                self.read_value(row, *v, transfer, s, mode)?;
                let data = self.snapshot(row, self.topology.value(*v))?;
                self.write(row, self.topology.value(i.results[0]), &data, false)?;
                if transfer {
                    self.consume(row, *v)?;
                }
            }
            Op::CopyPlace(id) | Op::MovePlace(id) => {
                let place = self.place(row, *id, s, mode)?;
                let ty = self
                    .typed
                    .places()
                    .facts(m::FunctionId(self.function as u32), *id)
                    .unwrap()
                    .value_type;
                let transfer = matches!(i.op, Op::MovePlace(_))
                    && !self
                        .typed
                        .places()
                        .declarations()
                        .types()
                        .properties(ty)
                        .unwrap()
                        .is_copy;
                self.access(
                    row,
                    &place,
                    if transfer {
                        Access::Write
                    } else {
                        Access::Read
                    },
                    s,
                    mode,
                )?;
                let result = self.topology.value(i.results[0]);
                self.clear(row, result)?;
                for node in place.targets {
                    let data = self.snapshot(row, node)?;
                    self.reference_value(row, node, transfer, s, mode)?;
                    self.write(row, result, &data, true)?;
                    if transfer
                        && !self
                            .typed
                            .places()
                            .declarations()
                            .types()
                            .properties(self.topology.cells[node].ty)
                            .unwrap()
                            .is_copy
                    {
                        self.clear(row, node)?;
                    }
                }
            }
            Op::Initialize { local, value } => {
                self.read_value(row, *value, true, s, mode)?;
                let node = self.topology.local(*local);
                let place = self.direct(node)?;
                self.access(row, &place, Access::Write, s, mode)?;
                let data = self.snapshot(row, self.topology.value(*value))?;
                self.write(row, node, &data, false)?;
                self.consume(row, *value)?;
            }
            Op::Replace { place, value } => {
                let place = self.place(row, *place, s, mode)?;
                self.replacement(row, place, *value, s, mode)?;
            }
            Op::PreparePlace { destination, place } => {
                let place = self.place(row, *place, s, mode)?;
                let id = destination.0 as usize;
                self.budget
                    .charge(self.layout.node_words + self.layout.loan_words + 1)?;
                row[self.layout.capture(id)].fill(0);
                row[self.layout.capture_deps(id)].fill(0);
                set(row, self.layout.capture_dead.clone(), id, false);
                for node in place.targets {
                    self.budget.charge(1)?;
                    set(row, self.layout.capture(id), node, true);
                }
                for parent in place.via {
                    self.budget.charge(1)?;
                    set(row, self.layout.capture_deps(id), parent, true);
                }
            }
            Op::ReplacePrepared { destination, value } => {
                let id = destination.0 as usize;
                if mode == Mode::Check && has(row, self.layout.capture_dead.clone(), id) {
                    return Err(self.fail(ErrorKind::InvalidPrepared(*destination), s, None));
                }
                let place = Place {
                    targets: ids(self.budget, row, self.layout.capture(id), self.layout.nodes)?,
                    via: ids(
                        self.budget,
                        row,
                        self.layout.capture_deps(id),
                        self.layout.loans,
                    )?,
                };
                self.replacement(row, place, *value, s, mode)?;
                self.clear_capture(row, id)?;
            }
            Op::DiscardPrepared(id) => self.clear_capture(row, id.0 as usize)?,
            Op::DropValue(v) | Op::DropValueIfAvailable(v) => {
                let node = self.topology.value(*v);
                let place = self.direct(node)?;
                let copy = self
                    .typed
                    .places()
                    .declarations()
                    .types()
                    .properties(self.topology.cells[node].ty)
                    .unwrap()
                    .is_copy;
                self.access(
                    row,
                    &place,
                    if copy { Access::Read } else { Access::Write },
                    s,
                    mode,
                )?;
                self.consume(row, *v)?;
            }
            Op::DropLocal(l) | Op::DropLocalIfInitialized(l) | Op::ArenaExit { owner: l } => {
                let node = self.topology.local(*l);
                let place = self.direct(node)?;
                self.access(row, &place, Access::Write, s, mode)?;
                self.clear(row, node)?;
            }
            Op::Call { arguments, .. }
            | Op::Import { arguments, .. }
            | Op::Record {
                fields: arguments, ..
            } => {
                for v in arguments {
                    self.read_value(row, *v, true, s, mode)?;
                    if matches!(i.op, Op::Call { .. }) {
                        self.reserve_argument(row, *v, s, mode)?;
                    }
                    self.consume(row, *v)?;
                }
                self.clear(row, self.topology.value(i.results[0]))?;
            }
            Op::Variant { case, payload } => {
                let node = self.topology.value(i.results[0]);
                self.clear(row, node)?;
                for value in payload {
                    self.read_value(row, *value, true, s, mode)?;
                    if let Some(child) = self.topology.cells[node]
                        .children
                        .get(*case as usize)
                        .copied()
                        .flatten()
                    {
                        let data = self.snapshot(row, self.topology.value(*value))?;
                        self.write(row, child, &data, false)?;
                    }
                    self.consume(row, *value)?;
                }
            }
            Op::UnpackVariant { value, case, .. } => {
                self.read_value(row, *value, true, s, mode)?;
                let node = self.topology.value(*value);
                for result in &i.results {
                    let result = self.topology.value(*result);
                    self.clear(row, result)?;
                    if let Some(child) = self.topology.cells[node]
                        .children
                        .get(*case as usize)
                        .copied()
                        .flatten()
                    {
                        let data = self.snapshot(row, child)?;
                        self.write(row, result, &data, false)?;
                    }
                }
                self.consume(row, *value)?;
            }
            Op::IsCase { value, .. } | Op::CopyCaseField { value, .. } => {
                self.read_value(row, *value, false, s, mode)?
            }
            _ => {
                for operand in i.op.operands() {
                    self.read_value(row, operand, false, s, mode)?;
                }
                for result in &i.results {
                    self.clear(row, self.topology.value(*result))?;
                }
            }
        }
        Ok(())
    }
    fn clear_capture(&mut self, row: &mut [u64], id: usize) -> Result<(), Error> {
        self.budget
            .charge(self.layout.node_words + self.layout.loan_words + 1)?;
        row[self.layout.capture(id)].fill(0);
        row[self.layout.capture_deps(id)].fill(0);
        set(row, self.layout.capture_dead.clone(), id, false);
        Ok(())
    }
    pub fn block(&mut self, row: &mut [u64], body: &b::Block, mode: Mode) -> Result<(), Error> {
        for i in &body.instructions {
            self.operation(row, i, mode)?;
        }
        match body.terminator.kind {
            b::Term::Return(value) => {
                self.read_value(row, value, true, body.terminator.span, mode)?
            }
            b::Term::Branch { condition, .. } => {
                self.read_value(row, condition, false, body.terminator.span, mode)?
            }
            b::Term::Jump(_) | b::Term::Unreachable => {}
        }
        Ok(())
    }
    pub fn edge(
        &mut self,
        row: &mut [u64],
        target: &m::Target,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        let mut data = self.budget.vector(target.arguments.len())?;
        for value in &target.arguments {
            self.read_value(row, *value, true, span, mode)?;
            data.push(self.snapshot(row, self.topology.value(*value))?);
        }
        for value in &target.arguments {
            self.consume(row, *value)?;
        }
        for (parameter, value) in self.typed.functions()[self.function].blocks
            [target.block.0 as usize]
            .parameters
            .iter()
            .zip(data)
        {
            let destination = self.direct(self.topology.value(*parameter))?;
            self.access(row, &destination, Access::Write, span, mode)?;
            self.write(row, self.topology.value(*parameter), &value, false)?;
        }
        Ok(())
    }
}
