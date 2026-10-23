//! RFC 0028 schema-5 task MIR. Decoding reconstructs independent semantic
//! proofs; providers, allocator authority and runtime state remain host inputs.
use crate::ownership::{self, CodecError};
use lang_mir::unit::{self as m, ownership as o};

#[derive(Clone, Copy, Debug)]
pub struct CodecLimits {
    pub bytes: usize,
    pub verification: o::tasks::Limits,
}
impl Default for CodecLimits {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024,
            verification: Default::default(),
        }
    }
}
impl CodecLimits {
    pub(crate) fn shared(self) -> ownership::CodecLimits {
        let v = self.verification;
        let common = v.bodies.places.ownership.unit;
        ownership::CodecLimits {
            bytes: self.bytes,
            verification: ownership::VerificationLimits {
                bodies: v.bodies,
                moves: o::moves::Limits {
                    work: common.work,
                    allocations: common.allocations,
                    elements: common.elements,
                    state_cells: v.state_cells,
                },
                activity: o::loan_activity::Limits {
                    work: common.work,
                    allocations: common.allocations,
                    elements: common.elements,
                    state_cells: v.state_cells,
                },
                loans: o::loans::Limits {
                    work: common.work,
                    allocations: common.allocations,
                    elements: common.elements,
                    nodes: v.loan_nodes,
                    depth: common.depth,
                    state_words: v.loan_words,
                },
                failure: o::failure::Limits {
                    work: common.work,
                    allocations: common.allocations,
                    elements: common.elements,
                    nodes: v.failure_nodes,
                    regions: v.regions,
                },
            },
        }
    }
}

/// Complete task proof and optional checked script entry, without live bindings.
pub struct Program {
    pub(crate) proof: o::tasks::VerifiedTasks,
    pub(crate) entry: Option<m::FunctionId>,
}
impl Program {
    pub fn proof(&self) -> &o::tasks::VerifiedTasks {
        &self.proof
    }
    pub const fn entry(&self) -> Option<m::FunctionId> {
        self.entry
    }
    pub fn usage(&self) -> m::Usage {
        let p = self.proof.plan();
        m::Usage {
            work: p.work_used(),
            allocation_bytes: p.requested_bytes(),
            elements: p.elements_used(),
        }
    }
    /// Recover raw semantic input; proof flags and live state are not retained.
    pub fn into_parts(self) -> (o::bodies::BodyTable, Option<m::FunctionId>) {
        let table = self
            .proof
            .into_plan()
            .into_source()
            .into_activity()
            .into_moved()
            .into_typed()
            .into_table();
        (table, self.entry)
    }
}

pub fn decode(bytes: &[u8], limits: CodecLimits) -> Result<Program, CodecError> {
    ownership::decode_tasks(bytes, limits)
}

pub fn encode(
    table: o::bodies::BodyTable,
    entry: Option<m::FunctionId>,
    limits: CodecLimits,
) -> Result<Vec<u8>, CodecError> {
    ownership::encode_tasks(table, entry, limits)
}
