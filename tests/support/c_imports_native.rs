//! A trusted scalar fixture adapter for the production native Runtime API.
//! MIR arithmetic/branches/import nodes execute as generated machine code;
//! these primitives use the shared typed frame storage and context allocator.
use super::*;
use lang_codegen_clif::owned::{self as n, Runtime};
use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{cleanup as clean, layout as l};

type Ledger<'a> = clean::Frame<'a, usize, fn(&mut usize, clean::Reason) -> Result<(), u32>>;
struct RuntimeState<'i, 'pool, 'layout, 'provider, 'control> {
    layout: &'layout n::FunctionLayout<'layout>,
    storage: l::FrameStorage<'layout, 'layout>,
    allocator: &'pool mut a::Allocator<lang_codegen_clif::c_abi::allocator::Provider<'provider>>,
    ledger: Option<Ledger<'i>>,
    environment: n::CEnvironment<'control>,
    usage: e::Usage,
    staged: Option<i64>,
    result: Option<i64>,
    failure: Option<(clean::ExecutionFailure, m::Span)>,
    report: Option<clean::Report>,
}
fn bad() -> clean::Cause {
    clean::Cause::Execution(clean::ExecutionFailure::Invariant)
}
fn storage_error(error: l::Error) -> clean::Cause {
    clean::Cause::Execution(
        e::service_failure(&error)
            .map(Into::into)
            .unwrap_or(clean::ExecutionFailure::Invariant),
    )
}
impl<'i, 'pool, 'layout, 'provider, 'control>
    RuntimeState<'i, 'pool, 'layout, 'provider, 'control>
{
    fn new(
        layout: &'layout n::FunctionLayout<'layout>,
        instance: &'i mut clean::Instance,
        allocator: &'pool mut a::Allocator<
            lang_codegen_clif::c_abi::allocator::Provider<'provider>,
        >,
        environment: n::CEnvironment<'control>,
    ) -> Result<Self, c::Fault> {
        let span = layout
            .source()
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .functions()[layout.function().0 as usize]
            .signature_span;
        let preparation =
            || c::Fault::new(c::Kind::Execution(clean::ExecutionFailure::AllocationLimit)).at(span);
        // This fixture's main contains only Copy scalar/Unit storage; no owner
        // cleaner or aggregate-layout substitute is used to claim full F04.
        for index in 0..layout.frame().cell_count() {
            let ty = layout.frame().cell_type(index).unwrap().ty;
            assert!(
                matches!(ty, o::Type::I64 | o::Type::Bool)
                    || matches!(ty, o::Type::Constructed(id) if matches!(layout.layouts().types().table().constructed[id.0 as usize].constructor, o::Constructor::Builtin(o::Builtin::Unit)))
            );
        }
        let cleaner: fn(&mut usize, clean::Reason) -> Result<(), u32> = |_, _| Ok(());
        let mut ledger = instance
            .enter(
                clean::Limits {
                    slots: layout.frame().cell_count(),
                    scopes: 1,
                    failure_records: environment.cleanup_records(),
                },
                cleaner,
            )
            .map_err(|_| preparation())?;
        for index in 0..layout.frame().cell_count() {
            ledger
                .declare(clean::Slot(index), clean::Kind::Owner)
                .map_err(|_| {
                    c::Fault::new(c::Kind::Execution(clean::ExecutionFailure::Invariant)).at(span)
                })?;
        }
        let storage = layout
            .frame()
            .reserve(allocator)
            .map_err(|_| preparation())?;
        Ok(Self {
            layout,
            storage,
            allocator,
            ledger: Some(ledger),
            environment,
            usage: e::Usage {
                peak_frames: 1,
                peak_live_cells: layout.control_cells(),
                ..Default::default()
            },
            staged: None,
            result: None,
            failure: None,
            report: None,
        })
    }
    fn finish(&mut self, exit: clean::Exit) -> n::ResultStep {
        if let Some(ledger) = self.ledger.take() {
            self.report = Some(ledger.finish(exit).report);
        }
        self.storage.release(self.allocator).map_err(storage_error)
    }
    fn ready(&self, cell: usize) -> n::ResultStep {
        self.ledger
            .as_ref()
            .ok_or_else(bad)?
            .value(clean::Slot(cell))
            .map(|_| ())
            .map_err(|_| bad())
    }
}
impl Runtime for RuntimeState<'_, '_, '_, '_, '_> {
    fn c_environment(&self) -> Option<&n::CEnvironment<'_>> {
        Some(&self.environment)
    }
    fn c_usage(&self) -> Option<e::Usage> {
        Some(e::Usage {
            service_work: self.environment.work().used(),
            ..self.usage
        })
    }
    fn set_c_usage(&mut self, usage: e::Usage) -> n::ResultStep {
        self.usage = usage;
        Ok(())
    }
    fn is_poisoned(&self) -> bool {
        self.ledger.as_ref().is_some_and(clean::Frame::is_poisoned)
    }
    fn enter(&mut self, layout: &n::FunctionLayout<'_>) -> n::ResultStep {
        if !std::ptr::eq(layout, self.layout) {
            return Err(bad());
        }
        self.environment
            .control()
            .check()
            .map_err(|e| clean::Cause::Execution(e.into()))?;
        if self.environment.control().limits().frames == Some(0) {
            return Err(clean::Cause::Execution(clean::ExecutionFailure::FrameLimit));
        }
        if self
            .environment
            .control()
            .limits()
            .live_cells
            .is_some_and(|limit| self.layout.control_cells() > limit)
        {
            return Err(clean::Cause::Execution(clean::ExecutionFailure::CellLimit));
        }
        Ok(())
    }
    fn charge(&mut self, _: m::Span) -> n::ResultStep {
        self.environment
            .control()
            .charge(&mut self.usage)
            .map_err(|e| clean::Cause::Execution(e.into()))
    }
    fn read_scalar(&mut self, cell: usize) -> Result<i64, clean::Cause> {
        self.ready(cell)?;
        let cell = self.storage.cell(cell).map_err(storage_error)?;
        match cell
            .read_immediate(cell.root(), self.allocator)
            .map_err(storage_error)?
        {
            l::Immediate::I64(value) => Ok(value),
            l::Immediate::Bool(value) => Ok(i64::from(value)),
            l::Immediate::Unit => Ok(0),
            _ => Err(bad()),
        }
    }
    fn write_scalar(&mut self, cell: usize, bits: i64, span: m::Span) -> n::ResultStep {
        let ty = self.layout.frame().cell_type(cell).ok_or_else(bad)?.ty;
        let value = match ty {
            o::Type::I64 => l::Immediate::I64(bits),
            o::Type::Bool if (0..=1).contains(&bits) => l::Immediate::Bool(bits == 1),
            o::Type::Constructed(_) if bits == 0 => l::Immediate::Unit,
            _ => return Err(bad()),
        };
        let target = self.storage.cell(cell).map_err(storage_error)?;
        target
            .write_immediate(target.root(), value, self.allocator)
            .map_err(storage_error)?;
        let ledger = self.ledger.as_mut().ok_or_else(bad)?;
        if ledger.value(clean::Slot(cell)).is_err() {
            ledger
                .initialize(clean::Slot(cell), cell, Some(span))
                .map_err(|_| bad())?;
        }
        Ok(())
    }
    fn copy_slot(&mut self, from: usize, to: usize, span: m::Span) -> n::ResultStep {
        let bits = self.read_scalar(from)?;
        self.write_scalar(to, bits, span)
    }
    fn move_slot(&mut self, from: usize, to: usize, span: m::Span) -> n::ResultStep {
        self.copy_slot(from, to, span)
    }
    fn transfer(&mut self, site: &n::TransferSite) -> n::ResultStep {
        for movement in &site.moves {
            let span = self
                .layout
                .frame()
                .cell_type(movement.scratch)
                .ok_or_else(bad)?
                .span;
            self.copy_slot(movement.source, movement.scratch, span)?;
        }
        for movement in &site.moves {
            let span = self
                .layout
                .frame()
                .cell_type(movement.target)
                .ok_or_else(bad)?
                .span;
            self.copy_slot(movement.scratch, movement.target, span)?;
        }
        Ok(())
    }
    fn cleanup(&mut self, slots: &[(usize, m::Span)]) -> n::ResultStep {
        for (slot, _) in slots {
            let ledger = self.ledger.as_mut().ok_or_else(bad)?;
            if ledger.value(clean::Slot(*slot)).is_ok() {
                ledger.drop_value(clean::Slot(*slot)).map_err(|_| bad())?;
            }
        }
        Ok(())
    }
    fn stage_return(&mut self, slot: usize, _: m::Span) -> n::ResultStep {
        self.staged = Some(self.read_scalar(slot)?);
        Ok(())
    }
    fn retire_return(&mut self) -> n::ResultStep {
        // Copy result bits remain in escrow while normal scope obligations
        // retire. Failure retirement/discard suppresses their publication.
        Ok(())
    }
    fn publish_return(&mut self) -> n::ResultStep {
        self.finish(clean::Exit::Return(None))?;
        self.result = self.staged.take();
        Ok(())
    }
    fn record_failure(&mut self, cause: clean::Cause, span: m::Span) -> n::ResultStep {
        let clean::Cause::Execution(failure) = cause else {
            return Err(bad());
        };
        self.failure.get_or_insert((failure, span));
        Ok(())
    }
    fn retire_failure(&mut self) -> n::ResultStep {
        let (failure, span) = self.failure.ok_or_else(bad)?;
        self.staged = None;
        self.finish(clean::Exit::Failure {
            failure,
            span: Some(span),
        })
    }
    fn discard_result(&mut self) -> n::ResultStep {
        self.staged = None;
        self.result = None;
        Ok(())
    }
}

pub(super) fn observe(
    compiled: &n::CCompiledProgram<'_>,
    artifact: &Artifact,
    library: &Library,
    control: e::Control<'_>,
    fail_allocation: bool,
) -> Observed {
    let stats = Stats::default();
    stats.fail.set(u32::from(fail_allocation));
    let program = c::Program::new(
        artifact,
        compiled,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
    let bindings = bindings(artifact, library, &stats);
    let context = program
        .create(config::Config {
            version: 1,
            size: 80,
            allocator: Some(&allocator),
            limits: Some(&limits),
            imports: Some(&bindings),
            import_count: bindings.len() as u64,
            artifact: artifact.digest(),
            flags: 0,
        })
        .unwrap();
    // SAFETY: generated typedefs checked the exact C code retained by Library;
    // all callback user/context storage remains live for the adapter borrow.
    let mut invoker = unsafe { NativeInvoker::new(&context, Default::default()) }.unwrap();
    // SAFETY: the retained C provider obeys complete/unique storage and exact
    // matching deallocation, with no unwind/reentry throughout this borrow.
    let mut allocations = unsafe { CAllocator::new(&context) }.unwrap();
    let mut usage = e::Usage::default();
    let work = e::ServiceWork::new(control.service_work_limit());
    let mut value = None;
    let result = context.run(
        &program,
        control,
        &work,
        &mut usage,
        c::EntryPoint::Guest,
        &mut |entry| {
            let (instance, session) = entry.parts();
            let environment = compiled.prepare(session, &invoker, allocations.managed())?;
            let main = m::FunctionId(0);
            let runtime = RuntimeState::new(
                compiled.layout(main).unwrap(),
                instance,
                allocations.managed_mut(),
                environment,
            )?;
            let mut runtime = n::RuntimeInstance::new(runtime);
            let native = compiled.invoke(main, session, &mut invoker, &mut runtime);
            value = runtime.runtime().result;
            let report = runtime.runtime_mut().report.take().unwrap();
            if let Some(fault) = c::Fault::from_report(&report, 0) {
                return Err(fault);
            }
            native.map_err(|failure| match failure.reason {
                n::Reason::Foreign(fault) => fault,
                _ => c::Fault::new(c::Kind::Execution(clean::ExecutionFailure::Fault)),
            })
        },
    );
    assert_eq!(allocations.managed().stats().live_allocations, 0);
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
    let fault = result.err();
    Observed {
        value: if fault.is_none() { value } else { None },
        fault,
        usage,
        effects: stats.effects.get(),
        summary: context.last_error().unwrap(),
        poisoned: context.is_poisoned().unwrap(),
    }
}

#[test]
fn source_c_callbacks_execute_in_generated_native_code() {
    for (body, expected, raw) in [
        ("unsafe { return increment(41); }", Some(42), None),
        (
            "unsafe { let mut notice: core::Unit = notify(core::Unit {}); if flag(true) { return combine(3, true, core::Unit {}, 5, false, 7, true, 11, 13, true, 17, false); } return 0; }",
            Some(59),
            None,
        ),
        ("unsafe { return increment(-1); }", None, Some(2)),
        ("unsafe { return increment(-2); }", None, Some(u32::MAX)),
    ] {
        let text = format!("{IMPORTS} fn main() -> i64 {{ {body} }}");
        let owner = source(&text);
        let flow = owner.failure_flow();
        let types = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let artifact = artifact(&text, target(), Limits::default()).unwrap();
        let compiled = n::CCompiledProgram::compile_scalar_imports(
            flow,
            &layouts,
            &artifact,
            Default::default(),
            4096,
        )
        .unwrap();
        let library = Library::new(&artifact);
        let stats = Stats::default();
        let program = c::Program::new(
            &artifact,
            &compiled,
            config::Capacities::from_strides(64, 16, 64).unwrap(),
        )
        .unwrap();
        let allocator = library.allocator(&stats);
        let limits = config::LimitsDescriptor::default();
        let bindings = bindings(&artifact, &library, &stats);
        let context = program
            .create(config::Config {
                version: 1,
                size: 80,
                allocator: Some(&allocator),
                limits: Some(&limits),
                imports: Some(&bindings),
                import_count: bindings.len() as u64,
                artifact: artifact.digest(),
                flags: 0,
            })
            .unwrap();
        // SAFETY: generated typedef checks prove every fixture C signature;
        // image, user cells and context outlive both adapters and every call.
        let mut invoker = unsafe { NativeInvoker::new(&context, Default::default()) }.unwrap();
        let mut allocations = unsafe { CAllocator::new(&context) }.unwrap();
        let mut usage = e::Usage::default();
        let work = e::ServiceWork::new(Some(100_000));
        let mut saved = None;
        let result = context.run(
            &program,
            e::Control::default().and_service_work(Some(100_000)),
            &work,
            &mut usage,
            c::EntryPoint::Guest,
            &mut |entry| {
                let (instance, session) = entry.parts();
                let environment = compiled.prepare(session, &invoker, allocations.managed())?;
                let main = m::FunctionId(0);
                assert_eq!(compiled.metadata(main).unwrap().symbol, "lang_owned_r2_f0");
                let runtime = RuntimeState::new(
                    compiled.layout(main).unwrap(),
                    instance,
                    allocations.managed_mut(),
                    environment,
                )?;
                let mut runtime = n::RuntimeInstance::new(runtime);
                let native = compiled.invoke(main, session, &mut invoker, &mut runtime);
                let scalar = runtime.runtime().result;
                let report = runtime.runtime_mut().report.take().unwrap();
                let fault = c::Fault::from_report(&report, 0);
                saved = Some((scalar, native, report));
                fault.map_or(Ok(()), Err)
            },
        );
        let (scalar, native, report) = saved.unwrap();
        if let Some(expected) = expected {
            assert!(result.is_ok(), "{result:?}");
            assert!(native.is_ok(), "{native:?}");
            assert_eq!(scalar, Some(expected));
            assert!(report.primary().is_none());
        } else {
            let failure = result.unwrap_err();
            let code = raw.unwrap();
            assert_eq!(
                failure.kind,
                c::Kind::ForeignStatus {
                    code,
                    known: code <= 6
                }
            );
            assert_eq!(failure.status(), c::Status::Runtime);
            assert!(matches!(native.unwrap_err().reason, n::Reason::Foreign(_)));
            assert!(scalar.is_none());
            assert!(report.primary().is_some());
        }
        assert!(usage.fuel > 0);
        assert!(usage.host_calls > 0);
        assert_eq!(usage.service_work, work.used());
        assert_eq!(allocations.managed().stats().live_allocations, 0);
        assert_eq!(stats.allocations.get(), stats.frees.get());
        assert_eq!(stats.wrong.get(), 0);
        assert!(!context.is_poisoned().unwrap());
    }
}
