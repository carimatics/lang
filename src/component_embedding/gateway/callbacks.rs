//! Synchronous binding calls over the actual native callback's borrowed state.
use super::*;

/// # Safety
/// Context, binding and initialized output/error storage are exact live objects
/// retained for this entire owner-thread operation.
pub unsafe extern "C" fn context_call(
    context: *const ffi::Context,
    binding: *mut ffi::Binding,
    arguments: ffi::Bytes,
    output: *mut *mut ffi::Result,
    error: *mut ffi::Error,
) -> u32 {
    if context.is_null() || binding.is_null() || output.is_null() || !unsafe { error_ready(error) }
    {
        return INVALID;
    }
    // SAFETY: only checked immutable engine routing is resolved here. The
    // dynamic native scope is still validated by with_child below.
    let owner = match unsafe { metadata((*context).engine) } {
        Ok(owner) => unsafe { owner.as_ref() },
        Err(status) => return status,
    };
    let scope = unsafe { component::context_check(context) };
    if scope != 0 && scope != 1 {
        return scope;
    }
    let binding = unsafe { &*binding.cast::<Binding>() };
    if binding.instance.engine.as_ptr() != ptr::from_ref(owner).cast_mut()
        || binding.instance.closed.get()
    {
        return INVALID;
    }
    if !owner.active.get() {
        return BUSY;
    }
    let Some(current) = owner.current.borrow().clone() else {
        return BUSY;
    };
    let arguments = match unsafe { bytes(arguments) } {
        Ok(value) => value,
        Err(status) => return status,
    };
    let reservation = match ResultReservation::new(owner) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let mut result = None;
    // SAFETY: the checked bridge retains the exact current control, task and
    // native call; every borrowed operand stays inside this synchronous scope.
    let called = unsafe {
        component::with_child_admission(context, |invoker, call, control, admission| {
            let mut operation = Call {
                owner,
                target: binding.instance.clone(),
                entry: binding.entry,
                current,
                arguments,
                invoker,
                control,
                admission,
                result: &mut result,
            };
            call.with_callback(&mut operation)
        })
    };
    if let Err(status) = called {
        return status;
    }
    let Some(result) = result else {
        return INVALID;
    };
    let status = if matches!(result.body, ResultBody::Failure(_)) {
        1
    } else {
        OK
    };
    // Native result indices were committed into the original context before
    // this immutable representation becomes visible; dropping it owns no data.
    unsafe { output.write(Box::into_raw(Box::new(result)).cast()) };
    reservation.commit();
    status
}

struct Call<'a, 'ctrl> {
    owner: &'a Metadata,
    target: Rc<InstanceData>,
    entry: m::FunctionId,
    current: CurrentCall,
    arguments: &'a [u8],
    invoker: &'a mut dyn execution::Invoker,
    control: e::Control<'ctrl>,
    admission: Result<(), e::Failure>,
    result: &'a mut Option<CallResult>,
}
impl execution::CallbackOperation for Call<'_, '_> {
    fn run<'code>(
        &mut self,
        origin: &mut execution::Instance<'_, '_>,
        resources: &mut storage::Resources<'code>,
        memory: &mut dyn storage::Memory,
        budget: &mut m::Budget,
        session: &mut wire::Session,
        arguments: &execution::CallbackArguments<'_, 'code>,
        _: storage::Holder,
        accounting: &e::InvocationAccounting,
        task: &mut lang_runtime::task::CurrentTask<'_>,
    ) -> Result<(), execution::CallbackFailure> {
        let target = self.target.clone();
        if let Some(first) = accounting.child_failure() {
            let Some(canonical_indices) = accounting.failure_indices() else {
                return Err(UNSUPPORTED.into());
            };
            let caller = self.current.instance.clone();
            let allocations = a::Allocator::borrow(memory).map_err(|_| INVALID)?.stats();
            let usage = accounting.snapshot();
            let tasks = task.usage();
            let failure = Rc::new(r::Failure {
                function: first.function,
                span: first.span,
                callers: None,
                usage,
                tasks,
                events: vec![],
                report: None,
                poisoned: first.poisoned,
                reason: r::Reason::Native(execution::NativeFailure::Context {
                    failure: first,
                    canonical_indices,
                    native_frame: accounting.failure_frame(),
                }),
            });
            let report = project_failure(
                &failure,
                self.current.ceiling,
                FailureCode {
                    digest: caller.digest,
                    mode: self.current.mode,
                    entry: failure.function.unwrap_or(self.entry),
                    functions: caller.module.unit().code().declarations().functions(),
                },
                allocations,
                canonical_indices,
            )
            .ok_or(INVALID)?;
            accounting.latch_boundary_failure_from(
                report.0.clone(),
                report.1.clone(),
                caller.clone(),
                failure.clone(),
            );
            *self.result = Some(CallResult {
                owner: ResultOwner::Instance(caller),
                body: ResultBody::Failure(failure),
                usage,
                tasks,
                committed: false,
                poisoned: report.0.poisoned,
                allocations,
                canonical_indices,
                report: Some(report),
            });
            return Ok(());
        }
        if let Some(latched) = accounting.boundary_failure() {
            let report = latched.report().clone();
            let bytes = latched.bytes().to_vec();
            let allocations = a::Allocator::borrow(memory).map_err(|_| INVALID)?.stats();
            let usage = accounting.snapshot();
            let tasks = task.usage();
            let poisoned = report.poisoned;
            let failure = r::Failure {
                reason: r::Reason::Native(execution::NativeFailure::Latched(latched)),
                function: None,
                span: None,
                callers: None,
                usage,
                tasks,
                events: vec![],
                report: None,
                poisoned,
            };
            *self.result = Some(CallResult {
                owner: ResultOwner::Instance(target),
                body: ResultBody::Failure(Rc::new(failure)),
                usage,
                tasks,
                committed: false,
                poisoned,
                allocations,
                canonical_indices: arguments.resource_indices() as u64,
                report: Some((report, bytes)),
            });
            return Ok(());
        }
        if let Err(reason) = self.admission {
            let allocations = a::Allocator::borrow(memory).map_err(|_| INVALID)?.stats();
            let canonical_indices = arguments.resource_indices() as u64;
            let usage = accounting.snapshot();
            let tasks = task.usage();
            let failure = r::Failure {
                reason: r::Reason::Control(reason),
                function: Some(self.entry),
                span: Some(
                    target.module.unit().code().declarations().functions()[self.entry.0 as usize]
                        .signature_span,
                ),
                callers: Some(execution::GuestCallFrames {
                    frames: Vec::new(),
                    omitted: 0,
                }),
                usage,
                tasks,
                events: vec![],
                report: None,
                poisoned: false,
            };
            let report = project_failure(
                &failure,
                self.current.ceiling,
                FailureCode {
                    digest: target.digest,
                    mode: self.current.mode,
                    entry: self.entry,
                    functions: target.module.unit().code().declarations().functions(),
                },
                allocations,
                canonical_indices,
            );
            let failure = Rc::new(failure);
            if let Some((report, bytes)) = &report {
                accounting.latch_boundary_failure_from(
                    report.clone(),
                    bytes.clone(),
                    target.clone(),
                    failure.clone(),
                );
            }
            *self.result = Some(CallResult {
                owner: ResultOwner::Instance(target),
                body: ResultBody::Failure(failure),
                usage,
                tasks,
                committed: false,
                poisoned: false,
                allocations,
                canonical_indices,
                report,
            });
            return Ok(());
        }
        if target.module.unit().code().declarations().functions()[self.entry.0 as usize].effect
            != m::ownership::declarations::FunctionEffect::Synchronous
        {
            return Err(BUSY.into());
        }
        let _current = CurrentScope::enter(
            self.owner,
            CurrentCall {
                instance: target.clone(),
                ..self.current.clone()
            },
        );
        if Rc::ptr_eq(&target, &self.current.instance) {
            self.execute(
                origin, resources, memory, budget, session, arguments, task, accounting,
            )
        } else {
            let mut state = target.state.try_borrow_mut().map_err(|_| BUSY)?;
            if state.poisoned() {
                return Err(BUSY.into());
            }
            let mut preparing = session
                .scope_preparation(budget)
                .map_err(execution::CallbackFailure::from)?;
            let mut views = preparing
                .vector(target.grants.len(), 0)
                .map_err(execution::CallbackFailure::from)?;
            for grant in &target.grants {
                views.push(grant.view());
            }
            let bound = grants::prepare(
                target.module.unit(),
                target.module.contract(),
                &views,
                || &target.providers,
                &mut preparing,
            )
            .map_err(execution::CallbackFailure::from)?;
            drop(preparing);
            let program = program::Program::prepare_with_budget(&bound, budget)
                .map_err(execution::CallbackFailure::from)?;
            let result = state
                .with_callback_program(&program, origin, |instance| {
                    self.execute(
                        instance, resources, memory, budget, session, arguments, task, accounting,
                    )
                })
                .map_err(|_| BUSY)?;
            if state.poisoned() {
                origin.poison();
            }
            result
        }
    }
}
impl Call<'_, '_> {
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep every original callback capability explicit."
    )]
    fn execute<'code>(
        &mut self,
        instance: &mut execution::Instance<'_, '_>,
        resources: &mut storage::Resources<'code>,
        memory: &mut dyn storage::Memory,
        budget: &mut m::Budget,
        session: &mut wire::Session,
        callback: &execution::CallbackArguments<'_, 'code>,
        task: &mut lang_runtime::task::CurrentTask<'_>,
        accounting: &e::InvocationAccounting,
    ) -> Result<(), execution::CallbackFailure> {
        let program = instance.program();
        let generated = match self.current.mode {
            0 => None,
            1 => Some(
                lang_codegen_clif::tasks::CompiledProgram::compile_component(
                    program,
                    self.entry,
                    self.current.limits.preparation,
                )
                .map_err(|_| INVALID)?,
            ),
            _ => return Err(UNSUPPORTED.into()),
        };
        let mut memory = a::Allocator::borrow(memory).map_err(|_| INVALID)?;
        let mut host = execution::Invocation::prepare_owned(
            instance,
            self.entry,
            resources,
            self.invoker,
            budget,
        )
        .map_err(execution::CallbackFailure::from)?;
        let values = {
            let mut preparing = session
                .scope_preparation(budget)
                .map_err(execution::CallbackFailure::from)?;
            host.decode_callback_arguments(self.arguments, callback, &memory, &mut preparing)
                .map_err(execution::CallbackFailure::from)?
        };
        host.prepare_callback_output(session, budget, callback)
            .map_err(execution::CallbackFailure::from)?;
        let limits = r::Limits {
            tasks: task.limits(),
            preparation: self.current.limits.preparation,
            cleanup_failure_records: usize::try_from(self.current.ceiling.suppressed_records)
                .map_err(|_| INVALID)?
                .saturating_add(1),
            ..Default::default()
        };
        let current = task.reborrow().map_err(|_| BUSY)?;
        let outcome = if let Some(generated) = generated {
            generated.execute_component_in_task(
                &mut memory,
                self.control,
                limits,
                &values,
                &mut host,
                budget,
                current,
            )
        } else {
            lang_vm::unit::tasks::execute_component_in_task(
                program,
                self.entry,
                &mut memory,
                self.control,
                limits,
                &values,
                &mut host,
                budget,
                current,
            )
        };
        let committed = execution::Host::arguments_committed(&host);
        drop(host);
        let allocations = memory.stats();
        let canonical_indices = callback.resource_indices() as u64;
        let (body, usage, tasks, poisoned, report) = match outcome {
            Ok(outcome) => {
                let execution::Value::Frame(frame) = outcome.value else {
                    return Err(INVALID.into());
                };
                (
                    ResultBody::Value(frame),
                    outcome.usage,
                    outcome.tasks,
                    outcome.poisoned,
                    None,
                )
            }
            Err(mut failure) => {
                let report = project_failure(
                    &failure,
                    self.current.ceiling,
                    FailureCode {
                        digest: self.target.digest,
                        mode: self.current.mode,
                        entry: self.entry,
                        functions: self.target.module.unit().code().declarations().functions(),
                    },
                    allocations,
                    canonical_indices,
                );
                if report.as_ref().is_some_and(|(report, _)| report.poisoned) {
                    failure.poisoned = true;
                    instance.poison();
                }
                let (usage, tasks, poisoned) = (failure.usage, failure.tasks, failure.poisoned);
                let failure = Rc::new(failure);
                if let Some((report, bytes)) = &report {
                    accounting.latch_boundary_failure_from(
                        report.clone(),
                        bytes.clone(),
                        self.target.clone(),
                        failure.clone(),
                    );
                }
                (ResultBody::Failure(failure), usage, tasks, poisoned, report)
            }
        };
        *self.result = Some(CallResult {
            owner: ResultOwner::Instance(self.target.clone()),
            body,
            usage,
            tasks,
            committed,
            poisoned,
            allocations,
            canonical_indices,
            report,
        });
        Ok(())
    }
}
