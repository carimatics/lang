use super::*;

/// Prepared native code and bounded host-pool policy for C exports. This
/// factory uses the original context allocator and cleanup Instance; it never
/// executes a guest body. NativeExports invokes the actual compiled machine code.
pub struct NativeFactory<'p> {
    program: &'p dyn n::NativeProgram<'p>,
    artifact: &'p Artifact,
    limits: m::Limits,
}
impl<'p> NativeFactory<'p> {
    pub fn new(
        program: &'p dyn n::NativeProgram<'p>,
        artifact: &'p Artifact,
        limits: m::Limits,
    ) -> Result<Self, gate::Fault> {
        if program.artifact_digest() != artifact.digest() {
            return Err(gate::Fault::new(gate::Kind::WrongProgram));
        }
        Ok(Self {
            program,
            artifact,
            limits,
        })
    }
}
fn capacity(_: impl std::fmt::Debug) -> gate::Fault {
    gate::Fault::new(gate::Kind::Capacity)
}
fn allocation(_: impl std::fmt::Debug) -> gate::Fault {
    gate::Fault::new(gate::Kind::Execution(c::ExecutionFailure::AllocationLimit))
}
fn usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}
impl<'p> ExportRuntime<'p> for NativeFactory<'p> {
    type State<'call>
        = State<'call, 'p, 'call, 'p>
    where
        'p: 'call;
    fn prepare<'call>(
        &'p self,
        layout: &'p n::FunctionLayout<'p>,
        instance: &'call mut c::Instance,
        allocator: &'call mut a::Allocator<Provider<'p>>,
        environment: n::CEnvironment<'call>,
        arguments: &[v::Argument<'_>],
    ) -> Result<Self::State<'call>, gate::Fault>
    where
        'p: 'call,
    {
        if !std::ptr::eq(
            self.program
                .layout(layout.function())
                .ok_or_else(|| capacity(()))?,
            layout,
        ) || allocator.identity() != environment.allocator()
        {
            return Err(gate::Fault::new(gate::Kind::WrongProgram));
        }
        let control = environment.control();
        let maximum_frames = control.limits().frames.ok_or_else(|| capacity(()))?;
        let cell_limit = control.limits().live_cells.ok_or_else(|| capacity(()))?;
        if maximum_frames == 0 || layout.control_cells() > cell_limit {
            return Err(capacity(()));
        }
        let flow = layout.source();
        control
            .check_service_work_support(e::service_work_supported(flow))
            .map_err(capacity)?;
        let typed = flow.source().activity().moved().typed();
        let function = (0..self.artifact.functions().len())
            .find(|i| self.program.exported_function(*i) == Some(layout.function()))
            .ok_or_else(|| capacity(()))?;
        let state = prepare(Preparation {
            program: self.program,
            layout,
            instance,
            allocator,
            limits: self.limits,
            control,
            cleanup_records: environment.cleanup_records(),
            work: environment.work().clone(),
            mode: EntryMode::C {
                artifact: self.artifact,
                function,
                environment,
            },
            foreign_import: Some(c_copy_import),
        })
        .map_err(setup_fault)?;
        {
            let mut store = state.store.borrow_mut();
            let Store {
                pools,
                live,
                allocator,
                ..
            } = &mut *store;
            pools[0]
                .admit(layout.frame(), allocator)
                .map_err(allocation)?;
            live[0] = true;
            let wire = transfer::CallStorage::reserve(self.artifact, function, allocator)
                .map_err(allocation)?;
            let declaration =
                &typed.places().declarations().functions()[layout.function().0 as usize];
            let received = (|| {
                for (index, parameter) in declaration.parameters.iter().enumerate() {
                    let slot = layout.frame().local(*parameter).map_err(capacity)?;
                    wire.receive(
                        index,
                        self.artifact,
                        arguments.get(index).ok_or_else(|| capacity(()))?,
                        pools[0].cell(slot).map_err(capacity)?,
                        allocator,
                    )
                    .map_err(capacity)?;
                }
                Ok(())
            })();
            let release = wire
                .release(allocator)
                .map_err(|_| gate::Fault::new(gate::Kind::Cleanup));
            received.and(release)?;
        }
        Ok(state)
    }
    fn complete<'call>(
        &self,
        state: &mut Self::State<'call>,
        failure: Option<n::Failure>,
        output: Option<a::Handle>,
    ) -> Result<(), gate::Fault>
    where
        'p: 'call,
    {
        let (artifact, function) = match &state.mode {
            EntryMode::C {
                artifact, function, ..
            } => (*artifact, *function),
            EntryMode::Ordinary => return Err(gate::Fault::new(gate::Kind::WrongProgram)),
        };
        let ledger = state
            .ledger
            .take()
            .ok_or_else(|| gate::Fault::new(gate::Kind::Cleanup))?;
        let completion = if failure.is_none() && state.published {
            ledger.finish_staged_return()
        } else {
            ledger.finish_reported_failure()
        };
        let outcome = (|| {
            if let Some(fault) = gate::Fault::from_report(
                &completion.report,
                failure.and_then(|f| f.panic_code).unwrap_or(0),
            ) {
                return Err(fault);
            }
            if let Some(failure) = failure {
                return Err(match failure.reason {
                    n::Reason::Foreign(fault) => fault,
                    _ => gate::Fault::new(gate::Kind::Execution(c::ExecutionFailure::Fault))
                        .at(failure.span),
                });
            }
            if let Some(output) = output {
                let physical = completion
                    .value
                    .ok_or_else(|| gate::Fault::new(gate::Kind::ResultShape))?
                    .physical()
                    .map_err(capacity)?;
                let mut store = state.store.borrow_mut();
                let Store {
                    pools, allocator, ..
                } = &mut *store;
                transfer::CallStorage::write_export_result(
                    artifact,
                    function,
                    pools[physical.frame]
                        .cell(physical.cell)
                        .map_err(capacity)?,
                    output,
                    allocator,
                )
                .map_err(capacity)?;
            }
            Ok(())
        })();
        let cleanup = state.close();
        match (outcome, cleanup) {
            (Err(mut first), Err(later)) => {
                first.cleanup_count = first.cleanup_count.saturating_add(later.cleanup_count);
                first.truncated |= later.truncated;
                first.quarantined = first.quarantined.saturating_add(later.quarantined);
                Err(first)
            }
            (Err(first), _) => Err(first),
            (Ok(()), result) => result,
        }
    }
}
impl<P: a::Provider> State<'_, '_, '_, '_, P> {
    pub(super) fn close(&mut self) -> Result<(), gate::Fault> {
        fn attempt(
            first: &mut Option<gate::Fault>,
            operation: impl FnOnce() -> Result<(), l::Error>,
        ) {
            let fault = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)) {
                Ok(Ok(())) => return,
                Ok(Err(_)) => gate::Fault::new(gate::Kind::Cleanup),
                Err(payload) => {
                    let mut fault = gate::Fault::host_panic(payload);
                    fault.cleanup_count = fault.cleanup_count.saturating_add(1);
                    fault
                }
            };
            if let Some(first) = first {
                first.cleanup_count = first.cleanup_count.saturating_add(fault.cleanup_count);
                first.quarantined = first.quarantined.saturating_add(fault.quarantined);
                first.truncated |= fault.truncated;
            } else {
                *first = Some(fault);
            }
        }
        let meter = self.service_work.clone();
        meter.unmetered(|| {
            let mut store = self.store.borrow_mut();
            let Store {
                pools,
                live,
                allocator,
                heap,
                ..
            } = &mut *store;
            let mut first = None;
            for (pool, live) in pools.iter_mut().zip(live) {
                if *live {
                    *live = false;
                    attempt(&mut first, || pool.release(allocator));
                }
            }
            if let Some(mut heap) = heap.take()
                && heap.is_open()
            {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    heap.close(allocator)
                }));
                let fault = match result {
                    Ok(Ok(())) => None,
                    Ok(Err(_)) => Some(gate::Fault::new(gate::Kind::Cleanup)),
                    Err(payload) => {
                        let mut fault = gate::Fault::host_panic(payload);
                        fault.cleanup_count = fault.cleanup_count.saturating_add(1);
                        Some(fault)
                    }
                };
                if let Some(fault) = fault {
                    if let Some(first) = &mut first {
                        first.cleanup_count =
                            first.cleanup_count.saturating_add(fault.cleanup_count);
                        first.quarantined = first.quarantined.saturating_add(fault.quarantined);
                        first.truncated |= fault.truncated;
                    } else {
                        first = Some(fault);
                    }
                }
            }
            first.map_or(Ok(()), Err)
        })
    }
}
impl<P: a::Provider> Drop for State<'_, '_, '_, '_, P> {
    fn drop(&mut self) {
        if let Some(ledger) = self.ledger.take() {
            let _ = ledger.finish(c::Exit::Failure {
                failure: if std::thread::panicking() {
                    c::ExecutionFailure::HostPanic
                } else {
                    c::ExecutionFailure::Fault
                },
                span: None,
            });
        }
        let _ = self.close();
    }
}

use n::runtime::PreparationError as SetupError;
pub(super) struct Preparation<'i, 'a, 'l, P: a::Provider> {
    pub program: &'l dyn n::runtime::FrameProgram<'l>,
    pub layout: &'l n::FunctionLayout<'l>,
    pub instance: &'i mut c::Instance,
    pub allocator: &'a mut a::Allocator<P>,
    pub limits: m::Limits,
    pub control: e::Control<'a>,
    pub cleanup_records: usize,
    pub work: e::ServiceWork,
    pub mode: EntryMode<'a, 'l>,
    pub foreign_import: Option<ForeignImport<'i, 'l, 'a, 'l, P>>,
}
pub(super) fn prepare<'i, 'a, 'l, P: a::Provider>(
    input: Preparation<'i, 'a, 'l, P>,
) -> Result<State<'i, 'l, 'a, 'l, P>, SetupError> {
    let Preparation {
        program,
        layout,
        instance,
        allocator,
        limits,
        control,
        cleanup_records,
        work,
        mode,
        foreign_import,
    } = input;
    let maximum_frames = control.limits().frames.ok_or(SetupError::Capacity)?;
    let cell_limit = control.limits().live_cells.ok_or(SetupError::Capacity)?;
    if maximum_frames == 0 || layout.control_cells() > cell_limit {
        return Err(SetupError::Capacity);
    }
    let flow = layout.source();
    control
        .check_service_work_support(e::service_work_supported(flow))
        .map_err(SetupError::ServiceWork)?;
    let typed = flow.source().activity().moved().typed();
    let types = typed.places().declarations().types();
    let layouts = layout.layouts();
    let largest = (0..flow.functions().len())
        .map(|i| {
            program
                .frame_layout(m::FunctionId(i as u32))
                .unwrap()
                .frame()
        })
        .max_by_key(|p| p.cell_count())
        .unwrap();
    let stride = (0..flow.functions().len())
        .map(|i| {
            program
                .frame_layout(m::FunctionId(i as u32))
                .unwrap()
                .slot_count()
        })
        .max()
        .unwrap();
    let loan_stride = typed
        .functions()
        .iter()
        .map(|f| f.loans)
        .max()
        .unwrap()
        .max(1);
    let prepared_stride = typed
        .functions()
        .iter()
        .map(|f| f.prepared)
        .max()
        .unwrap()
        .max(1);
    let multiply = |n: usize| n.checked_mul(maximum_frames).ok_or(SetupError::Capacity);
    let mut budget =
        m::Budget::with_usage(limits, program.frame_usage()).map_err(SetupError::Resource)?;
    let mut loans = budget
        .vector(multiply(loan_stride)?)
        .map_err(SetupError::Resource)?;
    loans.resize_with(multiply(loan_stride)?, places::Loan::default);
    let mut prepared = budget
        .vector(multiply(prepared_stride)?)
        .map_err(SetupError::Resource)?;
    prepared.resize(multiply(prepared_stride)?, None);
    let mut pools = budget
        .vector(maximum_frames)
        .map_err(SetupError::Resource)?;
    let mut live = budget
        .vector(maximum_frames)
        .map_err(SetupError::Resource)?;
    live.resize(maximum_frames, false);
    for _ in 0..maximum_frames {
        let pool = largest
            .prepare_pool_after(usage(&budget))
            .map_err(SetupError::Layout)?;
        budget = m::Budget::with_usage(limits, pool.usage()).map_err(SetupError::Resource)?;
        pools.push(pool);
    }
    let heap = if types.table().constructed.iter().any(|ty| {
        matches!(
            ty.constructor,
            m::ownership::Constructor::Shared(_) | m::ownership::Constructor::Weak(_)
        )
    }) {
        let heap = lang_runtime::shared::managed::Heap::prepare(
            layouts,
            Default::default(),
            limits,
            usage(&budget),
        )
        .map_err(SetupError::Shared)?;
        budget = m::Budget::with_usage(limits, heap.usage()).map_err(SetupError::Resource)?;
        Some(heap.with_service_work(work.clone()))
    } else {
        None
    };
    let mut arena = budget
        .vector(multiply(stride)?)
        .map_err(SetupError::Resource)?;
    arena.resize_with(multiply(stride)?, || None);
    let mut has_arena = false;
    budget
        .charge(
            multiply(largest.cell_count())?
                .checked_mul(flow.functions().len())
                .ok_or(SetupError::Capacity)?,
        )
        .map_err(SetupError::Resource)?;
    for depth in 0..maximum_frames {
        for cell in 0..largest.cell_count() {
            let owner = (0..flow.functions().len()).any(|function| {
                    program.frame_layout(m::FunctionId(function as u32)).unwrap().frame().cell_type(cell)
                        .is_some_and(|cell| matches!(cell.ty, m::ownership::Type::Constructed(id)
                            if matches!(types.table().constructed[id.0 as usize].constructor, m::ownership::Constructor::ArenaOwner(_))))
                });
            if owner {
                let prepared = lang_runtime::arena::managed::Arena::prepare(
                    layouts,
                    Default::default(),
                    limits,
                    usage(&budget),
                )
                .map_err(SetupError::Arena)?;
                budget = m::Budget::with_usage(limits, prepared.usage())
                    .map_err(SetupError::Resource)?;
                arena[depth * stride + cell] = Some(prepared.with_service_work(work.clone()));
                has_arena = true;
            }
        }
    }
    let inspection = if has_arena {
        let prepared =
            lang_runtime::inspection::Inspector::prepare(layouts, 4096, limits, usage(&budget))
                .map_err(SetupError::Inspection)?;
        budget = m::Budget::with_usage(limits, prepared.usage()).map_err(SetupError::Resource)?;
        Some(prepared)
    } else {
        None
    };
    let mut arena_live = budget
        .vector(multiply(stride)?)
        .map_err(SetupError::Resource)?;
    arena_live.resize(multiply(stride)?, false);
    let mut stack = budget
        .vector(maximum_frames)
        .map_err(SetupError::Resource)?;
    stack.push(Active {
        debug: n::DebugFrame::new(
            layout.function(),
            layout
                .source()
                .source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations()
                .functions()[layout.function().0 as usize]
                .span,
        ),
        handle: 1,
        function: layout.function(),
        parent: None,
    });
    let fault_stack = budget
        .vector(maximum_frames)
        .map_err(SetupError::Resource)?;
    let retirement =
        lang_runtime::retirement::Drain::prepare(layouts, allocator, limits, usage(&budget))
            .map_err(SetupError::Retirement)?;
    let store = Rc::new(RefCell::new(Store {
        pools,
        live,
        allocator,
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
    }));
    let ledger = instance
        .enter_cleaner(
            c::Limits {
                slots: multiply(stride)?,
                scopes: maximum_frames,
                failure_records: cleanup_records,
            },
            Drain(store.clone(), work.clone()),
        )
        .map_err(SetupError::Instance)?;
    let state = State {
        program,
        store,
        ledger: Some(ledger),
        stack,
        fault_stack,
        stride,
        maximum_frames,
        cell_limit,
        live_cells: layout.control_cells(),
        usage: e::Usage {
            peak_frames: 1,
            peak_live_cells: layout.control_cells(),
            ..Default::default()
        },
        control,
        handle: 1,
        pending: None,
        published: false,
        service_work: work,
        mode,
        foreign_import,
    };

    Ok(state)
}
fn setup_fault(error: SetupError) -> gate::Fault {
    match error {
        SetupError::Control(error) => gate::Fault::new(gate::Kind::Control(error)),
        _ => capacity(()),
    }
}
