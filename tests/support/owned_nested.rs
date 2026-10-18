use super::*;

fn run(source: &str) -> (i64, a::Stats) {
    let program = checked(source);
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    let result = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
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
fn source_nested_vector_strong_edges_reject_cycles_and_release_every_owner() {
    assert_eq!(
        run(include_str!("../../examples/analysis/nested_services.lang")).0,
        23
    );
}

#[test]
fn source_shared_vector_map_custody_survives_guest_calls_clone_weak_upgrade_and_reads() {
    assert_eq!(
        run(include_str!(
            "../../examples/scripts/nested_collections.lang"
        ))
        .0,
        8
    );
}

#[test]
fn source_arena_map_vector_cycle_reads_replacement_and_reverse_teardown_execute() {
    assert_eq!(
        run(include_str!("../../examples/scripts/nested_arena.lang")).0,
        4
    );
}

#[test]
fn source_map_vector_transitive_cycle_rejection_preserves_published_graphs_and_guest_returns() {
    assert_eq!(
        run(include_str!("../../examples/scripts/nested_cycle.lang")).0,
        21
    );
}

#[test]
fn source_mutable_nested_record_vector_map_replacement_removal_and_buffer_growth_execute() {
    assert_eq!(
        run(include_str!("../../examples/scripts/nested_mutation.lang")).0,
        6
    );
}

#[test]
fn cloned_nested_shared_owner_survives_parent_retirement_and_protected_guest_return() {
    let mut source = include_str!("../../examples/scripts/nested_collections.lang")
        .replace("fn main() -> i64", "fn extract() -> shared Cell")
        .replace("return inspect(value);", "return core::result_unwrap::<shared Cell, core::OwnershipError>(core::shared_clone::<Cell>(value));");
    for n in [2, 3, 4] {
        source = source.replace(
            &format!("return -{n};"),
            "return core::option_unwrap::<shared Cell>(core::Option::None);",
        );
    }
    source.push_str(r#"fn main() -> i64 {
        let mut alias: shared Cell = extract();
        match core::shared_read::<Cell>(alias) { core::Result::Ok(cell) => { return core::bytes_len(cell.bytes); }, core::Result::Err(_) => { return -9; }, }
    }"#);
    assert_eq!(run(&source).0, 1);
}

#[test]
fn nested_stored_weak_clone_and_expired_upgrade_keep_custody_after_payload_retirement() {
    let source = r#"fn make() -> shared core::Vec<weak i64> {
        let mut value: shared i64 = core::result_unwrap::<shared i64, core::OwnershipError>(core::shared_new::<i64>(core::allocator(), 7));
        let mut weak_value: weak i64 = core::result_unwrap::<weak i64, core::OwnershipError>(core::weak_new::<i64>(value));
        let mut values: core::Vec<weak i64> = core::result_unwrap::<core::Vec<weak i64>, core::AllocError>(core::vec_new::<weak i64>(core::allocator()));
        let mut added: core::Unit = core::result_unwrap::<core::Unit, core::AllocError>(core::vec_push::<weak i64>(values, weak_value));
        return core::result_unwrap::<shared core::Vec<weak i64>, core::OwnershipError>(core::shared_new::<core::Vec<weak i64>>(core::allocator(), values));
    }
    fn main() -> i64 {
        let mut owner: shared core::Vec<weak i64> = make();
        match core::shared_read::<core::Vec<weak i64>>(owner) { core::Result::Ok(values) => {
            match core::vec_get::<weak i64>(values, 0) { core::Result::Ok(value) => {
                let mut alias: weak i64 = core::result_unwrap::<weak i64, core::OwnershipError>(core::weak_clone::<i64>(value));
                match core::weak_upgrade::<i64>(value) { core::Result::Ok(_) => { return -1; }, core::Result::Err(error) => {
                    match error { core::OwnershipError::ExpiredWeak => { return 17; }, _ => { return -2; }, }
                }, }
            }, core::Result::Err(_) => { return -3; }, }
        }, core::Result::Err(_) => { return -4; }, }
    }"#;
    assert_eq!(run(source).0, 17);
}

#[test]
fn nested_arena_brands_cannot_cross_regions_escape_or_enter_shared_storage() {
    for tail in [
        "arena inner(core::allocator()) { let mut rejected: core::Result<arena Payload,core::ArenaError> = core::arena_new::<Payload>(inner,Payload{values:values}); }",
        "let mut rejected: core::Result<shared Payload,core::OwnershipError> = core::shared_new::<Payload>(core::allocator(),Payload{values:values});",
    ] {
        let source = format!(
            "struct Payload {{ values:core::Vec<arena i64> }} fn main()->i64 {{ arena outer(core::allocator()) {{ let mut first:arena i64=core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(outer,7)); let mut values:core::Vec<arena i64>=core::result_unwrap::<core::Vec<arena i64>,core::AllocError>(core::vec_new::<arena i64>(core::allocator())); let mut added:core::Result<core::Unit,core::AllocError>=core::vec_push::<arena i64>(values,first); {tail} }} return 0; }}"
        );
        let analysis = frontend::analyze_virtual(
            SourceMap::new(),
            Path::new("nested_escape.lang"),
            source.as_bytes(),
            None,
            Default::default(),
            None,
        );
        assert!(analysis.result.is_err());
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|d| matches!(d.code.as_str(), "SRC0022" | "SRC0023")),
            "{:?}",
            analysis.diagnostics
        );
    }
    let source = r#"fn main()->i64 {
        let mut saved: core::Option<core::Vec<arena i64>> = core::Option::None;
        arena graph(core::allocator()) {
            let mut value: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(graph,7));
            let mut values: core::Vec<arena i64> = core::result_unwrap::<core::Vec<arena i64>,core::AllocError>(core::vec_new::<arena i64>(core::allocator()));
            let mut added: core::Unit = core::result_unwrap::<core::Unit,core::AllocError>(core::vec_push::<arena i64>(values,value));
            saved = core::Option::Some(values);
        }
        return 0;
    }"#;
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("nested_lifetime.lang"),
        source.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(analysis.result.is_err());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| matches!(d.code.as_str(), "SRC0022" | "SRC0023")),
        "{:?}",
        analysis.diagnostics
    );
}

fn fixtures() -> [(&'static str, i64); 5] {
    [
        (
            include_str!("../../examples/analysis/nested_services.lang"),
            23,
        ),
        (
            include_str!("../../examples/scripts/nested_collections.lang"),
            8,
        ),
        (include_str!("../../examples/scripts/nested_arena.lang"), 4),
        (include_str!("../../examples/scripts/nested_cycle.lang"), 21),
        (
            include_str!("../../examples/scripts/nested_mutation.lang"),
            6,
        ),
    ]
}

#[test]
fn every_nested_source_provider_oom_releases_all_services_and_allows_instance_retry() {
    for (source, expected) in fixtures() {
        let program = checked(source);
        let (mut baseline, trace) = allocator_with_limits(Default::default());
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut baseline),
            Control::default(),
        )
        .unwrap();
        assert_eq!(result.value, expected);
        let requests = trace.borrow().requests.len();
        assert_all_released(&baseline, &trace.borrow());
        for index in 1..=requests {
            let (mut allocator, trace) = allocator_with_limits(Default::default());
            trace.borrow_mut().fail_at = index;
            let mut instance = c::Instance::default();
            let outcome = vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                Control::default(),
            );
            match outcome {
                Ok(value) => assert_eq!(value.value, expected),
                Err(error) => assert!(
                    matches!(
                        error.reason,
                        vm::Reason::Preparation(_)
                            | vm::Reason::Execution(c::FailureKind::Execution(
                                c::ExecutionFailure::AllocationLimit | c::ExecutionFailure::Panic
                            ))
                    ),
                    "OOM {index}: {error:?}"
                ),
            }
            assert_all_released(&allocator, &trace.borrow());
            assert!(!instance.is_poisoned(), "OOM {index}");
            trace.borrow_mut().fail_at = 0;
            let retried = vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                Control::default(),
            )
            .unwrap();
            assert_eq!(retried.value, expected);
            assert_all_released(&allocator, &trace.borrow());
        }
    }
}

#[test]
fn every_nested_source_fuel_prefix_ends_parent_reads_and_retires_all_descendants() {
    for (source, expected) in fixtures() {
        let program = checked(source);
        let (mut allocator, trace) = allocator_with_limits(Default::default());
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            Control::default(),
        )
        .unwrap();
        assert_eq!(result.value, expected);
        let fuel = result.usage.fuel;
        assert_all_released(&allocator, &trace.borrow());
        for limit in 0..=fuel {
            let (mut allocator, trace) = allocator_with_limits(Default::default());
            let mut instance = c::Instance::default();
            let result = vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                Control::with_limits(Limits {
                    fuel: Some(limit),
                    ..Default::default()
                }),
            );
            if limit == fuel {
                assert_eq!(result.unwrap().value, expected);
            } else {
                let error = result.err().unwrap();
                assert!(
                    matches!(
                        error.reason,
                        vm::Reason::Execution(c::FailureKind::Execution(
                            c::ExecutionFailure::FuelLimit
                        ))
                    ),
                    "{error:?}"
                );
                assert_eq!(error.usage.fuel, limit);
            }
            assert_all_released(&allocator, &trace.borrow());
            assert!(!instance.is_poisoned());
        }
    }
}
