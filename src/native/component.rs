//! RFC 0032 trusted component package publication and scoped installed binding.
//! An artifact supplies data; only the caller's actual Program supplies grants.
use super::Package;
use crate::artifact::component as artifact;
use lang_codegen_clif::{self as clif, native_record, sdk, tasks};
use lang_mir::unit as m;
use lang_runtime::component::program;
use std::{ffi::c_void, fmt, ptr::NonNull};

#[derive(Clone, Copy, Debug, Default)]
pub struct Limits {
    pub artifact: artifact::Limits,
    pub record: native_record::Limits,
    pub plans: m::Limits,
}
#[derive(Debug)]
pub enum Error {
    Artifact(Box<artifact::Error>),
    Correspondence,
    Entry,
    Target {
        expected: clif::Triple,
        actual: clif::Triple,
    },
    Build {
        expected: String,
        actual: String,
    },
    Record(native_record::Error),
    Plan(Box<tasks::Error>),
    Preparation(m::Error),
    Capacity,
    Package(Box<super::Error>),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native component package: {self:?}")
    }
}
impl std::error::Error for Error {}

fn correspondence(
    semantic: &[u8],
    program: &program::Program<'_>,
    limits: Limits,
) -> Result<(), Error> {
    let decoded =
        artifact::decode(semantic, limits.artifact).map_err(|e| Error::Artifact(Box::new(e)))?;
    if decoded.entry().is_some() {
        return Err(Error::Entry);
    }
    let expected = decoded.unit();
    let actual = program.bindings().unit();
    // Both independently checked flat graphs retain finite preparation usage.
    // Admit their whole comparison before traversing code or unused metadata.
    let mut budget = m::Budget::new(limits.plans);
    for usage in [expected.usage(), actual.usage()] {
        budget
            .charge(
                usage
                    .work
                    .saturating_add(usage.allocation_bytes)
                    .saturating_add(usage.elements),
            )
            .map_err(Error::Preparation)?;
    }
    if expected.document().document() != actual.document().document()
        || expected.code() != actual.code()
        || expected.mappings() != actual.mappings()
    {
        return Err(Error::Correspondence);
    }
    Ok(())
}

/// Publish one explicitly selected export from the actual bound Program.
/// Callers prepare that Program from these canonical artifact bytes and retain
/// their real provider owners; this operation executes no provider or guest.
pub fn compile(
    semantic: &[u8],
    program: &program::Program<'_>,
    entry: m::FunctionId,
    target: clif::Triple,
    limits: Limits,
) -> Result<Package, Error> {
    let expected = clif::Triple::host();
    if target != expected {
        return Err(Error::Target {
            expected,
            actual: target,
        });
    }
    correspondence(semantic, program, limits)?;
    if !program.is_export(entry) {
        return Err(Error::Entry);
    }
    let object = tasks::ObjectArtifact::emit_component(program, entry, limits.plans)
        .map_err(|e| Error::Plan(Box::new(e)))?;
    let symbols = object.metadata().map(|f| f.symbol.clone()).collect();
    let record = object
        .into_recorded(limits.plans)
        .map_err(|e| Error::Plan(Box::new(e)))?
        .into_record();
    // Admit the caller's record framing limits before package allocation or
    // publication; the common assembler independently verifies its own output.
    record
        .encode(semantic, limits.record)
        .map_err(Error::Record)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(semantic.len())
        .map_err(|_| Error::Capacity)?;
    bytes.extend_from_slice(semantic);
    super::assemble_package(
        bytes,
        record,
        "component",
        Some(entry),
        symbols,
        None,
        target,
    )
    .map_err(|e| Error::Package(Box::new(e)))
}

/// Reconstruct native nodes with the caller's actual component authority.
/// No code is generated and no guest or provider is invoked by this function.
///
/// # Safety
/// The resolver must return the matching symbols from the exact unmodified
/// object paired with input.native, emitted by this compiler/SDK for this host.
/// Its owner must retain that executable image through the returned program's
/// lifetime. Hashes and semantic validation do not authenticate arbitrary code.
pub unsafe fn bind<'p, I>(
    input: sdk::runtime::Input<'_>,
    program: &'p program::Program<'p>,
    owner: I,
    mut resolve: impl FnMut(&I, &str) -> Option<NonNull<c_void>>,
    limits: Limits,
) -> Result<tasks::LinkedProgram<'p, I>, Error> {
    let expected = clif::Triple::host();
    if input.target != expected.to_string() {
        let actual = input.target.parse().map_err(|_| Error::Correspondence)?;
        return Err(Error::Target { expected, actual });
    }
    let build = sdk::build_id(&expected);
    if input.build_id != build {
        return Err(Error::Build {
            expected: build,
            actual: input.build_id.to_owned(),
        });
    }
    correspondence(input.semantic, program, limits)?;
    let record = native_record::Record::decode(input.native, input.semantic, limits.record)
        .map_err(Error::Record)?;
    let native_record::Kind::Component(entry) = record.kind() else {
        return Err(Error::Entry);
    };
    if !program.is_export(entry) {
        return Err(Error::Entry);
    }
    let object = tasks::RecordedObject::from_record(record)
        .map_err(Error::Record)?
        .prepare_component(program, limits.plans)
        .map_err(|e| Error::Plan(Box::new(e)))?;
    // SAFETY: forwarded exact-image, address and lifetime contract above. All
    // metadata and correspondence checks finish before the first resolution.
    unsafe {
        object.bind(
            owner,
            |owner, name| resolve(owner, name).map(|p| p.as_ptr().cast_const().cast()),
            limits.plans,
        )
    }
    .map_err(|e| Error::Plan(Box::new(e)))
}
