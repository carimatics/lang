//! Trusted, exact-build object binding; no task package or executable wire format.
use super::*;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_object::{ObjectBuilder, ObjectModule};
use sha2::{Digest, Sha256};

mod recorded;
pub use recorded::RecordedObject;

fn object_prefix(proof: &o::tasks::VerifiedTasks, entry: m::FunctionId, component: bool) -> String {
    // Pairing identity only; trusted binding authenticates the actual image.
    let mut hash = Sha256::new();
    hash.update(super::super::sdk::build_id(&super::super::Triple::host()));
    if component {
        hash.update(b"checked-component-object-r1");
    }
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
    let namespace = if component {
        "lang_component_tasks_r1"
    } else {
        "lang_tasks_r1"
    };
    format!("{namespace}_{identity}")
}

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
        Self::emit_profile(proof, entry, limits, false, None)
    }
    /// Emit a trusted exact-build object for a checked component export.
    #[doc(hidden)]
    pub fn emit_component(
        program: &'p lang_runtime::component::program::Program<'p>,
        entry: m::FunctionId,
        limits: m::Limits,
    ) -> Result<Self, Error> {
        if !program.is_export(entry) {
            return Err(Error::Entry);
        }
        Self::emit_profile(program.proof(), entry, limits, true, Some(program))
    }
    fn emit_profile(
        proof: &'p o::tasks::VerifiedTasks,
        entry: m::FunctionId,
        limits: m::Limits,
        arguments: bool,
        component: Option<&'p lang_runtime::component::program::Program<'p>>,
    ) -> Result<Self, Error> {
        let mut flags = settings::builder();
        flags.set("is_pic", "true").map_err(backend)?;
        let isa = cranelift_native::builder()
            .map_err(backend)?
            .finish(settings::Flags::new(flags))
            .map_err(backend)?;
        let prefix = object_prefix(proof, entry, component.is_some());
        let mut module = ObjectModule::new(
            ObjectBuilder::new(isa, "lang-tasks-r1", default_libcall_names()).map_err(backend)?,
        );
        let (functions, _ids, budget) = if component.is_some() {
            prepare_program_profile(
                proof,
                entry,
                limits,
                &mut module,
                &prefix,
                arguments,
                component,
            )?
        } else {
            prepare_program(proof, entry, limits, &mut module, &prefix, arguments)?
        };
        let bytes = module.finish().emit().map_err(backend)?;
        Ok(Self {
            bytes,
            image: Image {
                component,
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
        self,
        owner: I,
        resolve: impl FnMut(&I, &str) -> Option<*const u8>,
        limits: m::Limits,
    ) -> Result<LinkedProgram<'p, I>, Error> {
        let mut budget = m::Budget::with_usage(limits, self.image.usage)?;
        // SAFETY: the caller supplies the exact same image/address/owner proof.
        unsafe { self.bind_with_budget(owner, resolve, &mut budget) }
    }

    /// Bind using the original ledger which already admitted reconstruction.
    /// Failed symbol lookup retains all previously admitted charges.
    ///
    /// # Safety
    /// The exact unmodified image, symbol addresses and retained owner must
    /// satisfy the same executable-code lifetime obligations as `Self::bind`.
    pub unsafe fn bind_with_budget<I>(
        mut self,
        owner: I,
        mut resolve: impl FnMut(&I, &str) -> Option<*const u8>,
        budget: &mut m::Budget,
    ) -> Result<LinkedProgram<'p, I>, Error> {
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
        self.image.usage = usage(budget);
        Ok(LinkedProgram {
            _owner: owner,
            image: self.image,
        })
    }
}
impl<I> LinkedProgram<'_, I> {
    #[doc(hidden)]
    pub fn execute_component<P: a::Provider>(
        &self,
        allocator: &mut a::Allocator<P>,
        control: e::Control<'_>,
        limits: r::Limits,
        arguments: &[r::EntryArgument],
        host: &mut dyn lang_runtime::component::execution::Host,
    ) -> Result<r::Outcome<lang_runtime::component::execution::Value>, r::Failure> {
        let Some(program) = self.image.component else {
            return Err(r::Failure {
                reason: r::Reason::Entry,
                function: Some(self.image.entry),
                span: None,
                callers: Some(lang_runtime::component::execution::GuestCallFrames {
                    frames: Vec::new(),
                    omitted: 0,
                }),
                usage: Default::default(),
                tasks: Default::default(),
                events: Vec::new(),
                report: None,
                poisoned: false,
            });
        };
        r::execute_component(
            &mut Adapter(&self.image),
            program,
            self.image.entry,
            allocator,
            control,
            limits,
            arguments,
            host,
        )
    }
    /// Lend the original cumulative execution-preparation budget.
    #[doc(hidden)]
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep original preparation authority explicit alongside the existing component operands."
    )]
    pub fn execute_component_with_budget<P: a::Provider>(
        &self,
        allocator: &mut a::Allocator<P>,
        control: e::Control<'_>,
        limits: r::Limits,
        arguments: &[r::EntryArgument],
        host: &mut dyn lang_runtime::component::execution::Host,
        budget: &mut m::Budget,
    ) -> Result<r::Outcome<lang_runtime::component::execution::Value>, r::Failure> {
        let Some(program) = self.image.component else {
            return self.execute_component(allocator, control, limits, arguments, host);
        };
        r::execute_component_with_budget(
            &mut Adapter(&self.image),
            program,
            self.image.entry,
            allocator,
            control,
            limits,
            arguments,
            host,
            budget,
        )
    }
    /// Run a synchronous component scope in the original borrowed task.
    #[doc(hidden)]
    #[allow(
        clippy::too_many_arguments,
        reason = "Original running-task authority accompanies component preparation and control."
    )]
    pub fn execute_component_in_task<P: a::Provider>(
        &self,
        allocator: &mut a::Allocator<P>,
        control: e::Control<'_>,
        limits: r::Limits,
        arguments: &[r::EntryArgument],
        host: &mut dyn lang_runtime::component::execution::Host,
        budget: &mut m::Budget,
        current: lang_runtime::task::CurrentTask<'_>,
    ) -> Result<r::Outcome<lang_runtime::component::execution::Value>, r::Failure> {
        let Some(program) = self.image.component else {
            return Err(r::Failure {
                reason: r::Reason::Entry,
                function: Some(self.image.entry),
                span: None,
                callers: Some(lang_runtime::component::execution::GuestCallFrames {
                    frames: Vec::new(),
                    omitted: 0,
                }),
                usage: Default::default(),
                tasks: current.usage(),
                events: Vec::new(),
                report: None,
                poisoned: false,
            });
        };
        r::execute_component_in_task(
            &mut Adapter(&self.image),
            program,
            self.image.entry,
            allocator,
            control,
            limits,
            arguments,
            host,
            budget,
            current,
        )
    }
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
