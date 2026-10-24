//! Component value transport and resource retirement for the shared engines.
//! This service marshals values and invokes registered native cleanup; it never
//! evaluates guest instructions or creates a replacement invocation allocator.
use super::{
    native::ResourceKind,
    program::Program,
    storage::{self, Claim, Holder, Memory, Resources, Transfer},
    wire,
};
use crate::{
    cleanup as c, layout as l,
    task::runner::{EntryArgument, Reason},
};
use lang_mir::unit::{self as m, ownership as o};
mod call;
mod instance;
mod native_result;
mod result;
pub(crate) use call::ReadRoot;
pub use call::{
    NativeArguments, NativeContext, NativeFailure, NativeResourceFailure, NativeResult,
    NativeReturn, ResourceArgument, StorageOperation,
};
use instance::Phase;
pub use instance::{Completion, Instance, InstanceError, InstanceState};

type Normalize<'program, 'storage> =
    fn(ResourceKind<'program>) -> Result<ResourceKind<'storage>, Reason>;
pub(crate) use result::read_parameters;
pub(crate) use result::read_result;

/// An already issued caller namespace. Locators never replace claim authority.
#[derive(Clone, Copy, Debug)]
pub struct ResourceNamespace {
    issued: u32,
    maximum: u32,
}
impl ResourceNamespace {
    pub const fn new(issued: u32, maximum: u32) -> Self {
        Self { issued, maximum }
    }
    fn check(&self, ordinal: u64) -> Result<(), Reason> {
        if ordinal > u64::from(self.maximum) {
            Err(Reason::ComponentIndices {
                limit: self.maximum.into(),
                attempted: ordinal,
            })
        } else {
            Ok(())
        }
    }
    fn result(&self, count: usize) -> Result<u32, Reason> {
        if count == 0 {
            return Ok(0);
        }
        self.check(u64::from(self.issued).saturating_add(count as u64))?;
        Ok(self.issued)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Copy(l::Immediate),
    Resource(Claim),
    Frame(FrameResult),
}

/// Canonical caller representation. Releasing these bytes does not release the
/// independently committed resource claims; their originating table owns them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameResult {
    pub(crate) values: wire::Values,
    pub(crate) resources: Vec<Claim>,
    pub(crate) bytes: Vec<u8>,
}
impl FrameResult {
    pub fn values(&self) -> &wire::Values {
        &self.values
    }
    pub fn resources(&self) -> &[Claim] {
        &self.resources
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A call-local locator into an admitted immutable value arena. Copying it
/// grants no resource authority, and another call or replacement arena refuses it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameArgument {
    holder: Holder,
    generation: u64,
    root: wire::ValueId,
}
impl FrameArgument {
    pub(crate) const fn root(self) -> wire::ValueId {
        self.root
    }
}
struct InputFrame {
    generation: u64,
    values: wire::Values,
    claims: Vec<storage::Input>,
    nodes: Vec<usize>,
    requires_drain: bool,
}

/// Trusted native adapter. It must invoke the actual retained descriptor under
/// the original live invocation's context. No fabricated or null engine token
/// may be supplied. Mandatory cleanup remains unmetered.
pub trait Invoker {
    fn reentry_supported(&self) -> bool {
        false
    }
    fn control(&self) -> crate::execution::Control<'_>;
    fn accounting(&self) -> Option<&crate::execution::InvocationAccounting> {
        None
    }
    fn control_matches(&self, control: crate::execution::Control<'_>) -> bool;
    fn service_work_limit(&self) -> Option<u64>;
    fn check_control(&self) -> Result<(), crate::execution::Failure>;
    fn invoke(
        &mut self,
        method: &super::native::Method,
        parameters: &[u8],
        context: &mut dyn NativeContext,
    ) -> Result<NativeReturn, u32>;
    fn destroy(
        &mut self,
        kind: ResourceKind<'_>,
        backing: &mut storage::Detached<'_>,
        memory: &mut dyn Memory,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32>;
}

/// Private data/cleanup service used by both existing backends. Guest bodies
/// remain in their interpreter/generated-code dispatchers.
pub trait Host {
    fn reentry_supported(&self) -> bool;
    fn accounting(&self) -> &crate::execution::InvocationAccounting;
    fn service_work(&self) -> &crate::execution::ServiceWork;
    fn work_failure(&self) -> Option<crate::execution::Failure>;
    fn complete(&mut self, completion: Completion, memory: &dyn Memory) -> Result<(), Reason>;
    fn read_native_arguments(
        &mut self,
        read: &mut dyn FnMut(&mut wire::Session) -> Result<NativeArguments, Reason>,
    ) -> Result<NativeArguments, Reason>;
    fn invoke_native(
        &mut self,
        import: m::ImportId,
        arguments: &mut NativeArguments,
        span: m::Span,
        memory: &mut dyn Memory,
        budget: &mut m::Budget,
        parent_live: crate::execution::LiveUsage,
    ) -> Result<NativeResult, Reason>;
    fn commit_native_result(
        &mut self,
        result: &mut NativeResult,
        memory: &mut dyn Memory,
    ) -> Result<(), Reason>;
    fn discard_native_result(
        &mut self,
        result: &mut NativeResult,
        cleanup: &mut c::Session,
        span: m::Span,
        memory: &mut dyn Memory,
    ) -> Result<(), Reason>;
    fn indirect_input(&self) -> bool;
    fn result_session(&mut self) -> Result<&mut wire::Session, Reason>;
    fn abandon_result(
        &mut self,
        memory: &mut dyn Memory,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32>;
    fn frame(&self, argument: FrameArgument) -> Result<&wire::Values, Reason>;
    fn control_matches(&self, control: crate::execution::Control<'_>) -> bool;
    fn corresponds(&self, proof: &o::tasks::VerifiedTasks, function: m::FunctionId) -> bool;
    fn validate(&self, arguments: &[EntryArgument], memory: &dyn Memory) -> Result<(), Reason>;
    fn prepare_arguments(
        &mut self,
        arguments: &[EntryArgument],
        memory: &dyn Memory,
    ) -> Result<(), Reason>;
    fn argument(&self, index: usize) -> Result<Claim, Reason>;
    fn check_argument_loan(
        &self,
        index: usize,
        claim: Claim,
        memory: &dyn Memory,
    ) -> Result<(), Reason>;
    fn end_argument_loan(&mut self, index: usize) -> Result<(), Reason>;
    fn commit_arguments(&mut self, memory: &mut dyn Memory) -> Result<(), Reason>;
    fn abort_arguments(&mut self);
    fn retire(
        &mut self,
        token: &mut l::Token,
        memory: &mut dyn Memory,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32>;
    fn prepare_result(&mut self, value: Value, memory: &dyn Memory) -> Result<Value, Reason>;
    fn commit_result(&mut self, memory: &mut dyn Memory) -> Result<(), Reason>;
    fn arguments_committed(&self) -> bool;
}

pub struct Invocation<'state, 'p, 'u, 'resources, 'code, 'native> {
    instance: &'state mut Instance<'p, 'u>,
    finished: bool,
    program: &'p Program<'u>,
    function: m::FunctionId,
    resources: &'resources mut Resources<'code>,
    invoker: &'native mut dyn Invoker,
    normalize: Normalize<'u, 'code>,
    work: crate::execution::ServiceWork,
    accounting: crate::execution::InvocationAccounting,
    caller: Holder,
    guest: Holder,
    inputs: Vec<storage::Input>,
    indices: Vec<usize>,
    loans: Vec<Option<storage::Loan>>,
    transfer: Option<Transfer>,
    committed: bool,
    input_frame: Option<InputFrame>,
    next_frame: Option<u64>,
    output_session: Option<wire::WorkScope<'native>>,
    resource_namespace: Option<ResourceNamespace>,
    output_claims: Vec<(Claim, Option<m::Span>)>,
    holding_output: bool,
}
fn fault(error: storage::Error) -> Reason {
    Reason::Component(error)
}
impl<'state, 'p, 'u, 'resources, 'code, 'native>
    Invocation<'state, 'p, 'u, 'resources, 'code, 'native>
{
    pub fn prepare(
        instance: &'state mut Instance<'p, 'u>,
        function: m::FunctionId,
        resources: &'resources mut Resources<'code>,
        invoker: &'native mut dyn Invoker,
        budget: &mut m::Budget,
    ) -> Result<Self, Reason>
    where
        'u: 'code,
    {
        Self::prepare_inner(instance, function, resources, invoker, budget, |kind| {
            Ok(kind)
        })
    }
    fn prepare_inner(
        instance: &'state mut Instance<'p, 'u>,
        function: m::FunctionId,
        resources: &'resources mut Resources<'code>,
        invoker: &'native mut dyn Invoker,
        budget: &mut m::Budget,
        normalize: Normalize<'u, 'code>,
    ) -> Result<Self, Reason> {
        instance.ready()?;
        let program = instance.program;
        if !program.is_export(function) {
            return Err(Reason::Entry);
        }
        let caller = instance.caller.unwrap_or_else(|| resources.host());
        resources.check_holder(caller).map_err(fault)?;
        let inputs = budget.vector(resources.input_capacity())?;
        let indices = budget.vector(resources.input_capacity())?;
        let loans = budget.vector(resources.input_capacity())?;
        let output_claims = budget.vector(resources.capacity())?;
        let accounting = match invoker.accounting() {
            Some(accounting) if accounting.matches(invoker.control()) => accounting.clone(),
            Some(_) => return Err(Reason::Entry),
            None => crate::execution::InvocationAccounting::prepare(invoker.control(), budget)?,
        };
        let work = accounting.work().clone();
        let guest = resources.new_holder().map_err(fault)?;
        instance.phase = Phase::Active;
        Ok(Self {
            instance,
            finished: false,
            program,
            function,
            resources,
            invoker,
            normalize,
            work,
            accounting,
            caller,
            guest,
            inputs,
            indices,
            loans,
            transfer: None,
            committed: false,
            input_frame: None,
            next_frame: Some(1),
            output_session: None,
            resource_namespace: None,
            output_claims,
            holding_output: false,
        })
    }
    fn check_work(&self) -> Result<(), Reason> {
        self.accounting
            .preparation_failure()
            .map_or(Ok(()), |failure| Err(Reason::Control(failure)))
    }
    fn latch_work<T>(&self, result: Result<T, Reason>) -> Result<T, Reason> {
        if self.accounting.preparation_failure().is_none()
            && let Err(Reason::Control(
                failure @ (crate::execution::Failure::ServiceWorkLimit { .. }
                | crate::execution::Failure::CounterOverflow),
            )) = &result
        {
            self.accounting.latch_preparation_failure(*failure);
        }
        result
    }
    fn key(&self, ty: o::Type) -> Result<&super::negotiation::ResourceTypeKey, Reason> {
        let types = self.program.proof().typed().places().declarations().types();
        let o::Type::Constructed(id) = ty else {
            return Err(Reason::Entry);
        };
        let o::Constructor::ComponentResource(tag) =
            types.table().constructed[id.0 as usize].constructor
        else {
            return Err(Reason::Entry);
        };
        let r = self
            .program
            .bindings()
            .unit()
            .resource(tag)
            .ok_or(Reason::Entry)?;
        Ok(
            &self.program.bindings().contract().document().interfaces[r.interface].resources
                [r.resource]
                .value,
        )
    }
    fn declaration(&self) -> &o::declarations::Function {
        &self
            .program
            .proof()
            .typed()
            .places()
            .declarations()
            .functions()[self.function.0 as usize]
    }

    /// Select an external locator namespace before any argument/result staging.
    /// Ordinary Rust entries retain their existing call-local indices.
    pub fn resource_namespace(&mut self, namespace: ResourceNamespace) -> Result<(), Reason> {
        if self.committed
            || self.input_frame.is_some()
            || self.output_session.is_some()
            || self.resource_namespace.is_some()
        {
            return Err(Reason::Entry);
        }
        self.resource_namespace = Some(namespace);
        Ok(())
    }

    /// Reuse the caller's cumulative codec budget for rich result publication.
    pub fn prepare_output(&mut self, session: &'native mut wire::Session) -> Result<(), Reason> {
        if self.committed || self.transfer.is_some() || self.output_session.is_some() {
            return Err(Reason::Entry);
        }
        self.output_session = Some(session.scope_work(&self.work).map_err(Reason::from)?);
        Ok(())
    }
    /// Dynamic argument loans remaining in this invocation's original table.
    pub fn active_loans(&self) -> usize {
        self.loans.iter().filter(|loan| loan.is_some()).count()
    }
    /// Service work already charged during this root's canonical preparation.
    /// Reading it does not create a counter or reset a failed admission latch.
    pub fn service_work_used(&self) -> u64 {
        self.work.used()
    }
    fn abort_transfer(&mut self) {
        self.holding_output = false;
        if let Some(ticket) = self.transfer.take()
            && self.resources.abort_transfer(ticket).is_err()
        {
            self.instance.phase = Phase::Poisoned;
        }
    }

    fn retire_claim(
        &mut self,
        claim: Claim,
        span: Option<m::Span>,
        memory: &mut dyn Memory,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        self.retire_from_holder(claim, self.guest, span, memory, context)
    }
    fn retire_from_holder(
        &mut self,
        claim: Claim,
        holder: Holder,
        span: Option<m::Span>,
        memory: &mut dyn Memory,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let mut backing = self
            .resources
            .detach(claim, holder, memory)
            .map_err(|_| 17u32)?;
        let kind = backing.kind().map_err(|_| 17u32)?;
        let mut cleanup =
            c::Contextual::new(|_: &mut (), _: c::Reason, context: &mut c::Context<'_>| {
                self.invoker
                    .destroy(kind.clone(), &mut backing, memory, context)
            });
        context.field((), span, &mut cleanup);
        backing.release(memory).map_err(|_| 17u32)
    }

    fn frame_signature(&self, session: &mut wire::Session) -> Result<wire::Signature<'_>, Reason> {
        let export = self
            .program
            .bindings()
            .unit()
            .mappings()
            .exports
            .iter()
            .find(|export| export.source == self.function)
            .ok_or(Reason::Entry)?;
        wire::Signature::new(
            self.program.bindings().contract(),
            &export.member.interface,
            &export.member.member,
            session,
        )
        .map_err(Reason::from)
    }

    fn checked_resources(
        &self,
        values: &wire::Values,
        uses: &[wire::ResourceUse<'_>],
        resources: &[Claim],
        memory: &dyn Memory,
        session: &mut wire::Session,
    ) -> Result<(Vec<storage::Input>, Vec<usize>), Reason> {
        if self.committed
            || self.transfer.is_some()
            || values.roots.len() != self.declaration().parameters.len()
        {
            return Err(Reason::Entry);
        }
        if uses.len() > self.resources.input_capacity() {
            return Err(fault(storage::Error::Capacity));
        }
        session
            .charge(
                uses.len()
                    .saturating_mul(uses.len())
                    .saturating_mul(9)
                    .saturating_add(values.roots.len()),
                0,
            )
            .map_err(Reason::from)?;
        let mut claims = session.vector(uses.len(), 0).map_err(Reason::from)?;
        let mut nodes = session.vector(uses.len(), 0).map_err(Reason::from)?;
        for used in uses {
            if let Some(namespace) = self.resource_namespace {
                namespace.check(used.index.get().into())?;
            }
            let access = match used.access {
                wire::ResourceAccess::Own => storage::Access::Own,
                wire::ResourceAccess::Borrow(super::idl::Access::Shared) => storage::Access::Shared,
                wire::ResourceAccess::Borrow(super::idl::Access::Exclusive) => {
                    storage::Access::Exclusive
                }
            };
            let claim = *resources.get((used.index.get() - 1) as usize).ok_or({
                Reason::ComponentFrame(wire::Error {
                    offset: used.offset,
                    kind: wire::ErrorKind::ValueType,
                })
            })?;
            self.resources
                .validate(claim, self.caller, used.key, memory)
                .map_err(fault)?;
            claims.push(storage::Input { claim, access });
            nodes.push(used.value.0);
        }
        self.resources
            .check_call(claims.iter().copied(), self.caller, self.guest, memory)
            .map_err(fault)?;
        Ok((claims, nodes))
    }

    /// Encode claims in this independently bound call's index namespace.
    /// Structural bytes and immutable locators grant no ownership authority.
    pub fn encode_arguments(
        &self,
        values: &wire::Values,
        resources: &[Claim],
        memory: &dyn Memory,
        session: &mut wire::Session,
    ) -> Result<Vec<u8>, Reason> {
        self.check_work()?;
        self.latch_work(self.encode_arguments_inner(values, resources, memory, session))
    }
    fn encode_arguments_inner(
        &self,
        values: &wire::Values,
        resources: &[Claim],
        memory: &dyn Memory,
        session: &mut wire::Session,
    ) -> Result<Vec<u8>, Reason> {
        let mut scope = session.scope_work(&self.work).map_err(Reason::from)?;
        let session = &mut *scope;
        let signature = self.frame_signature(session)?;
        let (bytes, uses) = wire::encode_contextual_values(
            &signature,
            wire::FrameKind::Parameters,
            values,
            session,
        )
        .map_err(Reason::from)?;
        self.checked_resources(values, &uses, resources, memory, session)?;
        Ok(bytes)
    }

    /// Publish an immutable, call-branded input arena only after complete
    /// structural/resource admission. Original owners remain unchanged.
    pub fn decode_arguments(
        &mut self,
        bytes: &[u8],
        resources: &[Claim],
        memory: &dyn Memory,
        session: &mut wire::Session,
    ) -> Result<Vec<EntryArgument>, Reason> {
        self.check_work()?;
        let result = self.decode_arguments_inner(bytes, resources, memory, session);
        self.latch_work(result)
    }
    fn decode_arguments_inner(
        &mut self,
        bytes: &[u8],
        resources: &[Claim],
        memory: &dyn Memory,
        session: &mut wire::Session,
    ) -> Result<Vec<EntryArgument>, Reason> {
        let mut scope = session.scope_work(&self.work).map_err(Reason::from)?;
        let session = &mut *scope;
        let signature = self.frame_signature(session)?;
        let (values, uses) =
            wire::decode_contextual_values(&signature, wire::FrameKind::Parameters, bytes, session)
                .map_err(Reason::from)?;
        let (claims, nodes) = self.checked_resources(&values, &uses, resources, memory, session)?;
        let generation = self
            .next_frame
            .ok_or_else(|| fault(storage::Error::Exhausted))?;
        let mut arguments = session
            .vector(values.roots.len(), 0)
            .map_err(Reason::from)?;
        for root in &values.roots {
            arguments.push(EntryArgument::Frame(FrameArgument {
                holder: self.guest,
                generation,
                root: *root,
            }));
        }
        self.next_frame = generation.checked_add(1);
        let requires_drain = values
            .nodes
            .iter()
            .any(|value| matches!(value, wire::Value::List(_)));
        self.input_frame = Some(InputFrame {
            generation,
            values,
            claims,
            nodes,
            requires_drain,
        });
        Ok(arguments)
    }
}
impl<'state, 'p, 'u, 'resources, 'code, 'native> Host
    for Invocation<'state, 'p, 'u, 'resources, 'code, 'native>
{
    fn reentry_supported(&self) -> bool {
        self.invoker.reentry_supported()
    }
    fn accounting(&self) -> &crate::execution::InvocationAccounting {
        &self.accounting
    }
    fn complete(&mut self, completion: Completion, memory: &dyn Memory) -> Result<(), Reason> {
        self.finished = true;
        let closed = self.transfer.is_none()
            && self.loans.iter().all(Option::is_none)
            && !self.holding_output
            && self.output_claims.is_empty()
            && self
                .resources
                .scope_closed(self.caller, self.guest, &self.inputs, None, memory)
                .is_ok();
        if matches!(completion, Completion::Poisoned) || !closed {
            self.instance.phase = Phase::Poisoned;
        }
        if closed {
            Ok(())
        } else {
            Err(Reason::Instance(InstanceError::ScopeNotClosed))
        }
    }
    fn read_native_arguments(
        &mut self,
        read: &mut dyn FnMut(&mut wire::Session) -> Result<NativeArguments, Reason>,
    ) -> Result<NativeArguments, Reason> {
        read(
            self.output_session
                .as_deref_mut()
                .ok_or(Reason::Unsupported)?,
        )
    }
    fn invoke_native(
        &mut self,
        import: m::ImportId,
        arguments: &mut NativeArguments,
        span: m::Span,
        memory: &mut dyn Memory,
        budget: &mut m::Budget,
        parent_live: crate::execution::LiveUsage,
    ) -> Result<NativeResult, Reason> {
        let mut session_scope = self.output_session.take().ok_or(Reason::Unsupported)?;
        let session = &mut *session_scope;
        let mut created = Vec::new();
        let mut owned = Vec::new();
        let mut native_holder = None;
        let mut result = (|| -> Result<NativeResult, Reason> {
            let bindings = self.program.bindings();
            let target = bindings.resolve(import).ok_or(Reason::Entry)?;
            if target.method().flags & !1 != 0
                || (target.method().flags != 0 && !self.invoker.reentry_supported())
            {
                return Err(Reason::Unsupported);
            }
            let function = *bindings
                .unit()
                .imports()
                .get(import.0 as usize)
                .ok_or(Reason::Entry)?;
            let document = bindings.contract().document();
            let interface = &document.interfaces[function.interface];
            let member = &interface.functions[function.member];
            let signature = wire::Signature::new(
                bindings.contract(),
                &interface.key.value,
                &member.name.value,
                session,
            )
            .map_err(Reason::from)?;
            let (bytes, uses) = wire::encode_contextual_values(
                &signature,
                wire::FrameKind::Parameters,
                &arguments.values,
                session,
            )
            .map_err(Reason::from)?;
            if uses.len() != arguments.resources.len() {
                return Err(Reason::Entry);
            }
            if let Some(namespace) = self.resource_namespace {
                namespace.check(uses.len() as u64)?;
            }
            session
                .charge(uses.len().saturating_mul(uses.len()).saturating_mul(9), 0)
                .map_err(Reason::from)?;
            let mut loans = session.vector(uses.len(), 0).map_err(Reason::from)?;
            loans.resize(uses.len(), None);
            let mut kinds = session.vector(uses.len(), 0).map_err(Reason::from)?;
            let mut accesses = session.vector(uses.len(), 0).map_err(Reason::from)?;
            let mut inputs = session.vector(uses.len(), 0).map_err(Reason::from)?;
            let mut positions = session.vector(uses.len(), 0).map_err(Reason::from)?;
            let mut seen = session.vector(uses.len(), 0).map_err(Reason::from)?;
            let native_capacity = uses
                .len()
                .checked_add(self.resources.capacity())
                .ok_or(Reason::Invariant)?;
            let mut views = session.vector(native_capacity, 0).map_err(Reason::from)?;
            let mut accepted = session.vector(native_capacity, 0).map_err(Reason::from)?;
            arguments.consumed = session.vector(uses.len(), 0).map_err(Reason::from)?;
            {
                arguments.cleanup = Some(c::Session::prepare(
                    std::num::NonZeroUsize::new(native_capacity.saturating_add(1))
                        .ok_or(Reason::Invariant)?,
                    budget,
                )?);
            }
            for (position, used) in uses.iter().enumerate() {
                let access = match used.access {
                    wire::ResourceAccess::Own => super::idl::Access::Exclusive,
                    wire::ResourceAccess::Borrow(access) => access,
                };
                let argument = *arguments
                    .resources
                    .get((used.index.get() - 1) as usize)
                    .ok_or(Reason::Entry)?;
                if seen
                    .iter()
                    .any(|(prior, mode): &(Claim, super::idl::Access)| {
                        prior.same_owner(argument.claim)
                            && (*mode != super::idl::Access::Shared
                                || access != super::idl::Access::Shared)
                    })
                {
                    return Err(fault(storage::Error::AliasedOwner));
                }
                seen.push((argument.claim, access));
                let kind = target
                    .provider()
                    .resource_kind(used.key, session)
                    .map_err(|_| Reason::Entry)?;
                let kind = (self.normalize)(kind)?;
                if let Some(parent) = argument.parent {
                    if matches!(used.access, wire::ResourceAccess::Own) {
                        return Err(Reason::Entry);
                    }
                    let index = self
                        .indices
                        .iter()
                        .position(|i| *i == parent)
                        .ok_or(Reason::Entry)?;
                    let loan = self
                        .loans
                        .get(index)
                        .copied()
                        .flatten()
                        .ok_or(Reason::Entry)?;
                    self.resources
                        .check_loan(loan, argument.claim, self.guest, memory)
                        .map_err(fault)?;
                    self.resources
                        .check_view(loan, kind.clone(), access)
                        .map_err(fault)?;
                    loans[position] = Some(loan);
                } else {
                    self.resources
                        .validate(argument.claim, self.guest, used.key, memory)
                        .map_err(fault)?;
                    // Implementation matching precedes callback/view publication.
                    self.resources
                        .bytes(argument.claim, self.guest, kind.clone(), memory)
                        .map_err(fault)?;
                    inputs.push(storage::Input {
                        claim: argument.claim,
                        access: match used.access {
                            wire::ResourceAccess::Own => storage::Access::Own,
                            wire::ResourceAccess::Borrow(super::idl::Access::Shared) => {
                                storage::Access::Shared
                            }
                            wire::ResourceAccess::Borrow(super::idl::Access::Exclusive) => {
                                storage::Access::Exclusive
                            }
                        },
                    });
                    positions.push(position);
                }
                kinds.push(kind);
                accesses.push(access);
            }
            let native = self.resources.new_holder().map_err(fault)?;
            native_holder = Some(native);
            if !inputs.is_empty() {
                created = session.vector(inputs.len(), 0).map_err(Reason::from)?;
                created.resize(inputs.len(), None);
                let transfer = self
                    .resources
                    .prepare_call(inputs.iter().copied(), self.guest, native, memory)
                    .map_err(fault)?;
                for (index, position) in positions.iter().enumerate() {
                    if matches!(uses[*position].access, wire::ResourceAccess::Own) {
                        match self.resources.prepared_claim(transfer, index) {
                            Ok(claim) => accepted.push((claim, *position)),
                            Err(error) => {
                                let _ = self.resources.abort_transfer(transfer);
                                return Err(fault(error));
                            }
                        }
                    }
                }
                if let Err(error) = self.resources.commit_call(transfer, memory, &mut created) {
                    let _ = self.resources.abort_transfer(transfer);
                    return Err(fault(error));
                }
                for (index, position) in positions.iter().enumerate() {
                    loans[*position] = created[index];
                }
                for (_, position) in &accepted {
                    let used = &uses[*position];
                    arguments
                        .consumed
                        .push(arguments.resources[(used.index.get() - 1) as usize].claim);
                }
            }
            owned = accepted;
            for index in 0..uses.len() {
                let (authority, bytes) = if matches!(uses[index].access, wire::ResourceAccess::Own)
                {
                    let claim = owned
                        .iter()
                        .find(|(_, position)| *position == index)
                        .map(|(claim, _)| *claim)
                        .ok_or(Reason::Invariant)?;
                    let holder = native_holder.ok_or(Reason::Invariant)?;
                    (
                        call::NativeAuthority::Own(claim, holder),
                        self.resources
                            .owned_view(claim, holder, kinds[index].clone(), memory)
                            .map_err(fault)?,
                    )
                } else {
                    let loan = loans[index].ok_or(Reason::Invariant)?;
                    (
                        call::NativeAuthority::Loan(loan),
                        self.resources
                            .view(loan, kinds[index].clone(), accesses[index], memory)
                            .map_err(fault)?,
                    )
                };
                views.push(call::NativeView {
                    authority,
                    kind: kinds[index].clone(),
                    access: accesses[index],
                    bytes,
                });
            }
            let normalize = self.normalize;
            let kind = |key: &[u8], budget: &mut m::Budget| {
                let kind = target.provider().resource_kind_bytes(key, budget)?;
                normalize(kind).map_err(|_| 7u32)
            };
            let mut context = call::CallContext::new(
                self.resources,
                memory,
                budget,
                &mut views,
                &mut owned,
                native,
                &kind,
            )
            .with_index_limit(
                self.resource_namespace
                    .map_or(u32::MAX, |namespace| namespace.maximum),
            )
            .with_accounting(&self.accounting)
            .with_parent_live(parent_live);
            let returned = self.invoker.invoke(target.method(), &bytes, &mut context);
            let decoded = (|| -> Result<NativeResult, Reason> {
                self.invoker.check_control().map_err(Reason::Control)?;
                let returned =
                    returned.map_err(|status| Reason::Native(NativeFailure::Invocation(status)))?;
                if returned.status > 1 {
                    return Err(Reason::Native(NativeFailure::Invocation(returned.status)));
                }
                if returned.status != 0 {
                    let bytes = if returned.buffer.is_null() {
                        Vec::new()
                    } else {
                        context.result(returned.buffer, session)?
                    };
                    return Err(Reason::Native(NativeFailure::Status {
                        code: returned.status,
                        bytes,
                    }));
                }
                if returned.buffer.is_null() {
                    return Err(Reason::Native(NativeFailure::MissingResult));
                }
                let bytes = context.result(returned.buffer, session)?;
                let (values, resources) = wire::decode_contextual_values(
                    &signature,
                    wire::FrameKind::Result,
                    &bytes,
                    session,
                )
                .map_err(|error| {
                    if matches!(
                        error.kind,
                        wire::ErrorKind::Limit { .. }
                            | wire::ErrorKind::Budget(_)
                            | wire::ErrorKind::Execution(_)
                            | wire::ErrorKind::WorkContext
                    ) {
                        Reason::from(error)
                    } else {
                        Reason::Native(NativeFailure::InvalidResult(error))
                    }
                })?;
                let mut nodes = session.vector(resources.len(), 0).map_err(Reason::from)?;
                let mut claims = session.vector(resources.len(), 0).map_err(Reason::from)?;
                let mut originals = session.vector(resources.len(), 0).map_err(Reason::from)?;
                for used in resources {
                    let failure = |kind| {
                        Reason::Native(NativeFailure::Resource {
                            index: used.index.get(),
                            kind,
                        })
                    };
                    let view = context
                        .views
                        .get((used.index.get() - 1) as usize)
                        .ok_or_else(|| failure(NativeResourceFailure::UnknownIndex))?;
                    let call::NativeAuthority::Own(claim, _) = view.authority else {
                        return Err(failure(NativeResourceFailure::NotOwned));
                    };
                    if used.access != wire::ResourceAccess::Own || view.kind.key() != used.key {
                        return Err(failure(NativeResourceFailure::WrongKind));
                    }
                    session
                        .charge(originals.len().saturating_add(1), 0)
                        .map_err(Reason::from)?;
                    if originals.contains(&claim) {
                        return Err(failure(NativeResourceFailure::Aliased));
                    }
                    nodes.push(used.value.0);
                    claims.push(claim);
                    originals.push(claim);
                }
                Ok(NativeResult {
                    values,
                    nodes,
                    claims,
                    originals,
                    holder: native_holder,
                    transfer: None,
                    committed: false,
                })
            })();
            let closed = context.close();
            match decoded {
                Err(error) => Err(error),
                Ok(values) => {
                    closed?;
                    Ok(values)
                }
            }
        })();
        let mut ending = None;
        for loan in created.into_iter().rev().flatten() {
            if let Err(error) = self.resources.end_loan(loan) {
                ending.get_or_insert(error);
            }
        }
        let mut cleanup_failed = false;
        for (claim, position) in owned.into_iter().rev() {
            if result
                .as_ref()
                .is_ok_and(|value| value.originals.contains(&claim))
            {
                continue;
            }
            let holder = native_holder.ok_or(Reason::Invariant)?;
            let cleanup = arguments.cleanup.as_mut().ok_or(Reason::Invariant)?;
            let mut cleaner = c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
                let mut backing = self
                    .resources
                    .detach(claim, holder, memory)
                    .map_err(|_| 17u32)?;
                let kind = backing.kind().map_err(|_| 17u32)?;
                let mut destroy =
                    c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
                        self.invoker
                            .destroy(kind.clone(), &mut backing, memory, context)
                    });
                context.field((), Some(span), &mut destroy);
                backing.release(memory).map_err(|_| 17u32)
            });
            cleanup_failed |= cleanup
                .retire(
                    c::Slot(position),
                    (),
                    Some(span),
                    if result.is_err() {
                        c::Reason::Failure
                    } else {
                        c::Reason::ScopeExit
                    },
                    &mut cleaner,
                )
                .is_err();
        }
        if result.is_ok() && (cleanup_failed || ending.is_some()) {
            if let Ok(value) = &mut result {
                let _ = self.abort_native_result(
                    value,
                    arguments.cleanup.as_mut().ok_or(Reason::Invariant)?,
                    span,
                    memory,
                );
            }
            result = Err(if cleanup_failed {
                Reason::Cleanup
            } else {
                fault(ending.expect("observed ending failure"))
            });
        }
        if let Ok(value) = &mut result
            && let Err(error) = self.prepare_native_result(value, memory)
        {
            let _ = self.abort_native_result(
                value,
                arguments.cleanup.as_mut().ok_or(Reason::Invariant)?,
                span,
                memory,
            );
            result = Err(error);
        }
        self.output_session = Some(session_scope);
        result
    }
    fn commit_native_result(
        &mut self,
        result: &mut NativeResult,
        memory: &mut dyn Memory,
    ) -> Result<(), Reason> {
        if let Some(ticket) = result.transfer {
            self.resources
                .commit_transfer(ticket, memory)
                .map_err(fault)?;
            result.transfer = None;
        }
        result.committed = true;
        result.originals.clear();
        Ok(())
    }
    fn discard_native_result(
        &mut self,
        result: &mut NativeResult,
        cleanup: &mut c::Session,
        span: m::Span,
        memory: &mut dyn Memory,
    ) -> Result<(), Reason> {
        self.abort_native_result(result, cleanup, span, memory)
    }
    fn indirect_input(&self) -> bool {
        self.input_frame
            .as_ref()
            .is_some_and(|frame| frame.requires_drain)
    }
    fn result_session(&mut self) -> Result<&mut wire::Session, Reason> {
        self.output_session
            .as_deref_mut()
            .ok_or(Reason::Unsupported)
    }
    fn abandon_result(
        &mut self,
        memory: &mut dyn Memory,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        self.abort_arguments();
        while let Some((claim, span)) = self.output_claims.pop() {
            let mut cleanup =
                c::Contextual::new(|claim: &mut Claim, _, context: &mut c::Context<'_>| {
                    self.retire_claim(*claim, span, memory, context)
                });
            context.field(claim, span, &mut cleanup);
        }
        Ok(())
    }
    fn frame(&self, argument: FrameArgument) -> Result<&wire::Values, Reason> {
        let frame = self.input_frame.as_ref().ok_or(Reason::Entry)?;
        if argument.holder != self.guest
            || argument.generation != frame.generation
            || frame.values.nodes.get(argument.root.0).is_none()
        {
            return Err(Reason::Entry);
        }
        Ok(&frame.values)
    }
    fn control_matches(&self, control: crate::execution::Control<'_>) -> bool {
        self.invoker.control_matches(control)
    }
    fn service_work(&self) -> &crate::execution::ServiceWork {
        &self.work
    }
    fn work_failure(&self) -> Option<crate::execution::Failure> {
        self.accounting.preparation_failure()
    }
    fn corresponds(&self, proof: &o::tasks::VerifiedTasks, function: m::FunctionId) -> bool {
        std::ptr::eq(self.program.proof(), proof) && self.function == function
    }
    fn validate(&self, arguments: &[EntryArgument], memory: &dyn Memory) -> Result<(), Reason> {
        let decl = self.declaration();
        if self.committed || self.transfer.is_some() || arguments.len() != decl.parameters.len() {
            return Err(Reason::Entry);
        }
        if self.program.native_calls() && self.output_session.is_none() {
            return Err(Reason::Unsupported);
        }
        let result = decl.result.ty;
        let needs_output =
            result != o::Type::I64 && result != o::Type::Bool && self.key(result).is_err();
        if arguments
            .iter()
            .any(|argument| matches!(argument, EntryArgument::Frame(_)))
        {
            let frame = self.input_frame.as_ref().ok_or(Reason::Entry)?;
            for (argument, root) in arguments.iter().zip(&frame.values.roots) {
                let EntryArgument::Frame(argument) = argument else {
                    return Err(Reason::Entry);
                };
                self.frame(*argument)?;
                if argument.root != *root {
                    return Err(Reason::Entry);
                }
            }
            if needs_output && self.output_session.is_none() {
                return Err(Reason::Unsupported);
            }
            return self
                .resources
                .check_call(
                    frame.claims.iter().copied(),
                    self.caller,
                    self.guest,
                    memory,
                )
                .map_err(fault);
        }
        for (argument, local) in arguments.iter().zip(&decl.parameters) {
            let ty = decl.locals[local.0 as usize].ty.ty;
            match argument {
                EntryArgument::Resource(claim) => self
                    .resources
                    .validate(*claim, self.caller, self.key(ty)?, memory)
                    .map_err(fault)?,
                argument if argument.ty() == Some(ty) => {}
                _ => return Err(Reason::Entry),
            }
        }
        if needs_output && self.output_session.is_none() {
            return Err(Reason::Unsupported);
        }
        self.resources
            .check_transfer(
                arguments.iter().filter_map(|argument| match argument {
                    EntryArgument::Resource(claim) => Some(*claim),
                    _ => None,
                }),
                self.caller,
                self.guest,
                memory,
            )
            .map_err(fault)?;
        Ok(())
    }
    fn prepare_arguments(
        &mut self,
        arguments: &[EntryArgument],
        memory: &dyn Memory,
    ) -> Result<(), Reason> {
        self.validate(arguments, memory)?;
        self.inputs.clear();
        self.indices.clear();
        if arguments
            .iter()
            .any(|argument| matches!(argument, EntryArgument::Frame(_)))
        {
            let frame = self.input_frame.as_ref().ok_or(Reason::Entry)?;
            self.inputs.extend_from_slice(&frame.claims);
            self.indices.extend_from_slice(&frame.nodes);
        } else {
            for (index, arg) in arguments.iter().enumerate() {
                if let EntryArgument::Resource(claim) = arg {
                    self.inputs.push(storage::Input {
                        claim: *claim,
                        access: storage::Access::Own,
                    });
                    self.indices.push(index);
                }
            }
        }
        self.loans.clear();
        self.loans.resize(self.inputs.len(), None);
        self.transfer = Some(
            self.resources
                .prepare_call(self.inputs.iter().copied(), self.caller, self.guest, memory)
                .map_err(fault)?,
        );
        Ok(())
    }
    fn argument(&self, index: usize) -> Result<Claim, Reason> {
        let position = self
            .indices
            .iter()
            .position(|i| *i == index)
            .ok_or(Reason::Entry)?;
        self.resources
            .prepared_claim(self.transfer.ok_or(Reason::Invariant)?, position)
            .map_err(fault)
    }
    fn commit_arguments(&mut self, memory: &mut dyn Memory) -> Result<(), Reason> {
        self.resources
            .commit_call(
                self.transfer.ok_or(Reason::Invariant)?,
                memory,
                &mut self.loans,
            )
            .map_err(fault)?;
        self.transfer = None;
        self.committed = true;
        Ok(())
    }
    fn check_argument_loan(
        &self,
        index: usize,
        claim: Claim,
        memory: &dyn Memory,
    ) -> Result<(), Reason> {
        if !self.committed {
            return Err(Reason::Entry);
        }
        let position = self
            .indices
            .iter()
            .position(|i| *i == index)
            .ok_or(Reason::Entry)?;
        let loan = self
            .loans
            .get(position)
            .copied()
            .flatten()
            .ok_or(Reason::Entry)?;
        self.resources
            .check_loan(loan, claim, self.guest, memory)
            .map_err(fault)
    }
    fn end_argument_loan(&mut self, index: usize) -> Result<(), Reason> {
        if !self.committed {
            return Ok(());
        }
        let position = self
            .indices
            .iter()
            .position(|i| *i == index)
            .ok_or(Reason::Entry)?;
        if let Some(loan) = self.loans.get_mut(position).ok_or(Reason::Entry)?.take() {
            self.resources.end_loan(loan).map_err(fault)?;
        }
        Ok(())
    }
    fn abort_arguments(&mut self) {
        self.abort_transfer();
    }
    fn arguments_committed(&self) -> bool {
        self.committed
    }
    fn retire(
        &mut self,
        token: &mut l::Token,
        memory: &mut dyn Memory,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let claim = Claim::from_token(token.bytes).map_err(|_| 17u32)?;
        if !self.committed {
            let transfer = self.transfer.ok_or(17u32)?;
            if !(0..self.inputs.len())
                .any(|index| self.resources.prepared_claim(transfer, index).ok() == Some(claim))
            {
                return Err(17u32);
            }
            token.bytes.fill(0);
            return Ok(());
        }
        if self.holding_output
            && let Some((_, span)) = self
                .output_claims
                .iter_mut()
                .find(|(held, _)| *held == claim)
        {
            *span = token.leaf.span;
            token.bytes.fill(0);
            return Ok(());
        }
        token.bytes.fill(0);
        self.retire_claim(claim, token.leaf.span, memory, context)
    }
    fn prepare_result(&mut self, mut value: Value, memory: &dyn Memory) -> Result<Value, Reason> {
        if self.transfer.is_some() || !self.committed {
            return Err(Reason::Invariant);
        }
        if self.output_session.is_some() && !matches!(value, Value::Frame(_)) {
            let session = self
                .output_session
                .as_deref_mut()
                .ok_or(Reason::Invariant)?;
            let mut nodes = session.vector(1, 0).map_err(Reason::from)?;
            let mut roots = session.vector(1, 0).map_err(Reason::from)?;
            let mut resources = session
                .vector(usize::from(matches!(value, Value::Resource(_))), 0)
                .map_err(Reason::from)?;
            nodes.push(match value {
                Value::Copy(l::Immediate::I64(value)) => wire::Value::I64(value),
                Value::Copy(l::Immediate::Bool(value)) => wire::Value::Bool(value),
                Value::Copy(l::Immediate::Unit) => wire::Value::Unit,
                Value::Resource(claim) => {
                    resources.push(claim);
                    wire::Value::Resource(
                        crate::component::adapter::BoundaryResourceIndex::new(1)
                            .ok_or(Reason::Invariant)?,
                    )
                }
                _ => return Err(Reason::Entry),
            });
            roots.push(wire::ValueId(0));
            value = Value::Frame(FrameResult {
                values: wire::Values { roots, nodes },
                resources,
                bytes: Vec::new(),
            });
        }
        match value {
            Value::Frame(mut frame) => {
                let mut session_scope = self.output_session.take().ok_or(Reason::Unsupported)?;
                let session = &mut *session_scope;
                let staged = (|| -> Result<_, Reason> {
                    let resource_base = self
                        .resource_namespace
                        .map(|n| n.result(frame.resources.len()))
                        .transpose()?
                        .unwrap_or(0);
                    if resource_base != 0 {
                        for node in &mut frame.values.nodes {
                            if let wire::Value::Resource(index) = node {
                                session.value_work(1, 0).map_err(Reason::from)?;
                                let raw = resource_base
                                    .checked_add(index.get())
                                    .ok_or(Reason::Entry)?;
                                *index = super::adapter::BoundaryResourceIndex::new(raw)
                                    .ok_or(Reason::Entry)?;
                            }
                        }
                    }
                    let signature = self.frame_signature(session)?;
                    let (bytes, uses) = wire::encode_contextual_values(
                        &signature,
                        wire::FrameKind::Result,
                        &frame.values,
                        session,
                    )
                    .map_err(Reason::from)?;
                    if uses.len() != frame.resources.len() || uses.len() > self.resources.capacity()
                    {
                        return Err(Reason::Entry);
                    }
                    session
                        .charge(uses.len().saturating_mul(uses.len()).saturating_mul(3), 0)
                        .map_err(Reason::from)?;
                    for used in uses {
                        if used.access != wire::ResourceAccess::Own {
                            return Err(Reason::Entry);
                        }
                        let claim = *frame
                            .resources
                            .get(
                                used.index
                                    .get()
                                    .checked_sub(resource_base)
                                    .and_then(|n| n.checked_sub(1))
                                    .ok_or(Reason::Entry)? as usize,
                            )
                            .ok_or(Reason::Entry)?;
                        self.resources
                            .validate(claim, self.guest, used.key, memory)
                            .map_err(fault)?;
                    }
                    self.resources
                        .check_transfer(
                            frame.resources.iter().copied(),
                            self.guest,
                            self.caller,
                            memory,
                        )
                        .map_err(fault)?;
                    let caller = session
                        .vector(frame.resources.len(), 0)
                        .map_err(Reason::from)?;
                    Ok((bytes, caller))
                })();
                self.output_session = Some(session_scope);
                let (bytes, mut caller) = staged?;
                let transfer = self
                    .resources
                    .prepare_transfer(&frame.resources, self.guest, self.caller, memory)
                    .map_err(fault)?;
                self.transfer = Some(transfer);
                self.output_claims.clear();
                for (index, claim) in frame.resources.iter().enumerate() {
                    self.output_claims.push((*claim, None));
                    caller.push(
                        self.resources
                            .prepared_claim(transfer, index)
                            .map_err(fault)?,
                    );
                }
                self.holding_output = true;
                frame.resources = caller;
                frame.bytes = bytes;
                Ok(Value::Frame(frame))
            }
            Value::Resource(claim) => {
                self.resources
                    .validate(
                        claim,
                        self.guest,
                        self.key(self.declaration().result.ty)?,
                        memory,
                    )
                    .map_err(fault)?;
                let transfer = self
                    .resources
                    .prepare_transfer(&[claim], self.guest, self.caller, memory)
                    .map_err(fault)?;
                let result = self.resources.prepared_claim(transfer, 0).map_err(fault)?;
                self.transfer = Some(transfer);
                Ok(Value::Resource(result))
            }
            Value::Copy(value) => {
                let ty = match value {
                    l::Immediate::I64(_) => o::Type::I64,
                    l::Immediate::Bool(_) => o::Type::Bool,
                    _ => return Err(Reason::Entry),
                };
                if self.declaration().result.ty != ty {
                    return Err(Reason::Entry);
                }
                Ok(Value::Copy(value))
            }
        }
    }
    fn commit_result(&mut self, memory: &mut dyn Memory) -> Result<(), Reason> {
        if self.loans.iter().any(Option::is_some)
            || self
                .resources
                .scope_closed(self.caller, self.guest, &self.inputs, self.transfer, memory)
                .is_err()
        {
            self.instance.phase = Phase::Poisoned;
            return Err(Reason::Instance(InstanceError::ScopeNotClosed));
        }
        if let Some(transfer) = self.transfer {
            self.resources
                .commit_transfer(transfer, memory)
                .map_err(fault)?;
            self.transfer = None;
        }
        self.output_claims.clear();
        self.holding_output = false;
        Ok(())
    }
}
impl<'state, 'p, 'u, 'resources, 'code, 'native>
    Invocation<'state, 'p, 'u, 'resources, 'code, 'native>
{
    /// Prepare a root over retained storage and independently scoped program
    /// views. All native registrations must retain their original owners.
    pub fn prepare_owned(
        instance: &'state mut Instance<'p, 'u>,
        function: m::FunctionId,
        resources: &'resources mut Resources<'code>,
        invoker: &'native mut dyn Invoker,
        budget: &mut m::Budget,
    ) -> Result<Self, Reason> {
        instance.ready()?;
        if instance.caller.is_none() {
            return Err(Reason::Entry);
        }
        for import in instance.program.bindings().imports() {
            budget.charge(1)?;
            if !import.provider().is_owned() {
                return Err(Reason::Entry);
            }
        }
        Self::prepare_inner(instance, function, resources, invoker, budget, |kind| {
            kind.into_owned().map_err(|_| Reason::Entry)
        })
    }
}

impl Drop for Invocation<'_, '_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        if self.committed && !self.finished {
            self.instance.phase = Phase::Poisoned;
        }
        self.abort_transfer();
        for loan in self.loans.iter_mut().filter_map(Option::take) {
            if self.resources.end_loan(loan).is_err() {
                self.instance.phase = Phase::Poisoned;
            }
        }
        if self.instance.phase != Phase::Poisoned {
            self.instance.phase = Phase::Ready;
        }
    }
}
