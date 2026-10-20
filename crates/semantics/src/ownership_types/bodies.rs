//! Automatic lowering from region-checked source bodies, including generated
//! lexical/temporary cleanup ranges and record initialization order. Executable
//! layouts, guest allocation and execution adapters remain required.

use super::*;
use lang_hir::program::{BodyTypes, FunctionFacts};
use lang_mir::unit::ownership::{bodies as b, declarations as d, places as p};

mod matching;

fn push<T>(c: &mut Context<'_>, out: &mut Vec<T>, value: T) -> Result<()> {
    c.budget.charge(1)?;
    if out.len() == out.capacity() {
        let next = out
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Capacity)?;
        c.budget.allocate::<T>(next)?;
        out.try_reserve_exact(next - out.len())
            .map_err(|_| Error::Capacity)?;
    }
    out.push(value);
    Ok(())
}

#[derive(Default)]
struct Block {
    instructions: Vec<b::Instruction>,
    terminator: Option<b::Terminator>,
    span: Option<m::Span>,
}
#[derive(Clone, Copy)]
enum Action {
    Local(d::LocalId, Span),
    Value(m::Register, Span),
    Loan(b::LoanId, Span),
    Arena(d::LocalId, Span),
}
#[derive(Default)]
struct Counts {
    registers: usize,
    blocks: usize,
    instructions: usize,
    places: usize,
    loans: usize,
    prepared: usize,
}

/// Lower complete RFC 0007 source functions atomically, including match flow.
/// All construction and independent verification share one cumulative budget.
pub fn lower(
    checked: &RegionCheckedBodies,
    sources: &SourceMap,
    limits: b::Limits,
) -> Result<b::TypedBodies> {
    let input = checked.loaned().moved().bodies();
    let (mut c, mut table) = super::places::construct(checked, sources, limits.places)?;
    let mut counts = Counts::default();
    for (frame, declaration) in table.functions.iter().zip(&table.declarations.functions) {
        counts.registers = counts
            .registers
            .checked_add(frame.values.len())
            .and_then(|n| n.checked_add(declaration.locals.len()))
            .ok_or(Error::Capacity)?;
        counts.places = counts
            .places
            .checked_add(frame.places.len())
            .ok_or(Error::Capacity)?;
    }
    let mut functions = c.budget.vector(input.source().functions.len())?;
    for (index, function) in input.source().functions.iter().enumerate() {
        let frame = &mut table.functions[index];
        let mut paths = c.budget.vector(function.expressions.len())?;
        paths.resize(function.expressions.len(), None);
        c.budget.charge(frame.places.len())?;
        for (i, path) in frame.places.iter().enumerate() {
            paths[path.origin.0 as usize] = Some(p::PlaceId(id(i)?));
        }
        let mut lower = Lowerer {
            c: &mut c,
            input,
            function,
            facts: &input.functions()[index],
            index,
            frame,
            types: &mut table.declarations.types.constructed,
            paths,
            blocks: Vec::new(),
            actions: Vec::new(),
            counts: &mut counts,
            limits,
            loans: 0,
            prepared: 0,
        };
        let entry = lower.block(function.span)?;
        for local in &function.parameters {
            lower.action(Action::Local(d::LocalId(id(local.0)?), function.span))?;
        }
        if lower.body(function.body, entry, 0)?.is_some() {
            return Err(Error::Fallthrough { function: index });
        }
        let mut blocks = lower.c.budget.vector(lower.blocks.len())?;
        for block in lower.blocks {
            blocks.push(b::Block {
                parameters: Vec::new(),
                instructions: block.instructions,
                terminator: block
                    .terminator
                    .ok_or(Error::Fallthrough { function: index })?,
                span: block.span.ok_or(Error::Fallthrough { function: index })?,
            });
        }
        functions.push(b::FunctionBody {
            task_scopes: Vec::new(),
            task_bindings: Vec::new(),
            blocks,
            loans: lower.loans,
            prepared: lower.prepared,
        });
    }
    Ok(b::BodyTable {
        places: table,
        functions,
    }
    .verify_with_budget(
        &mut c.budget,
        limits.places.ownership.constructed,
        limits.places.places,
        limits.loans,
        limits.prepared,
    )?)
}

struct Lowerer<'a, 'b, 'c> {
    c: &'a mut Context<'c>,
    input: &'b BodyTypes,
    function: &'b r::Function,
    facts: &'b FunctionFacts,
    index: usize,
    frame: &'a mut p::FunctionPlaces,
    types: &'a mut Vec<o::Constructed>,
    paths: Vec<Option<p::PlaceId>>,
    blocks: Vec<Block>,
    actions: Vec<Action>,
    counts: &'a mut Counts,
    limits: b::Limits,
    loans: usize,
    prepared: usize,
}
impl Lowerer<'_, '_, '_> {
    fn enter(&mut self, depth: usize) -> Result<()> {
        self.c.budget.charge(1)?;
        cap(
            depth,
            self.limits.places.ownership.unit.depth,
            m::LimitKind::Depth,
        )
    }
    fn action(&mut self, action: Action) -> Result<()> {
        push(self.c, &mut self.actions, action)
    }
    fn block(&mut self, _span: Span) -> Result<m::BlockId> {
        self.counts.blocks = self.counts.blocks.checked_add(1).ok_or(Error::Capacity)?;
        cap(
            self.counts.blocks,
            self.c.budget.limits().blocks,
            m::LimitKind::Blocks,
        )?;
        let id = m::BlockId(id(self.blocks.len())?);
        let span = self.c.span(self.function.span)?;
        push(
            self.c,
            &mut self.blocks,
            Block {
                span: Some(span),
                ..Default::default()
            },
        )?;
        Ok(id)
    }
    fn value(&mut self, ty: o::Type, span: Span) -> Result<m::Register> {
        self.counts.registers = self
            .counts
            .registers
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        cap(
            self.counts.registers,
            self.c.budget.limits().registers,
            m::LimitKind::Registers,
        )?;
        let out = m::Register(id(self.frame.values.len())?);
        let span = self.c.span(span)?;
        push(self.c, &mut self.frame.values, o::TypeUse { ty, span })?;
        Ok(out)
    }
    fn emit(
        &mut self,
        block: m::BlockId,
        output: Option<m::Register>,
        op: b::Op,
        span: Span,
    ) -> Result<()> {
        self.counts.instructions = self
            .counts
            .instructions
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        cap(
            self.counts.instructions,
            self.c.budget.limits().instructions,
            m::LimitKind::Instructions,
        )?;
        let mut results = self.c.budget.vector(usize::from(output.is_some()))?;
        results.extend(output);
        let span = self.c.span(span)?;
        push(
            self.c,
            &mut self.blocks[block.0 as usize].instructions,
            b::Instruction { results, op, span },
        )
    }
    fn seal(&mut self, block: m::BlockId, kind: b::Term, span: Span) -> Result<()> {
        self.c.budget.charge(1)?;
        let span = self.c.span(span)?;
        self.blocks[block.0 as usize].terminator = Some(b::Terminator { kind, span });
        Ok(())
    }
    fn loan(&mut self) -> Result<b::LoanId> {
        self.counts.loans = self.counts.loans.checked_add(1).ok_or(Error::Capacity)?;
        if self.counts.loans > self.limits.loans {
            return Err(o::Error {
                kind: o::ErrorKind::LoanLimit {
                    requested: self.counts.loans,
                    limit: self.limits.loans,
                },
                span: None,
            }
            .into());
        }
        let loan = b::LoanId(id(self.loans)?);
        self.loans = self.loans.checked_add(1).ok_or(Error::Capacity)?;
        Ok(loan)
    }
    fn path(&mut self, expression: r::ExpressionId) -> Result<p::PlaceId> {
        self.c.budget.charge(1)?;
        self.paths
            .get(expression.0)
            .copied()
            .flatten()
            .ok_or(Error::MissingPlace {
                function: self.index,
                expression: expression.0,
            })
    }
    fn evaluate_path(&mut self, path: p::PlaceId, block: m::BlockId, depth: usize) -> Result<()> {
        if let p::Root::Value(value) = self.frame.places[path.0 as usize].root {
            self.expression(r::ExpressionId(value.0 as usize), block, depth + 1)?;
        }
        Ok(())
    }
    fn local_path(&mut self, local: r::LocalId, span: Span) -> Result<p::PlaceId> {
        let ty = ty(self.facts.locals()[local.0])?;
        let origin = self.value(ty, span)?;
        self.counts.places = self.counts.places.checked_add(1).ok_or(Error::Capacity)?;
        if self.counts.places > self.limits.places.places {
            return Err(o::Error {
                kind: o::ErrorKind::PlaceLimit {
                    requested: self.counts.places,
                    limit: self.limits.places.places,
                },
                span: Some(self.c.span(span)?),
            }
            .into());
        }
        let out = p::PlaceId(id(self.frame.places.len())?);
        let span = self.c.span(span)?;
        push(
            self.c,
            &mut self.frame.places,
            p::Place {
                origin,
                root: p::Root::Local(d::LocalId(id(local.0)?)),
                projections: Vec::new(),
                span,
            },
        )?;
        Ok(out)
    }
    fn cleanup(
        &mut self,
        mark: usize,
        block: m::BlockId,
        returned: Option<m::Register>,
    ) -> Result<()> {
        self.cleanup_range(mark, self.actions.len(), block, returned)
    }
    fn cleanup_range(
        &mut self,
        mark: usize,
        end: usize,
        block: m::BlockId,
        returned: Option<m::Register>,
    ) -> Result<()> {
        let mut steps = self.c.budget.vector(end - mark)?;
        for i in (mark..end).rev() {
            self.c.budget.charge(1)?;
            let (target, span) = match self.actions[i] {
                Action::Local(local, span) => (b::CleanupTarget::Local(local), span),
                Action::Value(value, _) if Some(value) == returned => continue,
                Action::Value(value, span) => (b::CleanupTarget::Value(value), span),
                Action::Loan(loan, span) => (b::CleanupTarget::Loan(loan), span),
                Action::Arena(owner, span) => (b::CleanupTarget::Arena(owner), span),
            };
            steps.push(b::CleanupStep {
                target,
                span: self.c.span(span)?,
            });
        }
        if !steps.is_empty() {
            // emit() remaps its source span. Each step already carries the
            // independently remapped original action span.
            let span = match self.actions[mark] {
                Action::Local(_, span)
                | Action::Value(_, span)
                | Action::Loan(_, span)
                | Action::Arena(_, span) => span,
            };
            self.emit(block, None, b::Op::Cleanup(b::Cleanup { steps }), span)?;
        }
        Ok(())
    }
    fn arguments(
        &mut self,
        arguments: &[r::ExpressionId],
        expected: Option<&[r::Type]>,
        block: m::BlockId,
        depth: usize,
    ) -> Result<Vec<m::Register>> {
        let mut out = self.c.budget.vector(arguments.len())?;
        for (index, value) in arguments.iter().enumerate() {
            out.push(self.argument(
                *value,
                expected.and_then(|p| p.get(index)).copied(),
                block,
                depth + 1,
            )?);
        }
        Ok(out)
    }
    fn argument(
        &mut self,
        expression: r::ExpressionId,
        expected: Option<r::Type>,
        block: m::BlockId,
        depth: usize,
    ) -> Result<m::Register> {
        self.enter(depth)?;
        if self.facts.diverges(expression) {
            return self.expression(expression, block, depth + 1);
        }
        let actual = self.facts.expressions()[expression.0];
        let span = self.function.expressions[expression.0].span;
        if let Some(expected @ r::Type::Constructed(id)) = expected
            && let r::TypeConstructor::Loan(kind, target) = self.input.types()[id.0].constructor
        {
            let path = if actual == target {
                Some(self.path(expression)?)
            } else if let r::Type::Constructed(actual) = actual
                && let r::TypeConstructor::Loan(_, other_target) =
                    self.input.types()[actual.0].constructor
                && other_target == target
            {
                let origin = self.value(ty(target)?, span)?;
                let (root, mut projections) = if let Some(path) = self.paths[expression.0] {
                    self.evaluate_path(path, block, depth + 1)?;
                    let path = &self.frame.places[path.0 as usize];
                    let mut projections = self.c.budget.vector(path.projections.len() + 1)?;
                    self.c.budget.charge(path.projections.len())?;
                    projections.extend_from_slice(&path.projections);
                    (path.root, projections)
                } else {
                    let value = self.expression(expression, block, depth + 1)?;
                    (p::Root::Value(value), self.c.budget.vector(1)?)
                };
                projections.push(p::Projection {
                    kind: p::ProjectionKind::Dereference,
                    span: self.c.span(span)?,
                });
                Some(self.add_path(origin, root, projections, span)?)
            } else {
                None
            };
            if let Some(path) = path {
                if actual == target {
                    self.evaluate_path(path, block, depth + 1)?;
                }
                let out = self.value(ty(expected)?, span)?;
                let loan = self.loan()?;
                self.emit(
                    block,
                    Some(out),
                    b::Op::Borrow {
                        loan,
                        kind: match kind {
                            r::LoanKind::Shared => o::LoanKind::Shared,
                            r::LoanKind::Exclusive => o::LoanKind::Exclusive,
                        },
                        place: path,
                    },
                    span,
                )?;
                self.action(Action::Loan(loan, span))?;
                return Ok(out);
            }
        }
        self.expression(expression, block, depth + 1)
    }
    fn add_path(
        &mut self,
        origin: m::Register,
        root: p::Root,
        projections: Vec<p::Projection>,
        span: Span,
    ) -> Result<p::PlaceId> {
        self.counts.places = self.counts.places.checked_add(1).ok_or(Error::Capacity)?;
        if self.counts.places > self.limits.places.places {
            return Err(o::Error {
                kind: o::ErrorKind::PlaceLimit {
                    requested: self.counts.places,
                    limit: self.limits.places.places,
                },
                span: Some(self.c.span(span)?),
            }
            .into());
        }
        let out = p::PlaceId(id(self.frame.places.len())?);
        let span = self.c.span(span)?;
        push(
            self.c,
            &mut self.frame.places,
            p::Place {
                origin,
                root,
                projections,
                span,
            },
        )?;
        Ok(out)
    }
    fn expression(
        &mut self,
        expression: r::ExpressionId,
        block: m::BlockId,
        depth: usize,
    ) -> Result<m::Register> {
        self.enter(depth)?;
        let source = &self.function.expressions[expression.0];
        let span = source.span;
        let out = m::Register(id(expression.0)?);
        if self.facts.diverges(expression) {
            let mut code = None;
            for (index, child) in source.kind.operands().enumerate() {
                let expected = self
                    .facts
                    .call(expression)
                    .map(|call| call.parameters[index]);
                code = Some(self.argument(child, expected, block, depth + 1)?);
                if self.facts.diverges(child) {
                    return Ok(out);
                }
            }
            if matches!(
                source.kind,
                r::ExpressionKind::CoreCall {
                    operation: lang_hir::core::Operation::Panic,
                    ..
                }
            ) {
                self.emit(
                    block,
                    None,
                    b::Op::Panic(code.ok_or(Error::Capacity)?),
                    span,
                )?;
                self.seal(block, b::Term::Unreachable, span)?;
            }
            return Ok(out);
        }
        let op = match &source.kind {
            r::ExpressionKind::Spawn(_) | r::ExpressionKind::Await(_) => {
                return Err(Error::UnsupportedBody {
                    feature: super::UnsupportedBody::Task,
                    span: source.span,
                });
            }
            r::ExpressionKind::Integer(value) => b::Op::I64(*value),
            r::ExpressionKind::Boolean(value) => b::Op::Bool(*value),
            r::ExpressionKind::CoreUnit => b::Op::Unit,
            r::ExpressionKind::Local(_)
            | r::ExpressionKind::Field { .. }
            | r::ExpressionKind::Dereference(_) => {
                let path = self.path(expression)?;
                self.evaluate_path(path, block, depth)?;
                if self
                    .input
                    .properties(self.facts.expressions()[expression.0])
                    .unwrap()
                    .is_copy
                {
                    b::Op::CopyPlace(path)
                } else {
                    b::Op::MovePlace(path)
                }
            }
            r::ExpressionKind::Borrow { kind, place } => {
                let path = self.path(r::ExpressionId(place.0))?;
                self.evaluate_path(path, block, depth)?;
                let loan = self.loan()?;
                self.action(Action::Loan(loan, span))?;
                b::Op::Borrow {
                    loan,
                    kind: match kind {
                        r::LoanKind::Shared => o::LoanKind::Shared,
                        r::LoanKind::Exclusive => o::LoanKind::Exclusive,
                    },
                    place: path,
                }
            }
            r::ExpressionKind::Add(left, right) | r::ExpressionKind::LessThan(left, right) => {
                let left = self.expression(*left, block, depth + 1)?;
                let right = self.expression(*right, block, depth + 1)?;
                if matches!(source.kind, r::ExpressionKind::Add(..)) {
                    b::Op::Add(left, right)
                } else {
                    b::Op::LessThan(left, right)
                }
            }
            r::ExpressionKind::Call { callee, arguments } => {
                let mark = self.actions.len();
                let mut parameters = self.c.budget.vector(match callee {
                    r::Callable::Guest(f) => self.input.source().functions[f.0].parameters.len(),
                    _ => 0,
                })?;
                if let r::Callable::Guest(f) = callee {
                    for p in &self.input.source().functions[f.0].parameters {
                        parameters.push(self.input.functions()[f.0].locals()[p.0]);
                    }
                }
                let arguments = self.arguments(arguments, Some(&parameters), block, depth)?;
                let op = match callee {
                    r::Callable::Guest(function) => b::Op::Call {
                        function: m::FunctionId(id(function.0)?),
                        arguments,
                    },
                    r::Callable::Host(import) => b::Op::Import {
                        import: m::ImportId(id(import.0)?),
                        arguments,
                    },
                    r::Callable::Case { index, .. } => b::Op::Variant {
                        case: id(*index)?,
                        payload: arguments,
                    },
                };
                self.emit(block, Some(out), op, span)?;
                self.cleanup(mark, block, None)?;
                self.actions.truncate(mark);
                return self.result(expression);
            }
            r::ExpressionKind::CoreCall { .. } => {
                return self.core_call(expression, false, block, depth);
            }
            r::ExpressionKind::Record { definition, fields } => {
                let mut values = self.c.budget.vector(fields.len())?;
                let mut initialization_order = self.c.budget.vector(fields.len())?;
                values.resize(fields.len(), m::Register(0));
                for (index, field) in fields.iter().enumerate() {
                    let ordinal = self.facts.record_order(expression).unwrap()[index];
                    values[ordinal] = self.expression(field.value, block, depth + 1)?;
                    initialization_order.push(id(ordinal)?);
                }
                b::Op::Record {
                    definition: m::DefinitionId(id(definition.0)?),
                    fields: values,
                    initialization_order,
                }
            }
            r::ExpressionKind::Case { index, .. } => b::Op::Variant {
                case: id(*index)?,
                payload: Vec::new(),
            },
            r::ExpressionKind::CoreCase { case, payload, .. } => {
                self.c.budget.charge(lang_hir::core::Case::ALL.len())?;
                let index = lang_hir::core::Case::ALL
                    .into_iter()
                    .filter(|c| c.family() == case.family())
                    .position(|c| c == *case)
                    .unwrap();
                b::Op::Variant {
                    case: id(index)?,
                    payload: self.arguments(payload, None, block, depth)?,
                }
            }
        };
        self.emit(block, Some(out), op, span)?;
        self.result(expression)
    }
    fn result(&mut self, expression: r::ExpressionId) -> Result<m::Register> {
        let value = m::Register(id(expression.0)?);
        self.c.budget.charge(1)?;
        if self
            .input
            .properties(self.facts.expressions()[expression.0])
            .unwrap()
            .requires_drop
        {
            self.action(Action::Value(
                value,
                self.function.expressions[expression.0].span,
            ))?;
        }
        Ok(value)
    }
    fn assignment(
        &mut self,
        path: p::PlaceId,
        value: r::ExpressionId,
        block: m::BlockId,
        span: Span,
        depth: usize,
    ) -> Result<()> {
        self.evaluate_path(path, block, depth)?;
        self.counts.prepared = self.counts.prepared.checked_add(1).ok_or(Error::Capacity)?;
        if self.counts.prepared > self.limits.prepared {
            return Err(o::Error {
                kind: o::ErrorKind::PreparedLimit {
                    requested: self.counts.prepared,
                    limit: self.limits.prepared,
                },
                span: Some(self.c.span(span)?),
            }
            .into());
        }
        let destination = b::PreparedPlaceId(id(self.prepared)?);
        self.prepared = self.prepared.checked_add(1).ok_or(Error::Capacity)?;
        self.emit(
            block,
            None,
            b::Op::PreparePlace {
                destination,
                place: path,
            },
            span,
        )?;
        let value = self.expression(value, block, depth + 1)?;
        if self.blocks[block.0 as usize].terminator.is_some() {
            return Ok(());
        }
        self.emit(
            block,
            None,
            b::Op::ReplacePrepared { destination, value },
            span,
        )
    }
    fn body(
        &mut self,
        source: r::BlockId,
        entry: m::BlockId,
        depth: usize,
    ) -> Result<Option<m::BlockId>> {
        self.enter(depth)?;
        let mark = self.actions.len();
        let mut current = Some(entry);
        for statement in &self.function.blocks[source.0].statements {
            let Some(block) = current else { break };
            self.c.budget.charge(1)?;
            let span = statement.span;
            if let Some(expression) = statement
                .kind
                .operands()
                .find(|id| self.facts.diverges(*id))
            {
                match statement.kind {
                    r::StatementKind::PlaceAssign { place, .. } if place.0 != expression.0 => {
                        let path = self.path(r::ExpressionId(place.0))?;
                        self.assignment(path, expression, block, span, depth + 1)?;
                    }
                    r::StatementKind::Assign { local, .. } => {
                        let path = self.local_path(local, span)?;
                        self.assignment(path, expression, block, span, depth + 1)?;
                    }
                    _ => {
                        self.expression(expression, block, depth + 1)?;
                    }
                }
                current = None;
                continue;
            }
            match &statement.kind {
                r::StatementKind::TaskScope { .. }
                | r::StatementKind::Yield
                | r::StatementKind::Cancel(_)
                | r::StatementKind::CancelCheck => {
                    return Err(Error::UnsupportedBody {
                        feature: super::UnsupportedBody::Task,
                        span: statement.span,
                    });
                }
                r::StatementKind::LetMut { local, initializer } => {
                    let value = self.expression(*initializer, block, depth + 1)?;
                    let local = d::LocalId(id(local.0)?);
                    self.emit(block, None, b::Op::Initialize { local, value }, span)?;
                    self.action(Action::Local(local, span))?;
                }
                r::StatementKind::Assign { local, value } => {
                    let path = self.local_path(*local, span)?;
                    self.assignment(path, *value, block, span, depth + 1)?;
                }
                r::StatementKind::PlaceAssign { place, value } => {
                    let path = self.path(r::ExpressionId(place.0))?;
                    self.assignment(path, *value, block, span, depth + 1)?;
                }
                r::StatementKind::Return(value) => {
                    let value = self.expression(*value, block, depth + 1)?;
                    self.cleanup(0, block, Some(value))?;
                    self.seal(block, b::Term::Return(value), span)?;
                    current = None;
                }
                r::StatementKind::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let condition = self.expression(*condition, block, depth + 1)?;
                    let yes = self.block(span)?;
                    let no = self.block(span)?;
                    self.seal(
                        block,
                        b::Term::Branch {
                            condition,
                            then_target: target(yes),
                            else_target: target(no),
                        },
                        span,
                    )?;
                    let left = self.body(*then_block, yes, depth + 1)?;
                    let right = if let Some(other) = else_block {
                        self.body(*other, no, depth + 1)?
                    } else {
                        Some(no)
                    };
                    current = if left.is_some() || right.is_some() {
                        let join = self.block(span)?;
                        for predecessor in [left, right].into_iter().flatten() {
                            self.seal(predecessor, b::Term::Jump(target(join)), span)?;
                        }
                        Some(join)
                    } else {
                        None
                    };
                }
                r::StatementKind::While { condition, body } => {
                    let header = self.block(span)?;
                    let work = self.block(span)?;
                    let after = self.block(span)?;
                    self.seal(block, b::Term::Jump(target(header)), span)?;
                    let condition = self.expression(*condition, header, depth + 1)?;
                    self.seal(
                        header,
                        b::Term::Branch {
                            condition,
                            then_target: target(work),
                            else_target: target(after),
                        },
                        span,
                    )?;
                    if let Some(exit) = self.body(*body, work, depth + 1)? {
                        self.seal(exit, b::Term::Jump(target(header)), span)?;
                    }
                    current = Some(after);
                }
                r::StatementKind::Unsafe { body } => {
                    current = self.body(*body, block, depth + 1)?;
                }
                r::StatementKind::Arena {
                    owner,
                    allocator,
                    body,
                } => {
                    let arena_mark = self.actions.len();
                    // HIR checks this as an implicit shared allocator loan,
                    // including a dereferenced allocator place. Reborrow using
                    // that expected type rather than moving the expression.
                    self.c.budget.charge(self.input.types().len())?;
                    let expected = self
                        .input
                        .types()
                        .iter()
                        .enumerate()
                        .find_map(|(index, ty)| {
                            if let r::TypeConstructor::Loan(
                                r::LoanKind::Shared,
                                r::Type::Constructed(target),
                            ) = ty.constructor
                                && self.input.types()[target.0].constructor
                                    == r::TypeConstructor::Builtin(r::BuiltinType::Allocator)
                            {
                                Some(r::Type::Constructed(r::ConstructedTypeId(index)))
                            } else {
                                None
                            }
                        })
                        .ok_or(Error::Capacity)?;
                    let allocator = self.argument(*allocator, Some(expected), block, depth + 1)?;
                    let owner = d::LocalId(id(owner.0)?);
                    self.emit(block, None, b::Op::ArenaEnter { owner, allocator }, span)?;
                    self.action(Action::Arena(owner, span))?;
                    current = self.body(*body, block, depth + 1)?;
                    if let Some(exit) = current {
                        self.cleanup(arena_mark, exit, None)?;
                    }
                    self.actions.truncate(arena_mark);
                }
                r::StatementKind::Match { scrutinee, arms } => {
                    current = self.match_statement(*scrutinee, arms, block, span, depth + 1)?;
                }
            }
        }
        if let Some(exit) = current {
            self.cleanup(mark, exit, None)?;
        }
        self.actions.truncate(mark);
        Ok(current)
    }
}

fn target(block: m::BlockId) -> m::Target {
    m::Target {
        block,
        arguments: Vec::new(),
    }
}
