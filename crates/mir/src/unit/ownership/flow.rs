//! Shared bounded traversal for independently checked ownership CFG stages.

use super::super::{Budget, Error};
use super::bodies;

pub(super) fn filled<T: Clone>(budget: &mut Budget, n: usize, value: T) -> Result<Vec<T>, Error> {
    let mut out = budget.vector(n)?;
    out.resize(n, value);
    Ok(out)
}

pub(super) struct Cycles {
    pub feedback: Vec<[bool; 2]>,
    pub order: Vec<usize>,
}

/// Input reachability and target bounds are already proved by TypedBodies.
/// Removing DFS feedback edges leaves a DAG, including irreducible cycles.
pub(super) fn cycles(body: &bodies::FunctionBody, budget: &mut Budget) -> Result<Cycles, Error> {
    let n = body.blocks.len();
    let mut color = filled(budget, n, 0u8)?;
    let mut feedback = filled(budget, n, [false; 2])?;
    let mut pending = budget.vector(n)?;
    let mut order = budget.vector(n)?;
    color[0] = 1;
    pending.push((0usize, 0usize));
    while let Some((block, next)) = pending.last_mut() {
        budget.charge(1)?;
        if let Some(target) = body.blocks[*block].terminator.kind.targets().nth(*next) {
            let edge = *next;
            *next += 1;
            let to = target.block.0 as usize;
            match color[to] {
                0 => {
                    color[to] = 1;
                    pending.push((to, 0));
                }
                1 => feedback[*block][edge] = true,
                _ => {}
            }
        } else {
            let node = *block;
            pending.pop();
            color[node] = 2;
            order.push(node);
        }
    }
    budget.charge(n)?;
    order.reverse();
    Ok(Cycles { feedback, order })
}
