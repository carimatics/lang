#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_bytecode::{Instruction, Program, VerificationError};
use lang_mir::HostFunctionId;
use lang_runtime::HostFunctionRegistry;

pub fn execute(program: &Program, arguments: &[i64]) -> Result<i64, VmError> {
    execute_with_hosts(program, arguments, &HostFunctionRegistry::new())
}

pub fn execute_with_hosts(
    program: &Program,
    arguments: &[i64],
    host_functions: &HostFunctionRegistry,
) -> Result<i64, VmError> {
    program.verify().map_err(VmError::InvalidBytecode)?;

    if arguments.len() != program.parameter_count() {
        return Err(VmError::Arity {
            expected: program.parameter_count(),
            actual: arguments.len(),
        });
    }

    let mut registers = vec![0; program.register_count()];
    registers[..arguments.len()].copy_from_slice(arguments);

    for instruction in program.instructions() {
        match *instruction {
            Instruction::I64Constant { destination, value } => {
                registers[destination] = value;
            }
            Instruction::I64AddWrapping {
                destination,
                left,
                right,
            } => {
                registers[destination] = registers[left].wrapping_add(registers[right]);
            }
            Instruction::CallHostI64 {
                destination,
                function,
                argument,
            } => {
                let callback = host_functions
                    .resolve_i64(function)
                    .ok_or(VmError::UnknownHostFunction(function))?;
                registers[destination] = callback(registers[argument]);
            }
        }
    }

    Ok(registers[program.return_register()])
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmError {
    InvalidBytecode(VerificationError),
    Arity { expected: usize, actual: usize },
    UnknownHostFunction(HostFunctionId),
}

impl Display for VmError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBytecode(error) => write!(formatter, "invalid bytecode: {error}"),
            Self::Arity { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} arguments, received {actual}"
                )
            }
            Self::UnknownHostFunction(id) => {
                write!(formatter, "host function {id} is not registered")
            }
        }
    }
}

impl Error for VmError {}

#[cfg(test)]
mod tests {
    use lang_bytecode::{Instruction, Program};
    use lang_mir::HostFunctionId;
    use lang_runtime::HostFunctionRegistry;

    use super::{VmError, execute, execute_with_hosts};

    extern "C" fn wrapping_double(value: i64) -> i64 {
        value.wrapping_mul(2)
    }

    #[test]
    fn executes_wrapping_addition() {
        let program = Program::new(
            2,
            3,
            vec![Instruction::I64AddWrapping {
                destination: 2,
                left: 0,
                right: 1,
            }],
            2,
        )
        .unwrap();

        assert_eq!(execute(&program, &[i64::MAX, 1]), Ok(i64::MIN));
        assert_eq!(
            execute(&program, &[1]),
            Err(VmError::Arity {
                expected: 2,
                actual: 1,
            })
        );
    }

    #[test]
    fn executes_registered_host_function() {
        let function = HostFunctionId::new(3);
        let program = Program::new(
            1,
            2,
            vec![Instruction::CallHostI64 {
                destination: 1,
                function,
                argument: 0,
            }],
            1,
        )
        .unwrap();
        let mut hosts = HostFunctionRegistry::new();
        hosts.register_i64(function, wrapping_double).unwrap();

        assert_eq!(execute_with_hosts(&program, &[21], &hosts), Ok(42));
        assert_eq!(
            execute(&program, &[21]),
            Err(VmError::UnknownHostFunction(function))
        );
    }
}
