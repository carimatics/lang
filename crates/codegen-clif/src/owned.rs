//! RFC 0012 ownership native call boundary. Managed payloads stay in the shared
//! runtime layouts; packets contain only an invocation context and frame IDs.
use lang_mir::unit::{
    self as m,
    ownership::{bodies as b, failure as f},
};
use lang_runtime::layout::{self as l, FramePlan, Layouts};
use std::{
    ffi::c_void,
    fmt::{Display, Formatter},
};

use super::OwnedJitModule;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, SourceLoc, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use lang_runtime::cleanup as c;
use std::{
    mem::offset_of,
    panic::{AssertUnwindSafe, catch_unwind},
};

pub type ResultStep = Result<(), c::Cause>;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    Poisoned,
    Runtime(c::Cause),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Failure {
    pub function: m::FunctionId,
    pub reason: Reason,
    pub span: m::Span,
}

type Helper = unsafe extern "C" fn(*mut c_void, u64, u64, u64) -> i32;
const HELPERS: usize = 11;
#[repr(C)]
struct Header {
    data: *mut c_void,
    helpers: [Helper; HELPERS],
}

#[derive(Clone, Copy)]
enum Action {
    Move(usize, usize),
    Cleanup,
}
struct Node {
    span: m::Span,
    action: Option<Action>,
    cleanup: Vec<(usize, m::Span)>,
    raised: Option<c::ExecutionFailure>,
}

/// Initial production call boundary for straight-line whole-owner forwarding.
/// Other operations are diagnosed before invocation; this is not the complete
/// ownership native execution mode. General moves/loans/core/control retain the
/// M2-N02–N05 implementation and conformance obligations.
pub struct CompiledFunction<'a> {
    _owner: OwnedJitModule,
    code: *const u8,
    layout: FunctionLayout<'a>,
    nodes: Vec<Node>,
    usage: m::Usage,
    metadata: super::program::FunctionCode,
}

impl<'a> CompiledFunction<'a> {
    pub fn compile(
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        function: m::FunctionId,
        limits: m::Limits,
        cells: usize,
    ) -> Result<Self, Error> {
        let layout = FunctionLayout::compile(flow, layouts, function, limits, cells)?;
        let body = &typed(flow).functions()[function.0 as usize];
        let unsupported = |span| Error::Unsupported { function, span };
        let graph = &flow.functions()[function.0 as usize];
        if body.loans != 0 || body.prepared != 0 {
            return Err(unsupported(graph.nodes()[0].span));
        }
        let mut budget = m::Budget::with_usage(limits, layout.frame.usage())?;
        let mut nodes = budget.vector(graph.nodes().len())?;
        for node in graph.nodes() {
            budget.charge(1)?;
            let mut data = Node {
                span: node.span,
                action: None,
                cleanup: Vec::new(),
                raised: None,
            };
            match node.kind {
                f::Kind::Execute { site, .. } => {
                    // Order zero names block parameters; source instructions
                    // use one-based definition order.
                    let i = site
                        .order
                        .checked_sub(1)
                        .and_then(|order| {
                            body.blocks[site.block.0 as usize].instructions.get(order)
                        })
                        .ok_or_else(|| unsupported(node.span))?;
                    let value = |v| layout.frame.value(v).map_err(Error::from);
                    let local = |v| layout.frame.local(v).map_err(Error::from);
                    let mut one = |slot, span| -> Result<(), Error> {
                        data.cleanup = budget.vector(1)?;
                        data.cleanup.push((slot, span));
                        data.action = Some(Action::Cleanup);
                        Ok(())
                    };
                    data.action = match &i.op {
                        b::Op::Move(from) => {
                            Some(Action::Move(value(*from)?, value(i.results[0])?))
                        }
                        b::Op::MovePlace(place) => {
                            let place = &typed(flow).places().functions()[function.0 as usize]
                                .places[place.0 as usize];
                            if !place.projections.is_empty() {
                                return Err(unsupported(i.span));
                            }
                            let from = match place.root {
                                m::ownership::places::Root::Local(l) => local(l)?,
                                m::ownership::places::Root::Value(v) => value(v)?,
                            };
                            Some(Action::Move(from, value(i.results[0])?))
                        }
                        b::Op::Initialize {
                            local: to,
                            value: from,
                        } => Some(Action::Move(value(*from)?, local(*to)?)),
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
                                    b::CleanupTarget::Local(l) => local(l)?,
                                    b::CleanupTarget::Value(v) => value(v)?,
                                    _ => return Err(unsupported(step.span)),
                                };
                                data.cleanup.push((slot, step.span));
                            }
                            Some(Action::Cleanup)
                        }
                        _ => return Err(unsupported(i.span)),
                    };
                }
                f::Kind::Branch { .. } | f::Kind::Transfer { .. } => {
                    return Err(unsupported(node.span));
                }
                f::Kind::Raise { failure, .. } => data.raised = Some(failure),
                _ => {}
            }
            nodes.push(data);
        }
        let mut owner = OwnedJitModule(Some(JITModule::new(
            JITBuilder::new(default_libcall_names()).map_err(backend)?,
        )));
        let module = owner.0.as_mut().expect("new module");
        let (id, metadata) = emit(module, graph, &layout, &nodes, &mut budget)?;
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
        let span = self.nodes[0].span;
        let failure = |reason| Failure {
            function: self.layout.function,
            reason,
            span,
        };
        if instance.is_poisoned() {
            return Err(failure(Reason::Poisoned));
        }
        let mut context = Invocation {
            compiled: self,
            instance,
            fault: None,
        };
        let mut header = Header {
            data: (&mut context as *mut Invocation<'_, '_, R>).cast(),
            helpers: [
                entry::<R>,
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
            ],
        };
        let input = Input {
            context: (&mut header as *mut Header).cast(),
            frame: 0,
            continuation: 0,
            reserved: 0,
        };
        let mut output = Output {
            frame: 0,
            function: 0,
            continuation: 0,
        };
        // SAFETY: emit uses precisely the target-native two-pointer/i32 C ABI.
        // Both packets and the header have the repr(C) extents used by lowering.
        // The opaque pointer borrows this exact live Invocation<R>; callbacks
        // are monomorphized for R and contain all Rust panics. Nothing retains
        // packet/context pointers. The executable owner outlives the call.
        let status = unsafe {
            let call: unsafe extern "C" fn(*const Input, *mut Output) -> i32 =
                std::mem::transmute(self.code);
            call(&input, &mut output)
        };
        match (status, context.fault) {
            (0, None) => Ok(()),
            (_, Some(fault)) => Err(fault),
            _ => {
                context.instance.poisoned = true;
                Err(failure(Reason::Runtime(c::Cause::Execution(
                    c::ExecutionFailure::Invariant,
                ))))
            }
        }
    }
}

fn backend(e: impl Display) -> Error {
    Error::Backend(e.to_string())
}

struct Invocation<'c, 'a, R> {
    compiled: &'c CompiledFunction<'a>,
    instance: &'c mut RuntimeInstance<R>,
    fault: Option<Failure>,
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
        });
    }
    fn apply(
        &mut self,
        node: u64,
        action: impl FnOnce(&mut R, &CompiledFunction<'_>, m::Span) -> ResultStep,
    ) -> i32 {
        let Some(info) = usize::try_from(node)
            .ok()
            .and_then(|index| self.compiled.nodes.get(index))
        else {
            self.note(
                c::Cause::Execution(c::ExecutionFailure::Invariant),
                self.compiled.nodes[0].span,
            );
            return 2;
        };
        let span = info.span;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            action(&mut self.instance.runtime, self.compiled, span)
        }));
        let cause = match outcome {
            Ok(Ok(())) => return 0,
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
        2
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
helper!(entry, ctx, _a, _b, node, {
    ctx.apply(node, |r, p, _| r.enter(&p.layout))
});
helper!(charge, ctx, _a, _b, node, {
    ctx.apply(node, |r, _, s| r.charge(s))
});
helper!(move_slot, ctx, a, b, node, {
    ctx.apply(node, |r, _, s| r.move_slot(a as usize, b as usize, s))
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
    let cause = ctx
        .fault
        .and_then(|f| match f.reason {
            Reason::Runtime(cause) => Some(cause),
            Reason::Poisoned => None,
        })
        .unwrap_or(c::Cause::Execution(c::ExecutionFailure::Invariant));
    ctx.apply(node, |r, _, s| r.record_failure(cause, s))
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

fn emit<M: Module>(
    module: &mut M,
    graph: &f::Function,
    layout: &FunctionLayout<'_>,
    nodes: &[Node],
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
        let bits = b.ins().bor(pc, reserved);
        let bits = b.ins().uextend(types::I64, bits);
        let bits = b.ins().bor(frame, bits);
        let valid = b.ins().icmp_imm_u(IntCC::Equal, bits, 0);
        b.ins()
            .brif(valid, blocks[graph.entry().0 as usize], &[], invalid, &[]);
        for (index, node) in graph.nodes().iter().enumerate() {
            budget.charge(1)?;
            b.switch_to_block(blocks[index]);
            b.set_srcloc(SourceLoc::new(u32::try_from(index).map_err(backend)?));
            let mut call = |which: usize, a: usize, c: usize| {
                let header = b.ins().load(pointer, MemFlagsData::new(), input, 0);
                let data = b.ins().load(
                    pointer,
                    MemFlagsData::new(),
                    header,
                    offset_of!(Header, data) as i32,
                );
                let address = b.ins().load(
                    pointer,
                    MemFlagsData::new(),
                    header,
                    (offset_of!(Header, helpers) + which * 8) as i32,
                );
                let a = b.ins().iconst(types::I64, a as i64);
                let c = b.ins().iconst(types::I64, c as i64);
                let n = b.ins().iconst(types::I64, index as i64);
                let instruction = b.ins().call_indirect(sig, address, &[data, a, c, n]);
                b.inst_results(instruction)[0]
            };
            let conditional = match node.kind {
                f::Kind::Entry { next, failure } => Some((call(0, 0, 0), next, failure)),
                f::Kind::Fuel { next, failure, .. } => Some((call(1, 0, 0), next, failure)),
                f::Kind::Execute { next, failure, .. } => {
                    let status = match nodes[index].action.expect("preflighted instruction") {
                        Action::Move(from, to) => call(2, from, to),
                        Action::Cleanup => call(3, 0, 0),
                    };
                    Some((status, next, failure))
                }
                f::Kind::StageReturn {
                    value,
                    next,
                    failure,
                } => Some((call(4, layout.frame.value(value)?, 0), next, failure)),
                f::Kind::RetireReturn { next, failure } => Some((call(5, 0, 0), next, failure)),
                f::Kind::PublishReturn => {
                    let status = call(6, 0, 0);
                    let success = b.create_block();
                    let valid = b.ins().icmp_imm_u(IntCC::Equal, status, 0);
                    b.ins()
                        .brif(valid, success, &[], blocks[graph.failure().0 as usize], &[]);
                    b.switch_to_block(success);
                    let status = b.ins().iconst(types::I32, 0);
                    b.ins().return_(&[status]);
                    None
                }
                f::Kind::DiscardPrepared { next } => {
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::Raise { next, .. } => {
                    call(10, 0, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::RecordFailure { next } => {
                    call(7, 0, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::RetireFailure { next } => {
                    call(8, 0, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::DiscardResult { next } => {
                    call(9, 0, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                    None
                }
                f::Kind::PropagateFailure => {
                    let status = b.ins().iconst(types::I32, 2);
                    b.ins().return_(&[status]);
                    None
                }
                _ => {
                    return Err(Error::Unsupported {
                        function: layout.function,
                        span: node.span,
                    });
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
}
impl<'a> FunctionLayout<'a> {
    pub fn compile(
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        function: m::FunctionId,
        limits: m::Limits,
        cells: usize,
    ) -> Result<Self, Error> {
        let typed = flow.source().activity().moved().typed();
        let types = typed.places().declarations().types();
        if typed.functions().get(function.0 as usize).is_none() {
            return Err(Error::InvalidFunction(function));
        }
        fn combine(a: usize, extra: usize, common: usize) -> Result<usize, Error> {
            extra
                .checked_sub(common)
                .and_then(|delta| a.checked_add(delta))
                .ok_or(Error::Layout(l::Error::Overflow))
        }
        let existing = layouts.usage();
        let initial = m::Usage {
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
        };
        let frame = FramePlan::compile_after(layouts, typed, function, limits, cells, initial)?;
        Ok(Self {
            function,
            frame,
            source: flow,
            layouts,
        })
    }
    pub const fn function(&self) -> m::FunctionId {
        self.function
    }
    pub const fn frame(&self) -> &FramePlan<'a, 'a> {
        &self.frame
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
