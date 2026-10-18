use super::*;

fn run(source: &str) -> (i64, a::Stats) {
    let program = checked(source);
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    let mut instance = c::Instance::default();
    let result = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::default(),
    )
    .unwrap();
    assert_all_released(&allocator, &trace.borrow());
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
    (result.value, allocator.stats())
}

#[test]
fn source_shared_scalar_reads_through_a_registered_loan_and_releases_all_storage() {
    assert_eq!(run(include_str!("../../examples/scripts/shared.lang")).0, 9);
}

#[test]
fn source_shared_owned_graphs_reject_cycles_and_forward_live_read_loans() {
    assert_eq!(
        run(include_str!("../../examples/scripts/shared_owned.lang")).0,
        13
    );
}

#[test]
fn source_weak_owner_outlives_payload_and_upgrade_reports_expiration() {
    let source = r#"fn main() -> i64 {
        let mut saved: core::Option<weak i64> = core::Option::None;
        if true { let mut made: core::Result<shared i64, core::OwnershipError> = core::shared_new::<i64>(core::allocator(), 7);
          let mut owner: shared i64 = core::result_unwrap::<shared i64, core::OwnershipError>(made);
          let mut weak_result: core::Result<weak i64, core::OwnershipError> = core::weak_new::<i64>(owner);
          let mut weak_value: weak i64 = core::result_unwrap::<weak i64, core::OwnershipError>(weak_result);
          saved = core::Option::Some(weak_value); }
        match saved { core::Option::Some(value) => {
            match core::weak_upgrade::<i64>(value) {
                core::Result::Ok(_) => { return -1; },
                core::Result::Err(error) => { match error { core::OwnershipError::ExpiredWeak => { return 17; }, _ => { return -2; }, } },
            }
        }, core::Option::None => { return -3; }, }
    }"#;
    assert_eq!(run(source).0, 17);
}

#[test]
fn source_shared_text_payload_uses_the_same_checked_buffer_read_surface() {
    let source = r#"fn main() -> i64 {
        let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
        let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
        let mut text: core::Text = core::result_unwrap::<core::Text, core::Utf8Error>(core::text_from_utf8(bytes));
        let mut owner: shared core::Text = core::result_unwrap::<shared core::Text, core::OwnershipError>(core::shared_new::<core::Text>(core::allocator(), text));
        match core::shared_read::<core::Text>(owner) { core::Result::Ok(value) => { return core::text_len(value); }, core::Result::Err(_) => { return -1; }, }
    }"#;
    assert_eq!(run(source).0, 1);
}

#[test]
fn recursive_shared_shapes_with_inactive_nested_collections_execute() {
    assert_eq!(run(r#"struct Node { next: core::Option<shared Node>, pending: core::Option<core::Vec<i64>> }
    fn main() -> i64 { let mut first: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); let mut saved: core::Option<shared Node> = core::Option::None; return 0; }"#).0, 0);
}

#[test]
fn source_shared_copy_operands_survive_new_and_replace_and_unused_reads_end() {
    let source = r#"fn main() -> i64 { let mut number: i64 = 9;
        let mut owner: shared i64 = core::result_unwrap::<shared i64, core::OwnershipError>(core::shared_new::<i64>(core::allocator(), number));
        match core::shared_read::<i64>(owner) { core::Result::Ok(_) => {}, core::Result::Err(_) => { return -3; }, }
        let mut changed: core::Result<core::Unit, core::OwnershipError> = core::shared_replace::<i64>(owner, number);
        match changed { core::Result::Err(_) => { return -1; }, core::Result::Ok(_) => {}, }
        match core::shared_read::<i64>(owner) { core::Result::Ok(value) => { return number + (*value); }, core::Result::Err(_) => { return -2; }, }
    }"#;
    assert_eq!(run(source).0, 18);
}

#[test]
fn source_shared_alias_mutation_fails_while_another_guest_parameter_has_a_live_read() {
    let source = r#"fn probe(first: &shared i64, second: &mut shared i64) -> i64 {
        match core::shared_read::<i64>(first) { core::Result::Ok(value) => {
            let mut changed: core::Result<core::Unit, core::OwnershipError> = core::shared_replace::<i64>(second, 11);
            match changed { core::Result::Ok(_) => { return -1; }, core::Result::Err(error) => {
                match error { core::OwnershipError::BorrowConflict => { return (*value) + 1; }, _ => { return -2; }, }
            }, }
        }, core::Result::Err(_) => { return -3; }, }
    }
    fn main() -> i64 { let mut owner: shared i64 = core::result_unwrap::<shared i64, core::OwnershipError>(core::shared_new::<i64>(core::allocator(), 7));
        let mut alias: shared i64 = core::result_unwrap::<shared i64, core::OwnershipError>(core::shared_clone::<i64>(owner)); return probe(owner, alias);
    }"#;
    assert_eq!(run(source).0, 8);
}

const CLONED_RETURN: &str = r#"struct Payload { bytes: core::Bytes }
fn copy_out() -> core::Bytes {
    let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
    let mut owner: shared Payload = core::result_unwrap::<shared Payload, core::OwnershipError>(core::shared_new::<Payload>(core::allocator(), Payload { bytes: bytes }));
    match core::shared_read::<Payload>(owner) { core::Result::Ok(value) => { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_clone(core::allocator(), value.bytes)); }, core::Result::Err(_) => { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); }, }
}
fn main() -> i64 { let mut bytes: core::Bytes = copy_out(); return core::bytes_len(bytes); }"#;
#[test]
fn source_cloned_read_payload_survives_shared_owner_cleanup_and_a_protected_guest_return() {
    assert_eq!(run(CLONED_RETURN).0, 1);
}

#[test]
fn source_shared_allocator_parameters_reborrow_instead_of_moving_the_authority() {
    let source = r#"fn make(allocator: &core::Allocator) -> shared i64 {
        let mut first: shared i64 = core::result_unwrap::<shared i64, core::OwnershipError>(core::shared_new::<i64>(allocator, 5));
        return core::result_unwrap::<shared i64, core::OwnershipError>(core::shared_new::<i64>((*allocator), 7));
    }
    fn main() -> i64 { let mut owner: shared i64 = make(core::allocator()); match core::shared_read::<i64>(owner) { core::Result::Ok(value) => { return (*value); }, core::Result::Err(_) => { return -1; }, } }"#;
    assert_eq!(run(source).0, 7);
}

const CHAIN: &str = r#"struct Node { bytes: core::Bytes, next: core::Option<shared Node> }
fn main() -> i64 { let mut tail: core::Option<shared Node> = core::Option::None; let mut i: i64 = 0;
    while i < 64 { let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
        let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
        let mut owner: shared Node = core::result_unwrap::<shared Node, core::OwnershipError>(core::shared_new::<Node>(core::allocator(), Node { bytes: bytes, next: tail }));
        tail = core::Option::Some(owner); i = i + 1;
    }
    match tail { core::Option::Some(head) => { match core::shared_read::<Node>(head) { core::Result::Ok(value) => { return core::bytes_len(value.bytes); }, core::Result::Err(_) => { return -1; }, } }, core::Option::None => { return -2; }, }
}"#;
#[test]
fn source_loop_moves_whole_optional_owners_into_a_shared_chain_and_retires_every_node() {
    assert_eq!(run(CHAIN).0, 1);
}

const RECURSIVE: &str = r#"fn descend(owner: shared i64, count: i64) -> i64 {
    if count < 1 { match core::shared_read::<i64>(owner) { core::Result::Ok(value) => { return (*value); }, core::Result::Err(_) => { return -1; }, } }
    let mut next: shared i64 = core::result_unwrap::<shared i64, core::OwnershipError>(core::shared_clone::<i64>(owner));
    return descend(next, count + -1);
}
fn main() -> i64 { let mut owner: shared i64 = core::result_unwrap::<shared i64, core::OwnershipError>(core::shared_new::<i64>(core::allocator(), 9)); return descend(owner, 10); }"#;
#[test]
fn recursive_shared_owners_use_bounded_guest_frames_and_release_after_frame_refusal() {
    assert_eq!(run(RECURSIVE).0, 9);
    let program = checked(RECURSIVE);
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::with_limits(Limits {
            frames: Some(4),
            ..Default::default()
        }),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FrameLimit))
    ));
    assert_all_released(&allocator, &trace.borrow());
    assert!(!instance.is_poisoned());
    assert_eq!(
        vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Control::default()
        )
        .unwrap()
        .value,
        9
    );
    assert_all_released(&allocator, &trace.borrow());
}

#[test]
fn every_shared_source_provider_oom_releases_arguments_payloads_and_registry_and_can_reuse_instance()
 {
    let program = checked(CLONED_RETURN);
    let (mut baseline, trace) = allocator_with_limits(Default::default());
    vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        Control::default(),
    )
    .unwrap();
    let attempts = trace.borrow().requests.len();
    for fail in 1..=attempts {
        let (mut allocator, trace) = allocator_with_limits(Default::default());
        trace.borrow_mut().fail_at = fail;
        let mut instance = c::Instance::default();
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
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
        assert_all_released(&allocator, &trace.borrow());
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
        assert!(!instance.is_poisoned(), "attempt {fail}");
        trace.borrow_mut().fail_at = 0;
        assert_eq!(
            vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                Control::default()
            )
            .unwrap()
            .value,
            1
        );
        assert_all_released(&allocator, &trace.borrow());
    }
}

#[test]
fn every_shared_source_fuel_prefix_retires_clones_weak_metadata_and_nested_read_loans() {
    let program = checked(include_str!("../../examples/scripts/shared_owned.lang"));
    let (mut baseline, _) = allocator_with_limits(Default::default());
    let fuel = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        Control::default(),
    )
    .unwrap()
    .usage
    .fuel;
    for available in 0..=fuel {
        let (mut allocator, trace) = allocator_with_limits(Default::default());
        let mut instance = c::Instance::default();
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(Limits {
                fuel: Some(available),
                ..Default::default()
            }),
        );
        if available == fuel {
            assert_eq!(result.unwrap().value, 13);
        } else {
            let error = result.err().unwrap();
            assert!(
                matches!(
                    error.reason,
                    vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
                ),
                "fuel {available}: {error:?}"
            );
            assert_eq!(error.usage.fuel, available);
        }
        assert_all_released(&allocator, &trace.borrow());
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn interruption_during_shared_payload_teardown_discards_the_protected_cloned_return() {
    let program = checked(CLONED_RETURN);
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    let flag = Rc::new(InterruptionFlag::new());
    trace.borrow_mut().interrupt_release = Some(flag.clone());
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::default().and_interruption(&flag),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::Interrupted))
    ));
    assert_all_released(&allocator, &trace.borrow());
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
    assert!(!instance.is_poisoned());
}
