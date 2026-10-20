//! RFC 0023 task adapter over independently verified ownership MIR.
//! Guest frames/finite values use managed storage; scheduler handles remain
//! invocation-local claims. Existing synchronous APIs keep their own profile.
#![allow(
    clippy::result_large_err,
    reason = "Keep task failures and cleanup evidence inline without allocating during failure handling."
)]
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, failure as f, places as p},
};
use lang_runtime::{allocation as a, execution as e, layout as l, task as t};
use std::fmt::{Display, Formatter};

mod frame;
mod instructions;
mod machine;
use frame::{Frame, Pending};

#[derive(Clone, Copy, Debug, Default)]
pub struct Limits {
    pub tasks: t::Limits,
    pub preparation: m::Limits,
    /// Zero disables observation. Trace storage is reserved before guest entry.
    pub trace_events: usize,
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
#[derive(Debug)]
pub struct Failure {
    pub reason: Reason,
    pub function: Option<m::FunctionId>,
    pub span: Option<m::Span>,
    pub usage: e::Usage,
    pub tasks: t::Usage,
    pub events: Vec<Event>,
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
}

fn usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}
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

pub fn execute_entry<P: a::Provider>(
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
    execute(proof, entry, allocator, control, limits)
}
pub fn execute_function<P: a::Provider>(
    proof: &o::tasks::VerifiedTasks,
    function: m::FunctionId,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    limits: Limits,
) -> Result<Outcome, Failure> {
    execute(proof, Some(function), allocator, control, limits)
}
fn execute<P: a::Provider>(
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
    };
    let prepared=(||->Result<_,Reason>{
        control.check()?;
        if control.service_work_limit().is_some(){return Err(Reason::Unsupported);}
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
            if body.loans!=0 {return Err(Reason::Unsupported);}
            for ty in declaration.locals.iter().map(|l|l.ty.ty).chain(typed.places().functions()[index].values.iter().map(|v|v.ty)).chain(std::iter::once(declaration.result.ty)) {
                budget.charge(1)?;
                let claim=matches!(ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Task(_)));
                if !claim && !types.properties(ty).is_some_and(|p|p.is_copy&&!p.contains_loan&&!p.contains_arena&&!p.contains_allocator&&!p.contains_owner){return Err(Reason::Unsupported);}
            }
            for instruction in body.blocks.iter().flat_map(|b|&b.instructions){
                budget.charge(1)?;
                if !instructions::supported(&instruction.op){return Err(Reason::Unsupported);}
                if let b::Op::Call {function,..}|b::Op::TaskSpawn {function,..}=instruction.op
                    && !seen[function.0 as usize] { seen[function.0 as usize]=true;reachable.push(function.0 as usize); }
            }
        }
        let layouts=l::Layouts::compile_tasks(proof,limits.preparation)?;
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
        let machine =
            machine::Machine::new(proof, &plans, allocator, control, limits, budget, entry)?;
        Ok(machine.run())
    })();
    match result {
        Ok(result) => result,
        Err(reason) => Err(fail(reason)),
    }
}
