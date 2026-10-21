use super::*;
use crate::unit::ownership::{
    Constructor, Error, ErrorKind, Type, VerifiedTypes, at, declarations,
};
use crate::unit::{Budget, FunctionId, Span, validation};

fn constructor(types: &VerifiedTypes, ty: Type) -> Option<Constructor> {
    if let Type::Constructed(id) = ty {
        types
            .table()
            .constructed
            .get(id.0 as usize)
            .map(|t| t.constructor)
    } else {
        None
    }
}
fn within(
    types: &VerifiedTypes,
    owner: Span,
    span: Span,
    budget: &mut Budget,
) -> Result<(), Error> {
    at(
        validation::source_span(&types.table().sources, Some(span), budget),
        span,
    )?;
    if owner.source != span.source || span.start < owner.start || span.end > owner.end {
        return Err(Error::new(ErrorKind::SpanOutsideOwner, Some(span)));
    }
    Ok(())
}
fn matches(
    types: &VerifiedTypes,
    function: FunctionId,
    pattern: TypePattern,
    actual: Type,
    arguments: &[TypeUse],
    budget: &mut Budget,
) -> Result<bool, Error> {
    budget.charge(1)?;
    Ok(match (pattern, constructor(types, actual)) {
        (TypePattern::I64, _) => actual == Type::I64,
        (TypePattern::Bool, _) => actual == Type::Bool,
        (TypePattern::Argument(n), _) => {
            types.equivalent(arguments[n as usize].ty, actual) == Some(true)
        }
        (TypePattern::Builtin(b), Some(Constructor::Builtin(a))) => a == b,
        (TypePattern::ArenaOwner, Some(Constructor::ArenaOwner(owner))) => {
            owner.function == function
        }
        (TypePattern::ChannelSetup(t), Some(Constructor::ChannelSetup(a)))
        | (TypePattern::ServiceHandle(t), Some(Constructor::ServiceHandle(a)))
        | (TypePattern::Sender(t), Some(Constructor::Sender(a)))
        | (TypePattern::Receiver(t), Some(Constructor::Receiver(a)))
        | (TypePattern::SendResult(t), Some(Constructor::SendResult(a)))
        | (TypePattern::Option(t), Some(Constructor::Option(a)))
        | (TypePattern::Vec(t), Some(Constructor::Vec(a)))
        | (TypePattern::Shared(t), Some(Constructor::Shared(a)))
        | (TypePattern::Weak(t), Some(Constructor::Weak(a)))
        | (TypePattern::Arena(t), Some(Constructor::Arena(a))) => {
            matches(types, function, *t, a, arguments, budget)?
        }
        (TypePattern::Result(t, e), Some(Constructor::Result(a, b)))
        | (TypePattern::ServiceSetup(t, e), Some(Constructor::ServiceSetup(a, b)))
        | (TypePattern::ServiceOwner(t, e), Some(Constructor::ServiceOwner(a, b)))
        | (TypePattern::Map(t, e), Some(Constructor::Map(a, b))) => {
            matches(types, function, *t, a, arguments, budget)?
                && matches(types, function, *e, b, arguments, budget)?
        }
        (TypePattern::Loan(k, t), Some(Constructor::Loan(a, b))) => {
            a == k && matches(types, function, *t, b, arguments, budget)?
        }
        _ => false,
    })
}

/// Check a call against declarations and the independent MIR type graph.
/// No type is inferred or allocated, and no operation is executed.
pub fn check(
    declarations: &declarations::VerifiedDeclarations,
    function: FunctionId,
    call: &Call,
    arguments: &[Type],
    result: Type,
    span: Span,
    budget: &mut Budget,
) -> Result<CheckedCall, Error> {
    let types = declarations.types();
    let declaration = declarations
        .functions()
        .get(function.0 as usize)
        .ok_or_else(|| Error::new(ErrorKind::InvalidFunction(function), Some(span)))?;
    within(types, declaration.span, span, budget)?;
    let signature = call.operation.signature();
    if call.type_arguments.len() != signature.type_arguments.len() {
        return Err(Error::new(
            ErrorKind::CoreTypeArity {
                expected: signature.type_arguments.len(),
                actual: call.type_arguments.len(),
            },
            Some(span),
        ));
    }
    for (index, (argument, rule)) in call
        .type_arguments
        .iter()
        .zip(signature.type_arguments)
        .enumerate()
    {
        within(types, declaration.span, argument.span, budget)?;
        budget.charge(1)?;
        let p = types
            .properties(argument.ty)
            .ok_or_else(|| Error::new(ErrorKind::InvalidType(argument.ty), Some(argument.span)))?;
        let storable = !p.contains_loan && !p.contains_allocator && !p.contains_owner;
        let valid = match rule {
            TypeArgumentRule::Storable => storable,
            TypeArgumentRule::Send => storable && p.is_send(),
            TypeArgumentRule::SharedPayload => storable && !p.contains_arena,
            TypeArgumentRule::MapKey => {
                argument.ty == Type::I64
                    || constructor(types, argument.ty) == Some(Constructor::Builtin(Builtin::Text))
            }
        };
        if !valid {
            return Err(Error::new(
                ErrorKind::CoreTypeArgument {
                    index,
                    rule: *rule,
                    actual: argument.ty,
                },
                Some(argument.span),
            ));
        }
    }
    if call.arguments.len() != signature.parameters.len() {
        return Err(Error::new(
            ErrorKind::CallArity {
                expected: signature.parameters.len(),
                actual: call.arguments.len(),
            },
            Some(span),
        ));
    }
    if arguments.len() != signature.parameters.len() {
        return Err(Error::new(
            ErrorKind::CallArity {
                expected: signature.parameters.len(),
                actual: arguments.len(),
            },
            Some(span),
        ));
    }
    for (index, (pattern, actual)) in signature.parameters.iter().zip(arguments).enumerate() {
        if types.properties(*actual).is_none() {
            return Err(Error::new(ErrorKind::InvalidType(*actual), Some(span)));
        }
        if !matches(
            types,
            function,
            *pattern,
            *actual,
            &call.type_arguments,
            budget,
        )? {
            return Err(Error::new(
                ErrorKind::CoreType {
                    operation: call.operation,
                    position: Position::Argument(index),
                    actual: *actual,
                },
                Some(span),
            ));
        }
    }
    if types.properties(result).is_none() {
        return Err(Error::new(ErrorKind::InvalidType(result), Some(span)));
    }
    if !matches(
        types,
        function,
        signature.result,
        result,
        &call.type_arguments,
        budget,
    )? {
        return Err(Error::new(
            ErrorKind::CoreType {
                operation: call.operation,
                position: Position::Result,
                actual: result,
            },
            Some(span),
        ));
    }
    let needs_loan = call.loan_kind().is_some();
    if needs_loan != call.result_loan.is_some() {
        return Err(Error::new(
            ErrorKind::CoreLoan {
                operation: call.operation,
                expected: needs_loan,
                actual: call.result_loan.map(|l| l.0),
            },
            Some(span),
        ));
    }
    let arena_owner = if matches!(
        call.operation,
        Operation::ArenaNew | Operation::ArenaRead | Operation::ArenaReplace
    ) {
        let Some(Constructor::Loan(_, target)) = constructor(types, arguments[0]) else {
            unreachable!()
        };
        let Some(Constructor::ArenaOwner(owner)) = constructor(types, target) else {
            unreachable!()
        };
        Some(owner)
    } else {
        None
    };
    Ok(CheckedCall {
        result_binding: signature.result_binding,
        arena_owner,
    })
}
