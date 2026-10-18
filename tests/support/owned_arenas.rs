use super::*;

fn run(source: &str) -> (i64, a::Stats) {
    let program = checked(source);
    let (mut allocator, _) = allocator_with_limits(a::Limits {
        live_allocations: 1024,
        ..Default::default()
    });
    let mut instance = c::Instance::default();
    let result = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::default(),
    )
    .unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
    (result.value, allocator.stats())
}
const OWNED: &str = r#"
struct Node { bytes: core::Bytes, next: core::Option<arena Node> }
fn main() -> i64 {
    arena graph(core::allocator()) {
        let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
        let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
        let mut token: arena Node = core::result_unwrap::<arena Node, core::ArenaError>(core::arena_new::<Node>(graph, Node { bytes: bytes, next: core::Option::None }));
        let mut replacement: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
        let mut changed: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<Node>(graph, token, Node { bytes: replacement, next: core::Option::Some(token) });
        match core::arena_read::<Node>(graph, token) {
            core::Result::Ok(node) => { return core::bytes_len(node.bytes); },
            core::Result::Err(_) => { return -1; },
        }
    }
}
"#;

#[test]
fn source_arena_sample_observes_replacement_through_checked_read() {
    assert_eq!(run(include_str!("../../examples/scripts/arena.lang")).0, 9);
    assert_eq!(run(include_str!("../../examples/analysis/arena.lang")).0, 0);
}

#[test]
fn source_arena_owned_node_replacement_cleans_old_and_new_descendants() {
    assert_eq!(run(OWNED).0, 0);
}

#[test]
fn source_arena_loop_reentry_reuses_prepared_wrappers_with_new_generations() {
    let source = r#"fn main() -> i64 {
        let mut total: i64 = 0;
        let mut i: i64 = 0;
        while i < 5 {
            arena graph(core::allocator()) {
                let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(graph, i));
                match core::arena_read::<i64>(graph, token) {
                    core::Result::Ok(value) => { total = total + (*value); },
                    core::Result::Err(_) => { return -1; },
                }
            }
            i = i + 1;
        }
        return total;
    }"#;
    assert_eq!(run(source).0, 10);
}

#[test]
fn nested_source_arenas_reborrow_the_shared_allocator_parameter() {
    let source = r#"fn build(allocator: &core::Allocator) -> i64 {
        let mut total: i64 = 0;
        arena outer(allocator) {
            let mut a: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(outer, 5));
            arena inner(allocator) {
                let mut b: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(inner, 7));
                match core::arena_read::<i64>(inner, b) {
                    core::Result::Ok(value) => { total = total + (*value); },
                    core::Result::Err(_) => { return -1; },
                }
            }
            match core::arena_read::<i64>(outer, a) {
                core::Result::Ok(outside) => { total = total + (*outside); },
                core::Result::Err(_) => { return -2; },
            }
        }
        return total;
    }
    fn main() -> i64 { return build(core::allocator()); }"#;
    assert_eq!(run(source).0, 12);
    let dereferenced = source.replace("arena inner(allocator)", "arena inner((*allocator))");
    assert_eq!(run(&dereferenced).0, 12);
}

const CYCLE: &str = include_str!("../../examples/scripts/arena_owned.lang");
#[test]
fn source_arena_cycles_follow_nested_reads_across_owning_guest_calls() {
    assert_eq!(run(CYCLE).0, 5);
}

#[test]
fn unused_provisional_arena_read_ends_before_the_next_mutation() {
    let source = r#"fn main() -> i64 { arena graph(core::allocator()) {
        let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(graph, 7));
        match core::arena_read::<i64>(graph, token) { core::Result::Ok(_) => {}, core::Result::Err(_) => { return -1; }, }
        let mut changed: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<i64>(graph, token, 11);
        match core::arena_read::<i64>(graph, token) {
            core::Result::Ok(value) => { return (*value); }, core::Result::Err(_) => { return -2; },
        }
    } }"#;
    assert_eq!(run(source).0, 11);
}

#[test]
fn unsupported_reachable_container_after_arena_allocation_rejects_before_callbacks() {
    let source = r#"fn main() -> i64 { arena graph(core::allocator()) {
        let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(graph, 7));
        let mut pending: core::Result<core::Map<i64, core::Vec<i64>>, core::AllocError> = core::map_new::<i64, core::Vec<i64>>(core::allocator());
        return 0;
    } }"#;
    let program = checked(source);
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::default(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        vm::Reason::Unsupported {
            core: Some(Operation::MapNew)
        }
    ));
    assert_eq!(error.usage.fuel, 0);
    assert!(trace.borrow().requests.is_empty());
    assert_eq!(allocator.stats().requests, 0);
}

#[test]
fn every_provider_oom_in_source_arena_execution_releases_all_live_storage() {
    let program = checked(CYCLE);
    let (mut baseline, trace) = allocator_with_limits(a::Limits {
        live_allocations: 1024,
        ..Default::default()
    });
    vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        Control::default(),
    )
    .unwrap();
    let attempts = trace.borrow().requests.len();
    for fail in 1..=attempts {
        let (mut allocator, trace) = allocator_with_limits(a::Limits {
            live_allocations: 1024,
            ..Default::default()
        });
        trace.borrow_mut().fail_at = fail;
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            Control::default(),
        );
        if let Err(error) = &result {
            assert!(
                matches!(
                    error.reason,
                    vm::Reason::Preparation(_)
                        | vm::Reason::Execution(c::FailureKind::Execution(
                            ExecutionFailure::AllocationLimit | ExecutionFailure::Panic
                        ))
                ),
                "attempt {fail}: {error:?}"
            );
        }
        assert_eq!(
            allocator.stats().live_allocations,
            0,
            "attempt {fail}: {:?}",
            result.as_ref().err()
        );
        assert_eq!(allocator.stats().live_bytes, 0, "attempt {fail}");
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations,
            "attempt {fail}"
        );
        assert_eq!(
            allocator.stats().cumulative_bytes,
            trace
                .borrow()
                .requests
                .iter()
                .map(|r| r.bytes())
                .sum::<u64>()
        );
    }
}

#[test]
fn arena_allocation_errors_clean_consumed_arguments_before_publishing_the_typed_error() {
    let source = r#"fn main() -> i64 { arena graph(core::allocator()) {
        let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
        match core::arena_new::<core::Bytes>(graph, bytes) {
            core::Result::Ok(_) => { return 1; },
            core::Result::Err(error) => { match error {
                core::ArenaError::Allocation(reason) => { match reason {
                    core::AllocError::OutOfMemory => { return 42; },
                    core::AllocError::Limit => { return 43; },
                    core::AllocError::Overflow => { return 44; },
                } },
                core::ArenaError::CrossRegion => { return -1; },
                core::ArenaError::Unavailable => { return -2; },
                core::ArenaError::BorrowConflict => { return -3; },
            } },
        }
    } }"#;
    let program = checked(source);
    let (mut baseline, trace) = allocator_with_limits(a::Limits {
        live_allocations: 1024,
        ..Default::default()
    });
    assert_eq!(
        vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut baseline),
            Control::default()
        )
        .unwrap()
        .value,
        1
    );
    let node_request = trace
        .borrow()
        .requests
        .iter()
        .position(|r| r.bytes() == 56)
        .unwrap()
        + 1;
    for denied in [false, true] {
        let limits = a::Limits {
            live_allocations: 1024,
            cumulative_bytes: if denied {
                baseline.stats().cumulative_bytes - 1
            } else {
                u64::MAX
            },
            ..Default::default()
        };
        let (mut allocator, trace) = allocator_with_limits(limits);
        if !denied {
            trace.borrow_mut().fail_at = node_request;
        }
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            Control::default(),
        )
        .unwrap();
        assert_eq!(result.value, if denied { 43 } else { 42 });
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
    }
}

#[test]
fn recursive_arena_activations_use_prepared_pools_and_drain_on_frame_limit() {
    let source = r#"fn descend(n: i64) -> i64 { arena graph(core::allocator()) {
        let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(graph, n));
        if n < 4 { return descend(n + 1); }
        match core::arena_read::<i64>(graph, token) {
            core::Result::Ok(value) => { return (*value); }, core::Result::Err(_) => { return -1; },
        }
    } }
    fn main() -> i64 { return descend(0); }"#;
    let program = checked(source);
    for frames in [None, Some(2), Some(8)] {
        let (mut allocator, _) = allocator_with_limits(a::Limits {
            live_allocations: 4096,
            ..Default::default()
        });
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            Control::with_limits(Limits {
                frames,
                ..Default::default()
            }),
        );
        if frames == Some(2) {
            assert!(matches!(
                result.err().unwrap().reason,
                vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FrameLimit))
            ));
        } else {
            assert_eq!(result.unwrap().value, 4);
        }
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
    }
}

#[test]
fn cloned_owned_read_payload_survives_arena_exit_and_protected_return() {
    let source = r#"fn copied() -> core::Bytes { arena graph(core::allocator()) {
        let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
        let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
        let mut token: arena core::Bytes = core::result_unwrap::<arena core::Bytes, core::ArenaError>(core::arena_new::<core::Bytes>(graph, bytes));
        match core::arena_read::<core::Bytes>(graph, token) {
            core::Result::Ok(value) => { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_clone(core::allocator(), value)); },
            core::Result::Err(_) => { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); },
        }
    } }
    fn main() -> i64 { let mut bytes: core::Bytes = copied(); return core::bytes_len(bytes); }"#;
    assert_eq!(run(source).0, 1);
}
