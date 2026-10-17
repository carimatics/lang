use lang_mir::unit::{SourceId, Span};
use lang_runtime::shared::{Edge, ErrorKind, LimitKind, Limits, Payload, SharedHeap, Strong};
use std::sync::{Arc, Barrier};

fn payload(bytes: &[u8], edges: Vec<Edge>) -> Payload {
    Payload {
        bytes: bytes.to_vec(),
        edges,
    }
}
fn node(heap: &SharedHeap, bytes: &[u8]) -> Strong {
    heap.allocate(payload(bytes, vec![]), None).unwrap()
}
fn span(start: u32) -> Span {
    Span {
        source: SourceId(1),
        start,
        end: start + 1,
    }
}

#[test]
fn retained_upgrade_keeps_payload_alive_after_the_last_original_owner_drops() {
    let heap = SharedHeap::default();
    let owner = node(&heap, b"alive");
    let weak = owner.downgrade().unwrap();
    let retained = weak.upgrade().unwrap();
    assert_eq!(owner.strong_count().unwrap(), 2);
    drop(owner);
    assert_eq!(retained.read().unwrap().bytes(), b"alive");
    assert_eq!(heap.stats().unwrap().live_nodes, 1);
    drop(retained);
    let error = weak.upgrade().unwrap_err();
    assert_eq!(error.kind, ErrorKind::ExpiredWeak);
    assert_eq!(error.identity(), "ownership.expired-weak");
    assert_eq!(heap.stats().unwrap().reclaimed_nodes, 1);
    assert!(weak.try_clone().unwrap().upgrade().is_err());
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn moving_edge_owners_retains_targets_and_weak_back_edges_do_not_keep_them_alive() {
    let heap = SharedHeap::default();
    let child = node(&heap, b"child");
    let weak_child = child.downgrade().unwrap();
    let parent = heap
        .allocate(payload(b"parent", vec![Edge::Strong(child)]), None)
        .unwrap();
    let read = parent.read().unwrap();
    assert_eq!(read.edge_count().unwrap(), 1);
    let child = read.clone_strong_edge(0).unwrap();
    drop(read);
    child
        .replace(
            payload(b"child", vec![Edge::Weak(parent.downgrade().unwrap())]),
            None,
        )
        .unwrap();
    let read = child.read().unwrap();
    let weak_parent = read.clone_weak_edge(0).unwrap();
    assert_eq!(
        read.clone_strong_edge(0).unwrap_err().kind,
        ErrorKind::WrongEdgeKind
    );
    drop(read);
    drop(child);
    assert_eq!(heap.stats().unwrap().live_nodes, 2);
    drop(parent);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert!(weak_parent.upgrade().is_err());
    assert!(weak_child.upgrade().is_err());
    assert_eq!(heap.stats().unwrap().reclaimed_nodes, 2);
}

#[test]
fn cycle_rejection_preserves_payload_edges_and_counts_with_related_spans() {
    let heap = SharedHeap::default();
    let left = node(&heap, b"left");
    let right = node(&heap, b"right");
    left.replace(
        payload(b"left", vec![Edge::Strong(right.try_clone().unwrap())]),
        Some(span(10)),
    )
    .unwrap();
    let incoming = payload(
        b"replacement",
        vec![Edge::Strong(left.try_clone().unwrap())],
    );
    let before = heap.stats().unwrap();
    let counts = (left.strong_count().unwrap(), right.strong_count().unwrap());
    let failure = right.replace(incoming, Some(span(20))).unwrap_err();
    assert_eq!(failure.error.kind, ErrorKind::StrongCycle);
    assert_eq!(failure.error.identity(), "ownership.strong-cycle");
    assert_eq!(failure.error.primary, Some(span(20)));
    assert_eq!(failure.error.secondary, Some(span(10)));
    assert_eq!(failure.payload.bytes, b"replacement");
    assert_eq!(heap.stats().unwrap(), before);
    assert_eq!(
        (left.strong_count().unwrap(), right.strong_count().unwrap()),
        counts
    );
    assert_eq!(right.read().unwrap().bytes(), b"right");
    assert_eq!(right.read().unwrap().edge_count().unwrap(), 0);
    drop(failure); // Explicit cleanup of the incoming owner follows rejection.
    assert_eq!(left.strong_count().unwrap(), counts.0 - 1);
    drop(left);
    drop(right);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn self_cycles_are_rejected_and_replacing_an_edge_releases_the_old_target() {
    let heap = SharedHeap::default();
    let parent = node(&heap, b"parent");
    let failure = parent
        .replace(
            payload(b"bad", vec![Edge::Strong(parent.try_clone().unwrap())]),
            None,
        )
        .unwrap_err();
    assert_eq!(failure.error.kind, ErrorKind::StrongCycle);
    drop(failure);
    let old = node(&heap, b"old");
    let old_weak = old.downgrade().unwrap();
    parent
        .replace(payload(b"old edge", vec![Edge::Strong(old)]), None)
        .unwrap();
    let new = node(&heap, b"new");
    parent
        .replace(payload(b"new edge", vec![Edge::Strong(new)]), None)
        .unwrap();
    assert_eq!(old_weak.upgrade().unwrap_err().kind, ErrorKind::ExpiredWeak);
    assert_eq!(
        parent
            .read()
            .unwrap()
            .clone_strong_edge(0)
            .unwrap()
            .read()
            .unwrap()
            .bytes(),
        b"new"
    );
    drop(parent);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert_eq!(heap.stats().unwrap().reclaimed_nodes, 3);
}

#[test]
fn alias_reads_block_mutation_and_multiple_reads_can_coexist_without_holding_the_lock() {
    let heap = SharedHeap::default();
    let owner = node(&heap, b"original");
    let alias = owner.try_clone().unwrap();
    let first = owner.read().unwrap();
    let second = alias.read().unwrap();
    assert_eq!(
        alias
            .replace(payload(b"rejected", vec![]), None)
            .unwrap_err()
            .error
            .kind,
        ErrorKind::BorrowConflict
    );
    assert_eq!(first.bytes(), b"original");
    drop(first);
    assert!(
        owner
            .replace(payload(b"still rejected", vec![]), None)
            .is_err()
    );
    drop(second);
    alias.replace(payload(b"new", vec![]), None).unwrap();
    assert_eq!(owner.read().unwrap().bytes(), b"new");
}

#[test]
fn foreign_heap_edges_are_rejected_before_mutation_or_consumption() {
    let first = SharedHeap::default();
    let second = SharedHeap::default();
    let owner = node(&first, b"local");
    let foreign = node(&second, b"foreign");
    let weak = foreign.downgrade().unwrap();
    for edge in [Edge::Strong(foreign), Edge::Weak(weak)] {
        let before = first.stats().unwrap();
        let failure = owner
            .replace(payload(b"bad", vec![edge]), None)
            .unwrap_err();
        assert_eq!(failure.error.kind, ErrorKind::ForeignHeap);
        assert_eq!(first.stats().unwrap(), before);
        drop(failure);
    }
    assert_eq!(owner.read().unwrap().bytes(), b"local");
    assert_eq!(second.stats().unwrap().live_nodes, 0);
}

#[test]
fn reference_count_and_resource_boundaries_fail_without_publication() {
    let heap = SharedHeap::new(Limits {
        reference_count: 1,
        ..Limits::default()
    });
    let owner = node(&heap, b"one");
    assert_eq!(
        owner.try_clone().unwrap_err().kind,
        ErrorKind::CountOverflow
    );
    let weak = owner.downgrade().unwrap();
    assert_eq!(weak.try_clone().unwrap_err().kind, ErrorKind::CountOverflow);
    assert_eq!(weak.upgrade().unwrap_err().kind, ErrorKind::CountOverflow);
    assert_eq!(owner.strong_count().unwrap(), 1);
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
                slots: 0,
                ..Limits::default()
            },
            LimitKind::Slots,
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
        let heap = SharedHeap::new(limits);
        let before = heap.stats().unwrap();
        let failure = heap.allocate(payload(b"one", vec![]), None).unwrap_err();
        assert_eq!(failure.error.kind, ErrorKind::Limit(expected));
        assert_eq!(failure.payload.bytes, b"one");
        assert_eq!(heap.stats().unwrap(), before);
    }
    let heap = SharedHeap::new(Limits {
        edges: 0,
        ..Limits::default()
    });
    let a = node(&heap, b"a");
    let b = node(&heap, b"b");
    let failure = a
        .replace(payload(b"bad", vec![Edge::Strong(b)]), None)
        .unwrap_err();
    assert_eq!(failure.error.kind, ErrorKind::Limit(LimitKind::Edges));
    assert_eq!(a.read().unwrap().bytes(), b"a");
}

#[test]
fn rejected_work_during_reachability_preserves_the_existing_graph() {
    let heap = SharedHeap::new(Limits {
        publication_work: 5,
        ..Limits::default()
    });
    let a = node(&heap, b"a");
    let b = node(&heap, b"b");
    let c = node(&heap, b"c");
    let incoming = payload(b"changed", vec![Edge::Strong(b.try_clone().unwrap())]);
    let before = heap.stats().unwrap();
    let failure = a.replace(incoming, None).unwrap_err();
    assert_eq!(
        failure.error.kind,
        ErrorKind::Limit(LimitKind::PublicationWork)
    );
    assert_eq!(heap.stats().unwrap(), before);
    assert_eq!(a.read().unwrap().bytes(), b"a");
    drop((failure, a, b, c));
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn deep_graph_retirement_is_iterative_and_reuses_metadata_slots() {
    let heap = SharedHeap::new(Limits {
        nodes: 16_000,
        slots: 16_000,
        edges: 16_000,
        ..Limits::default()
    });
    let mut root = node(&heap, b"leaf");
    for _ in 0..15_000 {
        root = heap
            .allocate(payload(b"node", vec![Edge::Strong(root)]), None)
            .unwrap();
    }
    let slots = heap.stats().unwrap().slots;
    drop(root);
    let stats = heap.stats().unwrap();
    assert_eq!(stats.live_nodes, 0);
    assert_eq!(stats.stored_edges, 0);
    assert_eq!(stats.payload_bytes, 0);
    assert_eq!(stats.reclaimed_nodes, 15_001);
    let replacement = node(&heap, b"replacement");
    assert_eq!(heap.stats().unwrap().slots, slots);
    assert_eq!(replacement.read().unwrap().bytes(), b"replacement");
}

#[test]
fn simultaneous_opposite_publications_cannot_commit_a_cycle() {
    for _ in 0..64 {
        let heap = SharedHeap::default();
        let a = node(&heap, b"a");
        let b = node(&heap, b"b");
        let barrier = Arc::new(Barrier::new(2));
        let mut threads = Vec::new();
        for (from, to) in [(&a, &b), (&b, &a)] {
            let from = from.try_clone().unwrap();
            let to = to.try_clone().unwrap();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                from.replace(payload(b"replacement", vec![Edge::Strong(to)]), None)
                    .map_err(|failure| failure.error.kind)
            }));
        }
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|r| **r == Err(ErrorKind::StrongCycle))
                .count(),
            1
        );
        drop((a, b));
        assert_eq!(heap.stats().unwrap().live_nodes, 0);
        assert_eq!(heap.stats().unwrap().reclaimed_nodes, 2);
    }
}

#[test]
fn weak_upgrade_racing_last_release_never_returns_an_unretained_value() {
    for _ in 0..128 {
        let heap = SharedHeap::default();
        let owner = node(&heap, b"value");
        let weak = owner.downgrade().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let other = barrier.clone();
        let task = std::thread::spawn(move || {
            other.wait();
            weak.upgrade()
        });
        barrier.wait();
        drop(owner);
        match task.join().unwrap() {
            Ok(retained) => {
                assert_eq!(retained.read().unwrap().bytes(), b"value");
                assert_eq!(retained.strong_count().unwrap(), 1);
                drop(retained);
            }
            Err(error) => assert_eq!(error.kind, ErrorKind::ExpiredWeak),
        }
        assert_eq!(heap.stats().unwrap().live_nodes, 0);
        assert_eq!(heap.stats().unwrap().reclaimed_nodes, 1);
    }
}

#[test]
fn read_lease_keeps_a_target_alive_after_the_host_drops_its_owner() {
    let heap = SharedHeap::default();
    let owner = node(&heap, b"read lease");
    let weak = owner.downgrade().unwrap();
    let guard = owner.read().unwrap();
    drop(owner);
    assert_eq!(guard.bytes(), b"read lease");
    assert_eq!(heap.stats().unwrap().live_nodes, 1);
    assert_eq!(
        weak.upgrade()
            .unwrap()
            .replace(payload(b"no", vec![]), None)
            .unwrap_err()
            .error
            .kind,
        ErrorKind::BorrowConflict
    );
    drop(guard);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert_eq!(weak.upgrade().unwrap_err().kind, ErrorKind::ExpiredWeak);
}

#[test]
fn mixed_replacements_match_independent_reachability_and_count_oracles() {
    const N: usize = 12;
    let heap = SharedHeap::default();
    let owners: Vec<_> = (0..N).map(|i| node(&heap, &[i as u8])).collect();
    let mut graph = vec![Vec::<(usize, bool)>::new(); N];
    let mut random = 0x946b_d132_756c_0185u64;
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        random as usize
    };
    let mut rejected = 0;
    let mut accepted = 0;
    for _ in 0..1000 {
        let from = next() % N;
        let mut proposed = Vec::new();
        let mut edges = Vec::new();
        for _ in 0..next() % 5 {
            let to = next() % N;
            let weak = next() % 3 == 0;
            proposed.push((to, weak));
            edges.push(if weak {
                Edge::Weak(owners[to].downgrade().unwrap())
            } else {
                Edge::Strong(owners[to].try_clone().unwrap())
            });
        }
        // Floyd-Warshall over a boolean matrix is independent of the runtime's
        // per-edge DFS and its generation/refcount bookkeeping.
        let mut reachable = [[false; N]; N];
        for (v, outgoing) in graph.iter().enumerate() {
            for &(to, weak) in outgoing {
                if !weak {
                    reachable[v][to] = true;
                }
            }
        }
        for k in 0..N {
            for i in 0..N {
                for j in 0..N {
                    reachable[i][j] |= reachable[i][k] && reachable[k][j];
                }
            }
        }
        let cycle = proposed
            .iter()
            .any(|&(to, weak)| !weak && (to == from || reachable[to][from]));
        let before = heap.stats().unwrap();
        let counts: Vec<_> = owners.iter().map(|o| o.strong_count().unwrap()).collect();
        match owners[from].replace(payload(&[from as u8], edges), None) {
            Ok(()) => {
                assert!(!cycle);
                graph[from] = proposed;
                accepted += 1;
            }
            Err(failure) => {
                assert!(cycle);
                assert_eq!(failure.error.kind, ErrorKind::StrongCycle);
                assert_eq!(heap.stats().unwrap(), before);
                assert_eq!(
                    owners
                        .iter()
                        .map(|o| o.strong_count().unwrap())
                        .collect::<Vec<_>>(),
                    counts
                );
                drop(failure);
                rejected += 1;
            }
        }
        for (v, owner) in owners.iter().enumerate() {
            let incoming = graph
                .iter()
                .flatten()
                .filter(|&&(to, weak)| to == v && !weak)
                .count();
            assert_eq!(owner.strong_count().unwrap(), incoming as u64 + 1);
            assert_eq!(owner.read().unwrap().bytes(), [v as u8]);
        }
        assert_eq!(
            heap.stats().unwrap().stored_edges,
            graph.iter().map(Vec::len).sum()
        );
    }
    assert!(accepted > 100 && rejected > 100);
    drop(owners);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert_eq!(heap.stats().unwrap().reclaimed_nodes, N as u64);
}
