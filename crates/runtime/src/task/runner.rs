//! Shared task invocation state and storage. Backend evaluators remain separate.
#![allow(
    clippy::result_large_err,
    reason = "Preserve failure evidence inline during cleanup."
)]
use crate::{allocation as a, cleanup as c, execution as e, layout as l, task as t};
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, failure as f, places as p},
};
use std::fmt::{Display, Formatter};
#[doc(hidden)]
pub mod frame;
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub tasks: t::Limits,
    pub channels: crate::channel::Limits,
    pub preparation: m::Limits,
    /// Zero disables observation. Trace storage is reserved before guest entry.
    pub trace_events: usize,
    /// Includes the primary record; zero refuses execution before guest entry.
    pub cleanup_failure_records: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            tasks: Default::default(),
            channels: Default::default(),
            preparation: Default::default(),
            trace_events: 0,
            cleanup_failure_records: c::Limits::default().failure_records,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventKind {
    Resume,
    Spawn { child: usize },
    Await { child: usize },
    Yield,
    Check,
    ScopeEnter(u32),
    ScopeExit(u32),
    Cleanup(t::Outcome),
    Complete(t::Outcome),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    pub task: usize,
    pub function: m::FunctionId,
    pub kind: EventKind,
    pub span: m::Span,
}
#[derive(Debug)]
pub enum Reason {
    ChannelDeadlock(crate::channel::Deadlock),
    Channel(crate::channel::managed::Error),
    Control(e::Failure),
    Schedule(t::Error),
    Layout(l::Error),
    Resource(m::Error),
    Execution(f::ExecutionFailure),
    Unsupported,
    Entry,
    Invariant,
    Panic(Option<i64>),
    Cancelled,
    TraceLimit,
    Cleanup,
}
impl From<l::Error> for Reason {
    fn from(e: l::Error) -> Self {
        Self::Layout(e)
    }
}
impl From<m::Error> for Reason {
    fn from(e: m::Error) -> Self {
        Self::Resource(e)
    }
}
impl From<t::Error> for Reason {
    fn from(e: t::Error) -> Self {
        Self::Schedule(e)
    }
}
impl From<e::Failure> for Reason {
    fn from(e: e::Failure) -> Self {
        Self::Control(e)
    }
}
impl Reason {
    fn service(error: &(dyn std::error::Error + 'static)) -> Self {
        e::service_failure(error).map_or(Self::Invariant, Self::Control)
    }
}
#[derive(Debug)]
pub struct Failure {
    pub reason: Reason,
    pub function: Option<m::FunctionId>,
    pub span: Option<m::Span>,
    pub usage: e::Usage,
    pub tasks: t::Usage,
    pub events: Vec<Event>,
    pub report: Option<c::Report>,
    pub poisoned: bool,
}
impl Display for Failure {
    fn fmt(&self, out: &mut Formatter<'_>) -> std::fmt::Result {
        write!(out, "task execution failed: {:?}", self.reason)
    }
}
impl std::error::Error for Failure {}
pub struct Outcome {
    pub value: i64,
    pub usage: e::Usage,
    pub tasks: t::Usage,
    pub allocations: a::Stats,
    pub events: Vec<Event>,
    /// Suppressed cleanup evidence, including a cancelled child's cleanup.
    pub report: Option<c::Report>,
    pub poisoned: bool,
}

fn usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}

/// Compiler-internal evaluator boundary. No wire or host ABI stability is implied.
#[doc(hidden)]
pub trait Backend<P: a::Provider> {
    fn supported(&self, op: &b::Op) -> bool;
    fn services_supported(&self) -> bool {
        false
    }
    fn resume(
        &mut self,
        proof: &o::tasks::VerifiedTasks,
        machine: &mut machine::Machine<'_, '_, '_, '_, '_, P>,
        task: usize,
    ) -> Result<(), Reason>;
    fn cleanup(
        &mut self,
        machine: &mut machine::Machine<'_, '_, '_, '_, '_, P>,
        task: usize,
    ) -> Result<(), Reason>;
}
#[doc(hidden)]
pub mod machine;
#[doc(hidden)]
pub mod primitives;
use frame::{Frame, Pending};
#[doc(hidden)]
pub mod services;

fn reserve_push<T>(budget: &mut m::Budget, out: &mut Vec<T>) -> Result<(), Reason> {
    budget.charge(1)?;
    if out.len() == out.capacity() {
        let capacity = out
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_mul(2))
            .ok_or(Reason::Invariant)?;
        budget.allocate::<T>(capacity)?;
        out.try_reserve_exact(capacity - out.len())
            .map_err(|_| Reason::Invariant)?;
    }
    Ok(())
}

pub fn execute_entry<P: a::Provider, B: Backend<P>>(
    backend: &mut B,
    proof: &o::tasks::VerifiedTasks,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    limits: Limits,
) -> Result<Outcome, Failure> {
    let entry = proof
        .typed()
        .places()
        .declarations()
        .functions()
        .iter()
        .position(|f| f.name == "main")
        .map(|i| m::FunctionId(i as u32));
    execute(backend, proof, entry, allocator, control, limits)
}
pub fn execute_function<P: a::Provider, B: Backend<P>>(
    backend: &mut B,
    proof: &o::tasks::VerifiedTasks,
    function: m::FunctionId,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    limits: Limits,
) -> Result<Outcome, Failure> {
    execute(backend, proof, Some(function), allocator, control, limits)
}
fn execute<P: a::Provider, B: Backend<P>>(
    backend: &mut B,
    proof: &o::tasks::VerifiedTasks,
    entry: Option<m::FunctionId>,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    limits: Limits,
) -> Result<Outcome, Failure> {
    let typed = proof.typed();
    let declarations = typed.places().declarations();
    let types = declarations.types();
    let span = entry
        .and_then(|f| declarations.functions().get(f.0 as usize))
        .map(|f| f.signature_span);
    let fail = |reason| Failure {
        reason,
        function: entry,
        span,
        usage: Default::default(),
        tasks: Default::default(),
        events: Vec::new(),
        report: None,
        poisoned: false,
    };
    let prepared=(||->Result<_,Reason>{
        control.check()?;
        if control.service_work_limit().is_some()&&(!declarations.imports().is_empty()||!e::service_work_supported(proof.plan())){return Err(Reason::Unsupported);}
        let entry=entry.ok_or(Reason::Entry)?;
        let decl=declarations.functions().get(entry.0 as usize).ok_or(Reason::Entry)?;
        if !decl.parameters.is_empty() || decl.result.ty!=o::Type::I64 {return Err(Reason::Entry);}
        if limits.tasks.live_tasks==0 {return Err(t::Error::LiveTaskLimit{limit:0}.into());}
        if limits.tasks.total_tasks==0 {return Err(t::Error::TotalTaskLimit{limit:0}.into());}
        let mut budget=m::Budget::with_usage(limits.preparation, m::Usage{work:proof.plan().work_used(),allocation_bytes:proof.plan().requested_bytes(),elements:proof.plan().elements_used()})?;
        let mut reachable=budget.vector(typed.functions().len())?;
        let mut seen=budget.vector(typed.functions().len())?; seen.resize(typed.functions().len(),false);
        reachable.push(entry.0 as usize); seen[entry.0 as usize]=true;
        let mut at=0;
        while at<reachable.len(){
            let index=reachable[at];at+=1;
            let declaration=&declarations.functions()[index]; let body=&typed.functions()[index];
            if !signature_supported(types,declaration) {return Err(Reason::Unsupported);}


            for ty in declaration.locals.iter().map(|l|l.ty.ty).chain(typed.places().functions()[index].values.iter().map(|v|v.ty)).chain(std::iter::once(declaration.result.ty)) {
                budget.charge(1)?;
                let claim=matches!(ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Task(_)));
                if !claim && !managed_type_supported_profile(types,ty,&mut budget,backend.services_supported())? {return Err(Reason::Unsupported);}
            }
            for instruction in body.blocks.iter().flat_map(|b|&b.instructions){
                budget.charge(1)?;
                if !backend.supported(&instruction.op)||!instruction_supported(typed,index,instruction){return Err(Reason::Unsupported);}
                if let b::Op::Call {function,..}|b::Op::TaskSpawn {function,..}=instruction.op
                    && !seen[function.0 as usize] { seen[function.0 as usize]=true;reachable.push(function.0 as usize); }
            }
        }
        let layouts=if types.service_feature_span().is_some() && backend.services_supported() {l::Layouts::compile_service_tasks(proof,limits.preparation)?} else if types.channel_feature_span().is_some() {l::Layouts::compile_channel_tasks(proof,limits.preparation)?} else {l::Layouts::compile_tasks(proof,limits.preparation)?};
        let extra=|a:usize,b:usize,c:usize|a.checked_add(b.checked_sub(c)?);
        let previous=usage(&budget);let layout=layouts.usage();
        let initial=m::Usage {work:extra(previous.work,layout.work,types.work_used()).ok_or(Reason::Invariant)?, allocation_bytes:extra(previous.allocation_bytes,layout.allocation_bytes,types.requested_bytes()).ok_or(Reason::Invariant)?,elements:extra(previous.elements,layout.elements,types.elements_used()).ok_or(Reason::Invariant)?};
        budget=m::Budget::with_usage(limits.preparation,initial)?;
        Ok((entry,layouts,budget,reachable))
    })().map_err(fail)?;
    let (entry, layouts, mut budget, reachable) = prepared;
    let result = (|| -> Result<_, Reason> {
        let mut plans = budget.vector(typed.functions().len())?;
        plans.resize_with(typed.functions().len(), || None);
        for function in reachable {
            let plan = l::FramePlan::compile_tasks_after(
                &layouts,
                proof,
                m::FunctionId(function as u32),
                limits.preparation,
                usize::MAX,
                usage(&budget),
            )?;
            budget = m::Budget::with_usage(limits.preparation, plan.usage())?;
            plans[function] = Some(plan);
        }
        let machine = machine::Machine::new(
            proof, &plans, &layouts, allocator, control, limits, budget, entry,
        )?;
        Ok(machine.run(backend))
    })();
    match result {
        Ok(result) => result,
        Err(reason) => Err(fail(reason)),
    }
}

#[doc(hidden)]
pub fn task_supported(op: &b::Op) -> bool {
    match op {
        b::Op::PrepareService { .. } | b::Op::ServiceSend { .. } | b::Op::ServiceReceive { .. } => {
            false
        }
        b::Op::PrepareChannel { .. } | b::Op::ChannelSend { .. } | b::Op::ChannelReceive { .. } => {
            true
        }
        b::Op::Import { .. } => false,
        b::Op::CoreCall(call) => matches!(
            call.operation,
            o::core::Operation::ChannelNew
                | o::core::Operation::ChannelSender
                | o::core::Operation::ChannelClone
                | o::core::Operation::ChannelReceiver
                | o::core::Operation::ChannelSeal
                | o::core::Operation::ChannelUnsent
                | o::core::Operation::OptionUnwrap
                | o::core::Operation::ResultUnwrap
                | o::core::Operation::ResultUnwrapErr
                | o::core::Operation::Allocator
                | o::core::Operation::BytesNew
                | o::core::Operation::BytesPush
                | o::core::Operation::BytesLen
                | o::core::Operation::BytesGet
                | o::core::Operation::BytesSet
                | o::core::Operation::BytesClone
                | o::core::Operation::TextFromUtf8
                | o::core::Operation::TextLen
                | o::core::Operation::TextEqual
                | o::core::Operation::TextClone
                | o::core::Operation::TextBytes
                | o::core::Operation::TextConcat
                | o::core::Operation::VecNew
                | o::core::Operation::VecLen
                | o::core::Operation::VecPush
                | o::core::Operation::VecPop
                | o::core::Operation::VecGet
                | o::core::Operation::VecGetMut
                | o::core::Operation::MapNew
                | o::core::Operation::MapLen
                | o::core::Operation::MapInsert
                | o::core::Operation::MapGet
                | o::core::Operation::MapRemove
                | o::core::Operation::SharedNew
                | o::core::Operation::SharedClone
                | o::core::Operation::SharedRead
                | o::core::Operation::SharedReplace
                | o::core::Operation::WeakNew
                | o::core::Operation::WeakClone
                | o::core::Operation::WeakUpgrade
                | o::core::Operation::ArenaNew
                | o::core::Operation::ArenaRead
                | o::core::Operation::ArenaReplace
        ),
        b::Op::BorrowCaseField { .. }
        | b::Op::Borrow { .. }
        | b::Op::I64(_)
        | b::Op::Bool(_)
        | b::Op::Unit
        | b::Op::Panic(_)
        | b::Op::Add(..)
        | b::Op::LessThan(..)
        | b::Op::EqualI64(..)
        | b::Op::Copy(_)
        | b::Op::Move(_)
        | b::Op::CopyPlace(_)
        | b::Op::MovePlace(_)
        | b::Op::Initialize { .. }
        | b::Op::Replace { .. }
        | b::Op::PreparePlace { .. }
        | b::Op::ReplacePrepared { .. }
        | b::Op::DiscardPrepared(_)
        | b::Op::DropValue(_)
        | b::Op::DropLocal(_)
        | b::Op::DropLocalIfInitialized(_)
        | b::Op::DropValueIfAvailable(_)
        | b::Op::Cleanup(_)
        | b::Op::EndBorrow { .. }
        | b::Op::Call { .. }
        | b::Op::Record { .. }
        | b::Op::Variant { .. }
        | b::Op::IsCase { .. }
        | b::Op::CopyCaseField { .. }
        | b::Op::UnpackVariant { .. }
        | b::Op::TaskScopeEnter(_)
        | b::Op::TaskScopeExit(_)
        | b::Op::TaskSpawn { .. }
        | b::Op::TaskAwait(_)
        | b::Op::TaskCancel(_)
        | b::Op::TaskYield
        | b::Op::TaskCheck
        | b::Op::TaskSaveReturn(_) => true,
        b::Op::ArenaEnter { .. } | b::Op::ArenaExit { .. } => true,
    }
}

/// Synchronous loans borrow live ancestors; no ordinary loan crosses a task transfer.
#[doc(hidden)]
pub fn signature_supported(
    types: &o::VerifiedTypes,
    declaration: &o::declarations::Function,
) -> bool {
    let asynchronous = declaration.effect.is_async();
    declaration.parameters.iter().all(|id|{
        let ty=declaration.locals[id.0 as usize].ty.ty;
        let loan=matches!(ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Loan(..)));
        if loan {return !asynchronous;}
        types.properties(ty).is_some_and(|p|if asynchronous {p.is_send()}else{!p.contains_loan&&!p.contains_allocator})
    })&&types.properties(declaration.result.ty).is_some_and(|p|if asynchronous {p.is_send()}else{!p.contains_loan&&!p.contains_allocator&&!p.contains_arena})
}
/// Supported whole-sum extraction forms in the managed task adapter.
#[doc(hidden)]
pub fn instruction_supported(
    typed: &b::TypedBodies,
    function: usize,
    instruction: &b::Instruction,
) -> bool {
    let b::Op::UnpackVariant { value, .. } = instruction.op else {
        return true;
    };
    let ty = typed.places().functions()[function].values[value.0 as usize].ty;
    let types = typed.places().declarations().types();
    types.properties(ty).is_some_and(|p| p.is_copy)
        || instruction.results.is_empty()
        || (instruction.results.len() == 1
            && matches!(ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Option(_)|o::Constructor::Result(..))))
}
/// Initial managed task service family; traversal is iterative and budgeted.
#[doc(hidden)]
pub fn managed_type_supported(
    types: &o::VerifiedTypes,
    ty: o::Type,
    budget: &mut m::Budget,
) -> Result<bool, Reason> {
    managed_type_supported_profile(types, ty, budget, false)
}
fn managed_type_supported_profile(
    types: &o::VerifiedTypes,
    ty: o::Type,
    budget: &mut m::Budget,
    services: bool,
) -> Result<bool, Reason> {
    let mut pending = budget.vector(1)?;
    pending.push(ty);
    let mut seen = budget.vector(0)?;
    while let Some(ty) = pending.pop() {
        budget.charge(seen.len().saturating_add(1))?;
        if seen.contains(&ty) {
            continue;
        }
        reserve_push(budget, &mut seen)?;
        seen.push(ty);
        let properties = types.properties(ty).ok_or(Reason::Invariant)?;
        if properties.is_copy
            && !properties.contains_loan
            && !properties.contains_allocator
            && !properties.contains_arena
            && !properties.contains_owner
        {
            continue;
        }
        if properties.contains_loan
            && !matches!(ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Loan(..)|o::Constructor::Option(_)|o::Constructor::Result(..)))
        {
            return Ok(false);
        }
        match ty {
            o::Type::I64 | o::Type::Bool => {}
            o::Type::Constructed(id) => {
                match types.table().constructed[id.0 as usize].constructor {
                    o::Constructor::Builtin(_) => {}
                    constructor @ (o::Constructor::Option(_)
                    | o::Constructor::SendResult(_)
                    | o::Constructor::Result(..)
                    | o::Constructor::Vec(_)
                    | o::Constructor::Map(..)
                    | o::Constructor::Shared(_)
                    | o::Constructor::Weak(_)
                    | o::Constructor::Arena(_)
                    | o::Constructor::ArenaOwner(_)
                    | o::Constructor::Loan(..)) => {
                        for argument in constructor.arguments() {
                            reserve_push(budget, &mut pending)?;
                            pending.push(argument);
                        }
                    }
                    o::Constructor::Task(_) => {}
                    o::Constructor::ServiceSetup(..)
                    | o::Constructor::ServiceOwner(..)
                    | o::Constructor::ServiceHandle(_) => {
                        if !services {
                            return Ok(false);
                        }
                        for argument in types.table().constructed[id.0 as usize]
                            .constructor
                            .arguments()
                        {
                            reserve_push(budget, &mut pending)?;
                            pending.push(argument);
                        }
                    }
                    o::Constructor::ChannelSetup(_)
                    | o::Constructor::Sender(_)
                    | o::Constructor::Receiver(_) => {
                        let message = types.table().constructed[id.0 as usize]
                            .constructor
                            .arguments()
                            .next()
                            .ok_or(Reason::Invariant)?;
                        reserve_push(budget, &mut pending)?;
                        pending.push(message);
                    }
                }
            }
            o::Type::Nominal(id) => match &types.table().definitions[id.0 as usize].kind {
                o::DefinitionKind::Record(fields) => {
                    for field in fields {
                        reserve_push(budget, &mut pending)?;
                        pending.push(field.ty.ty);
                    }
                }
                o::DefinitionKind::Enum(cases) => {
                    for field in cases.iter().flat_map(|c| &c.payload) {
                        reserve_push(budget, &mut pending)?;
                        pending.push(field.ty);
                    }
                }
            },
        }
    }
    Ok(true)
}
