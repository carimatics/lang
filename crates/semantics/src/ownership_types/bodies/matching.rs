//! Test a pattern before publishing bindings. Failed tests release their views.
use super::*;

#[derive(Clone, Copy)]
struct View {
    ty: o::Type,
    kind: ViewKind,
}
#[derive(Clone, Copy)]
enum ViewKind {
    Value(m::Register),
    Field {
        parent: m::Register,
        field: u32,
    },
    Case {
        parent: m::Register,
        case: u32,
        field: u32,
        guard: m::Register,
    },
}
#[derive(Clone, Copy)]
struct Binding {
    local: r::LocalId,
    view: View,
    span: Span,
}

impl Lowerer<'_, '_, '_> {
    fn ctor(&mut self, ty: o::Type) -> Result<Option<o::Constructor>> {
        self.c.budget.charge(1)?;
        Ok(match ty {
            o::Type::Constructed(id) => Some(self.types[id.0 as usize].constructor),
            _ => None,
        })
    }
    fn copy_type(&mut self, ty: o::Type) -> Result<bool> {
        // All ordinary loans are affine, including shared loans.
        if let Some(o::Constructor::Loan(..)) = self.ctor(ty)? {
            return Ok(false);
        }
        Ok(self
            .input
            .properties(match ty {
                o::Type::I64 => r::Type::I64,
                o::Type::Bool => r::Type::Bool,
                o::Type::Nominal(id) => r::Type::Nominal(r::DefinitionId(id.0 as usize)),
                o::Type::Constructed(id) => {
                    r::Type::Constructed(r::ConstructedTypeId(id.0 as usize))
                }
            })
            .unwrap()
            .is_copy)
    }
    fn shared_type(&mut self, target: o::Type, span: Span) -> Result<o::Type> {
        for i in 0..self.types.len() {
            self.c.budget.charge(1)?;
            if self.types[i].constructor == o::Constructor::Loan(o::LoanKind::Shared, target) {
                return Ok(o::Type::Constructed(o::ConstructedId(id(i)?)));
            }
        }
        let count = self.types.len().checked_add(1).ok_or(Error::Capacity)?;
        if count > self.limits.places.ownership.constructed {
            return Err(o::Error {
                kind: o::ErrorKind::ConstructedLimit {
                    requested: count,
                    limit: self.limits.places.ownership.constructed,
                },
                span: Some(self.c.span(span)?),
            }
            .into());
        }
        let ty = o::Type::Constructed(o::ConstructedId(id(self.types.len())?));
        let span = self.c.span(span)?;
        push(
            self.c,
            self.types,
            o::Constructed {
                constructor: o::Constructor::Loan(o::LoanKind::Shared, target),
                span,
            },
        )?;
        Ok(ty)
    }
    fn borrow_shared(
        &mut self,
        path: p::PlaceId,
        target: o::Type,
        block: m::BlockId,
        span: Span,
    ) -> Result<m::Register> {
        let ty = self.shared_type(target, span)?;
        let value = self.value(ty, span)?;
        let loan = self.loan()?;
        self.emit(
            block,
            Some(value),
            b::Op::Borrow {
                loan,
                kind: o::LoanKind::Shared,
                place: path,
            },
            span,
        )?;
        self.action(Action::Loan(loan, span))?;
        Ok(value)
    }
    fn value_type(&self, value: m::Register) -> o::Type {
        self.frame.values[value.0 as usize].ty
    }
    fn projection(&mut self, kind: p::ProjectionKind, span: Span) -> Result<p::Projection> {
        Ok(p::Projection {
            kind,
            span: self.c.span(span)?,
        })
    }
    fn resolve_view(&mut self, view: View, block: m::BlockId, span: Span) -> Result<m::Register> {
        match view.kind {
            ViewKind::Value(value) => Ok(value),
            ViewKind::Field { parent, field } => {
                let origin = self.value(view.ty, span)?;
                let mut steps = self.c.budget.vector(2)?;
                if matches!(
                    self.ctor(self.value_type(parent))?,
                    Some(o::Constructor::Loan(..))
                ) {
                    steps.push(self.projection(p::ProjectionKind::Dereference, span)?);
                }
                steps.push(self.projection(p::ProjectionKind::Field(field), span)?);
                let path = self.add_path(origin, p::Root::Value(parent), steps, span)?;
                if self.copy_type(view.ty)? {
                    self.emit(block, Some(origin), b::Op::CopyPlace(path), span)?;
                    Ok(origin)
                } else {
                    self.borrow_shared(path, view.ty, block, span)
                }
            }
            ViewKind::Case {
                parent,
                case,
                field,
                guard,
            } => {
                let copied = self.copy_type(view.ty)?;
                let ty = if copied {
                    view.ty
                } else {
                    self.shared_type(view.ty, span)?
                };
                let value = self.value(ty, span)?;
                let op = if copied {
                    b::Op::CopyCaseField {
                        value: parent,
                        case,
                        field,
                        guard,
                    }
                } else {
                    let loan = self.loan()?;
                    self.action(Action::Loan(loan, span))?;
                    b::Op::BorrowCaseField {
                        loan,
                        value: parent,
                        case,
                        field,
                        guard,
                    }
                };
                self.emit(block, Some(value), op, span)?;
                Ok(value)
            }
        }
    }
    fn shape(
        &mut self,
        view: View,
        block: m::BlockId,
        span: Span,
        scalar: bool,
    ) -> Result<m::Register> {
        let value = self.resolve_view(view, block, span)?;
        let mut ty = self.value_type(value);
        let mut count = 0usize;
        while let Some(o::Constructor::Loan(_, target)) = self.ctor(ty)? {
            count += 1;
            ty = target;
        }
        if count == 0 || (count == 1 && !scalar) {
            return Ok(value);
        }
        let origin = self.value(ty, span)?;
        let mut steps = self.c.budget.vector(count)?;
        for _ in 0..count {
            steps.push(self.projection(p::ProjectionKind::Dereference, span)?);
        }
        let path = self.add_path(origin, p::Root::Value(value), steps, span)?;
        if self.copy_type(ty)? {
            self.emit(block, Some(origin), b::Op::CopyPlace(path), span)?;
            Ok(origin)
        } else {
            self.borrow_shared(path, ty, block, span)
        }
    }
    fn can_fail(&mut self, id: r::PatternId, depth: usize) -> Result<bool> {
        self.enter(depth)?;
        match &self.function.patterns[id.0].kind {
            r::PatternKind::Wildcard | r::PatternKind::Binding(_) | r::PatternKind::CoreUnit => {
                Ok(false)
            }
            r::PatternKind::Record { fields, .. } => {
                for field in fields {
                    if self.can_fail(field.pattern, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(true),
        }
    }
    fn loan_binding(&mut self, id: r::PatternId, depth: usize) -> Result<bool> {
        self.enter(depth)?;
        match &self.function.patterns[id.0].kind {
            r::PatternKind::Binding(local) => Ok(
                matches!(self.facts.locals()[local.0], r::Type::Constructed(id) if matches!(self.input.types()[id.0].constructor, r::TypeConstructor::Loan(..))),
            ),
            r::PatternKind::Record { fields, .. } => {
                for field in fields {
                    if self.loan_binding(field.pattern, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            r::PatternKind::Case { payload, .. } | r::PatternKind::CoreCase { payload, .. } => {
                for p in payload {
                    if self.loan_binding(*p, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    fn pattern_guard(
        &mut self,
        block: m::BlockId,
        condition: m::Register,
        failure: m::BlockId,
        mark: usize,
        span: Span,
        positive: bool,
    ) -> Result<m::BlockId> {
        let pass = self.block(span)?;
        let fail = self.block(span)?;
        self.cleanup(mark, fail, None)?;
        self.seal(fail, b::Term::Jump(target(failure)), span)?;
        let (yes, no) = if positive { (pass, fail) } else { (fail, pass) };
        self.seal(
            block,
            b::Term::Branch {
                condition,
                then_target: target(yes),
                else_target: target(no),
            },
            span,
        )?;
        Ok(pass)
    }
    #[allow(clippy::too_many_arguments)]
    fn test_pattern(
        &mut self,
        id: r::PatternId,
        view: View,
        mut block: m::BlockId,
        failure: Option<m::BlockId>,
        mark: usize,
        bindings: &mut Vec<Binding>,
        depth: usize,
    ) -> Result<m::BlockId> {
        self.enter(depth)?;
        let pattern = &self.function.patterns[id.0];
        let span = pattern.span;
        match &pattern.kind {
            r::PatternKind::Wildcard | r::PatternKind::CoreUnit => {}
            r::PatternKind::Binding(local) => push(
                self.c,
                bindings,
                Binding {
                    local: *local,
                    view,
                    span,
                },
            )?,
            r::PatternKind::Integer(integer) => {
                let value = self.shape(view, block, span, true)?;
                let constant = self.value(o::Type::I64, span)?;
                self.emit(block, Some(constant), b::Op::I64(*integer), span)?;
                let condition = self.value(o::Type::Bool, span)?;
                self.emit(
                    block,
                    Some(condition),
                    b::Op::EqualI64(value, constant),
                    span,
                )?;
                block = self.pattern_guard(block, condition, failure.unwrap(), mark, span, true)?;
            }
            r::PatternKind::Boolean(boolean) => {
                let value = self.shape(view, block, span, true)?;
                block = self.pattern_guard(block, value, failure.unwrap(), mark, span, *boolean)?;
            }
            r::PatternKind::Record { .. } => {
                let parent = self.shape(view, block, span, false)?;
                for (field, child) in self.facts.pattern_fields(id).unwrap().iter().enumerate() {
                    let child_view = View {
                        ty: ty(self.facts.patterns()[child.0])?,
                        kind: ViewKind::Field {
                            parent,
                            field: super::id(field)?,
                        },
                    };
                    block = self.test_pattern(
                        *child,
                        child_view,
                        block,
                        failure,
                        mark,
                        bindings,
                        depth + 1,
                    )?;
                }
            }
            r::PatternKind::Case { index, payload, .. } => {
                block = self.test_case(
                    view,
                    super::id(*index)?,
                    payload,
                    block,
                    failure.unwrap(),
                    mark,
                    bindings,
                    span,
                    depth,
                )?;
            }
            r::PatternKind::CoreCase { case, payload, .. } => {
                self.c.budget.charge(lang_hir::core::Case::ALL.len())?;
                let case_index = lang_hir::core::Case::ALL
                    .into_iter()
                    .filter(|c| c.family() == case.family())
                    .position(|c| c == *case)
                    .unwrap();
                block = self.test_case(
                    view,
                    super::id(case_index)?,
                    payload,
                    block,
                    failure.unwrap(),
                    mark,
                    bindings,
                    span,
                    depth,
                )?;
            }
        }
        Ok(block)
    }
    #[allow(clippy::too_many_arguments)]
    fn test_case(
        &mut self,
        view: View,
        case: u32,
        payload: &[r::PatternId],
        block: m::BlockId,
        failure: m::BlockId,
        mark: usize,
        bindings: &mut Vec<Binding>,
        span: Span,
        depth: usize,
    ) -> Result<m::BlockId> {
        let parent = self.shape(view, block, span, false)?;
        let guard = self.value(o::Type::Bool, span)?;
        self.emit(
            block,
            Some(guard),
            b::Op::IsCase {
                value: parent,
                case,
            },
            span,
        )?;
        let mut block = self.pattern_guard(block, guard, failure, mark, span, true)?;
        for (field, child) in payload.iter().enumerate() {
            let child_view = View {
                ty: ty(self.facts.patterns()[child.0])?,
                kind: ViewKind::Case {
                    parent,
                    case,
                    field: super::id(field)?,
                    guard,
                },
            };
            block = self.test_pattern(
                *child,
                child_view,
                block,
                Some(failure),
                mark,
                bindings,
                depth + 1,
            )?;
        }
        Ok(block)
    }
    fn bind_pattern(&mut self, binding: Binding, block: m::BlockId) -> Result<()> {
        let Binding { local, view, span } = binding;
        let expected = ty(self.facts.locals()[local.0])?;
        let value = if let ViewKind::Case {
            parent,
            case,
            field: 0,
            guard,
        } = view.kind
            && expected == view.ty
            && matches!(self.ctor(expected)?, Some(o::Constructor::Loan(..)))
        {
            // Only compiler-created provisional sums can contain an ordinary
            // loan payload. Test before taking its sole complete payload.
            let out = self.value(expected, span)?;
            self.emit(
                block,
                Some(out),
                b::Op::UnpackVariant {
                    value: parent,
                    case,
                    guard,
                },
                span,
            )?;
            out
        } else {
            let value = self.resolve_view(view, block, span)?;
            self.binding_value(value, expected, block, span)?
        };
        let local = d::LocalId(super::id(local.0)?);
        self.emit(block, None, b::Op::Initialize { local, value }, span)?;
        self.action(Action::Local(local, span))
    }
    fn binding_value(
        &mut self,
        value: m::Register,
        expected: o::Type,
        block: m::BlockId,
        span: Span,
    ) -> Result<m::Register> {
        if self.value_type(value) == expected {
            return Ok(value);
        }
        let borrowed = match self.ctor(expected)? {
            Some(o::Constructor::Loan(kind, target)) => Some((kind, target)),
            _ => None,
        };
        let target = borrowed.map_or(expected, |(_, target)| target);
        let mut ty = self.value_type(value);
        let mut steps = Vec::new();
        while ty != target {
            let Some(o::Constructor::Loan(_, next)) = self.ctor(ty)? else {
                return Ok(value);
            };
            let step = self.projection(p::ProjectionKind::Dereference, span)?;
            push(self.c, &mut steps, step)?;
            ty = next;
        }
        let origin = self.value(target, span)?;
        let path = self.add_path(origin, p::Root::Value(value), steps, span)?;
        if let Some((kind, _)) = borrowed {
            let out = self.value(expected, span)?;
            let loan = self.loan()?;
            self.emit(
                block,
                Some(out),
                b::Op::Borrow {
                    loan,
                    kind,
                    place: path,
                },
                span,
            )?;
            self.action(Action::Loan(loan, span))?;
            Ok(out)
        } else {
            self.emit(block, Some(origin), b::Op::CopyPlace(path), span)?;
            Ok(origin)
        }
    }
    pub(super) fn message_await(
        &mut self,
        expression: r::ExpressionId,
        call: r::ExpressionId,
        block: m::BlockId,
        depth: usize,
    ) -> Result<m::Register> {
        let source = &self.function.expressions[call.0];
        let r::ExpressionKind::CoreCall {
            operation,
            arguments,
            type_arguments,
            ..
        } = &source.kind
        else {
            unreachable!("typed message await")
        };
        let span = self.function.expressions[expression.0].span;
        let mark = self.actions.len();
        let mut values = self.c.budget.vector(arguments.len())?;
        for (index, argument) in arguments.iter().enumerate() {
            values.push(self.argument(
                *argument,
                Some(self.facts.call(call).unwrap().parameters[index]),
                block,
                depth + 1,
            )?);
        }
        let destination = b::PreparedPlaceId(id(self.prepared)?);
        self.prepared = self.prepared.checked_add(1).ok_or(Error::Capacity)?;
        let sending = matches!(
            operation,
            lang_hir::core::Operation::ChannelSend | lang_hir::core::Operation::ServiceSend
        );
        let service = operation.is_service();
        let message_index = usize::from(service && !sending);
        let message_type = o::TypeUse {
            ty: ty(type_arguments[message_index].value_type)?,
            span: self.c.span(type_arguments[message_index].span)?,
        };
        let capture = if service {
            b::Op::PrepareService {
                destination,
                endpoint: values[0],
                role: if sending {
                    b::ServiceRole::Handle
                } else {
                    b::ServiceRole::Owner
                },
                state_type: if sending {
                    None
                } else {
                    Some(o::TypeUse {
                        ty: ty(type_arguments[0].value_type)?,
                        span: self.c.span(type_arguments[0].span)?,
                    })
                },
                message_type,
            }
        } else {
            b::Op::PrepareChannel {
                destination,
                endpoint: values[0],
                role: if sending {
                    b::ChannelRole::Sender
                } else {
                    b::ChannelRole::Receiver
                },
                message_type,
            }
        };
        self.emit(block, None, capture, span)?;
        let payload = sending.then(|| values[1]);
        // Preparation captured only private protocol authority. End temporary
        // argument loans before the explicit suspension; the payload remains
        // owned until the following instruction consumes/returns/retires it.
        self.cleanup(mark, block, payload)?;
        self.actions.truncate(mark);
        let op = match (service, payload) {
            (true, Some(payload)) => b::Op::ServiceSend {
                authority: destination,
                payload,
            },
            (true, None) => b::Op::ServiceReceive {
                authority: destination,
            },
            (false, Some(payload)) => b::Op::ChannelSend {
                authority: destination,
                payload,
            },
            (false, None) => b::Op::ChannelReceive {
                authority: destination,
            },
        };
        let out = m::Register(id(expression.0)?);
        self.emit(block, Some(out), op, span)?;
        self.result(expression)
    }

    pub(super) fn core_call(
        &mut self,
        expression: r::ExpressionId,
        allow_provisional: bool,
        block: m::BlockId,
        depth: usize,
    ) -> Result<m::Register> {
        let source = &self.function.expressions[expression.0];
        let r::ExpressionKind::CoreCall {
            operation,
            arguments,
            type_arguments,
            ..
        } = &source.kind
        else {
            unreachable!()
        };
        let span = source.span;
        if *operation == lang_hir::core::Operation::Drop {
            // Preserve ordinary argument evaluation/moves and the current
            // lexical loan checks. Copy arguments have no retirement action;
            // owners use exactly the existing explicit-drop failure path.
            let mark = self.actions.len();
            let argument = arguments[0];
            let expected = self.facts.call(expression).unwrap().parameters[0];
            let value = self.argument(argument, Some(expected), block, depth + 1)?;
            if self.input.properties(expected).unwrap().requires_drop {
                self.emit(block, None, b::Op::DropValue(value), span)?;
            }
            let out = m::Register(super::id(expression.0)?);
            self.emit(block, Some(out), b::Op::Unit, span)?;
            self.cleanup(mark, block, None)?;
            self.actions.truncate(mark);
            return self.result(expression);
        }
        let operation = super::super::core_operation(*operation).ok_or(Error::UnsupportedBody {
            feature: UnsupportedBody::SourceIntrinsic,
            span,
        })?;
        let retained = match operation.signature().result_binding {
            o::core::ResultBinding::ProvisionalLoan { parameter, .. } => Some(parameter),
            _ => None,
        };
        if retained.is_some() && !allow_provisional {
            return Err(Error::UnsupportedBody {
                feature: UnsupportedBody::ProvisionalResult,
                span,
            });
        }
        let mark = self.actions.len();
        let mut ranges = self.c.budget.vector(arguments.len())?;
        let mut values = self.c.budget.vector(arguments.len())?;
        for (i, argument) in arguments.iter().enumerate() {
            let start = self.actions.len();
            values.push(self.argument(
                *argument,
                Some(self.facts.call(expression).unwrap().parameters[i]),
                block,
                depth + 1,
            )?);
            ranges.push((start, self.actions.len()));
        }
        let mut types = self.c.budget.vector(type_arguments.len())?;
        for argument in type_arguments {
            types.push(o::TypeUse {
                ty: ty(argument.value_type)?,
                span: self.c.span(argument.span)?,
            });
        }
        let result_loan = if retained.is_some() || operation == o::core::Operation::Allocator {
            Some(self.loan()?)
        } else {
            None
        };
        let out = m::Register(super::id(expression.0)?);
        self.emit(
            block,
            Some(out),
            b::Op::CoreCall(o::core::Call {
                operation,
                arguments: values,
                type_arguments: types,
                result_loan,
            }),
            span,
        )?;
        if let Some(retained) = retained {
            for (i, (start, end)) in ranges.into_iter().enumerate().rev() {
                if i != retained {
                    self.cleanup_range(start, end, block, None)?;
                    self.c.budget.charge(self.actions.len())?;
                    self.actions.drain(start..end);
                }
            }
        } else {
            self.cleanup(mark, block, None)?;
            self.actions.truncate(mark);
        }
        if let Some(loan) = result_loan {
            self.action(Action::Loan(loan, span))?;
        }
        self.result(expression)
    }
    pub(super) fn match_statement(
        &mut self,
        scrutinee: r::ExpressionId,
        arms: &[r::Arm],
        entry: m::BlockId,
        span: Span,
        depth: usize,
    ) -> Result<Option<m::BlockId>> {
        self.enter(depth)?;
        let mark = self.actions.len();
        let source_ty = self.facts.expressions()[scrutinee.0];
        let sem_ty = ty(source_ty)?;
        let provisional = matches!(&self.function.expressions[scrutinee.0].kind, r::ExpressionKind::CoreCall { operation, .. } if matches!(operation.signature().result_binding, lang_hir::core::ResultBinding::ProvisionalLoan { .. }));
        let value = if provisional {
            let value = self.core_call(scrutinee, true, entry, depth + 1)?;
            self.action(Action::Value(
                value,
                self.function.expressions[scrutinee.0].span,
            ))?;
            value
        } else if matches!(self.ctor(sem_ty)?, Some(o::Constructor::Loan(..))) {
            self.argument(scrutinee, Some(source_ty), entry, depth + 1)?
        } else if !self.copy_type(sem_ty)? {
            let path = if let Some(path) = self.paths[scrutinee.0] {
                self.evaluate_path(path, entry, depth + 1)?;
                path
            } else {
                let value = self.expression(scrutinee, entry, depth + 1)?;
                let origin = self.value(sem_ty, span)?;
                self.add_path(origin, p::Root::Value(value), Vec::new(), span)?
            };
            self.borrow_shared(path, sem_ty, entry, span)?
        } else {
            self.expression(scrutinee, entry, depth + 1)?
        };
        let mut snapshot = self.c.budget.vector(self.actions.len() - mark)?;
        self.c.budget.charge(self.actions.len() - mark)?;
        snapshot.extend_from_slice(&self.actions[mark..]);
        let pattern_mark = self.actions.len();
        let mut reached = self.c.budget.vector(arms.len())?;
        for arm in arms {
            self.c.budget.charge(1)?;
            if !self.facts.arm_is_unreachable(arm.pattern) {
                reached.push(arm);
            }
        }
        let mut next = entry;
        let mut join = None;
        for (index, arm) in reached.iter().enumerate() {
            let failed = if self.can_fail(arm.pattern, depth + 1)? {
                Some(self.block(arm.span)?)
            } else {
                None
            };
            let mut bindings = Vec::new();
            let mut selected = self.test_pattern(
                arm.pattern,
                View {
                    ty: sem_ty,
                    kind: ViewKind::Value(value),
                },
                next,
                failed,
                pattern_mark,
                &mut bindings,
                depth + 1,
            )?;
            let binding_mark = self.actions.len();
            for binding in bindings {
                self.bind_pattern(binding, selected)?;
            }
            let early = provisional && !self.loan_binding(arm.pattern, depth + 1)?;
            let arm_mark = if early {
                self.cleanup_range(pattern_mark, binding_mark, selected, None)?;
                self.cleanup_range(mark, pattern_mark, selected, None)?;
                self.c.budget.charge(self.actions.len())?;
                self.actions.drain(mark..binding_mark);
                mark
            } else {
                pattern_mark
            };
            let exit = self.body(arm.body, selected, depth + 1)?;
            if let Some(exit) = exit {
                selected = exit;
                self.cleanup(arm_mark, selected, None)?;
                if !early {
                    self.cleanup_range(mark, pattern_mark, selected, None)?;
                }
                let after = if let Some(join) = join {
                    join
                } else {
                    let block = self.block(span)?;
                    join = Some(block);
                    block
                };
                self.seal(selected, b::Term::Jump(target(after)), arm.span)?;
            }
            self.actions.truncate(mark);
            for action in &snapshot {
                self.action(*action)?;
            }
            if let Some(failed) = failed {
                if index + 1 == reached.len() {
                    self.cleanup(0, failed, None)?;
                    self.seal(failed, b::Term::Unreachable, span)?;
                } else {
                    next = failed;
                }
            }
        }
        self.actions.truncate(mark);
        Ok(join)
    }
}
