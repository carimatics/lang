//! RFC 0015 private, trusted, in-memory C object binding. The object contains
//! relocatable code only; a retained binding publishes the synchronous bridge.
use super::{context, exports, scalar};
use crate::owned as n;
use cranelift_codegen::ir::{
    AbiParam, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::ObjectModule;
pub use exports::Error;
use lang_mir::unit as m;
use lang_runtime::c_abi::{self as abi, context as c};
use std::{ffi::c_void, ptr::NonNull};

fn backend(error: impl std::fmt::Display) -> Error {
    Error::Backend(error.to_string())
}

pub(crate) struct Symbols {
    pub slot: String,
    pub support: [String; 3],
    pub exports: Vec<(usize, String)>,
    pub thunks: Vec<(usize, String)>,
}

pub(crate) fn emit(
    module: &mut ObjectModule,
    artifact: &abi::Artifact,
    budget: &mut m::Budget,
) -> Result<Symbols, Error> {
    let pointer = module.target_config().pointer_type();
    if pointer != types::I64 {
        return Err(Error::Target);
    }
    let slot = format!("lang_c_obj1_slot_{}", artifact.digest());
    budget.allocate::<u8>(slot.len())?;
    let data = module
        .declare_data(&slot, Linkage::Export, true, false)
        .map_err(backend)?;
    let mut description = DataDescription::new();
    description.define_zeroinit(8);
    description.set_align(8);
    module.define_data(data, &description).map_err(backend)?;
    let support = ["create", "destroy", "last_error"]
        .map(|name| format!("lang_context_{name}_{}", artifact.digest()));
    let mut exports = budget.vector(artifact.functions().len())?;
    // (symbol, selected callback offset, source-export ordinal). Support
    // signatures have one or two arguments; exports preserve profile 1 exactly.
    let mut entries = budget.vector(artifact.functions().len() + 3)?;
    for (i, name) in support.iter().enumerate() {
        budget.allocate::<u8>(name.len())?;
        entries.push((name.clone(), 16 + i * 8, None));
    }
    for (index, function) in artifact.functions().iter().enumerate() {
        if function.direction == abi::Direction::Export {
            if function.parameters.len() > 256 {
                return Err(Error::Profile(index));
            }
            entries.push((function.symbol.clone(), 40, Some(index)));
            budget.allocate::<u8>(function.symbol.len())?;
            exports.push((index, function.symbol.clone()));
        }
    }
    for (symbol, offset, ordinal) in entries {
        budget.charge(64 + ordinal.map_or(0, |i| artifact.functions()[i].parameters.len() * 4))?;
        budget.allocate::<u8>(symbol.len())?;
        let mut signature = module.make_signature();
        let unit = ordinal.is_some_and(|i| {
            matches!(
                artifact.types()[artifact.functions()[i].result.0].kind,
                abi::Kind::Unit
            )
        });
        signature.params.push(AbiParam::new(pointer));
        if let Some(i) = ordinal {
            for ty in &artifact.functions()[i].parameters {
                signature.params.push(AbiParam::new(
                    if matches!(artifact.types()[ty.0].kind, abi::Kind::Bool) {
                        types::I8
                    } else {
                        pointer
                    },
                ));
            }
            if !unit {
                signature.params.push(AbiParam::new(pointer));
            }
        } else if offset != 24 {
            signature.params.push(AbiParam::new(pointer));
        }
        signature.returns.push(AbiParam::new(types::I32));
        let id = module
            .declare_function(&symbol, Linkage::Export, &signature)
            .map_err(backend)?;
        let mut callback_signature = module.make_signature();
        callback_signature.params.extend(
            (0..if ordinal.is_some() {
                5
            } else {
                signature.params.len() + 1
            })
                .map(|_| AbiParam::new(pointer)),
        );
        callback_signature.returns.push(AbiParam::new(types::I32));
        let mut code = module.make_context();
        code.func.signature = signature;
        let global = module.declare_data_in_func(data, &mut code.func);
        let mut frontend = FunctionBuilderContext::new();
        let mut b = FunctionBuilder::new(&mut code.func, &mut frontend);
        let entry = b.create_block();
        let header_block = b.create_block();
        let callback_block = b.create_block();
        let ready = b.create_block();
        let invalid = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let parameters = b.block_params(entry).to_vec();
        let address = b.ins().symbol_value(pointer, global);
        let header = b.ins().load(pointer, MemFlagsData::new(), address, 0);
        b.ins().brif(header, header_block, &[], invalid, &[]);
        b.switch_to_block(header_block);
        // The unsafe binder retains an actual immutable 48-byte header. Check
        // its version and extent before reading any pointer or caller object.
        let version = b.ins().load(types::I32, MemFlagsData::new(), header, 0);
        let size = b.ins().load(types::I32, MemFlagsData::new(), header, 4);
        let version =
            b.ins()
                .icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::Equal, version, 1);
        let size = b
            .ins()
            .icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::Equal, size, 48);
        let valid = b.ins().band(version, size);
        b.ins().brif(valid, callback_block, &[], invalid, &[]);
        b.switch_to_block(callback_block);
        let dispatch = b.ins().load(pointer, MemFlagsData::new(), header, 8);
        let callback = b
            .ins()
            .load(pointer, MemFlagsData::new(), header, offset as i32);
        let dispatch_valid = b.ins().icmp_imm_u(
            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
            dispatch,
            0,
        );
        let callback_valid = b.ins().icmp_imm_u(
            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
            callback,
            0,
        );
        let valid = b.ins().band(dispatch_valid, callback_valid);
        b.ins().brif(valid, ready, &[], invalid, &[]);
        b.switch_to_block(ready);
        let mut arguments = budget.vector(5)?;
        arguments.push(dispatch);
        if let Some(i) = ordinal {
            let function = &artifact.functions()[i];
            let words = b.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (function.parameters.len().max(1) * 8) as u32,
                3,
            ));
            for (index, ty) in function.parameters.iter().enumerate() {
                let value = if matches!(artifact.types()[ty.0].kind, abi::Kind::Bool) {
                    b.ins().uextend(types::I64, parameters[index + 1])
                } else {
                    parameters[index + 1]
                };
                b.ins()
                    .stack_store(pointer, value, words, (index * 8) as i32);
            }
            arguments.push(b.ins().iconst(pointer, i as i64));
            arguments.push(parameters[0]);
            arguments.push(b.ins().stack_addr(pointer, words, 0));
            arguments.push(if unit {
                b.ins().iconst(pointer, 0)
            } else {
                *parameters.last().unwrap()
            });
        } else {
            arguments.extend_from_slice(&parameters);
        }
        let callback_signature = b.func.import_signature(callback_signature);
        let call = b
            .ins()
            .call_indirect(callback_signature, callback, &arguments);
        let result = b.inst_results(call)[0];
        b.ins().return_(&[result]);
        b.switch_to_block(invalid);
        let status = b.ins().iconst(types::I32, 1);
        b.ins().return_(&[status]);
        b.seal_all_blocks();
        b.finalize(module.target_config());
        module.define_function(id, &mut code).map_err(backend)?;
    }
    let emitted_thunks =
        scalar::emit_thunks(module, artifact, budget, scalar::ThunkProfile::Object)
            .map_err(backend)?;
    let mut thunks = budget.vector(emitted_thunks.len())?;
    for (i, _, symbol) in emitted_thunks {
        thunks.push((i, symbol));
    }
    Ok(Symbols {
        slot,
        support,
        exports,
        thunks,
    })
}

#[repr(C)]
pub(crate) struct Header {
    version: u32,
    size: u32,
    data: *const c_void,
    create:
        unsafe extern "C" fn(*const c_void, *const context::ProgramConfig, *mut *mut c_void) -> u32,
    destroy: unsafe extern "C" fn(*const c_void, *mut c_void) -> u32,
    query: unsafe extern "C" fn(*const c_void, *mut c_void, *mut c::ErrorSummary) -> u32,
    invoke: unsafe extern "C" fn(*const c_void, u64, *mut c_void, *const u64, *mut u8) -> u32,
}
const _: () = assert!(std::mem::size_of::<Header>() == 48 && std::mem::align_of::<Header>() == 8);

/// An active binding borrows the actual registered Program, linked code and
/// runtime factory. It owns the stationary bridge and clears the slot on drop.
pub struct ObjectExports<'p, F: exports::ExportRuntime<'p>, Image: 'p> {
    _dispatch: Box<exports::Dispatch<'p, F, n::CLinkedProgram<'p, Image>>>,
    _header: Box<Header>,
    compiled: &'p n::CLinkedProgram<'p, Image>,
    slot: NonNull<*const Header>,
}
impl<'p, F: exports::ExportRuntime<'p>, Image: 'p> ObjectExports<'p, F, Image> {
    /// # Safety
    /// The image was bound to this artifact's actual unchanged code/data.
    /// Calls are synchronous on the owning thread. Every call must return and
    /// every context must be destroyed before this binding is dropped. The
    /// slot may only be read/written by this binder and its matching wrappers;
    /// no concurrent call, slot mutation, rebind or unload is permitted.
    pub unsafe fn new(
        program: &'p c::Program<'p>,
        compiled: &'p n::CLinkedProgram<'p, Image>,
        factory: &'p F,
    ) -> Result<Self, Error> {
        use n::NativeProgram;
        if program.artifact().digest() != compiled.artifact_digest()
            || !program.retains_code(compiled)
        {
            return Err(Error::Profile(0));
        }
        let slot = compiled.slot;
        // SAFETY: trusted matched image provides a writable aligned pointer slot.
        if !unsafe { slot.as_ptr().read() }.is_null() {
            return Err(Error::Profile(0));
        }
        let dispatch = Box::new(exports::Dispatch {
            program,
            compiled,
            factory,
        });
        let header = Box::new(Header {
            version: 1,
            size: 48,
            data: (&*dispatch as *const exports::Dispatch<'_, F, n::CLinkedProgram<'_, Image>>)
                .cast(),
            create: create::<F, Image>,
            destroy: destroy::<F, Image>,
            query: query::<F, Image>,
            invoke: invoke::<F, Image>,
        });
        // SAFETY: all validation/reservation succeeded; both boxes are stationary
        // and their borrowed owners survive the complete active binding.
        unsafe {
            slot.as_ptr().write(&*header);
        }
        Ok(Self {
            _dispatch: dispatch,
            _header: header,
            compiled,
            slot,
        })
    }
    pub fn create(&self) -> context::Create {
        // SAFETY: the matched image contains the object-emitted profile-1
        // create entry with this exact target C signature; its owner is retained.
        unsafe { std::mem::transmute(self.compiled.support[0]) }
    }
    pub fn destroy(&self) -> context::Destroy {
        // SAFETY: support[1] is the matched retained profile-1 destroy entry.
        unsafe { std::mem::transmute(self.compiled.support[1]) }
    }
    pub fn last_error(&self) -> context::LastError {
        // SAFETY: support[2] is the matched retained profile-1 query entry.
        unsafe { std::mem::transmute(self.compiled.support[2]) }
    }
    pub fn symbol(&self, function: usize) -> Option<(&str, *const u8)> {
        self.compiled.symbol(function)
    }
}
impl<'p, F: exports::ExportRuntime<'p>, Image: 'p> Drop for ObjectExports<'p, F, Image> {
    fn drop(&mut self) {
        // SAFETY: the constructor requires no outstanding calls or contexts at
        // drop. Revoke publication before either retained box can be freed.
        unsafe {
            self.slot.as_ptr().write(std::ptr::null());
        }
    }
}
unsafe extern "C" fn create<'p, F: exports::ExportRuntime<'p>, Image: 'p>(
    data: *const c_void,
    config: *const context::ProgramConfig,
    output: *mut *mut c_void,
) -> u32 {
    // SAFETY: the retained immutable header selects this exact dispatch type.
    let dispatch =
        unsafe { &*data.cast::<exports::Dispatch<'p, F, n::CLinkedProgram<'p, Image>>>() };
    unsafe { context::raw::create(dispatch.program, config, output) }
}
unsafe extern "C" fn destroy<'p, F: exports::ExportRuntime<'p>, Image: 'p>(
    data: *const c_void,
    context: *mut c_void,
) -> u32 {
    // SAFETY: exact live dispatch type, actual opaque context contract.
    let dispatch =
        unsafe { &*data.cast::<exports::Dispatch<'p, F, n::CLinkedProgram<'p, Image>>>() };
    unsafe { context::raw::destroy(dispatch.program, context) }
}
unsafe extern "C" fn query<'p, F: exports::ExportRuntime<'p>, Image: 'p>(
    data: *const c_void,
    context: *mut c_void,
    output: *mut c::ErrorSummary,
) -> u32 {
    // SAFETY: exact live dispatch type and admitted C output contract.
    let dispatch =
        unsafe { &*data.cast::<exports::Dispatch<'p, F, n::CLinkedProgram<'p, Image>>>() };
    unsafe { context::raw::last_error(dispatch.program, context, output) }
}
unsafe extern "C" fn invoke<'p, F: exports::ExportRuntime<'p>, Image: 'p>(
    data: *const c_void,
    index: u64,
    context: *mut c_void,
    words: *const u64,
    output: *mut u8,
) -> u32 {
    // SAFETY: generated wrapper/retained header provide this exact dispatch,
    // ordinal and initialized bounded word reservation synchronously.
    unsafe {
        exports::invoke(
            data.cast::<exports::Dispatch<'p, F, n::CLinkedProgram<'p, Image>>>(),
            index,
            context,
            words,
            output,
        )
    }
}
