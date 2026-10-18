use super::*;

fn run(source: &str) -> i64 {
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
    result.value
}
#[test]
fn source_vec_scalar_executes_a_registered_element_read() {
    assert_eq!(run(include_str!("../../examples/scripts/vec.lang")), 9);
}
#[test]
fn source_vec_owning_values_grow_mutate_projected_buffers_pop_and_forward_loans() {
    assert_eq!(
        run(include_str!("../../examples/scripts/vec_owned.lang")),
        12
    );
}

#[test]
fn source_vec_copy_push_and_mutable_element_assignment_preserve_reusable_operands() {
    assert_eq!(
        run(r#"fn main() -> i64 {
        let mut number: i64 = 9;
        let mut values: core::Vec<i64> = core::result_unwrap::<core::Vec<i64>, core::AllocError>(core::vec_new::<i64>(core::allocator()));
        let mut added: core::Result<core::Unit, core::AllocError> = core::vec_push::<i64>(values, number);
        match core::vec_get_mut::<i64>(values, 0) { core::Result::Ok(item) => { (*item) = number + 1; }, core::Result::Err(_) => { return -1; }, }
        return number + core::option_unwrap::<i64>(core::vec_pop::<i64>(values));
    }"#),
        19
    );
}

#[test]
fn source_vec_bounds_empty_pop_and_unused_provisional_reads_leave_no_live_registration() {
    assert_eq!(
        run(r#"fn main() -> i64 {
        let mut values: core::Vec<i64> = core::result_unwrap::<core::Vec<i64>, core::AllocError>(core::vec_new::<i64>(core::allocator()));
        let mut score: i64 = 0;
        match core::vec_pop::<i64>(values) { core::Option::None => { score = 1; }, core::Option::Some(_) => { return -1; }, }
        let mut added: core::Result<core::Unit, core::AllocError> = core::vec_push::<i64>(values, 5);
        match core::vec_get::<i64>(values, -1) { core::Result::Ok(_) => { return -2; }, core::Result::Err(error) => { match error { core::BoundsError::OutOfBounds => { score = score + 10; }, } }, }
        match core::vec_get_mut::<i64>(values, 99) { core::Result::Ok(_) => { return -3; }, core::Result::Err(other_error) => { match other_error { core::BoundsError::OutOfBounds => { score = score + 100; }, } }, }
        match core::vec_get::<i64>(values, 0) { core::Result::Ok(_) => {}, core::Result::Err(_) => { return -4; }, }
        let mut pushed: core::Result<core::Unit, core::AllocError> = core::vec_push::<i64>(values, 7);
        let mut last: i64 = core::option_unwrap::<i64>(core::vec_pop::<i64>(values));
        return score + last + core::vec_len::<i64>(values);
    }"#),
        119
    );
}

#[test]
fn source_vec_whole_owning_element_assignment_retires_the_displaced_payload() {
    let source = include_str!("../../examples/scripts/vec_owned.lang").replace(
        "pair.right = make(3);",
        "(*pair) = Pair { left: make(4), right: make(5) };",
    );
    assert_eq!(run(&source), 16);
}

#[test]
fn source_vec_text_elements_read_clone_and_pop_through_checked_utf8_storage() {
    assert_eq!(
        run(r#"fn main() -> i64 {
        let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
        let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
        let mut text: core::Text = core::result_unwrap::<core::Text, core::Utf8Error>(core::text_from_utf8(bytes));
        let mut values: core::Vec<core::Text> = core::result_unwrap::<core::Vec<core::Text>, core::AllocError>(core::vec_new::<core::Text>(core::allocator()));
        let mut added: core::Result<core::Unit, core::AllocError> = core::vec_push::<core::Text>(values, text);
        match core::vec_get::<core::Text>(values, 0) { core::Result::Ok(value) => { let mut clone: core::Text = core::result_unwrap::<core::Text, core::AllocError>(core::text_clone(core::allocator(), value)); if core::text_len(clone) < 1 { return -1; } }, core::Result::Err(_) => { return -2; }, }
        let mut last: core::Text = core::option_unwrap::<core::Text>(core::vec_pop::<core::Text>(values)); return core::text_len(last);
    }"#),
        1
    );
}

#[test]
fn source_vec_inline_enum_payloads_survive_borrowed_matching_and_whole_pop() {
    assert_eq!(
        run(r#"enum Item { Empty, Full(core::Bytes) }
    fn main() -> i64 {
        let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
        let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
        let mut values: core::Vec<Item> = core::result_unwrap::<core::Vec<Item>, core::AllocError>(core::vec_new::<Item>(core::allocator()));
        let mut added: core::Result<core::Unit, core::AllocError> = core::vec_push::<Item>(values, Item::Full(bytes));
        let mut score: i64 = 0;
        match core::vec_get::<Item>(values, 0) { core::Result::Ok(item) => { match item { Item::Empty => { return -1; }, Item::Full(data) => { score = core::bytes_len(data); }, } }, core::Result::Err(_) => { return -2; }, }
        let mut last: Item = core::option_unwrap::<Item>(core::vec_pop::<Item>(values));
        match last { Item::Empty => { return -3; }, Item::Full(other) => { return score + core::bytes_len(other); }, }
    }"#),
        2
    );
}

const CLONED_RETURN: &str = r#"fn copy_out() -> core::Bytes {
    let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
    let mut values: core::Vec<core::Bytes> = core::result_unwrap::<core::Vec<core::Bytes>, core::AllocError>(core::vec_new::<core::Bytes>(core::allocator()));
    let mut added: core::Result<core::Unit, core::AllocError> = core::vec_push::<core::Bytes>(values, bytes);
    match core::vec_get::<core::Bytes>(values, 0) { core::Result::Ok(value) => { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_clone(core::allocator(), value)); }, core::Result::Err(_) => { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); }, }
}
fn main() -> i64 { let mut result: core::Bytes = copy_out(); return core::bytes_len(result); }"#;
#[test]
fn source_vec_borrowed_payload_clone_survives_callee_vector_cleanup_and_a_protected_return() {
    assert_eq!(run(CLONED_RETURN), 1);
}

const RECURSIVE: &str = r#"fn carry(values: core::Vec<core::Bytes>, count: i64) -> core::Vec<core::Bytes> { if count < 1 { return values; } return carry(values, count + (-1)); }
fn main() -> i64 {
    let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
    let mut values: core::Vec<core::Bytes> = core::result_unwrap::<core::Vec<core::Bytes>, core::AllocError>(core::vec_new::<core::Bytes>(core::allocator()));
    let mut added: core::Result<core::Unit, core::AllocError> = core::vec_push::<core::Bytes>(values, bytes);
    let mut returned: core::Vec<core::Bytes> = carry(values, 10);
    let mut last: core::Bytes = core::option_unwrap::<core::Bytes>(core::vec_pop::<core::Bytes>(returned)); return core::bytes_len(last);
}"#;
#[test]
fn source_vec_owners_cross_recursive_returns_and_cleanup_after_frame_refusal() {
    assert_eq!(run(RECURSIVE), 1);
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
        1
    );
    assert_all_released(&allocator, &trace.borrow());
}

#[test]
fn source_vec_allocator_parameters_reborrow_the_authority_for_multiple_constructors() {
    assert_eq!(
        run(r#"fn make(a: &core::Allocator) -> core::Vec<i64> {
        let mut first: core::Vec<i64> = core::result_unwrap::<core::Vec<i64>, core::AllocError>(core::vec_new::<i64>((*a)));
        let mut second: core::Vec<i64> = core::result_unwrap::<core::Vec<i64>, core::AllocError>(core::vec_new::<i64>((*a)));
        let mut added: core::Result<core::Unit, core::AllocError> = core::vec_push::<i64>(second, 7); return second;
    } fn main() -> i64 { let mut values: core::Vec<i64> = make(core::allocator()); return core::option_unwrap::<i64>(core::vec_pop::<i64>(values)); }"#),
        7
    );
}

#[test]
fn source_vec_and_service_specific_payload_preflight_rejects_inactive_unsupported_shapes_before_effects()
 {
    for later in [
        "let mut later: core::Option<core::Vec<core::Vec<i64>>> = core::Option::None;",
        "let mut later: core::Option<core::Vec<shared i64>> = core::Option::None;",
        "let mut later: core::Option<shared Payload> = core::Option::None;",
        "let mut later: core::Result<shared Payload, core::OwnershipError> = core::shared_new::<Payload>(core::allocator(), Payload { values: core::Option::None });",
        "arena graph(core::allocator()) { let mut later: core::Result<arena Payload, core::ArenaError> = core::arena_new::<Payload>(graph, Payload { values: core::Option::None }); }",
    ] {
        let source = format!(
            "struct Payload {{ values: core::Option<core::Vec<i64>> }} fn main() -> i64 {{ let mut earlier: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); {later} return 0; }}"
        );
        let program = checked(&source);
        let (mut allocator, trace) = allocator_with_limits(Default::default());
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            Control::default(),
        )
        .err()
        .unwrap();
        assert!(
            matches!(error.reason, vm::Reason::Unsupported { .. }),
            "{error:?}"
        );
        assert_eq!(error.usage.fuel, 0);
        assert_eq!(allocator.stats(), a::Stats::default());
        assert!(trace.borrow().requests.is_empty());
    }
}

#[test]
fn every_vec_source_provider_oom_releases_evaluated_arguments_and_allows_instance_reuse() {
    let program = checked(include_str!("../../examples/scripts/vec_owned.lang"));
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
        if let Err(e) = &result {
            assert!(
                matches!(
                    e.reason,
                    vm::Reason::Preparation(_)
                        | vm::Reason::Execution(c::FailureKind::Execution(
                            ExecutionFailure::AllocationLimit | ExecutionFailure::Panic
                        ))
                ),
                "attempt {fail}: {e:?}"
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
            12
        );
        assert_all_released(&allocator, &trace.borrow());
    }
}

#[test]
fn every_vec_source_fuel_prefix_ends_element_loans_and_retires_every_owner() {
    let program = checked(include_str!("../../examples/scripts/vec_owned.lang"));
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
            assert_eq!(result.unwrap().value, 12);
        } else {
            let e = result.err().unwrap();
            assert!(
                matches!(
                    e.reason,
                    vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
                ),
                "fuel {available}: {e:?}"
            );
            assert_eq!(e.usage.fuel, available);
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
fn interruption_during_vec_callee_teardown_retires_the_protected_cloned_result() {
    let program = checked(CLONED_RETURN);
    let flag = Rc::new(InterruptionFlag::new());
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    trace.borrow_mut().interrupt_release = Some(flag.clone());
    let mut instance = c::Instance::default();
    let e = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::default().and_interruption(&flag),
    )
    .err()
    .unwrap();
    assert!(
        matches!(
            e.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::Interrupted))
        ),
        "{e:?}"
    );
    assert_all_released(&allocator, &trace.borrow());
    assert!(!instance.is_poisoned());
}

#[test]
fn source_vec_loan_pool_exhaustion_fails_before_a_new_registration_and_unwinds_all_frames() {
    let program = checked(
        r#"fn dive(values: &core::Vec<i64>, count: i64) -> i64 {
        match core::vec_get::<i64>(values, 0) { core::Result::Ok(value) => { if count < 1 { return (*value); } let mut answer: i64 = dive(values, count + (-1)); return answer + (*value); }, core::Result::Err(_) => { return -1; }, }
    } fn main() -> i64 { let mut values: core::Vec<i64> = core::result_unwrap::<core::Vec<i64>, core::AllocError>(core::vec_new::<i64>(core::allocator())); let mut added: core::Result<core::Unit, core::AllocError> = core::vec_push::<i64>(values, 1); return dive(values, 300); }"#,
    );
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::with_limits(Limits {
            frames: Some(512),
            ..Default::default()
        }),
    )
    .err()
    .unwrap();
    assert!(
        matches!(
            error.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::AllocationLimit))
        ),
        "{error:?}"
    );
    assert_all_released(&allocator, &trace.borrow());
    assert!(!instance.is_poisoned());
}
