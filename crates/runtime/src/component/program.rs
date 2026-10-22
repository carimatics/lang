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
}
impl<'a> Program<'a> {
    pub fn prepare(bindings: &'a grants::Bindings<'a>, limits: m::Limits) -> Result<Self, Error> {
        let unit = bindings.unit();
        let proof = match unit.code() {
            bindings::Code::Tasks(proof) => Cow::Borrowed(proof),
            bindings::Code::Owned(flow) => {
                let typed = flow.source().activity().moved().typed();
                let mut budget = m::Budget::with_usage(limits, unit.usage())?;
                budget.charge(typed.work_used())?;
                budget.allocate::<u8>(typed.requested_bytes())?;
                let table = typed.clone().into_table();
                let mut verification = o::tasks::Limits::default();
                verification.bodies.places.ownership.unit = limits;
                let usage = m::Usage {
                    work: budget.work_used(),
                    allocation_bytes: budget.allocation_bytes(),
                    elements: budget.elements_used(),
                };
                Cow::Owned(
                    o::tasks::verify_with_usage(table, verification, usage)
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
        Ok(Self { bindings, proof })
    }
    pub const fn bindings(&self) -> &'a grants::Bindings<'a> {
        self.bindings
    }
    pub fn proof(&self) -> &o::tasks::VerifiedTasks {
        &self.proof
    }
    pub fn is_export(&self, function: m::FunctionId) -> bool {
        self.bindings
            .unit()
            .exports()
            .iter()
            .any(|(id, _)| *id == function)
    }
}
