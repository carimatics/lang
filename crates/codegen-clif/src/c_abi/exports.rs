//! Native profile-1 export wrappers. Native MIR executes through the existing
//! checked CCompiledProgram; trusted runtime factories supply its typed storage
//! primitives, never a replacement guest body. Output publication follows the
//! shared context gate and completed cleanup.
use super::{allocator, context, scalar::NativeInvoker};
use crate::{OwnedJitModule, owned as n};
use cranelift_codegen::ir::{AbiParam, InstBuilder, StackSlotData, StackSlotKind, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use lang_mir::unit as m;
use lang_runtime::c_abi::{self as abi, context as c, validation as v};
use lang_runtime::{allocation as a, cleanup, execution as e};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
};

/// Same trusted primitive contract as owned::Runtime. prepare initializes only
/// admitted parameters in the exact compiled frame and uses the supplied real
/// allocator/environment. complete retires storage, propagates native/report
/// failure and writes canonical C result bytes only after successful cleanup.
/// prepare must roll back partial reservations before returning an error.
/// The optional result handle is borrowed staging owned by the wrapper; write
/// through the supplied allocator, but never release/retain this handle.
pub trait ExportRuntime<'program>: 'program {
    type State<'call>: n::Runtime
    where
        'program: 'call,
        Self: 'call;
    fn prepare<'call>(
        &'program self,
        layout: &'program n::FunctionLayout<'program>,
        instance: &'call mut cleanup::Instance,
        allocator: &'call mut a::Allocator<allocator::Provider<'program>>,
        environment: n::CEnvironment<'call>,
        arguments: &[v::Argument<'_>],
    ) -> Result<Self::State<'call>, c::Fault>
    where
        'program: 'call;
    fn complete<'call>(
        &self,
        runtime: &mut Self::State<'call>,
        failure: Option<n::Failure>,
        result: Option<a::Handle>,
    ) -> Result<(), c::Fault>
    where
        'program: 'call;
}
#[derive(Debug)]
pub enum Error {
    Target,
    Profile(usize),
    Resource(m::Error),
    Backend(String),
    SourceDebug(crate::SourceDebugError),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "C export preparation failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<m::Error> for Error {
    fn from(value: m::Error) -> Self {
        Self::Resource(value)
    }
}
fn backend(error: impl std::fmt::Display) -> Error {
    Error::Backend(error.to_string())
}

pub(crate) struct Dispatch<'p, F: ExportRuntime<'p>, C: n::CExecutable<'p>> {
    pub program: &'p c::Program<'p>,
    pub compiled: &'p C,
    pub factory: &'p F,
}
/// Generated C entries borrowing the registered program, machine code and
/// trusted runtime factory. Keep this owner alive through every entry call.
pub struct NativeExports<'p, F: ExportRuntime<'p>> {
    _dispatch: Box<Dispatch<'p, F, n::CCompiledProgram<'p>>>,
    _owner: OwnedJitModule,
    entries: Vec<(usize, String, *const u8)>,
}
impl<'p, F: ExportRuntime<'p>> NativeExports<'p, F> {
    /// Returned code pointers are invoked only under the same unsafe actual-C-
    /// object, owner-thread and code/user/context lifetime contract as ContextSupport.
    pub fn new(
        program: &'p c::Program<'p>,
        compiled: &'p n::CCompiledProgram<'p>,
        factory: &'p F,
        limits: m::Limits,
    ) -> Result<Self, Error> {
        let artifact = program.artifact();
        if artifact.digest() != compiled.artifact_digest() {
            return Err(Error::Profile(0));
        }
        let native = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            abi::Target::MacosAarch64
        } else if cfg!(all(
            target_os = "windows",
            target_arch = "x86_64",
            target_env = "msvc"
        )) {
            abi::Target::WindowsX86_64
        } else {
            return Err(Error::Target);
        };
        if artifact.target() != native {
            return Err(Error::Target);
        }
        let dispatch = Box::new(Dispatch {
            program,
            compiled,
            factory,
        });
        let mut budget = m::Budget::new(limits);
        let mut builder = JITBuilder::new(default_libcall_names()).map_err(backend)?;
        builder.symbol(
            "lang_c_export_profile1",
            invoke::<F, n::CCompiledProgram<'p>> as *const u8,
        );
        let mut owner = OwnedJitModule(Some(JITModule::new(builder)));
        let module = owner.0.as_mut().unwrap();
        let pointer = module.target_config().pointer_type();
        if pointer != types::I64 {
            return Err(Error::Target);
        }
        let mut helper_signature = module.make_signature();
        helper_signature.params.extend([AbiParam::new(pointer); 5]);
        helper_signature.returns.push(AbiParam::new(types::I32));
        let helper = module
            .declare_function("lang_c_export_profile1", Linkage::Import, &helper_signature)
            .map_err(backend)?;
        let mut ids = budget.vector(artifact.functions().len())?;
        for (index, function) in artifact.functions().iter().enumerate() {
            budget.charge(1)?;
            if function.direction != abi::Direction::Export {
                continue;
            }
            if compiled.exported_function(index).is_none() || function.parameters.len() > 256 {
                return Err(Error::Profile(index));
            }
            let unit = matches!(artifact.types()[function.result.0].kind, abi::Kind::Unit);
            let mut signature = module.make_signature();
            signature.params.push(AbiParam::new(pointer));
            for parameter in &function.parameters {
                signature.params.push(AbiParam::new(
                    if matches!(artifact.types()[parameter.0].kind, abi::Kind::Bool) {
                        types::I8
                    } else {
                        pointer
                    },
                ));
            }
            if !unit {
                signature.params.push(AbiParam::new(pointer));
            }
            signature.returns.push(AbiParam::new(types::I32));
            let id = module
                .declare_function(&function.symbol, Linkage::Export, &signature)
                .map_err(backend)?;
            let mut code = module.make_context();
            code.func.signature = signature;
            let helper = module.declare_func_in_func(helper, &mut code.func);
            let mut frontend = FunctionBuilderContext::new();
            let mut b = FunctionBuilder::new(&mut code.func, &mut frontend);
            let block = b.create_block();
            b.append_block_params_for_function_params(block);
            b.switch_to_block(block);
            let parameters = b.block_params(block).to_vec();
            let slot = b.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (function.parameters.len().max(1) * 8) as u32,
                3,
            ));
            for (i, parameter) in function.parameters.iter().enumerate() {
                let value = if matches!(artifact.types()[parameter.0].kind, abi::Kind::Bool) {
                    b.ins().uextend(types::I64, parameters[i + 1])
                } else {
                    parameters[i + 1]
                };
                b.ins().stack_store(pointer, value, slot, (i * 8) as i32);
            }
            let data = b.ins().iconst(
                pointer,
                (&*dispatch as *const Dispatch<'_, F, n::CCompiledProgram<'p>>).expose_provenance()
                    as i64,
            );
            let ordinal = b.ins().iconst(pointer, index as i64);
            let words = b.ins().stack_addr(pointer, slot, 0);
            let output = if unit {
                b.ins().iconst(pointer, 0)
            } else {
                *parameters.last().unwrap()
            };
            let call = b
                .ins()
                .call(helper, &[data, ordinal, parameters[0], words, output]);
            let result = b.inst_results(call)[0];
            b.ins().return_(&[result]);
            b.seal_all_blocks();
            b.finalize(module.target_config());
            module.define_function(id, &mut code).map_err(backend)?;
            ids.push((index, function.symbol.clone(), id));
        }
        module.finalize_definitions().map_err(backend)?;
        let mut entries = budget.vector(ids.len())?;
        for (index, name, id) in ids {
            entries.push((index, name, module.get_finalized_function(id)));
        }
        Ok(Self {
            _dispatch: dispatch,
            _owner: owner,
            entries,
        })
    }
    pub fn symbol(&self, function: usize) -> Option<(&str, *const u8)> {
        self.entries
            .iter()
            .find(|e| e.0 == function)
            .map(|e| (e.1.as_str(), e.2))
    }
}
struct RawReader {
    pointer: *const u8,
    size: usize,
}
impl v::Reader for RawReader {
    fn address(&self) -> v::Address {
        v::Address {
            start: self.pointer.addr(),
            extent: self.size,
        }
    }
    fn read(&self, offset: usize, destination: &mut [u8]) -> Result<(), v::Error> {
        if offset
            .checked_add(destination.len())
            .is_none_or(|end| end > self.size)
        {
            return Err(v::Error::Read {
                offset,
                bytes: destination.len(),
            });
        }
        // SAFETY: complete numeric preflight precedes every request. The unsafe
        // C caller supplies stable actual initialized field storage; padding is
        // requested by neither the validator nor typed transfer primitives.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.pointer.add(offset),
                destination.as_mut_ptr(),
                destination.len(),
            );
        }
        Ok(())
    }
}
struct Stage<'context, 'program> {
    allocator: &'context RefCell<allocator::CAllocator<'program>>,
    handle: Cell<Option<a::Handle>>,
    size: usize,
    alignment: u64,
}
impl Stage<'_, '_> {
    fn reserve(
        &self,
        allocator: &mut a::Allocator<allocator::Provider<'_>>,
    ) -> Result<(), c::Fault> {
        let request = a::Request::new(self.size as u64, self.alignment)
            .map_err(|_| c::Fault::new(c::Kind::Capacity))?;
        let handle = allocator.allocate(request).map_err(|_| {
            c::Fault::new(c::Kind::Execution(
                cleanup::ExecutionFailure::AllocationLimit,
            ))
        })?;
        self.handle.set(Some(handle));
        allocator
            .bytes_mut(handle)
            .map_err(|_| c::Fault::new(c::Kind::Allocation))?
            .fill(0);
        Ok(())
    }
    unsafe fn publish(&self, output: *mut u8) -> Result<(), c::Fault> {
        let allocator = self.allocator.borrow();
        let bytes = allocator
            .managed()
            .bytes(
                self.handle
                    .get()
                    .ok_or_else(|| c::Fault::new(c::Kind::ResultShape))?,
            )
            .map_err(|_| c::Fault::new(c::Kind::Allocation))?;
        // SAFETY: the Export gate admitted this disjoint writable actual C
        // object, validated this canonical staging and finished guest cleanup.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
        }
        Ok(())
    }
}
impl v::Reader for Stage<'_, '_> {
    fn address(&self) -> v::Address {
        let allocator = self.allocator.borrow();
        let bytes = self
            .handle
            .get()
            .and_then(|h| allocator.managed().bytes(h).ok());
        v::Address {
            start: bytes.map_or(0, |b| b.as_ptr().addr()),
            extent: self.size,
        }
    }
    fn read(&self, offset: usize, destination: &mut [u8]) -> Result<(), v::Error> {
        let error = || v::Error::Read {
            offset,
            bytes: destination.len(),
        };
        let allocator = self.allocator.borrow();
        let bytes = allocator
            .managed()
            .bytes(self.handle.get().ok_or_else(error)?)
            .map_err(|_| error())?;
        let end = offset.checked_add(destination.len()).ok_or_else(error)?;
        destination.copy_from_slice(bytes.get(offset..end).ok_or_else(error)?);
        Ok(())
    }
}
impl Drop for Stage<'_, '_> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // Runtime/factory borrows retire before this owner. The trusted
            // factory only borrows this token; it cannot release or retain it.
            // The admitted C deallocator has the existing no-unwind contract.
            if let Ok(mut allocator) = self.allocator.try_borrow_mut() {
                let _ = allocator.managed_mut().release(handle);
            }
        }
    }
}
pub(crate) unsafe extern "C" fn invoke<'p, F: ExportRuntime<'p>, C: n::CExecutable<'p>>(
    dispatch: *const Dispatch<'p, F, C>,
    index: u64,
    pointer: *mut c_void,
    words: *const u64,
    output: *mut u8,
) -> u32 {
    let operation = || -> Result<(), c::Fault> {
        // SAFETY: generated entry captures its stable owned dispatch and passes
        // its complete initialized argument-word stack reservation synchronously.
        let dispatch = unsafe { &*dispatch };
        let index = usize::try_from(index).map_err(|_| c::Fault::new(c::Kind::Capacity))?;
        let artifact = dispatch.program.artifact();
        let metadata = artifact
            .functions()
            .get(index)
            .ok_or_else(|| c::Fault::new(c::Kind::Function(index)))?;
        let function = dispatch
            .compiled
            .exported_function(index)
            .ok_or_else(|| c::Fault::new(c::Kind::Function(index)))?;
        let result = &artifact.types()[metadata.result.0];
        let unit = matches!(result.kind, abi::Kind::Unit);
        let mut readers = Vec::new();
        let mut arguments = Vec::new();
        readers
            .try_reserve_exact(metadata.parameters.len())
            .map_err(|_| c::Fault::new(c::Kind::Allocation))?;
        arguments
            .try_reserve_exact(metadata.parameters.len())
            .map_err(|_| c::Fault::new(c::Kind::Allocation))?;
        for (i, ty) in metadata.parameters.iter().enumerate() {
            // SAFETY: one initialized word exists for every generated parameter.
            let value = unsafe { words.add(i).read() };
            readers.push(RawReader {
                pointer: std::ptr::with_exposed_provenance(value as usize),
                size: artifact.types()[ty.0].size as usize,
            });
        }
        for (i, ty) in metadata.parameters.iter().enumerate() {
            // SAFETY: same complete generated argument-word reservation.
            let value = unsafe { words.add(i).read() };
            arguments.push(match artifact.types()[ty.0].kind {
                abi::Kind::I64 => v::Argument::I64(value as i64),
                abi::Kind::Bool => v::Argument::Bool(value as u8),
                _ => v::Argument::Object(&readers[i]),
            });
        }
        // SAFETY: unsafe C caller supplies a live synchronous opaque context;
        // the bridge rejects another registration before access to its allocator.
        unsafe {
            context::raw::with_allocator(
                dispatch.program,
                pointer,
                |context, allocator, address| {
                    let stage = Stage {
                        allocator,
                        handle: Cell::new(None),
                        size: result.size as usize,
                        alignment: result.align,
                    };
                    let control = e::Control::with_limits(context.limits().execution);
                    let work = e::ServiceWork::new(None);
                    let mut usage = e::Usage::default();
                    let point = c::EntryPoint::Export {
                        function: index,
                        call: c::Call {
                            span: None,
                            context: address,
                            arguments: &arguments,
                            output: if unit {
                                None
                            } else {
                                Some(v::Address {
                                    start: output.addr(),
                                    extent: result.size as usize,
                                })
                            },
                        },
                        result: if unit { None } else { Some(&stage) },
                    };
                    context.run(
                        dispatch.program,
                        control,
                        &work,
                        &mut usage,
                        point,
                        &mut |entry| {
                            let (instance, session) = entry.parts();
                            let mut allocator = allocator
                                .try_borrow_mut()
                                .map_err(|_| c::Fault::new(c::Kind::Reentry))?;
                            // SAFETY: copied code/user bindings retain their declared exact
                            // callback types and callbacks receive the ORIGINAL opaque pointer.
                            let mut invoker = match dispatch.compiled.linked_thunks() {
                                Some(thunks) => NativeInvoker::from_linked(
                                    context,
                                    address,
                                    thunks,
                                    Default::default(),
                                ),
                                None => {
                                    NativeInvoker::with_opaque(context, address, Default::default())
                                }
                            }
                            .map_err(|_| c::Fault::new(c::Kind::Allocation))?;
                            let environment = dispatch.compiled.prepare_c(
                                session,
                                &invoker,
                                allocator.managed(),
                            )?;
                            let layout = dispatch.compiled.layout(function).unwrap();
                            if session
                                .control()
                                .limits()
                                .live_cells
                                .is_some_and(|limit| layout.control_cells() > limit)
                            {
                                return Err(c::Fault::new(c::Kind::Execution(
                                    cleanup::ExecutionFailure::CellLimit,
                                )));
                            }
                            if !unit {
                                stage.reserve(allocator.managed_mut())?;
                            }
                            let state = dispatch.factory.prepare(
                                layout,
                                instance,
                                allocator.managed_mut(),
                                environment,
                                &arguments,
                            )?;
                            let mut runtime = n::RuntimeInstance::new(state);
                            let failure = dispatch
                                .compiled
                                .execute_export(function, session, &mut invoker, &mut runtime)
                                .err();
                            dispatch.factory.complete(
                                runtime.runtime_mut(),
                                failure,
                                stage.handle.get(),
                            )?;
                            // Runtime cleanup/result adapters cannot turn a
                            // failed machine-code invocation into C success.
                            failure.map_or(Ok(()), |failure| Err(native_fault(failure)))
                        },
                    )?;
                    if !unit {
                        // SAFETY: all Export gate validation and cleanup completed.
                        stage.publish(output)?;
                    }
                    Ok(())
                },
            )
        }
    };
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => result.map_or_else(|fault| fault.status().code(), |()| 0),
        Err(payload) => c::Fault::host_panic(payload).status().code(),
    }
}

fn native_fault(failure: n::Failure) -> c::Fault {
    if let n::Reason::Foreign(fault) = failure.reason {
        return fault;
    }
    let kind = match failure.reason {
        n::Reason::Poisoned => c::Kind::Poisoned,
        n::Reason::Runtime(cleanup::Cause::Execution(reason)) => c::Kind::Execution(reason),
        _ => c::Kind::Execution(cleanup::ExecutionFailure::Fault),
    };
    let mut fault = c::Fault::new(kind).at(failure.span);
    fault.panic_code = failure.panic_code.unwrap_or(0);
    fault
}
