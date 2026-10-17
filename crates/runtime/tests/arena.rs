use lang_mir::unit::{SourceId, Span};
use lang_runtime::arena::{
    ArenaHeap, Edge, ErrorKind, LimitKind, Limits, Payload, Reference, Target,
};
use lang_runtime::shared;
use std::sync::{Arc, Barrier};

fn heap() -> ArenaHeap {
    ArenaHeap::new(Limits::default()).unwrap()
}
fn payload(bytes: &[u8], references: &[Reference]) -> Payload {
    Payload {
        bytes: bytes.to_vec(),
        edges: references.iter().map(|r| Edge::arena(*r, None)).collect(),
    }
}
fn span(start: u32) -> Span {
    Span {
        source: SourceId(1),
        start,
        end: start + 1,
    }
}

#[test]
fn self_and_mutual_cycles_use_nonowning_copy_tokens() {
    let heap = heap();
    let mut arena = heap.open(None).unwrap();
    let a = arena.allocate(payload(b"a", &[]), None).unwrap();
    let b = arena.allocate(payload(b"b", &[a]), None).unwrap();
    drop(arena.replace(a, payload(b"new", &[a, b]), None).unwrap());
    let before = heap.stats().unwrap();
    let copies = vec![a; 1000];
    assert_eq!(heap.stats().unwrap(), before);
    assert_eq!(
        arena.read(copies[0], None).unwrap().bytes().unwrap(),
        b"new"
    );
    let read = arena.read(a, None).unwrap();
    assert_eq!(read.edge(0).unwrap(), a);
    assert_eq!(read.edge(1).unwrap(), b);
    assert_eq!(read.edge(2).unwrap_err().kind, ErrorKind::WrongEdgeKind);
    drop(read);
    let heap_clone = heap.clone();
    drop(arena);
    drop(copies);
    assert_eq!(heap_clone.stats().unwrap().live_nodes, 0);
    assert_eq!(heap_clone.stats().unwrap().resident_payloads, 0);
    assert_eq!(heap_clone.stats().unwrap().retired_nodes, 2);
}

#[test]
fn cross_arena_runtime_and_forged_identities_fail_before_publication() {
    let heap = heap();
    let other_heap = self::heap();
    let mut a = heap.open(Some(span(1))).unwrap();
    let mut b = heap.open(None).unwrap();
    let mut foreign = other_heap.open(None).unwrap();
    let ra = a.allocate(payload(b"a", &[]), None).unwrap();
    let rb = b.allocate(payload(b"b", &[]), Some(span(10))).unwrap();
    let rf = foreign
        .allocate(payload(b"foreign", &[]), Some(span(11)))
        .unwrap();
    for reference in [rb, rf] {
        let before = heap.stats().unwrap();
        let incoming = Payload {
            bytes: vec![7],
            edges: vec![Edge::arena(reference, Some(span(20)))],
        };
        let failure = a.replace(ra, incoming, Some(span(30))).unwrap_err();
        assert_eq!(failure.error.kind, ErrorKind::CrossRegion);
        assert_eq!(failure.error.identity(), "ownership.cross-region-edge");
        assert_eq!(failure.error.primary, Some(span(30)));
        assert_eq!(failure.error.secondary, Some(span(20)));
        assert_eq!(failure.payload.bytes, [7]);
        assert_eq!(failure.payload.edges.len(), 1);
        assert_eq!(heap.stats().unwrap(), before);
        assert_eq!(a.read(ra, None).unwrap().bytes().unwrap(), b"a");
    }
    let mut forged = ra.identity();
    forged.node = usize::MAX;
    assert_eq!(
        a.resolve(forged, None).unwrap_err().kind,
        ErrorKind::Unavailable
    );
    forged = ra.identity();
    forged.generation = u64::MAX;
    assert_eq!(
        a.resolve(forged, None).unwrap_err().kind,
        ErrorKind::Unavailable
    );
    assert_eq!(
        a.resolve(rf.identity(), None).unwrap_err().kind,
        ErrorKind::Unavailable
    );
    assert_eq!(
        a.resolve(rb.identity(), None).unwrap_err().kind,
        ErrorKind::CrossRegion
    );
    assert_eq!(a.resolve(ra.identity(), None).unwrap(), ra);
}

#[test]
fn replacing_payloads_preserves_identity_and_returns_old_storage_after_commit() {
    let heap = heap();
    let mut arena = heap.open(None).unwrap();
    let reference = arena.allocate(payload(b"old", &[]), Some(span(1))).unwrap();
    let old = arena
        .replace(reference, payload(b"new", &[reference]), Some(span(2)))
        .unwrap();
    assert_eq!(old.identity(), reference.identity());
    assert_eq!(old.bytes(), b"old");
    assert_eq!(
        arena.read(reference, None).unwrap().bytes().unwrap(),
        b"new"
    );
    assert_eq!(heap.stats().unwrap().resident_payloads, 2);
    let old = old.try_into_payload().unwrap();
    assert_eq!(old.bytes, b"old");
    assert_eq!(heap.stats().unwrap().resident_payloads, 1);
    assert_eq!(heap.stats().unwrap().replaced_payloads, 1);
}

#[test]
fn any_read_lease_prevents_allocation_replacement_and_normal_close() {
    let heap = heap();
    let mut arena = heap.open(None).unwrap();
    let a = arena.allocate(payload(b"a", &[]), None).unwrap();
    let b = arena.allocate(payload(b"b", &[]), None).unwrap();
    let read = arena.read(a, None).unwrap();
    let before = heap.stats().unwrap();
    assert_eq!(
        arena
            .allocate(payload(b"incoming", &[]), None)
            .unwrap_err()
            .error
            .kind,
        ErrorKind::BorrowConflict
    );
    assert_eq!(
        arena
            .replace(b, payload(b"incoming", &[]), None)
            .unwrap_err()
            .error
            .kind,
        ErrorKind::BorrowConflict
    );
    assert_eq!(
        arena.close(None).unwrap_err().kind,
        ErrorKind::BorrowConflict
    );
    assert_eq!(heap.stats().unwrap(), before);
    let second = arena.read(b, None).unwrap();
    assert_eq!(second.bytes().unwrap(), b"b");
    drop(second);
    drop(read);
    drop(arena.replace(b, payload(b"changed", &[]), None).unwrap());
    drop(arena.close(None).unwrap());
    assert_eq!(heap.stats().unwrap().resident_payloads, 0);
}

#[test]
fn close_revokes_before_reverse_drain_and_slot_reuse() {
    let heap = heap();
    let mut arena = heap.open(None).unwrap();
    let refs: Vec<_> = (0..4)
        .map(|i| arena.allocate(payload(&[i], &[]), None).unwrap())
        .collect();
    let mut drain = arena.close(None).unwrap();
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert_eq!(drain.len(), 4);
    assert_eq!(
        arena.read(refs[0], None).unwrap_err().identity(),
        "ownership.unavailable-arena-reference"
    );
    let mut next = heap.open(None).unwrap();
    let fresh = next.allocate(payload(b"fresh", &[]), None).unwrap();
    assert_eq!(fresh.identity().arena_slot, refs[0].identity().arena_slot);
    assert_ne!(fresh.identity().generation, refs[0].identity().generation);
    assert_eq!(
        next.read(refs[0], None).unwrap_err().kind,
        ErrorKind::Unavailable
    );
    let mut order = Vec::new();
    for retired in drain.by_ref() {
        order.push((retired.identity().node, retired.bytes()[0]));
    }
    assert_eq!(order, [(3, 3), (2, 2), (1, 1), (0, 0)]);
    assert!(drain.next().is_none());
    assert_eq!(next.read(fresh, None).unwrap().bytes().unwrap(), b"fresh");
    assert_eq!(heap.stats().unwrap().resident_payloads, 1);
}

#[test]
fn dropping_an_owner_revokes_retained_reads_without_touching_reused_read_counts() {
    let heap = heap();
    let mut owner = heap.open(None).unwrap();
    let old = owner.allocate(payload(b"old", &[]), None).unwrap();
    let read = owner.read(old, None).unwrap();
    drop(owner);
    assert_eq!(read.bytes().unwrap_err().kind, ErrorKind::Unavailable);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert_eq!(heap.stats().unwrap().resident_bytes, 3);
    let mut next = heap.open(None).unwrap();
    let fresh = next.allocate(payload(b"new", &[]), None).unwrap();
    let new_read = next.read(fresh, None).unwrap();
    drop(read);
    assert_eq!(
        next.allocate(Payload::default(), None)
            .unwrap_err()
            .error
            .kind,
        ErrorKind::BorrowConflict
    );
    assert_eq!(new_read.bytes().unwrap(), b"new");
    drop(new_read);
    next.allocate(Payload::default(), None).unwrap();
    drop(next);
    assert_eq!(heap.stats().unwrap().resident_payloads, 0);
}

#[test]
fn abandoning_a_partial_drain_releases_every_remaining_payload() {
    let heap = heap();
    let mut arena = heap.open(None).unwrap();
    for i in 0..10 {
        arena.allocate(payload(&[i], &[]), None).unwrap();
    }
    let mut drain = arena.close(None).unwrap();
    let last = drain.next().unwrap();
    assert_eq!(last.identity().node, 9);
    assert_eq!(heap.stats().unwrap().resident_payloads, 10);
    drop(drain);
    assert_eq!(heap.stats().unwrap().resident_payloads, 1);
    drop(last);
    assert_eq!(heap.stats().unwrap().resident_payloads, 0);
    assert_eq!(heap.stats().unwrap().retired_nodes, 10);
}

#[test]
fn teardown_is_reentrant_and_panic_still_releases_remaining_storage() {
    let heap = heap();
    let mut arena = heap.open(None).unwrap();
    let references: Vec<_> = (0..8)
        .map(|i| arena.allocate(payload(&[i], &[]), None).unwrap())
        .collect();
    let drain = arena.close(None).unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for retired in drain {
            let mut other = heap.open(None).unwrap();
            other.allocate(Payload::default(), None).unwrap();
            assert_eq!(
                other.read(references[0], None).unwrap_err().kind,
                ErrorKind::Unavailable
            );
            if retired.identity().node == 5 {
                panic!("host cleanup failure");
            }
        }
    }));
    assert!(outcome.is_err());
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert_eq!(heap.stats().unwrap().resident_payloads, 0);
    assert_eq!(heap.stats().unwrap().retired_nodes, 11);
}

#[test]
fn retained_snapshots_count_against_peak_payload_and_byte_limits() {
    let heap = ArenaHeap::new(Limits {
        payloads: 2,
        payload_bytes: 3,
        ..Limits::default()
    })
    .unwrap();
    let mut arena = heap.open(None).unwrap();
    let reference = arena.allocate(payload(b"ab", &[]), None).unwrap();
    let before = heap.stats().unwrap();
    let failed = arena
        .replace(reference, payload(b"cd", &[]), None)
        .unwrap_err();
    assert_eq!(failed.error.kind, ErrorKind::Limit(LimitKind::PayloadBytes));
    assert_eq!(failed.payload.bytes, b"cd");
    assert_eq!(heap.stats().unwrap(), before);
    let old = arena.replace(reference, payload(b"x", &[]), None).unwrap();
    assert_eq!(heap.stats().unwrap().resident_bytes, 3);
    assert_eq!(
        arena
            .allocate(Payload::default(), None)
            .unwrap_err()
            .error
            .kind,
        ErrorKind::Limit(LimitKind::Payloads)
    );
    drop(old);
    arena.allocate(payload(b"y", &[]), None).unwrap();
    assert_eq!(heap.stats().unwrap().resident_bytes, 2);
}

#[test]
fn shared_and_weak_edges_are_rejected_without_consuming_their_owners() {
    let heap = heap();
    let mut arena = heap.open(None).unwrap();
    let shared = shared::SharedHeap::default();
    let strong = shared.allocate(shared::Payload::default(), None).unwrap();
    for weak in [false, true] {
        let target = if weak {
            Target::Weak(strong.downgrade().unwrap())
        } else {
            Target::Shared(strong.try_clone().unwrap())
        };
        let before = strong.strong_count().unwrap();
        let failed = arena
            .allocate(
                Payload {
                    bytes: vec![1],
                    edges: vec![Edge {
                        target,
                        span: Some(span(4)),
                    }],
                },
                Some(span(5)),
            )
            .unwrap_err();
        assert_eq!(failed.error.kind, ErrorKind::CrossRegion);
        assert_eq!(failed.error.secondary, Some(span(4)));
        assert_eq!(failed.payload.edges.len(), 1);
        assert_eq!(strong.strong_count().unwrap(), before);
        drop(failed);
        assert_eq!(strong.strong_count().unwrap(), 1);
    }
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn structural_work_and_registration_limits_fail_atomically() {
    for (limits, expected) in [
        (
            Limits {
                arenas: 0,
                ..Limits::default()
            },
            LimitKind::Arenas,
        ),
        (
            Limits {
                arena_slots: 0,
                ..Limits::default()
            },
            LimitKind::ArenaSlots,
        ),
    ] {
        assert_eq!(
            ArenaHeap::new(limits).unwrap().open(None).unwrap_err().kind,
            ErrorKind::Limit(expected)
        );
    }
    for (limits, expected) in [
        (
            Limits {
                nodes: 0,
                ..Limits::default()
            },
            LimitKind::Nodes,
        ),
        (
            Limits {
                payloads: 0,
                ..Limits::default()
            },
            LimitKind::Payloads,
        ),
        (
            Limits {
                payload_bytes: 0,
                ..Limits::default()
            },
            LimitKind::PayloadBytes,
        ),
        (
            Limits {
                publication_work: 0,
                ..Limits::default()
            },
            LimitKind::PublicationWork,
        ),
    ] {
        let heap = ArenaHeap::new(limits).unwrap();
        let mut arena = heap.open(None).unwrap();
        let before = heap.stats().unwrap();
        let failed = arena
            .allocate(payload(b"x", &[]), Some(span(7)))
            .unwrap_err();
        assert_eq!(failed.error.kind, ErrorKind::Limit(expected));
        assert_eq!(failed.error.primary, Some(span(7)));
        assert_eq!(failed.payload.bytes, b"x");
        assert_eq!(heap.stats().unwrap(), before);
    }
    for (limits, expected) in [
        (
            Limits {
                edges: 0,
                ..Limits::default()
            },
            LimitKind::Edges,
        ),
        (
            Limits {
                publication_work: 1,
                ..Limits::default()
            },
            LimitKind::PublicationWork,
        ),
    ] {
        let heap = ArenaHeap::new(limits).unwrap();
        let mut arena = heap.open(None).unwrap();
        let reference = arena.allocate(Payload::default(), None).unwrap();
        let before = heap.stats().unwrap();
        let failed = arena
            .replace(reference, payload(b"x", &[reference]), None)
            .unwrap_err();
        assert_eq!(failed.error.kind, ErrorKind::Limit(expected));
        assert_eq!(failed.payload.edges.len(), 1);
        assert_eq!(heap.stats().unwrap(), before);
    }
    let heap = ArenaHeap::new(Limits {
        readers: 0,
        ..Limits::default()
    })
    .unwrap();
    let mut arena = heap.open(None).unwrap();
    let reference = arena.allocate(Payload::default(), None).unwrap();
    let before = heap.stats().unwrap();
    assert_eq!(
        arena.read(reference, None).unwrap_err().kind,
        ErrorKind::Limit(LimitKind::Readers)
    );
    assert_eq!(heap.stats().unwrap(), before);
    drop(arena.close(None).unwrap());
}

#[test]
fn generated_graphs_match_an_independent_payload_and_retirement_oracle() {
    let heap = heap();
    let mut random = 91u64;
    for case in 0..1000 {
        let count = 1 + case % 7;
        let mut arena = heap.open(None).unwrap();
        let mut references = Vec::new();
        let mut bytes = Vec::new();
        let mut graph: Vec<Vec<usize>> = vec![vec![]; count];
        for i in 0..count {
            references.push(arena.allocate(payload(&[i as u8], &[]), None).unwrap());
            bytes.push(i as u8);
        }
        for _ in 0..16 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let from = random as usize % count;
            let targets: Vec<_> = (0..count).filter(|i| random & (1 << i) != 0).collect();
            let incoming: Vec<_> = targets.iter().map(|i| references[*i]).collect();
            let value = (random >> 32) as u8;
            drop(
                arena
                    .replace(references[from], payload(&[value], &incoming), None)
                    .unwrap(),
            );
            graph[from] = targets;
            bytes[from] = value;
            for i in 0..count {
                let read = arena.read(references[i], None).unwrap();
                assert_eq!(read.bytes().unwrap(), [bytes[i]]);
                assert_eq!(read.edge_count().unwrap(), graph[i].len());
                for (j, target) in graph[i].iter().enumerate() {
                    assert_eq!(read.edge(j).unwrap(), references[*target]);
                }
            }
        }
        let retired: Vec<_> = arena
            .close(None)
            .unwrap()
            .map(|r| (r.identity().node, r.bytes()[0]))
            .collect();
        assert_eq!(
            retired,
            (0..count).rev().map(|i| (i, bytes[i])).collect::<Vec<_>>()
        );
        assert_eq!(heap.stats().unwrap().resident_payloads, 0);
    }
}

#[test]
fn closing_and_read_lease_release_linearize_without_double_retirement() {
    for _ in 0..128 {
        let heap = heap();
        let mut arena = heap.open(None).unwrap();
        let reference = arena.allocate(payload(b"value", &[]), None).unwrap();
        let read = arena.read(reference, None).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let other = barrier.clone();
        let worker = std::thread::spawn(move || {
            other.wait();
            drop(read);
        });
        barrier.wait();
        let first = arena.close(None);
        worker.join().unwrap();
        match first {
            Ok(drain) => drop(drain),
            Err(error) => {
                assert_eq!(error.kind, ErrorKind::BorrowConflict);
                drop(arena.close(None).unwrap());
            }
        }
        assert_eq!(heap.stats().unwrap().retired_nodes, 1);
        assert_eq!(heap.stats().unwrap().resident_payloads, 0);
    }
}

#[test]
fn nested_host_arenas_have_independent_lifetimes_and_storage() {
    let heap = heap();
    let mut outer = heap.open(None).unwrap();
    let a = outer.allocate(payload(b"outer", &[]), None).unwrap();
    let b;
    {
        let mut inner = heap.open(None).unwrap();
        b = inner.allocate(payload(b"inner", &[]), None).unwrap();
    }
    assert_eq!(outer.read(a, None).unwrap().bytes().unwrap(), b"outer");
    assert_eq!(
        outer.read(b, None).unwrap_err().kind,
        ErrorKind::Unavailable
    );
    assert_eq!(heap.stats().unwrap().live_arenas, 1);
    assert_eq!(heap.stats().unwrap().resident_payloads, 1);
}

#[test]
fn a_reference_copy_does_not_extend_arena_ownership() {
    let heap = heap();
    let copied;
    {
        let mut arena = heap.open(None).unwrap();
        copied = arena.allocate(payload(b"one", &[]), None).unwrap();
    }
    let mut next = heap.open(None).unwrap();
    next.allocate(payload(b"two", &[]), None).unwrap();
    assert_eq!(
        next.read(copied, None).unwrap_err().kind,
        ErrorKind::Unavailable
    );
    assert_eq!(heap.stats().unwrap().live_nodes, 1);
}

#[test]
fn forgotten_host_reads_retain_a_bounded_quarantined_snapshot() {
    let heap = ArenaHeap::new(Limits {
        payloads: 1,
        ..Limits::default()
    })
    .unwrap();
    let mut arena = heap.open(None).unwrap();
    let reference = arena.allocate(payload(b"x", &[]), None).unwrap();
    std::mem::forget(arena.read(reference, None).unwrap());
    assert_eq!(
        arena.close(None).unwrap_err().kind,
        ErrorKind::BorrowConflict
    );
    drop(arena);
    let mut next = heap.open(None).unwrap();
    assert_eq!(
        next.allocate(Payload::default(), None)
            .unwrap_err()
            .error
            .kind,
        ErrorKind::Limit(LimitKind::Payloads)
    );
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert_eq!(heap.stats().unwrap().resident_payloads, 1);
}

#[test]
fn large_cycles_retire_iteratively_without_recursive_node_drops() {
    let count = 20_001;
    let heap = ArenaHeap::new(Limits {
        nodes: count,
        payloads: count + 1,
        ..Limits::default()
    })
    .unwrap();
    let mut arena = heap.open(None).unwrap();
    let references: Vec<_> = (0..count)
        .map(|_| arena.allocate(Payload::default(), None).unwrap())
        .collect();
    for i in 0..count {
        drop(
            arena
                .replace(
                    references[i],
                    payload(b"", &[references[(i + 1) % count]]),
                    None,
                )
                .unwrap(),
        );
    }
    let order: Vec<_> = arena
        .close(None)
        .unwrap()
        .map(|r| r.identity().node)
        .collect();
    assert_eq!(order, (0..count).rev().collect::<Vec<_>>());
    assert_eq!(heap.stats().unwrap().retired_nodes, count as u64);
    assert_eq!(heap.stats().unwrap().resident_payloads, 0);
}
