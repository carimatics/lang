#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_mir::{
    Function, HostFunctionId, Instruction as MirInstruction, Terminator as MirTerminator, Type,
    ValidationError,
};

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
    I64LessThanSigned {
        destination: usize,
        left: usize,
        right: usize,
    },
    CallHostI64 {
        destination: usize,
        function: HostFunctionId,
        argument: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Terminator {
    Return(usize),
    BranchReturn {
        condition: usize,
        then_register: usize,
        else_register: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    parameter_count: usize,
    register_count: usize,
    instructions: Vec<Instruction>,
    terminator: Terminator,
}

impl Program {
    pub fn new(
        parameter_count: usize,
        register_count: usize,
        instructions: Vec<Instruction>,
        return_register: usize,
    ) -> Result<Self, VerificationError> {
        Self::with_terminator(
            parameter_count,
            register_count,
            instructions,
            Terminator::Return(return_register),
        )
    }

    pub fn with_terminator(
        parameter_count: usize,
        register_count: usize,
        instructions: Vec<Instruction>,
        terminator: Terminator,
    ) -> Result<Self, VerificationError> {
        let program = Self {
            parameter_count,
            register_count,
            instructions,
            terminator,
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

    pub const fn terminator(&self) -> Terminator {
        self.terminator
    }

    pub fn verify(&self) -> Result<(), VerificationError> {
        if self.parameter_count > self.register_count {
            return Err(VerificationError::InvalidParameterCount);
        }

        let mut register_types = vec![None; self.register_count];
        register_types[..self.parameter_count].fill(Some(Type::I64));

        for instruction in &self.instructions {
            match *instruction {
                Instruction::I64Constant { destination, .. } => {
                    verify_destination(destination, &register_types)?;
                    register_types[destination] = Some(Type::I64);
                }
                Instruction::I64AddWrapping {
                    destination,
                    left,
                    right,
                } => {
                    verify_source(left, Type::I64, &register_types)?;
                    verify_source(right, Type::I64, &register_types)?;
                    verify_destination(destination, &register_types)?;
                    register_types[destination] = Some(Type::I64);
                }
                Instruction::I64LessThanSigned {
                    destination,
                    left,
                    right,
                } => {
                    verify_source(left, Type::I64, &register_types)?;
                    verify_source(right, Type::I64, &register_types)?;
                    verify_destination(destination, &register_types)?;
                    register_types[destination] = Some(Type::Bool);
                }
                Instruction::CallHostI64 {
                    destination,
                    argument,
                    ..
                } => {
                    verify_source(argument, Type::I64, &register_types)?;
                    verify_destination(destination, &register_types)?;
                    register_types[destination] = Some(Type::I64);
                }
            }
        }

        match self.terminator {
            Terminator::Return(register) => verify_source(register, Type::I64, &register_types),
            Terminator::BranchReturn {
                condition,
                then_register,
                else_register,
            } => {
                verify_source(condition, Type::Bool, &register_types)?;
                verify_source(then_register, Type::I64, &register_types)?;
                verify_source(else_register, Type::I64, &register_types)
            }
        }
    }
}

fn verify_source(
    register: usize,
    expected: Type,
    register_types: &[Option<Type>],
) -> Result<(), VerificationError> {
    match register_types.get(register) {
        Some(Some(actual)) if *actual == expected => Ok(()),
        Some(Some(actual)) => Err(VerificationError::TypeMismatch {
            register,
            expected,
            actual: *actual,
        }),
        Some(None) => Err(VerificationError::UninitializedRegister(register)),
        None => Err(VerificationError::RegisterOutOfBounds(register)),
    }
}

fn verify_destination(
    register: usize,
    register_types: &[Option<Type>],
) -> Result<(), VerificationError> {
    match register_types.get(register) {
        Some(None) => Ok(()),
        Some(Some(_)) => Err(VerificationError::RegisterAlreadyInitialized(register)),
        None => Err(VerificationError::RegisterOutOfBounds(register)),
    }
}

pub fn lower(function: &Function) -> Result<Program, LowerError> {
    function.validate().map_err(LowerError::InvalidMir)?;
    if function.signature().parameters().contains(&Type::Bool)
        || function.signature().result() != Type::I64
    {
        return Err(LowerError::UnsupportedSignature);
    }

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
            MirInstruction::I64LessThanSigned {
                result,
                left,
                right,
            } => Instruction::I64LessThanSigned {
                destination: result.index(),
                left: left.index(),
                right: right.index(),
            },
            MirInstruction::CallHostI64 {
                result,
                function,
                argument,
            } => Instruction::CallHostI64 {
                destination: result.index(),
                function,
                argument: argument.index(),
            },
        })
        .collect();
    let terminator = match function.terminator() {
        MirTerminator::Return(return_value) => Terminator::Return(return_value.index()),
        MirTerminator::BranchReturn {
            condition,
            then_value,
            else_value,
        } => Terminator::BranchReturn {
            condition: condition.index(),
            then_register: then_value.index(),
            else_register: else_value.index(),
        },
    };

    Program::with_terminator(
        function.signature().parameters().len(),
        function.value_count(),
        instructions,
        terminator,
    )
    .map_err(LowerError::InvalidBytecode)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationError {
    InvalidParameterCount,
    RegisterOutOfBounds(usize),
    UninitializedRegister(usize),
    RegisterAlreadyInitialized(usize),
    TypeMismatch {
        register: usize,
        expected: Type,
        actual: Type,
    },
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
            Self::TypeMismatch {
                register,
                expected,
                actual,
            } => write!(
                formatter,
                "register {register} has type {actual}, expected {expected}"
            ),
        }
    }
}

impl Error for VerificationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LowerError {
    InvalidMir(ValidationError),
    InvalidBytecode(VerificationError),
    UnsupportedSignature,
}

impl Display for LowerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMir(error) => write!(formatter, "invalid MIR: {error}"),
            Self::InvalidBytecode(error) => write!(formatter, "invalid bytecode: {error}"),
            Self::UnsupportedSignature => {
                formatter.write_str("bytecode prototype supports only i64 parameters and results")
            }
        }
    }
}

impl Error for LowerError {}

#[cfg(test)]
mod tests {
    use lang_mir::{HostFunctionId, Type};

    use super::{Instruction, Program, Terminator, VerificationError};

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

    #[test]
    fn rejects_uninitialized_host_call_arguments() {
        let result = Program::new(
            0,
            2,
            vec![Instruction::CallHostI64 {
                destination: 1,
                function: HostFunctionId::new(0),
                argument: 0,
            }],
            1,
        );

        assert_eq!(result, Err(VerificationError::UninitializedRegister(0)));
    }

    #[test]
    fn verifies_typed_conditional_returns() {
        let result = Program::with_terminator(
            2,
            3,
            vec![Instruction::I64LessThanSigned {
                destination: 2,
                left: 0,
                right: 1,
            }],
            Terminator::BranchReturn {
                condition: 2,
                then_register: 0,
                else_register: 1,
            },
        );

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_i64_branch_conditions() {
        let result = Program::with_terminator(
            2,
            2,
            vec![],
            Terminator::BranchReturn {
                condition: 0,
                then_register: 0,
                else_register: 1,
            },
        );

        assert_eq!(
            result,
            Err(VerificationError::TypeMismatch {
                register: 0,
                expected: Type::Bool,
                actual: Type::I64,
            })
        );
    }
}
