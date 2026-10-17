//! Backend-neutral compilation units for the RFC 0006 semantic contract.
//! Wire encoding belongs to `lang-bytecode`; these Rust layouts are not formats.

pub mod ownership;
mod validation;

pub use validation::{
    Budget, Error, ErrorKind, FunctionFacts, Layout, LimitKind, Limits, SourceFacts, UnitFacts,
    Usage, VerifiedUnit,
};

use crate::ResourceTypeId;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DefinitionId(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FunctionId(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportId(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Register(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlockId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Type {
    I64,
    Bool,
    Own(ResourceTypeId),
    Record(DefinitionId),
    Enum(DefinitionId),
}
impl Type {
    pub const fn is_copy(self) -> bool {
        !matches!(self, Self::Own(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub source: SourceId,
    pub start: u32,
    pub end: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Source {
    pub path: String,
    pub text: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub name: String,
    pub value_type: Type,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Case {
    pub name: String,
    pub payload: Vec<Type>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefinitionKind {
    Record(Vec<Field>),
    Enum(Vec<Case>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Definition {
    pub name: String,
    pub kind: DefinitionKind,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    pub parameters: Vec<Type>,
    pub result: Type,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    pub name: String,
    pub signature: Signature,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Op {
    I64Constant {
        destination: Register,
        value: i64,
    },
    Add {
        destination: Register,
        left: Register,
        right: Register,
    },
    LessThan {
        destination: Register,
        left: Register,
        right: Register,
    },
    HostCall {
        destination: Register,
        import: ImportId,
        argument: Register,
    },
    BoolConstant {
        destination: Register,
        value: bool,
    },
    Call {
        destination: Register,
        function: FunctionId,
        arguments: Vec<Register>,
    },
    Record {
        destination: Register,
        definition: DefinitionId,
        fields: Vec<Register>,
    },
    Field {
        destination: Register,
        receiver: Register,
        field: u32,
    },
    Enum {
        destination: Register,
        definition: DefinitionId,
        case: u32,
        payload: Vec<Register>,
    },
    IsCase {
        destination: Register,
        receiver: Register,
        case: u32,
    },
    EnumField {
        destination: Register,
        receiver: Register,
        case: u32,
        field: u32,
    },
    EqualI64 {
        destination: Register,
        left: Register,
        right: Register,
    },
}
impl Op {
    pub const fn destination(&self) -> Register {
        match self {
            Self::I64Constant { destination, .. }
            | Self::Add { destination, .. }
            | Self::LessThan { destination, .. }
            | Self::HostCall { destination, .. }
            | Self::BoolConstant { destination, .. }
            | Self::Call { destination, .. }
            | Self::Record { destination, .. }
            | Self::Field { destination, .. }
            | Self::Enum { destination, .. }
            | Self::IsCase { destination, .. }
            | Self::EnumField { destination, .. }
            | Self::EqualI64 { destination, .. } => *destination,
        }
    }
    pub fn operands(&self) -> impl Iterator<Item = Register> + '_ {
        let (fixed, variable): ([Option<Register>; 2], &[Register]) = match self {
            Self::I64Constant { .. } | Self::BoolConstant { .. } => ([None, None], &[]),
            Self::Add { left, right, .. }
            | Self::LessThan { left, right, .. }
            | Self::EqualI64 { left, right, .. } => ([Some(*left), Some(*right)], &[]),
            Self::HostCall { argument, .. } => ([Some(*argument), None], &[]),
            Self::Field { receiver, .. }
            | Self::IsCase { receiver, .. }
            | Self::EnumField { receiver, .. } => ([Some(*receiver), None], &[]),
            Self::Call { arguments, .. } => ([None, None], arguments),
            Self::Record { fields, .. } => ([None, None], fields),
            Self::Enum { payload, .. } => ([None, None], payload),
        };
        fixed.into_iter().flatten().chain(variable.iter().copied())
    }
    pub fn remap_registers(&mut self, mut map: impl FnMut(Register) -> Register) {
        match self {
            Self::I64Constant { destination, .. } | Self::BoolConstant { destination, .. } => {
                *destination = map(*destination)
            }
            Self::Add {
                destination,
                left,
                right,
            }
            | Self::LessThan {
                destination,
                left,
                right,
            }
            | Self::EqualI64 {
                destination,
                left,
                right,
            } => {
                *destination = map(*destination);
                *left = map(*left);
                *right = map(*right);
            }
            Self::HostCall {
                destination,
                argument,
                ..
            } => {
                *destination = map(*destination);
                *argument = map(*argument);
            }
            Self::Field {
                destination,
                receiver,
                ..
            }
            | Self::IsCase {
                destination,
                receiver,
                ..
            }
            | Self::EnumField {
                destination,
                receiver,
                ..
            } => {
                *destination = map(*destination);
                *receiver = map(*receiver);
            }
            Self::Call {
                destination,
                arguments,
                ..
            }
            | Self::Record {
                destination,
                fields: arguments,
                ..
            }
            | Self::Enum {
                destination,
                payload: arguments,
                ..
            } => {
                *destination = map(*destination);
                for argument in arguments {
                    *argument = map(*argument);
                }
            }
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    pub op: Op,
    pub span: Option<Span>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target {
    pub block: BlockId,
    pub arguments: Vec<Register>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Term {
    Return(Register),
    Jump(Target),
    Branch {
        condition: Register,
        then_target: Target,
        else_target: Target,
    },
}
impl Term {
    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        match self {
            Self::Return(_) => [None, None],
            Self::Jump(target) => [Some(target), None],
            Self::Branch {
                then_target,
                else_target,
                ..
            } => [Some(then_target), Some(else_target)],
        }
        .into_iter()
        .flatten()
    }
    pub fn targets_mut(&mut self) -> impl Iterator<Item = &mut Target> {
        match self {
            Self::Return(_) => [None, None],
            Self::Jump(target) => [Some(target), None],
            Self::Branch {
                then_target,
                else_target,
                ..
            } => [Some(then_target), Some(else_target)],
        }
        .into_iter()
        .flatten()
    }
    pub fn operands(&self) -> impl Iterator<Item = Register> + '_ {
        let value = match self {
            Self::Return(value) => Some(*value),
            Self::Branch { condition, .. } => Some(*condition),
            Self::Jump(_) => None,
        };
        value.into_iter().chain(
            self.targets()
                .flat_map(|target| target.arguments.iter().copied()),
        )
    }
    pub fn remap_registers(&mut self, mut map: impl FnMut(Register) -> Register) {
        match self {
            Self::Return(value) => *value = map(*value),
            Self::Branch { condition, .. } => *condition = map(*condition),
            Self::Jump(_) => {}
        }
        for target in self.targets_mut() {
            for argument in &mut target.arguments {
                *argument = map(*argument);
            }
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Terminator {
    pub kind: Term,
    pub span: Option<Span>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub parameters: Vec<Register>,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub name: String,
    pub signature: Signature,
    pub registers: Vec<Type>,
    pub span: Option<Span>,
    pub blocks: Vec<Block>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unit {
    pub sources: Vec<Source>,
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
    pub functions: Vec<Function>,
    pub entry: Option<FunctionId>,
}

impl Unit {
    pub fn verify(self, limits: Limits) -> Result<VerifiedUnit, Error> {
        let mut budget = Budget::new(limits);
        self.verify_with_budget(&mut budget)
    }
    pub fn verify_with_budget(self, budget: &mut Budget) -> Result<VerifiedUnit, Error> {
        let facts = validation::validate(&self, budget)?;
        Ok(VerifiedUnit { unit: self, facts })
    }
}
