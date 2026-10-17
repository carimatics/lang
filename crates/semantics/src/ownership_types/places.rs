//! Source-connected normalization of typed places. Explicit/automatic ordinary
//! loan dereferences become projection steps; no instruction or loan is emitted.

use super::*;
use lang_hir::program::{BodyTypes, FunctionFacts};
use lang_mir::unit::ownership::places as p;

/// Preserve expression indices as type/source slots, including temporary loan
/// values used by explicit dereference. Evaluation/definitions remain unlowered.
pub fn lower(
    checked: &RegionCheckedBodies,
    sources: &SourceMap,
    limits: p::Limits,
) -> Result<p::VerifiedPlaces> {
    let (mut c, out) = construct(checked, sources, limits)?;
    Ok(out.verify_with_budget(&mut c.budget, limits.ownership.constructed, limits.places)?)
}

pub(super) fn construct<'a>(
    checked: &RegionCheckedBodies,
    sources: &'a SourceMap,
    limits: p::Limits,
) -> Result<(Context<'a>, p::PlaceTable)> {
    let bodies = checked.loaned().moved().bodies();
    let (mut c, declarations) = construct_declarations(checked, sources, limits.ownership)?;
    let mut out = p::PlaceTable {
        declarations,
        functions: c.budget.vector(bodies.source().functions.len())?,
    };
    let mut registers = 0usize;
    let mut places = 0usize;
    for (index, function) in bodies.source().functions.iter().enumerate() {
        let facts = &bodies.functions()[index];
        registers = registers
            .checked_add(function.locals.len())
            .and_then(|n| n.checked_add(function.expressions.len()))
            .ok_or(Error::Capacity)?;
        cap(
            registers,
            limits.ownership.unit.registers,
            m::LimitKind::Registers,
        )?;
        let count = function
            .expressions
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    r::ExpressionKind::Local(_)
                        | r::ExpressionKind::Field { .. }
                        | r::ExpressionKind::Dereference(_)
                )
            })
            .count();
        c.budget.charge(function.expressions.len())?;
        places = places.checked_add(count).ok_or(Error::Capacity)?;
        if places > limits.places {
            return Err(o::Error {
                kind: o::ErrorKind::PlaceLimit {
                    requested: places,
                    limit: limits.places,
                },
                span: Some(c.span(function.span)?),
            }
            .into());
        }
        let mut f = p::FunctionPlaces {
            values: c.budget.vector(function.expressions.len())?,
            places: c.budget.vector(count)?,
        };
        for (expression, e) in function.expressions.iter().enumerate() {
            c.budget.charge(1)?;
            let t =
                facts
                    .expressions()
                    .get(expression)
                    .copied()
                    .ok_or(Error::MissingExpression {
                        function: index,
                        expression,
                    })?;
            f.values.push(o::TypeUse {
                ty: ty(t)?,
                span: c.span(e.span)?,
            });
            if matches!(
                e.kind,
                r::ExpressionKind::Local(_)
                    | r::ExpressionKind::Field { .. }
                    | r::ExpressionKind::Dereference(_)
            ) {
                f.places.push(normalize(
                    &mut c,
                    bodies,
                    index,
                    facts,
                    r::ExpressionId(expression),
                    limits.ownership.unit.depth,
                )?);
            }
        }
        out.functions.push(f);
    }
    Ok((c, out))
}

fn project(
    c: &mut Context<'_>,
    out: &mut Vec<p::Projection>,
    kind: p::ProjectionKind,
    span: Span,
    depth: usize,
) -> Result<()> {
    cap(
        out.len().checked_add(1).ok_or(Error::Capacity)?,
        depth,
        m::LimitKind::Depth,
    )?;
    c.budget.charge(1)?;
    if out.len() == out.capacity() {
        let next = out
            .capacity()
            .max(2)
            .checked_mul(2)
            .ok_or(Error::Capacity)?;
        c.budget.allocate::<p::Projection>(next)?;
        out.try_reserve_exact(next - out.len())
            .map_err(|_| Error::Capacity)?;
    }
    out.push(p::Projection {
        kind,
        span: c.span(span)?,
    });
    Ok(())
}

fn normalize(
    c: &mut Context<'_>,
    bodies: &BodyTypes,
    function: usize,
    facts: &FunctionFacts,
    origin: r::ExpressionId,
    depth: usize,
) -> Result<p::Place> {
    let f = &bodies.source().functions[function];
    let mut current = origin;
    let mut projections = Vec::new();
    let root = loop {
        c.budget.charge(1)?;
        let e = f
            .expressions
            .get(current.0)
            .ok_or(Error::MissingExpression {
                function,
                expression: current.0,
            })?;
        match &e.kind {
            r::ExpressionKind::Local(local) => {
                break p::Root::Local(o::declarations::LocalId(id(local.0)?));
            }
            r::ExpressionKind::Dereference(value) => {
                project(
                    c,
                    &mut projections,
                    p::ProjectionKind::Dereference,
                    e.span,
                    depth,
                )?;
                current = *value;
            }
            r::ExpressionKind::Field { receiver, name } => {
                let field = facts.field_index(current).ok_or(Error::MissingField {
                    function,
                    expression: current.0,
                })?;
                project(
                    c,
                    &mut projections,
                    p::ProjectionKind::Field(id(field)?),
                    name.span,
                    depth,
                )?;
                let t = facts.expressions().get(receiver.0).copied().ok_or(
                    Error::MissingExpression {
                        function,
                        expression: receiver.0,
                    },
                )?;
                if matches!(t,r::Type::Constructed(t) if matches!(bodies.types()[t.0].constructor,r::TypeConstructor::Loan(..)))
                {
                    project(
                        c,
                        &mut projections,
                        p::ProjectionKind::Dereference,
                        f.expressions[receiver.0].span,
                        depth,
                    )?;
                }
                current = *receiver;
            }
            _ => break p::Root::Value(m::Register(id(current.0)?)),
        }
    };
    c.budget.charge(projections.len())?;
    projections.reverse();
    Ok(p::Place {
        origin: m::Register(id(origin.0)?),
        root,
        projections,
        span: c.span(f.expressions[origin.0].span)?,
    })
}
