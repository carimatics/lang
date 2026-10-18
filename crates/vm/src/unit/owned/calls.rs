#![allow(
    clippy::result_large_err,
    reason = "Keep the completed failure report inline after allocation failure."
)]
use super::*;

#[derive(Clone, Copy)]
struct ReturnTo {
    cell: usize,
    slot: c::Slot,
    next: f::NodeId,
    failure: f::NodeId,
    span: m::Span,
}
#[derive(Clone, Copy)]
struct Activation {
    address: Address,
    node: f::NodeId,
    returned: Option<ReturnTo>,
}
fn base_usage(layouts: &l::Layouts<'_>, typed: &b::TypedBodies) -> Result<m::Usage, l::Error> {
    let types = typed.places().declarations().types();
    let add = |a: usize, b: usize, c: usize| {
        b.checked_sub(c)
            .and_then(|extra| a.checked_add(extra))
            .ok_or(l::Error::Overflow)
    };
    Ok(m::Usage {
        work: add(layouts.usage().work, typed.work_used(), types.work_used())?,
        allocation_bytes: add(
            layouts.usage().allocation_bytes,
            typed.requested_bytes(),
            types.requested_bytes(),
        )?,
        elements: add(
            layouts.usage().elements,
            typed.elements_used(),
            types.elements_used(),
        )?,
    })
}
#[derive(Clone, Copy)]
struct PoolShape {
    cells: usize,
    loans: usize,
    prepared: usize,
    arena_owners: usize,
}
fn pool_usage(
    limits: m::Limits,
    initial: m::Usage,
    capacity: usize,
    shape: PoolShape,
    layouts: &l::Layouts<'_>,
) -> Result<m::Usage, l::Error> {
    let PoolShape {
        cells,
        loans,
        prepared,
        arena_owners,
    } = shape;
    let mut budget = m::Budget::with_usage(limits, initial)?;
    let cell_count = cells.checked_mul(capacity).ok_or(l::Error::Overflow)?;
    let loan_count = loans.checked_mul(capacity).ok_or(l::Error::Overflow)?;
    let prepared_count = prepared.checked_mul(capacity).ok_or(l::Error::Overflow)?;
    budget.allocate::<l::FrameStorage<'_, '_>>(capacity)?;
    budget.allocate::<bool>(capacity)?;
    budget.allocate::<state::Cell>(cell_count)?;
    budget.allocate::<usize>(cell_count)?;
    budget.allocate::<state::Loan>(loan_count)?;
    budget.allocate::<Option<state::Capture>>(prepared_count)?;
    budget.allocate::<Activation>(capacity)?;
    budget.allocate::<super::super::StackFrame>(capacity)?;
    budget.charge(cell_count)?;
    budget.allocate::<l::Reservation<'_, '_>>(cell_count)?;
    if arena_owners != 0 {
        budget.allocate::<Option<g::Arena<'_, '_>>>(cell_count)?;
        for _ in 0..arena_owners
            .checked_mul(capacity)
            .ok_or(l::Error::Overflow)?
        {
            let usage = g::Arena::preparation_usage(
                layouts,
                Default::default(),
                limits,
                budget_usage(&budget),
            )
            .map_err(|e| match e.kind {
                g::Kind::Layout(error) => error,
                _ => l::Error::State,
            })?;
            budget = m::Budget::with_usage(limits, usage)?;
        }
    }
    Ok(budget_usage(&budget))
}
fn capture(trace: &mut Vec<super::super::StackFrame>, stack: &[Activation], span: m::Span) {
    if !trace.is_empty() {
        return;
    }
    for (i, activation) in stack.iter().enumerate().rev() {
        trace.push(super::super::StackFrame {
            function: activation.address.function,
            span: Some(if i + 1 == stack.len() {
                span
            } else {
                stack[i + 1].returned.unwrap().span
            }),
        });
    }
}
fn declare<P: a::Provider, C: c::Cleaner<Ticket>>(
    typed: &b::TypedBodies,
    plan: &l::FramePlan<'_, '_>,
    shared: &Rc<RefCell<State<'_, '_, '_, P>>>,
    frame: &mut c::Frame<'_, Ticket, C>,
    address: Address,
) -> Step<()> {
    let index = address.function.0 as usize;
    let ordinary = plan.return_cell();
    {
        let mut state = shared.borrow_mut();
        state.current = address;
        for i in 0..plan.cell_count() {
            state.cells[address.cell + i] = state::Cell {
                physical: address.cell + i,
                ty: plan.cell_type(i).unwrap().ty,
                live: false,
                slot: (i < ordinary).then_some(address.slot(address.cell + i)),
            };
            state.inverse[address.cell + i] = address.cell + i;
        }
        state.prepared[address.prepared..address.prepared + typed.functions()[index].prepared]
            .fill(None);
    }
    for i in 0..ordinary {
        if !shared.borrow().is_copy(address.cell + i) {
            frame
                .declare(c::Slot(address.slot + i), c::Kind::Owner)
                .map_err(invariant)?;
        }
    }
    for i in 0..typed.functions()[index].loans {
        frame
            .declare(c::Slot(address.slot + ordinary + i), c::Kind::Borrow)
            .map_err(invariant)?;
    }
    for parameter in typed.functions()[index]
        .blocks
        .iter()
        .flat_map(|b| &b.parameters)
    {
        if !shared
            .borrow()
            .is_copy(address.cell + plan.scratch_cell(*parameter).unwrap())
        {
            frame
                .declare(
                    c::Slot(address.slot + plan.scratch_slot(*parameter).unwrap()),
                    c::Kind::Owner,
                )
                .map_err(invariant)?;
        }
    }
    Ok(())
}
fn relocate<P: a::Provider, C: c::Cleaner<Ticket>>(
    shared: &Rc<RefCell<State<'_, '_, '_, P>>>,
    frame: &mut c::Frame<'_, Ticket, C>,
    from: usize,
    to: usize,
    source_slot: c::Slot,
    target_slot: c::Slot,
) -> Step<()> {
    if shared.borrow().is_copy(from) {
        let mut state = shared.borrow_mut();
        let root = state.root(from)?;
        state.copy(root, to)
    } else {
        frame.value(source_slot).map_err(invariant)?;
        shared.borrow_mut().relocate(from, to)?;
        let physical = shared.borrow().cells[to].physical;
        *frame.value_mut(source_slot).map_err(invariant)? = Ticket::Cell(physical);
        frame
            .move_value(source_slot, target_slot)
            .map_err(invariant)
    }
}

enum Selection {
    Script,
    Preview {
        function: m::FunctionId,
        bytes: usize,
    },
}
enum Value {
    Scalar(i64),
    Preview(String),
}
struct Execution {
    value: Value,
    usage: Usage,
    allocations: a::Stats,
}
pub(super) fn execute<P: a::Provider>(
    flow: &f::Elaborated,
    instance: &mut c::Instance,
    allocator: Option<&mut a::Allocator<P>>,
    control: Control<'_>,
) -> Result<Outcome, Failure> {
    let Execution {
        value,
        usage,
        allocations,
    } = execute_with(flow, instance, allocator, control, Selection::Script)?;
    let Value::Scalar(value) = value else {
        return Err(rejected(Reason::Entry, None, None));
    };
    Ok(Outcome {
        value,
        usage,
        allocations,
    })
}
pub(super) fn preview<P: a::Provider>(
    flow: &f::Elaborated,
    function: m::FunctionId,
    instance: &mut c::Instance,
    allocator: Option<&mut a::Allocator<P>>,
    control: Control<'_>,
    bytes: usize,
) -> Result<PreviewOutcome, Failure> {
    let Execution {
        value,
        usage,
        allocations,
    } = execute_with(
        flow,
        instance,
        allocator,
        control,
        Selection::Preview { function, bytes },
    )?;
    let Value::Preview(value) = value else {
        return Err(rejected(Reason::Entry, None, None));
    };
    Ok(PreviewOutcome {
        value,
        usage,
        allocations,
    })
}
fn execute_with<P: a::Provider>(
    flow: &f::Elaborated,
    instance: &mut c::Instance,
    allocator: Option<&mut a::Allocator<P>>,
    control: Control<'_>,
    selection: Selection,
) -> Result<Execution, Failure> {
    if instance.is_poisoned() {
        return Err(rejected(Reason::Poisoned, None, None));
    }
    control
        .check()
        .map_err(|e| rejected(Reason::Control(e), None, None))?;
    let typed = flow.source().activity().moved().typed();
    let declarations = typed.places().declarations();
    let types = declarations.types();
    let entry = match selection {
        Selection::Script => declarations
            .functions()
            .iter()
            .position(|f| f.name == "main"),
        Selection::Preview { function, .. } => {
            ((function.0 as usize) < declarations.functions().len()).then_some(function.0 as usize)
        }
    }
    .ok_or_else(|| rejected(Reason::Entry, None, None))?;
    let function = m::FunctionId(entry as u32);
    let signature = declarations.functions()[entry].signature_span;
    let fail = |reason| rejected(reason, Some(function), Some(signature));
    if !declarations.functions()[entry].parameters.is_empty()
        || (matches!(selection, Selection::Script)
            && declarations.functions()[entry].result.ty != o::Type::I64)
    {
        return Err(fail(Reason::Entry));
    }
    if control.limits.frames == Some(0) {
        return Err(fail(Reason::Control(super::super::Reason::FrameLimit)));
    }
    let limits = m::Limits::default();
    let payloads =
        profile::Payloads::compile(types, limits).map_err(|e| fail(Reason::Preparation(e)))?;
    let layouts = l::Layouts::compile(types, limits).map_err(|e| fail(Reason::Preparation(e)))?;
    let mut initial = base_usage(&layouts, typed).map_err(|e| fail(Reason::Preparation(e)))?;
    let extra = |a: usize, b: usize, c: usize| a.checked_add(b.checked_sub(c)?);
    initial.work = extra(initial.work, flow.work_used(), typed.work_used())
        .and_then(|n| n.checked_add(payloads.usage.work))
        .ok_or_else(|| fail(Reason::Preparation(l::Error::Overflow)))?;
    initial.allocation_bytes = extra(
        initial.allocation_bytes,
        flow.requested_bytes(),
        typed.requested_bytes(),
    )
    .and_then(|n| n.checked_add(payloads.usage.allocation_bytes))
    .ok_or_else(|| fail(Reason::Preparation(l::Error::Overflow)))?;
    initial.elements = extra(
        initial.elements,
        flow.elements_used(),
        typed.elements_used(),
    )
    .and_then(|n| n.checked_add(payloads.usage.elements))
    .ok_or_else(|| fail(Reason::Preparation(l::Error::Overflow)))?;
    let mut budget =
        m::Budget::with_usage(limits, initial).map_err(|e| fail(Reason::Preparation(e.into())))?;
    let count = typed.functions().len();
    let mut reachable = budget
        .vector(count)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut seen = budget
        .vector(count)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    seen.resize(count, false);
    reachable.push(entry);
    seen[entry] = true;
    let mut cursor = 0;
    let mut edges = 0usize;
    while cursor < reachable.len() {
        let current = reachable[cursor];
        cursor += 1;
        for block in &typed.functions()[current].blocks {
            for instruction in &block.instructions {
                budget
                    .charge(1)
                    .map_err(|e| fail(Reason::Preparation(e.into())))?;
                if let b::Op::Call {
                    function: callee, ..
                } = instruction.op
                {
                    let id = callee.0 as usize;
                    edges = edges
                        .checked_add(1)
                        .ok_or_else(|| fail(Reason::Preparation(l::Error::Overflow)))?;
                    if !seen[id] {
                        seen[id] = true;
                        reachable.push(id);
                    }
                } else {
                    if !supported(
                        instruction,
                        &payloads,
                        &typed.places().functions()[current].values,
                        types,
                    ) {
                        let core = if let b::Op::CoreCall(c) = &instruction.op {
                            Some(c.operation)
                        } else {
                            None
                        };
                        return Err(rejected(
                            Reason::Unsupported { core },
                            Some(m::FunctionId(current as u32)),
                            Some(instruction.span),
                        ));
                    }
                }
            }
        }
    }
    let allocator = allocator.ok_or_else(|| fail(Reason::MissingAllocator))?;
    a::Allocator::preflight(Some(allocator)).map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut states = budget
        .vector(count)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    states.resize(count, 0u8);
    let mut depths = budget
        .vector(count)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    depths.resize(count, 0usize);
    let mut pending = budget
        .vector(
            count
                .checked_add(edges)
                .ok_or_else(|| fail(Reason::Preparation(l::Error::Overflow)))?,
        )
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    pending.push((entry, false));
    let mut recursive = false;
    while let Some((id, expanded)) = pending.pop() {
        budget
            .charge(1)
            .map_err(|e| fail(Reason::Preparation(e.into())))?;
        if states[id] == 2 {
            continue;
        }
        let children = || {
            typed.functions()[id]
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter_map(|i| {
                    if let b::Op::Call { function, .. } = i.op {
                        Some(function.0 as usize)
                    } else {
                        None
                    }
                })
        };
        if expanded {
            depths[id] = 1 + children().map(|i| depths[i]).max().unwrap_or(0);
            states[id] = 2;
        } else if states[id] == 1 {
            recursive = true;
        } else {
            states[id] = 1;
            pending.push((id, true));
            for child in children() {
                pending.push((child, false));
            }
        }
    }
    let mut plans = budget
        .vector(count)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    plans.resize_with(count, || None);
    for &id in &reachable {
        let plan = l::FramePlan::compile_after(
            &layouts,
            typed,
            m::FunctionId(id as u32),
            limits,
            usize::MAX,
            budget_usage(&budget),
        )
        .map_err(|e| fail(Reason::Preparation(e)))?;
        budget = m::Budget::with_usage(limits, plan.usage())
            .map_err(|e| fail(Reason::Preparation(e.into())))?;
        plans[id] = Some(plan);
    }
    if let Some(limit) = control.limits.live_cells {
        let cells = plans[entry].as_ref().unwrap().cleanup_slots() + 1;
        if cells > limit {
            return Err(fail(Reason::Preparation(l::Error::CellLimit {
                cells,
                limit,
            })));
        }
    }
    let mut preview = if let Selection::Preview { bytes, .. } = selection {
        let prepared = l::Preview::prepare(
            &layouts,
            declarations.functions()[entry].result.ty,
            bytes,
            limits,
            budget_usage(&budget),
        )
        .map_err(|error| {
            fail(if matches!(error, l::Error::InvalidType(_)) {
                Reason::Unsupported { core: None }
            } else {
                Reason::Preparation(error)
            })
        })?;
        budget = m::Budget::with_usage(limits, prepared.usage())
            .map_err(|error| fail(Reason::Preparation(error.into())))?;
        Some(prepared)
    } else {
        None
    };
    let maximum = *reachable
        .iter()
        .max_by_key(|id| plans[**id].as_ref().unwrap().cell_count())
        .unwrap();
    let cell_stride = plans[maximum].as_ref().unwrap().cell_count();
    let slot_stride = reachable
        .iter()
        .map(|id| plans[*id].as_ref().unwrap().cleanup_slots())
        .max()
        .unwrap()
        .max(1);
    let loan_stride = reachable
        .iter()
        .map(|id| typed.functions()[*id].loans)
        .max()
        .unwrap()
        .max(1);
    let prepared_stride = reachable
        .iter()
        .map(|id| typed.functions()[*id].prepared)
        .max()
        .unwrap();
    let mut owner_positions = budget
        .vector(cell_stride)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    owner_positions.resize(cell_stride, false);
    for &id in &reachable {
        for (index, local) in declarations.functions()[id].locals.iter().enumerate() {
            if local.kind == o::declarations::LocalKind::ArenaOwner {
                owner_positions[index] = true;
            }
        }
    }
    let arena_owners = owner_positions.iter().filter(|p| **p).count();
    let shape = PoolShape {
        cells: cell_stride,
        loans: loan_stride,
        prepared: prepared_stride,
        arena_owners,
    };
    // A disabled frame counter does not disable preparation budgets. Find the
    // exact largest pool those budgets admit without allocating trial buffers.
    let desired = control
        .limits
        .frames
        .unwrap_or(limits.elements)
        .min(limits.elements)
        .min(if recursive { usize::MAX } else { depths[entry] });
    let mut capacity = 0;
    let mut upper = desired;
    while capacity < upper {
        let middle = capacity + (upper - capacity).div_ceil(2);
        if pool_usage(limits, budget_usage(&budget), middle, shape, &layouts).is_ok() {
            capacity = middle;
        } else {
            upper = middle - 1;
        }
    }
    if capacity == 0 {
        pool_usage(limits, budget_usage(&budget), 1, shape, &layouts)
            .map_err(|e| fail(Reason::Preparation(e)))?;
        return Err(fail(Reason::Preparation(l::Error::State)));
    }
    let multiply = |n: usize| {
        n.checked_mul(capacity)
            .ok_or_else(|| fail(Reason::Preparation(l::Error::Overflow)))
    };
    let mut pools = budget
        .vector(capacity)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut active = budget
        .vector(capacity)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    active.resize(capacity, false);
    let mut cells = budget
        .vector(multiply(cell_stride)?)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut inverse = budget
        .vector(multiply(cell_stride)?)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    for i in 0..multiply(cell_stride)? {
        cells.push(state::Cell {
            physical: i,
            ty: o::Type::I64,
            live: false,
            slot: None,
        });
        inverse.push(i);
    }
    let mut loans = budget
        .vector(multiply(loan_stride)?)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    for _ in 0..multiply(loan_stride)? {
        loans.push(state::Loan {
            generation: 0,
            target: None,
            arena_read: None,
        });
    }
    let mut prepared = budget
        .vector(multiply(prepared_stride)?)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    prepared.resize(multiply(prepared_stride)?, None);
    let mut stack = budget
        .vector(capacity)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut trace = budget
        .vector(capacity)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    for _ in 0..capacity {
        let pool = plans[maximum]
            .as_ref()
            .unwrap()
            .prepare_pool_after(budget_usage(&budget))
            .map_err(|e| fail(Reason::Preparation(e)))?;
        budget = m::Budget::with_usage(limits, pool.usage())
            .map_err(|e| fail(Reason::Preparation(e.into())))?;
        pools.push(pool);
    }
    let arena_cells = if arena_owners == 0 {
        0
    } else {
        multiply(cell_stride)?
    };
    let mut arenas = budget
        .vector(arena_cells)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    for physical in 0..arena_cells {
        let local = physical % cell_stride;
        let owner = owner_positions[local];
        let arena = if owner {
            let arena =
                g::Arena::prepare(&layouts, Default::default(), limits, budget_usage(&budget))
                    .map_err(|error| {
                        fail(Reason::Preparation(match error.kind {
                            g::Kind::Layout(error) => error,
                            _ => l::Error::State,
                        }))
                    })?;
            budget = m::Budget::with_usage(limits, arena.usage())
                .map_err(|e| fail(Reason::Preparation(e.into())))?;
            Some(arena)
        } else {
            None
        };
        arenas.push(arena);
    }
    let shared = Rc::new(RefCell::new(State {
        allocator,
        storage: Some(StackStorage {
            frames: pools,
            active,
            stride: cell_stride,
        }),
        types,
        cells,
        inverse,
        loans,
        prepared,
        arenas,
        panic_code: None,
        current: Address::entry(function),
    }));
    let cleaner_state = Rc::clone(&shared);
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: multiply(slot_stride)?,
                scopes: capacity,
                ..Default::default()
            },
            move |ticket: &mut Ticket, _, context: &mut c::Context<'_>| {
                cleaner_state.borrow_mut().cleanup(ticket, context)
            },
        )
        .map_err(|e| fail(Reason::Enter(e)))?;
    let entry_plan = plans[entry].as_ref().unwrap();
    let admitted = {
        let mut state = shared.borrow_mut();
        let State {
            storage, allocator, ..
        } = &mut *state;
        storage.as_mut().unwrap().admit(0, entry_plan, allocator)
    };
    if let Err(error) = admitted {
        let completion = frame.finish(c::Exit::Failure {
            failure: c::ExecutionFailure::AllocationLimit,
            span: Some(signature),
        });
        let mut error = fail(Reason::Preparation(error));
        error.report = Some(completion.report);
        return Err(error);
    }
    declare(
        typed,
        entry_plan,
        &shared,
        &mut frame,
        Address::entry(function),
    )
    .map_err(|_| {
        fail(Reason::Execution(c::FailureKind::Execution(
            c::ExecutionFailure::Invariant,
        )))
    })?;
    stack.push(Activation {
        address: Address::entry(function),
        node: flow.functions()[entry].entry(),
        returned: None,
    });
    let mut live_cells = entry_plan.cleanup_slots() + 1;
    let mut usage = Usage {
        peak_frames: 1,
        peak_live_cells: live_cells,
        ..Default::default()
    };
    let mut cause = c::Cause::Execution(c::ExecutionFailure::Invariant);
    let mut fault = Some(signature);
    let mut failed_function = function;
    let mut result = None;
    let mut preview_error = None;
    let completion = loop {
        let activation = *stack.last().unwrap();
        let address = activation.address;
        shared.borrow_mut().current = address;
        let index = address.function.0 as usize;
        let plan = plans[index].as_ref().unwrap();
        let node = flow.functions()[index].nodes()[activation.node.0 as usize];
        let values = address.cell + declarations.functions()[index].locals.len();
        let mut failed = None;
        let next = match node.kind {
            f::Kind::Entry { next, .. } => next,
            f::Kind::Fuel { next, failure, .. } => {
                let checked = control.check().and_then(|()| {
                    if control.limits.fuel.is_some_and(|limit| usage.fuel >= limit) {
                        return Err(super::super::Reason::FuelExhausted);
                    }
                    usage.fuel = usage
                        .fuel
                        .checked_add(1)
                        .ok_or(super::super::Reason::CounterOverflow)?;
                    Ok(())
                });
                match checked {
                    Ok(()) => next,
                    Err(error) => {
                        failed = Some(execution_reason(error));
                        failure
                    }
                }
            }
            f::Kind::Execute {
                site,
                next,
                failure,
            } => {
                let instruction = &typed.functions()[index].blocks[site.block.0 as usize]
                    .instructions[site.order - 1];
                if let b::Op::Call {
                    function: callee,
                    arguments,
                } = &instruction.op
                {
                    let depth = stack.len();
                    let callee_plan = plans[callee.0 as usize].as_ref().unwrap();
                    let cells = live_cells.checked_add(callee_plan.cleanup_slots() + 1);
                    let checked = control.check().map_err(execution_reason).and_then(|()| {
                        if control.limits.frames.is_some_and(|n| depth >= n) {
                            return Err(c::ExecutionFailure::FrameLimit);
                        }
                        let cells = cells.ok_or(c::ExecutionFailure::CounterOverflow)?;
                        if control.limits.live_cells.is_some_and(|n| cells > n) {
                            return Err(c::ExecutionFailure::CellLimit);
                        }
                        if depth >= capacity {
                            return Err(c::ExecutionFailure::AllocationLimit);
                        }
                        Ok(cells)
                    });
                    let prepared = checked.and_then(|cells| {
                        let admitted = {
                            let mut state = shared.borrow_mut();
                            let State {
                                storage, allocator, ..
                            } = &mut *state;
                            storage
                                .as_mut()
                                .unwrap()
                                .admit(depth, callee_plan, allocator)
                        };
                        admitted.map_err(|_| c::ExecutionFailure::AllocationLimit)?;
                        frame.open_call().map_err(invariant)?;
                        let target_address = Address {
                            function: *callee,
                            cell: depth * cell_stride,
                            slot: depth * slot_stride,
                            loan: depth * loan_stride,
                            prepared: depth * prepared_stride,
                        };
                        declare(typed, callee_plan, &shared, &mut frame, target_address)?;
                        for (argument, parameter) in arguments
                            .iter()
                            .zip(&declarations.functions()[callee.0 as usize].parameters)
                        {
                            let from = values + argument.0 as usize;
                            let to = target_address.cell + parameter.0 as usize;
                            relocate(
                                &shared,
                                &mut frame,
                                from,
                                to,
                                address.slot(from),
                                target_address.slot(to),
                            )?;
                        }
                        let destination = values + instruction.results[0].0 as usize;
                        stack.push(Activation {
                            address: target_address,
                            node: flow.functions()[callee.0 as usize].entry(),
                            returned: Some(ReturnTo {
                                cell: destination,
                                slot: address.slot(destination),
                                next,
                                failure,
                                span: node.span,
                            }),
                        });
                        Ok(cells)
                    });
                    match prepared {
                        Ok(cells) => {
                            live_cells = cells;
                            usage.peak_frames = usage.peak_frames.max(stack.len());
                            usage.peak_live_cells = usage.peak_live_cells.max(cells);
                            continue;
                        }
                        Err(error) => {
                            failed = Some(error);
                            failure
                        }
                    }
                } else {
                    let mapping = c::MirLayout::new(typed, address.function, address.slot).unwrap();
                    match super::instruction(
                        typed,
                        address.function,
                        site,
                        &shared,
                        &mut frame,
                        &mapping,
                    ) {
                        Ok(()) => next,
                        Err(error) => {
                            failed = Some(error);
                            failure
                        }
                    }
                }
            }
            f::Kind::Transfer {
                block,
                arm,
                target,
                failure,
            } => match transfer_edge(
                typed,
                address.function,
                block,
                arm,
                plan,
                &shared,
                &mut frame,
            ) {
                Ok(()) => target,
                Err(error) => {
                    failed = Some(error);
                    failure
                }
            },
            f::Kind::Branch {
                condition,
                then_node,
                else_node,
                failure,
            } => {
                let condition = {
                    let state = shared.borrow();
                    state
                        .root(values + condition.0 as usize)
                        .and_then(|root| state.read(root))
                };
                match condition {
                    Ok(l::Immediate::Bool(true)) => then_node,
                    Ok(l::Immediate::Bool(false)) => else_node,
                    _ => {
                        failed = Some(c::ExecutionFailure::Invariant);
                        failure
                    }
                }
            }
            f::Kind::StageReturn {
                value,
                next,
                failure,
            } => {
                let from = values + value.0 as usize;
                let escrow = address.cell + plan.return_cell();
                let staged = (|| -> Step<()> {
                    if shared.borrow().is_copy(from) {
                        let mut state = shared.borrow_mut();
                        let root = state.root(from)?;
                        state.copy(root, escrow)?;
                        drop(state);
                        frame.stage_return(None).map_err(invariant)
                    } else {
                        frame.value(address.slot(from)).map_err(invariant)?;
                        shared.borrow_mut().relocate(from, escrow)?;
                        let physical = shared.borrow().cells[escrow].physical;
                        *frame.value_mut(address.slot(from)).map_err(invariant)? =
                            Ticket::Cell(physical);
                        frame
                            .stage_return(Some(address.slot(from)))
                            .map_err(invariant)
                    }
                })();
                match staged {
                    Ok(()) => next,
                    Err(error) => {
                        failed = Some(error);
                        failure
                    }
                }
            }
            f::Kind::DiscardPrepared { next } => {
                shared.borrow_mut().prepared
                    [address.prepared..address.prepared + typed.functions()[index].prepared]
                    .fill(None);
                next
            }
            f::Kind::RetireReturn { next, failure } => {
                if frame.retire_exit().is_ok() {
                    next
                } else {
                    cause = c::Cause::Reported;
                    capture(
                        &mut trace,
                        &stack,
                        frame
                            .report()
                            .primary()
                            .and_then(|p| p.span)
                            .unwrap_or(node.span),
                    );
                    failed_function = address.function;
                    fault = frame
                        .report()
                        .primary()
                        .and_then(|p| p.span)
                        .or(Some(node.span));
                    failure
                }
            }
            f::Kind::PublishReturn => {
                let escrow = address.cell + plan.return_cell();
                if let Some(to) = activation.returned {
                    let published = (|| -> Step<()> {
                        if shared.borrow().is_copy(escrow) {
                            let mut state = shared.borrow_mut();
                            let root = state.root(escrow)?;
                            state.copy(root, to.cell)?;
                            drop(state);
                            frame.publish_staged_to(None).map_err(invariant)
                        } else {
                            shared.borrow_mut().relocate(escrow, to.cell)?;
                            let physical = shared.borrow().cells[to.cell].physical;
                            *frame
                                .staged_value_mut()
                                .map_err(invariant)?
                                .ok_or(c::ExecutionFailure::Invariant)? = Ticket::Cell(physical);
                            frame.publish_staged_to(Some(to.slot)).map_err(invariant)
                        }
                    })();
                    if let Err(error) = published {
                        capture(&mut trace, &stack, node.span);
                        fault = Some(node.span);
                        failed_function = address.function;
                        break frame.finish(c::Exit::Failure {
                            failure: error,
                            span: fault,
                        });
                    }
                    {
                        let mut state = shared.borrow_mut();
                        let State {
                            storage, allocator, ..
                        } = &mut *state;
                        storage
                            .as_mut()
                            .unwrap()
                            .release_frame(stack.len() - 1, allocator)
                            .map_err(|e| fail(Reason::Preparation(e)))?;
                    }
                    live_cells -= plan.cleanup_slots() + 1;
                    stack.pop();
                    stack.last_mut().unwrap().node = to.next;
                    continue;
                }
                if let Some(prepared) = preview.take() {
                    let rendered = {
                        let state = shared.borrow();
                        let cell = state
                            .storage
                            .as_ref()
                            .unwrap()
                            .cell(state.cells[escrow].physical)
                            .unwrap();
                        prepared.render(cell, state.allocator)
                    };
                    match rendered {
                        Ok(value) => {
                            if frame.consume_entry_result().is_err() {
                                let span = frame
                                    .report()
                                    .primary()
                                    .and_then(|record| record.span)
                                    .unwrap_or(node.span);
                                capture(&mut trace, &stack, span);
                                fault = Some(span);
                                failed_function = address.function;
                                break frame.finish_reported_failure();
                            }
                            result = Some(Value::Preview(value));
                            break frame.finish_staged_return();
                        }
                        Err(error) => {
                            let cause = if matches!(error, l::PreviewError::Limit) {
                                c::ExecutionFailure::AllocationLimit
                            } else {
                                c::ExecutionFailure::Invariant
                            };
                            capture(&mut trace, &stack, node.span);
                            fault = Some(node.span);
                            failed_function = address.function;
                            preview_error = Some(error);
                            let _ = frame.begin_failure(c::Cause::Execution(cause), fault);
                            let _ = frame.discard_result();
                            break frame.finish_reported_failure();
                        }
                    }
                }
                let output = {
                    let state = shared.borrow();
                    state.root(escrow).and_then(|root| state.read(root))
                };
                if let Ok(l::Immediate::I64(value)) = output {
                    result = Some(Value::Scalar(value));
                    break frame.finish_staged_return();
                }
                break frame.finish(c::Exit::Failure {
                    failure: c::ExecutionFailure::Invariant,
                    span: Some(node.span),
                });
            }
            f::Kind::Raise { failure, next } => {
                failed = Some(failure);
                next
            }
            f::Kind::RecordFailure { next } => {
                let _ = frame.begin_failure(cause, fault);
                next
            }
            f::Kind::RetireFailure { next } => {
                let _ = frame.retire_exit();
                next
            }
            f::Kind::DiscardResult { next } => {
                let _ = frame.discard_result();
                next
            }
            f::Kind::PropagateFailure => {
                if let Some(to) = activation.returned {
                    let _ = frame.propagate_call_failure();
                    {
                        let mut state = shared.borrow_mut();
                        let State {
                            storage, allocator, ..
                        } = &mut *state;
                        storage
                            .as_mut()
                            .unwrap()
                            .release_frame(stack.len() - 1, allocator)
                            .map_err(|e| fail(Reason::Preparation(e)))?;
                    }
                    live_cells -= plan.cleanup_slots() + 1;
                    stack.pop();
                    stack.last_mut().unwrap().node = to.failure;
                    cause = c::Cause::Reported;
                    continue;
                }
                break frame.finish_reported_failure();
            }
        };
        if let Some(error) = failed {
            if trace.is_empty() {
                capture(&mut trace, &stack, node.span);
                fault = Some(node.span);
                failed_function = address.function;
            }
            cause = if frame.report().primary().is_some() {
                c::Cause::Reported
            } else {
                c::Cause::Execution(error)
            };
        }
        stack.last_mut().unwrap().node = next;
    };
    let mut state = shared.borrow_mut();
    let panic_code = state.panic_code;
    let State {
        storage, allocator, ..
    } = &mut *state;
    storage
        .as_mut()
        .unwrap()
        .release(allocator)
        .map_err(|e| fail(Reason::Preparation(e)))?;
    if let Some(primary) = completion.report.primary() {
        return Err(Failure {
            reason: preview_error.map_or(Reason::Execution(primary.kind), Reason::Preview),
            usage,
            function: Some(failed_function),
            fault: primary.span.or(fault),
            report: Some(completion.report),
            panic_code,
            stack: trace,
        });
    }
    Ok(Execution {
        value: result.ok_or_else(|| {
            fail(Reason::Execution(c::FailureKind::Execution(
                c::ExecutionFailure::Invariant,
            )))
        })?,
        usage,
        allocations: allocator.stats(),
    })
}
