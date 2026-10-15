use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_bytecode::{LowerError, lower};
use lang_codegen_clif::{CompileError, CompiledFunction, InvokeError};
use lang_mir::{Function, Instruction, Signature, Terminator, Type, ValueId};
use lang_vm::{VmError, execute};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionResults {
    pub bytecode: i64,
    pub native: i64,
}

#[derive(Debug)]
pub enum PrototypeError {
    Bytecode(LowerError),
    Vm(VmError),
    NativeCompile(CompileError),
    NativeInvoke(InvokeError),
}

impl Display for PrototypeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytecode(error) => write!(formatter, "bytecode lowering failed: {error}"),
            Self::Vm(error) => write!(formatter, "bytecode execution failed: {error}"),
            Self::NativeCompile(error) => write!(formatter, "native compilation failed: {error}"),
            Self::NativeInvoke(error) => write!(formatter, "native invocation failed: {error}"),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{ExecutionResults, run_prototype_add};

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
}
