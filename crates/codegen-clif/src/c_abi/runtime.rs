//! Typed native execution primitives for explicit C embeddings. Machine code
//! owns guest arithmetic/control; this adapter owns prepared frames/services.
use super::{allocator::Provider, exports::ExportRuntime};
use crate::owned::{self as n, CallSite, Runtime, Transition};
use lang_mir::unit as m;
use lang_runtime::c_abi::{Artifact, context as gate, transfer, validation as v};
use lang_runtime::{allocation as a, buffer, cleanup as c, execution as e, layout as l};
use std::{cell::RefCell, rc::Rc};
mod arenas;
mod core;
mod factory;
mod maps;
mod places;
mod shared;
pub(crate) mod source;
mod values;
mod vectors;
pub use factory::NativeFactory;
fn invariant(error: c::StateError) -> c::Cause {
    if error == c::StateError::Failed {
        c::Cause::Reported
    } else {
        c::Cause::Execution(c::ExecutionFailure::Invariant)
    }
}
fn control_cause(error: e::Failure) -> c::Cause {
    c::Cause::Execution(error.into())
}
fn service_invariant<T: std::error::Error + 'static>(error: T) -> c::Cause {
    c::Cause::Execution(
        e::service_failure(&error)
            .map(Into::into)
            .unwrap_or(c::ExecutionFailure::Invariant),
    )
}

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
struct Store<'a, 'l, P: a::Provider = Provider<'l>> {
    pools: Vec<l::FrameStorage<'l, 'l>>,
    live: Vec<bool>,
    allocator: &'a mut a::Allocator<P>,
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
impl<P: a::Provider> Store<'_, '_, P> {
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
struct Drain<'a, 'l, P: a::Provider = Provider<'l>>(Rc<RefCell<Store<'a, 'l, P>>>, e::ServiceWork);
impl<P: a::Provider> c::Cleaner<Ticket> for Drain<'_, '_, P> {
    fn cleanup(
        &mut self,
        ticket: &mut Ticket,
        _: c::Reason,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let meter = self.1.clone();
        meter.unmetered(|| {
            let mut storage = self.0.borrow_mut();
            let physical = match ticket {
                Ticket::Loan(id) => {
                    let Store { loans, layouts, allocator, heap, retirement, types, arena, .. } = &mut *storage;
                    let target = loans[*id].target.take();
                    loans[*id].custody = None;
                    if let Some(mut read) = loans[*id].vector_read.take() { read.end(layouts, allocator).map_err(|_| 98u32)?; }
                    if let Some(mut read) = loans[*id].map_read.take() { read.end(layouts, allocator).map_err(|_| 99u32)?; }
                    if let Some((owner, mut read)) = loans[*id].arena_read.take() {
                        arena[owner].as_mut().ok_or(103u32)?.end_read(&mut read, allocator).map_err(|_| 104u32)?;
                    }
                    if let Some(read) = loans[*id].shared_read.take() {
                        let Some(places::Target::Shared { ty, .. }) = target else { return Err(100); };
                        retirement.end_read(read, ty, heap.as_mut().ok_or(101u32)?, allocator, context,
                            |token, allocator, _, _| buffer::cleanup_token(types, token, allocator).map_err(|_| 94u32)).map_err(|_| 102u32)?;
                    }
                    return Ok(());
                }
                Ticket::Value(value) => value,
            };
            let Store { pools, allocator, types, retirement, heap, arena, arena_live, arena_stride, .. } = &mut *storage;
            let owner_type = pools[physical.frame].cell(physical.cell).map_err(|_| 90u32)?.root().ty();
            if matches!(owner_type, m::ownership::Type::Constructed(id)
                if matches!(types.table().constructed[id.0 as usize].constructor, m::ownership::Constructor::ArenaOwner(_))) {
                let id = physical.frame * *arena_stride + physical.cell;
                if !arena_live[id] { return Err(105); }
                arena[id].as_mut().ok_or(106u32)?.cleanup(
                    if context.reason() == c::Reason::Failure { lang_runtime::arena::managed::Exit::Abort } else { lang_runtime::arena::managed::Exit::Normal },
                    allocator, context,
                    |token, allocator, _, context| retirement.token(token, heap.as_mut(), allocator, context,
                        |leaf, allocator, _, _| buffer::cleanup_token(types, leaf, allocator).map_err(|_| 94u32))
                ).map_err(|_| 107u32)?;
                arena_live[id] = false;
                return Ok(());
            }
            let cell = pools[physical.frame].cell_mut(physical.cell).map_err(|_| 90u32)?;
            cell.retire(allocator, context, |token, allocator, _, context|
                retirement.token(token, heap.as_mut(), allocator, context,
                    |leaf, allocator, _, _| buffer::cleanup_token(types, leaf, allocator).map_err(|_| 94u32))
            ).map_err(|_| 96u32)?;
            cell.recycle(allocator).map_err(|_| 97u32)
        })
    }
}
#[derive(Clone, Copy)]
struct Parent {
    span: m::Span,
    result: usize,
    resume: u32,
    failure: u32,
}
struct Active {
    handle: u64,
    function: m::FunctionId,
    parent: Option<Parent>,
}
enum EntryMode<'a, 'l> {
    Ordinary,
    C {
        artifact: &'l Artifact,
        function: usize,
        environment: n::CEnvironment<'a>,
    },
}
type ForeignImport<'i, 's, 'a, 'l, P> = fn(
    &mut State<'i, 's, 'a, 'l, P>,
    &n::ImportSite,
    usize,
    &mut gate::Session<'_>,
    &mut dyn gate::scalar::Invoker,
) -> n::ResultStep;
pub struct State<'i, 's, 'a, 'l, P: a::Provider = Provider<'l>> {
    program: &'s dyn n::runtime::FrameProgram<'l>,
    store: Rc<RefCell<Store<'a, 'l, P>>>,
    ledger: Option<c::Frame<'i, Ticket, Drain<'a, 'l, P>>>,
    stack: Vec<Active>,
    fault_stack: Vec<(m::FunctionId, m::Span)>,
    stride: usize,
    maximum_frames: usize,
    cell_limit: usize,
    live_cells: usize,
    usage: e::Usage,
    control: e::Control<'a>,
    handle: u64,
    pending: Option<Transition>,
    published: bool,
    service_work: e::ServiceWork,
    mode: EntryMode<'a, 'l>,
    foreign_import: Option<ForeignImport<'i, 's, 'a, 'l, P>>,
}
impl<P: a::Provider> State<'_, '_, '_, '_, P> {
    fn work(&self) -> impl FnMut(u64) -> Result<(), e::Failure> + use<P> {
        let meter = self.service_work.clone();
        move |cost| meter.charge(cost)
    }

    fn depth(&self) -> usize {
        self.stack.len() - 1
    }
    fn slot(&self, slot: usize) -> c::Slot {
        c::Slot(self.depth() * self.stride + slot)
    }
    fn plan(&self) -> &l::FramePlan<'_, '_> {
        self.program
            .frame_layout(self.stack.last().unwrap().function)
            .unwrap()
            .frame()
    }
    fn retire_pool(&mut self, depth: usize) {
        let mut state = self.store.borrow_mut();
        let Store {
            pools,
            allocator,
            live,
            ..
        } = &mut *state;
        // Consumption precedes a provider callback; never retry this pool if
        // its trusted release panics after consuming an allocation handle.
        live[depth] = false;
        pools[depth].release(allocator).unwrap();
    }
}
impl<P: a::Provider> Runtime for State<'_, '_, '_, '_, P> {
    fn c_environment(&self) -> Option<&n::CEnvironment<'_>> {
        match &self.mode {
            EntryMode::C { environment, .. } => Some(environment),
            EntryMode::Ordinary => None,
        }
    }
    fn c_usage(&self) -> Option<e::Usage> {
        self.c_environment().map(|_| e::Usage {
            service_work: self.service_work.used(),
            ..self.usage
        })
    }
    fn set_c_usage(&mut self, usage: e::Usage) -> n::ResultStep {
        self.usage = usage;
        Ok(())
    }
    fn c_copy_import(
        &mut self,
        site: &n::ImportSite,
        function: usize,
        session: &mut gate::Session<'_>,
        invoker: &mut dyn gate::scalar::Invoker,
    ) -> n::ResultStep {
        let dispatch = self
            .foreign_import
            .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?;
        dispatch(self, site, function, session, invoker)
    }
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
            || !std::ptr::eq(self.program.frame_layout(active.function).unwrap(), layout)
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
            .frame_layout(self.stack.last().unwrap().function)
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
        if self.fault_stack.is_empty() {
            let mut at = span;
            for frame in self.stack.iter().rev() {
                assert!(self.fault_stack.len() < self.fault_stack.capacity());
                self.fault_stack.push((frame.function, at));
                if let Some(parent) = frame.parent {
                    at = parent.span;
                }
            }
        }
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
        let stable = self.program.frame_layout(call.function).unwrap();
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
                .map_err(|_| c::Cause::Execution(c::ExecutionFailure::AllocationLimit))?;
            state.live[depth] = true;
        }
        let caller_layout = self
            .program
            .frame_layout(self.stack.last().unwrap().function)
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
                span: call.span,
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
            .frame_layout(self.stack.last().unwrap().function)
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

fn c_copy_import(
    state: &mut State<'_, '_, '_, '_>,
    site: &n::ImportSite,
    function: usize,
    session: &mut gate::Session<'_>,
    invoker: &mut dyn gate::scalar::Invoker,
) -> n::ResultStep {
    let artifact = match &state.mode {
        EntryMode::C { artifact, .. } => *artifact,
        EntryMode::Ordinary => return Err(c::Cause::Execution(c::ExecutionFailure::Invariant)),
    };
    let output = Physical {
        frame: state.depth(),
        cell: site.result,
    };
    let call =
        transfer::CallStorage::reserve(artifact, function, state.store.borrow_mut().allocator)
            .map_err(service_invariant)?;
    let outcome = (|| {
        for (index, slot) in site.arguments.iter().enumerate() {
            let source = state
                .ledger
                .as_ref()
                .unwrap()
                .value(state.slot(*slot))
                .map_err(invariant)?
                .physical()?;
            let mut store = state.store.borrow_mut();
            let Store {
                pools, allocator, ..
            } = &mut *store;
            call.encode(
                index,
                artifact,
                function,
                pools[source.frame]
                    .cell(source.cell)
                    .map_err(service_invariant)?,
                allocator,
            )
            .map_err(service_invariant)?;
        }
        let mut store = state.store.borrow_mut();
        call.invoke(function, site.span, session, invoker, store.allocator)
            .map_err(|fault| {
                c::Cause::Execution(match fault.kind {
                    gate::Kind::Control(error)
                    | gate::Kind::Value(v::Error::Control(error))
                    | gate::Kind::ForeignValue(v::Error::Control(error)) => error.into(),
                    gate::Kind::HostPanic => c::ExecutionFailure::HostPanic,
                    _ => c::ExecutionFailure::HostCall,
                })
            })?;
        let Store {
            pools, allocator, ..
        } = &mut *store;
        call.decode(
            artifact,
            function,
            pools[output.frame]
                .cell(output.cell)
                .map_err(service_invariant)?,
            allocator,
        )
        .map_err(service_invariant)
    })();
    let release = call
        .release(state.store.borrow_mut().allocator)
        .map_err(service_invariant);
    outcome.and(release)?;
    values::publish(state, site.result, output, site.span)
}
