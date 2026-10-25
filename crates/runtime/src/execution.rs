//! Shared polling, charging and usage contract for instrumented execution.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
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
    pub fn same_configuration(self, other: Self) -> bool {
        self.limits == other.limits
            && self.service_work == other.service_work
            && self.deadline == other.deadline
            && match (self.interruption, other.interruption) {
                (None, None) => true,
                (Some(a), Some(b)) => std::ptr::eq(a, b),
                _ => false,
            }
    }
    /// Apply embedding bounds without losing interruption, deadline or service
    /// configuration. A caller cannot relax the context's finite limits.
    pub fn constrained(mut self, bounds: Limits) -> Self {
        fn lesser<T: Ord>(a: Option<T>, b: Option<T>) -> Option<T> {
            match (a, b) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        }
        self.limits = Limits {
            fuel: lesser(self.limits.fuel, bounds.fuel),
            host_calls: lesser(self.limits.host_calls, bounds.host_calls),
            frames: lesser(self.limits.frames, bounds.frames),
            live_cells: lesser(self.limits.live_cells, bounds.live_cells),
        };
        self
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
struct Configuration {
    limits: Limits,
    service_work: Option<u64>,
    interruption: Option<usize>,
    deadline: Option<Instant>,
}
impl From<Control<'_>> for Configuration {
    fn from(control: Control<'_>) -> Self {
        Self {
            limits: control.limits,
            service_work: control.service_work,
            // Identity comparison only; this address is never dereferenced.
            interruption: control
                .interruption
                .map(|flag| std::ptr::from_ref(flag) as usize),
            deadline: control.deadline,
        }
    }
}

/// Explicit counters for one component invocation. This grants no dynamic
/// entry, allocator, resource or loan authority. Clones retain the same counters.
#[derive(Clone, Debug)]
pub struct InvocationAccounting(Rc<Accounting>);
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LiveUsage {
    pub frames: usize,
    pub cells: usize,
}
#[derive(Debug)]
enum InvocationFailure {
    Report(Rc<crate::component::execution::LatchedFailure>),
    Child(Rc<crate::task::runner::Failure>),
}
#[derive(Debug)]
struct ScopedFailure {
    task: Option<crate::task::TaskId>,
    failure: InvocationFailure,
    canonical_indices: Option<u64>,
}
#[derive(Debug)]
struct Accounting {
    configuration: Configuration,
    usage: Cell<Usage>,
    work: ServiceWork,
    preparation_failure: Cell<Option<Failure>>,
    boundary_failure: RefCell<Option<ScopedFailure>>,
    parent: Cell<LiveUsage>,
    task: RefCell<Option<crate::task::TaskId>>,
}
impl InvocationAccounting {
    pub fn prepare(
        control: Control<'_>,
        budget: &mut lang_mir::unit::Budget,
    ) -> Result<Self, lang_mir::unit::Error> {
        budget.allocate::<Accounting>(1)?;
        budget.allocate::<usize>(2)?;
        Ok(Self(Rc::new(Accounting {
            configuration: control.into(),
            usage: Cell::new(Usage::default()),
            work: ServiceWork::new(control.service_work_limit()),
            preparation_failure: Cell::new(None),
            boundary_failure: RefCell::new(None),
            parent: Cell::new(LiveUsage::default()),
            task: RefCell::new(None),
        })))
    }
    pub fn matches(&self, control: Control<'_>) -> bool {
        self.0.configuration == control.into()
    }
    pub fn same_invocation(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    fn current_failure(&self) -> Option<std::cell::Ref<'_, ScopedFailure>> {
        let task = self.0.task.borrow();
        std::cell::Ref::filter_map(self.0.boundary_failure.borrow(), |slot| {
            slot.as_ref()
                .filter(|slot| slot.task.is_none() || slot.task.as_ref() == task.as_ref())
        })
        .ok()
    }
    pub fn boundary_failure(&self) -> Option<Rc<crate::component::execution::LatchedFailure>> {
        match self.current_failure().as_deref().map(|slot| &slot.failure) {
            Some(InvocationFailure::Report(report)) => Some(report.clone()),
            _ => None,
        }
    }
    pub fn child_failure(&self) -> Option<Rc<crate::task::runner::Failure>> {
        match self.current_failure().as_deref().map(|slot| &slot.failure) {
            Some(InvocationFailure::Child(failure)) => Some(failure.clone()),
            _ => None,
        }
    }
    pub fn failure_indices(&self) -> Option<u64> {
        self.current_failure()
            .and_then(|slot| slot.canonical_indices)
    }
    pub fn has_failure(&self) -> bool {
        self.current_failure().is_some()
    }
    fn install_failure(
        &self,
        failure: InvocationFailure,
        indices: Option<u64>,
        cause: Option<&crate::task::runner::Failure>,
    ) {
        let task = self.0.task.borrow().clone();
        let mut slot = self.0.boundary_failure.borrow_mut();
        let replace = match slot
            .as_ref()
            .filter(|slot| slot.task.is_none() || slot.task == task)
        {
            None => true,
            Some(ScopedFailure {
                failure: InvocationFailure::Child(first),
                ..
            }) => cause.is_some_and(|cause| {
                std::ptr::eq(cause.reason.primary_cause(), first.reason.primary_cause())
            }),
            Some(_) => false,
        };
        if replace {
            *slot = Some(ScopedFailure {
                task,
                failure,
                canonical_indices: indices,
            });
        }
    }
    /// Internal borrowed Rust child adapters retain the actual typed failure.
    pub fn latch_child_failure(&self, child: crate::task::runner::Failure) {
        self.install_failure(InvocationFailure::Child(Rc::new(child)), None, None);
    }
    pub(crate) fn latch_context_failure(&self, child: crate::task::runner::Failure, indices: u64) {
        self.install_failure(
            InvocationFailure::Child(Rc::new(child)),
            Some(indices),
            None,
        );
    }
    pub fn latch_boundary_failure(
        &self,
        report: crate::component::wire::Report,
        bytes: Vec<u8>,
        owner: Rc<dyn std::any::Any>,
    ) {
        self.install_failure(
            InvocationFailure::Report(Rc::new(crate::component::execution::LatchedFailure::new(
                report, bytes, owner, None,
            ))),
            None,
            None,
        );
    }
    /// Enrich only that same retained typed cause with checked C metadata.
    pub fn latch_boundary_failure_from(
        &self,
        report: crate::component::wire::Report,
        bytes: Vec<u8>,
        owner: Rc<dyn std::any::Any>,
        cause: Rc<crate::task::runner::Failure>,
    ) {
        let packet = Rc::new(crate::component::execution::LatchedFailure::new(
            report,
            bytes,
            owner,
            Some(cause.clone()),
        ));
        self.install_failure(InvocationFailure::Report(packet), None, Some(&cause));
    }
    /// Lend actual suspended guest live counts for one synchronous child. This
    /// changes no counter identity or authority, and restores the prior scope.
    pub fn with_parent<R>(
        &self,
        live: LiveUsage,
        operation: impl FnOnce() -> R,
    ) -> Result<R, Failure> {
        let previous = self.0.parent.get();
        let next = LiveUsage {
            frames: previous
                .frames
                .checked_add(live.frames)
                .ok_or(Failure::CounterOverflow)?,
            cells: previous
                .cells
                .checked_add(live.cells)
                .ok_or(Failure::CounterOverflow)?,
        };
        struct Restore<'a>(&'a Cell<LiveUsage>, LiveUsage);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _restore = Restore(&self.0.parent, previous);
        self.0.parent.set(next);
        Ok(operation())
    }
    /// Combine a Machine's live counts with its suspended synchronous ancestors.
    pub(crate) fn live_with_parent(&self, local: LiveUsage) -> Result<LiveUsage, Failure> {
        let parent = self.0.parent.get();
        Ok(LiveUsage {
            frames: parent
                .frames
                .checked_add(local.frames)
                .ok_or(Failure::CounterOverflow)?,
            cells: parent
                .cells
                .checked_add(local.cells)
                .ok_or(Failure::CounterOverflow)?,
        })
    }
    pub(crate) fn current_task(&self) -> Option<crate::task::TaskId> {
        self.0.task.borrow().clone()
    }
    pub(crate) fn with_task<R>(
        &self,
        task: crate::task::TaskId,
        operation: impl FnOnce() -> R,
    ) -> R {
        struct Restore<'a> {
            slot: &'a RefCell<Option<crate::task::TaskId>>,
            previous: Option<crate::task::TaskId>,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.slot.replace(self.previous.take());
            }
        }
        let _restore = Restore {
            slot: &self.0.task,
            previous: self.0.task.replace(Some(task)),
        };
        operation()
    }
    pub fn work(&self) -> &ServiceWork {
        &self.0.work
    }
    pub fn snapshot(&self) -> Usage {
        Usage {
            service_work: self.0.work.used(),
            ..self.0.usage.get()
        }
    }
    pub fn preparation_failure(&self) -> Option<Failure> {
        self.0.preparation_failure.get()
    }
    pub fn latch_preparation_failure(&self, failure: Failure) {
        if self.preparation_failure().is_none() {
            self.0.preparation_failure.set(Some(failure));
        }
    }
    pub(crate) fn charge(&self, control: Control<'_>) -> Result<(), Failure> {
        debug_assert!(self.matches(control));
        let mut usage = self.0.usage.get();
        control.charge(&mut usage)?;
        self.0.usage.set(usage);
        Ok(())
    }
    pub(crate) fn charge_host_call(&self, control: Control<'_>) -> Result<(), Failure> {
        debug_assert!(self.matches(control));
        let mut usage = self.0.usage.get();
        control.charge_host_call(&mut usage)?;
        self.0.usage.set(usage);
        Ok(())
    }
    /// Observe local peaks. Active-parent live offsets require checked child
    /// entry; this counter alone does not admit nested frames or tasks.
    pub(crate) fn observe(&self, peak_frames: usize, peak_live_cells: usize) {
        let mut usage = self.0.usage.get();
        let parent = self.0.parent.get();
        usage.peak_frames = usage
            .peak_frames
            .max(parent.frames.saturating_add(peak_frames));
        usage.peak_live_cells = usage
            .peak_live_cells
            .max(parent.cells.saturating_add(peak_live_cells));
        self.0.usage.set(usage);
    }
}

/// One synchronous invocation's service counter. Clones forward the same
/// counter through prepared services and guest calls; independent constructors
/// remain isolated. Construct before guest entry, without ambient state.
/// Prepared services and views retaining this counter are synchronous and
/// neither `Send` nor `Sync`; create them on their execution thread.
#[derive(Clone, Debug)]
pub struct ServiceWork(Rc<ServiceCounter>);
#[derive(Debug)]
struct ServiceCounter {
    control: Control<'static>,
    usage: Cell<Usage>,
    suspended: Cell<bool>,
}
impl ServiceWork {
    pub fn same_invocation(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    pub fn new(limit: Option<u64>) -> Self {
        Self(Rc::new(ServiceCounter {
            control: Control::with_limits(Limits::unbounded()).and_service_work(limit),
            usage: Cell::new(Usage::default()),
            suspended: Cell::new(false),
        }))
    }
    pub fn used(&self) -> u64 {
        self.0.usage.get().service_work
    }
    pub fn limit(&self) -> Option<u64> {
        self.0.control.service_work_limit()
    }
    pub fn charge(&self, requested: u64) -> Result<(), Failure> {
        self.charge_batch(&[requested])
    }
    /// Preflight before provider/storage effects. This does not reserve work;
    /// the owning operation still charges again at its commit boundary.
    pub fn check(&self, requested: u64) -> Result<(), Failure> {
        if self.0.suspended.get() {
            return Ok(());
        }
        self.0
            .control
            .charge_service_work(&mut self.0.usage.get(), requested)
    }
    pub fn charge_batch(&self, events: &[u64]) -> Result<(), Failure> {
        if self.0.suspended.get() {
            return Ok(());
        }
        let mut usage = self.0.usage.get();
        self.0.control.charge_service_batch(&mut usage, events)?;
        self.0.usage.set(usage);
        Ok(())
    }
    /// Mandatory cleanup and trusted post-execution inspection do not consume
    /// guest service work. Restore the prior scope even if its callback panics.
    pub fn unmetered<T>(&self, operation: impl FnOnce() -> T) -> T {
        struct Restore<'a>(&'a Cell<bool>, bool);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _restore = Restore(&self.0.suspended, self.0.suspended.replace(true));
        operation()
    }
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
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "execution control failed: {self:?}")
    }
}
impl std::error::Error for Failure {}

/// Recover a structured control refusal through runtime service error layers.
pub fn service_failure(error: &(dyn std::error::Error + 'static)) -> Option<Failure> {
    let mut cause = Some(error);
    while let Some(error) = cause {
        if let Some(failure) = error.downcast_ref::<Failure>() {
            return Some(*failure);
        }
        cause = error.source();
    }
    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedServiceWork {
    pub limit: u64,
}

/// Check the whole verified unit before effects; only completely metered
/// operation families may participate in a finite-budget program.
pub fn service_work_supported(flow: &lang_mir::unit::ownership::failure::Elaborated) -> bool {
    if flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .foreign_interface_span()
        .is_some()
    {
        return false;
    }
    service_work_supported_with_c_imports(flow)
}
/// The C entry additionally proves the supported import profile and meters its
/// F10 copies/validation through the same invocation counter before effects.
pub fn service_work_supported_with_c_imports(
    flow: &lang_mir::unit::ownership::failure::Elaborated,
) -> bool {
    use lang_mir::unit::ownership::{bodies::Op, core::Operation as C};
    flow.source()
        .activity()
        .moved()
        .typed()
        .functions()
        .iter()
        .all(|function| {
            function.blocks.iter().all(|block| {
                block
                    .instructions
                    .iter()
                    .all(|instruction| match &instruction.op {
                        Op::CoreCall(call) => match call.operation {
                            C::ServiceNew
                            | C::ServiceHandle
                            | C::ServiceClone
                            | C::ServiceOwner
                            | C::ServiceRead
                            | C::ServiceReplace
                            | C::ServiceFinish
                            | C::ServiceCancel => true,
                            C::ChannelNew
                            | C::ChannelSender
                            | C::ChannelClone
                            | C::ChannelReceiver
                            | C::ChannelSeal
                            | C::ChannelUnsent => true,
                            C::Allocator
                            | C::TextFromUtf8
                            | C::TextClone
                            | C::TextLen
                            | C::TextBytes
                            | C::TextEqual
                            | C::TextConcat
                            | C::BytesNew
                            | C::BytesClone
                            | C::BytesLen
                            | C::BytesGet
                            | C::BytesPush
                            | C::BytesSet
                            | C::VecNew
                            | C::VecLen
                            | C::VecPush
                            | C::VecPop
                            | C::VecGet
                            | C::VecGetMut
                            | C::MapNew
                            | C::MapLen
                            | C::MapInsert
                            | C::MapGet
                            | C::MapRemove
                            | C::SharedNew
                            | C::SharedClone
                            | C::SharedRead
                            | C::SharedReplace
                            | C::WeakNew
                            | C::WeakClone
                            | C::WeakUpgrade
                            | C::ArenaNew
                            | C::ArenaRead
                            | C::ArenaReplace
                            | C::OptionUnwrap
                            | C::ResultUnwrap
                            | C::ResultUnwrapErr => true,
                        },
                        _ => true,
                    })
            })
        })
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

#[cfg(test)]
mod accounting_tests {
    use super::*;

    #[test]
    fn suspended_live_usage_restores_after_error_unwind_and_overflow() {
        let control = Control::with_limits(Limits::unbounded());
        let accounting = InvocationAccounting::prepare(
            control,
            &mut lang_mir::unit::Budget::new(Default::default()),
        )
        .unwrap();
        accounting.observe(1, 2);
        let failure = accounting
            .with_parent(
                LiveUsage {
                    frames: 1,
                    cells: 2,
                },
                || {
                    accounting.observe(1, 3);
                    Err::<(), ()>(())
                },
            )
            .unwrap();
        assert!(failure.is_err());
        assert_eq!(accounting.snapshot().peak_frames, 2);
        assert_eq!(accounting.snapshot().peak_live_cells, 5);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = accounting.with_parent(
                    LiveUsage {
                        frames: 10,
                        cells: 20,
                    },
                    || panic!("scope unwind"),
                );
            }))
            .is_err()
        );
        accounting.observe(1, 2);
        assert_eq!(accounting.snapshot().peak_frames, 2);
        assert_eq!(accounting.snapshot().peak_live_cells, 5);
        accounting
            .with_parent(
                LiveUsage {
                    frames: usize::MAX,
                    cells: 0,
                },
                || {
                    assert_eq!(
                        accounting.with_parent(
                            LiveUsage {
                                frames: 1,
                                cells: 0
                            },
                            || ()
                        ),
                        Err(Failure::CounterOverflow)
                    );
                },
            )
            .unwrap();
        accounting.observe(1, 2);
        assert_eq!(accounting.snapshot().peak_frames, 2);
        assert_eq!(accounting.snapshot().peak_live_cells, 5);
    }
}
