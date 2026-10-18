use super::*;
#[path = "native_arenas.rs"]
mod arenas;
#[path = "native_control.rs"]
mod control;
#[path = "native_core.rs"]
mod core;
#[path = "native_maps.rs"]
mod maps;
#[path = "native_places.rs"]
mod places;
#[path = "native_shared.rs"]
mod shared;
#[path = "native_owned_values.rs"]
mod values;
#[path = "native_vectors.rs"]
mod vectors;
use lang_runtime::execution as e;
use n::{CallSite, CompiledProgram, Transition};

#[derive(Clone, Copy)]
struct Physical {
    frame: usize,
    cell: usize,
}
#[derive(Clone, Copy)]
enum Ticket {
    Value(Physical),
    Loan(usize),
}
impl Ticket {
    fn physical(self) -> Result<Physical, c::Cause> {
        match self {
            Self::Value(value) => Ok(value),
            Self::Loan(_) => Err(c::Cause::Execution(c::ExecutionFailure::Invariant)),
        }
    }
}
struct Store<'a, 'l> {
    pools: Vec<l::FrameStorage<'l, 'l>>,
    allocator: &'a mut a::Allocator<Provider>,
    expected: &'a [&'a [u8]],
    retired: Vec<usize>,
    refuse_cleanup: bool,
    loans: Vec<places::Loan>,
    prepared: Vec<Option<places::Capture>>,
    loan_stride: usize,
    prepared_stride: usize,
    types: &'l m::ownership::VerifiedTypes,
    layouts: &'l l::Layouts<'l>,
    retirement: lang_runtime::retirement::Drain<'l, 'l>,
    heap: Option<lang_runtime::shared::managed::Heap<'l, 'l>>,
    arena: Vec<Option<lang_runtime::arena::managed::Arena<'l, 'l>>>,
    arena_live: Vec<bool>,
    arena_stride: usize,
    inspection: Option<lang_runtime::inspection::Inspector<'l, 'l>>,
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
impl c::Cleaner<Ticket> for Drain<'_, '_, '_> {
    fn cleanup(
        &mut self,
        ticket: &mut Ticket,
        _: c::Reason,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let mut storage = self.0.borrow_mut();
        let physical = match ticket {
            Ticket::Loan(id) => {
                let Store {
                    loans,
                    layouts,
                    allocator,
                    heap,
                    retirement,
                    types,
                    arena,
                    ..
                } = &mut *storage;
                let target = loans[*id].target.take();
                loans[*id].custody = None;
                if let Some(mut read) = loans[*id].vector_read.take() {
                    read.end(layouts, allocator).map_err(|_| 98u32)?;
                }
                if let Some(mut read) = loans[*id].map_read.take() {
                    read.end(layouts, allocator).map_err(|_| 99u32)?;
                }
                if let Some((owner, mut read)) = loans[*id].arena_read.take() {
                    arena[owner]
                        .as_mut()
                        .ok_or(103u32)?
                        .end_read(&mut read, allocator)
                        .map_err(|_| 104u32)?;
                }
                if let Some(read) = loans[*id].shared_read.take() {
                    let Some(places::Target::Shared { ty, .. }) = target else {
                        return Err(100);
                    };
                    retirement
                        .end_read(
                            read,
                            ty,
                            heap.as_mut().ok_or(101u32)?,
                            allocator,
                            context,
                            |token, allocator, _, _| {
                                buffer::cleanup_token(types, token, allocator).map_err(|_| 94u32)
                            },
                        )
                        .map_err(|_| 102u32)?;
                }
                return Ok(());
            }
            Ticket::Value(value) => value,
        };
        let Store {
            pools,
            allocator,
            expected,
            retired,
            refuse_cleanup,
            types,
            retirement,
            heap,
            arena,
            arena_live,
            arena_stride,
            ..
        } = &mut *storage;
        let owner_type = pools[physical.frame]
            .cell(physical.cell)
            .map_err(|_| 90u32)?
            .root()
            .ty();
        if matches!(owner_type, m::ownership::Type::Constructed(id)
            if matches!(types.table().constructed[id.0 as usize].constructor, m::ownership::Constructor::ArenaOwner(_)))
        {
            let id = physical.frame * *arena_stride + physical.cell;
            if !arena_live[id] {
                return Err(105);
            }
            arena[id]
                .as_mut()
                .ok_or(106u32)?
                .cleanup(
                    if context.reason() == c::Reason::Failure {
                        lang_runtime::arena::managed::Exit::Abort
                    } else {
                        lang_runtime::arena::managed::Exit::Normal
                    },
                    allocator,
                    context,
                    |token, allocator, _, context| {
                        retirement.token(
                            token,
                            heap.as_mut(),
                            allocator,
                            context,
                            |leaf, allocator, _, _| {
                                buffer::cleanup_token(types, leaf, allocator).map_err(|_| 94u32)
                            },
                        )
                    },
                )
                .map_err(|_| 107u32)?;
            arena_live[id] = false;
            return if *refuse_cleanup { Err(95) } else { Ok(()) };
        }
        let cell = pools[physical.frame]
            .cell_mut(physical.cell)
            .map_err(|_| 90u32)?;
        cell.retire(allocator, context, |token, allocator, _, context| {
            if expected.is_empty() {
                retirement.token(
                    token,
                    heap.as_mut(),
                    allocator,
                    context,
                    |leaf, allocator, _, _| {
                        buffer::cleanup_token(types, leaf, allocator).map_err(|_| 94u32)
                    },
                )?;
                return if *refuse_cleanup { Err(95) } else { Ok(()) };
            }
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
    ledger: Option<c::Frame<'i, Ticket, Drain<'s, 'a, 'l>>>,
    stack: Vec<Active>,
    stride: usize,
    maximum_frames: usize,
    cell_limit: usize,
    live_cells: usize,
    usage: e::Usage,
    control: e::Control<'s>,
    handle: u64,
    pending: Option<Transition>,
    published: bool,
    service_metering: bool,
}
impl Unit<'_, '_, '_, '_> {
    fn work(&mut self) -> impl FnMut(u64) -> Result<(), e::Failure> + '_ {
        let control = self.control;
        let enabled = self.service_metering;
        let usage = &mut self.usage;
        move |cost| {
            if enabled {
                control.charge_service_work(usage, cost)
            } else {
                Ok(())
            }
        }
    }

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
            for loan in 0..typed.functions()[active.function.0 as usize].loans {
                ledger
                    .declare(
                        c::Slot(
                            layout
                                .loan_slot(m::ownership::bodies::LoanId(loan as u32))
                                .unwrap(),
                        ),
                        c::Kind::Borrow,
                    )
                    .map_err(invariant)?;
            }
            let function = &typed.places().declarations().functions()[active.function.0 as usize];
            for parameter in &function.parameters {
                let cell = layout.frame().local(*parameter).unwrap();
                ledger
                    .initialize(
                        c::Slot(cell),
                        Ticket::Value(Physical { frame: 0, cell }),
                        Some(function.locals[parameter.0 as usize].span),
                    )
                    .map_err(|_| c::Cause::Execution(c::ExecutionFailure::Invariant))?;
            }
        }
        Ok(())
    }
    fn charge(&mut self, _: m::Span) -> n::ResultStep {
        self.control.charge(&mut self.usage).map_err(control_cause)
    }
    fn move_slot(&mut self, source: usize, target: usize, span: m::Span) -> n::ResultStep {
        values::move_value(self, source, target, span)
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
        places::variant_case(self, cell)
    }
    fn construct_variant(&mut self, site: &n::VariantSite) -> n::ResultStep {
        values::aggregate(self, &site.payload, Some(site.case))
    }
    fn extract_variant(&mut self, site: &n::ExtractSite) -> n::ResultStep {
        values::extract(self, site)
    }
    fn copy_case_field(&mut self, site: &n::CaseFieldSite) -> n::ResultStep {
        places::copy_field(self, site)
    }
    fn transfer(&mut self, site: &n::TransferSite) -> n::ResultStep {
        values::transfer(self, site)
    }
    fn load_place(&mut self, site: &n::PlaceLoad) -> n::ResultStep {
        places::load(self, site)
    }
    fn borrow_place(&mut self, site: &n::BorrowSite) -> n::ResultStep {
        places::borrow(self, site)
    }
    fn borrow_case(&mut self, site: &n::BorrowCaseSite) -> n::ResultStep {
        places::borrow_case(self, site)
    }
    fn end_borrow(
        &mut self,
        loan: m::ownership::bodies::LoanId,
        mode: m::ownership::bodies::EndMode,
    ) -> n::ResultStep {
        places::end(self, loan, mode)
    }
    fn prepare_place(&mut self, site: &n::PreparedSite) -> n::ResultStep {
        places::prepare(self, site)
    }
    fn replace_place(&mut self, site: &n::ReplaceSite) -> n::ResultStep {
        places::replace(self, site)
    }
    fn discard_prepared(&mut self, id: Option<m::ownership::bodies::PreparedPlaceId>) {
        places::discard(self, id)
    }
    fn enter_arena(&mut self, site: &n::ArenaSite) -> n::ResultStep {
        arenas::enter(self, site)
    }
    fn core_call(&mut self, site: &n::CoreSite) -> n::ResultStep {
        if arenas::supports(site.operation) {
            arenas::call(self, site)
        } else if shared::supports(site.operation) {
            shared::call(self, site)
        } else if maps::supports(site.operation) {
            maps::call(self, site)
        } else if vectors::supports(site.operation) {
            vectors::call(self, site)
        } else {
            core::call(self, site)
        }
    }
    fn stage_return(&mut self, slot: usize, _: m::Span) -> n::ResultStep {
        let copied = values::is_copy(self, slot);
        let slot = self.slot(slot);
        let source = self
            .ledger
            .as_ref()
            .unwrap()
            .value(slot)
            .map_err(invariant)?
            .physical()?;
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
        *ledger.value_mut(slot).map_err(invariant)? = Ticket::Value(target);
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
        let source = self
            .ledger
            .as_mut()
            .unwrap()
            .staged_value_mut()
            .map_err(invariant)?
            .unwrap()
            .physical()?;
        let target = Physical {
            frame: depth - 1,
            cell: parent.result,
        };
        self.store.borrow_mut().transfer(source, target)?;
        let ledger = self.ledger.as_mut().unwrap();
        *ledger.staged_value_mut().map_err(invariant)?.unwrap() = Ticket::Value(target);
        ledger
            .publish_staged_to(Some(c::Slot((depth - 1) * self.stride + parent.result)))
            .map_err(invariant)?;
        self.live_cells -= self
            .program
            .layout(self.stack.last().unwrap().function)
            .unwrap()
            .control_cells();
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
        self.control.check().map_err(control_cause)?;
        let depth = self.stack.len();
        let next_handle = self
            .handle
            .checked_add(1)
            .ok_or(c::Cause::Execution(c::ExecutionFailure::CounterOverflow))?;
        if depth >= self.maximum_frames {
            return Err(c::Cause::Execution(c::ExecutionFailure::FrameLimit));
        }
        let live = self
            .live_cells
            .checked_add(target.control_cells())
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
        for loan in 0..typed.functions()[target.function().0 as usize].loans {
            ledger
                .declare(
                    c::Slot(
                        depth * self.stride
                            + target
                                .loan_slot(m::ownership::bodies::LoanId(loan as u32))
                                .unwrap(),
                    ),
                    c::Kind::Borrow,
                )
                .map_err(invariant)?;
        }
        for (argument, parameter) in call.arguments.iter().zip(&function.parameters) {
            let slot = c::Slot(caller_base + argument);
            let source = ledger.value(slot).map_err(invariant)?.physical()?;
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
                    Ticket::Value(destination),
                    Some(function.locals[parameter.0 as usize].span),
                )
                .map_err(|_| c::Cause::Execution(c::ExecutionFailure::Invariant))?;
        }
        self.handle = next_handle;
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
        self.usage.peak_frames = self.usage.peak_frames.max(self.stack.len());
        self.usage.peak_live_cells = self.usage.peak_live_cells.max(live);
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
        self.live_cells -= self
            .program
            .layout(self.stack.last().unwrap().function)
            .unwrap()
            .control_cells();
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
    total_requests: usize,
    usage: e::Usage,
}

#[test]
fn native_control_usage_includes_loans_and_return_escrow() {
    let text = include_str!("../../examples/scripts/utf8.lang");
    let native = execute_unit(text, &[], None, 8, 1000, None, false);
    let source = source(text);
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let vm = lang_vm::unit::owned::execute_entry(
        source.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Default::default(),
    )
    .unwrap();
    assert_eq!(native.scalar, Some(vm.value));
    assert_eq!(native.fuel, vm.usage.fuel);
    assert_eq!(native.peak, vm.usage.peak_frames);
    assert_eq!(native.peak_cells, vm.usage.peak_live_cells);
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
    execute_prepared(
        flow,
        &layouts,
        &program,
        Run {
            inputs,
            fuel,
            max_frames,
            cells,
            refuse,
            refuse_cleanup,
        },
    )
}
struct Run<'a> {
    inputs: &'a [&'a [u8]],
    fuel: Option<u64>,
    max_frames: usize,
    cells: usize,
    refuse: Option<usize>,
    refuse_cleanup: bool,
}
fn execute_prepared<'l>(
    flow: &'l m::ownership::failure::Elaborated,
    layouts: &'l l::Layouts<'l>,
    program: &CompiledProgram<'l>,
    run: Run<'_>,
) -> Seen {
    let control = e::Control::with_limits(e::Limits {
        fuel: run.fuel,
        host_calls: Some(0),
        frames: Some(run.max_frames),
        live_cells: Some(run.cells),
    });
    execute_controlled(flow, layouts, program, run, control, None)
}
fn control_cause(error: e::Failure) -> c::Cause {
    c::Cause::Execution(error.into())
}
fn rejected_control(entry: m::FunctionId, span: m::Span, reason: n::Reason) -> Seen {
    let mut instance = c::Instance::default();
    let report = instance
        .enter(
            c::Limits {
                slots: 0,
                scopes: 1,
                failure_records: 1,
            },
            |_: &mut (), _| Ok(()),
        )
        .unwrap()
        .finish(c::Exit::Return(None))
        .report;
    Seen {
        value: None,
        scalar: None,
        retired: Vec::new(),
        fuel: 0,
        peak: 0,
        peak_cells: 0,
        failure: Some(n::Failure {
            function: entry,
            reason,
            span,
            panic_code: None,
        }),
        report,
        requests: 0,
        total_requests: 0,
        usage: e::Usage::default(),
    }
}
fn execute_controlled<'l>(
    flow: &'l m::ownership::failure::Elaborated,
    layouts: &'l l::Layouts<'l>,
    program: &CompiledProgram<'l>,
    run: Run<'_>,
    control: e::Control<'_>,
    interrupt: Option<(usize, Rc<e::InterruptionFlag>)>,
) -> Seen {
    let Run {
        inputs,
        fuel: _,
        max_frames,
        cells,
        refuse,
        refuse_cleanup,
    } = run;
    let typed = flow.source().activity().moved().typed();
    let types = typed.places().declarations().types();
    let entry = m::FunctionId(
        typed
            .places()
            .declarations()
            .functions()
            .iter()
            .position(|f| f.name == "main")
            .unwrap_or(0) as u32,
    );
    let signature = typed.places().declarations().functions()[entry.0 as usize].signature_span;
    let denied = n::ControlCapacity {
        frames: max_frames,
        live_cells: cells,
    }
    .admit(control)
    .err()
    .map(n::Reason::UnsupportedControl)
    .or_else(|| {
        control
            .check()
            .err()
            .map(control_cause)
            .map(n::Reason::Runtime)
    })
    .or_else(|| {
        control
            .check_service_work_support(e::service_work_supported(flow))
            .err()
            .map(n::Reason::UnsupportedServiceWork)
    })
    .or_else(|| {
        if control.limits().frames == Some(0) {
            Some(n::Reason::Runtime(c::Cause::Execution(
                c::ExecutionFailure::FrameLimit,
            )))
        } else if control
            .limits()
            .live_cells
            .is_some_and(|limit| program.layout(entry).unwrap().control_cells() > limit)
        {
            Some(n::Reason::Runtime(c::Cause::Execution(
                c::ExecutionFailure::CellLimit,
            )))
        } else {
            None
        }
    });
    if let Some(reason) = denied {
        return rejected_control(entry, signature, reason);
    }
    let largest = (0..flow.functions().len())
        .map(|f| program.layout(m::FunctionId(f as u32)).unwrap().frame())
        .max_by_key(|p| p.cell_count())
        .unwrap();
    let stride = (0..flow.functions().len())
        .map(|f| {
            program
                .layout(m::FunctionId(f as u32))
                .unwrap()
                .slot_count()
        })
        .max()
        .unwrap();
    let bodies = flow.source().activity().moved().typed();
    let loan_stride = bodies
        .functions()
        .iter()
        .map(|f| f.loans)
        .max()
        .unwrap()
        .max(1);
    let prepared_stride = bodies
        .functions()
        .iter()
        .map(|f| f.prepared)
        .max()
        .unwrap()
        .max(1);
    // All host descriptors exist before root ownership or provider effects.
    let loans = std::iter::repeat_with(places::Loan::default)
        .take(loan_stride * max_frames)
        .collect();
    let prepared = vec![None; prepared_stride * max_frames];
    let mut usage = program.usage();
    let mut pools = Vec::with_capacity(max_frames);
    for _ in 0..max_frames {
        let pool = largest.prepare_pool_after(usage).unwrap();
        usage = pool.usage();
        pools.push(pool);
    }
    let trace = Rc::new(RefCell::new(Trace::default()));
    trace.borrow_mut().interrupt = interrupt;
    let mut allocator = a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap();
    let heap = if types.table().constructed.iter().any(|ty| {
        matches!(
            ty.constructor,
            m::ownership::Constructor::Shared(_) | m::ownership::Constructor::Weak(_)
        )
    }) {
        let heap = lang_runtime::shared::managed::Heap::prepare(
            layouts,
            Default::default(),
            Default::default(),
            usage,
        )
        .unwrap();
        usage = heap.usage();
        Some(heap)
    } else {
        None
    };
    let mut arena: Vec<Option<lang_runtime::arena::managed::Arena<'_, '_>>> =
        std::iter::repeat_with(|| None)
            .take(stride * max_frames)
            .collect();
    let mut has_arena = false;
    for depth in 0..max_frames {
        for cell in 0..largest.cell_count() {
            let owner = (0..flow.functions().len()).any(|function| {
                program.layout(m::FunctionId(function as u32)).unwrap().frame().cell_type(cell)
                    .is_some_and(|cell| matches!(cell.ty, m::ownership::Type::Constructed(id)
                        if matches!(types.table().constructed[id.0 as usize].constructor, m::ownership::Constructor::ArenaOwner(_))))
            });
            if owner {
                let prepared = lang_runtime::arena::managed::Arena::prepare(
                    layouts,
                    Default::default(),
                    Default::default(),
                    usage,
                )
                .unwrap();
                usage = prepared.usage();
                arena[depth * stride + cell] = Some(prepared);
                has_arena = true;
            }
        }
    }
    let inspection = if has_arena {
        let prepared =
            lang_runtime::inspection::Inspector::prepare(layouts, 4096, Default::default(), usage)
                .unwrap();
        usage = prepared.usage();
        Some(prepared)
    } else {
        None
    };
    let arena_live = vec![false; stride * max_frames];
    let retirement =
        lang_runtime::retirement::Drain::prepare(layouts, &allocator, Default::default(), usage)
            .unwrap();
    let root = program.layout(entry).unwrap();
    pools[0].admit(root.frame(), &mut allocator).unwrap();
    let typed = flow.source().activity().moved().typed();
    let declaration = &typed.places().declarations().functions()[entry.0 as usize];
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
        loans,
        prepared,
        loan_stride,
        prepared_stride,
        types,
        layouts,
        retirement,
        heap,
        arena,
        arena_live,
        arena_stride: stride,
        inspection,
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
        function: entry,
        parent: None,
    });
    let mut instance = RuntimeInstance::new(Unit {
        program,
        store: &store,
        ledger: Some(ledger),
        stack,
        stride,
        maximum_frames: control.limits().frames.unwrap(),
        cell_limit: control.limits().live_cells.unwrap(),
        live_cells: root.control_cells(),
        usage: e::Usage {
            peak_frames: 1,
            peak_live_cells: root.control_cells(),
            ..Default::default()
        },
        control,
        handle: 1,
        pending: None,
        published: false,
        service_metering: e::service_work_supported(flow),
    });
    let result = program.invoke(entry, &mut instance);
    assert_eq!(instance.runtime().published, result.is_ok());
    assert_eq!(instance.is_poisoned(), refuse_cleanup, "{result:?}");
    let usage = instance.runtime().usage;
    let fuel = usage.fuel;
    let peak = usage.peak_frames;
    let peak_cells = usage.peak_live_cells;
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
    assert!(
        state.loans.iter().all(|loan| loan.target.is_none()
            && loan.vector_read.is_none()
            && loan.map_read.is_none()
            && loan.shared_read.is_none()
            && loan.custody.is_none()
            && loan.arena_read.is_none()),
        "all registrations retire before frame release"
    );
    assert!(
        state.arena_live.iter().all(|live| !live),
        "every arena owner retires before frame release"
    );
    assert!(
        state.prepared.iter().all(Option::is_none),
        "all captures retire before frame release"
    );
    let mut scalar = None;
    let value = completion.value.and_then(|ticket| {
        let physical = ticket.physical().unwrap();
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
    if let Some(heap) = &mut state.heap
        && heap.is_open()
    {
        heap.close(state.allocator).unwrap();
    }
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
        total_requests: trace.requests,
        usage,
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
    for text in [CALLS, RECORDS, values::SUMS, places::LOANS] {
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
        let mut borrow_sites = 0;
        let mut prepared_sites = 0;
        let mut replacement_sites = 0;
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
                if plan.cell_type(plan.return_cell()).unwrap().ty == m::ownership::Type::I64 {
                    8
                } else {
                    32
                }
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
                    let names = if text == places::LOANS {
                        ["owner", "1"]
                    } else {
                        ["second", "first"]
                    };
                    for ((_, span), expected) in record.initialization_order.iter().zip(names) {
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
                if let Some(borrow) = function.borrow_site(node as u32) {
                    borrow_sites += 1;
                    assert_eq!(borrow.kind, m::ownership::LoanKind::Exclusive);
                    assert_eq!(borrow.span, source.span);
                    assert_eq!(borrow.place.projections.len(), 1);
                    assert!(matches!(
                        borrow.place.projections[0].kind,
                        m::ownership::places::ProjectionKind::Field(0)
                    ));
                    let slot = function.layout().loan_slot(borrow.loan).unwrap();
                    assert!(slot >= plan.cell_count() && slot < function.layout().slot_count());
                }
                if let Some(prepared) = function.prepared_site(node as u32) {
                    prepared_sites += 1;
                    assert_eq!(prepared.destination.0, 0);
                    assert!(matches!(
                        prepared.place.projections[0].kind,
                        m::ownership::places::ProjectionKind::Dereference
                    ));
                }
                if let Some(replace) = function.replace_site(node as u32) {
                    replacement_sites += 1;
                    assert!(
                        matches!(replace.destination, n::ReplacementDestination::Prepared(id) if id.0 == 0)
                    );
                    assert_eq!(
                        &text[replace.span.start as usize..replace.span.end as usize],
                        "(*value) + 2"
                    );
                }
            }
        }
        let root = artifact.function(m::FunctionId(0)).unwrap();
        let calls: Vec<_> = (0..flow.functions()[0].nodes().len())
            .filter_map(|node| root.call_site(node as u32))
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function, m::FunctionId(1));
        assert_eq!(
            calls[0].arguments.len(),
            if text == places::LOANS { 1 } else { 2 }
        );
        assert!(root.continuations().contains(&calls[0].resume));
        assert!(root.continuations().contains(&calls[0].failure));
        assert_eq!(
            record_sites,
            usize::from(text == RECORDS || text == places::LOANS)
        );
        assert_eq!(variant_sites, usize::from(text == values::SUMS));
        assert_eq!(extract_sites, usize::from(text == values::SUMS));
        assert_eq!(borrow_sites, usize::from(text == places::LOANS));
        assert_eq!(prepared_sites, usize::from(text == places::LOANS));
        assert_eq!(replacement_sites, usize::from(text == places::LOANS));
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
