use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn exhausted_region_generations_are_retired_without_reuse() {
    let heap = ArenaHeap::new(Limits {
        arena_slots: 2,
        ..Limits::default()
    })
    .unwrap();
    let mut arena = heap.open(None).unwrap();
    let mut reference = arena.allocate(Payload::default(), None).unwrap();
    let key = arena.key().unwrap();
    heap.inner.lock().unwrap().slots[key.slot].generation = u64::MAX;
    arena.key.as_mut().unwrap().generation = u64::MAX;
    reference.identity.generation = u64::MAX;
    drop(arena.close(None).unwrap());
    let mut next = heap.open(None).unwrap();
    let fresh = next.allocate(Payload::default(), None).unwrap();
    assert_ne!(fresh.identity.arena_slot, reference.identity.arena_slot);
    assert_eq!(
        next.read(reference, None).unwrap_err().kind,
        ErrorKind::Unavailable
    );
    assert_eq!(heap.stats().unwrap().retired_nodes, 1);
}

#[test]
fn provenance_counter_exhaustion_does_not_wrap_or_reissue_an_id() {
    let counter = AtomicU64::new(u64::MAX - 1);
    assert_eq!(next_runtime(&counter).unwrap(), u64::MAX - 1);
    assert_eq!(
        next_runtime(&counter).unwrap_err().kind,
        ErrorKind::CountOverflow
    );
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
}

#[test]
fn actual_reader_count_overflow_does_not_publish_a_lease() {
    let heap = ArenaHeap::new(Limits::default()).unwrap();
    let mut arena = heap.open(None).unwrap();
    let reference = arena.allocate(Payload::default(), None).unwrap();
    let key = arena.key().unwrap();
    heap.inner.lock().unwrap().region_mut(key).unwrap().readers = u64::MAX;
    assert_eq!(
        arena.read(reference, None).unwrap_err().kind,
        ErrorKind::CountOverflow
    );
    assert_eq!(
        heap.inner.lock().unwrap().region(key).unwrap().readers,
        u64::MAX
    );
    heap.inner.lock().unwrap().region_mut(key).unwrap().readers = 0;
    drop(arena);
    assert_eq!(heap.stats().unwrap().resident_payloads, 0);
}

#[test]
fn poisoned_state_denies_access_and_publication_without_consuming_input() {
    let heap = ArenaHeap::new(Limits::default()).unwrap();
    let mut arena = heap.open(None).unwrap();
    let reference = arena.allocate(Payload::default(), None).unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _guard = heap.inner.state.lock().unwrap();
            panic!("injected poison");
        }))
        .is_err()
    );
    let failure = arena
        .allocate(
            Payload {
                bytes: vec![9],
                edges: vec![],
            },
            None,
        )
        .unwrap_err();
    assert_eq!(failure.error.kind, ErrorKind::Poisoned);
    assert_eq!(failure.payload.bytes, [9]);
    assert_eq!(
        arena.read(reference, None).unwrap_err().kind,
        ErrorKind::Poisoned
    );
    assert_eq!(heap.stats().unwrap_err().kind, ErrorKind::Poisoned);
}

#[test]
fn poisoned_budget_quarantines_publication_but_storage_release_is_infallible() {
    let heap = ArenaHeap::new(Limits::default()).unwrap();
    let mut arena = heap.open(None).unwrap();
    let reference = arena.allocate(Payload::default(), None).unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _guard = heap.inner.budget.counts.lock().unwrap();
            panic!("injected poison");
        }))
        .is_err()
    );
    assert_eq!(
        arena
            .allocate(Payload::default(), None)
            .unwrap_err()
            .error
            .kind,
        ErrorKind::Poisoned
    );
    assert_eq!(arena.read(reference, None).unwrap().bytes().unwrap(), []);
    drop(arena.close(None).unwrap());
    let counts = heap.inner.budget.counts.lock().unwrap_err().into_inner();
    assert_eq!(*counts, Counts::default());
}
