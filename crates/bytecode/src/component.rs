//! Bounded schema-5 Component MIR transport for the complete artifact loader.
//! This module deliberately returns raw, unverified tables. Metadata and all
//! semantic/binding proofs must be checked before publishing an executable.
use crate::{ownership, tasks};
use lang_mir::unit::{self as m, ownership as o};

pub struct Unverified<'a> {
    pub table: o::bodies::BodyTable,
    pub entry: Option<m::FunctionId>,
    pub metadata: &'a [u8],
    pub metadata_offset: usize,
    pub usage: m::Usage,
}
pub fn decode(
    bytes: &[u8],
    limits: tasks::CodecLimits,
) -> Result<Unverified<'_>, ownership::CodecError> {
    ownership::decode_component(bytes, limits)
}
/// Encode an already checked MIR proof and the caller's complete checked
/// metadata. The result remains untrusted bytes, not a component binding proof.
/// Prior usage must contain this proof and metadata preparation on one ledger.
pub fn encode(
    proof: o::tasks::VerifiedTasks,
    entry: Option<m::FunctionId>,
    metadata: &[u8],
    limits: tasks::CodecLimits,
    prior: m::Usage,
) -> Result<Vec<u8>, ownership::CodecError> {
    ownership::encode_component(proof, entry, metadata, limits, prior)
}
