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
mod result;
pub(crate) use result::read_result;

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
    fn control_matches(&self, control: crate::execution::Control<'_>) -> bool;
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

pub struct Invocation<'p, 'u, 'resources, 'code, 'native> {
    program: &'p Program<'u>,
    function: m::FunctionId,
    resources: &'resources mut Resources<'code>,
    invoker: &'native mut dyn Invoker,
    caller: Holder,
    guest: Holder,
    inputs: Vec<storage::Input>,
    indices: Vec<usize>,
    loans: Vec<Option<storage::Loan>>,
    transfer: Option<Transfer>,
    committed: bool,
    input_frame: Option<InputFrame>,
    next_frame: Option<u64>,
    output_session: Option<&'native mut wire::Session>,
    output_claims: Vec<(Claim, Option<m::Span>)>,
    holding_output: bool,
}
fn fault(error: storage::Error) -> Reason {
    Reason::Component(error)
}
impl<'p, 'u, 'resources, 'code, 'native> Invocation<'p, 'u, 'resources, 'code, 'native> {
    pub fn prepare(
        program: &'p Program<'u>,
        function: m::FunctionId,
        resources: &'resources mut Resources<'code>,
        invoker: &'native mut dyn Invoker,
        budget: &mut m::Budget,
    ) -> Result<Self, Reason> {
        if !program.is_export(function) {
            return Err(Reason::Entry);
        }
        let inputs = budget.vector(resources.input_capacity())?;
        let indices = budget.vector(resources.input_capacity())?;
        let loans = budget.vector(resources.input_capacity())?;
        let output_claims = budget.vector(resources.capacity())?;
        let caller = resources.host();
        let guest = resources.new_holder().map_err(fault)?;
        Ok(Self {
            program,
            function,
            resources,
            invoker,
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
            output_claims,
            holding_output: false,
        })
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

    /// Reuse the caller's cumulative codec budget for rich result publication.
    pub fn prepare_output(&mut self, session: &'native mut wire::Session) -> Result<(), Reason> {
        if self.committed || self.transfer.is_some() || self.output_session.is_some() {
            return Err(Reason::Entry);
        }
        self.output_session = Some(session);
        Ok(())
    }
    /// Dynamic argument loans remaining in this invocation's original table.
    pub fn active_loans(&self) -> usize {
        self.loans.iter().filter(|loan| loan.is_some()).count()
    }

    fn retire_claim(
        &mut self,
        claim: Claim,
        span: Option<m::Span>,
        memory: &mut dyn Memory,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let mut backing = self
            .resources
            .detach(claim, self.guest, memory)
            .map_err(|_| 17u32)?;
        let kind = backing.kind().map_err(|_| 17u32)?;
        let mut cleanup =
            c::Contextual::new(|_: &mut (), _: c::Reason, context: &mut c::Context<'_>| {
                self.invoker.destroy(kind, &mut backing, memory, context)
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
        .map_err(Reason::ComponentFrame)
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
            .map_err(Reason::ComponentFrame)?;
        let mut claims = session
            .vector(uses.len(), 0)
            .map_err(Reason::ComponentFrame)?;
        let mut nodes = session
            .vector(uses.len(), 0)
            .map_err(Reason::ComponentFrame)?;
        for used in uses {
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
        let signature = self.frame_signature(session)?;
        let (bytes, uses) = wire::encode_contextual_values(
            &signature,
            wire::FrameKind::Parameters,
            values,
            session,
        )
        .map_err(Reason::ComponentFrame)?;
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
        let signature = self.frame_signature(session)?;
        let (values, uses) =
            wire::decode_contextual_values(&signature, wire::FrameKind::Parameters, bytes, session)
                .map_err(Reason::ComponentFrame)?;
        let (claims, nodes) = self.checked_resources(&values, &uses, resources, memory, session)?;
        let generation = self
            .next_frame
            .ok_or_else(|| fault(storage::Error::Exhausted))?;
        let mut arguments = session
            .vector(values.roots.len(), 0)
            .map_err(Reason::ComponentFrame)?;
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
impl Host for Invocation<'_, '_, '_, '_, '_> {
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
    fn corresponds(&self, proof: &o::tasks::VerifiedTasks, function: m::FunctionId) -> bool {
        std::ptr::eq(self.program.proof(), proof) && self.function == function
    }
    fn validate(&self, arguments: &[EntryArgument], memory: &dyn Memory) -> Result<(), Reason> {
        let decl = self.declaration();
        if self.committed || self.transfer.is_some() || arguments.len() != decl.parameters.len() {
            return Err(Reason::Entry);
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
        self.holding_output = false;
        if let Some(ticket) = self.transfer.take() {
            let _ = self.resources.abort_transfer(ticket);
        }
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
            let mut nodes = session.vector(1, 0).map_err(Reason::ComponentFrame)?;
            let mut roots = session.vector(1, 0).map_err(Reason::ComponentFrame)?;
            let mut resources = session
                .vector(usize::from(matches!(value, Value::Resource(_))), 0)
                .map_err(Reason::ComponentFrame)?;
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
                let session = self.output_session.take().ok_or(Reason::Unsupported)?;
                let staged = (|| -> Result<_, Reason> {
                    let signature = self.frame_signature(session)?;
                    let (bytes, uses) = wire::encode_contextual_values(
                        &signature,
                        wire::FrameKind::Result,
                        &frame.values,
                        session,
                    )
                    .map_err(Reason::ComponentFrame)?;
                    if uses.len() != frame.resources.len() || uses.len() > self.resources.capacity()
                    {
                        return Err(Reason::Entry);
                    }
                    session
                        .charge(uses.len().saturating_mul(uses.len()).saturating_mul(3), 0)
                        .map_err(Reason::ComponentFrame)?;
                    for used in uses {
                        if used.access != wire::ResourceAccess::Own {
                            return Err(Reason::Entry);
                        }
                        let claim = *frame
                            .resources
                            .get((used.index.get() - 1) as usize)
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
                        .map_err(Reason::ComponentFrame)?;
                    Ok((bytes, caller))
                })();
                self.output_session = Some(session);
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
impl Drop for Invocation<'_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        self.abort_arguments();
        for loan in self.loans.iter_mut().filter_map(Option::take) {
            let _ = self.resources.end_loan(loan);
        }
    }
}
