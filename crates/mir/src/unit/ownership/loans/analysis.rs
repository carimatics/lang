use super::*;
use crate::unit::ownership::flow;
use check::{Context, Mode};

/// Check reference-cell provenance, dependent lifetimes, and whole-owner loan
/// exclusion. Region effects, cleanup/layouts, and execution remain separate.
pub fn analyze(
    activity: loan_activity::ActivityBodies,
    limits: Limits,
) -> Result<LoanCheckedBodies, Error> {
    let mut budget = Budget::with_usage(
        m::Limits {
            work: limits.work,
            allocations: limits.allocations,
            elements: limits.elements,
            depth: limits.depth,
            ..Default::default()
        },
        m::Usage {
            work: activity.work_used(),
            allocation_bytes: activity.requested_bytes(),
            elements: activity.elements_used(),
        },
    )?;
    analyze_in(activity, limits, &mut budget)
}

pub(crate) fn analyze_in(
    activity: loan_activity::ActivityBodies,
    limits: Limits,
    budget: &mut Budget,
) -> Result<LoanCheckedBodies, Error> {
    let typed = activity.moved().typed();
    let mut functions = budget.vector(typed.functions().len())?;
    let (mut nodes, mut state_words) = (0usize, 0usize);
    for index in 0..typed.functions().len() {
        let span = typed.places().declarations().functions()[index].span;
        functions.push(
            function(typed, index, limits, &mut nodes, &mut state_words, budget).map_err(
                |mut e| {
                    if e.primary.is_none() {
                        e.primary = Some(span);
                    }
                    e
                },
            )?,
        );
    }
    Ok(LoanCheckedBodies {
        activity,
        functions,
        work: budget.work_used(),
        bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    })
}

fn function(
    typed: &b::TypedBodies,
    index: usize,
    limits: Limits,
    nodes: &mut usize,
    words: &mut usize,
    budget: &mut Budget,
) -> Result<FunctionLoans, Error> {
    let body = &typed.functions()[index];
    let topology = topology::Topology::build(typed, index, limits, nodes, budget)?;
    let layout = state::Layout::new(&topology, body.prepared)?;
    let (width, blocks) = (layout.words, body.blocks.len());
    let cells = mul(width, blocks)?;
    *words = add(*words, mul(cells, 2)?)?;
    if *words > limits.state_words {
        return Err(Error {
            kind: ErrorKind::StateWords {
                requested: *words,
                limit: limits.state_words,
            },
            primary: None,
            secondary: None,
        });
    }
    let graph = flow::cycles(body, budget)?;
    let mut baseline = flow::filled(budget, cells, 0u64)?;
    for parameter in &topology.parameters {
        budget.charge(3)?;
        layout.activity(&mut baseline[..width], parameter.loan, true);
        state::set(
            &mut baseline[..width],
            layout.refs(parameter.leaf),
            parameter.loan,
            true,
        );
        state::set(
            &mut baseline[..width],
            layout.targets(parameter.loan),
            parameter.target,
            true,
        );
    }
    let mut found = flow::filled(budget, blocks, false)?;
    found[0] = true;
    let mut row = flow::filled(budget, width, 0u64)?;
    let mut edge = flow::filled(budget, width, 0u64)?;
    let mut c = Context {
        typed,
        function: index,
        topology: &topology,
        layout: &layout,
        budget,
    };
    for node in &graph.order {
        let node = *node;
        state::copy(
            c.budget,
            &mut row,
            &baseline[node * width..(node + 1) * width],
        )?;
        c.block(&mut row, &body.blocks[node], Mode::Propagate)?;
        for (position, target) in body.blocks[node].terminator.kind.targets().enumerate() {
            c.budget.charge(1)?;
            if graph.feedback[node][position] {
                continue;
            }
            state::copy(c.budget, &mut edge, &row)?;
            c.edge(
                &mut edge,
                target,
                body.blocks[node].terminator.span,
                Mode::Propagate,
            )?;
            let to = target.block.0 as usize;
            let destination = &mut baseline[to * width..(to + 1) * width];
            if found[to] {
                layout.merge(c.budget, destination, &edge)?;
            } else {
                state::copy(c.budget, destination, &edge)?;
                found[to] = true;
            }
        }
    }
    let mut entries = c.budget.vector(cells)?;
    c.budget.charge(cells)?;
    entries.extend_from_slice(&baseline);
    let mut queue = flow::filled(c.budget, blocks, 0usize)?;
    let mut queued = flow::filled(c.budget, blocks, true)?;
    for (i, node) in graph.order.iter().copied().enumerate() {
        c.budget.charge(1)?;
        queue[i] = node;
    }
    let (mut head, mut tail, mut pending) = (0usize, 0usize, blocks);
    while pending > 0 {
        c.budget.charge(1)?;
        let node = queue[head];
        head = (head + 1) % blocks;
        pending -= 1;
        queued[node] = false;
        state::copy(
            c.budget,
            &mut row,
            &entries[node * width..(node + 1) * width],
        )?;
        c.block(&mut row, &body.blocks[node], Mode::Propagate)?;
        for target in body.blocks[node].terminator.kind.targets() {
            state::copy(c.budget, &mut edge, &row)?;
            c.edge(
                &mut edge,
                target,
                body.blocks[node].terminator.span,
                Mode::Propagate,
            )?;
            let to = target.block.0 as usize;
            if layout.merge(c.budget, &mut entries[to * width..(to + 1) * width], &edge)?
                && !queued[to]
            {
                queue[tail] = to;
                tail = (tail + 1) % blocks;
                pending += 1;
                queued[to] = true;
            }
        }
    }
    for (node, block) in body.blocks.iter().enumerate() {
        state::copy(
            c.budget,
            &mut row,
            &entries[node * width..(node + 1) * width],
        )?;
        c.block(&mut row, block, Mode::Propagate)?;
        for (position, target) in block.terminator.kind.targets().enumerate() {
            c.budget.charge(1)?;
            if !graph.feedback[node][position] {
                continue;
            }
            state::copy(c.budget, &mut edge, &row)?;
            c.edge(&mut edge, target, block.terminator.span, Mode::Propagate)?;
            let to = target.block.0 as usize;
            let expected = &baseline[to * width..(to + 1) * width];
            restore(&mut c, expected, &edge, target.block, block.terminator.span)?;
        }
    }
    for (node, block) in body.blocks.iter().enumerate() {
        state::copy(
            c.budget,
            &mut row,
            &entries[node * width..(node + 1) * width],
        )?;
        c.block(&mut row, block, Mode::Check)?;
        for target in block.terminator.kind.targets() {
            state::copy(c.budget, &mut edge, &row)?;
            c.edge(&mut edge, target, block.terminator.span, Mode::Check)?;
        }
    }
    Ok(FunctionLoans {
        topology,
        layout,
        blocks,
        entries,
    })
}

fn dominates_entry(
    facts: &b::FunctionFacts,
    site: b::Site,
    mut block: u32,
    budget: &mut Budget,
) -> Result<bool, m::Error> {
    if site.block.0 == block {
        return Ok(site.order == 0);
    }
    loop {
        budget.charge(1)?;
        if site.block.0 == block {
            return Ok(true);
        }
        if block == 0 {
            return Ok(false);
        }
        block = facts.immediate_dominators()[block as usize];
    }
}
fn restore(
    c: &mut Context<'_, '_>,
    expected: &[u64],
    actual: &[u64],
    header: m::BlockId,
    span: Span,
) -> Result<(), Error> {
    let fail = |owner| Error {
        kind: ErrorKind::LoopState { header, owner },
        primary: Some(span),
        secondary: Some(c.typed.functions()[c.function].blocks[header.0 as usize].span),
    };
    for (root, node) in c.topology.roots.iter().copied().enumerate() {
        c.budget.charge(1)?;
        if root >= c.topology.locals {
            let Some(site) = c.typed.facts()[c.function].definitions()[root - c.topology.locals]
            else {
                continue;
            };
            if !dominates_entry(&c.typed.facts()[c.function], site, header.0, c.budget)? {
                continue;
            }
        }
        for leaf in c.topology.cells[node].leaves.clone() {
            c.budget.charge(c.layout.loan_words + 1)?;
            if expected[c.layout.refs(leaf)] != actual[c.layout.refs(leaf)]
                || state::has(expected, c.layout.dead.clone(), leaf)
                    != state::has(actual, c.layout.dead.clone(), leaf)
            {
                return Err(fail(Some(c.topology.cells[node].owner)));
            }
        }
    }
    for id in 0..c.layout.loans {
        c.budget
            .charge(c.layout.node_words + c.layout.loan_words + 1)?;
        if c.layout.active(expected, id)
            && (expected[c.layout.targets(id)] != actual[c.layout.targets(id)]
                || expected[c.layout.deps(id)] != actual[c.layout.deps(id)])
        {
            return Err(fail(None));
        }
    }
    for id in 0..c.typed.functions()[c.function].prepared {
        c.budget
            .charge(c.layout.node_words + c.layout.loan_words + 1)?;
        if expected[c.layout.capture(id)] != actual[c.layout.capture(id)]
            || expected[c.layout.capture_deps(id)] != actual[c.layout.capture_deps(id)]
            || state::has(expected, c.layout.capture_dead.clone(), id)
                != state::has(actual, c.layout.capture_dead.clone(), id)
        {
            return Err(fail(None));
        }
    }
    Ok(())
}
