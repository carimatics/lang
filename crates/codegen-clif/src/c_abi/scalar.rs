//! Exact native C-signature thunks for the first scalar/Unit import profile.
//! Guest execution still goes through the shared context Session. No callback
//! symbol lookup occurs here: only its explicitly granted typed code pointer
//! reaches an indirect call with the verified generated C signature.
use crate::OwnedJitModule;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use lang_mir::unit as m;
use lang_runtime::c_abi::context::{
    AllocatorIdentity, Context,
    config::{BoundImport, ErasedFunction},
    scalar::{Input, Invoker, Output},
};
use lang_runtime::c_abi::validation::{Address, Reader};
use lang_runtime::c_abi::{Digest, Direction, Kind, Target};
use std::ffi::c_void;

#[derive(Debug)]
pub enum Error {
    Target,
    Signature(usize),
    Resource(m::Error),
    Backend(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "C import preparation failed: {self:?}")
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
type Entry =
    unsafe extern "C" fn(ErasedFunction, *mut c_void, *mut c_void, *const u64, *mut c_void) -> u32;
struct Thunk {
    function: usize,
    code: *const u8,
}
pub struct NativeInvoker<'a> {
    context: &'a Context<'a>,
    thunks: Vec<Thunk>,
    _owner: OwnedJitModule,
}
impl<'a> NativeInvoker<'a> {
    /// # Safety
    /// All context import pointers have the exact generated callback type for
    /// their signature digest. Code/user data remain live for this borrow;
    /// callbacks obey synchronous no-unwind/nonreentry and actual-object rules.
    /// They treat the context as opaque and do not directly mutate Rust bytes.
    pub unsafe fn new(context: &'a Context<'a>, limits: m::Limits) -> Result<Self, Error> {
        let artifact = context.artifact();
        let native = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
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
        if artifact.target() != native {
            return Err(Error::Target);
        }
        let mut budget = m::Budget::new(limits);
        // Check the complete import profile before code publication. Unused
        // declarations are not an escape from supported-signature admission.
        for (index, function) in artifact.functions().iter().enumerate() {
            budget.charge(1)?;
            if function.direction == Direction::Import
                && (function.parameters.len() > 256
                    || function
                        .parameters
                        .iter()
                        .chain(std::iter::once(&function.result))
                        .any(|ty| {
                            !matches!(
                                artifact.types()[ty.0].kind,
                                Kind::I64 | Kind::Bool | Kind::Unit
                            )
                        }))
            {
                return Err(Error::Signature(index));
            }
        }
        let mut thunks = budget.vector(artifact.functions().len())?;
        let mut owner = OwnedJitModule(Some(JITModule::new(
            JITBuilder::new(default_libcall_names()).map_err(backend)?,
        )));
        let module = owner.0.as_mut().expect("new C thunk module");
        let pointer = module.target_config().pointer_type();
        if pointer != types::I64 {
            return Err(Error::Target);
        }
        let mut entries = budget.vector(artifact.functions().len())?;
        for (index, function) in artifact.functions().iter().enumerate() {
            if function.direction != Direction::Import {
                continue;
            }
            budget.charge(32 + function.parameters.len() * 8)?;
            let mut signature = module.make_signature();
            signature.params.extend([AbiParam::new(pointer); 5]);
            signature.returns.push(AbiParam::new(types::I32));
            let mut callback = module.make_signature();
            callback.params.extend([AbiParam::new(pointer); 2]);
            for ty in &function.parameters {
                callback
                    .params
                    .push(if matches!(artifact.types()[ty.0].kind, Kind::Bool) {
                        AbiParam::new(types::I8).uext()
                    } else {
                        AbiParam::new(pointer)
                    });
            }
            let unit = matches!(artifact.types()[function.result.0].kind, Kind::Unit);
            if !unit {
                callback.params.push(AbiParam::new(pointer));
            }
            callback.returns.push(AbiParam::new(types::I32));
            let id = module
                .declare_function(
                    &format!("lang_c_import_thunk_{}", function.digest),
                    Linkage::Local,
                    &signature,
                )
                .map_err(backend)?;
            let mut code = module.make_context();
            code.func.signature = signature;
            let mut builder_context = FunctionBuilderContext::new();
            {
                let mut b = FunctionBuilder::new(&mut code.func, &mut builder_context);
                let entry = b.create_block();
                b.append_block_params_for_function_params(entry);
                b.switch_to_block(entry);
                let p: Vec<_> = b.block_params(entry).to_vec();
                let mut arguments = budget.vector(function.parameters.len() + 3)?;
                arguments.extend([p[1], p[2]]);
                for (argument, ty) in function.parameters.iter().enumerate() {
                    let bits =
                        b.ins()
                            .load(types::I64, MemFlagsData::new(), p[3], (argument * 8) as i32);
                    let bits = if matches!(artifact.types()[ty.0].kind, Kind::Bool) {
                        b.ins().ireduce(types::I8, bits)
                    } else {
                        bits
                    };
                    arguments.push(bits);
                }
                if !unit {
                    arguments.push(p[4]);
                }
                let callback = b.func.import_signature(callback);
                let call = b.ins().call_indirect(callback, p[0], &arguments);
                let status = b.inst_results(call)[0];
                b.ins().return_(&[status]);
                b.seal_all_blocks();
                b.finalize(module.target_config());
            }
            module.define_function(id, &mut code).map_err(backend)?;
            entries.push((index, id));
        }
        module.finalize_definitions().map_err(backend)?;
        for (function, id) in entries {
            thunks.push(Thunk {
                function,
                code: module.get_finalized_function(id),
            });
        }
        Ok(Self {
            context,
            thunks,
            _owner: owner,
        })
    }
}
impl Invoker for NativeInvoker<'_> {
    fn artifact(&self) -> Digest {
        self.context.artifact().digest()
    }
    fn allocator_identity(&self) -> AllocatorIdentity {
        self.context.allocator_identity()
    }
    fn context(&self) -> Address {
        Address {
            start: (self.context as *const Context<'_>).addr(),
            extent: std::mem::size_of_val(self.context),
        }
    }
    fn invoke(
        &mut self,
        binding: &BoundImport,
        arguments: &[Input<'_>],
        output: Output<'_>,
    ) -> u32 {
        // Keep this trait method safe even if a trusted Rust host calls it
        // directly with a different context's binding or malformed arguments.
        if !self
            .context
            .import(binding.index())
            .is_some_and(|expected| std::ptr::eq(expected, binding))
        {
            return 1;
        }
        let Ok(index) = self
            .thunks
            .binary_search_by_key(&binding.index(), |thunk| thunk.function)
        else {
            return 1;
        };
        let artifact = self.context.artifact();
        let function = &artifact.functions()[binding.index()];
        if arguments.len() != function.parameters.len() || binding.signature() != function.digest {
            return 1;
        }
        let mut words = [0u64; 256];
        for (index, (argument, ty)) in arguments.iter().zip(&function.parameters).enumerate() {
            words[index] = match (argument, &artifact.types()[ty.0].kind) {
                (Input::I64(value), Kind::I64) => *value as u64,
                (Input::Bool(value @ 0..=1), Kind::Bool) => u64::from(*value),
                (Input::Unit(unit), Kind::Unit) => unit.address().start as u64,
                _ => return 1,
            };
        }
        let result = match (output, &artifact.types()[function.result.0].kind) {
            (Output::I64(value), Kind::I64) => value.as_ptr().cast(),
            (Output::Bool(value), Kind::Bool) => value.as_ptr().cast(),
            (Output::Unit, Kind::Unit) => std::ptr::null_mut(),
            _ => return 1,
        };
        // SAFETY: code was emitted with exactly Entry's five native parameters
        // and u32 result. Its owner remains borrowed through this call. The C
        // function pointer stays a typed CODE pointer, never a void* data cast.
        let entry: Entry = unsafe { std::mem::transmute(self.thunks[index].code) };
        // SAFETY: exact binding/profile/shape checks above, initialized local
        // words/output cells and the constructor's code/user/opaque-context
        // contract establish every native argument's lifetime and ABI.
        unsafe {
            entry(
                binding.function(),
                (self.context as *const Context<'_>).cast_mut().cast(),
                binding.user(),
                words.as_ptr(),
                result,
            )
        }
    }
}
