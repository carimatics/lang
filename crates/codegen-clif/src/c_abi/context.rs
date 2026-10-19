//! Generated profile-1 C context support. The shared safe kernel owns admission
//! and state; this module reads actual C descriptors and publishes opaque boxes.
//! No source/bytecode loader or guest export is supplied by these three entries.
use crate::OwnedJitModule;
use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use lang_mir::unit as m;
use lang_runtime::c_abi::{Target, context as gate};
use std::ffi::c_void;

pub(crate) mod raw;
pub use raw::{ImportBinding, ProgramConfig, with_context};

pub type Create = unsafe extern "C" fn(*const ProgramConfig, *mut *mut c_void) -> u32;
pub type Destroy = unsafe extern "C" fn(*mut c_void) -> u32;
pub type LastError = unsafe extern "C" fn(*mut c_void, *mut gate::ErrorSummary) -> u32;

#[derive(Debug)]
pub enum Error {
    Target,
    Resource(m::Error),
    Backend(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "C context support preparation failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<m::Error> for Error {
    fn from(error: m::Error) -> Self {
        Self::Resource(error)
    }
}
fn backend(error: impl std::fmt::Display) -> Error {
    Error::Backend(error.to_string())
}

/// Retains the registered artifact/code owner through its Program borrow and
/// owns the generated create/destroy/last_error entry code. Calls through the
/// returned unsafe C function pointers must finish, and every created context
/// must be destroyed, before this support or its Program/code/user owner dies.
/// Calls are synchronous on the Program/context owner thread. Numeric-invalid
/// addresses may be supplied for rejection; every range actually accessed after
/// admission must be a complete live initialized C input or writable output.
/// Allocator and import callbacks retain the RFC 0008 exact-signature contract.
pub struct ContextSupport<'a> {
    _program: &'a gate::Program<'a>,
    _owner: OwnedJitModule,
    entries: [*const u8; 3],
    symbols: [String; 3],
}
impl<'a> ContextSupport<'a> {
    pub fn new(program: &'a gate::Program<'a>, limits: m::Limits) -> Result<Self, Error> {
        let target = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Target::MacosAarch64
        } else if cfg!(all(
            target_os = "windows",
            target_arch = "x86_64",
            target_env = "msvc"
        )) {
            Target::WindowsX86_64
        } else {
            return Err(Error::Target);
        };
        if program.artifact().target() != target {
            return Err(Error::Target);
        }
        let mut budget = m::Budget::new(limits);
        let mut builder = JITBuilder::new(default_libcall_names()).map_err(backend)?;
        builder.symbol("lang_c_create_profile1", raw::create as *const u8);
        builder.symbol("lang_c_destroy_profile1", raw::destroy as *const u8);
        builder.symbol("lang_c_last_error_profile1", raw::last_error as *const u8);
        let mut owner = OwnedJitModule(Some(JITModule::new(builder)));
        let module = owner.0.as_mut().expect("new context support module");
        let pointer = module.target_config().pointer_type();
        if pointer != types::I64 {
            return Err(Error::Target);
        }
        let mut symbols = [String::new(), String::new(), String::new()];
        let mut ids = budget.vector(3)?;
        for (index, name) in ["create", "destroy", "last_error"].into_iter().enumerate() {
            budget.charge(1)?;
            symbols[index] = format!("lang_context_{name}_{}", program.artifact().digest());
            budget.allocate::<u8>(symbols[index].len())?;
            let count = if index == 1 { 1 } else { 2 };
            let mut signature = module.make_signature();
            signature
                .params
                .extend((0..count).map(|_| AbiParam::new(pointer)));
            signature.returns.push(AbiParam::new(types::I32));
            let entry = module
                .declare_function(&symbols[index], Linkage::Export, &signature)
                .map_err(backend)?;
            let mut helper_signature = signature.clone();
            helper_signature.params.insert(0, AbiParam::new(pointer));
            let helper = module
                .declare_function(
                    &format!("lang_c_{name}_profile1"),
                    Linkage::Import,
                    &helper_signature,
                )
                .map_err(backend)?;
            let mut context = module.make_context();
            context.func.signature = signature;
            let helper = module.declare_func_in_func(helper, &mut context.func);
            let mut frontend = FunctionBuilderContext::new();
            let mut b = FunctionBuilder::new(&mut context.func, &mut frontend);
            let block = b.create_block();
            b.append_block_params_for_function_params(block);
            b.switch_to_block(block);
            let mut arguments = budget.vector(count + 1)?;
            // The Program borrow keeps this actual data object stationary and
            // live throughout every generated call and opaque-context lifetime.
            arguments.push(b.ins().iconst(
                pointer,
                (program as *const gate::Program<'_>).expose_provenance() as i64,
            ));
            arguments.extend_from_slice(b.block_params(block));
            let call = b.ins().call(helper, &arguments);
            let result = b.inst_results(call)[0];
            b.ins().return_(&[result]);
            b.seal_all_blocks();
            b.finalize(module.target_config());
            module
                .define_function(entry, &mut context)
                .map_err(backend)?;
            ids.push(entry);
        }
        module.finalize_definitions().map_err(backend)?;
        let entries = std::array::from_fn(|i| module.get_finalized_function(ids[i]));
        Ok(Self {
            _program: program,
            _owner: owner,
            entries,
            symbols,
        })
    }
    pub fn symbols(&self) -> &[String; 3] {
        &self.symbols
    }
    pub fn create(&self) -> Create {
        // SAFETY: entry 0 was emitted/finalized with exactly this native C
        // signature. Invoking it remains unsafe and requires the owner lifetime.
        unsafe { std::mem::transmute(self.entries[0]) }
    }
    pub fn destroy(&self) -> Destroy {
        // SAFETY: entry 1 has this exact C signature and retained code owner.
        unsafe { std::mem::transmute(self.entries[1]) }
    }
    pub fn last_error(&self) -> LastError {
        // SAFETY: entry 2 has this exact C signature and retained code owner.
        unsafe { std::mem::transmute(self.entries[2]) }
    }
}
