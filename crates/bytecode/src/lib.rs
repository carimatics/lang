#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_mir::{Function, Instruction as MirInstruction, Terminator, ValidationError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Instruction {
    I64Constant {
        destination: usize,
        value: i64,
    },
    I64AddWrapping {
        destination: usize,
        left: usize,
        right: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    parameter_count: usize,
    register_count: usize,
    instructions: Vec<Instruction>,
    return_register: usize,
}

impl Program {
    pub fn new(
        parameter_count: usize,
        register_count: usize,
        instructions: Vec<Instruction>,
        return_register: usize,
    ) -> Result<Self, VerificationError> {
        let program = Self {
            parameter_count,
            register_count,
            instructions,
            return_register,
        };
        program.verify()?;
        Ok(program)
    }

    pub const fn parameter_count(&self) -> usize {
        self.parameter_count
    }

    pub const fn register_count(&self) -> usize {
        self.register_count
    }

    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    pub const fn return_register(&self) -> usize {
        self.return_register
    }

    pub fn verify(&self) -> Result<(), VerificationError> {
        if self.parameter_count > self.register_count {
            return Err(VerificationError::InvalidParameterCount);
        }

        let mut initialized = vec![false; self.register_count];
        initialized[..self.parameter_count].fill(true);

        for instruction in &self.instructions {
            match *instruction {
                Instruction::I64Constant { destination, .. } => {
                    verify_destination(destination, &initialized)?;
                    initialized[destination] = true;
                }
                Instruction::I64AddWrapping {
                    destination,
                    left,
                    right,
                } => {
                    verify_source(left, &initialized)?;
                    verify_source(right, &initialized)?;
                    verify_destination(destination, &initialized)?;
                    initialized[destination] = true;
                }
            }
        }

        verify_source(self.return_register, &initialized)
    }
}

fn verify_source(register: usize, initialized: &[bool]) -> Result<(), VerificationError> {
    match initialized.get(register) {
        Some(true) => Ok(()),
        Some(false) => Err(VerificationError::UninitializedRegister(register)),
        None => Err(VerificationError::RegisterOutOfBounds(register)),
    }
}

fn verify_destination(register: usize, initialized: &[bool]) -> Result<(), VerificationError> {
    match initialized.get(register) {
        Some(false) => Ok(()),
        Some(true) => Err(VerificationError::RegisterAlreadyInitialized(register)),
        None => Err(VerificationError::RegisterOutOfBounds(register)),
    }
}

pub fn lower(function: &Function) -> Result<Program, LowerError> {
    function.validate().map_err(LowerError::InvalidMir)?;

    let instructions = function
        .instructions()
        .iter()
        .map(|instruction| match *instruction {
            MirInstruction::I64Constant { result, value } => Instruction::I64Constant {
                destination: result.index(),
                value,
            },
            MirInstruction::I64AddWrapping {
                result,
                left,
                right,
            } => Instruction::I64AddWrapping {
                destination: result.index(),
                left: left.index(),
                right: right.index(),
            },
        })
        .collect();
    let Terminator::Return(return_value) = function.terminator();

    Program::new(
        function.signature().parameters().len(),
        function.value_count(),
        instructions,
        return_value.index(),
    )
    .map_err(LowerError::InvalidBytecode)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationError {
    InvalidParameterCount,
    RegisterOutOfBounds(usize),
    UninitializedRegister(usize),
    RegisterAlreadyInitialized(usize),
}

impl Display for VerificationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidParameterCount => {
                formatter.write_str("parameter count exceeds register count")
            }
            Self::RegisterOutOfBounds(register) => {
                write!(formatter, "register {register} is out of bounds")
            }
            Self::UninitializedRegister(register) => {
                write!(formatter, "register {register} is not initialized")
            }
            Self::RegisterAlreadyInitialized(register) => {
                write!(
                    formatter,
                    "register {register} is initialized more than once"
                )
            }
        }
    }
}

impl Error for VerificationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LowerError {
    InvalidMir(ValidationError),
    InvalidBytecode(VerificationError),
}

impl Display for LowerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMir(error) => write!(formatter, "invalid MIR: {error}"),
            Self::InvalidBytecode(error) => write!(formatter, "invalid bytecode: {error}"),
        }
    }
}

impl Error for LowerError {}

#[cfg(test)]
mod tests {
    use super::{Instruction, Program, VerificationError};

    #[test]
    fn rejects_reads_from_uninitialized_registers() {
        let result = Program::new(
            1,
            3,
            vec![Instruction::I64AddWrapping {
                destination: 2,
                left: 0,
                right: 1,
            }],
            2,
        );

        assert_eq!(result, Err(VerificationError::UninitializedRegister(1)));
    }
}
