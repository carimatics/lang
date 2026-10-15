use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use lang_mir::{Function, HostFunctionId, Instruction, Terminator, Type, ValidationError};
use lang_runtime::HostFunctionRegistry;

pub struct CompiledFunction {
    _module: JITModule,
    code: *const u8,
    code_size: usize,
    parameter_count: usize,
}

impl CompiledFunction {
    pub fn compile(function: &Function) -> Result<Self, CompileError> {
        Self::compile_with_hosts(function, &HostFunctionRegistry::new())
    }

    pub fn compile_with_hosts(
        function: &Function,
        host_functions: &HostFunctionRegistry,
    ) -> Result<Self, CompileError> {
        function.validate().map_err(CompileError::InvalidMir)?;

        if function.signature().result() != Type::I64 {
            return Err(CompileError::UnsupportedSignature);
        }

        let mut jit_builder =
            JITBuilder::new(default_libcall_names()).map_err(CompileError::backend)?;
        let mut host_symbols = HashMap::new();
        for instruction in function.instructions() {
            let Instruction::CallHostI64 {
                function: host_id, ..
            } = instruction
            else {
                continue;
            };

            if host_symbols.contains_key(host_id) {
                continue;
            }

            let callback = host_functions
                .resolve_i64(*host_id)
                .ok_or(CompileError::UnknownHostFunction(*host_id))?;
            let symbol = host_symbol_name(*host_id);
            jit_builder.symbol(symbol.clone(), callback as *const u8);
            host_symbols.insert(*host_id, symbol);
        }

        let mut module = JITModule::new(jit_builder);
        let mut context = module.make_context();
        context.func.signature.params.extend(
            function
                .signature()
                .parameters()
                .iter()
                .map(|_| AbiParam::new(types::I64)),
        );
        context
            .func
            .signature
            .returns
            .push(AbiParam::new(types::I64));

        let function_id = module
            .declare_function(function.name(), Linkage::Local, &context.func.signature)
            .map_err(CompileError::backend)?;
        let mut builder_context = FunctionBuilderContext::new();
        let frontend_config = module.target_config();
        let mut host_references = HashMap::new();

        for (host_id, symbol) in host_symbols {
            let mut signature = module.make_signature();
            signature.params.push(AbiParam::new(types::I64));
            signature.returns.push(AbiParam::new(types::I64));
            let function_id = module
                .declare_function(&symbol, Linkage::Import, &signature)
                .map_err(CompileError::backend)?;
            let function_reference = module.declare_func_in_func(function_id, &mut context.func);
            host_references.insert(host_id, function_reference);
        }

        {
            let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);

            let mut values = builder.block_params(entry).to_vec();
            for instruction in function.instructions() {
                let value = match *instruction {
                    Instruction::I64Constant { value, .. } => {
                        builder.ins().iconst(types::I64, value)
                    }
                    Instruction::I64AddWrapping { left, right, .. } => builder
                        .ins()
                        .iadd(values[left.index()], values[right.index()]),
                    Instruction::CallHostI64 {
                        function, argument, ..
                    } => {
                        let function_reference = host_references
                            .get(&function)
                            .expect("host reference was declared before lowering");
                        let call = builder
                            .ins()
                            .call(*function_reference, &[values[argument.index()]]);
                        builder.inst_results(call)[0]
                    }
                };
                values.push(value);
            }

            let Terminator::Return(return_value) = function.terminator();
            builder.ins().return_(&[values[return_value.index()]]);
            builder.finalize(frontend_config);
        }

        module
            .define_function(function_id, &mut context)
            .map_err(CompileError::backend)?;
        let code_size = context
            .compiled_code()
            .expect("a successfully defined function has compiled code")
            .code_buffer()
            .len();
        module.clear_context(&mut context);
        module
            .finalize_definitions()
            .map_err(CompileError::backend)?;
        let code = module.get_finalized_function(function_id);

        Ok(Self {
            _module: module,
            code,
            code_size,
            parameter_count: function.signature().parameters().len(),
        })
    }

    pub const fn code_size(&self) -> usize {
        self.code_size
    }

    pub fn invoke_i64_2(&self, left: i64, right: i64) -> Result<i64, InvokeError> {
        if self.parameter_count != 2 {
            return Err(InvokeError::Arity {
                expected: self.parameter_count,
                actual: 2,
            });
        }

        // SAFETY: compilation records the validated MIR arity, and this branch
        // accepts only the exact two-i64 signature used for this function.
        let function: unsafe extern "C" fn(i64, i64) -> i64 =
            unsafe { std::mem::transmute(self.code) };
        // SAFETY: JIT code remains alive with `self._module`, and the signature
        // checked above matches the generated function's native signature.
        Ok(unsafe { function(left, right) })
    }

    pub fn invoke_i64_1(&self, argument: i64) -> Result<i64, InvokeError> {
        if self.parameter_count != 1 {
            return Err(InvokeError::Arity {
                expected: self.parameter_count,
                actual: 1,
            });
        }

        // SAFETY: compilation records the validated MIR arity, and this branch
        // accepts only the exact one-i64 signature used for this function.
        let function: unsafe extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(self.code) };
        // SAFETY: JIT code remains alive with `self._module`, and the signature
        // checked above matches the generated function's native signature.
        Ok(unsafe { function(argument) })
    }
}

fn host_symbol_name(id: HostFunctionId) -> String {
    format!("lang_host_i64_{}", id.as_u32())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    InvalidMir(ValidationError),
    UnsupportedSignature,
    UnknownHostFunction(HostFunctionId),
    Backend(String),
}

impl CompileError {
    fn backend(error: impl Display) -> Self {
        Self::Backend(error.to_string())
    }
}

impl Display for CompileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMir(error) => write!(formatter, "invalid MIR: {error}"),
            Self::UnsupportedSignature => formatter.write_str("unsupported function signature"),
            Self::UnknownHostFunction(id) => {
                write!(formatter, "host function {id} is not registered")
            }
            Self::Backend(error) => write!(formatter, "Cranelift backend error: {error}"),
        }
    }
}

impl Error for CompileError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvokeError {
    Arity { expected: usize, actual: usize },
}

impl Display for InvokeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arity { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} arguments, received {actual}"
                )
            }
        }
    }
}

impl Error for InvokeError {}

#[cfg(test)]
mod tests {
    use lang_mir::{Function, HostFunctionId, Instruction, Signature, Terminator, Type, ValueId};

    use super::{CompileError, CompiledFunction};

    #[test]
    fn reports_generated_code_size() {
        let function = Function::new(
            "identity",
            Signature::new(vec![Type::I64], Type::I64),
            vec![],
            Terminator::Return(ValueId::new(0)),
        );

        let compiled = CompiledFunction::compile(&function).unwrap();

        assert!(compiled.code_size() > 0);
        assert_eq!(compiled.invoke_i64_1(42), Ok(42));
    }

    #[test]
    fn rejects_unregistered_host_functions() {
        let host_id = HostFunctionId::new(9);
        let function = Function::new(
            "missing_host",
            Signature::new(vec![Type::I64], Type::I64),
            vec![Instruction::CallHostI64 {
                result: ValueId::new(1),
                function: host_id,
                argument: ValueId::new(0),
            }],
            Terminator::Return(ValueId::new(1)),
        );

        assert_eq!(
            CompiledFunction::compile(&function).map(|_| ()),
            Err(CompileError::UnknownHostFunction(host_id))
        );
    }
}
