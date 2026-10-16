//! Backend-neutral trace prototype for canonical component resources.
//!
//! The adapter deliberately models only boundary-visible identity and
//! ownership. Host values continue to belong to the runtime resource table;
//! this layer proves the ordering and failure rules that every execution
//! backend must preserve.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU32;
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::negotiation::ResourceTypeKey;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundaryResourceIndex(NonZeroU32);

impl BoundaryResourceIndex {
    pub const fn get(self) -> u32 {
        self.0.get()
    }

    fn from_position(position: usize) -> Self {
        let raw = u32::try_from(position + 1).unwrap_or_else(|_| {
            unreachable!("adapter positions are bounded before index construction")
        });
        let Some(index) = NonZeroU32::new(raw) else {
            unreachable!("adapter positions always produce non-zero indices")
        };
        Self(index)
    }

    fn position(self) -> usize {
        (self.get() - 1) as usize
    }
}

impl Display for BoundaryResourceIndex {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceIdentity(u64);

impl ResourceIdentity {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdapterLimits {
    max_live_entries: usize,
    max_boundary_indices: u32,
}

impl AdapterLimits {
    pub const fn new(max_live_entries: usize, max_boundary_indices: u32) -> Self {
        Self {
            max_live_entries,
            max_boundary_indices,
        }
    }

    pub const fn max_live_entries(self) -> usize {
        self.max_live_entries
    }

    pub const fn max_boundary_indices(self) -> u32 {
        self.max_boundary_indices
    }
}

impl Default for AdapterLimits {
    fn default() -> Self {
        Self::new(usize::MAX, u32::MAX)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceState {
    Reusable,
    Poisoned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterRole {
    Caller,
    Callee,
}

impl Display for AdapterRole {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Caller => formatter.write_str("caller"),
            Self::Callee => formatter.write_str("callee"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalOwnership {
    Own,
    Borrow,
}

impl Display for CanonicalOwnership {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Own => formatter.write_str("own"),
            Self::Borrow => formatter.write_str("borrow"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterErrorKind {
    InstancePoisoned(AdapterRole),
    LiveLimitExceeded {
        limit: usize,
    },
    BoundaryIndexLimitExceeded {
        limit: u32,
    },
    ReservationFailed,
    StaleBoundaryIndex(BoundaryResourceIndex),
    ExpectedOwned(BoundaryResourceIndex),
    ExpectedBorrowed(BoundaryResourceIndex),
    ResourceTypeMismatch {
        index: BoundaryResourceIndex,
        expected: ResourceTypeKey,
        actual: ResourceTypeKey,
    },
    AliasedArgument(BoundaryResourceIndex),
    ActiveLends {
        index: BoundaryResourceIndex,
        count: usize,
    },
    BorrowCountExhausted(BoundaryResourceIndex),
    ScopeDepthExhausted,
    NoChildBorrowScope,
    NotFrameOwned(BoundaryResourceIndex),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterError {
    kind: Box<AdapterErrorKind>,
}

impl AdapterError {
    fn new(kind: AdapterErrorKind) -> Self {
        Self {
            kind: Box::new(kind),
        }
    }

    pub fn kind(&self) -> &AdapterErrorKind {
        self.kind.as_ref()
    }
}

impl Display for AdapterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self.kind.as_ref() {
            AdapterErrorKind::InstancePoisoned(role) => {
                write!(formatter, "{role} component instance is poisoned")
            }
            AdapterErrorKind::LiveLimitExceeded { limit } => {
                write!(
                    formatter,
                    "component adapter live-entry limit {limit} exceeded"
                )
            }
            AdapterErrorKind::BoundaryIndexLimitExceeded { limit } => write!(
                formatter,
                "component adapter boundary-index limit {limit} exceeded"
            ),
            AdapterErrorKind::ReservationFailed => {
                formatter.write_str("component adapter destination reservation failed")
            }
            AdapterErrorKind::StaleBoundaryIndex(index) => {
                write!(formatter, "component boundary index {index} is stale")
            }
            AdapterErrorKind::ExpectedOwned(index) => {
                write!(formatter, "component boundary index {index} is not owned")
            }
            AdapterErrorKind::ExpectedBorrowed(index) => {
                write!(
                    formatter,
                    "component boundary index {index} is not borrowed"
                )
            }
            AdapterErrorKind::ResourceTypeMismatch {
                index,
                expected,
                actual,
            } => write!(
                formatter,
                "component boundary index {index} has resource type {actual:?}, expected {expected:?}"
            ),
            AdapterErrorKind::AliasedArgument(index) => write!(
                formatter,
                "component boundary index {index} appears more than once in one call"
            ),
            AdapterErrorKind::ActiveLends { index, count } => write!(
                formatter,
                "owned component boundary index {index} has {count} active lends"
            ),
            AdapterErrorKind::BorrowCountExhausted(index) => write!(
                formatter,
                "borrow count for component boundary index {index} is exhausted"
            ),
            AdapterErrorKind::ScopeDepthExhausted => {
                formatter.write_str("component borrow-scope depth is exhausted")
            }
            AdapterErrorKind::NoChildBorrowScope => {
                formatter.write_str("no child component borrow scope is open")
            }
            AdapterErrorKind::NotFrameOwned(index) => write!(
                formatter,
                "component boundary index {index} is not owned by this call frame"
            ),
        }
    }
}

impl Error for AdapterError {}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AdapterEntryKind {
    Owned {
        active_borrows: usize,
    },
    Borrowed {
        source_owner: BoundaryResourceIndex,
        scope_depth: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AdapterEntry {
    resource_type: ResourceTypeKey,
    resource: ResourceIdentity,
    kind: AdapterEntryKind,
}

#[derive(Debug)]
pub struct CanonicalResourceTable {
    entries: Vec<Option<AdapterEntry>>,
    live_entries: usize,
    limits: AdapterLimits,
    state: InstanceState,
}

impl CanonicalResourceTable {
    pub fn new(limits: AdapterLimits) -> Self {
        Self {
            entries: Vec::new(),
            live_entries: 0,
            limits,
            state: InstanceState::Reusable,
        }
    }

    pub const fn state(&self) -> InstanceState {
        self.state
    }

    pub const fn len(&self) -> usize {
        self.live_entries
    }

    pub const fn is_empty(&self) -> bool {
        self.live_entries == 0
    }

    pub fn allocated_index_count(&self) -> usize {
        self.entries.len()
    }

    pub fn is_live(&self, index: BoundaryResourceIndex) -> bool {
        self.entry(index).is_some()
    }

    pub fn active_borrow_count(&self, index: BoundaryResourceIndex) -> Option<usize> {
        match &self.entry(index)?.kind {
            AdapterEntryKind::Owned { active_borrows } => Some(*active_borrows),
            AdapterEntryKind::Borrowed { .. } => None,
        }
    }

    pub fn resource_identity(&self, index: BoundaryResourceIndex) -> Option<ResourceIdentity> {
        self.entry(index).map(|entry| entry.resource)
    }

    pub fn insert_owned(
        &mut self,
        resource_type: ResourceTypeKey,
        resource: ResourceIdentity,
    ) -> Result<BoundaryResourceIndex, AdapterError> {
        self.ensure_reusable(AdapterRole::Caller)?;
        self.reserve_entries(1)?;
        let index = BoundaryResourceIndex::from_position(self.entries.len());
        self.entries.push(Some(AdapterEntry {
            resource_type,
            resource,
            kind: AdapterEntryKind::Owned { active_borrows: 0 },
        }));
        self.live_entries += 1;
        Ok(index)
    }

    fn ensure_reusable(&self, role: AdapterRole) -> Result<(), AdapterError> {
        if self.state == InstanceState::Poisoned {
            return Err(AdapterError::new(AdapterErrorKind::InstancePoisoned(role)));
        }
        Ok(())
    }

    fn reserve_entries(&mut self, additional: usize) -> Result<(), AdapterError> {
        let Some(next_live) = self.live_entries.checked_add(additional) else {
            return Err(AdapterError::new(AdapterErrorKind::LiveLimitExceeded {
                limit: self.limits.max_live_entries,
            }));
        };
        if next_live > self.limits.max_live_entries {
            return Err(AdapterError::new(AdapterErrorKind::LiveLimitExceeded {
                limit: self.limits.max_live_entries,
            }));
        }

        let Some(next_total) = self.entries.len().checked_add(additional) else {
            return Err(AdapterError::new(
                AdapterErrorKind::BoundaryIndexLimitExceeded {
                    limit: self.limits.max_boundary_indices,
                },
            ));
        };
        if next_total > self.limits.max_boundary_indices as usize {
            return Err(AdapterError::new(
                AdapterErrorKind::BoundaryIndexLimitExceeded {
                    limit: self.limits.max_boundary_indices,
                },
            ));
        }

        self.entries
            .try_reserve_exact(additional)
            .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))
    }

    fn entry(&self, index: BoundaryResourceIndex) -> Option<&AdapterEntry> {
        self.entries.get(index.position())?.as_ref()
    }

    fn entry_mut(&mut self, index: BoundaryResourceIndex) -> Option<&mut AdapterEntry> {
        self.entries.get_mut(index.position())?.as_mut()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferArgument {
    source: BoundaryResourceIndex,
    expected_type: ResourceTypeKey,
    ownership: CanonicalOwnership,
}

impl TransferArgument {
    pub fn new(
        source: BoundaryResourceIndex,
        expected_type: ResourceTypeKey,
        ownership: CanonicalOwnership,
    ) -> Self {
        Self {
            source,
            expected_type,
            ownership,
        }
    }

    pub const fn source(&self) -> BoundaryResourceIndex {
        self.source
    }

    pub fn expected_type(&self) -> &ResourceTypeKey {
        &self.expected_type
    }

    pub const fn ownership(&self) -> CanonicalOwnership {
        self.ownership
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedArgument {
    source: BoundaryResourceIndex,
    destination: BoundaryResourceIndex,
    resource: ResourceIdentity,
    ownership: CanonicalOwnership,
}

impl CommittedArgument {
    pub const fn source(self) -> BoundaryResourceIndex {
        self.source
    }

    pub const fn destination(self) -> BoundaryResourceIndex {
        self.destination
    }

    pub const fn resource(self) -> ResourceIdentity {
        self.resource
    }

    pub const fn ownership(self) -> CanonicalOwnership {
        self.ownership
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureCategory {
    Panic,
    Trap,
    Destructor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailurePhase {
    Call,
    Cleanup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryFailureRecord {
    category: FailureCategory,
    phase: FailurePhase,
    diagnostic: &'static str,
    message: Option<String>,
}

impl BoundaryFailureRecord {
    fn call(category: FailureCategory, diagnostic: &'static str) -> Self {
        Self {
            category,
            phase: FailurePhase::Call,
            diagnostic,
            message: None,
        }
    }

    fn cleanup(
        category: FailureCategory,
        diagnostic: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            category,
            phase: FailurePhase::Cleanup,
            diagnostic,
            message: Some(message.into()),
        }
    }

    pub const fn category(&self) -> FailureCategory {
        self.category
    }

    pub const fn phase(&self) -> FailurePhase {
        self.phase
    }

    pub const fn diagnostic(&self) -> &'static str {
        self.diagnostic
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryFailure {
    primary: BoundaryFailureRecord,
    suppressed: Vec<BoundaryFailureRecord>,
    poisoned: bool,
}

impl BoundaryFailure {
    pub fn primary(&self) -> &BoundaryFailureRecord {
        &self.primary
    }

    pub fn suppressed(&self) -> &[BoundaryFailureRecord] {
        &self.suppressed
    }

    pub const fn poisoned(&self) -> bool {
        self.poisoned
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallOutcome {
    Returned,
    GuestPanic,
    GuestTrap,
    ContainedHostCallbackPanic,
    AdapterInvariantViolation,
}

impl CallOutcome {
    fn primary_failure(self) -> Option<BoundaryFailureRecord> {
        match self {
            Self::Returned => None,
            Self::GuestPanic | Self::ContainedHostCallbackPanic => Some(
                BoundaryFailureRecord::call(FailureCategory::Panic, "component.boundary.panic"),
            ),
            Self::GuestTrap | Self::AdapterInvariantViolation => Some(BoundaryFailureRecord::call(
                FailureCategory::Trap,
                "component.boundary.trap",
            )),
        }
    }

    const fn poisons_instance(self) -> bool {
        matches!(
            self,
            Self::ContainedHostCallbackPanic | Self::AdapterInvariantViolation
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterTraceEvent {
    PreflightValidated {
        argument_count: usize,
    },
    CommitStarted,
    OwnershipTransferred {
        source: BoundaryResourceIndex,
        destination: BoundaryResourceIndex,
        resource: ResourceIdentity,
    },
    BorrowPublished {
        source: BoundaryResourceIndex,
        destination: BoundaryResourceIndex,
        resource: ResourceIdentity,
        scope_depth: u32,
    },
    CommitCompleted,
    ChildScopeOpened {
        depth: u32,
    },
    BorrowScopeClosed {
        destination: BoundaryResourceIndex,
        scope_depth: u32,
    },
    OwnerPersisted {
        destination: BoundaryResourceIndex,
    },
    OwnerRemovedBeforeDestructor {
        destination: BoundaryResourceIndex,
        resource: ResourceIdentity,
    },
    DestructorSucceeded {
        resource: ResourceIdentity,
    },
    DestructorFailed {
        resource: ResourceIdentity,
    },
    CleanupInvariantViolation,
    InstancePoisoned,
    CallCompleted {
        state: InstanceState,
    },
}

#[derive(Clone, Debug)]
struct PlannedArgument {
    committed: CommittedArgument,
    entry: AdapterEntry,
}

#[derive(Clone, Copy, Debug)]
struct BorrowLedgerEntry {
    source_owner: BoundaryResourceIndex,
    destination: BoundaryResourceIndex,
    scope_depth: u32,
    active: bool,
}

#[derive(Clone, Copy, Debug)]
struct OwnerLedgerEntry {
    destination: BoundaryResourceIndex,
    active: bool,
}

#[must_use = "a prepared component call must be committed or discarded"]
pub struct PreparedCall<'a> {
    caller: &'a mut CanonicalResourceTable,
    callee: &'a mut CanonicalResourceTable,
    plans: Vec<PlannedArgument>,
    arguments: Vec<CommittedArgument>,
    borrows: Vec<BorrowLedgerEntry>,
    owners: Vec<OwnerLedgerEntry>,
    trace: Vec<AdapterTraceEvent>,
}

impl<'a> PreparedCall<'a> {
    /// Commits a fully reserved transfer without allocation, callbacks, or a
    /// recoverable failure path.
    pub fn commit(mut self) -> ActiveCall<'a> {
        self.trace.push(AdapterTraceEvent::CommitStarted);
        for plan in self.plans {
            let argument = plan.committed;
            match argument.ownership {
                CanonicalOwnership::Own => {
                    self.caller.entries[argument.source.position()] = None;
                    self.caller.live_entries -= 1;
                    self.callee.entries.push(Some(plan.entry));
                    self.callee.live_entries += 1;
                    self.trace.push(AdapterTraceEvent::OwnershipTransferred {
                        source: argument.source,
                        destination: argument.destination,
                        resource: argument.resource,
                    });
                }
                CanonicalOwnership::Borrow => {
                    let Some(AdapterEntry {
                        kind: AdapterEntryKind::Owned { active_borrows },
                        ..
                    }) = self.caller.entry_mut(argument.source)
                    else {
                        unreachable!("preflight keeps borrowed source owners live")
                    };
                    *active_borrows += 1;
                    self.callee.entries.push(Some(plan.entry));
                    self.callee.live_entries += 1;
                    self.trace.push(AdapterTraceEvent::BorrowPublished {
                        source: argument.source,
                        destination: argument.destination,
                        resource: argument.resource,
                        scope_depth: 0,
                    });
                }
            }
        }
        self.trace.push(AdapterTraceEvent::CommitCompleted);
        ActiveCall {
            caller: self.caller,
            callee: self.callee,
            arguments: self.arguments,
            borrows: self.borrows,
            owners: self.owners,
            trace: self.trace,
            current_scope_depth: 0,
        }
    }
}

pub fn preflight_call<'a>(
    caller: &'a mut CanonicalResourceTable,
    callee: &'a mut CanonicalResourceTable,
    arguments: &[TransferArgument],
) -> Result<PreparedCall<'a>, AdapterError> {
    caller.ensure_reusable(AdapterRole::Caller)?;
    callee.ensure_reusable(AdapterRole::Callee)?;
    callee.reserve_entries(arguments.len())?;

    let mut plans = Vec::new();
    let mut committed = Vec::new();
    let mut borrows = Vec::new();
    let mut owners = Vec::new();
    let mut trace = Vec::new();
    plans
        .try_reserve_exact(arguments.len())
        .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;
    committed
        .try_reserve_exact(arguments.len())
        .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;
    borrows
        .try_reserve_exact(arguments.len())
        .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;
    owners
        .try_reserve_exact(arguments.len())
        .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;
    trace
        .try_reserve_exact(arguments.len() + 3)
        .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;

    let mut seen = BTreeSet::new();
    for (offset, argument) in arguments.iter().enumerate() {
        if !seen.insert(argument.source) {
            return Err(AdapterError::new(AdapterErrorKind::AliasedArgument(
                argument.source,
            )));
        }

        let Some(source_entry) = caller.entry(argument.source) else {
            return Err(AdapterError::new(AdapterErrorKind::StaleBoundaryIndex(
                argument.source,
            )));
        };
        if source_entry.resource_type != argument.expected_type {
            return Err(AdapterError::new(AdapterErrorKind::ResourceTypeMismatch {
                index: argument.source,
                expected: argument.expected_type.clone(),
                actual: source_entry.resource_type.clone(),
            }));
        }
        let AdapterEntryKind::Owned { active_borrows } = source_entry.kind else {
            return Err(AdapterError::new(AdapterErrorKind::ExpectedOwned(
                argument.source,
            )));
        };
        if argument.ownership == CanonicalOwnership::Own && active_borrows != 0 {
            return Err(AdapterError::new(AdapterErrorKind::ActiveLends {
                index: argument.source,
                count: active_borrows,
            }));
        }
        if argument.ownership == CanonicalOwnership::Borrow
            && active_borrows.checked_add(1).is_none()
        {
            return Err(AdapterError::new(AdapterErrorKind::BorrowCountExhausted(
                argument.source,
            )));
        }

        let destination = BoundaryResourceIndex::from_position(callee.entries.len() + offset);
        let committed_argument = CommittedArgument {
            source: argument.source,
            destination,
            resource: source_entry.resource,
            ownership: argument.ownership,
        };
        let kind = match argument.ownership {
            CanonicalOwnership::Own => {
                owners.push(OwnerLedgerEntry {
                    destination,
                    active: true,
                });
                AdapterEntryKind::Owned { active_borrows: 0 }
            }
            CanonicalOwnership::Borrow => {
                borrows.push(BorrowLedgerEntry {
                    source_owner: argument.source,
                    destination,
                    scope_depth: 0,
                    active: true,
                });
                AdapterEntryKind::Borrowed {
                    source_owner: argument.source,
                    scope_depth: 0,
                }
            }
        };
        plans.push(PlannedArgument {
            committed: committed_argument,
            entry: AdapterEntry {
                resource_type: source_entry.resource_type.clone(),
                resource: source_entry.resource,
                kind,
            },
        });
        committed.push(committed_argument);
    }

    trace.push(AdapterTraceEvent::PreflightValidated {
        argument_count: arguments.len(),
    });
    Ok(PreparedCall {
        caller,
        callee,
        plans,
        arguments: committed,
        borrows,
        owners,
        trace,
    })
}

#[must_use = "an active component call must be finished to close borrows and clean owners"]
pub struct ActiveCall<'a> {
    caller: &'a mut CanonicalResourceTable,
    callee: &'a mut CanonicalResourceTable,
    arguments: Vec<CommittedArgument>,
    borrows: Vec<BorrowLedgerEntry>,
    owners: Vec<OwnerLedgerEntry>,
    trace: Vec<AdapterTraceEvent>,
    current_scope_depth: u32,
}

impl ActiveCall<'_> {
    pub fn arguments(&self) -> &[CommittedArgument] {
        &self.arguments
    }

    pub fn open_child_scope(&mut self) -> Result<u32, AdapterError> {
        let Some(depth) = self.current_scope_depth.checked_add(1) else {
            return Err(AdapterError::new(AdapterErrorKind::ScopeDepthExhausted));
        };
        self.trace
            .try_reserve_exact(1)
            .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;
        self.current_scope_depth = depth;
        self.trace
            .push(AdapterTraceEvent::ChildScopeOpened { depth });
        Ok(depth)
    }

    pub fn reborrow(
        &mut self,
        source: BoundaryResourceIndex,
    ) -> Result<BoundaryResourceIndex, AdapterError> {
        if self.current_scope_depth == 0 {
            return Err(AdapterError::new(AdapterErrorKind::NoChildBorrowScope));
        }
        let Some(source_entry) = self.callee.entry(source) else {
            return Err(AdapterError::new(AdapterErrorKind::StaleBoundaryIndex(
                source,
            )));
        };
        let AdapterEntryKind::Borrowed { source_owner, .. } = source_entry.kind else {
            return Err(AdapterError::new(AdapterErrorKind::ExpectedBorrowed(
                source,
            )));
        };
        let Some(AdapterEntry {
            kind: AdapterEntryKind::Owned { active_borrows },
            ..
        }) = self.caller.entry(source_owner)
        else {
            return Err(AdapterError::new(AdapterErrorKind::StaleBoundaryIndex(
                source_owner,
            )));
        };
        if active_borrows.checked_add(1).is_none() {
            return Err(AdapterError::new(AdapterErrorKind::BorrowCountExhausted(
                source_owner,
            )));
        }

        let resource = source_entry.resource;
        let entry = AdapterEntry {
            resource_type: source_entry.resource_type.clone(),
            resource,
            kind: AdapterEntryKind::Borrowed {
                source_owner,
                scope_depth: self.current_scope_depth,
            },
        };
        self.callee.reserve_entries(1)?;
        self.borrows
            .try_reserve_exact(1)
            .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;
        self.trace
            .try_reserve_exact(1)
            .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;

        let destination = BoundaryResourceIndex::from_position(self.callee.entries.len());
        let Some(AdapterEntry {
            kind: AdapterEntryKind::Owned { active_borrows },
            ..
        }) = self.caller.entry_mut(source_owner)
        else {
            unreachable!("reborrow preflight keeps the source owner live")
        };
        *active_borrows += 1;
        self.callee.entries.push(Some(entry));
        self.callee.live_entries += 1;
        self.borrows.push(BorrowLedgerEntry {
            source_owner,
            destination,
            scope_depth: self.current_scope_depth,
            active: true,
        });
        self.trace.push(AdapterTraceEvent::BorrowPublished {
            source,
            destination,
            resource,
            scope_depth: self.current_scope_depth,
        });
        Ok(destination)
    }

    pub fn close_child_scope(&mut self) -> Result<(), AdapterError> {
        if self.current_scope_depth == 0 {
            return Err(AdapterError::new(AdapterErrorKind::NoChildBorrowScope));
        }
        let depth = self.current_scope_depth;
        let event_count = self
            .borrows
            .iter()
            .filter(|entry| entry.active && entry.scope_depth == depth)
            .count();
        self.trace
            .try_reserve_exact(event_count)
            .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;
        let _ = self.close_scope(depth);
        self.current_scope_depth -= 1;
        Ok(())
    }

    pub fn persist_owner(
        &mut self,
        destination: BoundaryResourceIndex,
    ) -> Result<(), AdapterError> {
        let Some(entry) = self.callee.entry(destination) else {
            return Err(AdapterError::new(AdapterErrorKind::StaleBoundaryIndex(
                destination,
            )));
        };
        if !matches!(entry.kind, AdapterEntryKind::Owned { .. }) {
            return Err(AdapterError::new(AdapterErrorKind::ExpectedOwned(
                destination,
            )));
        }
        let Some(owner) = self
            .owners
            .iter_mut()
            .find(|entry| entry.active && entry.destination == destination)
        else {
            return Err(AdapterError::new(AdapterErrorKind::NotFrameOwned(
                destination,
            )));
        };
        self.trace
            .try_reserve_exact(1)
            .map_err(|_| AdapterError::new(AdapterErrorKind::ReservationFailed))?;
        owner.active = false;
        self.trace
            .push(AdapterTraceEvent::OwnerPersisted { destination });
        Ok(())
    }

    pub fn finish<F, E>(mut self, outcome: CallOutcome, mut destroy: F) -> CallCompletion
    where
        F: FnMut(ResourceIdentity) -> Result<(), E>,
        E: Display,
    {
        let mut primary = outcome.primary_failure();
        let mut suppressed = Vec::new();
        let mut poisoned = outcome.poisons_instance();

        while self.current_scope_depth != 0 {
            let depth = self.current_scope_depth;
            if !self.close_scope(depth) {
                record_cleanup_invariant(&mut primary, &mut suppressed);
                poisoned = true;
            }
            self.current_scope_depth -= 1;
        }
        if !self.close_scope(0) {
            record_cleanup_invariant(&mut primary, &mut suppressed);
            poisoned = true;
        }

        for ledger_index in (0..self.owners.len()).rev() {
            if !self.owners[ledger_index].active {
                continue;
            }
            self.owners[ledger_index].active = false;
            let destination = self.owners[ledger_index].destination;
            let Some(entry) = self
                .callee
                .entries
                .get_mut(destination.position())
                .and_then(Option::take)
            else {
                self.trace
                    .push(AdapterTraceEvent::CleanupInvariantViolation);
                record_cleanup_invariant(&mut primary, &mut suppressed);
                poisoned = true;
                continue;
            };
            let AdapterEntryKind::Owned { .. } = entry.kind else {
                self.trace
                    .push(AdapterTraceEvent::CleanupInvariantViolation);
                record_cleanup_invariant(&mut primary, &mut suppressed);
                poisoned = true;
                continue;
            };
            self.callee.live_entries -= 1;
            self.trace
                .push(AdapterTraceEvent::OwnerRemovedBeforeDestructor {
                    destination,
                    resource: entry.resource,
                });

            let result = catch_unwind(AssertUnwindSafe(|| destroy(entry.resource)));
            match result {
                Ok(Ok(())) => self.trace.push(AdapterTraceEvent::DestructorSucceeded {
                    resource: entry.resource,
                }),
                Ok(Err(error)) => {
                    self.trace.push(AdapterTraceEvent::DestructorFailed {
                        resource: entry.resource,
                    });
                    record_failure(
                        &mut primary,
                        &mut suppressed,
                        BoundaryFailureRecord::cleanup(
                            FailureCategory::Destructor,
                            "component.boundary.destructor-failure",
                            error.to_string(),
                        ),
                    );
                    poisoned = true;
                }
                Err(_) => {
                    self.trace.push(AdapterTraceEvent::DestructorFailed {
                        resource: entry.resource,
                    });
                    record_failure(
                        &mut primary,
                        &mut suppressed,
                        BoundaryFailureRecord::cleanup(
                            FailureCategory::Destructor,
                            "component.boundary.destructor-failure",
                            "resource destructor panicked",
                        ),
                    );
                    poisoned = true;
                }
            }
        }

        if poisoned {
            self.callee.state = InstanceState::Poisoned;
            self.trace.push(AdapterTraceEvent::InstancePoisoned);
        }
        self.trace.push(AdapterTraceEvent::CallCompleted {
            state: self.callee.state,
        });

        let failure = primary.map(|primary| BoundaryFailure {
            primary,
            suppressed,
            poisoned,
        });
        CallCompletion {
            failure,
            state: self.callee.state,
            trace: self.trace,
        }
    }

    fn close_scope(&mut self, depth: u32) -> bool {
        let mut valid = true;
        for ledger_index in (0..self.borrows.len()).rev() {
            let ledger = &mut self.borrows[ledger_index];
            if !ledger.active || ledger.scope_depth != depth {
                continue;
            }
            ledger.active = false;

            let destination_closed = self
                .callee
                .entries
                .get_mut(ledger.destination.position())
                .and_then(Option::take)
                .is_some();
            if destination_closed {
                self.callee.live_entries -= 1;
            } else {
                valid = false;
            }

            let owner_closed = match self.caller.entry_mut(ledger.source_owner) {
                Some(AdapterEntry {
                    kind: AdapterEntryKind::Owned { active_borrows },
                    ..
                }) if *active_borrows != 0 => {
                    *active_borrows -= 1;
                    true
                }
                _ => false,
            };
            if !owner_closed {
                valid = false;
            }

            if destination_closed && owner_closed {
                self.trace.push(AdapterTraceEvent::BorrowScopeClosed {
                    destination: ledger.destination,
                    scope_depth: depth,
                });
            } else {
                self.trace
                    .push(AdapterTraceEvent::CleanupInvariantViolation);
            }
        }
        valid
    }
}

fn record_failure(
    primary: &mut Option<BoundaryFailureRecord>,
    suppressed: &mut Vec<BoundaryFailureRecord>,
    failure: BoundaryFailureRecord,
) {
    if primary.is_none() {
        *primary = Some(failure);
    } else {
        suppressed.push(failure);
    }
}

fn record_cleanup_invariant(
    primary: &mut Option<BoundaryFailureRecord>,
    suppressed: &mut Vec<BoundaryFailureRecord>,
) {
    record_failure(
        primary,
        suppressed,
        BoundaryFailureRecord::cleanup(
            FailureCategory::Trap,
            "component.boundary.trap",
            "adapter cleanup invariant violated",
        ),
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallCompletion {
    failure: Option<BoundaryFailure>,
    state: InstanceState,
    trace: Vec<AdapterTraceEvent>,
}

impl CallCompletion {
    pub fn failure(&self) -> Option<&BoundaryFailure> {
        self.failure.as_ref()
    }

    pub const fn state(&self) -> InstanceState {
        self.state
    }

    pub fn trace(&self) -> &[AdapterTraceEvent] {
        &self.trace
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AdapterErrorKind, AdapterLimits, AdapterRole, AdapterTraceEvent, BoundaryResourceIndex,
        CallOutcome, CanonicalOwnership, CanonicalResourceTable, FailureCategory, FailurePhase,
        InstanceState, ResourceIdentity, TransferArgument, preflight_call,
    };
    use crate::component::negotiation::{InterfaceKey, PackageKey, ResourceTypeKey};

    fn resource_type(name: &str) -> ResourceTypeKey {
        ResourceTypeKey::new(
            InterfaceKey::new(PackageKey::new("example", "clock"), "timers"),
            name,
        )
    }

    fn unlimited_table() -> CanonicalResourceTable {
        CanonicalResourceTable::new(AdapterLimits::default())
    }

    fn one_argument(
        source: BoundaryResourceIndex,
        resource_type: ResourceTypeKey,
        ownership: CanonicalOwnership,
    ) -> [TransferArgument; 1] {
        [TransferArgument::new(source, resource_type, ownership)]
    }

    #[test]
    fn preflight_rejects_the_complete_list_without_changing_ownership() {
        let timer = resource_type("timer");
        let socket = resource_type("socket");
        let mut caller = unlimited_table();
        let first = caller
            .insert_owned(timer.clone(), ResourceIdentity::new(1))
            .expect("first owner should fit");
        let second = caller
            .insert_owned(socket, ResourceIdentity::new(2))
            .expect("second owner should fit");
        let mut callee = unlimited_table();

        let error = match preflight_call(
            &mut caller,
            &mut callee,
            &[
                TransferArgument::new(first, timer.clone(), CanonicalOwnership::Own),
                TransferArgument::new(second, timer.clone(), CanonicalOwnership::Own),
            ],
        ) {
            Ok(_) => panic!("the second argument has the wrong resource type"),
            Err(error) => error,
        };

        assert!(matches!(
            error.kind(),
            AdapterErrorKind::ResourceTypeMismatch { index, .. } if *index == second
        ));
        assert!(caller.is_live(first));
        assert!(caller.is_live(second));
        assert_eq!(caller.len(), 2);
        assert!(callee.is_empty());
        assert_eq!(callee.allocated_index_count(), 0);

        let alias_error = match preflight_call(
            &mut caller,
            &mut callee,
            &[
                TransferArgument::new(first, timer.clone(), CanonicalOwnership::Own),
                TransferArgument::new(first, timer, CanonicalOwnership::Borrow),
            ],
        ) {
            Ok(_) => panic!("one resource cannot be both owned and borrowed"),
            Err(error) => error,
        };
        assert_eq!(
            alias_error.kind(),
            &AdapterErrorKind::AliasedArgument(first)
        );
        assert!(caller.is_live(first));
        assert!(callee.is_empty());
    }

    #[test]
    fn committed_owners_make_old_indices_stale_and_indices_are_never_reused() {
        let timer = resource_type("timer");
        let mut left = unlimited_table();
        let old_left = left
            .insert_owned(timer.clone(), ResourceIdentity::new(7))
            .expect("owner should fit");
        let mut right = unlimited_table();

        let mut call = preflight_call(
            &mut left,
            &mut right,
            &one_argument(old_left, timer.clone(), CanonicalOwnership::Own),
        )
        .expect("transfer should preflight")
        .commit();
        let right_index = call.arguments()[0].destination();
        call.persist_owner(right_index)
            .expect("callee should persist the owner");
        let completion = call.finish(CallOutcome::Returned, |_| Ok::<_, &'static str>(()));
        assert!(completion.failure().is_none());
        assert!(!left.is_live(old_left));
        assert!(right.is_live(right_index));

        let mut return_call = preflight_call(
            &mut right,
            &mut left,
            &one_argument(right_index, timer, CanonicalOwnership::Own),
        )
        .expect("return transfer should preflight")
        .commit();
        let new_left = return_call.arguments()[0].destination();
        return_call
            .persist_owner(new_left)
            .expect("caller should retain the returned owner");
        let completion = return_call.finish(CallOutcome::Returned, |_| Ok::<_, &'static str>(()));

        assert!(completion.failure().is_none());
        assert_eq!(old_left.get(), 1);
        assert_eq!(new_left.get(), 2);
        assert!(!left.is_live(old_left));
        assert!(left.is_live(new_left));
    }

    #[test]
    fn nested_borrow_scopes_close_inner_to_outer() {
        let timer = resource_type("timer");
        let mut caller = unlimited_table();
        let owner = caller
            .insert_owned(timer.clone(), ResourceIdentity::new(11))
            .expect("owner should fit");
        let mut callee = unlimited_table();
        let mut call = preflight_call(
            &mut caller,
            &mut callee,
            &one_argument(owner, timer, CanonicalOwnership::Borrow),
        )
        .expect("borrow should preflight")
        .commit();
        let root_borrow = call.arguments()[0].destination();
        assert_eq!(call.open_child_scope(), Ok(1));
        let child_borrow = call.reborrow(root_borrow).expect("reborrow should fit");
        let completion = call.finish(CallOutcome::Returned, |_| Ok::<_, &'static str>(()));

        let closed = completion
            .trace()
            .iter()
            .filter_map(|event| match event {
                AdapterTraceEvent::BorrowScopeClosed {
                    destination,
                    scope_depth,
                } => Some((*destination, *scope_depth)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(closed, vec![(child_borrow, 1), (root_borrow, 0)]);
        assert_eq!(caller.active_borrow_count(owner), Some(0));
        assert!(callee.is_empty());
        assert_eq!(
            caller.resource_identity(owner),
            Some(ResourceIdentity::new(11))
        );
    }

    #[test]
    fn owner_cleanup_is_reverse_order_and_destructor_failures_are_ordered() {
        let timer = resource_type("timer");
        let mut caller = unlimited_table();
        let first = caller
            .insert_owned(timer.clone(), ResourceIdentity::new(21))
            .expect("first owner should fit");
        let second = caller
            .insert_owned(timer.clone(), ResourceIdentity::new(22))
            .expect("second owner should fit");
        let mut callee = unlimited_table();
        let call = preflight_call(
            &mut caller,
            &mut callee,
            &[
                TransferArgument::new(first, timer.clone(), CanonicalOwnership::Own),
                TransferArgument::new(second, timer, CanonicalOwnership::Own),
            ],
        )
        .expect("owners should preflight")
        .commit();
        let mut destroyed = Vec::new();
        let completion = call.finish(CallOutcome::GuestTrap, |resource| {
            destroyed.push(resource);
            Err::<(), _>(format!("destructor {} failed", resource.get()))
        });

        assert_eq!(
            destroyed,
            vec![ResourceIdentity::new(22), ResourceIdentity::new(21)]
        );
        let failure = completion.failure().expect("trap should be reported");
        assert_eq!(failure.primary().category(), FailureCategory::Trap);
        assert_eq!(failure.primary().phase(), FailurePhase::Call);
        assert_eq!(failure.suppressed().len(), 2);
        assert!(
            failure
                .suppressed()
                .iter()
                .all(|failure| failure.category() == FailureCategory::Destructor)
        );
        assert!(failure.poisoned());
        assert_eq!(completion.state(), InstanceState::Poisoned);
        assert!(callee.is_empty());

        for resource in destroyed {
            let removed = completion.trace().iter().position(|event| {
                matches!(
                    event,
                    AdapterTraceEvent::OwnerRemovedBeforeDestructor {
                        resource: traced,
                        ..
                    } if *traced == resource
                )
            });
            let failed = completion.trace().iter().position(|event| {
                matches!(
                    event,
                    AdapterTraceEvent::DestructorFailed { resource: traced }
                        if *traced == resource
                )
            });
            assert!(removed < failed);
        }
    }

    #[test]
    fn guest_panic_after_commit_does_not_roll_back_but_can_leave_instance_reusable() {
        let timer = resource_type("timer");
        let mut caller = unlimited_table();
        let owner = caller
            .insert_owned(timer.clone(), ResourceIdentity::new(31))
            .expect("owner should fit");
        let mut callee = unlimited_table();
        let call = preflight_call(
            &mut caller,
            &mut callee,
            &one_argument(owner, timer, CanonicalOwnership::Own),
        )
        .expect("owner should preflight")
        .commit();
        let completion = call.finish(CallOutcome::GuestPanic, |_| Ok::<_, &'static str>(()));

        let failure = completion.failure().expect("panic should be reported");
        assert_eq!(failure.primary().category(), FailureCategory::Panic);
        assert!(!failure.poisoned());
        assert_eq!(completion.state(), InstanceState::Reusable);
        assert!(!caller.is_live(owner));
        assert!(callee.is_empty());
    }

    #[test]
    fn contained_host_callback_panic_poisons_later_entry() {
        let timer = resource_type("timer");
        let mut caller = unlimited_table();
        let first = caller
            .insert_owned(timer.clone(), ResourceIdentity::new(41))
            .expect("first owner should fit");
        let second = caller
            .insert_owned(timer.clone(), ResourceIdentity::new(42))
            .expect("second owner should fit");
        let mut callee = unlimited_table();
        let call = preflight_call(
            &mut caller,
            &mut callee,
            &one_argument(first, timer.clone(), CanonicalOwnership::Own),
        )
        .expect("owner should preflight")
        .commit();
        let completion = call.finish(CallOutcome::ContainedHostCallbackPanic, |_| {
            Ok::<_, &'static str>(())
        });
        assert_eq!(completion.state(), InstanceState::Poisoned);

        let error = match preflight_call(
            &mut caller,
            &mut callee,
            &one_argument(second, timer, CanonicalOwnership::Own),
        ) {
            Ok(_) => panic!("a poisoned callee must reject later entry"),
            Err(error) => error,
        };
        assert_eq!(
            error.kind(),
            &AdapterErrorKind::InstancePoisoned(AdapterRole::Callee)
        );
        assert!(caller.is_live(second));
    }

    #[test]
    fn destination_limits_fail_before_any_source_mutation() {
        let timer = resource_type("timer");
        let mut caller = unlimited_table();
        let owner = caller
            .insert_owned(timer.clone(), ResourceIdentity::new(51))
            .expect("owner should fit");
        let mut callee = CanonicalResourceTable::new(AdapterLimits::new(0, 0));

        let error = match preflight_call(
            &mut caller,
            &mut callee,
            &one_argument(owner, timer, CanonicalOwnership::Own),
        ) {
            Ok(_) => panic!("destination has no live capacity"),
            Err(error) => error,
        };
        assert_eq!(
            error.kind(),
            &AdapterErrorKind::LiveLimitExceeded { limit: 0 }
        );
        assert!(caller.is_live(owner));
        assert!(callee.is_empty());
    }
}
