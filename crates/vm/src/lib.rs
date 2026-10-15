#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_bytecode::{Instruction, Program, VerificationError};

pub fn execute(program: &Program, arguments: &[i64]) -> Result<i64, VmError> {
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
        }
    }

    Ok(registers[program.return_register()])
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmError {
    InvalidBytecode(VerificationError),
    Arity { expected: usize, actual: usize },
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
        }
    }
}

impl Error for VmError {}

#[cfg(test)]
mod tests {
    use lang_bytecode::{Instruction, Program};

    use super::{VmError, execute};

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
}
