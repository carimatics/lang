//! In-memory execution of the loan-checked ownership profile. Unsupported
//! reachable operations are rejected before provider callbacks or guest effects.
use super::{Control, Usage};
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, failure as f},
};
use lang_runtime::{allocation as a, buffer, cleanup as c, layout as l};
use std::{
    cell::RefCell,
    fmt::{Display, Formatter},
    rc::Rc,
};

mod calls;
#[cfg(test)]
mod failure_tests;
mod profile;
mod state;
mod storage;
use state::{State, Step, Target, invariant};
use storage::StackStorage;
#[derive(Debug)]
pub enum Reason {
    Entry,
    Unsupported { core: Option<o::core::Operation> },
    BlockArguments,
    MissingAllocator,
    Poisoned,
    Control(super::Reason),
    Preparation(l::Error),
    Enter(c::EnterError),
    Execution(c::FailureKind),
    Preview(l::PreviewError),
}
#[derive(Debug)]
pub struct Failure {
    pub reason: Reason,
    pub usage: Usage,
    pub function: Option<m::FunctionId>,
    pub fault: Option<m::Span>,
    pub report: Option<c::Report>,
    /// Original RFC 0009 guest-panic code, preserved through failure cleanup.
    pub panic_code: Option<i64>,
    /// Innermost-first entered guest frames, reserved before execution.
    pub stack: Vec<super::StackFrame>,
}
impl Display for Failure {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ownership execution failed: {:?}", self.reason)?;
        if let Some(code) = self.panic_code {
            write!(f, " (panic code {code})")?;
        }
        Ok(())
    }
}
impl std::error::Error for Failure {}
pub struct Outcome {
    pub value: i64,
    pub usage: Usage,
    pub allocations: a::Stats,
}
pub struct PreviewOutcome {
    pub value: String,
    pub usage: Usage,
    pub allocations: a::Stats,
}
/// Execute a checked zero-argument function and expose only its RFC 0010 host
/// preview after all guest/result storage has been retired. No authority is
/// supplied implicitly; this does not change the script main/i64 contract.
#[allow(
    clippy::result_large_err,
    reason = "Preserve the completed failure report inline after OOM."
)]
pub fn execute_preview<P: a::Provider>(
    flow: &f::Elaborated,
    function: m::FunctionId,
    instance: &mut c::Instance,
    allocator: Option<&mut a::Allocator<P>>,
    control: Control<'_>,
    display_bytes: usize,
) -> Result<PreviewOutcome, Failure> {
    calls::preview(flow, function, instance, allocator, control, display_bytes)
}
#[derive(Clone, Copy)]
enum Ticket {
    Cell(usize),
    Loan(usize),
}
#[derive(Clone, Copy)]
struct Address {
    function: m::FunctionId,
    cell: usize,
    slot: usize,
    loan: usize,
    prepared: usize,
}
impl Address {
    fn entry(function: m::FunctionId) -> Self {
        Self {
            function,
            cell: 0,
            slot: 0,
            loan: 0,
            prepared: 0,
        }
    }
    fn slot(self, cell: usize) -> c::Slot {
        c::Slot(self.slot + (cell - self.cell))
    }
}
fn rejected(reason: Reason, function: Option<m::FunctionId>, fault: Option<m::Span>) -> Failure {
    Failure {
        reason,
        function,
        fault,
        usage: Usage::default(),
        report: None,
        panic_code: None,
        stack: Vec::new(),
    }
}
fn supported(op: &b::Op, payloads: &profile::Payloads, result: Option<o::Type>) -> bool {
    match op {
        b::Op::CoreCall(call) => {
            (call.operation.extraction().is_some()
                && call.type_arguments.iter().all(|t| payloads.supports(t.ty)))
                || matches!(
                    call.operation,
                    o::core::Operation::Allocator
                        | o::core::Operation::BytesNew
                        | o::core::Operation::BytesClone
                        | o::core::Operation::BytesLen
                        | o::core::Operation::BytesGet
                        | o::core::Operation::BytesSet
                        | o::core::Operation::BytesPush
                        | o::core::Operation::TextFromUtf8
                        | o::core::Operation::TextLen
                        | o::core::Operation::TextClone
                        | o::core::Operation::TextBytes
                        | o::core::Operation::TextConcat
                        | o::core::Operation::TextEqual
                )
        }
        b::Op::Record { .. } | b::Op::Variant { .. } => {
            result.is_some_and(|ty| payloads.supports(ty))
        }
        b::Op::I64(_)
        | b::Op::Bool(_)
        | b::Op::Unit
        | b::Op::Add(..)
        | b::Op::LessThan(..)
        | b::Op::EqualI64(..)
        | b::Op::Copy(_)
        | b::Op::Move(_)
        | b::Op::CopyPlace(_)
        | b::Op::MovePlace(_)
        | b::Op::Initialize { .. }
        | b::Op::Replace { .. }
        | b::Op::PreparePlace { .. }
        | b::Op::ReplacePrepared { .. }
        | b::Op::DiscardPrepared(_)
        | b::Op::DropValue(_)
        | b::Op::DropLocal(_)
        | b::Op::DropValueIfAvailable(_)
        | b::Op::DropLocalIfInitialized(_)
        | b::Op::EndBorrow { .. }
        | b::Op::IsCase { .. }
        | b::Op::CopyCaseField { .. }
        | b::Op::BorrowCaseField { .. }
        | b::Op::Borrow { .. } => true,
        b::Op::Cleanup(cleanup) => !cleanup
            .steps
            .iter()
            .any(|step| matches!(step.target, b::CleanupTarget::Arena(_))),
        _ => false,
    }
}
fn execution_reason(reason: super::Reason) -> c::ExecutionFailure {
    match reason {
        super::Reason::Interrupted => c::ExecutionFailure::Interrupted,
        super::Reason::DeadlineExceeded => c::ExecutionFailure::DeadlineExceeded,
        super::Reason::FuelExhausted => c::ExecutionFailure::FuelLimit,
        super::Reason::CounterOverflow => c::ExecutionFailure::CounterOverflow,
        _ => c::ExecutionFailure::Invariant,
    }
}
fn budget_usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}

/// Zero-argument i64 script entry with bounded guest activations. Ownership MIR
/// remains in-memory; filesystem/network/FFI host capabilities are not granted.
#[allow(
    clippy::result_large_err,
    reason = "Return the completed cleanup report inline; boxing it after OOM could allocate during failure handling."
)]
pub fn execute_entry<P: a::Provider>(
    flow: &f::Elaborated,
    instance: &mut c::Instance,
    allocator: Option<&mut a::Allocator<P>>,
    control: Control<'_>,
) -> Result<Outcome, Failure> {
    let typed = flow.source().activity().moved().typed();
    let entry = typed
        .places()
        .declarations()
        .functions()
        .iter()
        .position(|f| f.name == "main");
    let has_calls = entry.is_some_and(|i| {
        typed.functions()[i].blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.op, b::Op::Call { .. }))
        })
    });
    if has_calls {
        calls::execute(flow, instance, allocator, control)
    } else {
        execute_single(flow, instance, allocator, control)
    }
}
#[allow(
    clippy::result_large_err,
    reason = "Preserve inline failure reporting after allocation failure."
)]
fn execute_single<P: a::Provider>(
    flow: &f::Elaborated,
    instance: &mut c::Instance,
    allocator: Option<&mut a::Allocator<P>>,
    control: Control<'_>,
) -> Result<Outcome, Failure> {
    if instance.is_poisoned() {
        return Err(rejected(Reason::Poisoned, None, None));
    }
    control
        .check()
        .map_err(|e| rejected(Reason::Control(e), None, None))?;
    let typed = flow.source().activity().moved().typed();
    let declarations = typed.places().declarations();
    let index = declarations
        .functions()
        .iter()
        .position(|f| f.name == "main")
        .ok_or_else(|| rejected(Reason::Entry, None, None))?;
    let function = m::FunctionId(index as u32);
    let declaration = &declarations.functions()[index];
    let span = Some(declaration.signature_span);
    let fail = |reason| rejected(reason, Some(function), span);
    if !declaration.parameters.is_empty() || declaration.result.ty != o::Type::I64 {
        return Err(fail(Reason::Entry));
    }
    let body = &typed.functions()[index];
    let types = declarations.types();
    let limits = m::Limits::default();
    let payloads =
        profile::Payloads::compile(types, limits).map_err(|e| fail(Reason::Preparation(e)))?;
    for block in &body.blocks {
        for instruction in &block.instructions {
            let result = instruction
                .results
                .first()
                .map(|v| typed.places().functions()[index].values[v.0 as usize].ty);
            if !supported(&instruction.op, &payloads, result) {
                let core = if let b::Op::CoreCall(call) = &instruction.op {
                    Some(call.operation)
                } else {
                    None
                };
                return Err(rejected(
                    Reason::Unsupported { core },
                    Some(function),
                    Some(instruction.span),
                ));
            }
        }
    }
    if control.limits.frames == Some(0) {
        return Err(fail(Reason::Control(super::Reason::FrameLimit)));
    }
    let allocator = allocator.ok_or_else(|| fail(Reason::MissingAllocator))?;
    a::Allocator::preflight(Some(allocator)).map_err(|e| fail(Reason::Preparation(e.into())))?;
    let layouts = l::Layouts::compile(types, limits).map_err(|e| fail(Reason::Preparation(e)))?;
    let plan = l::FramePlan::compile(
        &layouts,
        typed,
        function,
        limits,
        control.limits.live_cells.unwrap_or(usize::MAX),
    )
    .map_err(|e| fail(Reason::Preparation(e)))?;
    let extra = |a: usize, b: usize, common: usize| a.checked_add(b.checked_sub(common)?);
    let initial = m::Usage {
        work: extra(plan.usage().work, flow.work_used(), typed.work_used())
            .and_then(|n| n.checked_add(payloads.usage.work))
            .ok_or_else(|| fail(Reason::Preparation(l::Error::Overflow)))?,
        allocation_bytes: extra(
            plan.usage().allocation_bytes,
            flow.requested_bytes(),
            typed.requested_bytes(),
        )
        .and_then(|n| n.checked_add(payloads.usage.allocation_bytes))
        .ok_or_else(|| fail(Reason::Preparation(l::Error::Overflow)))?,
        elements: extra(
            plan.usage().elements,
            flow.elements_used(),
            typed.elements_used(),
        )
        .and_then(|n| n.checked_add(payloads.usage.elements))
        .ok_or_else(|| fail(Reason::Preparation(l::Error::Overflow)))?,
    };
    let mut budget =
        m::Budget::with_usage(limits, initial).map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut cells = budget
        .vector(plan.cell_count())
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut inverse = budget
        .vector(plan.cell_count())
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut loans = budget
        .vector(body.loans)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut prepared = budget
        .vector(body.prepared)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    prepared.resize(body.prepared, None);
    let mut trace = budget
        .vector(1)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut storages = budget
        .vector(1)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    let mut active = budget
        .vector(1)
        .map_err(|e| fail(Reason::Preparation(e.into())))?;
    for i in 0..plan.cell_count() {
        cells.push(state::Cell {
            physical: i,
            ty: plan.cell_type(i).unwrap().ty,
            live: false,
            slot: (i < declaration.locals.len() + typed.places().functions()[index].values.len())
                .then_some(c::Slot(i)),
        });
        inverse.push(i);
    }
    for _ in 0..body.loans {
        loans.push(state::Loan {
            generation: 0,
            target: None,
        });
    }
    let shared = Rc::new(RefCell::new(State {
        allocator,
        storage: None,
        types,
        cells,
        inverse,
        loans,
        prepared,
        panic_code: None,
        current: Address::entry(function),
    }));
    let cleaner_state = Rc::clone(&shared);
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: plan.cleanup_slots(),
                scopes: 1,
                ..Default::default()
            },
            move |ticket: &mut Ticket, _, context: &mut c::Context<'_>| {
                cleaner_state.borrow_mut().cleanup(ticket, context)
            },
        )
        .map_err(|e| fail(Reason::Enter(e)))?;
    let locals = declaration.locals.len();
    let value_base = locals;
    let loan_base = locals + typed.places().functions()[index].values.len();
    for i in 0..loan_base {
        if !types
            .properties(plan.cell_type(i).unwrap().ty)
            .unwrap()
            .is_copy
        {
            frame.declare(c::Slot(i), c::Kind::Owner).map_err(|_| {
                fail(Reason::Execution(c::FailureKind::Execution(
                    c::ExecutionFailure::Invariant,
                )))
            })?;
        }
    }
    for i in 0..body.loans {
        frame
            .declare(c::Slot(loan_base + i), c::Kind::Borrow)
            .map_err(|_| {
                fail(Reason::Execution(c::FailureKind::Execution(
                    c::ExecutionFailure::Invariant,
                )))
            })?;
    }
    for parameter in body.blocks.iter().flat_map(|b| &b.parameters) {
        let cell = plan.scratch_cell(*parameter).expect("planned parameter");
        if !types
            .properties(plan.cell_type(cell).unwrap().ty)
            .unwrap()
            .is_copy
        {
            frame
                .declare(
                    c::Slot(plan.scratch_slot(*parameter).unwrap()),
                    c::Kind::Owner,
                )
                .map_err(|_| {
                    fail(Reason::Execution(c::FailureKind::Execution(
                        c::ExecutionFailure::Invariant,
                    )))
                })?;
        }
    }
    let reserved = plan.reserve_after(shared.borrow_mut().allocator, budget_usage(&budget));
    let storage = match reserved {
        Ok(storage) => storage,
        Err(error) => {
            let completion = frame.finish(c::Exit::Failure {
                failure: c::ExecutionFailure::AllocationLimit,
                span,
            });
            let mut error = fail(Reason::Preparation(error));
            error.report = Some(completion.report);
            return Err(error);
        }
    };
    storages.push(storage);
    active.push(true);
    shared.borrow_mut().storage = Some(StackStorage {
        frames: storages,
        active,
        stride: plan.cell_count(),
    });
    let mapping = c::MirLayout::new(typed, function, 0).expect("verified frame");
    let mut usage = Usage {
        peak_frames: 1,
        peak_live_cells: plan.cleanup_slots() + 1,
        ..Default::default()
    };
    let mut node = flow.functions()[index].entry();
    let mut cause = c::Cause::Execution(c::ExecutionFailure::Invariant);
    let mut fault = span;
    let mut result = None;
    let completion = loop {
        let current = flow.functions()[index].nodes()[node.0 as usize];
        node = match current.kind {
            f::Kind::Entry { next, .. } => next,
            f::Kind::Fuel { next, failure, .. } => {
                let charged = control.check().and_then(|()| {
                    if control.limits.fuel.is_some_and(|n| usage.fuel >= n) {
                        return Err(super::Reason::FuelExhausted);
                    }
                    usage.fuel = usage
                        .fuel
                        .checked_add(1)
                        .ok_or(super::Reason::CounterOverflow)?;
                    Ok(())
                });
                match charged {
                    Ok(()) => next,
                    Err(error) => {
                        cause = c::Cause::Execution(execution_reason(error));
                        fault = Some(current.span);
                        failure
                    }
                }
            }
            f::Kind::Execute {
                site,
                next,
                failure,
            } => match instruction(typed, function, site, &shared, &mut frame, &mapping) {
                Ok(()) => next,
                Err(error) => {
                    cause = if frame.report().primary().is_some() {
                        c::Cause::Reported
                    } else {
                        c::Cause::Execution(error)
                    };
                    fault = Some(current.span);
                    failure
                }
            },
            f::Kind::Transfer {
                block,
                arm,
                target,
                failure,
            } => match transfer_edge(typed, function, block, arm, &plan, &shared, &mut frame) {
                Ok(()) => target,
                Err(error) => {
                    cause = if frame.report().primary().is_some() {
                        c::Cause::Reported
                    } else {
                        c::Cause::Execution(error)
                    };
                    fault = Some(current.span);
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
                        .root(value_base + condition.0 as usize)
                        .and_then(|p| state.read(p))
                };
                match condition {
                    Ok(l::Immediate::Bool(true)) => then_node,
                    Ok(l::Immediate::Bool(false)) => else_node,
                    _ => {
                        cause = c::Cause::Execution(c::ExecutionFailure::Invariant);
                        fault = Some(current.span);
                        failure
                    }
                }
            }
            f::Kind::StageReturn {
                value,
                next,
                failure,
            } => {
                let value = {
                    let state = shared.borrow();
                    state
                        .root(value_base + value.0 as usize)
                        .and_then(|p| state.read(p))
                };
                match value {
                    Ok(l::Immediate::I64(value)) if frame.stage_return(None).is_ok() => {
                        result = Some(value);
                        next
                    }
                    _ => {
                        cause = c::Cause::Execution(c::ExecutionFailure::Invariant);
                        fault = Some(current.span);
                        failure
                    }
                }
            }
            f::Kind::DiscardPrepared { next } => {
                shared.borrow_mut().prepared.fill(None);
                next
            }
            f::Kind::RetireReturn { next, failure } => {
                if frame.retire_exit().is_ok() {
                    next
                } else {
                    cause = c::Cause::Reported;
                    failure
                }
            }
            f::Kind::PublishReturn => break frame.finish_staged_return(),
            f::Kind::Raise { failure, next } => {
                cause = c::Cause::Execution(failure);
                fault = Some(current.span);
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
                result = None;
                let _ = frame.discard_result();
                next
            }
            f::Kind::PropagateFailure => break frame.finish_reported_failure(),
        };
    };
    let mut state = shared.borrow_mut();
    let panic_code = state.panic_code;
    let State {
        allocator, storage, ..
    } = &mut *state;
    if let Err(error) = storage
        .as_mut()
        .expect("admitted storage")
        .release(allocator)
    {
        return Err(Failure {
            reason: Reason::Preparation(error),
            usage,
            function: Some(function),
            fault,
            report: Some(completion.report),
            panic_code,
            stack: Vec::new(),
        });
    }
    if let Some(primary) = completion.report.primary() {
        trace.push(super::StackFrame {
            function,
            span: primary.span.or(fault),
        });
        return Err(Failure {
            reason: Reason::Execution(primary.kind),
            usage,
            function: Some(function),
            fault: primary.span.or(fault),
            report: Some(completion.report),
            panic_code,
            stack: trace,
        });
    }
    let value = result.ok_or_else(|| {
        fail(Reason::Execution(c::FailureKind::Execution(
            c::ExecutionFailure::Invariant,
        )))
    })?;
    Ok(Outcome {
        value,
        usage,
        allocations: allocator.stats(),
    })
}

fn transfer<P: a::Provider, C: c::Cleaner<Ticket>>(
    shared: &Rc<RefCell<State<'_, '_, '_, P>>>,
    frame: &mut c::Frame<'_, Ticket, C>,
    from: usize,
    to: usize,
) -> Step<()> {
    let address = shared.borrow().current;
    transfer_between(
        shared,
        frame,
        from,
        to,
        address.slot(from),
        address.slot(to),
    )
}
fn transfer_between<P: a::Provider, C: c::Cleaner<Ticket>>(
    shared: &Rc<RefCell<State<'_, '_, '_, P>>>,
    frame: &mut c::Frame<'_, Ticket, C>,
    from: usize,
    to: usize,
    source_slot: c::Slot,
    target_slot: c::Slot,
) -> Step<()> {
    let mut state = shared.borrow_mut();
    let properties = state
        .types
        .properties(state.cells[from].ty)
        .ok_or(c::ExecutionFailure::Invariant)?;
    if properties.is_copy {
        let source = state.root(from)?;
        state.copy(source, to)
    } else {
        frame
            .move_value(source_slot, target_slot)
            .map_err(invariant)?;
        state.swap_move(from, to)
    }
}
fn replace<P: a::Provider, C: c::Cleaner<Ticket>>(
    shared: &Rc<RefCell<State<'_, '_, '_, P>>>,
    frame: &mut c::Frame<'_, Ticket, C>,
    capture: state::Capture,
    source: usize,
    span: m::Span,
) -> Step<()> {
    let (
        logical,
        target,
        root,
        copy,
        live,
        source_slot,
        target_slot,
        source_physical,
        target_physical,
    ) = {
        let state = shared.borrow();
        let (logical, target, root) = state.captured(capture)?;
        let Target::Data { cell, .. } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        (
            logical,
            target,
            root,
            state.is_copy(source),
            state.cells[logical].live,
            state.cells[source]
                .slot
                .ok_or(c::ExecutionFailure::Invariant)?,
            state.cells[logical]
                .slot
                .ok_or(c::ExecutionFailure::Invariant)?,
            state.cells[source].physical,
            cell,
        )
    };
    if !copy {
        frame.value(source_slot).map_err(invariant)?;
        if root && !live {
            return transfer_between(shared, frame, source, logical, source_slot, target_slot);
        }
        frame.value(target_slot).map_err(invariant)?;
    }
    let old_span = shared.borrow_mut().exchange(target, source, span)?;
    if copy {
        let mut state = shared.borrow_mut();
        // Copy operands remain reusable SSA values after publication.
        state.copy(target, source)?;
        state.cells[logical].live = true;
        Ok(())
    } else if root {
        *frame.value_mut(source_slot).map_err(invariant)? = Ticket::Cell(target_physical);
        *frame.value_mut(target_slot).map_err(invariant)? = Ticket::Cell(source_physical);
        frame.replace(source_slot, target_slot).map_err(invariant)
    } else {
        frame
            .retire_replaced(source_slot, old_span)
            .map_err(invariant)
    }
}

fn transfer_edge<P: a::Provider, C: c::Cleaner<Ticket>>(
    typed: &b::TypedBodies,
    function: m::FunctionId,
    block: m::BlockId,
    arm: u8,
    plan: &l::FramePlan<'_, '_>,
    shared: &Rc<RefCell<State<'_, '_, '_, P>>>,
    frame: &mut c::Frame<'_, Ticket, C>,
) -> Step<()> {
    let body = &typed.functions()[function.0 as usize];
    let address = shared.borrow().current;
    let target = body.blocks[block.0 as usize]
        .terminator
        .kind
        .targets()
        .nth(arm as usize)
        .ok_or(c::ExecutionFailure::Invariant)?;
    let parameters = &body.blocks[target.block.0 as usize].parameters;
    let values = address.cell
        + typed.places().declarations().functions()[function.0 as usize]
            .locals
            .len();
    // Capture every input before publishing any output. Affine inputs are
    // unique by the independent move proof; Copy inputs may be repeated.
    for (argument, parameter) in target.arguments.iter().zip(parameters) {
        let from = values + argument.0 as usize;
        let scratch = address.cell
            + plan
                .scratch_cell(*parameter)
                .ok_or(c::ExecutionFailure::Invariant)?;
        transfer_between(
            shared,
            frame,
            from,
            scratch,
            address.slot(from),
            c::Slot(address.slot + plan.scratch_slot(*parameter).unwrap()),
        )?;
    }
    for parameter in parameters {
        let to = values + parameter.0 as usize;
        let scratch = address.cell + plan.scratch_cell(*parameter).unwrap();
        let old_nondropping_affine = {
            let state = shared.borrow();
            let properties = state.types.properties(state.cells[to].ty).unwrap();
            state.cells[to].live && !properties.is_copy && !properties.requires_drop
        };
        if old_nondropping_affine {
            drop_cell(shared, frame, to, true)?;
        }
        transfer_between(
            shared,
            frame,
            scratch,
            to,
            c::Slot(address.slot + plan.scratch_slot(*parameter).unwrap()),
            address.slot(to),
        )?;
    }
    Ok(())
}
fn instruction<P: a::Provider, C: c::Cleaner<Ticket>>(
    typed: &b::TypedBodies,
    function: m::FunctionId,
    site: b::Site,
    shared: &Rc<RefCell<State<'_, '_, '_, P>>>,
    frame: &mut c::Frame<'_, Ticket, C>,
    mapping: &c::MirLayout<'_>,
) -> Step<()> {
    let index = function.0 as usize;
    let address = shared.borrow().current;
    let values = typed.places().declarations().functions()[index]
        .locals
        .len();
    let loans = address.slot + values + typed.places().functions()[index].values.len();
    let instruction =
        &typed.functions()[index].blocks[site.block.0 as usize].instructions[site.order - 1];
    let span = instruction.span;
    let places = &typed.places().functions()[index].places;
    let value = |v: m::Register| address.cell + values + v.0 as usize;
    let result = || {
        instruction
            .results
            .first()
            .map(|v| value(*v))
            .ok_or(c::ExecutionFailure::Invariant)
    };
    let register_loan = |frame: &mut c::Frame<'_, Ticket, C>, id: usize| {
        frame
            .initialize(
                c::Slot(loans + id),
                Ticket::Loan(address.loan + id),
                Some(span),
            )
            .map_err(invariant)
    };
    for register in &instruction.results {
        let index = value(*register);
        let nondropping_affine = {
            let state = shared.borrow();
            let properties = state.types.properties(state.cells[index].ty).unwrap();
            !properties.is_copy && !properties.requires_drop
        };
        if nondropping_affine && frame.value(address.slot(index)).is_ok() {
            frame.drop_value(address.slot(index)).map_err(invariant)?;
        }
    }
    match &instruction.op {
        b::Op::I64(n) => shared
            .borrow_mut()
            .write(result()?, l::Immediate::I64(*n))?,
        b::Op::Bool(n) => shared
            .borrow_mut()
            .write(result()?, l::Immediate::Bool(*n))?,
        b::Op::Unit => shared.borrow_mut().write(result()?, l::Immediate::Unit)?,
        op @ (b::Op::Add(a, b) | b::Op::LessThan(a, b) | b::Op::EqualI64(a, b)) => {
            let mut state = shared.borrow_mut();
            let l::Immediate::I64(a) = state.read(state.root(value(*a))?)? else {
                return Err(c::ExecutionFailure::Invariant);
            };
            let l::Immediate::I64(b) = state.read(state.root(value(*b))?)? else {
                return Err(c::ExecutionFailure::Invariant);
            };
            let output = match op {
                b::Op::Add(..) => l::Immediate::I64(a.wrapping_add(b)),
                b::Op::LessThan(..) => l::Immediate::Bool(a < b),
                _ => l::Immediate::Bool(a == b),
            };
            state.write(result()?, output)?;
        }
        b::Op::Copy(source) | b::Op::Move(source) => {
            transfer(shared, frame, value(*source), result()?)?
        }
        b::Op::Initialize {
            local,
            value: source,
        } => transfer(
            shared,
            frame,
            value(*source),
            address.cell + local.0 as usize,
        )?,
        b::Op::PreparePlace { destination, place } => {
            let mut state = shared.borrow_mut();
            let capture = state.capture(&places[place.0 as usize], values)?;
            state.prepared[address.prepared + destination.0 as usize] = Some(capture);
        }
        b::Op::ReplacePrepared {
            destination,
            value: source,
        } => {
            let capture = shared.borrow_mut().prepared[address.prepared + destination.0 as usize]
                .take()
                .ok_or(c::ExecutionFailure::Invariant)?;
            replace(
                shared,
                frame,
                capture,
                value(*source),
                typed.places().functions()[index].values[source.0 as usize].span,
            )?;
        }
        b::Op::Replace {
            place,
            value: source,
        } => {
            let capture = shared.borrow().capture(&places[place.0 as usize], values)?;
            replace(
                shared,
                frame,
                capture,
                value(*source),
                typed.places().functions()[index].values[source.0 as usize].span,
            )?;
        }
        b::Op::DiscardPrepared(destination) => {
            shared.borrow_mut().prepared[address.prepared + destination.0 as usize] = None;
        }
        b::Op::CopyPlace(place) | b::Op::MovePlace(place) => {
            let place = &places[place.0 as usize];
            let target = shared.borrow().place(place, values)?;
            if matches!(instruction.op, b::Op::MovePlace(_)) {
                if !place.projections.is_empty() {
                    return Err(c::ExecutionFailure::Invariant);
                }
                let from = match place.root {
                    o::places::Root::Local(id) => address.cell + id.0 as usize,
                    o::places::Root::Value(id) => value(id),
                };
                transfer(shared, frame, from, result()?)?;
            } else {
                shared.borrow_mut().copy(target, result()?)?;
            }
        }
        b::Op::Borrow { loan, place, .. } => {
            let target = shared.borrow().place(&places[place.0 as usize], values)?;
            shared
                .borrow_mut()
                .borrow(result()?, loan.0 as usize, target)?;
            register_loan(frame, loan.0 as usize)?;
        }
        b::Op::IsCase {
            value: source,
            case,
        } => {
            let mut state = shared.borrow_mut();
            let actual = state.case(state.referent(value(*source))?)?;
            state.write(result()?, l::Immediate::Bool(actual == *case))?;
        }
        b::Op::BorrowCaseField {
            value: source,
            case,
            field,
            guard,
            loan,
        } => {
            let target = {
                let state = shared.borrow();
                if state.read(state.root(value(*guard))?)? != l::Immediate::Bool(true) {
                    return Err(c::ExecutionFailure::Invariant);
                }
                let target = state.referent(value(*source))?;
                if state.case(target)? != *case {
                    return Err(c::ExecutionFailure::Invariant);
                }
                state.field(target, *field)?
            };
            shared
                .borrow_mut()
                .borrow(result()?, loan.0 as usize, target)?;
            register_loan(frame, loan.0 as usize)?;
        }
        b::Op::CopyCaseField {
            value: source,
            case,
            field,
            guard,
        } => {
            let mut state = shared.borrow_mut();
            if state.read(state.root(value(*guard))?)? != l::Immediate::Bool(true) {
                return Err(c::ExecutionFailure::Invariant);
            }
            let target = state.referent(value(*source))?;
            if state.case(target)? != *case {
                return Err(c::ExecutionFailure::Invariant);
            }
            if let Ok(field) = state.field(target, *field) {
                state.copy(field, result()?)?;
            } else {
                let payload = state
                    .read(target)?
                    .payload()
                    .ok_or(c::ExecutionFailure::Invariant)?;
                state.write(result()?, payload)?;
            }
        }
        b::Op::Record { fields, .. }
        | b::Op::Variant {
            payload: fields, ..
        } => {
            for from in fields.iter().map(|r| value(*r)) {
                if !shared.borrow().is_copy(from) {
                    frame.value(address.slot(from)).map_err(invariant)?;
                }
            }
            let field_span = |field: u32| {
                let register = fields[field as usize];
                typed.places().functions()[index].values[register.0 as usize].span
            };
            match &instruction.op {
                b::Op::Record {
                    initialization_order,
                    ..
                } => shared.borrow_mut().aggregate(
                    result()?,
                    None,
                    fields.iter().map(|r| value(*r)),
                    initialization_order
                        .iter()
                        .map(|field| (*field, field_span(*field))),
                )?,
                b::Op::Variant { case, .. } => shared.borrow_mut().aggregate(
                    result()?,
                    Some(*case),
                    fields.iter().map(|r| value(*r)),
                    (0..fields.len() as u32).map(|field| (field, field_span(field))),
                )?,
                _ => unreachable!(),
            }
            for from in fields.iter().map(|r| value(*r)) {
                if !shared.borrow().is_copy(from) {
                    let _ = frame.take_owner(address.slot(from)).map_err(invariant)?;
                }
            }
        }
        b::Op::CoreCall(call) if call.operation.extraction().is_some() => {
            let source = value(call.arguments[0]);
            let moved = !shared.borrow().is_copy(source);
            if moved {
                frame.value(address.slot(source)).map_err(invariant)?;
            }
            shared
                .borrow_mut()
                .extract(source, result()?, call.operation.extraction().unwrap())?;
            if moved {
                let _ = frame.take_owner(address.slot(source)).map_err(invariant)?;
            }
        }
        b::Op::CoreCall(call) => match call.operation {
            o::core::Operation::Allocator => {
                let id = call.result_loan.ok_or(c::ExecutionFailure::Invariant)?.0 as usize;
                shared
                    .borrow_mut()
                    .borrow(result()?, id, Target::Allocator)?;
                register_loan(frame, id)?;
            }
            o::core::Operation::BytesNew => {
                shared
                    .borrow()
                    .require_allocator(value(call.arguments[0]))?;
                let destination = result()?;
                shared.borrow_mut().bytes_new(destination, span)?;
                let physical = shared.borrow().cells[destination].physical;
                frame
                    .initialize(
                        address.slot(destination),
                        Ticket::Cell(physical),
                        Some(span),
                    )
                    .map_err(invariant)?;
            }
            o::core::Operation::BytesLen => {
                let len = shared.borrow().bytes_len(value(call.arguments[0]))?;
                shared
                    .borrow_mut()
                    .write(result()?, l::Immediate::I64(len))?;
            }
            o::core::Operation::TextLen => {
                let len = shared.borrow().text_len(value(call.arguments[0]))?;
                shared
                    .borrow_mut()
                    .write(result()?, l::Immediate::I64(len))?;
            }
            o::core::Operation::TextEqual => {
                let equal = shared
                    .borrow()
                    .text_equal(value(call.arguments[0]), value(call.arguments[1]))?;
                shared
                    .borrow_mut()
                    .write(result()?, l::Immediate::Bool(equal))?;
            }
            op @ (o::core::Operation::BytesClone
            | o::core::Operation::TextClone
            | o::core::Operation::TextBytes
            | o::core::Operation::TextConcat) => {
                let mut state = shared.borrow_mut();
                state.require_allocator(value(call.arguments[0]))?;
                state.allocate_buffer(
                    op,
                    value(call.arguments[1]),
                    call.arguments.get(2).map(|r| value(*r)),
                    result()?,
                    span,
                )?;
            }
            op @ (o::core::Operation::BytesGet
            | o::core::Operation::BytesSet
            | o::core::Operation::BytesPush) => {
                let mut state = shared.borrow_mut();
                let l::Immediate::I64(first) = state.read(state.root(value(call.arguments[1]))?)?
                else {
                    return Err(c::ExecutionFailure::Invariant);
                };
                let reference = value(call.arguments[0]);
                let destination = result()?;
                match op {
                    o::core::Operation::BytesGet => {
                        state.bytes_get(reference, first, destination, span)?
                    }
                    o::core::Operation::BytesPush => {
                        state.bytes_push(reference, first, destination, span)?
                    }
                    _ => {
                        let l::Immediate::I64(byte) =
                            state.read(state.root(value(call.arguments[2]))?)?
                        else {
                            return Err(c::ExecutionFailure::Invariant);
                        };
                        state.bytes_set(reference, first, byte, destination, span)?;
                    }
                }
            }
            o::core::Operation::TextFromUtf8 => {
                let source = value(call.arguments[0]);
                frame.value(address.slot(source)).map_err(invariant)?;
                shared
                    .borrow_mut()
                    .text_from_utf8(source, result()?, span)?;
                let _ = frame.take_owner(address.slot(source)).map_err(invariant)?;
            }
            _ => return Err(c::ExecutionFailure::Invariant),
        },
        b::Op::Cleanup(cleanup) => {
            let cleaned = frame.cleanup_mir(cleanup, mapping);
            let mut state = shared.borrow_mut();
            for step in &cleanup.steps {
                let target = match step.target {
                    b::CleanupTarget::Local(id) => Some(address.cell + id.0 as usize),
                    b::CleanupTarget::Value(id) => Some(value(id)),
                    _ => None,
                };
                if let Some(target) = target
                    && state
                        .types
                        .properties(state.cells[target].ty)
                        .unwrap()
                        .is_copy
                {
                    state.cells[target].live = false;
                }
            }
            cleaned.map_err(invariant)?;
        }
        b::Op::EndBorrow { loan, mode } => {
            let id = loan.0 as usize;
            if shared.borrow().loans[address.loan + id].target.is_some() {
                frame.drop_value(c::Slot(loans + id)).map_err(invariant)?;
            } else if *mode == b::EndMode::Required {
                return Err(c::ExecutionFailure::Invariant);
            }
        }
        b::Op::DropLocal(id) | b::Op::DropLocalIfInitialized(id) => {
            let required = matches!(instruction.op, b::Op::DropLocal(_));
            drop_cell(shared, frame, address.cell + id.0 as usize, required)?;
        }
        b::Op::DropValue(id) | b::Op::DropValueIfAvailable(id) => {
            let required = matches!(instruction.op, b::Op::DropValue(_));
            drop_cell(shared, frame, value(*id), required)?;
        }
        _ => return Err(c::ExecutionFailure::Invariant),
    }
    for register in &instruction.results {
        let index = value(*register);
        let (affine, physical) = {
            let state = shared.borrow();
            (
                !state
                    .types
                    .properties(state.cells[index].ty)
                    .unwrap()
                    .is_copy,
                state.cells[index].physical,
            )
        };
        if affine {
            match frame.value(address.slot(index)) {
                Ok(_) => {}
                Err(c::StateError::Uninitialized) => frame
                    .initialize(address.slot(index), Ticket::Cell(physical), Some(span))
                    .map_err(invariant)?,
                Err(_) => return Err(c::ExecutionFailure::Invariant),
            }
        }
    }
    Ok(())
}
fn drop_cell<P: a::Provider, C: c::Cleaner<Ticket>>(
    shared: &Rc<RefCell<State<'_, '_, '_, P>>>,
    frame: &mut c::Frame<'_, Ticket, C>,
    index: usize,
    required: bool,
) -> Step<()> {
    let (live, owning) = {
        let state = shared.borrow();
        (
            state.cells[index].live,
            !state
                .types
                .properties(state.cells[index].ty)
                .unwrap()
                .is_copy,
        )
    };
    if !live {
        return if required {
            Err(c::ExecutionFailure::Invariant)
        } else {
            Ok(())
        };
    }
    if owning {
        let slot = shared.borrow().current.slot(index);
        frame.drop_value(slot).map_err(invariant)?;
    } else {
        shared.borrow_mut().cells[index].live = false;
    }
    Ok(())
}
