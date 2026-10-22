//! Component value transport and resource retirement for the shared engines.
//! This service marshals values and invokes registered native cleanup; it never
//! evaluates guest instructions or creates a replacement invocation allocator.
use super::{
    native::ResourceKind,
    program::Program,
    storage::{self, Claim, Holder, Memory, Resources, Transfer},
};
use crate::{
    cleanup as c, layout as l,
    task::runner::{EntryArgument, Reason},
};
use lang_mir::unit::{self as m, ownership as o};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Copy(l::Immediate),
    Resource(Claim),
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
    fn control_matches(&self, control: crate::execution::Control<'_>) -> bool;
    fn corresponds(&self, proof: &o::tasks::VerifiedTasks, function: m::FunctionId) -> bool;
    fn validate(&self, arguments: &[EntryArgument], memory: &dyn Memory) -> Result<(), Reason>;
    fn prepare_arguments(
        &mut self,
        arguments: &[EntryArgument],
        memory: &dyn Memory,
    ) -> Result<(), Reason>;
    fn argument(&self, index: usize) -> Result<Claim, Reason>;
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
    inputs: Vec<Claim>,
    indices: Vec<usize>,
    transfer: Option<Transfer>,
    committed: bool,
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
        let declaration =
            &program.proof().typed().places().declarations().functions()[function.0 as usize];
        let inputs = budget.vector(declaration.parameters.len())?;
        let indices = budget.vector(declaration.parameters.len())?;
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
            transfer: None,
            committed: false,
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
}
impl Host for Invocation<'_, '_, '_, '_, '_> {
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
        let result = decl.result.ty;
        if result != o::Type::I64 && result != o::Type::Bool && self.key(result).is_err() {
            return Err(Reason::Entry);
        }
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
        for (index, arg) in arguments.iter().enumerate() {
            if let EntryArgument::Resource(claim) = arg {
                self.inputs.push(*claim);
                self.indices.push(index);
            }
        }
        self.transfer = Some(
            self.resources
                .prepare_transfer(&self.inputs, self.caller, self.guest, memory)
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
            .commit_transfer(self.transfer.ok_or(Reason::Invariant)?, memory)
            .map_err(fault)?;
        self.transfer = None;
        self.committed = true;
        Ok(())
    }
    fn abort_arguments(&mut self) {
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
        let mut backing = self
            .resources
            .detach(claim, self.guest, memory)
            .map_err(|_| 17u32)?;
        token.bytes.fill(0);
        let kind = backing.kind().map_err(|_| 17u32)?;
        // Context contains individual native failures/panics, then both original
        // allocations are freed even when that attempt failed.
        let mut cleanup =
            c::Contextual::new(|_: &mut (), _: c::Reason, context: &mut c::Context<'_>| {
                self.invoker.destroy(kind, &mut backing, memory, context)
            });
        context.field((), token.leaf.span, &mut cleanup);
        backing.release(memory).map_err(|_| 17u32)
    }
    fn prepare_result(&mut self, value: Value, memory: &dyn Memory) -> Result<Value, Reason> {
        if self.transfer.is_some() || !self.committed {
            return Err(Reason::Invariant);
        }
        match value {
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
        if let Some(transfer) = self.transfer.take() {
            self.resources
                .commit_transfer(transfer, memory)
                .map_err(fault)?;
        }
        Ok(())
    }
}
impl Drop for Invocation<'_, '_, '_, '_, '_> {
    fn drop(&mut self) {
        self.abort_arguments();
    }
}
