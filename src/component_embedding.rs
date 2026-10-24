//! Checked, owned module inputs for the RFC 0027 embedding gateway.
//!
//! This preparation layer supplies no C API table or execution authority.
use crate::{artifact, component_bindings as generated, frontend};
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::component::{bindings, idl, wire};
use std::{fmt, path::Path};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub input_bytes: usize,
    pub preparation: m::Limits,
    pub compile: frontend::CompileLimits,
    pub generation: generated::Limits,
    pub bindings: bindings::Limits,
    pub artifact: artifact::component::Limits,
    pub metadata: wire::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            input_bytes: 64 * 1024 * 1024,
            preparation: Default::default(),
            compile: Default::default(),
            generation: Default::default(),
            bindings: Default::default(),
            artifact: Default::default(),
            metadata: Default::default(),
        }
    }
}

pub enum Error {
    Format(u32),
    ArtifactContract,
    InputBytes { limit: usize, attempted: usize },
    Budget(m::Error),
    Idl(idl::Error),
    Generation(generated::Error),
    Source(Box<frontend::component::Analysis>),
    Artifact(artifact::component::Error),
    Wire(wire::Error),
    Utf8(std::str::Utf8Error),
}
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(format) => f.debug_tuple("Format").field(format).finish(),
            Self::ArtifactContract => f.write_str("ArtifactContract"),
            Self::InputBytes { limit, attempted } => f
                .debug_struct("InputBytes")
                .field("limit", limit)
                .field("attempted", attempted)
                .finish(),
            Self::Budget(e) => f.debug_tuple("Budget").field(e).finish(),
            Self::Idl(e) => f.debug_tuple("Idl").field(e).finish(),
            Self::Generation(e) => f.debug_tuple("Generation").field(e).finish(),
            Self::Source(e) => f
                .debug_struct("Source")
                .field("error", &e.result.as_ref().err())
                .field("diagnostics", &e.diagnostics)
                .finish(),
            Self::Artifact(e) => f.debug_tuple("Artifact").field(e).finish(),
            Self::Wire(e) => f.debug_tuple("Wire").field(e).finish(),
            Self::Utf8(e) => f.debug_tuple("Utf8").field(e).finish(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "component module preparation failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<m::Error> for Error {
    fn from(value: m::Error) -> Self {
        Self::Budget(value)
    }
}

pub struct Module {
    unit: bindings::Unit,
    contract: wire::Contract,
    source_idl: Option<m::Source>,
    usage: m::Usage,
}
impl Module {
    pub fn unit(&self) -> &bindings::Unit {
        &self.unit
    }
    pub fn contract(&self) -> &wire::Contract {
        &self.contract
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub fn idl_snapshot(&self) -> &m::Source {
        self.source_idl.as_ref().unwrap_or_else(|| {
            &self.unit.code().declarations().types().table().sources
                [self.unit.document().document().source.0 as usize]
        })
    }
}

fn usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}
fn add(limits: m::Limits, a: m::Usage, b: m::Usage) -> Result<m::Usage, Error> {
    let mut budget = m::Budget::with_usage(limits, a)?;
    budget.charge(b.work)?;
    budget.bytes(b.allocation_bytes)?;
    budget.allocate::<()>(b.elements)?;
    Ok(usage(&budget))
}
fn remaining(mut limits: m::Limits, used: m::Usage) -> Result<m::Limits, Error> {
    m::Budget::with_usage(limits, used)?;
    limits.work -= used.work;
    limits.allocations -= used.allocation_bytes;
    limits.elements -= used.elements;
    Ok(limits)
}
fn metadata(mut limits: wire::Limits, budget: m::Limits) -> wire::Limits {
    limits.work = limits.work.min(budget.work);
    limits.allocation_bytes = limits.allocation_bytes.min(budget.allocations);
    limits.elements = limits.elements.min(budget.elements);
    limits.metadata.work = limits.metadata.work.min(budget.work);
    limits.metadata.allocation_bytes = limits.metadata.allocation_bytes.min(budget.allocations);
    limits.metadata.elements = limits.metadata.elements.min(budget.elements);
    limits
}
fn restrict(mut limits: m::Limits, available: m::Limits) -> m::Limits {
    limits.work = limits.work.min(available.work);
    limits.allocations = limits.allocations.min(available.allocations);
    limits.elements = limits.elements.min(available.elements);
    limits
}
fn common(limits: Limits) -> m::Limits {
    let mut budget = limits.preparation;
    for other in [
        limits.generation.budget,
        limits.bindings.budget,
        limits.artifact.bindings.budget,
        limits.artifact.tasks.bodies.places.ownership.unit,
    ] {
        budget.work = budget.work.min(other.work);
        budget.allocations = budget.allocations.min(other.allocations);
        budget.elements = budget.elements.min(other.elements);
    }
    for other in [limits.metadata, limits.artifact.metadata] {
        budget.work = budget.work.min(other.work).min(other.metadata.work);
        budget.allocations = budget
            .allocations
            .min(other.allocation_bytes)
            .min(other.metadata.allocation_bytes);
        budget.elements = budget
            .elements
            .min(other.elements)
            .min(other.metadata.elements);
    }
    budget
}

/// Format 0: one virtual source plus its original IDL JSON. Format 1: a complete
/// schema-5 component artifact, with an empty external contract argument.
pub fn load(format: u32, program: &[u8], contract: &[u8], limits: Limits) -> Result<Module, Error> {
    let bytes = program
        .len()
        .checked_add(contract.len())
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))?;
    if bytes > limits.input_bytes {
        return Err(Error::InputBytes {
            limit: limits.input_bytes,
            attempted: bytes,
        });
    }
    // Inspect lengths and admit the scan before parsing format-dependent data.
    let common = common(limits);
    let mut preflight = m::Budget::new(common);
    preflight.charge(bytes)?;
    match format {
        0 => {
            // The IDL snapshot is separate from the code-local source table.
            let text = std::str::from_utf8(contract).map_err(Error::Utf8)?;
            preflight.charge(contract.len())?;
            preflight.allocate::<u8>(contract.len())?;
            let mut snapshot = String::new();
            snapshot
                .try_reserve_exact(contract.len())
                .map_err(|_| m::Error::new(m::ErrorKind::AllocationFailed))?;
            snapshot.push_str(text);
            preflight.bytes("component-idl.json".len())?;
            let base = usage(&preflight);
            let available = remaining(common, base)?;
            let document = idl::check(
                m::SourceId(1),
                contract,
                metadata(limits.metadata, available).metadata,
            )
            .map_err(Error::Idl)?;
            let original = document.usage().budget;
            let guest = generated::guest(
                &document,
                generated::Limits {
                    budget: restrict(limits.generation.budget, available),
                    ..limits.generation
                },
            )
            .map_err(Error::Generation)?;
            // Guest usage includes the IDL history, which the frontend also
            // retains. Carry only the generator's additional work once.
            let extra = m::Usage {
                work: guest.usage.work - original.work,
                allocation_bytes: guest.usage.allocation_bytes - original.allocation_bytes,
                elements: guest.usage.elements - original.elements,
            };
            let prior = add(common, base, extra)?;
            let available = remaining(common, prior)?;
            let mut compile = limits.compile;
            compile.mir.work = compile.mir.work.min(available.work);
            compile.mir.allocations = compile.mir.allocations.min(available.allocations);
            compile.mir.elements = compile.mir.elements.min(available.elements);
            let analysis = frontend::component::analyze(frontend::component::Request {
                sources: SourceMap::new(),
                display: Path::new("component-source.lang"),
                bytes: program,
                root: None,
                compile,
                document,
                bindings: guest.bindings,
                limits: bindings::Limits {
                    budget: restrict(limits.bindings.budget, available),
                    ..limits.bindings
                },
            });
            if analysis.result.is_err() {
                return Err(Error::Source(Box::new(analysis)));
            }
            let unit = analysis.result.expect("checked success");
            let prior = add(common, prior, unit.usage())?;
            // Recheck the original snapshot under the remaining cumulative
            // budget; no unmetered deep clone or synthetic wire spans.
            let document = idl::check(
                m::SourceId(1),
                contract,
                metadata(limits.metadata, remaining(common, prior)?).metadata,
            )
            .map_err(Error::Idl)?;
            let prior = add(common, prior, document.usage().budget)?;
            let mut session = wire::Session::with_usage(metadata(limits.metadata, common), prior)
                .map_err(Error::Wire)?;
            let contract =
                wire::Contract::new(document, vec![], &mut session).map_err(Error::Wire)?;
            Ok(Module {
                unit,
                contract,
                source_idl: Some(m::Source {
                    path: "component-idl.json".into(),
                    text: snapshot,
                }),
                usage: session.preparation_usage(),
            })
        }
        1 => {
            if !contract.is_empty() {
                return Err(Error::ArtifactContract);
            }
            let base = usage(&preflight);
            let available = remaining(common, base)?;
            let mut artifact_limits = limits.artifact;
            artifact_limits.bytes = artifact_limits.bytes.min(limits.input_bytes);
            artifact_limits.bindings.budget = restrict(artifact_limits.bindings.budget, available);
            artifact_limits.tasks.bodies.places.ownership.unit = restrict(
                artifact_limits.tasks.bodies.places.ownership.unit,
                available,
            );
            artifact_limits.metadata = metadata(artifact_limits.metadata, available);
            let unit = artifact::component::decode(program, artifact_limits)
                .map_err(Error::Artifact)?
                .into_unit();
            let prior = add(common, base, unit.usage())?;
            let mut session = wire::Session::with_usage(metadata(limits.metadata, common), prior)
                .map_err(Error::Wire)?;
            let contract =
                wire::artifact::source_contract(&unit, &mut session, limits.bindings.bindings)
                    .map_err(Error::Wire)?;
            Ok(Module {
                unit,
                contract,
                source_idl: None,
                usage: session.preparation_usage(),
            })
        }
        other => Err(Error::Format(other)),
    }
}
