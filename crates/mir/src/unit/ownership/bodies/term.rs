//! Ownership control flow is independent from the serialized Copy profile.
use crate::unit::{Register, Target, Term as CopyTerm};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Term {
    Return(Register),
    Jump(Target),
    Branch {
        condition: Register,
        then_target: Target,
        else_target: Target,
    },
    /// A checked-source path that cannot occur for a valid value. Execution
    /// adapters must report a contained invariant fault if it is reached.
    Unreachable,
}
impl Term {
    pub const fn is_exit(&self) -> bool {
        matches!(self, Self::Return(_) | Self::Unreachable)
    }
    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        match self {
            Self::Return(_) | Self::Unreachable => [None, None],
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
            Self::Return(_) | Self::Unreachable => [None, None],
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
            Self::Jump(_) | Self::Unreachable => None,
        };
        value
            .into_iter()
            .chain(self.targets().flat_map(|t| t.arguments.iter().copied()))
    }
}
impl From<CopyTerm> for Term {
    fn from(term: CopyTerm) -> Self {
        match term {
            CopyTerm::Return(value) => Self::Return(value),
            CopyTerm::Jump(target) => Self::Jump(target),
            CopyTerm::Branch {
                condition,
                then_target,
                else_target,
            } => Self::Branch {
                condition,
                then_target,
                else_target,
            },
        }
    }
}
