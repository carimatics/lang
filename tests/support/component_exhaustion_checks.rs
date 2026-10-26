use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Observed {
    pub(super) allocation: a::Stats,
    pub(super) requests: Vec<a::Request>,
    pub(super) usage: e::Usage,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Bounds {
    pub(super) allocator: a::Limits,
    pub(super) execution: e::Limits,
    pub(super) service_work: u64,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Refusal {
    Allocation(a::Limit),
    Fuel,
    HostCalls,
    Frames,
    Cells,
    Work,
    Provider(usize),
}
impl Refusal {
    pub(super) fn bounds(self, mut bounds: Bounds) -> Bounds {
        match self {
            Self::Allocation(a::Limit::RequestBytes) => bounds.allocator.request_bytes -= 1,
            Self::Allocation(a::Limit::LiveBytes) => bounds.allocator.live_bytes -= 1,
            Self::Allocation(a::Limit::CumulativeBytes) => bounds.allocator.cumulative_bytes -= 1,
            Self::Allocation(a::Limit::LiveAllocations) => bounds.allocator.live_allocations -= 1,
            Self::Allocation(a::Limit::Alignment) => bounds.allocator.alignment /= 2,
            Self::Fuel => *bounds.execution.fuel.as_mut().unwrap() -= 1,
            Self::HostCalls => *bounds.execution.host_calls.as_mut().unwrap() -= 1,
            Self::Frames => *bounds.execution.frames.as_mut().unwrap() -= 1,
            Self::Cells => *bounds.execution.live_cells.as_mut().unwrap() -= 1,
            Self::Work => bounds.service_work -= 1,
            Self::Provider(_) => (),
        }
        bounds
    }
    pub(super) fn cleanup_failure(self) -> bool {
        matches!(
            self,
            Self::Allocation(
                a::Limit::LiveBytes | a::Limit::CumulativeBytes | a::Limit::LiveAllocations
            ) | Self::HostCalls
                | Self::Frames
                | Self::Cells
                | Self::Provider(_)
        )
    }
    pub(super) fn check(self, reason: &r::Reason, bounds: Bounds) {
        fn cause(reason: &r::Reason) -> &r::Reason {
            match reason.without_native_frames() {
                r::Reason::Native(execution::NativeFailure::Child(failure)) => {
                    cause(&failure.reason)
                }
                reason => reason.primary_cause(),
            }
        }
        let reason = cause(reason);
        match self {
            Self::Allocation(expected) => {
                let r::Reason::Layout(lang_runtime::layout::Error::Allocation(a::Error::Limit(
                    actual,
                ))) = reason
                else {
                    panic!("original allocation cause {expected:?}: {reason:?}")
                };
                assert_eq!(actual.kind, expected);
                assert_eq!(
                    actual.limit,
                    match expected {
                        a::Limit::RequestBytes => bounds.allocator.request_bytes,
                        a::Limit::LiveBytes => bounds.allocator.live_bytes,
                        a::Limit::CumulativeBytes => bounds.allocator.cumulative_bytes,
                        a::Limit::LiveAllocations => bounds.allocator.live_allocations as u64,
                        a::Limit::Alignment => bounds.allocator.alignment,
                    }
                );
                assert!(actual.attempted > actual.limit);
            }
            Self::Fuel => assert!(
                matches!(reason, r::Reason::Control(e::Failure::FuelExhausted)),
                "{reason:?}"
            ),
            Self::HostCalls => assert!(
                matches!(reason, r::Reason::Control(e::Failure::HostCallLimit)),
                "{reason:?}"
            ),
            Self::Frames => {
                let r::Reason::FrameLimit { limit, attempted } = reason else {
                    panic!("{reason:?}")
                };
                assert_eq!(*limit, bounds.execution.frames.unwrap());
                assert!(*attempted > *limit);
            }
            Self::Cells => {
                let r::Reason::CellLimit { limit, attempted } = reason else {
                    panic!("{reason:?}")
                };
                assert_eq!(*limit, bounds.execution.live_cells.unwrap());
                assert!(*attempted > *limit);
            }
            Self::Work => {
                let r::Reason::Control(e::Failure::ServiceWorkLimit {
                    limit,
                    used,
                    requested,
                }) = reason
                else {
                    panic!("{reason:?}")
                };
                assert_eq!(*limit, bounds.service_work);
                assert!(*used <= *limit && used.saturating_add(*requested) > *limit);
            }
            Self::Provider(_) => assert!(
                matches!(
                    reason,
                    r::Reason::Layout(lang_runtime::layout::Error::Allocation(
                        a::Error::OutOfMemory
                    ))
                ),
                "{reason:?}"
            ),
        }
    }
}
impl Bounds {
    pub(super) fn generous() -> Self {
        Self {
            allocator: a::Limits {
                request_bytes: 1 << 20,
                live_bytes: 1 << 20,
                cumulative_bytes: 1 << 26,
                live_allocations: 10_000,
                alignment: 8,
            },
            execution: e::Limits {
                fuel: Some(1_000_000),
                host_calls: Some(16),
                frames: Some(64),
                live_cells: Some(1024),
            },
            service_work: 1_000_000,
        }
    }
    pub(super) fn exact(observed: &Observed) -> Self {
        Self {
            allocator: a::Limits {
                request_bytes: observed.requests.iter().map(|r| r.bytes()).max().unwrap(),
                live_bytes: observed.allocation.peak_live_bytes,
                cumulative_bytes: observed.allocation.cumulative_bytes,
                live_allocations: observed.allocation.peak_live_allocations,
                alignment: observed
                    .requests
                    .iter()
                    .map(|r| r.alignment())
                    .max()
                    .unwrap(),
            },
            execution: e::Limits {
                fuel: Some(observed.usage.fuel),
                host_calls: Some(observed.usage.host_calls),
                frames: Some(observed.usage.peak_frames),
                live_cells: Some(observed.usage.peak_live_cells),
            },
            service_work: observed.usage.service_work,
        }
    }
}

pub(super) fn cases(count: usize) -> [Refusal; 13] {
    [
        Refusal::Allocation(a::Limit::RequestBytes),
        Refusal::Allocation(a::Limit::LiveBytes),
        Refusal::Allocation(a::Limit::CumulativeBytes),
        Refusal::Allocation(a::Limit::LiveAllocations),
        Refusal::Allocation(a::Limit::Alignment),
        Refusal::Fuel,
        Refusal::HostCalls,
        Refusal::Frames,
        Refusal::Cells,
        Refusal::Work,
        Refusal::Provider(count - 1),
        Refusal::Provider(count - 2),
        Refusal::Provider(count - 3),
    ]
}
