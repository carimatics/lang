use super::*;
#[path = "native_owned_values.rs"]
mod values;
use n::{CallSite, CompiledProgram, Transition};

#[derive(Clone, Copy)]
struct Physical {
    frame: usize,
    cell: usize,
}
struct Store<'a, 'l> {
    pools: Vec<l::FrameStorage<'l, 'l>>,
    allocator: &'a mut a::Allocator<Provider>,
    expected: &'a [&'a [u8]],
    retired: Vec<usize>,
    refuse_cleanup: bool,
}
impl Store<'_, '_> {
    fn transfer(&mut self, from: Physical, to: Physical) -> n::ResultStep {
        let Store {
            pools, allocator, ..
        } = self;
        let result = if from.frame == to.frame {
            let (source, target) = pools[from.frame]
                .two_cells_mut(from.cell, to.cell)
                .map_err(|_| c::Cause::Execution(c::ExecutionFailure::Invariant))?;
            source
                .move_root_into(target, target.root(), allocator)
                .and_then(|()| source.recycle(allocator))
        } else if from.frame < to.frame {
            let (left, right) = pools.split_at_mut(to.frame);
            let source = left[from.frame].cell_mut(from.cell).unwrap();
            let target = right[0].cell(to.cell).unwrap();
            source
                .move_root_into(target, target.root(), allocator)
                .and_then(|()| source.recycle(allocator))
        } else {
            let (left, right) = pools.split_at_mut(from.frame);
            let source = right[0].cell_mut(from.cell).unwrap();
            let target = left[to.frame].cell(to.cell).unwrap();
            source
                .move_root_into(target, target.root(), allocator)
                .and_then(|()| source.recycle(allocator))
        };
        result.map_err(|_| c::Cause::Execution(c::ExecutionFailure::Invariant))
    }
}
struct Drain<'s, 'a, 'l>(&'s RefCell<Store<'a, 'l>>);
impl c::Cleaner<Physical> for Drain<'_, '_, '_> {
    fn cleanup(
        &mut self,
        physical: &mut Physical,
        _: c::Reason,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let mut storage = self.0.borrow_mut();
        let Store {
            pools,
            allocator,
            expected,
            retired,
            refuse_cleanup,
        } = &mut *storage;
        let cell = pools[physical.frame]
            .cell_mut(physical.cell)
            .map_err(|_| 90u32)?;
        cell.retire(allocator, context, |token, allocator, _, _| {
            let mut value =
                buffer::Bytes::from_token(&mut token.bytes, allocator).map_err(|_| 91u32)?;
            let data = value.as_slice(allocator).map_err(|_| 92u32)?;
            let index = expected
                .iter()
                .position(|candidate| *candidate == data)
                .ok_or(93u32)?;
            assert!(retired.len() < retired.capacity());
            retired.push(index);
            value.release(allocator).map_err(|_| 94u32)?;
            if *refuse_cleanup { Err(95) } else { Ok(()) }
        })
        .map_err(|_| 96u32)?;
        cell.recycle(allocator).map_err(|_| 97u32)
    }
}
#[derive(Clone, Copy)]
struct Parent {
    result: usize,
    resume: u32,
    failure: u32,
}
struct Active {
    handle: u64,
    function: m::FunctionId,
    parent: Option<Parent>,
}
struct Unit<'i, 's, 'a, 'l> {
    program: &'s CompiledProgram<'l>,
    store: &'s RefCell<Store<'a, 'l>>,
    ledger: Option<c::Frame<'i, Physical, Drain<'s, 'a, 'l>>>,
    stack: Vec<Active>,
    stride: usize,
    maximum_frames: usize,
    cell_limit: usize,
    live_cells: usize,
    peak_frames: usize,
    peak_cells: usize,
    handle: u64,
    pending: Option<Transition>,
    fuel: u64,
    fuel_limit: Option<u64>,
    published: bool,
}
impl Unit<'_, '_, '_, '_> {
    fn depth(&self) -> usize {
        self.stack.len() - 1
    }
    fn slot(&self, slot: usize) -> c::Slot {
        c::Slot(self.depth() * self.stride + slot)
    }
    fn plan(&self) -> &l::FramePlan<'_, '_> {
        self.program
            .layout(self.stack.last().unwrap().function)
            .unwrap()
            .frame()
    }
    fn retire_pool(&mut self, depth: usize) {
        let mut state = self.store.borrow_mut();
        let Store {
            pools, allocator, ..
        } = &mut *state;
        pools[depth].release(allocator).unwrap();
    }
}
impl Runtime for Unit<'_, '_, '_, '_> {
    fn is_poisoned(&self) -> bool {
        self.ledger.as_ref().unwrap().is_poisoned()
    }
    fn frame_identity(&self) -> u64 {
        self.stack.last().unwrap().handle
    }
    fn validate_transition(&self, target: Transition) -> bool {
        self.pending == Some(target)
            && self.frame_identity() == target.frame
            && self.stack.last().unwrap().function == target.function
    }
    fn enter(&mut self, layout: &n::FunctionLayout<'_>) -> n::ResultStep {
        let active = self.stack.last().unwrap();
        if active.function != layout.function()
            || !std::ptr::eq(self.program.layout(active.function).unwrap(), layout)
        {
            return Err(c::Cause::Execution(c::ExecutionFailure::Invariant));
        }
        if self.depth() == 0 {
            let ledger = self.ledger.as_mut().unwrap();
            for slot in 0..layout.frame().cell_count() {
                ledger
                    .declare(c::Slot(slot), c::Kind::Owner)
                    .map_err(invariant)?;
            }
            let typed = layout.source().source().activity().moved().typed();
            let function = &typed.places().declarations().functions()[active.function.0 as usize];
            for parameter in &function.parameters {
                let cell = layout.frame().local(*parameter).unwrap();
                ledger
                    .initialize(
                        c::Slot(cell),
                        Physical { frame: 0, cell },
                        Some(function.locals[parameter.0 as usize].span),
                    )
                    .map_err(|_| c::Cause::Execution(c::ExecutionFailure::Invariant))?;
            }
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
        let (source, target) = (self.slot(source), self.slot(target));
        self.ledger
            .as_mut()
            .unwrap()
            .move_value(source, target)
            .map_err(invariant)
    }
    fn cleanup(&mut self, slots: &[(usize, m::Span)]) -> n::ResultStep {
        let base = self.depth() * self.stride;
        let ledger = self.ledger.as_mut().unwrap();
        ledger
            .cleanup_slots(slots.iter().map(|(slot, _)| c::Slot(base + slot)))
            .map_err(invariant)
    }
    fn construct_record(&mut self, record: &n::RecordSite) -> n::ResultStep {
        values::aggregate(self, record, None)
    }
    fn read_scalar(&mut self, cell: usize) -> Result<i64, c::Cause> {
        values::read_scalar(self, cell)
    }
    fn write_scalar(&mut self, cell: usize, bits: i64, span: m::Span) -> n::ResultStep {
        values::write_scalar(self, cell, bits, span)
    }
    fn copy_slot(&mut self, from: usize, to: usize, span: m::Span) -> n::ResultStep {
        values::copy_slot(self, from, to, span)
    }
    fn variant_case(&mut self, cell: usize) -> Result<u32, c::Cause> {
        values::case(self, cell)
    }
    fn construct_variant(&mut self, site: &n::VariantSite) -> n::ResultStep {
        values::aggregate(self, &site.payload, Some(site.case))
    }
    fn extract_variant(&mut self, site: &n::ExtractSite) -> n::ResultStep {
        values::extract(self, site)
    }
    fn copy_case_field(&mut self, site: &n::CaseFieldSite) -> n::ResultStep {
        values::copy_field(self, site)
    }
    fn transfer(&mut self, site: &n::TransferSite) -> n::ResultStep {
        values::transfer(self, site)
    }
    fn stage_return(&mut self, slot: usize, _: m::Span) -> n::ResultStep {
        let copied = values::is_copy(self, slot);
        let slot = self.slot(slot);
        let source = *self
            .ledger
            .as_ref()
            .unwrap()
            .value(slot)
            .map_err(invariant)?;
        let target = Physical {
            frame: self.depth(),
            cell: self.plan().return_cell(),
        };
        if copied {
            values::copy_physical(&mut self.store.borrow_mut(), source, target)?;
        } else {
            self.store.borrow_mut().transfer(source, target)?;
        }
        let ledger = self.ledger.as_mut().unwrap();
        *ledger.value_mut(slot).map_err(invariant)? = target;
        ledger.stage_return(Some(slot)).map_err(invariant)
    }
    fn retire_return(&mut self) -> n::ResultStep {
        self.ledger
            .as_mut()
            .unwrap()
            .retire_exit()
            .map_err(invariant)
    }
    fn publish_return(&mut self) -> n::ResultStep {
        let depth = self.depth();
        let Some(parent) = self.stack[depth].parent else {
            self.published = true;
            self.pending = None;
            return Ok(());
        };
        let source = *self
            .ledger
            .as_mut()
            .unwrap()
            .staged_value_mut()
            .map_err(invariant)?
            .unwrap();
        let target = Physical {
            frame: depth - 1,
            cell: parent.result,
        };
        self.store.borrow_mut().transfer(source, target)?;
        let ledger = self.ledger.as_mut().unwrap();
        *ledger.staged_value_mut().map_err(invariant)?.unwrap() = target;
        ledger
            .publish_staged_to(Some(c::Slot((depth - 1) * self.stride + parent.result)))
            .map_err(invariant)?;
        self.live_cells -= self.plan().cell_count();
        self.stack.pop();
        self.pending = Some(Transition {
            frame: self.frame_identity(),
            function: self.stack.last().unwrap().function,
            continuation: parent.resume,
        });
        self.retire_pool(depth);
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
    fn prepare_call(
        &mut self,
        call: &CallSite,
        target: &n::FunctionLayout<'_>,
    ) -> Result<Transition, c::Cause> {
        let stable = self.program.layout(call.function).unwrap();
        if !std::ptr::eq(stable, target) {
            return Err(c::Cause::Execution(c::ExecutionFailure::Invariant));
        }
        let target = stable;
        let depth = self.stack.len();
        if depth == self.maximum_frames {
            return Err(c::Cause::Execution(c::ExecutionFailure::FrameLimit));
        }
        let live = self
            .live_cells
            .checked_add(target.frame().cell_count())
            .ok_or(c::Cause::Execution(c::ExecutionFailure::CounterOverflow))?;
        if live > self.cell_limit {
            return Err(c::Cause::Execution(c::ExecutionFailure::CellLimit));
        }
        let typed = target.source().source().activity().moved().typed();
        let function = &typed.places().declarations().functions()[target.function().0 as usize];
        assert_eq!(function.parameters.len(), call.arguments.len());
        // No host vectors grow and no caller ownership moves before admission.
        {
            let mut state = self.store.borrow_mut();
            let Store {
                pools, allocator, ..
            } = &mut *state;
            pools[depth]
                .admit(target.frame(), allocator)
                .map_err(|_| c::Cause::Execution(c::ExecutionFailure::Fault))?;
        }
        let caller_layout = self
            .program
            .layout(self.stack.last().unwrap().function)
            .unwrap();
        let types = typed.places().declarations().types();
        let caller_base = self.depth() * self.stride;
        let ledger = self.ledger.as_mut().unwrap();
        ledger.open_call().map_err(invariant)?;
        for slot in 0..target.frame().cell_count() {
            ledger
                .declare(c::Slot(depth * self.stride + slot), c::Kind::Owner)
                .map_err(invariant)?;
        }
        for (argument, parameter) in call.arguments.iter().zip(&function.parameters) {
            let slot = c::Slot(caller_base + argument);
            let source = *ledger.value(slot).map_err(invariant)?;
            let cell = target.frame().local(*parameter).unwrap();
            let destination = Physical { frame: depth, cell };
            let copied = types
                .properties(caller_layout.frame().cell_type(*argument).unwrap().ty)
                .unwrap()
                .is_copy;
            if copied {
                values::copy_physical(&mut self.store.borrow_mut(), source, destination)?;
            } else {
                self.store.borrow_mut().transfer(source, destination)?;
                ledger.take_owner(slot).map_err(invariant)?;
            }
            ledger
                .initialize(
                    c::Slot(depth * self.stride + cell),
                    destination,
                    Some(function.locals[parameter.0 as usize].span),
                )
                .map_err(|_| c::Cause::Execution(c::ExecutionFailure::Invariant))?;
        }
        self.handle += 1;
        assert!(self.stack.len() < self.stack.capacity());
        self.stack.push(Active {
            handle: self.handle,
            function: call.function,
            parent: Some(Parent {
                result: call.result,
                resume: call.resume,
                failure: call.failure,
            }),
        });
        self.live_cells = live;
        self.peak_frames = self.peak_frames.max(self.stack.len());
        self.peak_cells = self.peak_cells.max(live);
        let next = Transition {
            frame: self.handle,
            function: call.function,
            continuation: 0,
        };
        self.pending = Some(next);
        Ok(next)
    }
    fn after_return(&mut self) -> Result<Option<Transition>, c::Cause> {
        Ok(self.pending)
    }
    fn after_failure(&mut self) -> Result<Option<Transition>, c::Cause> {
        let depth = self.depth();
        let Some(parent) = self.stack[depth].parent else {
            return Ok(None);
        };
        self.ledger
            .as_mut()
            .unwrap()
            .propagate_call_failure()
            .map_err(invariant)?;
        self.live_cells -= self.plan().cell_count();
        self.stack.pop();
        let next = Transition {
            frame: self.frame_identity(),
            function: self.stack.last().unwrap().function,
            continuation: parent.failure,
        };
        self.pending = Some(next);
        self.retire_pool(depth);
        Ok(Some(next))
    }
}

struct Seen {
    value: Option<Vec<u8>>,
    scalar: Option<i64>,
    retired: Vec<usize>,
    fuel: u64,
    peak: usize,
    peak_cells: usize,
    failure: Option<n::Failure>,
    report: c::Report,
    requests: usize,
}
fn execute_unit(
    text: &str,
    inputs: &[&[u8]],
    fuel: Option<u64>,
    max_frames: usize,
    cells: usize,
    refuse: Option<usize>,
    refuse_cleanup: bool,
) -> Seen {
    let source = source(text);
    execute_flow(
        source.failure_flow(),
        inputs,
        fuel,
        max_frames,
        cells,
        refuse,
        refuse_cleanup,
    )
}
fn execute_flow(
    flow: &m::ownership::failure::Elaborated,
    inputs: &[&[u8]],
    fuel: Option<u64>,
    max_frames: usize,
    cells: usize,
    refuse: Option<usize>,
    refuse_cleanup: bool,
) -> Seen {
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let program = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let largest = (0..flow.functions().len())
        .map(|f| program.layout(m::FunctionId(f as u32)).unwrap().frame())
        .max_by_key(|p| p.cell_count())
        .unwrap();
    let stride = largest.cell_count();
    let mut usage = program.usage();
    let mut pools = Vec::with_capacity(max_frames);
    for _ in 0..max_frames {
        let pool = largest.prepare_pool_after(usage).unwrap();
        usage = pool.usage();
        pools.push(pool);
    }
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut allocator = a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap();
    let root = program.layout(m::FunctionId(0)).unwrap();
    pools[0].admit(root.frame(), &mut allocator).unwrap();
    let typed = flow.source().activity().moved().typed();
    let declaration = &typed.places().declarations().functions()[0];
    for (local, input) in declaration.parameters.iter().zip(inputs) {
        let cell = root.frame().local(*local).unwrap();
        let mut value = buffer::Bytes::from_slice(&mut allocator, input).unwrap();
        let token = value.take_token(&allocator).unwrap();
        pools[0]
            .cell(cell)
            .unwrap()
            .data_mut(&mut allocator)
            .unwrap()
            .copy_from_slice(&token);
    }
    let initial_requests = trace.borrow().requests;
    trace.borrow_mut().refuse = refuse.map(|n| initial_requests + n);
    let store = RefCell::new(Store {
        pools,
        allocator: &mut allocator,
        expected: inputs,
        retired: Vec::with_capacity(inputs.len()),
        refuse_cleanup,
    });
    let mut cleanup_instance = c::Instance::default();
    let ledger = cleanup_instance
        .enter_cleaner(
            c::Limits {
                slots: stride * max_frames,
                scopes: max_frames,
                failure_records: 128,
            },
            Drain(&store),
        )
        .unwrap();
    let mut stack = Vec::with_capacity(max_frames);
    stack.push(Active {
        handle: 1,
        function: m::FunctionId(0),
        parent: None,
    });
    let mut instance = RuntimeInstance::new(Unit {
        program: &program,
        store: &store,
        ledger: Some(ledger),
        stack,
        stride,
        maximum_frames: max_frames,
        cell_limit: cells,
        live_cells: root.frame().cell_count(),
        peak_frames: 1,
        peak_cells: root.frame().cell_count(),
        handle: 1,
        pending: None,
        fuel: 0,
        fuel_limit: fuel,
        published: false,
    });
    let result = program.invoke(m::FunctionId(0), &mut instance);
    assert_eq!(instance.runtime().published, result.is_ok());
    assert_eq!(instance.is_poisoned(), refuse_cleanup);
    let fuel = instance.runtime().fuel;
    let peak = instance.runtime().peak_frames;
    let peak_cells = instance.runtime().peak_cells;
    assert_eq!(
        instance.runtime().stack.len(),
        1,
        "all logical callees unwind"
    );
    let ledger = instance.runtime_mut().ledger.take().unwrap();
    let completion = if result.is_ok() {
        ledger.finish_staged_return()
    } else {
        ledger.finish_reported_failure()
    };
    drop(instance);
    let mut state = store.into_inner();
    let mut scalar = None;
    let value = completion.value.and_then(|physical| {
        assert_eq!(
            (physical.frame, physical.cell),
            (0, root.frame().return_cell())
        );
        if root.frame().cell_type(physical.cell).unwrap().ty == m::ownership::Type::I64 {
            let cell = state.pools[0].cell(physical.cell).unwrap();
            let l::Immediate::I64(value) =
                cell.read_immediate(cell.root(), state.allocator).unwrap()
            else {
                panic!("i64 result")
            };
            scalar = Some(value);
            return None;
        }
        let mut token: [u8; 32] = state.pools[0]
            .cell(physical.cell)
            .unwrap()
            .data(state.allocator)
            .unwrap()
            .try_into()
            .unwrap();
        let mut value = buffer::Bytes::from_token(&mut token, state.allocator).unwrap();
        state.pools[0]
            .cell(physical.cell)
            .unwrap()
            .data_mut(state.allocator)
            .unwrap()
            .fill(0);
        let bytes = value.as_slice(state.allocator).unwrap().to_vec();
        value.release(state.allocator).unwrap();
        Some(bytes)
    });
    state.pools[0].release(state.allocator).unwrap();
    assert_eq!(state.allocator.stats().live_allocations, 0);
    let trace = trace.borrow();
    assert_eq!(trace.allocated, trace.released);
    Seen {
        value,
        scalar,
        retired: state.retired,
        fuel,
        peak,
        peak_cells,
        failure: result.err(),
        report: completion.report,
        requests: trace.requests - initial_requests,
    }
}

const CALLS: &str = include_str!("../../examples/analysis/native_owned_calls.lang");
const RECORDS: &str = include_str!("../../examples/analysis/native_owned_records.lang");

struct MarkerProvider {
    retired: Rc<RefCell<Vec<usize>>>,
    trace: Rc<RefCell<Trace>>,
}
const RECORD_MARKER: [u8; 7] = [251, 211, 97, 182, 208, 173, 234];
impl a::Provider for MarkerProvider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<a::Storage> {
        let region = a::Storage::try_new(request).ok()?;
        self.trace.borrow_mut().allocated += 1;
        Some(region)
    }
    fn deallocate(&mut self, region: a::Storage, _: a::Request) {
        use a::Region;
        // Observe complete guest payload tags through the provider boundary;
        // no buffer-header size, token representation or alignment is assumed.
        for marker in region
            .bytes()
            .windows(8)
            .filter(|w| w[..7] == RECORD_MARKER)
        {
            let mut retired = self.retired.borrow_mut();
            assert!(retired.len() < retired.capacity());
            retired.push(usize::from(marker[7]));
        }
        self.trace.borrow_mut().released += 1;
    }
}
fn vm_record_order(text: &str, arguments: usize) -> Vec<usize> {
    let mut wrapped = text.to_owned();
    wrapped.push_str("\nfn main() -> i64 {\n");
    for i in 0..arguments {
        wrapped.push_str(&format!(
            "let mut v{i}: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));\n",
        ));
        for (j, byte) in RECORD_MARKER
            .into_iter()
            .chain(std::iter::once(i as u8))
            .enumerate()
        {
            wrapped.push_str(&format!("let mut pushed{i}_{j}: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(v{i}, {byte}));\n"));
        }
    }
    let names = (0..arguments)
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    wrapped.push_str(&format!(
        "let mut kept: core::Bytes = relay({names}); return core::bytes_len(kept); }}\n"
    ));
    let program = source(&wrapped);
    let retired = Rc::new(RefCell::new(Vec::with_capacity(arguments)));
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut allocator = a::Allocator::new(
        MarkerProvider {
            retired: retired.clone(),
            trace: trace.clone(),
        },
        Default::default(),
    )
    .unwrap();
    let mut instance = c::Instance::default();
    let outcome = lang_vm::unit::owned::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Default::default(),
    )
    .unwrap();
    assert_eq!(outcome.value, 8);
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(trace.borrow().allocated, trace.borrow().released);
    drop(allocator);
    Rc::try_unwrap(retired).unwrap().into_inner()
}

#[test]
fn source_native_owned_records_keep_initializer_order_through_calls_and_failures() {
    let values: &[&[u8]] = &[b"first", b"second", b"kept"];
    let normal = execute_unit(RECORDS, values, None, 4, 1000, None, false);
    assert_eq!(normal.value.as_deref(), Some(values[2]));
    assert_eq!(normal.retired, [0, 1], "reverse source initializer order");
    let mut native_order = normal.retired.clone();
    native_order.push(2); // The caller releases the returned Bytes last.
    assert_eq!(native_order, vm_record_order(RECORDS, 3));
    for fuel in 0..normal.fuel {
        let failed = execute_unit(RECORDS, values, Some(fuel), 4, 1000, None, false);
        assert!(failed.value.is_none());
        assert_eq!(failed.fuel, fuel);
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(
            retired,
            [0, 1, 2],
            "every owner is retired once at fuel {fuel}"
        );
        assert!(failed.report.secondary().is_empty());
    }
    for request in 1..=normal.requests {
        let failed = execute_unit(RECORDS, values, None, 4, 1000, Some(request), false);
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(retired, [0, 1, 2]);
        assert!(failed.value.is_none());
    }
    let failed = execute_unit(RECORDS, values, None, 4, 1000, None, true);
    assert!(failed.value.is_none());
    assert_eq!(failed.retired, [0, 1, 2]);
}

#[test]
fn nested_native_records_match_vm_for_each_inner_and_outer_initializer_order() {
    let template = "struct Inner { first: core::Bytes, second: core::Bytes }\n\
        struct Outer { pair: Inner, third: core::Bytes }\n\
        fn relay(first: core::Bytes, second: core::Bytes, third: core::Bytes, kept: core::Bytes) -> core::Bytes {\n\
            return sink(Outer { $OUTER }, kept);\n\
        }\n\
        fn sink(unused: Outer, kept: core::Bytes) -> core::Bytes { return kept; }\n";
    let values: &[&[u8]] = &[b"A", b"B", b"C", b"D"];
    for inner_reversed in [false, true] {
        for outer_reversed in [false, true] {
            let inner = if inner_reversed {
                "second: second, first: first"
            } else {
                "first: first, second: second"
            };
            let outer = if outer_reversed {
                "third: third, pair: Inner { $INNER }"
            } else {
                "pair: Inner { $INNER }, third: third"
            };
            let text = template.replace("$OUTER", outer).replace("$INNER", inner);
            let normal = execute_unit(&text, values, None, 4, 1000, None, false);
            assert_eq!(normal.value.as_deref(), Some(values[3]));
            let mut expected = if inner_reversed {
                vec![0, 1]
            } else {
                vec![1, 0]
            };
            if outer_reversed {
                expected.push(2);
            } else {
                expected.insert(0, 2);
            }
            assert_eq!(normal.retired, expected);
            expected.push(3);
            assert_eq!(expected, vm_record_order(&text, 4));
            for fuel in 0..normal.fuel {
                let failed = execute_unit(&text, values, Some(fuel), 4, 1000, None, false);
                let mut retired = failed.retired;
                retired.sort_unstable();
                assert_eq!(retired, [0, 1, 2, 3]);
                assert!(failed.value.is_none());
                assert!(failed.report.secondary().is_empty());
            }
        }
    }
}
#[test]
fn source_owned_guest_calls_return_and_unwind_all_native_continuations() {
    let values: &[&[u8]] = &[b"kept", b"child", b"parent"];
    let normal = execute_unit(CALLS, values, None, 4, 1000, None, false);
    assert_eq!(normal.value.as_deref(), Some(values[0]));
    assert_eq!(normal.retired, [1, 2]);
    assert_eq!(normal.peak, 2);
    for fuel in 0..normal.fuel {
        let failed = execute_unit(CALLS, values, Some(fuel), 4, 1000, None, false);
        assert!(failed.value.is_none());
        assert_eq!(failed.fuel, fuel);
        assert_eq!(
            failed.failure.unwrap().reason,
            n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::FuelLimit))
        );
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(retired, [0, 1, 2]);
        assert!(
            failed.report.secondary().is_empty(),
            "propagated failure is not duplicated"
        );
    }
    let failed = execute_unit(CALLS, values, None, 4, 1000, None, true);
    assert!(failed.value.is_none());
    assert_eq!(
        failed.retired,
        [1, 0, 2],
        "poison still retires the admitted caller"
    );
}

#[test]
fn source_native_call_admission_refuses_before_consuming_caller_arguments() {
    let values: &[&[u8]] = &[b"kept", b"child", b"parent"];
    let normal = execute_unit(CALLS, values, None, 4, 1000, None, false);
    for request in 1..=normal.requests {
        let failed = execute_unit(CALLS, values, None, 4, 1000, Some(request), false);
        assert!(failed.value.is_none());
        assert_eq!(failed.peak, 1);
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(retired, [0, 1, 2]);
    }
    let frame = execute_unit(CALLS, values, None, 1, 1000, None, false);
    assert_eq!(frame.requests, 0);
    assert_eq!(
        frame.failure.unwrap().reason,
        n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::FrameLimit))
    );
    let exact_cells = execute_unit(CALLS, values, None, 4, normal.peak_cells, None, false);
    assert_eq!(exact_cells.value.as_deref(), Some(values[0]));
    let short_cells = execute_unit(CALLS, values, None, 4, normal.peak_cells - 1, None, false);
    assert_eq!(short_cells.requests, 0);
    assert_eq!(
        short_cells.failure.unwrap().reason,
        n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::CellLimit))
    );
}

#[test]
fn recursive_native_calls_use_logical_frames_and_contain_the_frame_limit() {
    let text = "fn forever(value: core::Bytes) -> core::Bytes { return forever(value); }";
    let failed = execute_unit(text, &[b"one owner"], None, 64, 100000, None, false);
    assert_eq!(failed.peak, 64);
    assert_eq!(failed.retired, [0]);
    assert_eq!(
        failed.failure.unwrap().reason,
        n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::FrameLimit))
    );
    assert!(failed.report.secondary().is_empty());
}

#[test]
fn owned_native_objects_keep_matching_call_layout_and_source_metadata() {
    for text in [CALLS, RECORDS, values::SUMS] {
        check_native_object_metadata(text);
    }
}
fn check_native_object_metadata(text: &str) {
    use object::{Object, ObjectSymbol};
    let source = source(text);
    let flow = source.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    for name in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let target = name.parse().unwrap();
        let artifact =
            n::emit_object_for_target(flow, &layouts, &target, Default::default(), 1000).unwrap();
        let object = object::File::parse(&*artifact.bytes).unwrap();
        let mut record_sites = 0;
        let mut variant_sites = 0;
        let mut extract_sites = 0;
        for index in 0..flow.functions().len() {
            let function = artifact.function(m::FunctionId(index as u32)).unwrap();
            let metadata = function.metadata();
            assert!(object.symbols().any(|s| {
                s.is_definition()
                    && s.kind() == object::SymbolKind::Text
                    && s.name()
                        .is_ok_and(|name| name.trim_start_matches('_') == metadata.symbol)
            }));
            let plan = function.layout().frame();
            assert_eq!(
                layouts
                    .get(plan.cell_type(plan.return_cell()).unwrap().ty)
                    .unwrap()
                    .size,
                32
            );
            assert!(!metadata.source_ranges.is_empty());
            for range in &metadata.source_ranges {
                assert!(range.start < range.end && range.end as usize <= metadata.code_size);
                assert!(
                    flow.functions()[index]
                        .nodes()
                        .iter()
                        .any(|n| n.span == range.span)
                );
            }
            for (node, source) in flow.functions()[index].nodes().iter().enumerate() {
                assert_eq!(function.span(node as u32), Some(source.span));
                if let Some(record) = function.record_site(node as u32) {
                    record_sites += 1;
                    assert_eq!(record.span, source.span);
                    assert_eq!(record.fields.len(), 2);
                    assert_eq!(
                        record
                            .initialization_order
                            .iter()
                            .map(|(field, _)| *field)
                            .collect::<Vec<_>>(),
                        [1, 0]
                    );
                    for ((_, span), expected) in
                        record.initialization_order.iter().zip(["second", "first"])
                    {
                        assert_eq!(&text[span.start as usize..span.end as usize], expected);
                    }
                }
                if let Some(variant) = function.variant_site(node as u32) {
                    variant_sites += 1;
                    assert_eq!(variant.case, 1);
                    assert_eq!(variant.payload.span, source.span);
                    assert_eq!(variant.payload.fields.len(), 1);
                    let (field, span) = variant.payload.initialization_order[0];
                    assert_eq!(field, 0);
                    assert_eq!(&text[span.start as usize..span.end as usize], "value");
                }
                if let Some(extract) = function.extract_site(node as u32) {
                    extract_sites += 1;
                    assert_eq!(extract.case, 1);
                    assert_eq!(extract.guard, None);
                    assert_eq!(extract.panic_code, Some(-1));
                }
            }
        }
        let root = artifact.function(m::FunctionId(0)).unwrap();
        let calls: Vec<_> = (0..flow.functions()[0].nodes().len())
            .filter_map(|node| root.call_site(node as u32))
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function, m::FunctionId(1));
        assert_eq!(calls[0].arguments.len(), 2);
        assert!(root.continuations().contains(&calls[0].resume));
        assert!(root.continuations().contains(&calls[0].failure));
        assert_eq!(record_sites, usize::from(text == RECORDS));
        assert_eq!(variant_sites, usize::from(text == values::SUMS));
        assert_eq!(extract_sites, usize::from(text == values::SUMS));
        let usage = artifact.usage();
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
                n::emit_object_for_target(flow, &layouts, &target, exact, 1000)
                    .unwrap()
                    .usage(),
                usage
            );
            assert!(n::emit_object_for_target(flow, &layouts, &target, short, 1000).is_err());
        }
    }
    let program = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let usage = program.usage();
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
            CompiledProgram::compile(flow, &layouts, exact, 1000)
                .unwrap()
                .usage(),
            usage
        );
        assert!(CompiledProgram::compile(flow, &layouts, short, 1000).is_err());
    }
}
