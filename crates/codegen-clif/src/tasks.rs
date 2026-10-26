//! Native continuations for the RFC 0023 task proof. Arithmetic, branches and
//! failure edges are generated code; callbacks provide storage and scheduling.
#![allow(
    clippy::result_large_err,
    reason = "Keep task failure evidence inline."
)]
use cranelift_codegen::ir::{
    AbiParam, Block, InstBuilder, MemFlagsData, SigRef, SourceLoc, Value, condcodes::IntCC, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, failure as f},
};
use lang_runtime::{
    allocation as a, execution as e, layout as l,
    task::runner::{
        self as r,
        machine::Machine,
        primitives::{Flow, Primitive, Scalar},
    },
};
use std::{
    ffi::c_void,
    fmt::{Display, Formatter},
    mem::offset_of,
    panic::{AssertUnwindSafe, catch_unwind},
};

mod object;
pub use object::{LinkedProgram, ObjectArtifact};
#[cfg(all(test, target_pointer_width = "64"))]
mod tests;

#[derive(Debug)]
pub enum Error {
    Resource(m::Error),
    Layout(l::Error),
    Unsupported { span: m::Span },
    Entry,
    Backend(String),
}
impl From<m::Error> for Error {
    fn from(e: m::Error) -> Self {
        Self::Resource(e)
    }
}
impl From<l::Error> for Error {
    fn from(e: l::Error) -> Self {
        Self::Layout(e)
    }
}
impl Display for Error {
    fn fmt(&self, out: &mut Formatter<'_>) -> std::fmt::Result {
        write!(out, "native task compilation failed: {self:?}")
    }
}
impl std::error::Error for Error {}
fn backend(e: impl Display) -> Error {
    Error::Backend(e.to_string())
}
fn usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}
fn copied<T: Copy>(budget: &mut m::Budget, source: &[T]) -> Result<Vec<T>, Error> {
    let mut result = budget.vector(source.len())?;
    result.extend_from_slice(source);
    Ok(result)
}

#[derive(Clone, Copy)]
enum Binary {
    Add,
    Less,
    Equal,
}
#[derive(Clone, Copy)]
enum Emit {
    Storage,
    Constant(i64),
    Binary {
        left: usize,
        right: usize,
        op: Binary,
    },
    Case {
        from: usize,
        case: u32,
    },
    Guard(usize),
    Unwrap {
        from: usize,
        case: u32,
        panic_code: i64,
    },
    Panic(usize),
}
struct Node {
    primitive: Primitive,
    emit: Emit,
    span: m::Span,
    raised: Option<f::ExecutionFailure>,
    condition: Option<usize>,
}
struct Function {
    nodes: Vec<Node>,
    code: *const u8,
    metadata: super::program::FunctionCode,
}

/// In-memory native task code, bound to the exact independently verified proof.
/// Task package/wire publication remains outside this experimental entry point.
pub struct CompiledProgram<'p> {
    _owner: super::OwnedJitModule,
    image: Image<'p>,
}
struct Image<'p> {
    component: Option<&'p lang_runtime::component::program::Program<'p>>,
    proof: &'p o::tasks::VerifiedTasks,
    entry: m::FunctionId,
    functions: Vec<Option<Function>>,
    usage: m::Usage,
}
impl<'p> CompiledProgram<'p> {
    pub fn compile(proof: &'p o::tasks::VerifiedTasks, limits: m::Limits) -> Result<Self, Error> {
        let entry = proof
            .typed()
            .places()
            .declarations()
            .functions()
            .iter()
            .position(|f| f.name == "main")
            .map(|i| m::FunctionId(i as u32))
            .ok_or(Error::Entry)?;
        Self::compile_function(proof, entry, limits)
    }
    pub fn compile_function(
        proof: &'p o::tasks::VerifiedTasks,
        entry: m::FunctionId,
        limits: m::Limits,
    ) -> Result<Self, Error> {
        Self::compile_function_inner(proof, entry, limits, false, None)
    }
    #[doc(hidden)]
    pub fn compile_function_with_arguments(
        proof: &'p o::tasks::VerifiedTasks,
        entry: m::FunctionId,
        limits: m::Limits,
    ) -> Result<Self, Error> {
        Self::compile_function_inner(proof, entry, limits, true, None)
    }
    #[doc(hidden)]
    pub fn compile_component(
        program: &'p lang_runtime::component::program::Program<'p>,
        entry: m::FunctionId,
        limits: m::Limits,
    ) -> Result<Self, Error> {
        if !program.is_export(entry) {
            return Err(Error::Entry);
        }
        Self::compile_function_inner(program.proof(), entry, limits, true, Some(program))
    }
    fn compile_function_inner(
        proof: &'p o::tasks::VerifiedTasks,
        entry: m::FunctionId,
        limits: m::Limits,
        arguments: bool,
        component: Option<&'p lang_runtime::component::program::Program<'p>>,
    ) -> Result<Self, Error> {
        let mut owner = super::OwnedJitModule(Some(JITModule::new(
            JITBuilder::new(default_libcall_names()).map_err(backend)?,
        )));
        let module = owner.0.as_mut().expect("new module");
        let (mut functions, ids, budget) = prepare_program_profile(
            proof,
            entry,
            limits,
            module,
            "lang_tasks_r1",
            arguments,
            component,
        )?;
        module.finalize_definitions().map_err(backend)?;
        for (index, id) in ids {
            functions[index].as_mut().expect("prepared function").code =
                module.get_finalized_function(id);
        }
        Ok(Self {
            _owner: owner,
            image: Image {
                component,
                proof,
                entry,
                functions,
                usage: usage(&budget),
            },
        })
    }
    pub fn usage(&self) -> m::Usage {
        self.image.usage
    }
    pub fn metadata(&self) -> impl Iterator<Item = &super::program::FunctionCode> {
        self.image.functions.iter().flatten().map(|f| &f.metadata)
    }
    pub fn execute<P: a::Provider>(
        &self,
        allocator: &mut a::Allocator<P>,
        control: e::Control<'_>,
        limits: r::Limits,
    ) -> Result<r::Outcome, r::Failure> {
        r::execute_function(
            &mut Adapter(&self.image),
            self.image.proof,
            self.image.entry,
            allocator,
            control,
            limits,
        )
    }
    #[doc(hidden)]
    pub fn execute_with_arguments<P: a::Provider>(
        &self,
        allocator: &mut a::Allocator<P>,
        control: e::Control<'_>,
        limits: r::Limits,
        arguments: &[r::EntryArgument],
    ) -> Result<r::Outcome, r::Failure> {
        r::execute_function_with_arguments(
            &mut Adapter(&self.image),
            self.image.proof,
            self.image.entry,
            allocator,
            control,
            limits,
            arguments,
        )
    }
    #[doc(hidden)]
    pub fn execute_component<P: a::Provider>(
        &self,
        allocator: &mut a::Allocator<P>,
        control: e::Control<'_>,
        limits: r::Limits,
        arguments: &[r::EntryArgument],
        host: &mut dyn lang_runtime::component::execution::Host,
    ) -> Result<r::Outcome<lang_runtime::component::execution::Value>, r::Failure> {
        let Some(program) = self.image.component else {
            return Err(r::Failure {
                reason: r::Reason::Entry,
                function: Some(self.image.entry),
                span: None,
                callers: None,
                usage: Default::default(),
                tasks: Default::default(),
                events: Vec::new(),
                report: None,
                poisoned: false,
            });
        };
        r::execute_component(
            &mut Adapter(&self.image),
            program,
            self.image.entry,
            allocator,
            control,
            limits,
            arguments,
            host,
        )
    }
    /// Lend the original cumulative execution-preparation budget.
    #[doc(hidden)]
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep original preparation authority explicit alongside the existing component operands."
    )]
    pub fn execute_component_with_budget<P: a::Provider>(
        &self,
        allocator: &mut a::Allocator<P>,
        control: e::Control<'_>,
        limits: r::Limits,
        arguments: &[r::EntryArgument],
        host: &mut dyn lang_runtime::component::execution::Host,
        budget: &mut m::Budget,
    ) -> Result<r::Outcome<lang_runtime::component::execution::Value>, r::Failure> {
        let Some(program) = self.image.component else {
            return self.execute_component(allocator, control, limits, arguments, host);
        };
        r::execute_component_with_budget(
            &mut Adapter(&self.image),
            program,
            self.image.entry,
            allocator,
            control,
            limits,
            arguments,
            host,
            budget,
        )
    }
    /// Run a synchronous component scope in the original borrowed task.
    #[doc(hidden)]
    #[allow(
        clippy::too_many_arguments,
        reason = "Original running-task authority accompanies component preparation and control."
    )]
    pub fn execute_component_in_task<P: a::Provider>(
        &self,
        allocator: &mut a::Allocator<P>,
        control: e::Control<'_>,
        limits: r::Limits,
        arguments: &[r::EntryArgument],
        host: &mut dyn lang_runtime::component::execution::Host,
        budget: &mut m::Budget,
        current: lang_runtime::task::CurrentTask<'_>,
    ) -> Result<r::Outcome<lang_runtime::component::execution::Value>, r::Failure> {
        let Some(program) = self.image.component else {
            return Err(r::Failure {
                reason: r::Reason::Entry,
                function: Some(self.image.entry),
                span: None,
                callers: None,
                usage: Default::default(),
                tasks: current.usage(),
                events: Vec::new(),
                report: None,
                poisoned: false,
            });
        };
        r::execute_component_in_task(
            &mut Adapter(&self.image),
            program,
            self.image.entry,
            allocator,
            control,
            limits,
            arguments,
            host,
            budget,
            current,
        )
    }
}
struct Adapter<'a, 'p>(&'a Image<'p>);
impl<P: a::Provider> r::Backend<P> for Adapter<'_, '_> {
    fn supported(&self, op: &b::Op) -> bool {
        supported(op)
    }
    fn services_supported(&self) -> bool {
        true
    }
    fn resume(
        &mut self,
        proof: &o::tasks::VerifiedTasks,
        machine: &mut Machine<'_, '_, '_, '_, '_, P>,
        task: usize,
    ) -> Result<(), r::Reason> {
        if !std::ptr::eq(proof, self.0.proof) {
            return Err(r::Reason::Invariant);
        }
        invoke(self.0, machine, task)
    }
    fn cleanup(
        &mut self,
        machine: &mut Machine<'_, '_, '_, '_, '_, P>,
        task: usize,
    ) -> Result<(), r::Reason> {
        invoke(self.0, machine, task)
    }
}
type PreparedProgram = (
    Vec<Option<Function>>,
    Vec<(usize, cranelift_module::FuncId)>,
    m::Budget,
);
fn prepare_program<M: Module>(
    proof: &o::tasks::VerifiedTasks,
    entry: m::FunctionId,
    limits: m::Limits,
    module: &mut M,
    prefix: &str,
    arguments: bool,
) -> Result<PreparedProgram, Error> {
    prepare_program_profile(proof, entry, limits, module, prefix, arguments, None)
}
fn prepare_program_profile<M: Module>(
    proof: &o::tasks::VerifiedTasks,
    entry: m::FunctionId,
    limits: m::Limits,
    module: &mut M,
    prefix: &str,
    arguments: bool,
    component: Option<&lang_runtime::component::program::Program<'_>>,
) -> Result<PreparedProgram, Error> {
    let typed = proof.typed();
    let declarations = typed.places().declarations();
    if let Some(span) = declarations.component_feature_span()
        && component.is_none()
    {
        return Err(Error::Layout(l::Error::UnsupportedComponentProfile {
            span,
        }));
    }
    let types = declarations.types();
    let services = types.service_feature_span().is_some();
    let declaration = declarations
        .functions()
        .get(entry.0 as usize)
        .ok_or(Error::Entry)?;
    if (!arguments && !declaration.parameters.is_empty())
        || (component.is_none() && declaration.result.ty != o::Type::I64)
        || (component.is_none()
            && declaration.parameters.iter().any(|id| {
                !matches!(
                    declaration.locals[id.0 as usize].ty.ty,
                    o::Type::I64 | o::Type::Bool
                )
            }))
    {
        return Err(Error::Entry);
    }
    let mut budget = m::Budget::with_usage(
        limits,
        component.map(|p| p.usage()).unwrap_or(m::Usage {
            work: proof.plan().work_used(),
            allocation_bytes: proof.plan().requested_bytes(),
            elements: proof.plan().elements_used(),
        }),
    )?;
    let mut reachable = budget.vector(typed.functions().len())?;
    let mut seen = budget.vector(typed.functions().len())?;
    seen.resize(typed.functions().len(), false);
    reachable.push(entry.0 as usize);
    seen[entry.0 as usize] = true;
    let mut at = 0;
    while at < reachable.len() {
        let index = reachable[at];
        at += 1;
        let body = &typed.functions()[index];
        let declaration = &declarations.functions()[index];
        if !r::signature_supported(types, declaration) {
            return Err(Error::Unsupported {
                span: declaration.span,
            });
        }
        for ty in declaration
            .locals
            .iter()
            .map(|l| l.ty.ty)
            .chain(
                typed.places().functions()[index]
                    .values
                    .iter()
                    .map(|v| v.ty),
            )
            .chain(std::iter::once(declaration.result.ty))
        {
            budget.charge(1)?;
            let claim = matches!(ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Task(_)));
            let storage_supported = claim
                || (if component.is_some() {
                    r::managed_component_type_supported(types, ty, &mut budget)
                } else if services {
                    r::managed_service_type_supported(types, ty, &mut budget)
                } else {
                    r::managed_type_supported(types, ty, &mut budget)
                })
                .map_err(|e| match e {
                    r::Reason::Resource(e) => Error::Resource(e),
                    e => backend(format!("{e:?}")),
                })?;
            if !storage_supported {
                return Err(Error::Unsupported {
                    span: declaration.span,
                });
            }
        }
        for i in body.blocks.iter().flat_map(|b| &b.instructions) {
            budget.charge(1)?;
            if let b::Op::Import { import, .. } = i.op {
                let program = component.ok_or(Error::Unsupported { span: i.span })?;
                if !r::component_import_supported(program, import, &mut budget).map_err(
                    |error| match error {
                        r::Reason::Resource(error) => Error::Resource(error),
                        error => backend(format!("{error:?}")),
                    },
                )? {
                    return Err(Error::Unsupported { span: i.span });
                }
            }
            if !supported(&i.op) || !r::instruction_supported(typed, index, i) {
                return Err(Error::Unsupported { span: i.span });
            }
            if let b::Op::Call { function, .. } | b::Op::TaskSpawn { function, .. } = i.op
                && !seen[function.0 as usize]
            {
                seen[function.0 as usize] = true;
                reachable.push(function.0 as usize);
            }
        }
    }
    let layouts = if let Some(component) = component {
        l::Layouts::compile_component_program(component, limits)?
    } else if services {
        l::Layouts::compile_service_tasks(proof, limits)?
    } else if types.channel_feature_span().is_some() {
        l::Layouts::compile_channel_tasks(proof, limits)?
    } else {
        l::Layouts::compile_tasks(proof, limits)?
    };
    let combine = |a: usize, b: usize, c: usize| {
        b.checked_sub(c)
            .and_then(|d| a.checked_add(d))
            .ok_or_else(|| backend("preparation usage overflow"))
    };
    let old = usage(&budget);
    let extra = layouts.usage();
    budget = m::Budget::with_usage(
        limits,
        m::Usage {
            work: combine(old.work, extra.work, types.work_used())?,
            allocation_bytes: combine(
                old.allocation_bytes,
                extra.allocation_bytes,
                types.requested_bytes(),
            )?,
            elements: combine(old.elements, extra.elements, types.elements_used())?,
        },
    )?;
    let mut functions = budget.vector(typed.functions().len())?;
    functions.resize_with(typed.functions().len(), || None);
    let mut ids = budget.vector(reachable.len())?;
    for index in reachable {
        let function = m::FunctionId(index as u32);
        let plan = l::FramePlan::compile_tasks_after(
            &layouts,
            proof,
            function,
            limits,
            usize::MAX,
            usage(&budget),
        )?;
        budget = m::Budget::with_usage(limits, plan.usage())?;
        let nodes = prepare(proof, function, &plan, &mut budget, component.is_some())?;
        let symbol = format!("{prefix}_f{index}");
        let (id, metadata) = emit(
            module,
            &proof.plan().functions()[index],
            function,
            &nodes,
            symbol,
            &mut budget,
        )?;
        functions[index] = Some(Function {
            nodes,
            code: std::ptr::null(),
            metadata,
        });
        ids.push((index, id));
    }
    Ok((functions, ids, budget))
}

fn supported(op: &b::Op) -> bool {
    match op {
        b::Op::Import { .. } => true,
        b::Op::PrepareService { .. } | b::Op::ServiceSend { .. } | b::Op::ServiceReceive { .. } => {
            true
        }
        b::Op::CoreCall(call) if call.operation.is_service() => true,
        _ => r::task_supported(op),
    }
}

fn prepare(
    proof: &o::tasks::VerifiedTasks,
    function: m::FunctionId,
    plan: &l::FramePlan<'_, '_>,
    budget: &mut m::Budget,
    component: bool,
) -> Result<Vec<Node>, Error> {
    let typed = proof.typed();
    let body = &typed.functions()[function.0 as usize];
    let declarations = typed.places().declarations();
    if let Some(span) = declarations.component_feature_span()
        && !component
    {
        return Err(Error::Layout(l::Error::UnsupportedComponentProfile {
            span,
        }));
    }
    let locals = declarations.functions()[function.0 as usize].locals.len();
    let value = |r: m::Register| locals + r.0 as usize;
    let slots = |source: &[m::Register], budget: &mut m::Budget| -> Result<Vec<usize>, Error> {
        let mut out = budget.vector(source.len())?;
        out.extend(source.iter().map(|r| value(*r)));
        Ok(out)
    };
    let mut result = budget.vector(proof.plan().functions()[function.0 as usize].nodes().len())?;
    for node in proof.plan().functions()[function.0 as usize].nodes() {
        budget.charge(1)?;
        let mut native = Node {
            primitive: Primitive::Nop,
            emit: Emit::Storage,
            span: node.span,
            raised: None,
            condition: None,
        };
        native.primitive = match node.kind {
            f::Kind::Execute {
                site,
                next,
                failure,
            } => {
                let i = &body.blocks[site.block.0 as usize].instructions[site.order - 1];
                let to = || {
                    i.results
                        .first()
                        .map(|r| value(*r))
                        .ok_or_else(|| backend("missing result"))
                };
                match &i.op {
                    b::Op::I64(bits) => {
                        native.emit = Emit::Constant(*bits);
                        Primitive::Write {
                            to: to()?,
                            kind: Scalar::I64,
                        }
                    }
                    b::Op::Bool(bits) => {
                        native.emit = Emit::Constant(i64::from(*bits));
                        Primitive::Write {
                            to: to()?,
                            kind: Scalar::Bool,
                        }
                    }
                    b::Op::Unit => {
                        native.emit = Emit::Constant(0);
                        Primitive::Write {
                            to: to()?,
                            kind: Scalar::Unit,
                        }
                    }
                    op @ (b::Op::Add(a, b) | b::Op::LessThan(a, b) | b::Op::EqualI64(a, b)) => {
                        let (op, kind) = match op {
                            b::Op::Add(..) => (Binary::Add, Scalar::I64),
                            b::Op::LessThan(..) => (Binary::Less, Scalar::Bool),
                            _ => (Binary::Equal, Scalar::Bool),
                        };
                        native.emit = Emit::Binary {
                            left: value(*a),
                            right: value(*b),
                            op,
                        };
                        Primitive::Write { to: to()?, kind }
                    }
                    b::Op::Panic(from) => {
                        native.emit = Emit::Panic(value(*from));
                        Primitive::Nop
                    }
                    b::Op::Copy(from) | b::Op::Move(from) => Primitive::Copy {
                        from: value(*from),
                        to: to()?,
                    },
                    b::Op::Initialize { local, value: from } => Primitive::Copy {
                        from: value(*from),
                        to: local.0 as usize,
                    },
                    b::Op::CopyPlace(place) | b::Op::MovePlace(place) => Primitive::Load {
                        place: *place,
                        to: to()?,
                    },
                    b::Op::PreparePlace { destination, place } => Primitive::Prepare {
                        place: *place,
                        destination: destination.0 as usize,
                    },
                    b::Op::PrepareChannel {
                        destination,
                        endpoint,
                        ..
                    }
                    | b::Op::PrepareService {
                        destination,
                        endpoint,
                        ..
                    } => Primitive::PrepareChannel {
                        from: value(*endpoint),
                        destination: destination.0 as usize,
                    },
                    b::Op::ChannelSend { authority, payload }
                    | b::Op::ServiceSend { authority, payload } => Primitive::ChannelSend {
                        authority: authority.0 as usize,
                        payload: value(*payload),
                        to: to()?,
                        next,
                        failure,
                    },
                    b::Op::ChannelReceive { authority } | b::Op::ServiceReceive { authority } => {
                        Primitive::ChannelReceive {
                            authority: authority.0 as usize,
                            to: to()?,
                            next,
                            failure,
                        }
                    }
                    b::Op::Replace { place, value: from } => Primitive::Replace {
                        place: *place,
                        from: value(*from),
                    },
                    b::Op::ReplacePrepared {
                        destination,
                        value: from,
                    } => Primitive::ReplacePrepared {
                        destination: destination.0 as usize,
                        from: value(*from),
                    },
                    b::Op::DiscardPrepared(destination) => {
                        Primitive::DiscardPrepared(destination.0 as usize)
                    }
                    b::Op::DropLocal(local) | b::Op::DropLocalIfInitialized(local) => {
                        let mut v = budget.vector(1)?;
                        v.push(local.0 as usize);
                        Primitive::Forget(v)
                    }
                    b::Op::DropValue(from) | b::Op::DropValueIfAvailable(from) => {
                        let mut v = budget.vector(1)?;
                        v.push(value(*from));
                        Primitive::Forget(v)
                    }
                    b::Op::Cleanup(c) => {
                        let mut v = budget.vector(c.steps.len())?;
                        let mut loans = budget.vector(c.steps.len())?;
                        for step in &c.steps {
                            match step.target {
                                b::CleanupTarget::Local(l) => v.push(l.0 as usize),
                                b::CleanupTarget::Arena(l) => v.push(l.0 as usize),
                                b::CleanupTarget::Value(r) => v.push(value(r)),
                                b::CleanupTarget::Loan(loan) => loans.push(loan),
                            }
                        }
                        Primitive::Cleanup { slots: v, loans }
                    }
                    b::Op::Record {
                        fields,
                        initialization_order,
                        ..
                    } => {
                        let mut order = budget.vector(initialization_order.len())?;
                        for field in initialization_order {
                            order.push((
                                *field,
                                typed.places().functions()[function.0 as usize].values
                                    [fields[*field as usize].0 as usize]
                                    .span,
                            ));
                        }
                        Primitive::Record {
                            to: to()?,
                            fields: slots(fields, budget)?,
                            order,
                        }
                    }
                    b::Op::Variant { case, payload } => {
                        let mut order = budget.vector(payload.len())?;
                        order.extend(payload.iter().enumerate().map(|(index, from)| {
                            (
                                index as u32,
                                typed.places().functions()[function.0 as usize].values
                                    [from.0 as usize]
                                    .span,
                            )
                        }));
                        Primitive::Variant {
                            to: to()?,
                            case: *case,
                            payload: slots(payload, budget)?,
                            order,
                        }
                    }
                    b::Op::IsCase { value: from, case } => {
                        native.emit = Emit::Case {
                            from: value(*from),
                            case: *case,
                        };
                        Primitive::Write {
                            to: to()?,
                            kind: Scalar::Bool,
                        }
                    }
                    b::Op::BorrowCaseField {
                        value: from,
                        case,
                        field,
                        guard,
                        loan,
                    } => {
                        native.emit = Emit::Guard(value(*guard));
                        Primitive::BorrowField {
                            from: value(*from),
                            case: *case,
                            field: *field,
                            loan: *loan,
                            to: to()?,
                        }
                    }
                    b::Op::CopyCaseField {
                        value: from,
                        case,
                        field,
                        guard,
                    } => {
                        native.emit = Emit::Guard(value(*guard));
                        Primitive::Field {
                            from: value(*from),
                            case: *case,
                            field: *field,
                            to: to()?,
                        }
                    }
                    b::Op::UnpackVariant {
                        value: from,
                        case,
                        guard,
                    } => {
                        native.emit = Emit::Guard(value(*guard));
                        Primitive::Fields {
                            from: value(*from),
                            case: *case,
                            to: slots(&i.results, budget)?,
                        }
                    }
                    b::Op::CoreCall(c) => {
                        if let Some(contract) = c.operation.extraction() {
                            native.emit = Emit::Unwrap {
                                from: value(c.arguments[0]),
                                case: contract.case,
                                panic_code: contract.panic_code,
                            };
                        }
                        Primitive::Core {
                            operation: c.operation,
                            type_arguments: {
                                let mut arguments = budget.vector(c.type_arguments.len())?;
                                arguments.extend(c.type_arguments.iter().map(|t| t.ty));
                                arguments
                            },
                            result_loan: c.result_loan,
                            arguments: slots(&c.arguments, budget)?,
                            to: to()?,
                        }
                    }
                    b::Op::Import { import, arguments } => Primitive::Import {
                        import: *import,
                        arguments: copied(budget, arguments)?,
                        to: to()?,
                    },
                    b::Op::Call {
                        function,
                        arguments,
                    } => Primitive::Call {
                        function: *function,
                        arguments: copied(budget, arguments)?,
                        to: to()?,
                        next,
                        failure,
                    },
                    b::Op::TaskSpawn {
                        function,
                        arguments,
                    } => Primitive::Spawn {
                        function: *function,
                        arguments: copied(budget, arguments)?,
                        to: to()?,
                    },
                    b::Op::TaskAwait(from) => Primitive::Await {
                        from: value(*from),
                        to: to()?,
                        next,
                        failure,
                    },
                    b::Op::TaskCancel(place) => Primitive::Cancel(*place),
                    b::Op::TaskYield => Primitive::Yield(next),
                    b::Op::TaskCheck => Primitive::Check(next),
                    b::Op::TaskScopeEnter(scope) => Primitive::Enter(*scope),
                    b::Op::TaskScopeExit(scope) => Primitive::Exit {
                        scope: *scope,
                        next,
                        failure,
                    },
                    b::Op::TaskSaveReturn(from) => Primitive::Save(value(*from)),
                    b::Op::Borrow { loan, place, .. } => Primitive::Borrow {
                        loan: *loan,
                        place: *place,
                        to: to()?,
                    },
                    b::Op::EndBorrow { loan, .. } => Primitive::EndLoan(*loan),
                    b::Op::ArenaEnter { owner, allocator } => Primitive::ArenaEnter {
                        owner: owner.0 as usize,
                        allocator: value(*allocator),
                    },
                    b::Op::ArenaExit { owner } => {
                        let mut slots = budget.vector(1)?;
                        slots.push(owner.0 as usize);
                        Primitive::Forget(slots)
                    }
                }
            }
            f::Kind::Transfer { block, arm, .. } => {
                let edge = body.blocks[block.0 as usize]
                    .terminator
                    .kind
                    .targets()
                    .nth(arm as usize)
                    .ok_or_else(|| backend("missing edge"))?;
                let mut slots = budget.vector(edge.arguments.len())?;
                for (from, to) in edge
                    .arguments
                    .iter()
                    .zip(&body.blocks[edge.block.0 as usize].parameters)
                {
                    slots.push((
                        value(*from),
                        plan.scratch_cell(*to)
                            .ok_or_else(|| backend("missing scratch"))?,
                        value(*to),
                    ));
                }
                Primitive::Transfer(slots)
            }
            f::Kind::Branch { condition, .. } => {
                native.condition = Some(value(condition));
                Primitive::Nop
            }
            f::Kind::StageReturn { value: from, .. } => Primitive::Save(value(from)),
            f::Kind::PublishReturn => Primitive::Publish,
            f::Kind::PropagateFailure => Primitive::Propagate,
            f::Kind::RecordFailure { .. } => Primitive::RecordFailure,
            f::Kind::DiscardPrepared { .. } => Primitive::DiscardAllPrepared,
            f::Kind::DiscardResult { .. } => Primitive::DiscardResult,
            f::Kind::EndTaskLoans { .. } => Primitive::EndLoans,
            f::Kind::CancelTaskScopes { next } => Primitive::Close(next),
            f::Kind::JoinTaskScopes { .. } => Primitive::Children,
            f::Kind::FinishTask => Primitive::Finish,
            f::Kind::Raise { failure, .. } => {
                native.raised = Some(failure);
                Primitive::Nop
            }
            _ => Primitive::Nop,
        };
        result.push(native);
    }
    Ok(result)
}

// This private packet is scoped to an invocation and the matching compiled
// owner. It is neither a public C ABI nor a serialization representation.
type Helper = unsafe extern "C" fn(*mut Packet, u64, u64, u64, u64) -> u64;
#[repr(C)]
struct Packet {
    helper: Helper,
    data: *mut c_void,
    failed: u64,
}
struct Invocation<'a, 'p, 'l, 'ty, 'host, 'ctrl, P: a::Provider> {
    machine: &'a mut Machine<'p, 'l, 'ty, 'host, 'ctrl, P>,
    task: usize,
    nodes: &'a [Node],
    fatal: Option<r::Reason>,
    flow: Option<Flow>,
}
const APPLY: u64 = 0;
const CHARGE: u64 = 1;
const READ: u64 = 2;
const CASE: u64 = 3;
const PANIC: u64 = 4;
const UNWRAP: u64 = 5;
const RAISE: u64 = 6;
const INVARIANT: u64 = 7;
impl<P: a::Provider> Invocation<'_, '_, '_, '_, '_, '_, P> {
    fn apply(&mut self, site: usize, which: u64, a: u64) -> Result<u64, r::Reason> {
        let node = self.nodes.get(site).ok_or(r::Reason::Invariant)?;
        self.machine.frame_mut(self.task)?.node = f::NodeId(site as u32);
        match which {
            APPLY => {
                let flow = self
                    .machine
                    .apply(self.task, &node.primitive, a, node.span)?;
                if flow == Flow::Continue {
                    Ok(0)
                } else {
                    self.flow = Some(flow);
                    Ok(1)
                }
            }
            CHARGE => {
                self.machine.charge()?;
                Ok(0)
            }
            READ => {
                let frame = self.machine.frame(self.task)?;
                Ok(match frame.read(a as usize, self.machine.allocator)? {
                    l::Immediate::I64(bits) => bits as u64,
                    l::Immediate::Bool(bits) => u64::from(bits),
                    l::Immediate::Unit => 0,
                    _ => return Err(r::Reason::Invariant),
                })
            }
            CASE => {
                let frame = r::frame::Access::new(
                    &mut self.machine.records[self.task].frames,
                    &mut self.machine.services,
                )?;
                Ok(u64::from(frame.case(a as usize, self.machine.allocator)?))
            }
            PANIC => Err(r::Reason::Panic(Some(a as i64))),
            UNWRAP => Err(r::Reason::Panic(Some(a as i64))),
            RAISE => Err(r::Reason::Execution(
                node.raised.ok_or(r::Reason::Invariant)?,
            )),
            _ => Err(r::Reason::Invariant),
        }
    }
    fn fault(&mut self, site: usize, reason: r::Reason) {
        let Some(node) = self.nodes.get(site) else {
            self.fatal = Some(reason);
            return;
        };
        if let Err(reason) = self.machine.fault(self.task, reason, node.span) {
            self.fatal = Some(reason);
        }
    }
}
unsafe extern "C" fn helper<P: a::Provider>(
    packet: *mut Packet,
    site: u64,
    which: u64,
    a: u64,
    _b: u64,
) -> u64 {
    // SAFETY: generated code receives the live stack packet only from invoke;
    // its callback and opaque pointer were paired by that same generic invoke.
    let packet = unsafe { &mut *packet };
    let invocation = unsafe { &mut *packet.data.cast::<Invocation<'_, '_, '_, '_, '_, '_, P>>() };
    match catch_unwind(AssertUnwindSafe(|| {
        invocation.apply(site as usize, which, a)
    })) {
        Ok(Ok(value)) => value,
        Ok(Err(reason)) => {
            invocation.fault(site as usize, reason);
            packet.failed = 1;
            2
        }
        Err(_) => {
            invocation.fault(
                site as usize,
                r::Reason::Execution(f::ExecutionFailure::HostPanic),
            );
            packet.failed = 1;
            2
        }
    }
}
fn invoke<P: a::Provider>(
    program: &Image<'_>,
    machine: &mut Machine<'_, '_, '_, '_, '_, P>,
    task: usize,
) -> Result<(), r::Reason> {
    let frame = machine.frame(task)?;
    let pc = frame.node.0;
    let function = program
        .functions
        .get(frame.function.0 as usize)
        .and_then(Option::as_ref)
        .ok_or(r::Reason::Invariant)?;
    let mut invocation = Invocation {
        machine,
        task,
        nodes: &function.nodes,
        fatal: None,
        flow: None,
    };
    let mut packet = Packet {
        helper: helper::<P>,
        data: std::ptr::from_mut(&mut invocation).cast(),
        failed: 0,
    };
    // SAFETY: emit fixes this signature; the finalized code is owned and kept
    // alive by program. The packet/metadata/provider borrows outlive this call.
    let entry: unsafe extern "C" fn(*mut Packet, u32) -> u32 =
        unsafe { std::mem::transmute(function.code) };
    let status = unsafe { entry(&mut packet, pc) };
    if let Some(reason) = invocation.fatal {
        return Err(reason);
    }
    if status != 1 || invocation.flow.is_none() {
        return Err(r::Reason::Invariant);
    }
    Ok(())
}

fn helper_call(
    b: &mut FunctionBuilder<'_>,
    sig: SigRef,
    packet: Value,
    site: usize,
    which: u64,
    a: Value,
) -> Value {
    let function = b.ins().load(
        types::I64,
        MemFlagsData::new(),
        packet,
        offset_of!(Packet, helper) as i32,
    );
    let site = b.ins().iconst(types::I64, site as i64);
    let which = b.ins().iconst(types::I64, which as i64);
    let zero = b.ins().iconst(types::I64, 0);
    let call = b
        .ins()
        .call_indirect(sig, function, &[packet, site, which, a, zero]);
    b.inst_results(call)[0]
}
fn call_imm(
    b: &mut FunctionBuilder<'_>,
    sig: SigRef,
    packet: Value,
    site: usize,
    which: u64,
    a: u64,
) -> Value {
    let a = b.ins().iconst(types::I64, a as i64);
    helper_call(b, sig, packet, site, which, a)
}
fn read(
    b: &mut FunctionBuilder<'_>,
    sig: SigRef,
    packet: Value,
    site: usize,
    which: u64,
    slot: usize,
    failure: Block,
) -> Value {
    let value = call_imm(b, sig, packet, site, which, slot as u64);
    let failed = b.ins().load(
        types::I64,
        MemFlagsData::new(),
        packet,
        offset_of!(Packet, failed) as i32,
    );
    let valid = b.create_block();
    b.ins().brif(failed, failure, &[], valid, &[]);
    b.switch_to_block(valid);
    value
}
fn conditional(
    b: &mut FunctionBuilder<'_>,
    status: Value,
    next: Block,
    failure: Block,
    leave: Block,
) {
    let ok = b.ins().icmp_imm_u(IntCC::Equal, status, 0);
    let check = b.create_block();
    b.ins().brif(ok, next, &[], check, &[]);
    b.switch_to_block(check);
    let left = b.ins().icmp_imm_u(IntCC::Equal, status, 1);
    b.ins().brif(left, leave, &[], failure, &[]);
}
fn emit<M: Module>(
    module: &mut M,
    graph: &f::Function,
    function: m::FunctionId,
    nodes: &[Node],
    symbol: String,
    budget: &mut m::Budget,
) -> Result<(cranelift_module::FuncId, super::program::FunctionCode), Error> {
    if module.target_config().pointer_type() != types::I64 {
        return Err(backend("task continuations require 64-bit pointers"));
    }
    let mut signature = module.make_signature();
    signature
        .params
        .extend([AbiParam::new(types::I64), AbiParam::new(types::I32)]);
    signature.returns.push(AbiParam::new(types::I32));
    let mut helper_signature = module.make_signature();
    helper_signature
        .params
        .extend([AbiParam::new(types::I64); 5]);
    helper_signature.returns.push(AbiParam::new(types::I64));
    let id = module
        .declare_function(&symbol, Linkage::Export, &signature)
        .map_err(backend)?;
    let mut context = module.make_context();
    context.func.signature = signature;
    let mut builder = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut context.func, &mut builder);
        let sig = b.func.import_signature(helper_signature);
        let entry = b.create_block();
        let invalid = b.create_block();
        let leave = b.create_block();
        let mut blocks = budget.vector(nodes.len())?;
        for _ in nodes {
            blocks.push(b.create_block());
        }
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let packet = b.block_params(entry)[0];
        let pc = b.block_params(entry)[1];
        for (index, block) in blocks.iter().enumerate() {
            budget.charge(1)?;
            let fallback = if index + 1 == blocks.len() {
                invalid
            } else {
                b.create_block()
            };
            let selected = b.ins().icmp_imm_u(IntCC::Equal, pc, index as i64);
            b.ins().brif(selected, *block, &[], fallback, &[]);
            if fallback != invalid {
                b.switch_to_block(fallback);
            }
        }
        for (index, node) in graph.nodes().iter().enumerate() {
            budget.charge(1)?;
            b.switch_to_block(blocks[index]);
            b.set_srcloc(SourceLoc::new(index as u32));
            let metadata = &nodes[index];
            match node.kind {
                f::Kind::Entry { next, .. }
                | f::Kind::RetireReturn { next, .. }
                | f::Kind::RetireFailure { next } => {
                    b.ins().jump(blocks[next.0 as usize], &[]);
                }
                f::Kind::Fuel { next, failure, .. } => {
                    let status = call_imm(&mut b, sig, packet, index, CHARGE, 0);
                    conditional(
                        &mut b,
                        status,
                        blocks[next.0 as usize],
                        blocks[failure.0 as usize],
                        leave,
                    );
                }
                f::Kind::Branch {
                    then_node,
                    else_node,
                    ..
                } => {
                    let condition = metadata
                        .condition
                        .ok_or_else(|| backend("missing branch cell"))?;
                    let actual = read(
                        &mut b,
                        sig,
                        packet,
                        index,
                        READ,
                        condition,
                        blocks[graph.failure().0 as usize],
                    );
                    b.ins().brif(
                        actual,
                        blocks[then_node.0 as usize],
                        &[],
                        blocks[else_node.0 as usize],
                        &[],
                    );
                }
                f::Kind::Execute { next, failure, .. } => {
                    let failure = blocks[failure.0 as usize];
                    let bits = match metadata.emit {
                        Emit::Storage => b.ins().iconst(types::I64, 0),
                        Emit::Constant(bits) => b.ins().iconst(types::I64, bits),
                        Emit::Binary { left, right, op } => {
                            let left = read(&mut b, sig, packet, index, READ, left, failure);
                            let right = read(&mut b, sig, packet, index, READ, right, failure);
                            match op {
                                Binary::Add => b.ins().iadd(left, right),
                                Binary::Less => {
                                    let v = b.ins().icmp(IntCC::SignedLessThan, left, right);
                                    b.ins().uextend(types::I64, v)
                                }
                                Binary::Equal => {
                                    let v = b.ins().icmp(IntCC::Equal, left, right);
                                    b.ins().uextend(types::I64, v)
                                }
                            }
                        }
                        Emit::Case { from, case } => {
                            let actual = read(&mut b, sig, packet, index, CASE, from, failure);
                            let v = b.ins().icmp_imm_u(IntCC::Equal, actual, i64::from(case));
                            b.ins().uextend(types::I64, v)
                        }
                        Emit::Guard(guard) => {
                            let actual = read(&mut b, sig, packet, index, READ, guard, failure);
                            let valid = b.ins().icmp_imm_u(IntCC::Equal, actual, 1);
                            let good = b.create_block();
                            let bad = b.create_block();
                            b.ins().brif(valid, good, &[], bad, &[]);
                            b.switch_to_block(bad);
                            call_imm(&mut b, sig, packet, index, INVARIANT, 0);
                            b.ins().jump(failure, &[]);
                            b.switch_to_block(good);
                            b.ins().iconst(types::I64, 0)
                        }
                        Emit::Unwrap {
                            from,
                            case,
                            panic_code,
                        } => {
                            let actual = read(&mut b, sig, packet, index, CASE, from, failure);
                            let valid = b.ins().icmp_imm_u(IntCC::Equal, actual, i64::from(case));
                            let good = b.create_block();
                            let bad = b.create_block();
                            b.ins().brif(valid, good, &[], bad, &[]);
                            b.switch_to_block(bad);
                            call_imm(&mut b, sig, packet, index, UNWRAP, panic_code as u64);
                            b.ins().jump(failure, &[]);
                            b.switch_to_block(good);
                            b.ins().iconst(types::I64, 0)
                        }
                        Emit::Panic(from) => {
                            let bits = read(&mut b, sig, packet, index, READ, from, failure);
                            helper_call(&mut b, sig, packet, index, PANIC, bits);
                            b.ins().jump(failure, &[]);
                            continue;
                        }
                    };
                    let status = helper_call(&mut b, sig, packet, index, APPLY, bits);
                    conditional(&mut b, status, blocks[next.0 as usize], failure, leave);
                }
                f::Kind::Transfer { target, .. } => {
                    let status = call_imm(&mut b, sig, packet, index, APPLY, 0);
                    conditional(&mut b, status, blocks[target.0 as usize], invalid, leave);
                }
                f::Kind::StageReturn { next, .. }
                | f::Kind::DiscardPrepared { next }
                | f::Kind::RecordFailure { next }
                | f::Kind::DiscardResult { next }
                | f::Kind::EndTaskLoans { next }
                | f::Kind::CancelTaskScopes { next }
                | f::Kind::JoinTaskScopes { next } => {
                    let status = call_imm(&mut b, sig, packet, index, APPLY, 0);
                    conditional(&mut b, status, blocks[next.0 as usize], invalid, leave);
                }
                f::Kind::Raise { next, .. } => {
                    call_imm(&mut b, sig, packet, index, RAISE, 0);
                    b.ins().jump(blocks[next.0 as usize], &[]);
                }
                f::Kind::PublishReturn | f::Kind::PropagateFailure | f::Kind::FinishTask => {
                    let status = call_imm(&mut b, sig, packet, index, APPLY, 0);
                    let good = b.ins().icmp_imm_u(IntCC::Equal, status, 1);
                    b.ins().brif(good, leave, &[], invalid, &[]);
                }
            }
        }
        b.switch_to_block(leave);
        let status = b.ins().iconst(types::I32, 1);
        b.ins().return_(&[status]);
        b.switch_to_block(invalid);
        let status = b.ins().iconst(types::I32, 2);
        b.ins().return_(&[status]);
        b.seal_all_blocks();
        b.finalize(module.target_config());
    }
    module.define_function(id, &mut context).map_err(backend)?;
    let code = context.compiled_code().expect("defined function");
    let mut source_ranges = budget.vector(code.buffer.get_srclocs_sorted().len())?;
    for range in code.buffer.get_srclocs_sorted() {
        if !range.loc.is_default() {
            source_ranges.push(super::program::SourceRange {
                start: range.start,
                end: range.end,
                span: nodes[range.loc.bits() as usize].span,
            });
        }
    }
    Ok((
        id,
        super::program::FunctionCode {
            function,
            symbol,
            code_size: code.code_buffer().len(),
            source_ranges,
        },
    ))
}
