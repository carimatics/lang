use super::*;
use lang_mir::unit::{self as m};

const CALLS: &str = include_str!("../../examples/scripts/owned_calls.lang");
fn call_allocator() -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    allocator_with_limits(a::Limits {
        live_allocations: 4096,
        ..Default::default()
    })
}
fn plan(program: &OwnershipProgram, name: &str) -> (Vec<a::Request>, usize) {
    let typed = program.mir().activity().moved().typed();
    let layouts = lang_runtime::layout::Layouts::compile(
        typed.places().declarations().types(),
        Default::default(),
    )
    .unwrap();
    let id = typed
        .places()
        .declarations()
        .functions()
        .iter()
        .position(|f| f.name == name)
        .unwrap();
    let plan = lang_runtime::layout::FramePlan::compile(
        &layouts,
        typed,
        m::FunctionId(id as u32),
        Default::default(),
        usize::MAX,
    )
    .unwrap();
    (plan.requests().to_vec(), plan.cleanup_slots() + 1)
}
fn names(program: &OwnershipProgram, error: &vm::Failure) -> Vec<String> {
    let functions = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .functions();
    error
        .stack
        .iter()
        .map(|frame| functions[frame.function.0 as usize].name.clone())
        .collect()
}

#[test]
fn recursive_owned_results_and_caller_loans_execute_with_exact_frame_requests() {
    let program = checked(CALLS);
    let (main, _) = plan(&program, "main");
    let (carry, _) = plan(&program, "carry");
    let (length, _) = plan(&program, "length");
    let (mut allocator, trace) = call_allocator();
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 1);
    assert_eq!(outcome.usage.peak_frames, 5);
    assert_eq!(
        outcome.allocations.requests as usize,
        main.len() + carry.len() * 4 + length.len() + 2
    );
    assert_eq!(
        trace.borrow().requests.len(),
        main.iter()
            .chain(carry.iter().cycle().take(carry.len() * 4))
            .chain(&length)
            .filter(|r| r.bytes() != 0)
            .count()
            + 2
    );
    assert_all_released(&allocator, &trace.borrow());
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    assert!(!instance.is_poisoned());
}

const NESTED: &str = "struct Owned { data: core::Bytes, tag: i64 } fn make(a: &core::Allocator) -> core::Result<Owned, core::AllocError> { let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(a); match made { core::Result::Ok(_) => {}, core::Result::Err(error) => { return core::Result::Err(error); }, } let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(made); return core::Result::Ok(Owned { tag: 7, data: bytes }); } fn relay(value: core::Result<Owned, core::AllocError>) -> core::Result<Owned, core::AllocError> { return value; } fn main() -> i64 { let mut made: core::Result<Owned, core::AllocError> = relay(make(core::allocator())); match made { core::Result::Ok(value) => { return (*value).tag + core::bytes_len((*value).data); }, core::Result::Err(_) => { return -9; }, } }";

#[test]
fn mutual_recursion_reuses_different_activation_shapes_without_losing_owners() {
    let source=CALLS.replace("fn carry(value:","fn left(value:")
        .replace("return carry(value, depth + (-1));","return right(value, depth + (-1));")
        .replace("fn length(","fn right(value: core::Bytes, depth: i64) -> core::Bytes { let mut unused: core::Option<core::Bytes> = core::Option::None; return left(value, depth); } fn length(")
        .replace("carry(bytes, 3)","left(bytes, 3)");
    let program = checked(&source);
    let (mut allocator, trace) = call_allocator();
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 1);
    assert_eq!(outcome.usage.peak_frames, 8);
    assert_all_released(&allocator, &trace.borrow());
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    assert!(!instance.is_poisoned());
}

#[test]
fn an_unentered_large_callee_does_not_charge_the_live_cell_counter() {
    let locals = (0..40)
        .map(|n| format!("let mut x{n}: i64 = {n};"))
        .collect::<String>();
    let source = format!(
        "fn heavy(value: core::Bytes) -> i64 {{ {locals} return core::bytes_len(value); }} fn choose(value: bool) -> bool {{ return value; }} fn main() -> i64 {{ let mut unused: core::Option<core::Bytes> = core::Option::None; let mut selected: bool = choose(false); if selected {{ let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); return heavy(bytes); }} return 0; }}"
    );
    let program = checked(&source);
    let (main, main_cells) = plan(&program, "main");
    let (choose, choose_cells) = plan(&program, "choose");
    let (_, heavy_cells) = plan(&program, "heavy");
    assert!(heavy_cells > main_cells + choose_cells);
    let (mut allocator, trace) = call_allocator();
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::with_limits(Limits {
            live_cells: Some(main_cells + choose_cells),
            ..Default::default()
        }),
    )
    .unwrap();
    assert_eq!(outcome.value, 0);
    assert_eq!(outcome.usage.peak_live_cells, main_cells + choose_cells);
    assert_eq!(
        outcome.allocations.requests as usize,
        main.len() + choose.len()
    );
    assert_all_released(&allocator, &trace.borrow());
    assert!(!instance.is_poisoned());
}

#[test]
fn allocator_loans_and_nested_owned_sum_results_cross_multiple_calls() {
    let program = checked(NESTED);
    for deny in [false, true] {
        let (mut allocator, trace) = call_allocator();
        trace.borrow_mut().deny_buffer = deny;
        let mut instance = c::Instance::default();
        let outcome = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .unwrap();
        assert_eq!(outcome.value, if deny { -9 } else { 7 });
        assert_all_released(&allocator, &trace.borrow());
        assert_eq!(
            trace.borrow().requests.len(),
            trace.borrow().released.len() + usize::from(deny)
        );
        assert!(!instance.is_poisoned());
    }
}

const MUTATE: &str = "struct Bundle { bytes: core::Bytes } fn append(value: &mut Bundle) -> core::Unit { return core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push((*value).bytes,65)); } fn main() -> i64 { let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); let mut bundle: Bundle = Bundle { bytes: bytes }; let mut a: core::Unit = append(bundle); let mut b: core::Unit = append(bundle); return core::bytes_len(bundle.bytes); }";

#[test]
fn exclusive_callee_projections_update_the_caller_owner_before_interruption() {
    let program = checked(MUTATE);
    for interrupt in [false, true] {
        let flag = Rc::new(InterruptionFlag::new());
        let (mut allocator, trace) = call_allocator();
        if interrupt {
            trace.borrow_mut().interrupt_growth = Some(Rc::clone(&flag));
        }
        let mut instance = c::Instance::default();
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control().and_interruption(&flag),
        );
        if interrupt {
            let error = result.err().unwrap();
            assert!(matches!(
                error.reason,
                vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::Interrupted))
            ));
            assert_eq!(names(&program, &error), ["append", "main"]);
            assert!(error.report.unwrap().secondary().is_empty());
        } else {
            assert_eq!(result.unwrap().value, 2);
        }
        assert_all_released(&allocator, &trace.borrow());
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn owned_recursion_uses_bounded_activation_storage_instead_of_rust_recursion() {
    let source = CALLS.replace("carry(bytes, 3)", "carry(bytes, 1024)");
    let program = checked(&source);
    for frames in [Some(1030), None] {
        let (mut allocator, trace) = allocator_with_limits(a::Limits {
            live_allocations: 65536,
            ..Default::default()
        });
        let mut instance = c::Instance::default();
        let outcome = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(Limits {
                frames,
                ..Default::default()
            }),
        )
        .unwrap();
        assert_eq!(outcome.value, 1);
        assert_eq!(outcome.usage.peak_frames, 1026);
        assert_all_released(&allocator, &trace.borrow());
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn copy_record_enum_and_boolean_interfaces_remain_canonical_in_owned_calls() {
    let source = "struct Pair { number: i64, flag: bool } enum Choice { Empty, Full(Pair) } fn forward(value: Choice) -> Choice { return value; } fn accept(value: Pair) -> bool { return value.flag; } fn main() -> i64 { let mut unused: core::Option<core::Bytes> = core::Option::None; let mut pair: Pair = Pair { flag: true, number: 9 }; let mut choice: Choice = forward(Choice::Full(pair)); match choice { Choice::Empty => { return -1; }, Choice::Full(found) => { if accept(found) { return found.number; } else { return -2; } }, } }";
    let program = checked(source);
    let (mut allocator, trace) = call_allocator();
    assert_eq!(
        vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            control()
        )
        .unwrap()
        .value,
        9
    );
    assert_all_released(&allocator, &trace.borrow());
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
}

#[test]
fn call_frame_and_cell_refusal_preserve_caller_owners_and_exclude_unentered_frames() {
    let program = checked(CALLS);
    let (_, main) = plan(&program, "main");
    let (_, carry) = plan(&program, "carry");
    for (limits, expected, depth) in [
        (
            Limits {
                frames: Some(1),
                ..Default::default()
            },
            ExecutionFailure::FrameLimit,
            1,
        ),
        (
            Limits {
                frames: Some(2),
                ..Default::default()
            },
            ExecutionFailure::FrameLimit,
            2,
        ),
        (
            Limits {
                live_cells: Some(main + carry - 1),
                ..Default::default()
            },
            ExecutionFailure::CellLimit,
            1,
        ),
    ] {
        let (mut allocator, trace) = call_allocator();
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(limits),
        )
        .err()
        .unwrap();
        assert_eq!(error.usage.peak_frames, depth);
        assert_eq!(error.stack.len(), depth);
        assert!(
            matches!(error.reason,vm::Reason::Execution(c::FailureKind::Execution(actual)) if actual==expected)
        );
        assert!(error.usage.fuel > 0);
        assert!(error.report.unwrap().secondary().is_empty());
        assert_all_released(&allocator, &trace.borrow());
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert!(!instance.is_poisoned());
    }
    let (mut allocator, trace) = call_allocator();
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::with_limits(Limits {
            live_cells: Some(main - 1),
            ..Default::default()
        }),
    )
    .err()
    .unwrap();
    assert!(matches!(error.reason, vm::Reason::Preparation(_)));
    assert!(trace.borrow().requests.is_empty());
    assert!(!instance.is_poisoned());
}

#[test]
fn nested_guest_panic_keeps_the_original_code_span_and_entered_call_stack() {
    let source = "fn second(value: core::Bytes) -> core::Bytes { let mut none: core::Option<core::Bytes> = core::Option::None; return core::option_unwrap::<core::Bytes>(none); } fn first(value: core::Bytes) -> core::Bytes { return second(value); } fn main() -> i64 { let mut alive: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); let mut sent: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); let mut result: core::Bytes = first(sent); return core::bytes_len(result); }";
    let program = checked(source);
    let (mut allocator, trace) = call_allocator();
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert_eq!(error.panic_code, Some(-1));
    assert_eq!(names(&program, &error), ["second", "first", "main"]);
    assert_eq!(error.function, Some(error.stack[0].function));
    let span = error.fault.unwrap();
    assert!(source[span.start as usize..span.end as usize].contains("option_unwrap"));
    assert!(error.report.unwrap().secondary().is_empty());
    assert_all_released(&allocator, &trace.borrow());
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    assert!(!instance.is_poisoned());
}

#[test]
fn every_call_allocation_failure_unwinds_entered_activations_and_allows_reuse() {
    let program = checked(CALLS);
    let (mut baseline, trace) = call_allocator();
    vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
    )
    .unwrap();
    let requests = trace.borrow().requests.len();
    let root = plan(&program, "main")
        .0
        .iter()
        .filter(|r| r.bytes() != 0)
        .count();
    for failed in 1..=requests {
        let (mut allocator, trace) = call_allocator();
        trace.borrow_mut().fail_at = failed;
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        if failed <= root {
            assert!(matches!(error.reason, vm::Reason::Preparation(_)));
        } else {
            assert!(matches!(
                error.reason,
                vm::Reason::Execution(c::FailureKind::Execution(
                    ExecutionFailure::AllocationLimit | ExecutionFailure::Panic
                ))
            ));
        }
        assert!(error.report.unwrap().secondary().is_empty());
        assert_all_released(&allocator, &trace.borrow());
        assert_eq!(
            trace.borrow().requests.len(),
            trace.borrow().released.len() + 1
        );
        assert!(!instance.is_poisoned());
        trace.borrow_mut().fail_at = 0;
        assert_eq!(
            vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control()
            )
            .unwrap()
            .value,
            1
        );
        assert_all_released(&allocator, &trace.borrow());
    }
}

#[test]
fn every_recursive_call_fuel_prefix_retires_parameters_results_and_loans() {
    let program = checked(CALLS);
    let (mut baseline, _) = call_allocator();
    let fuel = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
    )
    .unwrap()
    .usage
    .fuel;
    for available in 0..=fuel {
        let (mut allocator, trace) = call_allocator();
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
            assert_eq!(result.unwrap().value, 1);
        } else {
            let error = result.err().unwrap();
            assert_eq!(error.usage.fuel, available);
            assert!(matches!(
                error.reason,
                vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
            ));
            assert!(error.report.unwrap().secondary().is_empty());
        }
        assert_all_released(&allocator, &trace.borrow());
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn reachable_callee_preflight_rejects_unsupported_effects_but_unused_functions_do_not() {
    let prefix = "fn length(value: &core::Bytes) -> i64 { return core::bytes_len(value); } fn bad() -> core::Map<i64, core::Vec<i64>> { return core::result_unwrap::<core::Map<i64, core::Vec<i64>>, core::AllocError>(core::map_new::<i64, core::Vec<i64>>(core::allocator())); } fn main() -> i64 { let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); let mut n: i64 = length(bytes);";
    for invoked in [false, true] {
        let source = format!(
            "{prefix} {} return n; }}",
            if invoked {
                "let mut bad_value: core::Map<i64, core::Vec<i64>> = bad();"
            } else {
                ""
            }
        );
        let program = checked(&source);
        let (mut allocator, trace) = call_allocator();
        let mut instance = c::Instance::default();
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        );
        if invoked {
            let error = result.err().unwrap();
            assert!(matches!(
                error.reason,
                vm::Reason::Unsupported {
                    core: Some(Operation::MapNew)
                }
            ));
            assert_eq!(error.usage.fuel, 0);
            assert!(trace.borrow().requests.is_empty());
        } else {
            assert_eq!(result.unwrap().value, 0);
            assert_all_released(&allocator, &trace.borrow());
        }
        assert!(!instance.is_poisoned());
    }
}
