use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_bytecode::{LowerError, lower};
use lang_codegen_clif::{CompileError, CompiledFunction, InvokeError};
use lang_mir::{Function, HostFunctionId, Instruction, Signature, Terminator, Type, ValueId};
use lang_runtime::{HostFunctionRegistry, RegistrationError};
use lang_vm::{VmError, execute, execute_with_hosts};

pub mod linker;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionResults {
    pub bytecode: i64,
    pub native: i64,
}

#[derive(Debug)]
pub enum PrototypeError {
    Bytecode(LowerError),
    HostRegistration(RegistrationError),
    Vm(VmError),
    NativeCompile(CompileError),
    NativeInvoke(InvokeError),
}

impl Display for PrototypeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytecode(error) => write!(formatter, "bytecode lowering failed: {error}"),
            Self::HostRegistration(error) => {
                write!(formatter, "host function registration failed: {error}")
            }
            Self::Vm(error) => write!(formatter, "bytecode execution failed: {error}"),
            Self::NativeCompile(error) => write!(formatter, "native compilation failed: {error}"),
            Self::NativeInvoke(error) => write!(formatter, "native invocation failed: {error}"),
        }
    }
}

pub const PROTOTYPE_DOUBLE_HOST: HostFunctionId = HostFunctionId::new(0);

extern "C" fn prototype_wrapping_double(value: i64) -> i64 {
    value.wrapping_mul(2)
}

impl Error for PrototypeError {}

pub fn prototype_add_function() -> Function {
    Function::new(
        "prototype_add",
        Signature::new(vec![Type::I64, Type::I64], Type::I64),
        vec![Instruction::I64AddWrapping {
            result: ValueId::new(2),
            left: ValueId::new(0),
            right: ValueId::new(1),
        }],
        Terminator::Return(ValueId::new(2)),
    )
}

pub fn run_prototype_add(left: i64, right: i64) -> Result<ExecutionResults, PrototypeError> {
    let function = prototype_add_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode = execute(&program, &[left, right]).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile(&function).map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_2(left, right)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

pub fn prototype_min_function() -> Function {
    Function::new(
        "prototype_min",
        Signature::new(vec![Type::I64, Type::I64], Type::I64),
        vec![Instruction::I64LessThanSigned {
            result: ValueId::new(2),
            left: ValueId::new(0),
            right: ValueId::new(1),
        }],
        Terminator::BranchReturn {
            condition: ValueId::new(2),
            then_value: ValueId::new(0),
            else_value: ValueId::new(1),
        },
    )
}

pub fn run_prototype_min(left: i64, right: i64) -> Result<ExecutionResults, PrototypeError> {
    let function = prototype_min_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode = execute(&program, &[left, right]).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile(&function).map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_2(left, right)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

pub fn prototype_host_callback_function() -> Function {
    Function::new(
        "prototype_host_callback",
        Signature::new(vec![Type::I64], Type::I64),
        vec![Instruction::CallHostI64 {
            result: ValueId::new(1),
            function: PROTOTYPE_DOUBLE_HOST,
            argument: ValueId::new(0),
        }],
        Terminator::Return(ValueId::new(1)),
    )
}

pub fn run_prototype_host_callback(value: i64) -> Result<ExecutionResults, PrototypeError> {
    let mut host_functions = HostFunctionRegistry::new();
    host_functions
        .register_i64(PROTOTYPE_DOUBLE_HOST, prototype_wrapping_double)
        .map_err(PrototypeError::HostRegistration)?;

    let function = prototype_host_callback_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode =
        execute_with_hosts(&program, &[value], &host_functions).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile_with_hosts(&function, &host_functions)
        .map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_1(value)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionResults, run_prototype_add, run_prototype_host_callback, run_prototype_min,
    };

    #[test]
    fn execution_modes_are_equivalent() {
        let cases = [(2, 2), (-7, 4), (i64::MAX, 1), (i64::MIN, -1)];

        for (left, right) in cases {
            let expected = left.wrapping_add(right);
            assert_eq!(
                run_prototype_add(left, right).unwrap(),
                ExecutionResults {
                    bytecode: expected,
                    native: expected,
                }
            );
        }
    }

    #[test]
    fn host_callbacks_are_equivalent() {
        let cases = [0_i64, 21, -7, i64::MAX, i64::MIN];

        for value in cases {
            let expected = value.wrapping_mul(2);
            assert_eq!(
                run_prototype_host_callback(value).unwrap(),
                ExecutionResults {
                    bytecode: expected,
                    native: expected,
                }
            );
        }
    }

    #[test]
    fn conditional_returns_are_equivalent() {
        let cases = [
            (2, 2),
            (-7, 4),
            (9, 3),
            (i64::MAX, i64::MIN),
            (i64::MIN, i64::MAX),
        ];

        for (left, right) in cases {
            let expected = left.min(right);
            assert_eq!(
                run_prototype_min(left, right).unwrap(),
                ExecutionResults {
                    bytecode: expected,
                    native: expected,
                }
            );
        }
    }
}
