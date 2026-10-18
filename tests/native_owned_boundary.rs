//! Source-derived native whole-owner call/return boundary. Runtime callbacks
//! perform typed storage/ledger primitives; no callback evaluates a MIR body.
use lang::frontend;
use lang_codegen_clif::owned::{self as n, CompiledFunction, Runtime, RuntimeInstance};
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::{allocation as a, buffer, cleanup as c, layout as l};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::{cell::RefCell, path::Path, rc::Rc};

fn source(text: &str) -> Box<OwnershipProgram> {
    let analyzed = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("native-owned.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analyzed.diagnostics.is_empty(),
        "{:?}",
        analyzed.diagnostics
    );
    let Program::Ownership(p) = analyzed.result.unwrap() else {
        panic!("ownership program")
    };
    p
}

#[derive(Default)]
struct Trace {
    requests: usize,
    allocated: usize,
    released: usize,
    refuse: Option<usize>,
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<a::Storage> {
        self.0.borrow_mut().requests += 1;
        if self.0.borrow().refuse == Some(self.0.borrow().requests) {
            return None;
        }
        let storage = a::Storage::try_new(request).ok()?;
        self.0.borrow_mut().allocated += 1;
        Some(storage)
    }
    fn deallocate(&mut self, _: a::Storage, _: a::Request) {
        self.0.borrow_mut().released += 1;
    }
}

struct Storage<'p, 'l, 't> {
    frames: l::FrameStorage<'l, 't>,
    allocator: &'p mut a::Allocator<Provider>,
    expected: &'p [&'p [u8]],
    retired: Vec<usize>,
    refuse_cleanup: bool,
}
struct Cleaner<'s, 'p, 'l, 't>(&'s RefCell<Storage<'p, 'l, 't>>);
impl c::Cleaner<usize> for Cleaner<'_, '_, '_, '_> {
    fn cleanup(
        &mut self,
        cell: &mut usize,
        _: c::Reason,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let mut state = self.0.borrow_mut();
        let Storage {
            frames,
            allocator,
            expected,
            retired,
            refuse_cleanup,
        } = &mut *state;
        frames
            .cell_mut(*cell)
            .map_err(|_| 80u32)?
            .retire(allocator, context, |token, allocator, _, _| {
                let mut bytes =
                    buffer::Bytes::from_token(&mut token.bytes, allocator).map_err(|_| 81u32)?;
                let data = bytes.as_slice(allocator).map_err(|_| 82u32)?;
                let index = expected
                    .iter()
                    .position(|candidate| *candidate == data)
                    .ok_or(82u32)?;
                assert!(
                    retired.len() < retired.capacity(),
                    "trace capacity reserved before entry"
                );
                retired.push(index);
                bytes.release(allocator).map_err(|_| 83u32)?;
                if *refuse_cleanup { Err(84) } else { Ok(()) }
            })
            .map_err(|_| 85u32)
    }
}
struct Frame<'i, 's, 'p, 'l, 't> {
    ledger: Option<c::Frame<'i, usize, Cleaner<'s, 'p, 'l, 't>>>,
    storage: &'s RefCell<Storage<'p, 'l, 't>>,
    parameters: Vec<(usize, m::Span)>,
    return_cell: usize,
    fuel: u64,
    fuel_limit: Option<u64>,
    published: bool,
    expected_source: usize,
    expected_layouts: usize,
}
fn invariant(error: c::StateError) -> c::Cause {
    if error == c::StateError::Failed {
        c::Cause::Reported
    } else {
        c::Cause::Execution(c::ExecutionFailure::Invariant)
    }
}
impl Runtime for Frame<'_, '_, '_, '_, '_> {
    fn is_poisoned(&self) -> bool {
        self.ledger.as_ref().unwrap().is_poisoned()
    }
    fn enter(&mut self, layout: &n::FunctionLayout<'_>) -> n::ResultStep {
        if layout.function() != m::FunctionId(0)
            || layout.source() as *const _ as usize != self.expected_source
            || layout.layouts() as *const _ as usize != self.expected_layouts
        {
            return Err(c::Cause::Execution(c::ExecutionFailure::Invariant));
        }
        let plan = layout.frame();
        let ledger = self.ledger.as_mut().unwrap();
        for slot in 0..plan.cell_count() {
            ledger
                .declare(c::Slot(slot), c::Kind::Owner)
                .map_err(invariant)?;
        }
        for &(slot, span) in &self.parameters {
            ledger
                .initialize(c::Slot(slot), slot, Some(span))
                .map_err(|_| c::Cause::Execution(c::ExecutionFailure::Invariant))?;
        }
        Ok(())
    }
    fn charge(&mut self, _: m::Span) -> n::ResultStep {
        if self.fuel_limit.is_some_and(|limit| self.fuel >= limit) {
            return Err(c::Cause::Execution(c::ExecutionFailure::FuelLimit));
        }
        self.fuel += 1;
        Ok(())
    }
    fn move_slot(&mut self, source: usize, target: usize, _: m::Span) -> n::ResultStep {
        self.ledger
            .as_mut()
            .unwrap()
            .move_value(c::Slot(source), c::Slot(target))
            .map_err(invariant)
    }
    fn cleanup(&mut self, slots: &[(usize, m::Span)]) -> n::ResultStep {
        let ledger = self.ledger.as_mut().unwrap();
        ledger
            .cleanup_slots(slots.iter().map(|(slot, _)| c::Slot(*slot)))
            .map_err(invariant)?;
        if ledger.report().primary().is_some() {
            Err(c::Cause::Reported)
        } else {
            Ok(())
        }
    }
    fn stage_return(&mut self, slot: usize, _: m::Span) -> n::ResultStep {
        let ledger = self.ledger.as_mut().unwrap();
        let source = *ledger.value(c::Slot(slot)).map_err(invariant)?;
        {
            let mut state = self.storage.borrow_mut();
            let Storage {
                frames, allocator, ..
            } = &mut *state;
            let (from, to) = frames
                .two_cells_mut(source, self.return_cell)
                .map_err(|_| c::Cause::Execution(c::ExecutionFailure::Invariant))?;
            from.move_root_into(to, to.root(), allocator)
                .map_err(|_| c::Cause::Execution(c::ExecutionFailure::Invariant))?;
        }
        *ledger.value_mut(c::Slot(slot)).map_err(invariant)? = self.return_cell;
        ledger.stage_return(Some(c::Slot(slot))).map_err(invariant)
    }
    fn retire_return(&mut self) -> n::ResultStep {
        let ledger = self.ledger.as_mut().unwrap();
        ledger.retire_exit().map_err(invariant)?;
        if ledger.report().primary().is_some() {
            Err(c::Cause::Reported)
        } else {
            Ok(())
        }
    }
    fn publish_return(&mut self) -> n::ResultStep {
        self.published = true;
        Ok(())
    }
    fn record_failure(&mut self, cause: c::Cause, span: m::Span) -> n::ResultStep {
        self.ledger
            .as_mut()
            .unwrap()
            .begin_failure(cause, Some(span))
            .map_err(invariant)
    }
    fn retire_failure(&mut self) -> n::ResultStep {
        self.ledger
            .as_mut()
            .unwrap()
            .retire_exit()
            .map_err(invariant)
    }
    fn discard_result(&mut self) -> n::ResultStep {
        self.ledger
            .as_mut()
            .unwrap()
            .discard_result()
            .map_err(invariant)
    }
}

#[derive(Debug)]
struct Observed {
    value: Option<Vec<u8>>,
    retired: Vec<Vec<u8>>,
    fuel: u64,
    failure: Option<c::Cause>,
}

fn execute(
    text: &str,
    inputs: &[&[u8]],
    fuel_limit: Option<u64>,
    refuse_cleanup: bool,
) -> Observed {
    let program = source(text);
    let flow = program.failure_flow();
    let typed = flow.source().activity().moved().typed();
    let declarations = typed.places().declarations();
    let layouts = l::Layouts::compile(declarations.types(), Default::default()).unwrap();
    let compiled =
        CompiledFunction::compile(flow, &layouts, m::FunctionId(0), Default::default(), 1000)
            .unwrap();
    let plan = compiled.layout().frame();
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut allocator = a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap();
    let frames = plan.reserve(&mut allocator).unwrap();
    let declaration = &declarations.functions()[0];
    assert_eq!(inputs.len(), declaration.parameters.len());
    let parameters: Vec<_> = declaration
        .parameters
        .iter()
        .zip(inputs)
        .map(|(local, input)| {
            let cell = plan.local(*local).unwrap();
            let mut bytes = buffer::Bytes::from_slice(&mut allocator, input).unwrap();
            let token = bytes.take_token(&allocator).unwrap();
            frames
                .cell(cell)
                .unwrap()
                .data_mut(&mut allocator)
                .unwrap()
                .copy_from_slice(&token);
            (cell, declaration.locals[local.0 as usize].span)
        })
        .collect();
    let state = RefCell::new(Storage {
        frames,
        allocator: &mut allocator,
        expected: inputs,
        retired: Vec::with_capacity(inputs.len()),
        refuse_cleanup,
    });
    let mut instance = c::Instance::default();
    let ledger = instance
        .enter_cleaner(
            c::Limits {
                slots: plan.cell_count(),
                scopes: 1,
                failure_records: 16,
            },
            Cleaner(&state),
        )
        .unwrap();
    let mut runtime = RuntimeInstance::new(Frame {
        ledger: Some(ledger),
        storage: &state,
        parameters,
        return_cell: plan.return_cell(),
        fuel: 0,
        fuel_limit,
        published: false,
        expected_source: flow as *const _ as usize,
        expected_layouts: &layouts as *const _ as usize,
    });
    let result = compiled.invoke(&mut runtime);
    let fuel = runtime.runtime().fuel;
    assert_eq!(runtime.runtime().published, result.is_ok());
    assert_eq!(
        runtime.is_poisoned(),
        refuse_cleanup,
        "cleanup errors poison; ordinary guest failures preserve reuse"
    );
    if refuse_cleanup {
        assert_eq!(
            compiled.invoke(&mut runtime).unwrap_err().reason,
            n::Reason::Poisoned
        );
        assert_eq!(
            runtime.runtime().fuel,
            fuel,
            "poison rejection precedes callbacks"
        );
    }
    let ledger = runtime.runtime_mut().ledger.take().unwrap();
    let completion = if result.is_ok() {
        ledger.finish_staged_return()
    } else {
        ledger.finish_reported_failure()
    };
    assert_eq!(
        completion.poisoned, refuse_cleanup,
        "{:?}",
        completion.report
    );
    drop(runtime);
    assert_eq!(instance.is_poisoned(), refuse_cleanup);
    let mut state = state.into_inner();
    let value = completion.value.map(|cell| {
        assert_eq!(cell, plan.return_cell());
        let mut token: [u8; 32] = state
            .frames
            .cell(cell)
            .unwrap()
            .data(state.allocator)
            .unwrap()
            .try_into()
            .unwrap();
        let mut bytes = buffer::Bytes::from_token(&mut token, state.allocator).unwrap();
        state
            .frames
            .cell(cell)
            .unwrap()
            .data_mut(state.allocator)
            .unwrap()
            .fill(0);
        let data = bytes.as_slice(state.allocator).unwrap().to_vec();
        bytes.release(state.allocator).unwrap();
        data
    });
    state.frames.release(state.allocator).unwrap();
    assert_eq!(state.allocator.stats().live_allocations, 0);
    let trace = trace.borrow();
    assert_eq!(
        trace.allocated, trace.released,
        "each actual region released once"
    );
    Observed {
        value,
        retired: state
            .retired
            .into_iter()
            .map(|index| inputs[index].to_vec())
            .collect(),
        fuel,
        failure: result.err().map(|f| match f.reason {
            n::Reason::Runtime(cause) => cause,
            n::Reason::Poisoned => panic!("fresh instance"),
        }),
    }
}

const FORWARD: &str = include_str!("../examples/analysis/native_owned_boundary.lang");

#[path = "support/native_owned_calls.rs"]
mod calls;

#[test]
fn source_bytes_cross_the_native_parameter_and_return_boundary() {
    let input: &[u8] = &[0, 65, 0xfe, 0xff];
    let normal = execute(FORWARD, &[input], None, false);
    assert_eq!(normal.value.as_deref(), Some(input));
    assert!(normal.retired.is_empty());
    assert!(normal.fuel > 0);
    for limit in 0..normal.fuel {
        let failure = execute(FORWARD, &[input], Some(limit), false);
        assert!(failure.value.is_none());
        assert_eq!(failure.fuel, limit);
        assert_eq!(
            failure.failure,
            Some(c::Cause::Execution(c::ExecutionFailure::FuelLimit))
        );
        assert_eq!(failure.retired, [input]);
    }
}

#[test]
fn source_unused_owner_cleanup_precedes_native_return_publication() {
    let text =
        "fn select(value: core::Bytes, unused: core::Bytes) -> core::Bytes { return value; }";
    let normal = execute(text, &[b"kept", b"retired"], None, false);
    assert_eq!(normal.value.as_deref(), Some(b"kept".as_slice()));
    assert_eq!(normal.retired, [b"retired"]);
    let failure = execute(text, &[b"kept", b"retired"], None, true);
    assert!(failure.value.is_none());
    assert_eq!(failure.failure, Some(c::Cause::Reported));
    assert_eq!(failure.retired, [b"retired".to_vec(), b"kept".to_vec()]);
}

#[test]
fn unsupported_source_is_rejected_at_its_original_span() {
    let program = source(include_str!("../examples/scripts/shared.lang"));
    let flow = program.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let error =
        CompiledFunction::compile(flow, &layouts, m::FunctionId(0), Default::default(), 1000)
            .err()
            .unwrap();
    assert!(matches!(
        error,
        n::Error::Unsupported {
            span: m::Span {
                source: m::SourceId(0),
                ..
            },
            ..
        }
    ));
}

struct PanicPayload;
impl Drop for PanicPayload {
    fn drop(&mut self) {
        panic!("panic payload disposal");
    }
}
#[derive(Default)]
struct Panicking {
    entered: usize,
    recorded: usize,
    retired: usize,
    discarded: usize,
    unexpected: usize,
    payload_panics: bool,
    transition: Option<n::Transition>,
    prepared: usize,
}
impl Runtime for Panicking {
    fn is_poisoned(&self) -> bool {
        false
    }
    fn enter(&mut self, _: &n::FunctionLayout<'_>) -> n::ResultStep {
        self.entered += 1;
        if self.transition.is_some() {
            return Ok(());
        }
        if self.payload_panics {
            std::panic::panic_any(PanicPayload);
        }
        panic!("trusted runtime callback panic");
    }
    fn charge(&mut self, _: m::Span) -> n::ResultStep {
        self.unexpected += 1;
        Ok(())
    }
    fn move_slot(&mut self, _: usize, _: usize, _: m::Span) -> n::ResultStep {
        self.unexpected += 1;
        Ok(())
    }
    fn cleanup(&mut self, _: &[(usize, m::Span)]) -> n::ResultStep {
        self.unexpected += 1;
        Ok(())
    }
    fn stage_return(&mut self, _: usize, _: m::Span) -> n::ResultStep {
        self.unexpected += 1;
        Ok(())
    }
    fn retire_return(&mut self) -> n::ResultStep {
        self.unexpected += 1;
        Ok(())
    }
    fn publish_return(&mut self) -> n::ResultStep {
        self.unexpected += 1;
        Ok(())
    }
    fn record_failure(&mut self, cause: c::Cause, _: m::Span) -> n::ResultStep {
        assert_eq!(
            cause,
            c::Cause::Execution(if self.transition.is_some() {
                c::ExecutionFailure::Invariant
            } else {
                c::ExecutionFailure::HostPanic
            })
        );
        self.recorded += 1;
        Ok(())
    }
    fn retire_failure(&mut self) -> n::ResultStep {
        self.retired += 1;
        Ok(())
    }
    fn discard_result(&mut self) -> n::ResultStep {
        self.discarded += 1;
        Ok(())
    }
    fn prepare_call(
        &mut self,
        _: &n::CallSite,
        _: &n::FunctionLayout<'_>,
    ) -> Result<n::Transition, c::Cause> {
        self.prepared += 1;
        Ok(self.transition.unwrap())
    }
    fn validate_transition(&self, _: n::Transition) -> bool {
        true
    }
}

#[test]
fn native_dispatch_independently_rejects_bad_function_frame_and_continuation() {
    let program = source(
        "fn outer(value: core::Bytes) -> core::Bytes { return inner(value); } fn inner(value: core::Bytes) -> core::Bytes { return value; }",
    );
    let flow = program.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let compiled = n::CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    for target in [
        n::Transition {
            frame: 0,
            function: m::FunctionId(u32::MAX),
            continuation: 0,
        },
        n::Transition {
            frame: 7,
            function: m::FunctionId(1),
            continuation: 0,
        },
        n::Transition {
            frame: 0,
            function: m::FunctionId(1),
            continuation: u32::MAX,
        },
    ] {
        let mut instance = RuntimeInstance::new(Panicking {
            transition: Some(target),
            ..Default::default()
        });
        let failure = compiled
            .invoke(m::FunctionId(0), &mut instance)
            .unwrap_err();
        assert_eq!(
            failure.reason,
            n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::Invariant))
        );
        assert!(instance.is_poisoned());
        let runtime = instance.runtime();
        assert_eq!(
            (
                runtime.entered,
                runtime.prepared,
                runtime.recorded,
                runtime.retired,
                runtime.discarded
            ),
            (1, 1, 1, 1, 1)
        );
    }
}

#[test]
fn callback_and_panic_payload_panics_stay_inside_the_native_boundary() {
    let program = source(FORWARD);
    let flow = program.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let compiled =
        CompiledFunction::compile(flow, &layouts, m::FunctionId(0), Default::default(), 1000)
            .unwrap();
    for payload_panics in [false, true] {
        let mut instance = RuntimeInstance::new(Panicking {
            payload_panics,
            ..Default::default()
        });
        let failure = compiled.invoke(&mut instance).unwrap_err();
        assert_eq!(
            failure.reason,
            n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::HostPanic))
        );
        assert_eq!(failure.function, m::FunctionId(0));
        assert_eq!(failure.span.source, m::SourceId(0));
        assert!(instance.is_poisoned());
        assert_eq!(instance.quarantined_panics(), usize::from(payload_panics));
        let runtime = instance.runtime();
        assert_eq!(
            (
                runtime.entered,
                runtime.recorded,
                runtime.retired,
                runtime.discarded
            ),
            (1, 1, 1, 1)
        );
        assert_eq!(runtime.unexpected, 0);
        assert_eq!(
            compiled.invoke(&mut instance).unwrap_err().reason,
            n::Reason::Poisoned
        );
        assert_eq!(instance.runtime().entered, 1);
    }
}

#[test]
fn proof_layout_and_native_metadata_share_exact_compilation_budgets() {
    let program = source(FORWARD);
    let flow = program.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let compiled =
        CompiledFunction::compile(flow, &layouts, m::FunctionId(0), Default::default(), 1000)
            .unwrap();
    let usage = compiled.usage();
    let metadata = compiled.metadata();
    assert_eq!(metadata.function, m::FunctionId(0));
    assert_eq!(metadata.symbol, "lang_owned_r1_f0");
    assert!(!metadata.source_ranges.is_empty());
    for range in &metadata.source_ranges {
        assert!(range.start < range.end);
        assert!(range.end as usize <= metadata.code_size);
        assert!(
            flow.functions()[0]
                .nodes()
                .iter()
                .any(|node| node.span == range.span)
        );
        assert!(range.span.end as usize <= FORWARD.len());
    }
    for (exact, short) in [
        (
            m::Limits {
                work: usage.work,
                ..Default::default()
            },
            m::Limits {
                work: usage.work - 1,
                ..Default::default()
            },
        ),
        (
            m::Limits {
                allocations: usage.allocation_bytes,
                ..Default::default()
            },
            m::Limits {
                allocations: usage.allocation_bytes - 1,
                ..Default::default()
            },
        ),
        (
            m::Limits {
                elements: usage.elements,
                ..Default::default()
            },
            m::Limits {
                elements: usage.elements - 1,
                ..Default::default()
            },
        ),
    ] {
        assert_eq!(
            CompiledFunction::compile(flow, &layouts, m::FunctionId(0), exact, 1000)
                .unwrap()
                .usage(),
            usage
        );
        assert!(CompiledFunction::compile(flow, &layouts, m::FunctionId(0), short, 1000).is_err());
    }
    assert!(matches!(
        CompiledFunction::compile(
            flow,
            &layouts,
            m::FunctionId(u32::MAX),
            Default::default(),
            1000
        ),
        Err(n::Error::InvalidFunction(_))
    ));
    assert!(
        CompiledFunction::compile(flow, &layouts, m::FunctionId(0), Default::default(), 0).is_err()
    );
    let other = source(FORWARD);
    let other_types = other
        .failure_flow()
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let other_layouts = l::Layouts::compile(other_types, Default::default()).unwrap();
    assert!(matches!(
        CompiledFunction::compile(
            flow,
            &other_layouts,
            m::FunctionId(0),
            Default::default(),
            1000
        ),
        Err(n::Error::Layout(l::Error::ForeignLocation))
    ));
}
