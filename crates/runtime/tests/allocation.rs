use lang_runtime::allocation::{
    Allocator, Error, Handle, Limit, Limits, Provider, Region, Request, Stats, Storage, System,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Debug, Eq, PartialEq)]
enum Event {
    Allocate(Request),
    Release(Request),
    ProviderDropped,
}
#[derive(Default)]
struct Trace {
    calls: usize,
    fail_on: Option<usize>,
    invalid_on: Option<usize>,
    events: Vec<Event>,
    live: BTreeMap<usize, Request>,
}
struct Tracked(Rc<RefCell<Trace>>);
impl Provider for Tracked {
    type Region = Storage;
    fn allocate(&mut self, request: Request) -> Option<Storage> {
        let mut trace = self.0.borrow_mut();
        trace.calls += 1;
        trace.events.push(Event::Allocate(request));
        if trace.fail_on == Some(trace.calls) {
            return None;
        }
        let region = if trace.invalid_on == Some(trace.calls) {
            Storage::try_new(request_for(0, 1)).unwrap()
        } else {
            Storage::try_new(request).unwrap()
        };
        assert!(
            trace
                .live
                .insert(region.bytes().as_ptr().addr(), request)
                .is_none()
        );
        Some(region)
    }
    fn deallocate(&mut self, region: Storage, request: Request) {
        let mut trace = self.0.borrow_mut();
        assert_eq!(
            trace.live.remove(&region.bytes().as_ptr().addr()),
            Some(request)
        );
        trace.events.push(Event::Release(request));
        drop(region);
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        assert!(self.0.borrow().live.is_empty());
        self.0.borrow_mut().events.push(Event::ProviderDropped);
    }
}
fn request_for(bytes: u64, alignment: u64) -> Request {
    Request::new(bytes, alignment).unwrap()
}
fn limits() -> Limits {
    Limits {
        request_bytes: 64,
        live_bytes: 64,
        cumulative_bytes: 256,
        live_allocations: 8,
        alignment: 4096,
    }
}
fn tracked(limits: Limits) -> (Allocator<Tracked>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let allocator = Allocator::new(Tracked(Rc::clone(&trace)), limits).unwrap();
    (allocator, trace)
}

#[test]
fn aligned_storage_is_contiguous_writable_and_disjoint() {
    let mut allocator = Allocator::new(
        System,
        Limits {
            live_allocations: 16,
            ..Limits::default()
        },
    )
    .unwrap();
    let mut handles = Vec::new();
    for power in 0..=12 {
        let alignment = 1 << power;
        let handle = allocator.allocate(request_for(37, alignment)).unwrap();
        let data = allocator.bytes_mut(handle).unwrap();
        assert_eq!(data.len(), 37);
        assert_eq!(data.as_ptr().addr() % alignment as usize, 0);
        data.fill(power as u8);
        handles.push(handle);
    }
    for (power, &handle) in handles.iter().enumerate() {
        assert_eq!(allocator.bytes(handle).unwrap(), &[power as u8; 37]);
        let start = allocator.bytes(handle).unwrap().as_ptr().addr();
        for &other in &handles[power + 1..] {
            let other = allocator.bytes(other).unwrap().as_ptr().addr();
            assert!(start + 37 <= other || other + 37 <= start);
        }
    }
    assert_eq!(allocator.stats().live_bytes, 13 * 37);
}

#[test]
fn zero_storage_retains_identity_accounting_and_authority_without_callbacks() {
    let (mut allocator, trace) = tracked(limits());
    let handle = allocator.allocate(request_for(0, 4096)).unwrap();
    assert_eq!(handle.allocator(), allocator.identity());
    assert!(allocator.bytes(handle).unwrap().is_empty());
    assert!(allocator.bytes_mut(handle).unwrap().is_empty());
    assert_eq!(
        allocator.stats(),
        Stats {
            live_allocations: 1,
            peak_live_allocations: 1,
            requests: 1,
            allocations: 1,
            ..Stats::default()
        }
    );
    assert!(trace.borrow().events.is_empty());
    assert!(matches!(
        allocator.revoke(),
        Err(Error::InUse { allocations: 1 })
    ));
    Allocator::preflight(Some(&allocator)).unwrap();
    allocator.release(handle).unwrap();
    assert_eq!(allocator.stats().deallocations, 1);
    assert!(trace.borrow().events.is_empty());
    let provider = allocator.revoke().unwrap();
    assert_eq!(
        Allocator::preflight(Some(&allocator)),
        Err(Error::MissingAuthority)
    );
    assert_eq!(
        Allocator::<Tracked>::preflight(None),
        Err(Error::MissingAuthority)
    );
    assert_eq!(
        allocator.allocate(request_for(1, 1)),
        Err(Error::MissingAuthority)
    );
    assert_eq!(allocator.check(&[]), Err(Error::MissingAuthority));
    assert!(matches!(allocator.revoke(), Err(Error::MissingAuthority)));
    drop(allocator);
    assert!(trace.borrow().events.is_empty());
    drop(provider);
    assert_eq!(trace.borrow().events, [Event::ProviderDropped]);
}

#[test]
fn request_arithmetic_and_alignment_are_checked_before_storage() {
    for alignment in [0, 3, 4097, 8192, u64::MAX] {
        assert_eq!(
            Request::new(0, alignment),
            Err(Error::InvalidAlignment(alignment))
        );
    }
    assert_eq!(Request::new(u64::MAX, 1), Err(Error::Overflow));
    assert_eq!(Request::new(i64::MAX as u64, 4096), Err(Error::Overflow));
    assert_eq!(Request::elements(-1, 0, 1), Err(Error::Overflow));
    assert_eq!(
        Request::elements(i64::MAX, u64::MAX, 1),
        Err(Error::Overflow)
    );
    assert_eq!(Request::elements(i64::MAX, 2, 1), Err(Error::Overflow));
    assert_eq!(
        Request::elements(0, u64::MAX, 4096).unwrap(),
        request_for(0, 4096)
    );
    assert_eq!(Request::elements(3, 7, 8).unwrap(), request_for(21, 8));
    for alignment in [0, 4097] {
        assert!(matches!(
            Allocator::new(
                System,
                Limits {
                    alignment,
                    ..limits()
                }
            ),
            Err(Error::InvalidLimits)
        ));
    }
    if usize::BITS > 32 {
        assert!(matches!(
            Allocator::new(
                System,
                Limits {
                    live_allocations: u32::MAX as usize + 1,
                    ..limits()
                }
            ),
            Err(Error::Overflow)
        ));
    }
}

#[test]
fn every_quota_is_checked_without_calling_the_provider_or_charging_attempts() {
    let cases = [
        (
            Limits {
                request_bytes: 7,
                ..limits()
            },
            request_for(8, 1),
            Limit::RequestBytes,
        ),
        (
            Limits {
                live_bytes: 7,
                ..limits()
            },
            request_for(8, 1),
            Limit::LiveBytes,
        ),
        (
            Limits {
                cumulative_bytes: 7,
                ..limits()
            },
            request_for(8, 1),
            Limit::CumulativeBytes,
        ),
        (
            Limits {
                live_allocations: 0,
                ..limits()
            },
            request_for(0, 1),
            Limit::LiveAllocations,
        ),
        (
            Limits {
                alignment: 7,
                ..limits()
            },
            request_for(1, 8),
            Limit::Alignment,
        ),
    ];
    for (limits, request, limit) in cases {
        let (mut allocator, trace) = tracked(limits);
        assert_eq!(allocator.allocate(request), Err(Error::Limit(limit)));
        assert_eq!(allocator.stats(), Stats::default());
        assert!(trace.borrow().events.is_empty());
    }
    let (mut allocator, trace) = tracked(Limits {
        request_bytes: 8,
        live_bytes: 8,
        cumulative_bytes: 16,
        live_allocations: 1,
        ..limits()
    });
    let first = allocator.allocate(request_for(8, 1)).unwrap();
    assert_eq!(
        allocator.allocate(request_for(0, 1)),
        Err(Error::Limit(Limit::LiveAllocations))
    );
    allocator.release(first).unwrap();
    let second = allocator.allocate(request_for(8, 1)).unwrap();
    allocator.release(second).unwrap();
    assert_eq!(
        allocator.allocate(request_for(1, 1)),
        Err(Error::Limit(Limit::CumulativeBytes))
    );
    assert_eq!(trace.borrow().calls, 2);
    assert_eq!(allocator.stats().cumulative_bytes, 16);
    assert_eq!(allocator.stats().peak_live_bytes, 8);
}

#[test]
fn oom_refunds_only_live_storage_and_counts_without_reusing_cumulative_budget() {
    let (mut allocator, trace) = tracked(Limits {
        cumulative_bytes: 16,
        ..limits()
    });
    trace.borrow_mut().fail_on = Some(1);
    assert_eq!(
        allocator.allocate(request_for(8, 8)),
        Err(Error::OutOfMemory)
    );
    assert_eq!(
        allocator.stats(),
        Stats {
            requests: 1,
            cumulative_bytes: 8,
            ..Stats::default()
        }
    );
    let handle = allocator.allocate(request_for(8, 8)).unwrap();
    assert_eq!(handle.generation(), 1);
    assert_eq!(allocator.stats().live_bytes, 8);
    allocator.release(handle).unwrap();
    assert_eq!(
        allocator.stats(),
        Stats {
            cumulative_bytes: 16,
            peak_live_bytes: 8,
            peak_live_allocations: 1,
            requests: 2,
            allocations: 1,
            deallocations: 1,
            ..Stats::default()
        }
    );
    assert_eq!(
        allocator.allocate(request_for(1, 1)),
        Err(Error::Limit(Limit::CumulativeBytes))
    );
    assert_eq!(
        trace.borrow().events,
        [
            Event::Allocate(request_for(8, 8)),
            Event::Allocate(request_for(8, 8)),
            Event::Release(request_for(8, 8)),
        ]
    );
}

#[test]
fn invalid_provider_storage_is_returned_once_and_not_published() {
    let (mut allocator, trace) = tracked(limits());
    trace.borrow_mut().invalid_on = Some(1);
    assert_eq!(
        allocator.allocate(request_for(8, 8)),
        Err(Error::InvalidStorage)
    );
    assert_eq!(
        allocator.stats(),
        Stats {
            requests: 1,
            cumulative_bytes: 8,
            ..Stats::default()
        }
    );
    assert_eq!(
        trace.borrow().events,
        [
            Event::Allocate(request_for(8, 8)),
            Event::Release(request_for(8, 8))
        ]
    );
    let handle = allocator.allocate(request_for(8, 8)).unwrap();
    assert_eq!(handle.generation(), 1);
    allocator.release(handle).unwrap();
}

#[test]
fn misaligned_provider_storage_is_rejected_and_returned() {
    struct Shifted(Storage);
    impl Region for Shifted {
        fn bytes(&self) -> &[u8] {
            &self.0.bytes()[1..]
        }
        fn bytes_mut(&mut self) -> &mut [u8] {
            &mut self.0.bytes_mut()[1..]
        }
    }
    struct Bad(Rc<RefCell<Vec<Request>>>);
    impl Provider for Bad {
        type Region = Shifted;
        fn allocate(&mut self, request: Request) -> Option<Shifted> {
            Some(Shifted(
                Storage::try_new(request_for(request.bytes() + 1, 4096)).unwrap(),
            ))
        }
        fn deallocate(&mut self, _: Shifted, request: Request) {
            self.0.borrow_mut().push(request);
        }
    }
    let releases = Rc::new(RefCell::new(Vec::new()));
    let mut allocator = Allocator::new(Bad(Rc::clone(&releases)), limits()).unwrap();
    let request = request_for(4, 4096);
    assert_eq!(allocator.allocate(request), Err(Error::InvalidStorage));
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().cumulative_bytes, 4);
    drop(allocator);
    assert_eq!(*releases.borrow(), [request]);
}

#[test]
fn stale_and_foreign_handles_never_reach_callbacks_or_new_owners() {
    let (mut allocator, trace) = tracked(limits());
    let (mut foreign, foreign_trace) = tracked(limits());
    let old = allocator.allocate(request_for(4, 4)).unwrap();
    assert_ne!(allocator.identity(), foreign.identity());
    assert_eq!(foreign.release(old), Err(Error::ForeignAllocator));
    assert_eq!(foreign.bytes(old), Err(Error::ForeignAllocator));
    let mut wrong_owner = old;
    assert_eq!(
        foreign.resize(&mut wrong_owner, 8),
        Err(Error::ForeignAllocator)
    );
    assert!(foreign_trace.borrow().events.is_empty());
    allocator.release(old).unwrap();
    let next = allocator.allocate(request_for(4, 4)).unwrap();
    allocator.bytes_mut(next).unwrap().fill(9);
    assert!(next.generation() > old.generation());
    let before = allocator.stats();
    assert_eq!(allocator.release(old), Err(Error::StaleHandle));
    assert_eq!(allocator.bytes_mut(old), Err(Error::StaleHandle));
    assert_eq!(allocator.request(old), Err(Error::StaleHandle));
    let mut old_again = old;
    assert_eq!(allocator.resize(&mut old_again, 8), Err(Error::StaleHandle));
    assert_eq!(allocator.stats(), before);
    assert_eq!(allocator.bytes(next).unwrap(), &[9; 4]);
    assert_eq!(trace.borrow().calls, 2);
}

#[test]
fn combined_data_and_metadata_are_preflighted_and_failed_plans_can_be_retired() {
    let plan = [request_for(16, 8), request_for(8, 8)];
    let (allocator, trace) = tracked(Limits {
        live_bytes: 23,
        ..limits()
    });
    assert_eq!(allocator.check(&plan), Err(Error::Limit(Limit::LiveBytes)));
    assert_eq!(allocator.stats(), Stats::default());
    assert!(trace.borrow().events.is_empty());
    let (mut allocator, trace) = tracked(limits());
    allocator.check(&plan).unwrap();
    assert_eq!(allocator.stats(), Stats::default());
    trace.borrow_mut().fail_on = Some(2);
    // A layout adapter owns rollback: reserve retirement metadata before data.
    let metadata = allocator.allocate(plan[0]).unwrap();
    assert_eq!(allocator.allocate(plan[1]), Err(Error::OutOfMemory));
    allocator.release(metadata).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(allocator.stats().cumulative_bytes, 24);
    assert_eq!(allocator.stats().peak_live_bytes, 16);
    assert_eq!(
        trace.borrow().events,
        [
            Event::Allocate(plan[0]),
            Event::Allocate(plan[1]),
            Event::Release(plan[0])
        ]
    );
}

#[test]
fn resize_prepares_with_original_allocator_and_preserves_owner_on_failure() {
    let (mut allocator, trace) = tracked(limits());
    let mut handle = allocator.allocate(request_for(4, 16)).unwrap();
    allocator
        .bytes_mut(handle)
        .unwrap()
        .copy_from_slice(b"abcd");
    let old = handle;
    trace.borrow_mut().fail_on = Some(2);
    assert_eq!(allocator.resize(&mut handle, 8), Err(Error::OutOfMemory));
    assert_eq!(handle, old);
    assert_eq!(allocator.bytes(handle).unwrap(), b"abcd");
    assert_eq!(allocator.stats().live_bytes, 4);
    assert_eq!(allocator.stats().peak_live_bytes, 4);
    allocator.resize(&mut handle, 8).unwrap();
    assert_ne!(handle, old);
    assert_eq!(handle.allocator(), old.allocator());
    assert_eq!(allocator.request(handle).unwrap(), request_for(8, 16));
    assert_eq!(&allocator.bytes(handle).unwrap()[..4], b"abcd");
    assert_eq!(allocator.bytes(old), Err(Error::StaleHandle));
    assert_eq!(allocator.stats().peak_live_bytes, 12);
    assert_eq!(allocator.stats().peak_live_allocations, 2);
    let before = allocator.stats();
    allocator.resize(&mut handle, 8).unwrap();
    assert_eq!(allocator.stats(), before);
    allocator.resize(&mut handle, 2).unwrap();
    assert_eq!(allocator.bytes(handle).unwrap(), b"ab");
    allocator.resize(&mut handle, 0).unwrap();
    assert!(allocator.bytes(handle).unwrap().is_empty());
    allocator.resize(&mut handle, 1).unwrap();
    allocator.release(handle).unwrap();
    assert_eq!(
        trace.borrow().events,
        [
            Event::Allocate(request_for(4, 16)),
            Event::Allocate(request_for(8, 16)),
            Event::Allocate(request_for(8, 16)),
            Event::Release(request_for(4, 16)),
            Event::Allocate(request_for(2, 16)),
            Event::Release(request_for(8, 16)),
            Event::Release(request_for(2, 16)),
            Event::Allocate(request_for(1, 16)),
            Event::Release(request_for(1, 16)),
        ]
    );
}

#[test]
fn resize_requires_budget_for_old_and_prepared_storage_before_callbacks() {
    for (limits, expected) in [
        (
            Limits {
                live_bytes: 11,
                ..limits()
            },
            Limit::LiveBytes,
        ),
        (
            Limits {
                live_allocations: 1,
                ..limits()
            },
            Limit::LiveAllocations,
        ),
    ] {
        let (mut allocator, trace) = tracked(limits);
        let mut handle = allocator.allocate(request_for(4, 8)).unwrap();
        let old = handle;
        let before = allocator.stats();
        assert_eq!(
            allocator.resize(&mut handle, 8),
            Err(Error::Limit(expected))
        );
        assert_eq!(handle, old);
        assert_eq!(allocator.stats(), before);
        assert_eq!(trace.borrow().calls, 1);
    }
}

#[test]
fn cross_allocator_clone_is_explicit_fallible_and_preserves_source() {
    let (mut source, source_trace) = tracked(limits());
    let (mut target, target_trace) = tracked(limits());
    let original = source.allocate(request_for(4, 32)).unwrap();
    source.bytes_mut(original).unwrap().copy_from_slice(b"test");
    target_trace.borrow_mut().fail_on = Some(1);
    assert_eq!(
        target.clone_from(&source, original),
        Err(Error::OutOfMemory)
    );
    assert_eq!(source.bytes(original).unwrap(), b"test");
    let copy = target.clone_from(&source, original).unwrap();
    assert_ne!(copy.allocator(), original.allocator());
    assert_eq!(
        target.request(copy).unwrap(),
        source.request(original).unwrap()
    );
    assert_eq!(target.bytes(copy).unwrap(), b"test");
    target.bytes_mut(copy).unwrap()[0] = b'b';
    assert_eq!(source.bytes(original).unwrap(), b"test");
    source.release(original).unwrap();
    target.release(copy).unwrap();
    assert_eq!(
        source_trace.borrow().events,
        [
            Event::Allocate(request_for(4, 32)),
            Event::Release(request_for(4, 32))
        ]
    );
    assert_eq!(
        target_trace.borrow().events,
        [
            Event::Allocate(request_for(4, 32)),
            Event::Allocate(request_for(4, 32)),
            Event::Release(request_for(4, 32))
        ]
    );
}

#[test]
fn teardown_returns_each_outstanding_region_before_destroying_its_provider() {
    let (mut allocator, trace) = tracked(limits());
    let first = allocator.allocate(request_for(4, 8)).unwrap();
    allocator.allocate(request_for(0, 1)).unwrap();
    allocator.allocate(request_for(7, 16)).unwrap();
    allocator.release(first).unwrap();
    allocator.allocate(request_for(3, 2)).unwrap();
    assert!(matches!(
        allocator.revoke(),
        Err(Error::InUse { allocations: 3 })
    ));
    drop(allocator);
    assert_eq!(
        trace.borrow().events,
        [
            Event::Allocate(request_for(4, 8)),
            Event::Allocate(request_for(7, 16)),
            Event::Release(request_for(4, 8)),
            Event::Allocate(request_for(3, 2)),
            Event::Release(request_for(3, 2)),
            Event::Release(request_for(7, 16)),
            Event::ProviderDropped,
        ]
    );
}

#[test]
fn generated_allocate_release_sequences_match_an_independent_ledger() {
    for seed in 1..=100_u64 {
        let mut random = seed;
        let (mut allocator, trace) = tracked(limits());
        let mut owners: Vec<(Handle, u64, u8)> = Vec::new();
        let mut expected = Stats::default();
        let mut next_generation = 1;
        for step in 0..256_u64 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            if random % 3 == 0 && !owners.is_empty() {
                let index = random as usize % owners.len();
                let (handle, bytes, _) = owners.swap_remove(index);
                allocator.release(handle).unwrap();
                expected.live_bytes -= bytes;
                expected.live_allocations -= 1;
                expected.deallocations += 1;
                assert_eq!(allocator.release(handle), Err(Error::StaleHandle));
            } else {
                let bytes = random % 18;
                let tag = step as u8;
                let fail = step % 7 == 0 && bytes != 0;
                let denied = if owners.len() == 8 {
                    Some(Limit::LiveAllocations)
                } else if owners.iter().map(|(_, bytes, _)| bytes).sum::<u64>() + bytes > 64 {
                    Some(Limit::LiveBytes)
                } else if expected.cumulative_bytes + bytes > 256 {
                    Some(Limit::CumulativeBytes)
                } else {
                    None
                };
                let call = trace.borrow().calls + 1;
                trace.borrow_mut().fail_on = fail.then_some(call);
                let result = allocator.allocate(request_for(bytes, 8));
                if let Some(limit) = denied {
                    assert_eq!(result, Err(Error::Limit(limit)));
                } else {
                    expected.requests += 1;
                    expected.cumulative_bytes += bytes;
                    if fail {
                        assert_eq!(result, Err(Error::OutOfMemory));
                    } else {
                        let handle = result.unwrap();
                        assert_eq!(handle.generation(), next_generation);
                        next_generation += 1;
                        allocator.bytes_mut(handle).unwrap().fill(tag);
                        owners.push((handle, bytes, tag));
                        expected.allocations += 1;
                        expected.live_allocations += 1;
                        expected.live_bytes += bytes;
                        expected.peak_live_bytes =
                            expected.peak_live_bytes.max(expected.live_bytes);
                        expected.peak_live_allocations =
                            expected.peak_live_allocations.max(owners.len());
                    }
                }
            }
            assert_eq!(allocator.stats(), expected, "seed {seed}, step {step}");
            for &(handle, bytes, tag) in &owners {
                assert_eq!(allocator.bytes(handle).unwrap(), vec![tag; bytes as usize]);
            }
        }
        for (handle, _, _) in owners {
            allocator.release(handle).unwrap();
        }
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
    }
}
