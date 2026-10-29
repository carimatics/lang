//! Bounded logical allocator observations, separate from guest accounting.
use super::{Identity, Request, Stats};
use std::fmt::{Debug, Display, Formatter};
use std::sync::Mutex;

pub const MAX_TRACE_EVENTS: usize = 16_384;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    OutOfMemory,
    InvalidStorage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventKind {
    Allocated { generation: u64 },
    AllocationFailed(FailureKind),
    Retired { generation: u64 },
}

/// An admitted operation and the original allocator's post-operation counters.
/// Generations identify logical owners, including zero-byte owners; no address,
/// region, byte contents or allocation capability is retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub request: Request,
    pub stats: Stats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceError {
    InvalidLimit,
    Storage,
    Capacity,
    Poisoned,
    AlreadyTracing,
    NotFresh,
    BorrowedAllocator,
    MissingAuthority,
}

impl Display for TraceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidLimit => "allocator trace limit must be between 1 and 16384",
            Self::Storage => "cannot reserve allocator trace metadata",
            Self::Capacity => "allocator trace event limit exceeded",
            Self::Poisoned => "allocator trace state is poisoned",
            Self::AlreadyTracing => "allocator trace is already attached",
            Self::NotFresh => "allocator trace requires an allocator with no admitted requests",
            Self::BorrowedAllocator => "attach allocator trace to the original allocator",
            Self::MissingAuthority => "allocator trace requires live allocator authority",
        })
    }
}
impl std::error::Error for TraceError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceSnapshot {
    pub allocator: Identity,
    pub events: Vec<Event>,
    pub stats: Stats,
}

struct State {
    events: Vec<Event>,
    stats: Stats,
    failure: Option<TraceError>,
}

/// Read-only observation receipt. Metadata is reserved at host setup, and
/// recording never grows that buffer or calls foreign code. Observation failure
/// latches separately and never changes the guest operation's result.
pub struct Trace {
    allocator: Identity,
    limit: usize,
    state: Mutex<State>,
}

impl Debug for Trace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trace")
            .field("allocator", &self.allocator)
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}

impl Trace {
    pub(super) fn new(allocator: Identity, limit: usize) -> Result<Self, TraceError> {
        if !(1..=MAX_TRACE_EVENTS).contains(&limit) {
            return Err(TraceError::InvalidLimit);
        }
        let mut events = Vec::new();
        events
            .try_reserve_exact(limit)
            .map_err(|_| TraceError::Storage)?;
        Ok(Self {
            allocator,
            limit,
            state: Mutex::new(State {
                events,
                stats: Stats::default(),
                failure: None,
            }),
        })
    }

    pub(super) fn record(&self, event: Event) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.failure.is_some() {
            return;
        }
        if state.events.len() == self.limit {
            state.failure = Some(TraceError::Capacity);
            return;
        }
        state.stats = event.stats;
        state.events.push(event);
    }

    /// A bounded prefix, or an explicit error; never a truncated success.
    /// At an execution boundary the caller must compare `stats` with the original
    /// allocator. This does not release still-live owners or imply teardown.
    pub fn snapshot(&self) -> Result<TraceSnapshot, TraceError> {
        let state = self.state.lock().map_err(|_| TraceError::Poisoned)?;
        if let Some(error) = state.failure {
            return Err(error);
        }
        let mut events = Vec::new();
        events
            .try_reserve_exact(state.events.len())
            .map_err(|_| TraceError::Storage)?;
        events.extend_from_slice(&state.events);
        Ok(TraceSnapshot {
            allocator: self.allocator,
            events,
            stats: state.stats,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocation::{Allocator, Error, Limits, Provider, Storage, System};

    #[derive(Default)]
    struct ProviderCounts {
        attempts: usize,
        frees: usize,
    }
    impl Provider for ProviderCounts {
        type Region = Storage;
        fn allocate(&mut self, request: Request) -> Option<Storage> {
            self.attempts += 1;
            if request.bytes() == 64 {
                None
            } else {
                System.allocate(request)
            }
        }
        fn deallocate(&mut self, region: Storage, request: Request) {
            self.frees += 1;
            System.deallocate(region, request);
        }
    }
    fn allocator() -> Allocator<ProviderCounts> {
        Allocator::new(
            ProviderCounts::default(),
            Limits {
                live_allocations: 8,
                ..Default::default()
            },
        )
        .unwrap()
    }
    fn exercise(a: &mut Allocator<ProviderCounts>) {
        let zero = a.allocate(Request::new(0, 8).unwrap()).unwrap();
        let mut value = a.allocate(Request::new(16, 8).unwrap()).unwrap();
        a.bytes_mut(value).unwrap().fill(42);
        let old = value;
        a.resize(&mut value, 32).unwrap();
        assert_eq!(&a.bytes(value).unwrap()[..16], &[42; 16]);
        assert_eq!(a.release(old), Err(Error::StaleHandle));
        a.release(value).unwrap();
        a.release(zero).unwrap();
        assert_eq!(
            a.allocate(Request::new(64, 8).unwrap()),
            Err(Error::OutOfMemory)
        );
    }

    #[test]
    fn logical_trace_includes_zero_resize_and_refunded_oom_without_provider_events() {
        let mut original = allocator();
        exercise(&mut original);
        let mut observed = allocator();
        let trace = observed.start_trace(7).unwrap();
        exercise(&mut observed);
        let snapshot = trace.snapshot().unwrap();
        assert_eq!(snapshot.allocator, observed.identity());
        assert_eq!(snapshot.stats, original.stats());
        assert_eq!(snapshot.stats, observed.stats());
        assert_eq!(snapshot.stats.requests, 4);
        assert_eq!(snapshot.stats.allocations, 3);
        assert_eq!(snapshot.stats.deallocations, 3);
        assert_eq!(snapshot.stats.cumulative_bytes, 112);
        assert_eq!(snapshot.stats.peak_live_bytes, 48);
        assert_eq!(snapshot.stats.live_allocations, 0);
        assert_eq!(
            snapshot.events.iter().map(|e| e.kind).collect::<Vec<_>>(),
            [
                EventKind::Allocated { generation: 1 },
                EventKind::Allocated { generation: 2 },
                EventKind::Allocated { generation: 3 },
                EventKind::Retired { generation: 2 },
                EventKind::Retired { generation: 3 },
                EventKind::Retired { generation: 1 },
                EventKind::AllocationFailed(FailureKind::OutOfMemory),
            ]
        );
        assert_eq!(
            snapshot
                .events
                .iter()
                .map(|e| e.stats.live_bytes)
                .collect::<Vec<_>>(),
            [0, 16, 48, 32, 0, 0, 0]
        );
        assert_eq!(
            snapshot
                .events
                .iter()
                .map(|e| e.stats.live_allocations)
                .collect::<Vec<_>>(),
            [1, 2, 3, 2, 1, 0, 0]
        );
        for a in [&observed, &original] {
            assert_eq!(a.provider().unwrap().attempts, 3);
            assert_eq!(a.provider().unwrap().frees, 2);
        }
    }

    #[test]
    fn trace_exhaustion_preserves_results_and_cannot_return_partial_success() {
        let mut a = allocator();
        let trace = a.start_trace(1).unwrap();
        exercise(&mut a);
        let mut original = allocator();
        exercise(&mut original);
        assert_eq!(a.stats(), original.stats());
        assert_eq!(a.provider().unwrap().attempts, 3);
        assert_eq!(a.provider().unwrap().frees, 2);
        assert_eq!(trace.snapshot(), Err(TraceError::Capacity));
        drop(a);
        assert_eq!(trace.snapshot(), Err(TraceError::Capacity));
    }

    #[test]
    fn trace_requires_fresh_root_and_nested_borrows_record_once() {
        let mut a = allocator();
        for limit in [0, MAX_TRACE_EVENTS + 1] {
            assert_eq!(a.start_trace(limit).unwrap_err(), TraceError::InvalidLimit);
        }
        let trace = a.start_trace(4).unwrap();
        assert_eq!(a.start_trace(4).unwrap_err(), TraceError::AlreadyTracing);
        assert_eq!(trace.snapshot().unwrap().stats, Stats::default());
        {
            let mut view = Allocator::borrow(&mut a).unwrap();
            assert_eq!(
                view.start_trace(4).unwrap_err(),
                TraceError::BorrowedAllocator
            );
            let mut nested = Allocator::borrow(&mut view).unwrap();
            let handle = nested.allocate(Request::new(0, 8).unwrap()).unwrap();
            nested.release(handle).unwrap();
        }
        assert_eq!(trace.snapshot().unwrap().events.len(), 2);
        assert_eq!(trace.snapshot().unwrap().stats, a.stats());
        let mut late = allocator();
        let handle = late.allocate(Request::new(0, 8).unwrap()).unwrap();
        late.release(handle).unwrap();
        assert_eq!(late.start_trace(4).unwrap_err(), TraceError::NotFresh);
        let mut revoked = allocator();
        revoked.revoke().unwrap();
        assert_eq!(
            revoked.start_trace(4).unwrap_err(),
            TraceError::MissingAuthority
        );
    }

    #[test]
    fn invalid_provider_storage_is_recorded_after_refund_and_retirement() {
        struct Short {
            frees: usize,
        }
        impl Provider for Short {
            type Region = Storage;
            fn allocate(&mut self, _: Request) -> Option<Storage> {
                System.allocate(Request::new(1, 1).unwrap())
            }
            fn deallocate(&mut self, _: Storage, _: Request) {
                self.frees += 1;
            }
        }
        let mut a = Allocator::new(
            Short { frees: 0 },
            Limits {
                live_allocations: 2,
                ..Default::default()
            },
        )
        .unwrap();
        let trace = a.start_trace(2).unwrap();
        let request = Request::new(16, 1).unwrap();
        assert_eq!(a.allocate(request), Err(Error::InvalidStorage));
        assert_eq!(a.provider().unwrap().frees, 1);
        let snapshot = trace.snapshot().unwrap();
        assert_eq!(
            snapshot.events,
            [Event {
                kind: EventKind::AllocationFailed(FailureKind::InvalidStorage),
                request,
                stats: a.stats()
            }]
        );
        assert_eq!(snapshot.stats.live_allocations, 0);
        assert_eq!(snapshot.stats.cumulative_bytes, 16);
        assert_eq!(snapshot.stats.allocations, 0);
        assert_eq!(snapshot.stats.deallocations, 0);
    }

    #[test]
    fn preflight_refusal_is_absent_and_snapshot_does_not_release_live_owners() {
        let mut a = Allocator::new(
            System,
            Limits {
                request_bytes: 32,
                live_allocations: 2,
                ..Default::default()
            },
        )
        .unwrap();
        let trace = a.start_trace(4).unwrap();
        let rejected = Request::new(64, 8).unwrap();
        assert!(a.check(&[rejected]).is_err());
        assert!(a.allocate(rejected).is_err());
        assert!(trace.snapshot().unwrap().events.is_empty());
        assert_eq!(a.stats(), Stats::default());
        let handle = a.allocate(Request::new(8, 8).unwrap()).unwrap();
        let boundary = trace.snapshot().unwrap();
        assert_eq!(boundary.stats, a.stats());
        assert_eq!(boundary.stats.live_allocations, 1);
        assert_eq!(a.bytes(handle).unwrap().len(), 8);
        drop(a);
        assert_eq!(boundary.events.len(), 1);
        assert_eq!(boundary.stats.live_bytes, 8);
        let after_teardown = trace.snapshot().unwrap();
        assert_eq!(after_teardown.events.len(), 2);
        assert_eq!(after_teardown.stats.live_allocations, 0);
    }
}
