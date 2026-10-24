//! Complete RFC 0028 Component artifact admission.
#![allow(
    clippy::result_large_err,
    reason = "Keep structured failures inline after exhaustion."
)]
use lang_bytecode::{component as code, ownership, tasks as task_wire};
use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::component::{bindings as b, wire, wire::artifact as metadata};
use std::fmt;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub tasks: o::tasks::Limits,
    pub metadata: wire::Limits,
    pub bindings: b::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024,
            tasks: Default::default(),
            metadata: Default::default(),
            bindings: Default::default(),
        }
    }
}
impl Limits {
    fn normalized(mut self) -> Self {
        let mut common = self.tasks.bodies.places.ownership.unit;
        common.work = common
            .work
            .min(self.metadata.work)
            .min(self.metadata.metadata.work)
            .min(self.bindings.budget.work);
        common.allocations = common
            .allocations
            .min(self.metadata.allocation_bytes)
            .min(self.metadata.metadata.allocation_bytes)
            .min(self.bindings.budget.allocations);
        common.elements = common
            .elements
            .min(self.metadata.elements)
            .min(self.bindings.budget.elements);
        self.tasks.bodies.places.ownership.unit = common;
        self.bindings.budget = common;
        self.metadata.work = common.work;
        self.metadata.allocation_bytes = common.allocations;
        self.metadata.elements = common.elements;
        self.metadata.bytes = self.metadata.bytes.min(self.bytes);
        self
    }
    fn code(self) -> task_wire::CodecLimits {
        task_wire::CodecLimits {
            bytes: self.bytes,
            verification: self.tasks,
        }
    }
}
#[derive(Debug)]
pub enum Error {
    Code(ownership::CodecError),
    Metadata(wire::Error),
    Tasks(o::tasks::Error),
    Bindings(b::Error),
    Entry,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "component artifact: {self:?}")
    }
}
impl std::error::Error for Error {}
pub struct Program {
    unit: b::Unit,
    entry: Option<m::FunctionId>,
}
impl Program {
    pub fn unit(&self) -> &b::Unit {
        &self.unit
    }
    pub const fn entry(&self) -> Option<m::FunctionId> {
        self.entry
    }
    pub fn usage(&self) -> m::Usage {
        self.unit.usage()
    }
    pub fn into_unit(self) -> b::Unit {
        self.unit
    }
}
fn checked(
    raw: o::bodies::BodyTable,
    data: metadata::Metadata,
    entry: Option<m::FunctionId>,
    limits: Limits,
    usage: m::Usage,
) -> Result<Program, Error> {
    let proof = o::tasks::verify_with_usage(raw, limits.tasks, usage).map_err(Error::Tasks)?;
    if let Some(id) = entry {
        let declaration = proof
            .typed()
            .places()
            .declarations()
            .functions()
            .get(id.0 as usize)
            .ok_or(Error::Entry)?;
        if !declaration.parameters.is_empty() || declaration.result.ty != o::Type::I64 {
            return Err(Error::Entry);
        }
    }
    let flow = proof.plan();
    let usage = m::Usage {
        work: flow.work_used(),
        allocation_bytes: flow.requested_bytes(),
        elements: flow.elements_used(),
    };
    let unit = b::Unit::with_cumulative_usage(
        data.document,
        b::Code::Tasks(proof),
        data.mappings,
        limits.bindings,
        usage,
    )
    .map_err(Error::Bindings)?;
    Ok(Program { unit, entry })
}
pub fn decode(bytes: &[u8], limits: Limits) -> Result<Program, Error> {
    let limits = limits.normalized();
    let raw = code::decode(bytes, limits.code()).map_err(Error::Code)?;
    let mut session =
        wire::Session::with_usage(limits.metadata, raw.usage).map_err(Error::Metadata)?;
    let data = metadata::decode(
        raw.metadata,
        &raw.table.places.declarations.types.sources,
        &mut session,
        limits.bindings.bindings,
    )
    .map_err(|mut e| {
        e.offset = e.offset.saturating_add(raw.metadata_offset);
        Error::Metadata(e)
    })?;
    checked(
        raw.table,
        data,
        raw.entry,
        limits,
        session.preparation_usage(),
    )
}
/// Preserve code snapshot indices and append/reuse the explicitly supplied
/// original IDL snapshot. No source text is parsed by this encoder or decoder.
pub fn encode(
    unit: b::Unit,
    snapshot: m::Source,
    entry: Option<m::FunctionId>,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let limits = limits.normalized();
    let (document, code, map) = unit.into_parts();
    let flow = match code {
        b::Code::Owned(flow) => flow,
        b::Code::Tasks(proof) => proof.into_plan(),
    };
    let mut table = flow
        .into_source()
        .into_activity()
        .into_moved()
        .into_typed()
        .into_table();
    let mut document = document.into_document();
    let mut session =
        wire::Session::with_usage(limits.metadata, m::Usage::default()).map_err(Error::Metadata)?;
    metadata::relocate(
        &mut document,
        &mut table.places.declarations.types.sources,
        snapshot,
        limits.tasks.bodies.places.ownership.unit.sources,
        &mut session,
    )
    .map_err(Error::Metadata)?;
    let data = metadata::prepare(
        document,
        map,
        &table.places.declarations.types.sources,
        &mut session,
        limits.bindings.bindings,
    )
    .map_err(Error::Metadata)?;
    let program = checked(table, data, entry, limits, session.preparation_usage())?;
    session
        .advance(program.unit.usage())
        .map_err(Error::Metadata)?;
    let suffix = metadata::encode(
        program.unit.document(),
        program.unit.mappings(),
        &program.unit.code().declarations().types().table().sources,
        &mut session,
        limits.bindings.bindings,
    )
    .map_err(Error::Metadata)?;
    let (_, code, _) = program.unit.into_parts();
    let b::Code::Tasks(proof) = code else {
        unreachable!("checked component task proof")
    };
    code::encode(
        proof,
        entry,
        &suffix,
        limits.code(),
        session.preparation_usage(),
    )
    .map_err(Error::Code)
}
