//! Existing-engine proof for an independently bound component source unit.
use super::{bindings, grants};
use lang_mir::unit::{self as m, ownership as o};
use std::{borrow::Cow, fmt};

#[derive(Debug)]
pub enum Error {
    Resource(m::Error),
    Tasks(o::tasks::Error),
    Correspondence,
}
impl From<m::Error> for Error {
    fn from(e: m::Error) -> Self {
        Self::Resource(e)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "component executable proof: {self:?}")
    }
}
impl std::error::Error for Error {}

/// Own source units reuse the stronger existing task/ownership proof so both
/// backends can share frame/control/retirement services. This introduces no
/// evaluator or new source/serialized representation.
pub struct Program<'a> {
    bindings: &'a grants::Bindings<'a>,
    proof: Cow<'a, o::tasks::VerifiedTasks>,
    native_calls: bool,
    usage: m::Usage,
    prepared: std::cell::Cell<Option<m::BudgetCheckpoint>>,
}
impl<'a> Program<'a> {
    pub fn prepare(bindings: &'a grants::Bindings<'a>, limits: m::Limits) -> Result<Self, Error> {
        Self::prepare_using(bindings, &mut m::Budget::new(limits))
    }

    /// Resume every proof phase on the original ledger, retaining refused prefixes.
    /// The returned program may consume its admitted entry checkpoint once, only
    /// on that same ledger. Later entries pay the ordinary intrinsic cost again.
    pub fn prepare_with_budget(
        bindings: &'a grants::Bindings<'a>,
        budget: &mut m::Budget,
    ) -> Result<Self, Error> {
        let mut scope = budget.scope_usage();
        let program = Self::prepare_using(bindings, &mut scope)?;
        program.prepared.set(Some(scope.checkpoint()?));
        Ok(program)
    }

    fn charge_usage(budget: &mut m::Budget, usage: m::Usage) -> Result<(), Error> {
        budget.charge(usage.work)?;
        budget.bytes(usage.allocation_bytes)?;
        budget.allocate::<()>(usage.elements)?;
        Ok(())
    }

    pub(crate) fn begin_preparation(&self, budget: &mut m::Budget) -> Result<(), Error> {
        if let Some(checkpoint) = self.prepared.get() {
            if !budget.includes_checkpoint(checkpoint) {
                return Err(Error::Correspondence);
            }
            self.prepared.set(None);
            Ok(())
        } else {
            Self::charge_usage(budget, self.usage)
        }
    }

    fn prepare_using(
        bindings: &'a grants::Bindings<'a>,
        budget: &mut m::Budget,
    ) -> Result<Self, Error> {
        let unit = bindings.unit();
        let proof = match unit.code() {
            bindings::Code::Tasks(proof) => {
                Self::charge_usage(
                    budget,
                    m::Usage {
                        work: proof.plan().work_used(),
                        allocation_bytes: proof.plan().requested_bytes(),
                        elements: proof.plan().elements_used(),
                    },
                )?;
                Cow::Borrowed(proof)
            }
            bindings::Code::Owned(flow) => {
                let typed = flow.source().activity().moved().typed();
                Self::charge_usage(budget, unit.usage())?;
                budget.charge(typed.work_used())?;
                budget.allocate::<u8>(typed.requested_bytes())?;
                let table = typed.clone().into_table();
                let mut verification = o::tasks::Limits::default();
                verification.bodies.places.ownership.unit = budget.limits();
                Cow::Owned(
                    o::tasks::verify_with_budget(table, verification, budget)
                        .map_err(Error::Tasks)?,
                )
            }
        };
        let actual = proof.typed().places().declarations();
        let expected = unit.code().declarations();
        if actual.types().table() != expected.types().table()
            || actual.functions() != expected.functions()
            || actual.imports() != expected.imports()
        {
            return Err(Error::Correspondence);
        }
        let mut native_calls = false;
        for instruction in proof
            .typed()
            .functions()
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
        {
            budget.charge(1)?;
            native_calls |= matches!(instruction.op, o::bodies::Op::Import { .. });
        }
        let usage = m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        };
        Ok(Self {
            bindings,
            proof,
            native_calls,
            usage,
            prepared: std::cell::Cell::new(None),
        })
    }
    pub const fn bindings(&self) -> &'a grants::Bindings<'a> {
        self.bindings
    }
    pub fn proof(&self) -> &o::tasks::VerifiedTasks {
        &self.proof
    }
    pub const fn native_calls(&self) -> bool {
        self.native_calls
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub fn is_export(&self, function: m::FunctionId) -> bool {
        self.bindings
            .unit()
            .exports()
            .iter()
            .any(|(id, _)| *id == function)
    }
}
