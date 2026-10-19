//! Ordinary native root using the shared production primitives.
#![allow(
    clippy::result_large_err,
    reason = "Keep structured failure, source stack and cleanup report inline after resource exhaustion."
)]
use super::*;
use n::runtime::{Config, EntryError, ErrorKind, Outcome, PreparationError};
use std::{
    mem,
    panic::{AssertUnwindSafe, catch_unwind},
};

fn error(kind: ErrorKind) -> EntryError {
    EntryError {
        kind,
        usage: e::Usage::default(),
        report: None,
        stack: Vec::new(),
        cleanup: None,
        function: None,
        fault: None,
        panic_code: None,
    }
}
pub(crate) fn execute_entry<'a, 'l: 'a, P: a::Provider, S: n::runtime::EntryProgram<'l>>(
    program: &'l S,
    entry: m::FunctionId,
    instance: &'a mut c::Instance,
    allocator: &'a mut a::Allocator<P>,
    config: Config<'a>,
) -> Result<Outcome, EntryError> {
    let (mut result, poisoned) = execute(program, entry, &mut *instance, allocator, config);
    if poisoned {
        instance.poison();
    }
    if let Err(error) = &mut result {
        error.function.get_or_insert(entry);
        if error.fault.is_none() {
            error.fault = error
                .report
                .as_ref()
                .and_then(|r| r.primary())
                .and_then(|r| r.span)
                .or_else(|| {
                    program.frame_layout(entry).map(|layout| {
                        layout
                            .source()
                            .source()
                            .activity()
                            .moved()
                            .typed()
                            .places()
                            .declarations()
                            .functions()[entry.0 as usize]
                            .signature_span
                    })
                });
        }
    }
    result
}
fn execute<'i, 'a, 'l: 'a, P: a::Provider, S: n::runtime::EntryProgram<'l>>(
    program: &'l S,
    entry: m::FunctionId,
    instance: &'i mut c::Instance,
    allocator: &'a mut a::Allocator<P>,
    config: Config<'a>,
) -> (Result<Outcome, EntryError>, bool) {
    let state = prepare_entry(program, entry, instance, allocator, config);
    let mut state = match state {
        Ok(state) => state,
        Err(mut error) => {
            error.function = Some(entry);
            error.fault = program.frame_layout(entry).map(|layout| {
                layout
                    .source()
                    .source()
                    .activity()
                    .moved()
                    .typed()
                    .places()
                    .declarations()
                    .functions()[entry.0 as usize]
                    .signature_span
            });
            return (Err(error), false);
        }
    };
    let layout = program.frame_layout(entry).expect("preflight");
    let span = layout
        .source()
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .functions()[entry.0 as usize]
        .signature_span;
    let admitted = catch_unwind(AssertUnwindSafe(|| {
        let mut store = state.store.borrow_mut();
        let Store {
            pools,
            allocator,
            live,
            ..
        } = &mut *store;
        pools[0].admit(layout.frame(), allocator)?;
        live[0] = true;
        Ok::<_, l::Error>(())
    }));
    let (cause, host, preparation) = match admitted {
        Ok(Ok(())) => (None, None, None),
        Ok(Err(error)) => (
            Some(c::Cause::Execution(c::ExecutionFailure::AllocationLimit)),
            None,
            Some(error),
        ),
        Err(payload) => (
            Some(c::Cause::Execution(c::ExecutionFailure::HostPanic)),
            Some(gate::Fault::host_panic(payload)),
            None,
        ),
    };
    if let Some(cause) = cause {
        // Root storage was never admitted: no guest frame, instruction or
        // guest stack entry has been entered yet.
        state.usage = e::Usage::default();
        let _ = state
            .ledger
            .as_mut()
            .unwrap()
            .begin_failure(cause, Some(span));
        let mut result = complete(
            &mut state,
            Some(n::Failure {
                function: entry,
                reason: n::Reason::Runtime(cause),
                span,
                panic_code: None,
            }),
        );
        if let Err(error) = &mut result {
            if let Some(host) = host {
                error.cleanup = Some(host);
            }
            if let Some(preparation) = preparation {
                error.kind = ErrorKind::Preparation(PreparationError::Layout(preparation));
            }
        }
        let poisoned = matches!(cause, c::Cause::Execution(c::ExecutionFailure::HostPanic));
        return (result, poisoned);
    }
    let mut runtime = n::RuntimeInstance::new(state);
    let failed = program.invoke_entry(entry, &mut runtime).err();
    let poisoned = runtime.is_poisoned();
    let result = complete(runtime.runtime_mut(), failed);
    let poisoned = poisoned
        || result.as_ref().err().is_some_and(|e| {
            e.cleanup.is_some_and(gate::Fault::poisons)
                || matches!(e.kind, ErrorKind::HostPanic | ErrorKind::Invariant)
        });
    (result, poisoned)
}

fn complete<P: a::Provider>(
    state: &mut State<'_, '_, '_, '_, P>,
    failure: Option<n::Failure>,
) -> Result<Outcome, EntryError> {
    let ledger = state
        .ledger
        .take()
        .ok_or_else(|| error(ErrorKind::Invariant))?;
    let completion = if failure.is_none() && state.published {
        ledger.finish_staged_return()
    } else if ledger.report().primary().is_some() {
        ledger.finish_reported_failure()
    } else {
        let (cause, span) = match failure {
            Some(f) => (
                match f.reason {
                    n::Reason::Runtime(c::Cause::Execution(cause)) => cause,
                    n::Reason::Poisoned => c::ExecutionFailure::HostPanic,
                    _ => c::ExecutionFailure::Invariant,
                },
                Some(f.span),
            ),
            None => (c::ExecutionFailure::Invariant, None),
        };
        ledger.finish(c::Exit::Failure {
            failure: cause,
            span,
        })
    };
    let usage = e::Usage {
        service_work: state.service_work.used(),
        ..state.usage
    };
    let read = catch_unwind(AssertUnwindSafe(|| {
        completion.value.and_then(|ticket| {
            let physical = ticket.physical().ok()?;
            let store = state.store.borrow();
            let cell = store.pools[physical.frame].cell(physical.cell).ok()?;
            match cell.read_immediate(cell.root(), store.allocator).ok()? {
                l::Immediate::I64(value) => Some(value),
                _ => None,
            }
        })
    }));
    let (value, read_fault) = match read {
        Ok(value) => (value, None),
        Err(payload) => (None, Some(gate::Fault::host_panic(payload))),
    };
    let stack = mem::take(&mut state.fault_stack);
    let mut cleanup = state.close().err();
    if let Some(read_fault) = read_fault {
        if let Some(later) = cleanup {
            let mut first = read_fault;
            first.cleanup_count = first.cleanup_count.saturating_add(later.cleanup_count);
            first.quarantined = first.quarantined.saturating_add(later.quarantined);
            first.truncated |= later.truncated;
            cleanup = Some(first);
        } else {
            cleanup = Some(read_fault);
        }
    }
    if let Some(failure) = failure {
        return Err(EntryError {
            kind: ErrorKind::Native(failure),
            usage,
            report: Some(completion.report),
            stack,
            cleanup,
            function: Some(failure.function),
            fault: Some(failure.span),
            panic_code: failure.panic_code,
        });
    }
    if completion.report.primary().is_some() || cleanup.is_some() {
        let kind = if cleanup.is_some_and(|f| matches!(f.kind, gate::Kind::HostPanic)) {
            ErrorKind::HostPanic
        } else {
            ErrorKind::Cleanup
        };
        return Err(EntryError {
            kind,
            usage,
            report: Some(completion.report),
            stack,
            cleanup,
            function: None,
            fault: None,
            panic_code: None,
        });
    }
    match value {
        Some(value) => Ok(Outcome {
            value,
            usage,
            report: completion.report,
        }),
        None => Err(EntryError {
            kind: ErrorKind::Invariant,
            usage,
            report: Some(completion.report),
            stack,
            cleanup: None,
            function: None,
            fault: None,
            panic_code: None,
        }),
    }
}

fn prepare_entry<'i, 'a, 'l: 'a, P: a::Provider, S: n::runtime::EntryProgram<'l>>(
    program: &'l S,
    entry: m::FunctionId,
    instance: &'i mut c::Instance,
    allocator: &'a mut a::Allocator<P>,
    config: Config<'a>,
) -> Result<State<'i, 'l, 'a, 'l, P>, EntryError> {
    if instance.is_poisoned() {
        return Err(error(ErrorKind::Poisoned));
    }
    let layout = program
        .frame_layout(entry)
        .ok_or_else(|| error(ErrorKind::Entry))?;
    let declarations = layout
        .source()
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    let declaration = declarations
        .functions()
        .get(entry.0 as usize)
        .ok_or_else(|| error(ErrorKind::Entry))?;
    if !declaration.parameters.is_empty() || declaration.result.ty != m::ownership::Type::I64 {
        return Err(error(ErrorKind::Entry));
    }
    if declarations
        .imports()
        .iter()
        .any(|i| i.abi == m::ownership::declarations::ImportAbi::C)
        || declarations
            .functions()
            .iter()
            .any(|f| f.abi == m::ownership::declarations::FunctionAbi::CExport)
    {
        return Err(error(ErrorKind::CInterface));
    }
    config
        .control
        .check()
        .map_err(|e| error(ErrorKind::Preparation(PreparationError::Control(e))))?;
    if config.control.limits().frames == Some(0) {
        return Err(error(ErrorKind::Admission(c::ExecutionFailure::FrameLimit)));
    }
    if config
        .control
        .limits()
        .live_cells
        .is_some_and(|limit| limit < layout.control_cells())
    {
        return Err(error(ErrorKind::Admission(c::ExecutionFailure::CellLimit)));
    }
    let work = e::ServiceWork::new(config.control.service_work_limit());
    factory::prepare(factory::Preparation {
        program,
        layout,
        instance,
        allocator,
        limits: config.limits,
        control: config.control,
        cleanup_records: config.cleanup_records,
        work,
        mode: EntryMode::Ordinary,
        foreign_import: None,
    })
    .map_err(|e| error(ErrorKind::Preparation(e)))
}
