#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Type {
    I64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValueId(u32);

impl ValueId {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HostFunctionId(u32);

impl HostFunctionId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl Display for HostFunctionId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    parameters: Vec<Type>,
    result: Type,
}

impl Signature {
    pub fn new(parameters: Vec<Type>, result: Type) -> Self {
        Self { parameters, result }
    }

    pub fn parameters(&self) -> &[Type] {
        &self.parameters
    }

    pub const fn result(&self) -> Type {
        self.result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Instruction {
    I64Constant {
        result: ValueId,
        value: i64,
    },
    I64AddWrapping {
        result: ValueId,
        left: ValueId,
        right: ValueId,
    },
    CallHostI64 {
        result: ValueId,
        function: HostFunctionId,
        argument: ValueId,
    },
}

impl Instruction {
    pub const fn result(&self) -> ValueId {
        match self {
            Self::I64Constant { result, .. }
            | Self::I64AddWrapping { result, .. }
            | Self::CallHostI64 { result, .. } => *result,
        }
    }

    fn operands(&self) -> impl Iterator<Item = ValueId> {
        let operands = match self {
            Self::I64Constant { .. } => [None, None],
            Self::I64AddWrapping { left, right, .. } => [Some(*left), Some(*right)],
            Self::CallHostI64 { argument, .. } => [Some(*argument), None],
        };
        operands.into_iter().flatten()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Terminator {
    Return(ValueId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    name: String,
    signature: Signature,
    instructions: Vec<Instruction>,
    terminator: Terminator,
}

impl Function {
    pub fn new(
        name: impl Into<String>,
        signature: Signature,
        instructions: Vec<Instruction>,
        terminator: Terminator,
    ) -> Self {
        Self {
            name: name.into(),
            signature,
            instructions,
            terminator,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    pub const fn terminator(&self) -> Terminator {
        self.terminator
    }

    pub fn value_count(&self) -> usize {
        self.signature.parameters.len() + self.instructions.len()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.name.is_empty() {
            return Err(ValidationError::EmptyName);
        }

        let mut next_value = self.signature.parameters.len();
        for instruction in &self.instructions {
            for operand in instruction.operands() {
                if operand.index() >= next_value {
                    return Err(ValidationError::UndefinedValue(operand));
                }
            }

            if instruction.result().index() != next_value {
                return Err(ValidationError::NonSequentialResult {
                    expected: ValueId::new(next_value as u32),
                    actual: instruction.result(),
                });
            }
            next_value += 1;
        }

        let Terminator::Return(value) = self.terminator;
        if value.index() >= next_value {
            return Err(ValidationError::UndefinedValue(value));
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationError {
    EmptyName,
    UndefinedValue(ValueId),
    NonSequentialResult { expected: ValueId, actual: ValueId },
}

impl Display for ValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("function name must not be empty"),
            Self::UndefinedValue(value) => {
                write!(
                    formatter,
                    "value {} is used before it is defined",
                    value.index()
                )
            }
            Self::NonSequentialResult { expected, actual } => write!(
                formatter,
                "expected result value {}, found {}",
                expected.index(),
                actual.index()
            ),
        }
    }
}

impl Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::{Function, Instruction, Signature, Terminator, Type, ValidationError, ValueId};

    #[test]
    fn rejects_use_before_definition() {
        let function = Function::new(
            "invalid",
            Signature::new(vec![Type::I64], Type::I64),
            vec![Instruction::I64AddWrapping {
                result: ValueId::new(1),
                left: ValueId::new(0),
                right: ValueId::new(2),
            }],
            Terminator::Return(ValueId::new(1)),
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::UndefinedValue(ValueId::new(2)))
        );
    }
}
