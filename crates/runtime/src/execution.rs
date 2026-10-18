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
    service_work: Option<u64>,
    interruption: Option<&'a InterruptionFlag>,
    deadline: Option<Instant>,
}
impl<'a> Control<'a> {
    pub const fn with_limits(limits: Limits) -> Self {
        Self {
            limits,
            service_work: None,
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
    pub const fn and_service_work(mut self, limit: Option<u64>) -> Self {
        self.service_work = limit;
        self
    }
    pub const fn service_work_limit(self) -> Option<u64> {
        self.service_work
    }
    /// Finite budgets require every service in the selected profile to be metered.
    pub fn check_service_work_support(self, supported: bool) -> Result<(), UnsupportedServiceWork> {
        match self.service_work {
            Some(limit) if !supported => Err(UnsupportedServiceWork { limit }),
            _ => Ok(()),
        }
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
    pub service_work: u64,
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
    ServiceWorkLimit {
        limit: u64,
        used: u64,
        requested: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedServiceWork {
    pub limit: u64,
}

impl From<Failure> for crate::cleanup::ExecutionFailure {
    fn from(error: Failure) -> Self {
        match error {
            Failure::Interrupted => Self::Interrupted,
            Failure::DeadlineExceeded => Self::DeadlineExceeded,
            Failure::FuelExhausted => Self::FuelLimit,
            Failure::HostCallLimit => Self::HostCallLimit,
            Failure::CounterOverflow => Self::CounterOverflow,
            Failure::ServiceWorkLimit {
                limit,
                used,
                requested,
            } => Self::ServiceWorkLimit {
                limit,
                used,
                requested,
            },
        }
    }
}
impl Control<'_> {
    /// Admit one service event without adding a polling/fuel position.
    /// Refused work leaves all usage fields unchanged.
    pub fn charge_service_work(self, usage: &mut Usage, requested: u64) -> Result<(), Failure> {
        let total = usage
            .service_work
            .checked_add(requested)
            .ok_or(Failure::CounterOverflow)?;
        if let Some(limit) = self.service_work
            && total > limit
        {
            return Err(Failure::ServiceWorkLimit {
                limit,
                used: usage.service_work,
                requested,
            });
        }
        usage.service_work = total;
        Ok(())
    }
    /// Admit known dependent events together before allocation or handle mutation.
    pub fn charge_service_batch(self, usage: &mut Usage, events: &[u64]) -> Result<(), Failure> {
        let requested = events.iter().try_fold(0u64, |total, &cost| {
            total.checked_add(cost).ok_or(Failure::CounterOverflow)
        })?;
        self.charge_service_work(usage, requested)
    }
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
