//! RFC 0012 ownership native call boundary. Managed payloads stay in the shared
//! runtime layouts; packets contain only an invocation context and frame IDs.
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, failure as f},
};
use lang_runtime::layout::{self as l, FramePlan, Layouts};
use std::{
    ffi::c_void,
    fmt::{Display, Formatter},
};

use super::OwnedJitModule;
use cranelift_codegen::ir::{
    AbiParam, Block, InstBuilder, MemFlagsData, SigRef, SourceLoc, Value, types,
};
use cranelift_codegen::settings::Configurable;
use cranelift_codegen::{isa, settings};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use lang_runtime::cleanup as c;
use std::{
    mem::offset_of,
    panic::{AssertUnwindSafe, catch_unwind},
};

pub type ResultStep = Result<(), c::Cause>;

/// Prepared host capacity for the instrumented ownership profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlCapacity {
    pub frames: usize,
    pub live_cells: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlLimit {
    Frames,
    LiveCells,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedControl {
    pub limit: ControlLimit,
    pub requested: Option<usize>,
    pub capacity: usize,
}
impl ControlCapacity {
    /// Reject unsupported bounds before guest allocation or ownership transfer.
    /// An unbounded request cannot be satisfied by a finite prepared pool.
    pub fn admit(
        self,
        control: lang_runtime::execution::Control<'_>,
    ) -> Result<(), UnsupportedControl> {
        let limits = control.limits();
        for (limit, requested, capacity) in [
            (ControlLimit::Frames, limits.frames, self.frames),
            (ControlLimit::LiveCells, limits.live_cells, self.live_cells),
        ] {
            if !requested.is_some_and(|requested| requested <= capacity) {
                return Err(UnsupportedControl {
                    limit,
                    requested,
                    capacity,
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Transition {
    pub frame: u64,
    pub function: m::FunctionId,
    pub continuation: u32,
}

pub struct CallSite {
    pub function: m::FunctionId,
    pub arguments: Vec<usize>,
    pub result: usize,
    pub resume: u32,
    pub failure: u32,
    pub span: m::Span,
}

/// Statically prepared record operands. Fields use declaration order; the
/// separate initializer ledger preserves source evaluation order and spans.
pub struct RecordSite {
    pub result: usize,
    pub fields: Vec<usize>,
    pub initialization_order: Vec<(u32, m::Span)>,
    pub span: m::Span,
}
pub struct VariantSite {
    pub case: u32,
    pub payload: RecordSite,
}
pub struct ExtractSite {
    pub source: usize,
    pub result: usize,
    pub case: u32,
    pub guard: Option<usize>,
    pub panic_code: Option<i64>,
}
pub struct CaseFieldSite {
    pub source: usize,
    pub result: usize,
    pub case: u32,
    pub field: u32,
    pub guard: usize,
}
pub struct TransferMove {
    pub source: usize,
    pub scratch: usize,
    pub target: usize,
}
pub struct TransferSite {
    pub moves: Vec<TransferMove>,
}
pub struct PlaceSite {
    pub root: usize,
    pub projections: Vec<o::places::Projection>,
    pub span: m::Span,
}
pub struct PlaceLoad {
    pub place: PlaceSite,
    pub result: usize,
}
pub struct BorrowSite {
    pub loan: b::LoanId,
    pub kind: o::LoanKind,
    pub place: PlaceSite,
    pub result: usize,
    pub span: m::Span,
}
pub struct BorrowCaseSite {
    pub loan: b::LoanId,
    pub kind: o::LoanKind,
    pub field: CaseFieldSite,
    pub span: m::Span,
}
pub struct PreparedSite {
    pub destination: b::PreparedPlaceId,
    pub place: PlaceSite,
}
pub enum ReplacementDestination {
    Prepared(b::PreparedPlaceId),
    Place(PlaceSite),
}
pub struct ReplaceSite {
    pub destination: ReplacementDestination,
    pub value: usize,
    pub span: m::Span,
}
/// A lexical arena admission bound to typed frame slots before code generation.
pub struct ArenaSite {
    pub owner: usize,
    pub allocator: usize,
    pub span: m::Span,
}

/// A statically bound core intrinsic and its typed operands, not a MIR opcode
/// stream. Trusted adapters must enforce allocator authority and configured
/// byte-work admission before invoking allocation or scanning services.
pub struct CoreSite {
    pub operation: o::core::Operation,
    pub type_arguments: Vec<o::Type>,
    pub arguments: Vec<usize>,
    pub result: usize,
    pub loan: Option<b::LoanId>,
    pub span: m::Span,
}

/// Trusted prepared-frame primitives, not an instruction interpreter. Methods
/// receive statically lowered slots/ranges. Implementations supply the shared
/// runtime's ownership ledger, typed storage, control and cleanup services.
/// No method may retain a temporary reference or invocation packet pointer.
/// Production adapters must preserve failure reports and admission atomically.
/// `enter` checks program/layout identity and limits before accepting parameters.
/// Existing cleanup reports are returned as `Cause::Reported`; an ordinary
/// reported guest failure is not an invariant fault. `is_poisoned` includes the
/// underlying shared cleanup instance, not only the native panic flag. Adapter
/// bookkeeping must be reserved before entry; callbacks must mark consumed
/// cleanup attempts before any external effect that can fail or panic.
/// Check configured service-work support before guest ownership/effects using
/// the shared Control guard and the canonical profile capability.
pub trait Runtime {
    fn is_poisoned(&self) -> bool;
    fn enter(&mut self, layout: &FunctionLayout<'_>) -> ResultStep;
    fn charge(&mut self, span: m::Span) -> ResultStep;
    fn move_slot(&mut self, source: usize, target: usize, span: m::Span) -> ResultStep;
    fn cleanup(&mut self, slots: &[(usize, m::Span)]) -> ResultStep;
    fn stage_return(&mut self, slot: usize, span: m::Span) -> ResultStep;
    fn retire_return(&mut self) -> ResultStep;
    fn publish_return(&mut self) -> ResultStep;
    fn record_failure(&mut self, cause: c::Cause, span: m::Span) -> ResultStep;
    fn retire_failure(&mut self) -> ResultStep;
    fn discard_result(&mut self) -> ResultStep;
    /// Transfer prepared fields (copy Copy values, consume owners) into
    /// admitted record storage and publish its cleanup obligation without
    /// allocating or running guest code.
    fn construct_record(&mut self, _: &RecordSite) -> ResultStep {
        Err(c::Cause::Execution(c::ExecutionFailure::Invariant))
    }
    /// Return i64 bits, canonical 0/1 for bool, or zero for Unit. Reads must
    /// validate the active typed cell; arithmetic is emitted as native code.
    fn read_scalar(&mut self, _: usize) -> Result<i64, c::Cause> {
        missing()
    }
    fn write_scalar(&mut self, _: usize, _: i64, _: m::Span) -> ResultStep {
        missing()
    }
    fn copy_slot(&mut self, _: usize, _: usize, _: m::Span) -> ResultStep {
        missing()
    }
    fn variant_case(&mut self, _: usize) -> Result<u32, c::Cause> {
        missing()
    }
    fn construct_variant(&mut self, _: &VariantSite) -> ResultStep {
        missing()
    }
    /// Extract a complete Option/Result payload. Copy containers remain
    /// available; an owning container transfers its single payload atomically.
    fn extract_variant(&mut self, _: &ExtractSite) -> ResultStep {
        missing()
    }
    fn copy_case_field(&mut self, _: &CaseFieldSite) -> ResultStep {
        missing()
    }
    /// Capture all arguments into admitted scratch cells before publishing
    /// any block parameter. Copy fanout and affine cycles must be simultaneous.
    fn transfer(&mut self, _: &TransferSite) -> ResultStep {
        missing()
    }
    fn load_place(&mut self, _: &PlaceLoad) -> ResultStep {
        missing()
    }
    fn borrow_place(&mut self, _: &BorrowSite) -> ResultStep {
        missing()
    }
    fn borrow_case(&mut self, _: &BorrowCaseSite) -> ResultStep {
        missing()
    }
    fn end_borrow(&mut self, _: b::LoanId, _: b::EndMode) -> ResultStep {
        missing()
    }
    fn prepare_place(&mut self, _: &PreparedSite) -> ResultStep {
        missing()
    }
    fn replace_place(&mut self, _: &ReplaceSite) -> ResultStep {
        missing()
    }
    /// Drop one capture or every capture in the current frame. This must not
    /// allocate or run guest effects, including while unwinding a failure.
    fn discard_prepared(&mut self, _: Option<b::PreparedPlaceId>) {}
    fn enter_arena(&mut self, _: &ArenaSite) -> ResultStep {
        missing()
    }
    fn core_call(&mut self, _: &CoreSite) -> ResultStep {
        missing()
    }
    fn frame_identity(&self) -> u64 {
        0
    }
    /// The adapter reserves/adopts the callee before consuming caller owners.
    fn prepare_call(
        &mut self,
        _: &CallSite,
        _: &FunctionLayout<'_>,
    ) -> Result<Transition, c::Cause> {
        Err(c::Cause::Execution(c::ExecutionFailure::Invariant))
    }
    /// Called after successful result publication, when the callee is retired.
    fn after_return(&mut self) -> Result<Option<Transition>, c::Cause> {
        Ok(None)
    }
    /// Unwind one logical call and choose its caller's failure continuation.
    fn after_failure(&mut self) -> Result<Option<Transition>, c::Cause> {
        Ok(None)
    }
    /// Validate against the adapter's prepared transition and live frame state.
    fn validate_transition(&self, _: Transition) -> bool {
        false
    }
}
fn missing<T>() -> Result<T, c::Cause> {
    Err(c::Cause::Execution(c::ExecutionFailure::Invariant))
}

/// Owns the adapter together with the native boundary's permanent panic state.
/// Creating another instance does not grant authority to reuse poisoned values.
pub struct RuntimeInstance<R> {
    runtime: R,
    poisoned: bool,
    quarantined_panics: usize,
}
impl<R> RuntimeInstance<R> {
    pub const fn new(runtime: R) -> Self {
        Self {
            runtime,
            poisoned: false,
            quarantined_panics: 0,
        }
    }
    /// Secondary panic payloads whose destructor could not be safely retried.
    pub const fn quarantined_panics(&self) -> usize {
        self.quarantined_panics
    }
    pub const fn runtime(&self) -> &R {
        &self.runtime
    }
    pub fn runtime_mut(&mut self) -> &mut R {
        &mut self.runtime
    }
}
impl<R: Runtime> RuntimeInstance<R> {
    pub fn is_poisoned(&self) -> bool {
        self.poisoned || self.runtime.is_poisoned()
    }
}

/// The canonical owned runtime profile meters all current service families.
pub const SERVICE_WORK_SUPPORTED: bool = true;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    Poisoned,
    UnsupportedControl(UnsupportedControl),
    UnsupportedServiceWork(lang_runtime::execution::UnsupportedServiceWork),
    Runtime(c::Cause),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Failure {
    pub function: m::FunctionId,
    pub reason: Reason,
    pub span: m::Span,
    /// Source core panic payload, retained with the first failing function/span.
    pub panic_code: Option<i64>,
}

type Helper = unsafe extern "C" fn(*mut c_void, u64, u64, u64) -> i32;
const HELPERS: usize = 33;
#[repr(C)]
struct Header {
    data: *mut c_void,
    helpers: [Helper; HELPERS],
}

#[derive(Clone, Copy)]
enum Action {
    Move(usize, usize),
    Cleanup,
    Call,
    Record,
    Constant(usize, i64),
    Binary(Binary, usize, usize, usize),
    Copy(usize, usize),
    IsCase(usize, u32, usize),
    Variant,
    Extract,
    CaseField,
    PlaceLoad,
    Borrow,
    BorrowCase,
    EndBorrow(b::LoanId, b::EndMode),
    Prepare,
    Replace,
    DiscardPrepared(b::PreparedPlaceId),
    Core,
    ArenaEnter,
}
#[derive(Clone, Copy)]
enum Binary {
    Add,
    Less,
    Equal,
}
struct Node {
    span: m::Span,
    action: Option<Action>,
    cleanup: Vec<(usize, m::Span)>,
    raised: Option<c::ExecutionFailure>,
    call: Option<CallSite>,
    record: Option<RecordSite>,
    variant: Option<VariantSite>,
    extract: Option<ExtractSite>,
    case_field: Option<CaseFieldSite>,
    transfer: Option<TransferSite>,
    place: Option<PlaceLoad>,
    borrow: Option<BorrowSite>,
    borrow_case: Option<BorrowCaseSite>,
    prepared: Option<PreparedSite>,
    replace: Option<ReplaceSite>,
    core: Option<CoreSite>,
    arena: Option<ArenaSite>,
}

/// Machine-code lowering for supported owned MIR and trusted runtime hooks.
/// Unsupported operations are diagnosed before invocation. Full service
/// conformance, production drivers and linked execution remain milestone work.
pub struct CompiledFunction<'a> {
    _owner: OwnedJitModule,
    code: *const u8,
    layout: FunctionLayout<'a>,
    nodes: Vec<Node>,
    usage: m::Usage,
    metadata: super::program::FunctionCode,
    continuations: Vec<u32>,
    failure_node: u32,
}

#[derive(Clone, Copy)]
enum CompileScope {
    Function,
    Unit,
}

impl<'a> CompiledFunction<'a> {
    pub fn compile(
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        function: m::FunctionId,
        limits: m::Limits,
        cells: usize,
    ) -> Result<Self, Error> {
        Self::compile_after(
            flow,
            layouts,
            function,
            limits,
            cells,
            None,
            CompileScope::Function,
        )
    }
    fn compile_after(
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        function: m::FunctionId,
        limits: m::Limits,
        cells: usize,
        prefix: Option<m::Usage>,
        scope: CompileScope,
    ) -> Result<Self, Error> {
        let Prepared {
            layout,
            nodes,
            continuations,
            mut budget,
        } = prepare(flow, layouts, function, limits, cells, prefix, scope)?;
        let graph = &flow.functions()[function.0 as usize];
        let mut owner = OwnedJitModule(Some(JITModule::new(
            JITBuilder::new(default_libcall_names()).map_err(backend)?,
        )));
        let module = owner.0.as_mut().expect("new module");
        let (id, metadata) = emit(module, graph, &layout, &nodes, &continuations, &mut budget)?;
        module.finalize_definitions().map_err(backend)?;
        let code = module.get_finalized_function(id);
        let usage = m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        };
        Ok(Self {
            _owner: owner,
            code,
            layout,
            nodes,
            usage,
            metadata,
            continuations,
            failure_node: graph.failure().0,
        })
    }

    /// Cumulative proof/layout/native metadata usage. Cranelift's internal
    /// allocations remain the existing trusted compiler boundary.
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub const fn metadata(&self) -> &super::program::FunctionCode {
        &self.metadata
    }

    pub const fn layout(&self) -> &FunctionLayout<'a> {
        &self.layout
    }

    pub fn invoke<R: Runtime>(&self, instance: &mut RuntimeInstance<R>) -> Result<(), Failure> {
        run(self, None, instance)
    }
}

/// Whole-unit native continuations share one cumulative preparation budget.
/// Dispatch selects compiled addresses and never evaluates a MIR operation.
pub struct CompiledProgram<'a> {
    functions: Vec<CompiledFunction<'a>>,
    usage: m::Usage,
}
impl<'a> CompiledProgram<'a> {
    pub fn compile(
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        limits: m::Limits,
        cells: usize,
    ) -> Result<Self, Error> {
        if flow.functions().is_empty() {
            return Err(Error::InvalidFunction(m::FunctionId(0)));
        }
        if !std::ptr::eq(layouts.types(), typed(flow).places().declarations().types()) {
            return Err(Error::Layout(l::Error::ForeignLocation));
        }
        let mut budget = m::Budget::with_usage(limits, FunctionLayout::base_usage(flow, layouts)?)?;
        let mut functions = budget.vector(flow.functions().len())?;
        let mut usage = m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        };
        for index in 0..flow.functions().len() {
            let function = CompiledFunction::compile_after(
                flow,
                layouts,
                m::FunctionId(index as u32),
                limits,
                cells,
                Some(usage),
                CompileScope::Unit,
            )?;
            usage = function.usage;
            functions.push(function);
        }
        Ok(Self { functions, usage })
    }
    fn compiled(&self, function: m::FunctionId) -> Option<&CompiledFunction<'a>> {
        self.functions.get(function.0 as usize)
    }
    pub fn layout(&self, function: m::FunctionId) -> Option<&FunctionLayout<'a>> {
        self.compiled(function).map(|f| &f.layout)
    }
    pub fn metadata(&self, function: m::FunctionId) -> Option<&super::program::FunctionCode> {
        self.compiled(function).map(|f| &f.metadata)
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub fn invoke<R: Runtime>(
        &self,
        function: m::FunctionId,
        instance: &mut RuntimeInstance<R>,
    ) -> Result<(), Failure> {
        let entry = self.compiled(function).ok_or(Failure {
            function,
            reason: Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::Invariant)),
            span: self.functions[0].nodes[0].span,
            panic_code: None,
        })?;
        run(entry, Some(self), instance)
    }
}

fn run<'c, 'a, R: Runtime>(
    entry: &'c CompiledFunction<'a>,
    program: Option<&'c CompiledProgram<'a>>,
    instance: &'c mut RuntimeInstance<R>,
) -> Result<(), Failure> {
    let rejected = |reason| Failure {
        function: entry.layout.function,
        reason,
        span: entry.nodes[0].span,
        panic_code: None,
    };
    if instance.is_poisoned() {
        return Err(rejected(Reason::Poisoned));
    }
    let mut context = Invocation {
        dispatch: DispatchState {
            next: Output {
                frame: 0,
                function: 0,
                continuation: 0,
            },
            frame: instance.runtime.frame_identity(),
            scalar: 0,
        },
        compiled: entry,
        program,
        instance,
        fault: None,
        report_recorded: false,
    };
    let mut header = Header {
        data: (&mut context as *mut Invocation<'_, '_, R>).cast(),
        helpers: [
            entry_helper::<R>,
            charge::<R>,
            move_slot::<R>,
            cleanup::<R>,
            stage::<R>,
            retire::<R>,
            publish::<R>,
            record::<R>,
            retire_failure::<R>,
            discard::<R>,
            raise::<R>,
            prepare_call::<R>,
            after_return::<R>,
            after_failure::<R>,
            construct_record::<R>,
            read_scalar::<R>,
            write_scalar::<R>,
            copy_slot::<R>,
            variant_case::<R>,
            construct_variant::<R>,
            extract_variant::<R>,
            transfer::<R>,
            panic::<R>,
            copy_case_field::<R>,
            borrow_place::<R>,
            load_place::<R>,
            end_borrow::<R>,
            prepare_place::<R>,
            replace_place::<R>,
            discard_prepared::<R>,
            borrow_case::<R>,
            core_call::<R>,
            enter_arena::<R>,
        ],
    };
    let mut continuation = 0;
    loop {
        let input = Input {
            context: (&mut header as *mut Header).cast(),
            frame: context.dispatch.frame,
            continuation,
            reserved: 0,
        };
        let mut output = Output {
            frame: u64::MAX,
            function: u32::MAX,
            continuation: u32::MAX,
        };
        // SAFETY: emit produces precisely this target-native C signature. Both
        // packets/header and the repr(C) DispatchState prefix have the exact
        // extents used by lowering. The opaque pointer borrows this Invocation
        // with matching R callbacks; callbacks contain all Rust panics and
        // retain nothing. Every code owner remains borrowed throughout the loop.
        let status = unsafe {
            let call: unsafe extern "C" fn(*const Input, *mut Output) -> i32 =
                std::mem::transmute(context.compiled.code);
            call(&input, &mut output)
        };
        if status == 1 {
            let next = Transition {
                frame: output.frame,
                function: m::FunctionId(output.function),
                continuation: output.continuation,
            };
            if output != context.dispatch.next
                || !valid_target(program, next, context.fault.is_some())
            {
                context.note(
                    c::Cause::Execution(c::ExecutionFailure::Invariant),
                    context.compiled.nodes[0].span,
                );
                return Err(context.fault.expect("protocol failure"));
            }
            context.compiled = program
                .and_then(|p| p.compiled(next.function))
                .expect("sealed target");
            context.dispatch.frame = next.frame;
            continuation = next.continuation;
            // Poison blocks new outer entries, while admitted failure exits
            // continue dispatching until every logical caller is retired.
            continue;
        }
        return match (status, context.fault) {
            (0, None) => Ok(()),
            (_, Some(fault)) => Err(fault),
            _ => {
                context.instance.poisoned = true;
                Err(rejected(Reason::Runtime(c::Cause::Execution(
                    c::ExecutionFailure::Invariant,
                ))))
            }
        };
    }
}

fn valid_target(program: Option<&CompiledProgram<'_>>, target: Transition, failure: bool) -> bool {
    program
        .and_then(|p| p.compiled(target.function))
        .is_some_and(|f| {
            f.continuations.contains(&target.continuation)
                && (!failure || target.continuation == f.failure_node)
        })
}

pub struct ObjectFunction<'a> {
    layout: FunctionLayout<'a>,
    nodes: Vec<Node>,
    continuations: Vec<u32>,
    metadata: super::program::FunctionCode,
}
impl<'a> ObjectFunction<'a> {
    pub const fn layout(&self) -> &FunctionLayout<'a> {
        &self.layout
    }
    pub const fn metadata(&self) -> &super::program::FunctionCode {
        &self.metadata
    }
    pub fn continuations(&self) -> &[u32] {
        &self.continuations
    }
    pub fn span(&self, node: u32) -> Option<m::Span> {
        self.nodes.get(node as usize).map(|n| n.span)
    }
    pub fn call_site(&self, node: u32) -> Option<&CallSite> {
        self.nodes.get(node as usize)?.call.as_ref()
    }
    pub fn record_site(&self, node: u32) -> Option<&RecordSite> {
        self.nodes.get(node as usize)?.record.as_ref()
    }
    pub fn variant_site(&self, node: u32) -> Option<&VariantSite> {
        self.nodes.get(node as usize)?.variant.as_ref()
    }
    pub fn transfer_site(&self, node: u32) -> Option<&TransferSite> {
        self.nodes.get(node as usize)?.transfer.as_ref()
    }
    pub fn extract_site(&self, node: u32) -> Option<&ExtractSite> {
        self.nodes.get(node as usize)?.extract.as_ref()
    }
    pub fn case_field_site(&self, node: u32) -> Option<&CaseFieldSite> {
        self.nodes.get(node as usize)?.case_field.as_ref()
    }
    pub fn place_site(&self, node: u32) -> Option<&PlaceLoad> {
        self.nodes.get(node as usize)?.place.as_ref()
    }
    pub fn borrow_site(&self, node: u32) -> Option<&BorrowSite> {
        self.nodes.get(node as usize)?.borrow.as_ref()
    }
    pub fn borrow_case_site(&self, node: u32) -> Option<&BorrowCaseSite> {
        self.nodes.get(node as usize)?.borrow_case.as_ref()
    }
    pub fn prepared_site(&self, node: u32) -> Option<&PreparedSite> {
        self.nodes.get(node as usize)?.prepared.as_ref()
    }
    pub fn replace_site(&self, node: u32) -> Option<&ReplaceSite> {
        self.nodes.get(node as usize)?.replace.as_ref()
    }
    pub fn end_borrow_site(&self, node: u32) -> Option<(b::LoanId, b::EndMode)> {
        match self.nodes.get(node as usize)?.action {
            Some(Action::EndBorrow(loan, mode)) => Some((loan, mode)),
            _ => None,
        }
    }
    pub fn arena_site(&self, node: u32) -> Option<&ArenaSite> {
        self.nodes.get(node as usize)?.arena.as_ref()
    }
    pub fn core_site(&self, node: u32) -> Option<&CoreSite> {
        self.nodes.get(node as usize)?.core.as_ref()
    }
    pub fn cleanup(&self, node: u32) -> Option<&[(usize, m::Span)]> {
        self.nodes.get(node as usize).map(|n| n.cleanup.as_slice())
    }
}

/// Native object bytes and their matching in-memory invocation metadata. The
/// metadata is not a Rust-layout wire format or a self-contained runtime image.
pub struct ObjectArtifact<'a> {
    pub bytes: Vec<u8>,
    functions: Vec<ObjectFunction<'a>>,
    usage: m::Usage,
}
impl<'a> ObjectArtifact<'a> {
    pub fn function(&self, function: m::FunctionId) -> Option<&ObjectFunction<'a>> {
        self.functions.get(function.0 as usize)
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
}

pub fn emit_object_for_target<'a>(
    flow: &'a f::Elaborated,
    layouts: &'a Layouts<'a>,
    target: &crate::Triple,
    limits: m::Limits,
    cells: usize,
) -> Result<ObjectArtifact<'a>, Error> {
    if !std::ptr::eq(layouts.types(), typed(flow).places().declarations().types()) {
        return Err(Error::Layout(l::Error::ForeignLocation));
    }
    let mut flags = settings::builder();
    flags.set("is_pic", "true").map_err(backend)?;
    let isa = isa::lookup(target.clone())
        .map_err(backend)?
        .finish(settings::Flags::new(flags))
        .map_err(backend)?;
    let mut module = ObjectModule::new(
        ObjectBuilder::new(isa, "lang-owned-r1", default_libcall_names()).map_err(backend)?,
    );
    let mut budget = m::Budget::with_usage(limits, FunctionLayout::base_usage(flow, layouts)?)?;
    let mut functions = budget.vector(flow.functions().len())?;
    let mut usage = m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    };
    for index in 0..flow.functions().len() {
        let function = m::FunctionId(index as u32);
        let Prepared {
            layout,
            nodes,
            continuations,
            mut budget,
        } = prepare(
            flow,
            layouts,
            function,
            limits,
            cells,
            Some(usage),
            CompileScope::Unit,
        )?;
        let (_, metadata) = emit(
            &mut module,
            &flow.functions()[index],
            &layout,
            &nodes,
            &continuations,
            &mut budget,
        )?;
        usage = m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        };
        functions.push(ObjectFunction {
            layout,
            nodes,
            continuations,
            metadata,
        });
    }
    let bytes = module.finish().emit().map_err(backend)?;
    Ok(ObjectArtifact {
        bytes,
        functions,
        usage,
    })
}

struct Prepared<'a> {
    layout: FunctionLayout<'a>,
    nodes: Vec<Node>,
    continuations: Vec<u32>,
    budget: m::Budget,
}
fn prepare<'a>(
    flow: &'a f::Elaborated,
    layouts: &'a Layouts<'a>,
    function: m::FunctionId,
    limits: m::Limits,
    cells: usize,
    prefix: Option<m::Usage>,
    scope: CompileScope,
) -> Result<Prepared<'a>, Error> {
    let layout = FunctionLayout::compile_after(flow, layouts, function, limits, cells, prefix)?;
    let body = &typed(flow).functions()[function.0 as usize];
    let unsupported = |span| Error::Unsupported { function, span };
    let graph = &flow.functions()[function.0 as usize];
    let mut budget = m::Budget::with_usage(limits, layout.frame.usage())?;
    let mut nodes = budget.vector(graph.nodes().len())?;
    budget.charge(graph.nodes().len())?;
    let call_count = body
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|i| matches!(i.op, b::Op::Call { .. }))
        .count();
    let capacity = call_count
        .checked_mul(2)
        .and_then(|n| n.checked_add(2))
        .ok_or(Error::Layout(l::Error::Overflow))?;
    let mut continuations = budget.vector(capacity)?;
    continuations.push(graph.entry().0);
    if matches!(scope, CompileScope::Unit) {
        continuations.push(graph.failure().0);
    }
    for node in graph.nodes() {
        budget.charge(1)?;
        let mut data = Node {
            span: node.span,
            action: None,
            cleanup: Vec::new(),
            raised: None,
            call: None,
            record: None,
            variant: None,
            extract: None,
            case_field: None,
            transfer: None,
            place: None,
            borrow: None,
            borrow_case: None,
            prepared: None,
            replace: None,
            core: None,
            arena: None,
        };
        match node.kind {
            f::Kind::Execute { site, .. } => {
                // Order zero names block parameters; source instructions
                // use one-based definition order.
                let i = site
                    .order
                    .checked_sub(1)
                    .and_then(|order| body.blocks[site.block.0 as usize].instructions.get(order))
                    .ok_or_else(|| unsupported(node.span))?;
                let value = |v| layout.frame.value(v).map_err(Error::from);
                let local = |v| layout.frame.local(v).map_err(Error::from);
                let copied = |slot| {
                    layouts
                        .types()
                        .properties(layout.frame.cell_type(slot).unwrap().ty)
                        .unwrap()
                        .is_copy
                };
                let mut one = |slot, span| -> Result<(), Error> {
                    data.cleanup = budget.vector(1)?;
                    data.cleanup.push((slot, span));
                    data.action = Some(Action::Cleanup);
                    Ok(())
                };
                data.action = match &i.op {
                    b::Op::ArenaEnter { owner, allocator } => {
                        data.arena = Some(ArenaSite {
                            owner: local(*owner)?,
                            allocator: value(*allocator)?,
                            span: i.span,
                        });
                        Some(Action::ArenaEnter)
                    }
                    b::Op::ArenaExit { owner } => {
                        one(local(*owner)?, i.span)?;
                        Some(Action::Cleanup)
                    }
                    b::Op::Borrow { loan, kind, place } => {
                        data.borrow = Some(BorrowSite {
                            loan: *loan,
                            kind: *kind,
                            place: prepare_place_site(&layout, *place, &mut budget)?,
                            result: value(i.results[0])?,
                            span: i.span,
                        });
                        Some(Action::Borrow)
                    }
                    b::Op::BorrowCaseField {
                        loan,
                        value: from,
                        case,
                        field,
                        guard,
                    } => {
                        let result = value(i.results[0])?;
                        let o::Type::Constructed(id) = layout.frame.cell_type(result).unwrap().ty
                        else {
                            return Err(unsupported(i.span));
                        };
                        let o::Constructor::Loan(kind, _) =
                            layouts.types().table().constructed[id.0 as usize].constructor
                        else {
                            return Err(unsupported(i.span));
                        };
                        data.borrow_case = Some(BorrowCaseSite {
                            loan: *loan,
                            kind,
                            field: CaseFieldSite {
                                source: value(*from)?,
                                result,
                                case: *case,
                                field: *field,
                                guard: value(*guard)?,
                            },
                            span: i.span,
                        });
                        Some(Action::BorrowCase)
                    }
                    b::Op::EndBorrow { loan, mode } => Some(Action::EndBorrow(*loan, *mode)),
                    b::Op::PreparePlace { destination, place } => {
                        data.prepared = Some(PreparedSite {
                            destination: *destination,
                            place: prepare_place_site(&layout, *place, &mut budget)?,
                        });
                        Some(Action::Prepare)
                    }
                    b::Op::ReplacePrepared {
                        destination,
                        value: from,
                    } => {
                        data.replace = Some(ReplaceSite {
                            destination: ReplacementDestination::Prepared(*destination),
                            value: value(*from)?,
                            span: typed(flow).places().functions()[function.0 as usize].values
                                [from.0 as usize]
                                .span,
                        });
                        Some(Action::Replace)
                    }
                    b::Op::Replace { place, value: from } => {
                        data.replace = Some(ReplaceSite {
                            destination: ReplacementDestination::Place(prepare_place_site(
                                &layout,
                                *place,
                                &mut budget,
                            )?),
                            value: value(*from)?,
                            span: typed(flow).places().functions()[function.0 as usize].values
                                [from.0 as usize]
                                .span,
                        });
                        Some(Action::Replace)
                    }
                    b::Op::DiscardPrepared(destination) => {
                        Some(Action::DiscardPrepared(*destination))
                    }
                    b::Op::I64(bits) => Some(Action::Constant(value(i.results[0])?, *bits)),
                    b::Op::Bool(bits) => {
                        Some(Action::Constant(value(i.results[0])?, i64::from(*bits)))
                    }
                    b::Op::Unit => Some(Action::Constant(value(i.results[0])?, 0)),
                    b::Op::Add(a, b) | b::Op::LessThan(a, b) | b::Op::EqualI64(a, b) => {
                        let op = match i.op {
                            b::Op::Add(..) => Binary::Add,
                            b::Op::LessThan(..) => Binary::Less,
                            _ => Binary::Equal,
                        };
                        Some(Action::Binary(
                            op,
                            value(*a)?,
                            value(*b)?,
                            value(i.results[0])?,
                        ))
                    }
                    b::Op::Copy(from) => Some(Action::Copy(value(*from)?, value(i.results[0])?)),
                    b::Op::Variant { case, payload } => {
                        let mut fields = budget.vector(payload.len())?;
                        let mut order = budget.vector(payload.len())?;
                        let values = &typed(flow).places().functions()[function.0 as usize].values;
                        for (field, v) in payload.iter().enumerate() {
                            budget.charge(1)?;
                            fields.push(value(*v)?);
                            order.push((field as u32, values[v.0 as usize].span));
                        }
                        data.variant = Some(VariantSite {
                            case: *case,
                            payload: RecordSite {
                                result: value(i.results[0])?,
                                fields,
                                initialization_order: order,
                                span: i.span,
                            },
                        });
                        Some(Action::Variant)
                    }
                    b::Op::IsCase { value: from, case } => {
                        Some(Action::IsCase(value(*from)?, *case, value(i.results[0])?))
                    }
                    b::Op::CopyCaseField {
                        value: from,
                        case,
                        field,
                        guard,
                    } => {
                        data.case_field = Some(CaseFieldSite {
                            source: value(*from)?,
                            result: value(i.results[0])?,
                            case: *case,
                            field: *field,
                            guard: value(*guard)?,
                        });
                        Some(Action::CaseField)
                    }
                    b::Op::UnpackVariant {
                        value: from,
                        case,
                        guard,
                    } if i.results.len() == 1
                        && matches!(typed(flow).places().functions()[function.0 as usize].values[from.0 as usize].ty,
                        o::Type::Constructed(id) if matches!(layouts.types().table().constructed[id.0 as usize].constructor, o::Constructor::Option(_) | o::Constructor::Result(..))) =>
                    {
                        data.extract = Some(ExtractSite {
                            source: value(*from)?,
                            result: value(i.results[0])?,
                            case: *case,
                            guard: Some(value(*guard)?),
                            panic_code: None,
                        });
                        Some(Action::Extract)
                    }
                    b::Op::CoreCall(call) if call.operation.extraction().is_some() => {
                        let contract = call.operation.extraction().unwrap();
                        data.extract = Some(ExtractSite {
                            source: value(call.arguments[0])?,
                            result: value(i.results[0])?,
                            case: contract.case,
                            guard: None,
                            panic_code: Some(contract.panic_code),
                        });
                        Some(Action::Extract)
                    }
                    b::Op::CoreCall(call)
                        if matches!(
                            call.operation,
                            o::core::Operation::Allocator
                                | o::core::Operation::BytesNew
                                | o::core::Operation::BytesClone
                                | o::core::Operation::BytesLen
                                | o::core::Operation::BytesGet
                                | o::core::Operation::BytesSet
                                | o::core::Operation::BytesPush
                                | o::core::Operation::TextFromUtf8
                                | o::core::Operation::TextClone
                                | o::core::Operation::TextLen
                                | o::core::Operation::TextBytes
                                | o::core::Operation::TextEqual
                                | o::core::Operation::ArenaNew
                                | o::core::Operation::ArenaRead
                                | o::core::Operation::ArenaReplace
                                | o::core::Operation::SharedNew
                                | o::core::Operation::SharedClone
                                | o::core::Operation::SharedRead
                                | o::core::Operation::SharedReplace
                                | o::core::Operation::WeakNew
                                | o::core::Operation::WeakClone
                                | o::core::Operation::WeakUpgrade
                                | o::core::Operation::MapNew
                                | o::core::Operation::MapLen
                                | o::core::Operation::MapInsert
                                | o::core::Operation::MapGet
                                | o::core::Operation::MapRemove
                                | o::core::Operation::VecNew
                                | o::core::Operation::VecLen
                                | o::core::Operation::VecPush
                                | o::core::Operation::VecPop
                                | o::core::Operation::VecGet
                                | o::core::Operation::VecGetMut
                                | o::core::Operation::TextConcat
                        ) =>
                    {
                        let mut arguments = budget.vector(call.arguments.len())?;
                        for arg in &call.arguments {
                            budget.charge(1)?;
                            arguments.push(value(*arg)?);
                        }
                        let mut type_arguments = budget.vector(call.type_arguments.len())?;
                        for ty in &call.type_arguments {
                            budget.charge(1)?;
                            type_arguments.push(ty.ty);
                        }
                        data.core = Some(CoreSite {
                            operation: call.operation,
                            type_arguments,
                            arguments,
                            result: value(i.results[0])?,
                            loan: call.result_loan,
                            span: i.span,
                        });
                        Some(Action::Core)
                    }
                    b::Op::Record {
                        fields,
                        initialization_order,
                        ..
                    } => {
                        let mut operands = budget.vector(fields.len())?;
                        for field in fields {
                            budget.charge(1)?;
                            operands.push(value(*field)?);
                        }
                        let mut order = budget.vector(initialization_order.len())?;
                        let values = &typed(flow).places().functions()[function.0 as usize].values;
                        for &field in initialization_order {
                            budget.charge(1)?;
                            order.push((field, values[fields[field as usize].0 as usize].span));
                        }
                        data.record = Some(RecordSite {
                            result: value(i.results[0])?,
                            fields: operands,
                            initialization_order: order,
                            span: i.span,
                        });
                        Some(Action::Record)
                    }
                    b::Op::Move(from) => {
                        let from = value(*from)?;
                        Some(if copied(from) {
                            Action::Copy(from, value(i.results[0])?)
                        } else {
                            Action::Move(from, value(i.results[0])?)
                        })
                    }
                    b::Op::CopyPlace(place) => {
                        data.place = Some(PlaceLoad {
                            place: prepare_place_site(&layout, *place, &mut budget)?,
                            result: value(i.results[0])?,
                        });
                        Some(Action::PlaceLoad)
                    }
                    b::Op::MovePlace(place) => {
                        let place = &typed(flow).places().functions()[function.0 as usize].places
                            [place.0 as usize];
                        if !place.projections.is_empty() {
                            return Err(unsupported(i.span));
                        }
                        let from = match place.root {
                            m::ownership::places::Root::Local(l) => local(l)?,
                            m::ownership::places::Root::Value(v) => value(v)?,
                        };
                        Some(if copied(from) {
                            Action::Copy(from, value(i.results[0])?)
                        } else {
                            Action::Move(from, value(i.results[0])?)
                        })
                    }
                    b::Op::Initialize {
                        local: to,
                        value: from,
                    } => {
                        let from = value(*from)?;
                        Some(if copied(from) {
                            Action::Copy(from, local(*to)?)
                        } else {
                            Action::Move(from, local(*to)?)
                        })
                    }
                    b::Op::Call {
                        function: callee,
                        arguments,
                    } if matches!(scope, CompileScope::Unit) => {
                        let f::Kind::Execute { next, failure, .. } = node.kind else {
                            unreachable!()
                        };
                        let mut cells = budget.vector(arguments.len())?;
                        for argument in arguments {
                            budget.charge(1)?;
                            cells.push(value(*argument)?);
                        }
                        data.call = Some(CallSite {
                            function: *callee,
                            arguments: cells,
                            result: value(i.results[0])?,
                            resume: next.0,
                            failure: failure.0,
                            span: i.span,
                        });
                        continuations.extend([next.0, failure.0]);
                        Some(Action::Call)
                    }
                    b::Op::DropLocalIfInitialized(l) | b::Op::DropLocal(l) => {
                        one(local(*l)?, i.span)?;
                        data.action
                    }
                    b::Op::DropValueIfAvailable(v) | b::Op::DropValue(v) => {
                        one(value(*v)?, i.span)?;
                        data.action
                    }
                    b::Op::Cleanup(range) => {
                        data.cleanup = budget.vector(range.steps.len())?;
                        for step in &range.steps {
                            budget.charge(1)?;
                            let slot = match step.target {
                                b::CleanupTarget::Local(l) | b::CleanupTarget::Arena(l) => {
                                    local(l)?
                                }
                                b::CleanupTarget::Value(v) => value(v)?,
                                b::CleanupTarget::Loan(loan) => layout
                                    .loan_slot(loan)
                                    .ok_or_else(|| unsupported(step.span))?,
                            };
                            data.cleanup.push((slot, step.span));
                        }
                        Some(Action::Cleanup)
                    }
                    _ => return Err(unsupported(i.span)),
                };
            }
            f::Kind::Transfer { block, arm, .. } => {
                let target = body.blocks[block.0 as usize]
                    .terminator
                    .kind
                    .targets()
                    .nth(arm as usize)
                    .ok_or_else(|| unsupported(node.span))?;
                let parameters = &body.blocks[target.block.0 as usize].parameters;
                let mut moves = budget.vector(parameters.len())?;
                for (argument, parameter) in target.arguments.iter().zip(parameters) {
                    budget.charge(1)?;
                    moves.push(TransferMove {
                        source: layout.frame.value(*argument)?,
                        scratch: layout
                            .frame
                            .scratch_cell(*parameter)
                            .ok_or_else(|| unsupported(node.span))?,
                        target: layout.frame.value(*parameter)?,
                    });
                }
                data.transfer = Some(TransferSite { moves });
            }
            f::Kind::Raise { failure, .. } => data.raised = Some(failure),
            _ => {}
        }
        nodes.push(data);
    }
    continuations.sort_unstable();
    continuations.dedup();
    Ok(Prepared {
        layout,
        nodes,
        continuations,
        budget,
    })
}

fn prepare_place_site(
    layout: &FunctionLayout<'_>,
    place: o::places::PlaceId,
    budget: &mut m::Budget,
) -> Result<PlaceSite, Error> {
    let p = &typed(layout.source).places().functions()[layout.function.0 as usize].places
        [place.0 as usize];
    let root = match p.root {
        o::places::Root::Local(id) => layout.frame.local(id)?,
        o::places::Root::Value(id) => layout.frame.value(id)?,
    };
    let mut projections = budget.vector(p.projections.len())?;
    for projection in &p.projections {
        budget.charge(1)?;
        projections.push(*projection);
    }
    Ok(PlaceSite {
        root,
        projections,
        span: p.span,
    })
}

fn backend(e: impl Display) -> Error {
    Error::Backend(e.to_string())
}

#[repr(C)]
struct DispatchState {
    next: Output,
    frame: u64,
    scalar: i64,
}

#[repr(C)]
struct Invocation<'c, 'a, R> {
    // The non-generic prefix is read by native code only between callbacks.
    dispatch: DispatchState,
    compiled: &'c CompiledFunction<'a>,
    program: Option<&'c CompiledProgram<'a>>,
    instance: &'c mut RuntimeInstance<R>,
    fault: Option<Failure>,
    report_recorded: bool,
}
impl<R: Runtime> Invocation<'_, '_, R> {
    fn note(&mut self, cause: c::Cause, span: m::Span) {
        if matches!(
            cause,
            c::Cause::Execution(c::ExecutionFailure::HostPanic | c::ExecutionFailure::Invariant)
        ) {
            self.instance.poisoned = true;
        }
        self.fault.get_or_insert(Failure {
            function: self.compiled.layout.function,
            reason: Reason::Runtime(cause),
            span,
            panic_code: None,
        });
    }
    fn apply(
        &mut self,
        node: u64,
        action: impl FnOnce(&mut R, &CompiledFunction<'_>, m::Span) -> ResultStep,
    ) -> i32 {
        if self.evaluate(node, action).is_ok() {
            0
        } else {
            2
        }
    }
    fn evaluate<T>(
        &mut self,
        node: u64,
        action: impl FnOnce(&mut R, &CompiledFunction<'_>, m::Span) -> Result<T, c::Cause>,
    ) -> Result<T, ()> {
        let Some(info) = usize::try_from(node)
            .ok()
            .and_then(|index| self.compiled.nodes.get(index))
        else {
            self.note(
                c::Cause::Execution(c::ExecutionFailure::Invariant),
                self.compiled.nodes[0].span,
            );
            return Err(());
        };
        let span = info.span;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            action(&mut self.instance.runtime, self.compiled, span)
        }));
        let cause = match outcome {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(cause)) => cause,
            Err(payload) => {
                // A panic payload can itself panic in Drop. Contain disposal
                // before returning through the extern-C shim; never retry it.
                if let Err(secondary) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
                    std::mem::forget(secondary);
                    self.instance.quarantined_panics =
                        self.instance.quarantined_panics.saturating_add(1);
                }
                c::Cause::Execution(c::ExecutionFailure::HostPanic)
            }
        };
        self.note(cause, span);
        Err(())
    }
    fn transition(
        &mut self,
        node: u64,
        action: impl FnOnce(
            &mut R,
            &CompiledFunction<'_>,
            m::Span,
        ) -> Result<Option<Transition>, c::Cause>,
    ) -> i32 {
        let program = self.program;
        let failed = self.fault.is_some();
        match self.evaluate(node, |r, p, span| {
            let target = action(r, p, span)?;
            if let Some(target) = target
                && (!valid_target(program, target, failed)
                    || !r.validate_transition(target)
                    || r.frame_identity() != target.frame)
            {
                return Err(c::Cause::Execution(c::ExecutionFailure::Invariant));
            }
            Ok(target)
        }) {
            Ok(Some(target)) => {
                self.dispatch.next = Output {
                    frame: target.frame,
                    function: target.function.0,
                    continuation: target.continuation,
                };
                1
            }
            Ok(None) => 0,
            Err(()) => 2,
        }
    }
}

macro_rules! helper {
    ($name:ident, $ctx:ident, $a:ident, $b:ident, $node:ident, $body:block) => {
        unsafe extern "C" fn $name<R: Runtime>(
            raw: *mut c_void,
            $a: u64,
            $b: u64,
            $node: u64,
        ) -> i32 {
            // SAFETY: only invoke-created headers supply this callback, while
            // its exclusive Invocation<R> borrow and executable remain live.
            let $ctx = unsafe { &mut *raw.cast::<Invocation<'_, '_, R>>() };
            $body
        }
    };
}
helper!(entry_helper, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| r.enter(&p.layout))
});
helper!(charge, ctx, _a, _b, node, {
    ctx.apply(node, |r, _, s| r.charge(s))
});
helper!(move_slot, ctx, a, b, node, {
    ctx.apply(node, |r, _, s| r.move_slot(a as usize, b as usize, s))
});
helper!(construct_record, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        let record = p.nodes[node as usize]
            .record
            .as_ref()
            .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?;
        r.construct_record(record)
    })
});
helper!(enter_arena, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.enter_arena(
            p.nodes[node as usize]
                .arena
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(read_scalar, ctx, a, _b, node, {
    match ctx.evaluate(node, |r, _, _| r.read_scalar(a as usize)) {
        Ok(value) => {
            ctx.dispatch.scalar = value;
            0
        }
        Err(()) => 2,
    }
});
helper!(write_scalar, ctx, a, b, node, {
    ctx.apply(node, |r, _, span| {
        r.write_scalar(a as usize, b as i64, span)
    })
});
helper!(copy_slot, ctx, a, b, node, {
    ctx.apply(node, |r, _, span| r.copy_slot(a as usize, b as usize, span))
});
helper!(variant_case, ctx, a, _b, node, {
    match ctx.evaluate(node, |r, _, _| r.variant_case(a as usize)) {
        Ok(case) => {
            ctx.dispatch.scalar = i64::from(case);
            0
        }
        Err(()) => 2,
    }
});
helper!(construct_variant, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.construct_variant(
            p.nodes[node as usize]
                .variant
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(extract_variant, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.extract_variant(
            p.nodes[node as usize]
                .extract
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(copy_case_field, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.copy_case_field(
            p.nodes[node as usize]
                .case_field
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(transfer, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.transfer(
            p.nodes[node as usize]
                .transfer
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(borrow_place, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.borrow_place(
            p.nodes[node as usize]
                .borrow
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(load_place, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.load_place(
            p.nodes[node as usize]
                .place
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(end_borrow, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        let Some(Action::EndBorrow(loan, mode)) = p.nodes[node as usize].action else {
            return missing();
        };
        r.end_borrow(loan, mode)
    })
});
helper!(prepare_place, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.prepare_place(
            p.nodes[node as usize]
                .prepared
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(replace_place, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.replace_place(
            p.nodes[node as usize]
                .replace
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(discard_prepared, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        let destination = match p.nodes[node as usize].action {
            Some(Action::DiscardPrepared(id)) => Some(id),
            _ => None,
        };
        r.discard_prepared(destination);
        Ok(())
    })
});
helper!(borrow_case, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.borrow_case(
            p.nodes[node as usize]
                .borrow_case
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(core_call, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| {
        r.core_call(
            p.nodes[node as usize]
                .core
                .as_ref()
                .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?,
        )
    })
});
helper!(panic, ctx, _a, _b, node, {
    let info = &ctx.compiled.nodes[node as usize];
    let code = info.extract.as_ref().and_then(|e| e.panic_code);
    let first = ctx.fault.is_none();
    ctx.note(
        c::Cause::Execution(if code.is_some() {
            c::ExecutionFailure::Panic
        } else {
            c::ExecutionFailure::Invariant
        }),
        info.span,
    );
    if first {
        ctx.fault.as_mut().unwrap().panic_code = code;
    }
    2
});
helper!(cleanup, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| r.cleanup(&p.nodes[node as usize].cleanup))
});
helper!(stage, ctx, a, _b, node, {
    ctx.apply(node, |r, _, s| r.stage_return(a as usize, s))
});
helper!(retire, ctx, _a, _b, node, {
    ctx.apply(node, |r, _, _| r.retire_return())
});
helper!(publish, ctx, _a, _b, node, {
    ctx.apply(node, |r, _, _| r.publish_return())
});
helper!(record, ctx, _a, _b, node, {
    let cause = if ctx.report_recorded {
        c::Cause::Reported
    } else {
        ctx.fault
            .and_then(|f| match f.reason {
                Reason::Runtime(cause) => Some(cause),
                Reason::Poisoned
                | Reason::UnsupportedControl(_)
                | Reason::UnsupportedServiceWork(_) => None,
            })
            .unwrap_or(c::Cause::Execution(c::ExecutionFailure::Invariant))
    };
    let fault_span = ctx.fault.map(|fault| fault.span);
    let status = ctx.apply(node, |r, _, span| {
        r.record_failure(cause, fault_span.unwrap_or(span))
    });
    ctx.report_recorded = true;
    status
});
helper!(retire_failure, ctx, _a, _b, node, {
    ctx.apply(node, |r, _, _| r.retire_failure())
});
helper!(discard, ctx, _a, _b, node, {
    ctx.apply(node, |r, _, _| r.discard_result())
});
helper!(raise, ctx, _a, _b, node, {
    let index = node as usize;
    let info = &ctx.compiled.nodes[index];
    ctx.note(
        c::Cause::Execution(info.raised.unwrap_or(c::ExecutionFailure::Invariant)),
        info.span,
    );
    0
});
helper!(prepare_call, ctx, _a, _b, node, {
    let program = ctx.program;
    ctx.transition(node, |r, p, _| {
        let call = p.nodes[node as usize]
            .call
            .as_ref()
            .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?;
        let target = program
            .and_then(|p| p.compiled(call.function))
            .ok_or(c::Cause::Execution(c::ExecutionFailure::Invariant))?;
        r.prepare_call(call, &target.layout).map(Some)
    })
});
helper!(after_return, ctx, _a, _b, node, {
    let status = ctx.apply(node, |r, _, _| r.publish_return());
    if status != 0 {
        return status;
    }
    ctx.transition(node, |r, _, _| r.after_return())
});
helper!(after_failure, ctx, _a, _b, node, {
    ctx.transition(node, |r, _, _| r.after_failure())
});

fn emit<M: Module>(
    module: &mut M,
    graph: &f::Function,
    layout: &FunctionLayout<'_>,
    nodes: &[Node],
    continuations: &[u32],
    budget: &mut m::Budget,
) -> Result<(cranelift_module::FuncId, super::program::FunctionCode), Error> {
    let pointer = module.target_config().pointer_type();
    if pointer != types::I64 {
        return Err(Error::Backend(
            "ownership private revision 1 requires 64-bit pointers".into(),
        ));
    }
    let mut signature = module.make_signature();
    signature.params.extend([AbiParam::new(pointer); 2]);
    signature.returns.push(AbiParam::new(types::I32));
    let mut helper_signature = module.make_signature();
    helper_signature
        .params
        .extend([AbiParam::new(types::I64); 4]);
    helper_signature.returns.push(AbiParam::new(types::I32));
    let id = module
        .declare_function(
            &format!("lang_owned_r1_f{}", layout.function.0),
            Linkage::Export,
            &signature,
        )
        .map_err(backend)?;
    let mut context = module.make_context();
    context.func.signature = signature;
    let mut builder_context = FunctionBuilderContext::new();
    {
        use cranelift_codegen::ir::condcodes::IntCC;
        let mut b = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let sig = b.func.import_signature(helper_signature);
        let guard = b.create_block();
        let invalid = b.create_block();
        let mut blocks = budget.vector(nodes.len())?;
        for _ in nodes {
            blocks.push(b.create_block());
        }
        b.append_block_params_for_function_params(guard);
        b.switch_to_block(guard);
        let input = b.block_params(guard)[0];
        let output = b.block_params(guard)[1];
        let frame = b.ins().load(
            types::I64,
            MemFlagsData::new(),
            input,
            offset_of!(Input, frame) as i32,
        );
        let pc = b.ins().load(
            types::I32,
            MemFlagsData::new(),
            input,
            offset_of!(Input, continuation) as i32,
        );
        let reserved = b.ins().load(
            types::I32,
            MemFlagsData::new(),
            input,
            offset_of!(Input, reserved) as i32,
        );
        let header = b.ins().load(pointer, MemFlagsData::new(), input, 0);
        let data = b.ins().load(
            pointer,
            MemFlagsData::new(),
            header,
            offset_of!(Header, data) as i32,
        );
        let expected = b.ins().load(
            types::I64,
            MemFlagsData::new(),
            data,
            offset_of!(DispatchState, frame) as i32,
        );
        let frame_valid = b.ins().icmp(IntCC::Equal, frame, expected);
        let reserved_valid = b.ins().icmp_imm_u(IntCC::Equal, reserved, 0);
        let valid = b.ins().band(frame_valid, reserved_valid);
        let select = b.create_block();
        b.ins().brif(valid, select, &[], invalid, &[]);
        b.switch_to_block(select);
        for (index, continuation) in continuations.iter().enumerate() {
            budget.charge(1)?;
            let fallback = if index + 1 == continuations.len() {
                invalid
            } else {
                b.create_block()
            };
            let selected = b
                .ins()
                .icmp_imm_u(IntCC::Equal, pc, i64::from(*continuation));
            b.ins()
                .brif(selected, blocks[*continuation as usize], &[], fallback, &[]);
            if fallback != invalid {
                b.switch_to_block(fallback);
            }
        }
        for (index, node) in graph.nodes().iter().enumerate() {
            budget.charge(1)?;
            b.switch_to_block(blocks[index]);
            b.set_srcloc(SourceLoc::new(u32::try_from(index).map_err(backend)?));
            macro_rules! call {
                ($which:expr, $a:expr, $c:expr) => {
                    native_helper_imm(&mut b, sig, input, index, $which, $a, $c)
                };
            }
            let conditional = match node.kind {
                f::Kind::Entry { next, failure } => Some((call!(0, 0, 0), next, failure)),
                f::Kind::Fuel { next, failure, .. } => Some((call!(1, 0, 0), next, failure)),
                f::Kind::Execute { next, failure, .. } => {
                    match nodes[index].action.expect("preflighted instruction") {
                        Action::Move(from, to) => Some((call!(2, from, to), next, failure)),
                        Action::Cleanup => Some((call!(3, 0, 0), next, failure)),
                        Action::Record => Some((call!(14, 0, 0), next, failure)),
                        Action::Constant(to, bits) => {
                            let to = b.ins().iconst(types::I64, to as i64);
                            let bits = b.ins().iconst(types::I64, bits);
                            Some((
                                native_helper(&mut b, sig, input, index, 16, to, bits),
                                next,
                                failure,
                            ))
                        }
                        Action::Copy(from, to) => Some((call!(17, from, to), next, failure)),
                        Action::Variant => Some((call!(19, 0, 0), next, failure)),
                        Action::PlaceLoad => Some((call!(25, 0, 0), next, failure)),
                        Action::Core => Some((call!(31, 0, 0), next, failure)),
                        Action::ArenaEnter => Some((call!(32, 0, 0), next, failure)),
                        Action::Borrow => Some((call!(24, 0, 0), next, failure)),
                        Action::EndBorrow(..) => Some((call!(26, 0, 0), next, failure)),
                        Action::Prepare => Some((call!(27, 0, 0), next, failure)),
                        Action::Replace => Some((call!(28, 0, 0), next, failure)),
                        Action::DiscardPrepared(_) => Some((call!(29, 0, 0), next, failure)),
                        Action::Binary(op, left, right, to) => {
                            let failed = blocks[failure.0 as usize];
                            let left = native_read(&mut b, sig, input, index, 15, left, failed);
                            let right = native_read(&mut b, sig, input, index, 15, right, failed);
                            let result = match op {
                                Binary::Add => b.ins().iadd(left, right),
                                Binary::Less | Binary::Equal => {
                                    let predicate = if matches!(op, Binary::Less) {
                                        IntCC::SignedLessThan
                                    } else {
                                        IntCC::Equal
                                    };
                                    let bit = b.ins().icmp(predicate, left, right);
                                    b.ins().uextend(types::I64, bit)
                                }
                            };
                            let to = b.ins().iconst(types::I64, to as i64);
                            Some((
                                native_helper(&mut b, sig, input, index, 16, to, result),
                                next,
                                failure,
                            ))
                        }
                        Action::IsCase(from, case, to) => {
                            let actual = native_read(
                                &mut b,
                                sig,
                                input,
                                index,
                                18,
                                from,
                                blocks[failure.0 as usize],
                            );
                            let bit = b.ins().icmp_imm_u(IntCC::Equal, actual, i64::from(case));
                            let result = b.ins().uextend(types::I64, bit);
                            let to = b.ins().iconst(types::I64, to as i64);
                            Some((
                                native_helper(&mut b, sig, input, index, 16, to, result),
                                next,
                                failure,
                            ))
                        }
                        Action::Extract | Action::CaseField | Action::BorrowCase => {
                            let (source, case, guard, helper) =
                                if let Some(extract) = &nodes[index].extract {
                                    (extract.source, extract.case, extract.guard, 20)
                                } else if let Some(borrow) = &nodes[index].borrow_case {
                                    let field = &borrow.field;
                                    (field.source, field.case, Some(field.guard), 30)
                                } else {
                                    let field = nodes[index].case_field.as_ref().unwrap();
                                    (field.source, field.case, Some(field.guard), 23)
                                };
                            let failed = blocks[failure.0 as usize];
                            let actual = native_read(&mut b, sig, input, index, 18, source, failed);
                            let mut valid =
                                b.ins().icmp_imm_u(IntCC::Equal, actual, i64::from(case));
                            if let Some(guard) = guard {
                                let guard =
                                    native_read(&mut b, sig, input, index, 15, guard, failed);
                                let guarded = b.ins().icmp_imm_u(IntCC::Equal, guard, 1);
                                valid = b.ins().band(valid, guarded);
                            }
                            let success = b.create_block();
                            let mismatch = b.create_block();
                            b.ins().brif(valid, success, &[], mismatch, &[]);
                            b.switch_to_block(mismatch);
                            call!(22, 0, 0);
                            b.ins().jump(failed, &[]);
                            b.switch_to_block(success);
                            Some((call!(helper, 0, 0), next, failure))
                        }
                        Action::Call => {
                            let status = call!(11, 0, 0);
                            let dispatch = b.create_block();
                            let valid = b.ins().icmp_imm_u(IntCC::Equal, status, 1);
                            b.ins()
                                .brif(valid, dispatch, &[], blocks[failure.0 as usize], &[]);
                            b.switch_to_block(dispatch);
                            emit_packet(&mut b, input, output);
                            None
                        }
                    }
                }
                f::Kind::StageReturn {
                    value,
                    next,
                    failure,
                } => Some((call!(4, layout.frame.value(value)?, 0), next, failure)),
                f::Kind::Transfer {
                    target, failure, ..
                } => Some((call!(21, 0, 0), target, failure)),
                f::Kind::Branch {
                    condition,
                    then_node,
                    else_node,
                    failure,
                } => {
                    let condition = native_read(
                        &mut b,
                        sig,
                        input,
                        index,
                        15,
                        layout.frame.value(condition)?,
                        blocks[failure.0 as usize],
                    );
                    let bit = b.ins().icmp_imm_u(IntCC::Equal, condition, 1);
                    b.ins().brif(
                        bit,
                        blocks[then_node.0 as usize],
                        &[],
                        blocks[else_node.0 as usize],
                        &[],
                    );
                    None
                }
                f::Kind::RetireReturn { next, failure } => Some((call!(5, 0, 0), next, failure)),
                f::Kind::PublishReturn => {
                    let status = call!(12, 0, 0);
                    let success = b.create_block();
                    let maybe_dispatch = b.create_block();
                    let dispatch = b.create_block();
                    let valid = b.ins().icmp_imm_u(IntCC::Equal, status, 0);
                    b.ins().brif(valid, success, &[], maybe_dispatch, &[]);
                    b.switch_to_block(maybe_dispatch);
                    let valid = b.ins().icmp_imm_u(IntCC::Equal, status, 1);
                    b.ins().brif(
                        valid,
                        dispatch,
                        &[],
                        blocks[graph.failure().0 as usize],
                        &[],
                    );
                    b.switch_to_block(dispatch);
                    emit_packet(&mut b, input, output);
                    b.switch_to_block(success);
                    let status = b.ins().iconst(types::I32, 0);
                    b.ins().return_(&[status]);
                    None
                }
                f::Kind::DiscardPrepared { next } => {
                    call!(29, 0, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::Raise { next, .. } => {
                    call!(10, 0, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::RecordFailure { next } => {
                    call!(7, 0, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::RetireFailure { next } => {
                    call!(8, 0, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::DiscardResult { next } => {
                    call!(9, 0, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::PropagateFailure => {
                    let status = call!(13, 0, 0);
                    let dispatch = b.create_block();
                    let failed = b.create_block();
                    let valid = b.ins().icmp_imm_u(IntCC::Equal, status, 1);
                    b.ins().brif(valid, dispatch, &[], failed, &[]);
                    b.switch_to_block(dispatch);
                    emit_packet(&mut b, input, output);
                    b.switch_to_block(failed);
                    let status = b.ins().iconst(types::I32, 2);
                    b.ins().return_(&[status]);
                    None
                }
            };
            if let Some((status, next, failure)) = conditional {
                let success = b.ins().icmp_imm_u(IntCC::Equal, status, 0);
                b.ins().brif(
                    success,
                    blocks[next.0 as usize],
                    &[],
                    blocks[failure.0 as usize],
                    &[],
                );
            }
        }
        b.switch_to_block(invalid);
        b.set_srcloc(SourceLoc::default());
        let status = b.ins().iconst(types::I32, 2);
        b.ins().return_(&[status]);
        b.seal_all_blocks();
        b.finalize(module.target_config());
    }
    module.define_function(id, &mut context).map_err(backend)?;
    let code = context.compiled_code().expect("defined code");
    let mut ranges = budget.vector(code.buffer.get_srclocs_sorted().len())?;
    for range in code.buffer.get_srclocs_sorted() {
        if !range.loc.is_default() {
            ranges.push(super::program::SourceRange {
                start: range.start,
                end: range.end,
                span: nodes[range.loc.bits() as usize].span,
            });
        }
    }
    Ok((
        id,
        super::program::FunctionCode {
            function: layout.function,
            symbol: format!("lang_owned_r1_f{}", layout.function.0),
            code_size: code.code_buffer().len(),
            source_ranges: ranges,
        },
    ))
}

fn native_helper_imm(
    b: &mut FunctionBuilder<'_>,
    sig: SigRef,
    input: Value,
    node: usize,
    which: usize,
    a: usize,
    c: usize,
) -> Value {
    let a = b.ins().iconst(types::I64, a as i64);
    let c = b.ins().iconst(types::I64, c as i64);
    native_helper(b, sig, input, node, which, a, c)
}
fn native_helper(
    b: &mut FunctionBuilder<'_>,
    sig: SigRef,
    input: Value,
    node: usize,
    which: usize,
    a: Value,
    c: Value,
) -> Value {
    let header = b.ins().load(types::I64, MemFlagsData::new(), input, 0);
    let data = b.ins().load(
        types::I64,
        MemFlagsData::new(),
        header,
        offset_of!(Header, data) as i32,
    );
    let address = b.ins().load(
        types::I64,
        MemFlagsData::new(),
        header,
        (offset_of!(Header, helpers) + which * 8) as i32,
    );
    let n = b.ins().iconst(types::I64, node as i64);
    let call = b.ins().call_indirect(sig, address, &[data, a, c, n]);
    b.inst_results(call)[0]
}
fn native_read(
    b: &mut FunctionBuilder<'_>,
    sig: SigRef,
    input: Value,
    node: usize,
    which: usize,
    slot: usize,
    failure: Block,
) -> Value {
    use cranelift_codegen::ir::condcodes::IntCC;
    let status = native_helper_imm(b, sig, input, node, which, slot, 0);
    let valid = b.ins().icmp_imm_u(IntCC::Equal, status, 0);
    let success = b.create_block();
    b.ins().brif(valid, success, &[], failure, &[]);
    b.switch_to_block(success);
    let header = b.ins().load(types::I64, MemFlagsData::new(), input, 0);
    let data = b.ins().load(
        types::I64,
        MemFlagsData::new(),
        header,
        offset_of!(Header, data) as i32,
    );
    b.ins().load(
        types::I64,
        MemFlagsData::new(),
        data,
        offset_of!(DispatchState, scalar) as i32,
    )
}

fn emit_packet(
    b: &mut FunctionBuilder<'_>,
    input: cranelift_codegen::ir::Value,
    output: cranelift_codegen::ir::Value,
) {
    let header = b.ins().load(types::I64, MemFlagsData::new(), input, 0);
    let data = b.ins().load(
        types::I64,
        MemFlagsData::new(),
        header,
        offset_of!(Header, data) as i32,
    );
    for (offset, ty) in [
        (offset_of!(Output, frame), types::I64),
        (offset_of!(Output, function), types::I32),
        (offset_of!(Output, continuation), types::I32),
    ] {
        let value = b.ins().load(
            ty,
            MemFlagsData::new(),
            data,
            (offset_of!(DispatchState, next) + offset) as i32,
        );
        b.ins()
            .store(MemFlagsData::new(), value, output, offset as i32);
    }
    let status = b.ins().iconst(types::I32, 1);
    b.ins().return_(&[status]);
}

#[repr(C)]
struct Input {
    context: *mut c_void,
    frame: u64,
    continuation: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Output {
    frame: u64,
    function: u32,
    continuation: u32,
}

#[derive(Debug)]
pub enum Error {
    Layout(l::Error),
    Resource(m::Error),
    InvalidFunction(m::FunctionId),
    Unsupported {
        function: m::FunctionId,
        span: m::Span,
    },
    Backend(String),
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "native ownership compilation failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<l::Error> for Error {
    fn from(e: l::Error) -> Self {
        Self::Layout(e)
    }
}
impl From<m::Error> for Error {
    fn from(e: m::Error) -> Self {
        Self::Resource(e)
    }
}

/// A private whole-unit function layout. Compilation starts with all previous
/// proof/layout usage; the common verified type graph is counted only once.
pub struct FunctionLayout<'a> {
    function: m::FunctionId,
    frame: FramePlan<'a, 'a>,
    source: &'a f::Elaborated,
    layouts: &'a Layouts<'a>,
    slots: usize,
}
impl<'a> FunctionLayout<'a> {
    /// Logical cell admission includes loan registrations and the return escrow,
    /// matching the VM's instrumented profile independently of physical storage.
    pub fn control_cells(&self) -> usize {
        self.frame.cleanup_slots() + 1
    }

    pub fn compile(
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        function: m::FunctionId,
        limits: m::Limits,
        cells: usize,
    ) -> Result<Self, Error> {
        Self::compile_after(flow, layouts, function, limits, cells, None)
    }
    fn compile_after(
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        function: m::FunctionId,
        limits: m::Limits,
        cells: usize,
        prefix: Option<m::Usage>,
    ) -> Result<Self, Error> {
        let typed = flow.source().activity().moved().typed();
        if typed.functions().get(function.0 as usize).is_none() {
            return Err(Error::InvalidFunction(function));
        }
        let base = Self::base_usage(flow, layouts)?;
        let initial = prefix.unwrap_or(base);
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(Error::Layout(l::Error::State));
        }
        let frame = FramePlan::compile_after(layouts, typed, function, limits, cells, initial)?;
        let slots = frame
            .cell_count()
            .checked_add(typed.functions()[function.0 as usize].loans)
            .ok_or(Error::Layout(l::Error::Overflow))?;
        Ok(Self {
            function,
            frame,
            source: flow,
            layouts,
            slots,
        })
    }
    fn base_usage(flow: &f::Elaborated, layouts: &Layouts<'_>) -> Result<m::Usage, Error> {
        let types = typed(flow).places().declarations().types();
        fn combine(a: usize, extra: usize, common: usize) -> Result<usize, Error> {
            extra
                .checked_sub(common)
                .and_then(|delta| a.checked_add(delta))
                .ok_or(Error::Layout(l::Error::Overflow))
        }
        let existing = layouts.usage();
        Ok(m::Usage {
            work: combine(flow.work_used(), existing.work, types.work_used())?,
            allocation_bytes: combine(
                flow.requested_bytes(),
                existing.allocation_bytes,
                types.requested_bytes(),
            )?,
            elements: combine(
                flow.elements_used(),
                existing.elements,
                types.elements_used(),
            )?,
        })
    }
    pub const fn function(&self) -> m::FunctionId {
        self.function
    }
    pub const fn frame(&self) -> &FramePlan<'a, 'a> {
        &self.frame
    }
    /// Typed cells use their FramePlan indices; borrow registrations follow
    /// them in a disjoint logical cleanup-slot range.
    pub const fn slot_count(&self) -> usize {
        self.slots
    }
    pub fn loan_slot(&self, loan: b::LoanId) -> Option<usize> {
        let slot = self.frame.cell_count().checked_add(loan.0 as usize)?;
        (slot < self.slots).then_some(slot)
    }
    pub const fn source(&self) -> &'a f::Elaborated {
        self.source
    }
    pub const fn layouts(&self) -> &'a Layouts<'a> {
        self.layouts
    }
}

fn typed(flow: &f::Elaborated) -> &b::TypedBodies {
    flow.source().activity().moved().typed()
}
