//! Exact-SDK opaque image. Metadata and snapshots carry no provider authority.
use super::super::manifest;
use crate::component_embedding::{self as embedding, Module};
use lang_codegen_clif::{native_record, sdk, tasks};
use lang_mir::unit as m;
use lang_runtime::component::program;
use sha2::{Digest, Sha256};
use std::{ffi::c_void, ptr::NonNull, rc::Rc};

#[derive(Debug)]
pub enum ImageError {
    Manifest(Box<manifest::Error>),
    Profile,
    Target,
    Build,
    InputBytes,
    Role(manifest::Role),
    Entry,
    Correspondence,
    Module {
        error: Box<embedding::Error>,
        prior: m::Usage,
    },
    Record(native_record::Error),
    Plan(Box<tasks::Error>),
    Preparation(m::Error),
}
impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native component image: {self:?}")
    }
}
impl std::error::Error for ImageError {}
impl From<m::Error> for ImageError {
    fn from(error: m::Error) -> Self {
        Self::Preparation(error)
    }
}

type Resolver = dyn Fn(&str) -> Option<NonNull<c_void>>;
pub(crate) struct ImageData {
    pub(crate) module: Rc<Module>,
    pub(crate) metadata: Vec<u8>,
    semantic: Vec<u8>,
    native: Vec<u8>,
    bindings: Vec<u8>,
    pub(crate) entry: m::FunctionId,
    resolve: Box<Resolver>,
}

/// Opaque only to C, and meaningful only to this exact compiler/SDK build.
#[derive(Clone)]
pub struct Image(pub(crate) Rc<ImageData>);
impl ImageData {
    pub(crate) fn semantic(&self) -> &[u8] {
        &self.semantic
    }
}

fn usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}
fn add(a: m::Usage, b: m::Usage) -> Result<m::Usage, ImageError> {
    let sum = |a: usize, b: usize| {
        a.checked_add(b)
            .ok_or_else(|| ImageError::Preparation(m::Error::new(m::ErrorKind::ArithmeticOverflow)))
    };
    Ok(m::Usage {
        work: sum(a.work, b.work)?,
        allocation_bytes: sum(a.allocation_bytes, b.allocation_bytes)?,
        elements: sum(a.elements, b.elements)?,
    })
}
fn copy(bytes: &[u8], budget: &mut m::Budget) -> Result<Vec<u8>, ImageError> {
    budget.charge(bytes.len())?;
    let mut output = budget.vector(bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(output)
}

/// This check requires no private Image pointer and invokes no native code.
pub(crate) fn compatible(
    metadata: &[u8],
    limits: embedding::Limits,
    budget: &mut m::Budget,
) -> Result<manifest::Program, ImageError> {
    if metadata.len() > limits.input_bytes {
        return Err(ImageError::InputBytes);
    }
    // Bound the strict parser's scans, four records and decoded strings before
    // parsing. Their text storage is bounded by the complete input snapshot.
    budget.charge(metadata.len().saturating_mul(8))?;
    budget.allocate::<manifest::Program>(1)?;
    budget.allocate::<manifest::File>(4)?;
    budget.allocate::<u8>(metadata.len())?;
    let parsed = manifest::parse_program(
        metadata,
        manifest::Limits {
            program_manifest: limits.input_bytes,
            program_total: limits.input_bytes as u64,
            depth: limits.metadata.depth,
            ..Default::default()
        },
    )
    .map_err(|e| ImageError::Manifest(Box::new(e)))?;
    if parsed.version() != 3 || parsed.profile() != manifest::Profile::Component {
        return Err(ImageError::Profile);
    }
    let host = lang_codegen_clif::Triple::host();
    if parsed.target().triple() != host {
        return Err(ImageError::Target);
    }
    budget.allocate::<u8>(64)?;
    budget.charge(64)?;
    let expected = sdk::build_id(&host);
    if parsed
        .build_id()
        .0
        .iter()
        .zip(expected.as_bytes().as_chunks::<2>().0)
        .any(|(&byte, pair)| {
            const HEX: &[u8] = b"0123456789abcdef";
            *pair != [HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]]
        })
    {
        return Err(ImageError::Build);
    }
    Ok(parsed)
}

fn role(
    manifest: &manifest::Program,
    role: manifest::Role,
    bytes: &[u8],
    budget: &mut m::Budget,
) -> Result<(), ImageError> {
    let expected = manifest.file(role).ok_or(ImageError::Role(role))?;
    budget.charge(bytes.len())?;
    if expected.bytes() != bytes.len() as u64
        || expected.hash().0 != <[u8; 32]>::from(Sha256::digest(bytes))
    {
        return Err(ImageError::Role(role));
    }
    Ok(())
}

fn prepare(
    metadata: &[u8],
    semantic: &[u8],
    native: &[u8],
    bindings: &[u8],
    mut limits: embedding::Limits,
    budget: &mut m::Budget,
) -> Result<(Rc<Module>, m::FunctionId), ImageError> {
    let total = [metadata, semantic, native, bindings]
        .iter()
        .try_fold(0usize, |n, b| n.checked_add(b.len()))
        .ok_or(ImageError::InputBytes)?;
    if total > limits.input_bytes {
        return Err(ImageError::InputBytes);
    }
    let manifest = compatible(metadata, limits, budget)?;
    role(&manifest, manifest::Role::Semantic, semantic, budget)?;
    role(&manifest, manifest::Role::NativeRecord, native, budget)?;
    role(&manifest, manifest::Role::Bindings, bindings, budget)?;
    let prior = usage(budget);
    let available = budget.limits();
    limits.preparation = m::Limits {
        work: available.work.saturating_sub(prior.work),
        allocations: available.allocations.saturating_sub(prior.allocation_bytes),
        elements: available.elements.saturating_sub(prior.elements),
        ..available
    };
    let module = embedding::load(1, semantic, &[], limits).map_err(|error| ImageError::Module {
        error: Box::new(error),
        prior,
    })?;
    budget.advance_to(add(prior, module.usage())?)?;
    if module.artifact_entry().is_some() {
        return Err(ImageError::Entry);
    }
    let record =
        native_record::Record::decode_with_budget(native, semantic, limits.input_bytes, budget)
            .map_err(ImageError::Record)?;
    role(&manifest, manifest::Role::Object, record.object(), budget)?;
    let Some(entry) = manifest.entry().map(m::FunctionId) else {
        return Err(ImageError::Entry);
    };
    if record.kind() != native_record::Kind::Component(entry)
        || record.target() != &manifest.target().triple()
        || !module.unit().exports().iter().any(|(id, _)| *id == entry)
    {
        return Err(ImageError::Entry);
    }
    budget.allocate::<Module>(1)?;
    budget.allocate::<usize>(2)?;
    Ok((Rc::new(module), entry))
}

impl Image {
    /// Retain exact installed snapshots and their explicitly supplied code owner.
    ///
    /// # Safety
    /// The resolver must return the matching functions from the exact original
    /// object described by these snapshots, linked with this compiler/SDK for
    /// this host. The owner must retain that code through its last use. Resolver
    /// and owner lifetime operations must not unwind. Hashes are not a sandbox.
    pub unsafe fn new<I: 'static, R: Fn(&I, &str) -> Option<NonNull<c_void>> + 'static>(
        metadata: &[u8],
        input: sdk::runtime::Input<'_>,
        bindings: &[u8],
        owner: I,
        resolve: R,
        limits: embedding::Limits,
    ) -> Result<Self, ImageError> {
        let mut budget = m::Budget::new(limits.preparation);
        let (module, entry) = prepare(
            metadata,
            input.semantic,
            input.native,
            bindings,
            limits,
            &mut budget,
        )?;
        if input.target != lang_codegen_clif::Triple::host().to_string() {
            return Err(ImageError::Target);
        }
        if input.build_id != sdk::build_id(&lang_codegen_clif::Triple::host()) {
            return Err(ImageError::Build);
        }
        let metadata = copy(metadata, &mut budget)?;
        let semantic = copy(input.semantic, &mut budget)?;
        let native = copy(input.native, &mut budget)?;
        let bindings = copy(bindings, &mut budget)?;
        budget.allocate::<ImageData>(1)?;
        budget.allocate::<usize>(2)?;
        budget.allocate::<(I, R)>(1)?;
        Ok(Self(Rc::new(ImageData {
            module,
            metadata,
            semantic,
            native,
            bindings,
            entry,
            resolve: Box::new(move |name| resolve(&owner, name)),
        })))
    }
    pub fn metadata(&self) -> &[u8] {
        &self.0.metadata
    }
    pub fn as_opaque(&self) -> *const c_void {
        std::ptr::from_ref(self).cast()
    }

    pub(crate) fn validate(
        &self,
        limits: embedding::Limits,
        budget: &mut m::Budget,
    ) -> Result<(), ImageError> {
        let (_, entry) = prepare(
            &self.0.metadata,
            &self.0.semantic,
            &self.0.native,
            &self.0.bindings,
            limits,
            budget,
        )?;
        if entry != self.0.entry {
            return Err(ImageError::Correspondence);
        }
        Ok(())
    }
    pub(crate) fn bind<'p>(
        &self,
        program: &'p program::Program<'p>,
        budget: &mut m::Budget,
    ) -> Result<tasks::LinkedProgram<'p, Rc<ImageData>>, ImageError> {
        // Exact immutable Unit identity supplies semantic correspondence only.
        // The actual Program and Instance still supply all dynamic authority.
        if !std::ptr::eq(program.bindings().unit(), self.0.module.unit()) {
            return Err(ImageError::Correspondence);
        }
        let record = native_record::Record::decode_with_budget(
            &self.0.native,
            &self.0.semantic,
            self.0.native.len().max(self.0.semantic.len()),
            budget,
        )
        .map_err(ImageError::Record)?;
        let object = tasks::RecordedObject::from_record(record)
            .map_err(ImageError::Record)?
            .prepare_component_with_budget(program, budget)
            .map_err(|e| ImageError::Plan(Box::new(e)))?;
        // SAFETY: the explicit factory retained the original matching resolver
        // and owner; this clone keeps their exact executable image alive.
        unsafe {
            object.bind_with_budget(
                self.0.clone(),
                |data, name| (data.resolve)(name).map(|p| p.as_ptr().cast_const().cast()),
                budget,
            )
        }
        .map_err(|e| ImageError::Plan(Box::new(e)))
    }
}
