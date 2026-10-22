//! Trusted, exact-build object binding; no task package or executable wire format.
use super::*;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_object::{ObjectBuilder, ObjectModule};
use sha2::{Digest, Sha256};

pub struct ObjectArtifact<'p> {
    bytes: Vec<u8>,
    image: Image<'p>,
}
pub struct LinkedProgram<'p, I> {
    _owner: I,
    image: Image<'p>,
}
impl<'p> ObjectArtifact<'p> {
    /// Emit for the current host's target and private task runtime revision.
    pub fn emit(proof: &'p o::tasks::VerifiedTasks, limits: m::Limits) -> Result<Self, Error> {
        let entry = proof
            .typed()
            .places()
            .declarations()
            .functions()
            .iter()
            .position(|f| f.name == "main")
            .map(|i| m::FunctionId(i as u32))
            .ok_or(Error::Entry)?;
        Self::emit_function(proof, entry, limits)
    }
    pub fn emit_function(
        proof: &'p o::tasks::VerifiedTasks,
        entry: m::FunctionId,
        limits: m::Limits,
    ) -> Result<Self, Error> {
        let mut flags = settings::builder();
        flags.set("is_pic", "true").map_err(backend)?;
        let isa = cranelift_native::builder()
            .map_err(backend)?
            .finish(settings::Flags::new(flags))
            .map_err(backend)?;
        // Namespacing avoids collisions between source units; it is not code
        // authentication. Binding still requires the exact trusted object.
        let mut hash = Sha256::new();
        hash.update(super::super::sdk::build_id(&super::super::Triple::host()));
        hash.update(entry.0.to_le_bytes());
        for source in &proof
            .typed()
            .places()
            .declarations()
            .types()
            .table()
            .sources
        {
            hash.update((source.text.len() as u64).to_le_bytes());
            hash.update(source.text.as_bytes());
        }
        let identity: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        let prefix = format!("lang_tasks_r1_{identity}");
        let mut module = ObjectModule::new(
            ObjectBuilder::new(isa, "lang-tasks-r1", default_libcall_names()).map_err(backend)?,
        );
        let (functions, _ids, budget) =
            prepare_program(proof, entry, limits, &mut module, &prefix, false)?;
        let bytes = module.finish().emit().map_err(backend)?;
        Ok(Self {
            bytes,
            image: Image {
                component: None,
                proof,
                entry,
                functions,
                usage: usage(&budget),
            },
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn usage(&self) -> m::Usage {
        self.image.usage
    }
    pub fn metadata(&self) -> impl Iterator<Item = &super::super::program::FunctionCode> {
        self.image.functions.iter().flatten().map(|f| &f.metadata)
    }
    /// Bind functions from the exact emitted object before guest admission.
    ///
    /// # Safety
    /// The image must contain this exact unmodified object, emitted from this
    /// proof by the same compiler/SDK build for this host. Each resolved address
    /// must name its matching generated function and remain executable until
    /// the returned program is dropped. The owner must keep that code alive.
    /// These are trusted native inputs, not a sandbox for foreign machine code.
    pub unsafe fn bind<I>(
        mut self,
        owner: I,
        mut resolve: impl FnMut(&I, &str) -> Option<*const u8>,
        limits: m::Limits,
    ) -> Result<LinkedProgram<'p, I>, Error> {
        let mut budget = m::Budget::with_usage(limits, self.image.usage)?;
        for function in self.image.functions.iter_mut().flatten() {
            budget.charge(1)?;
            function.code = resolve(&owner, &function.metadata.symbol)
                .filter(|p| !p.is_null())
                .ok_or_else(|| {
                    backend(format!(
                        "missing task object symbol {}",
                        function.metadata.symbol
                    ))
                })?;
        }
        self.image.usage = usage(&budget);
        Ok(LinkedProgram {
            _owner: owner,
            image: self.image,
        })
    }
}
impl<I> LinkedProgram<'_, I> {
    pub fn usage(&self) -> m::Usage {
        self.image.usage
    }
    pub fn execute<P: a::Provider>(
        &self,
        allocator: &mut a::Allocator<P>,
        control: e::Control<'_>,
        limits: r::Limits,
    ) -> Result<r::Outcome, r::Failure> {
        r::execute_function(
            &mut Adapter(&self.image),
            self.image.proof,
            self.image.entry,
            allocator,
            control,
            limits,
        )
    }
}
