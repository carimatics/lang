//! Shared polling, charging and usage contract for instrumented execution.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Debug, Default)]
pub struct InterruptionFlag {
    interrupted: AtomicBool,
}

impl InterruptionFlag {
    pub const fn new() -> Self {
        Self {
            interrupted: AtomicBool::new(false),
        }
    }

    pub fn interrupt(&self) {
        self.interrupted.store(true, Ordering::Release);
    }

    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub fuel: Option<u64>,
    pub host_calls: Option<u64>,
    pub frames: Option<usize>,
    pub live_cells: Option<usize>,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            fuel: Some(1_000_000),
            host_calls: Some(1_000),
            frames: Some(256),
            live_cells: Some(1_000_000),
        }
    }
}
impl Limits {
    pub const fn unbounded() -> Self {
        Self {
            fuel: None,
            host_calls: None,
            frames: None,
            live_cells: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Control<'a> {
    limits: Limits,
    interruption: Option<&'a InterruptionFlag>,
    deadline: Option<Instant>,
}
impl<'a> Control<'a> {
    pub const fn with_limits(limits: Limits) -> Self {
        Self {
            limits,
            interruption: None,
            deadline: None,
        }
    }
    pub const fn and_interruption(mut self, flag: &'a InterruptionFlag) -> Self {
        self.interruption = Some(flag);
        self
    }
    pub fn and_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }
    pub const fn limits(self) -> Limits {
        self.limits
    }
    pub fn check(self) -> Result<(), Failure> {
        if self
            .interruption
            .is_some_and(InterruptionFlag::is_interrupted)
        {
            return Err(Failure::Interrupted);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(Failure::DeadlineExceeded);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    pub fuel: u64,
    pub host_calls: u64,
    pub peak_frames: usize,
    pub peak_live_cells: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    Interrupted,
    DeadlineExceeded,
    FuelExhausted,
    HostCallLimit,
    CounterOverflow,
}
impl Control<'_> {
    /// Poll before charging one guest step; refused charges leave usage unchanged.
    pub fn charge(self, usage: &mut Usage) -> Result<(), Failure> {
        self.check()?;
        if self.limits.fuel.is_some_and(|limit| usage.fuel >= limit) {
            return Err(Failure::FuelExhausted);
        }
        usage.fuel = usage.fuel.checked_add(1).ok_or(Failure::CounterOverflow)?;
        Ok(())
    }
    /// Charge only a user-declared host call, after polling and before its effect.
    pub fn charge_host_call(self, usage: &mut Usage) -> Result<(), Failure> {
        self.check()?;
        if self
            .limits
            .host_calls
            .is_some_and(|limit| usage.host_calls >= limit)
        {
            return Err(Failure::HostCallLimit);
        }
        usage.host_calls = usage
            .host_calls
            .checked_add(1)
            .ok_or(Failure::CounterOverflow)?;
        Ok(())
    }
}
