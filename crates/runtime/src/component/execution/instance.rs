//! Persistent root admission, independent from scoped checked Program views.
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

/// Root admission and caller authority retained across program preparation
/// scopes. Creating another state mints another holder, not a poison reset.
pub struct InstanceState {
    phase: Phase,
    caller: Holder,
}
impl InstanceState {
    pub fn prepare(resources: &mut Resources<'_>) -> Result<Self, Reason> {
        Ok(Self {
            phase: Phase::Ready,
            caller: resources.new_holder().map_err(fault)?,
        })
    }
    pub const fn holder(&self) -> Holder {
        self.caller
    }
    pub fn poisoned(&self) -> bool {
        self.phase == Phase::Poisoned
    }
    /// Mandatory external-owner cleanup can add poison, never clear it.
    pub fn poison(&mut self) {
        self.phase = Phase::Poisoned;
    }
    pub fn with_program<'p, 'u, T>(
        &mut self,
        program: &'p Program<'u>,
        run: impl FnOnce(&mut Instance<'p, 'u>) -> T,
    ) -> Result<T, Reason> {
        ready(self.phase)?;
        let caller = self.caller;
        let phase = self.phase;
        self.phase = Phase::Active;
        let mut scope = Scope {
            target: self,
            instance: Instance {
                program,
                phase,
                caller: Some(caller),
                callback: false,
            },
        };
        Ok(run(&mut scope.instance))
    }
}
struct Scope<'state, 'p, 'u> {
    target: &'state mut InstanceState,
    instance: Instance<'p, 'u>,
}
impl Drop for Scope<'_, '_, '_> {
    fn drop(&mut self) {
        self.target.phase = self.instance.phase;
    }
}

pub struct Instance<'p, 'u> {
    pub(super) program: &'p Program<'u>,
    pub(super) phase: Phase,
    pub(super) caller: Option<Holder>,
    pub(super) callback: bool,
}
impl<'p, 'u> Instance<'p, 'u> {
    pub fn new(program: &'p Program<'u>) -> Self {
        Self {
            program,
            phase: Phase::Ready,
            caller: None,
            callback: false,
        }
    }
    pub fn program(&self) -> &'p Program<'u> {
        self.program
    }
    pub fn poisoned(&self) -> bool {
        self.phase == Phase::Poisoned
    }
    pub(super) fn ready(&self) -> Result<(), Reason> {
        ready(self.phase)
    }
}

/// Borrowed implementation view of an already active instance. The native
/// context is the only constructor of this dynamic callback capability.
pub(super) trait CallbackInstance {
    fn poisoned(&self) -> bool;
    fn with_callback(
        &mut self,
        caller: Holder,
        run: &mut dyn FnMut(&mut Instance<'_, '_>) -> Result<(), u32>,
    ) -> Result<(), u32>;
}
struct Activation<'a, 'p, 'u> {
    parent: &'a mut Instance<'p, 'u>,
    child: Instance<'p, 'u>,
}
impl Drop for Activation<'_, '_, '_> {
    fn drop(&mut self) {
        self.parent.phase = if self.child.phase == Phase::Ready {
            Phase::Active
        } else {
            Phase::Poisoned
        };
    }
}
impl CallbackInstance for Instance<'_, '_> {
    fn poisoned(&self) -> bool {
        self.poisoned()
    }
    fn with_callback(
        &mut self,
        caller: Holder,
        run: &mut dyn FnMut(&mut Instance<'_, '_>) -> Result<(), u32>,
    ) -> Result<(), u32> {
        if self.phase != Phase::Active {
            return Err(4);
        }
        let child = Instance {
            program: self.program,
            phase: Phase::Ready,
            caller: Some(caller),
            callback: true,
        };
        let mut activation = Activation {
            parent: self,
            child,
        };
        run(&mut activation.child)
    }
}

fn ready(phase: Phase) -> Result<(), Reason> {
    match phase {
        Phase::Ready => Ok(()),
        Phase::Active => Err(Reason::Instance(InstanceError::Active)),
        Phase::Poisoned => Err(Reason::Instance(InstanceError::Poisoned)),
    }
}
