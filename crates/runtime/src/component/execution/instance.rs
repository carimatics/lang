//! Persistent call admission for one checked component Program.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceError {
    Active,
    Poisoned,
    ScopeNotClosed,
}

#[derive(Clone, Copy)]
pub enum Completion {
    Clean,
    Poisoned,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Phase {
    Ready,
    Active,
    Poisoned,
}

pub struct Instance<'p, 'u> {
    pub(super) program: &'p Program<'u>,
    pub(super) phase: Phase,
}
impl<'p, 'u> Instance<'p, 'u> {
    pub fn new(program: &'p Program<'u>) -> Self {
        Self {
            program,
            phase: Phase::Ready,
        }
    }
    pub fn poisoned(&self) -> bool {
        self.phase == Phase::Poisoned
    }
    pub(super) fn ready(&self) -> Result<(), Reason> {
        match self.phase {
            Phase::Ready => Ok(()),
            Phase::Active => Err(Reason::Instance(InstanceError::Active)),
            Phase::Poisoned => Err(Reason::Instance(InstanceError::Poisoned)),
        }
    }
}
