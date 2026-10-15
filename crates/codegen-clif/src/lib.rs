use std::error::Error;
use std::fmt::{Display, Formatter};

use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use lang_mir::{Function, Instruction, Terminator, Type, ValidationError};

pub struct CompiledFunction {
    _module: JITModule,
    code: *const u8,
    parameter_count: usize,
}

impl CompiledFunction {
    pub fn compile(function: &Function) -> Result<Self, CompileError> {
        function.validate().map_err(CompileError::InvalidMir)?;

        if function.signature().result() != Type::I64 {
            return Err(CompileError::UnsupportedSignature);
        }

        let jit_builder =
            JITBuilder::new(default_libcall_names()).map_err(CompileError::backend)?;
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
        module.clear_context(&mut context);
        module
            .finalize_definitions()
            .map_err(CompileError::backend)?;
        let code = module.get_finalized_function(function_id);

        Ok(Self {
            _module: module,
            code,
            parameter_count: function.signature().parameters().len(),
        })
    }

    pub fn invoke_i64_2(&self, left: i64, right: i64) -> Result<i64, InvokeError> {
        if self.parameter_count != 2 {
            return Err(InvokeError::Arity {
                expected: self.parameter_count,
                actual: 2,
            });
        }

        let function: unsafe extern "C" fn(i64, i64) -> i64 =
            unsafe { std::mem::transmute(self.code) };
        Ok(unsafe { function(left, right) })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    InvalidMir(ValidationError),
    UnsupportedSignature,
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
