use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit::ownership::{core::Operation, failure::ExecutionFailure};
use lang_runtime::{allocation as a, cleanup as c};
use lang_semantics::analysis::{OwnershipProgram, Program};
use lang_vm::{
    InterruptionFlag,
    unit::{Control, Limits, owned as vm},
};
use std::{cell::RefCell, path::Path, rc::Rc, time::Instant};

const SOURCE: &str = include_str!("../examples/scripts/bytes.lang");
const OWNED_SOURCE: &str = include_str!("../examples/scripts/owned_bytes.lang");
const UTF8_SOURCE: &str = include_str!("../examples/scripts/utf8.lang");
const MUTATION_SOURCE: &str = include_str!("../examples/analysis/extraction.lang");
const TEXT_SOURCE: &str = include_str!("../examples/scripts/text.lang");
fn checked(source: &str) -> Box<OwnershipProgram> {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("owned.lang"),
        source.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let Program::Ownership(program) = analysis.result.unwrap() else {
        panic!("ownership source")
    };
    program
}
#[derive(Default)]
struct Trace {
    requests: Vec<a::Request>,
    released: Vec<a::Request>,
    deny_buffer: bool,
    deny_growth: bool,
    fail_at: usize,
    interrupt_growth: Option<Rc<InterruptionFlag>>,
    interrupt_release: Option<Rc<InterruptionFlag>>,
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<a::Storage> {
        let mut trace = self.0.borrow_mut();
        trace.requests.push(request);
        if (trace.deny_buffer && request.bytes() == 24)
            || (trace.deny_growth && request.bytes() == 25)
            || trace.fail_at == trace.requests.len()
        {
            None
        } else {
            if request.bytes() == 25
                && let Some(flag) = &trace.interrupt_growth
            {
                flag.interrupt();
            }
            a::Storage::try_new(request).ok()
        }
    }
    fn deallocate(&mut self, _: a::Storage, request: a::Request) {
        let mut trace = self.0.borrow_mut();
        trace.released.push(request);
        if request.bytes() == 25
            && let Some(flag) = &trace.interrupt_release
        {
            flag.interrupt();
        }
    }
}
fn allocator() -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    allocator_with_limits(a::Limits {
        live_allocations: 128,
        ..Default::default()
    })
}
fn allocator_with_limits(limits: a::Limits) -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let allocator = a::Allocator::new(Provider(Rc::clone(&trace)), limits).unwrap();
    (allocator, trace)
}
fn text_allocator() -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    allocator_with_limits(a::Limits {
        live_allocations: 256,
        ..Default::default()
    })
}
fn control() -> Control<'static> {
    Control::with_limits(Limits::default())
}

fn bytes_program(pushes: &[i64], tail: &str) -> String {
    let mut source = "fn main() -> i64 { let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));".to_owned();
    for (i, byte) in pushes.iter().enumerate() {
        source.push_str(&format!("let mut p{i}: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(bytes, {byte}));"));
    }
    source.push_str(tail);
    source.push('}');
    source
}
const INVALID_UTF8: &str = "match core::text_from_utf8(bytes) { core::Result::Ok(_) => { return -1; }, core::Result::Err(core::Utf8Error::Invalid(offset)) => { return offset; }, }";

fn text_prefix(name: &str, data: &[i64]) -> String {
    let mut source = format!(
        "let mut b_{name}: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));"
    );
    for (i, byte) in data.iter().enumerate() {
        source.push_str(&format!("let mut p_{name}_{i}: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(b_{name}, {byte}));"));
    }
    source.push_str(&format!("let mut {name}: core::Text = core::result_unwrap::<core::Text, core::Utf8Error>(core::text_from_utf8(b_{name}));"));
    source
}

#[test]
fn text_operations_have_exact_allocations_and_preserve_independent_sources() {
    let program = checked(TEXT_SOURCE);
    let typed = program.mir().activity().moved().typed();
    let layouts = lang_runtime::layout::Layouts::compile(
        typed.places().declarations().types(),
        Default::default(),
    )
    .unwrap();
    let plan = lang_runtime::layout::FramePlan::compile(
        &layouts,
        typed,
        lang_mir::unit::FunctionId(0),
        Default::default(),
        usize::MAX,
    )
    .unwrap();
    let admitted: Vec<_> = plan
        .requests()
        .iter()
        .copied()
        .filter(|r| r.bytes() != 0)
        .collect();
    let (mut allocator, trace) = text_allocator();
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 2);
    let trace = trace.borrow();
    assert_eq!(&trace.requests[..admitted.len()], admitted.as_slice());
    assert_eq!(
        trace.requests[admitted.len()..]
            .iter()
            .map(|r| r.bytes())
            .collect::<Vec<_>>(),
        [24, 25, 25, 25, 26, 26]
    );
    assert_eq!(
        outcome.allocations.requests as usize,
        plan.requests().len() + 6
    );
    assert_eq!(outcome.allocations.live_allocations, 0);
    assert_eq!(trace.requests.len(), trace.released.len());
    assert!(!instance.is_poisoned());
}

#[test]
fn text_equality_is_exact_utf8_bytes_and_shared_aliases_can_be_cloned_and_concatenated() {
    for (left, right, expected) in [
        (&[][..], &[][..], 1),
        (&[65, 0, 66][..], &[65, 0, 66][..], 1),
        (&[65, 0, 66][..], &[65][..], 0),
        (&[195, 169][..], &[101, 204, 129][..], 0),
    ] {
        let source = format!(
            "fn main() -> i64 {{ {} {} if core::text_equal(left, right) {{ return 1; }} else {{ return 0; }} }}",
            text_prefix("left", left),
            text_prefix("right", right)
        );
        let program = checked(&source);
        let (mut allocator, trace) = text_allocator();
        assert_eq!(
            vm::execute_entry(
                program.failure_flow(),
                &mut c::Instance::default(),
                Some(&mut allocator),
                control()
            )
            .unwrap()
            .value,
            expected
        );
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    }
    let source = format!(
        "fn main() -> i64 {{ {} let mut view: &core::Text = &text; let mut copied: core::Text = core::result_unwrap::<core::Text, core::AllocError>(core::text_clone(core::allocator(), view)); let mut joined: core::Text = core::result_unwrap::<core::Text, core::AllocError>(core::text_concat(core::allocator(), view, view)); if core::text_equal(view, copied) {{ return core::text_len(joined); }} else {{ return -1; }} }}",
        text_prefix("text", &[65, 0, 66])
    );
    let program = checked(&source);
    let (mut allocator, trace) = text_allocator();
    assert_eq!(
        vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            control()
        )
        .unwrap()
        .value,
        6
    );
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
}

#[test]
fn every_text_script_allocation_failure_keeps_original_panic_and_releases_prior_owners() {
    let program = checked(TEXT_SOURCE);
    let (mut baseline, trace) = text_allocator();
    vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
    )
    .unwrap();
    let requests = trace.borrow().requests.len();
    for failed in requests - 5..=requests {
        let (mut allocator, trace) = text_allocator();
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
        assert_eq!(error.panic_code, Some(-2));
        assert!(matches!(
            error.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::Panic))
        ));
        assert!(error.report.unwrap().secondary().is_empty());
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(
            trace.borrow().requests.len(),
            trace.borrow().released.len() + 1
        );
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn buffer_clone_concat_and_conversion_oom_or_quota_errors_leave_inputs_readable() {
    for (expression, result, read, text_input) in [
        (
            "core::bytes_clone(core::allocator(), bytes)",
            "core::Bytes",
            "core::bytes_len(bytes)",
            false,
        ),
        (
            "core::text_clone(core::allocator(), text)",
            "core::Text",
            "core::text_len(text)",
            true,
        ),
        (
            "core::text_concat(core::allocator(), text, text)",
            "core::Text",
            "core::text_len(text)",
            true,
        ),
        (
            "core::text_bytes(core::allocator(), text)",
            "core::Bytes",
            "core::text_len(text)",
            true,
        ),
    ] {
        for quota in [false, true] {
            let error = if quota { "Limit" } else { "OutOfMemory" };
            let prefix = if text_input {
                "let mut text: core::Text = core::result_unwrap::<core::Text, core::Utf8Error>(core::text_from_utf8(bytes));"
            } else {
                ""
            };
            let source = bytes_program(
                &[65],
                &format!(
                    "{prefix} let mut made: core::Result<{result}, core::AllocError> = {expression}; match made {{ core::Result::Ok(_) => {{ return -1; }}, core::Result::Err(core::AllocError::{error}) => {{ return {read}; }}, core::Result::Err(_) => {{ return -2; }}, }}"
                ),
            );
            let program = checked(&source);
            let typed = program.mir().activity().moved().typed();
            let layouts = lang_runtime::layout::Layouts::compile(
                typed.places().declarations().types(),
                Default::default(),
            )
            .unwrap();
            let plan = lang_runtime::layout::FramePlan::compile(
                &layouts,
                typed,
                lang_mir::unit::FunctionId(0),
                Default::default(),
                usize::MAX,
            )
            .unwrap();
            let admitted = plan.requests().iter().filter(|r| r.bytes() != 0).count();
            let (mut allocator, trace) = allocator_with_limits(a::Limits {
                live_allocations: 256,
                cumulative_bytes: if quota {
                    plan.requests().iter().map(|r| r.bytes()).sum::<u64>() + 49
                } else {
                    a::Limits::default().cumulative_bytes
                },
                ..Default::default()
            });
            if !quota {
                trace.borrow_mut().fail_at = admitted + 3;
            }
            let mut instance = c::Instance::default();
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
            assert_eq!(allocator.stats().live_allocations, 0);
            assert_eq!(
                trace.borrow().requests.len(),
                trace.borrow().released.len() + usize::from(!quota)
            );
            assert!(!instance.is_poisoned());
        }
    }
}

#[test]
fn repeated_text_operations_and_shared_loans_retire_each_iteration_without_leaks() {
    let source = format!(
        "fn main() -> i64 {{ let mut running: bool = true; while running {{ {} let mut view: &core::Text = &text; let mut copy: core::Text = core::result_unwrap::<core::Text, core::AllocError>(core::text_clone(core::allocator(), view)); let mut same: bool = core::text_equal(view, copy); let mut joined: core::Text = core::result_unwrap::<core::Text, core::AllocError>(core::text_concat(core::allocator(), view, copy)); let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::text_bytes(core::allocator(), joined)); let mut copied: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_clone(core::allocator(), bytes)); }} return 0; }}",
        text_prefix("text", &[65])
    );
    let program = checked(&source);
    let (mut allocator, trace) = text_allocator();
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::with_limits(Limits {
            fuel: Some(300),
            ..Default::default()
        }),
    )
    .err()
    .unwrap();
    assert!(
        matches!(
            error.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
        ),
        "{error:?}"
    );
    assert!(
        trace
            .borrow()
            .requests
            .iter()
            .filter(|r| r.bytes() == 26)
            .count()
            > 2
    );
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    assert!(!instance.is_poisoned());
}

#[test]
fn mutation_and_utf8_reuse_the_grown_buffer_without_conversion_allocation() {
    let program = checked(UTF8_SOURCE);
    let typed = program.mir().activity().moved().typed();
    let layouts = lang_runtime::layout::Layouts::compile(
        typed.places().declarations().types(),
        Default::default(),
    )
    .unwrap();
    let plan = lang_runtime::layout::FramePlan::compile(
        &layouts,
        typed,
        lang_mir::unit::FunctionId(0),
        Default::default(),
        usize::MAX,
    )
    .unwrap();
    let admitted: Vec<_> = plan
        .requests()
        .iter()
        .copied()
        .filter(|r| r.bytes() != 0)
        .collect();
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 3);
    let trace = trace.borrow();
    assert_eq!(&trace.requests[..admitted.len()], admitted.as_slice());
    assert_eq!(
        trace.requests[admitted.len()..]
            .iter()
            .map(|r| r.bytes())
            .collect::<Vec<_>>(),
        [24, 25, 26, 28]
    );
    assert_eq!(
        outcome.allocations.requests as usize,
        plan.requests().len() + 4
    );
    assert_eq!(trace.requests.len(), trace.released.len());
    assert_eq!(outcome.allocations.live_allocations, 0);
    assert!(!instance.is_poisoned());
}

#[test]
fn bytes_domain_errors_preserve_storage_and_follow_index_before_byte_validation() {
    for (pushes, tail, expected) in [
        (
            &[][..],
            "match core::bytes_get(bytes, -1) { core::Result::Ok(_) => { return -1; }, core::Result::Err(core::BoundsError::OutOfBounds) => { return core::bytes_len(bytes); }, }",
            0,
        ),
        (
            &[65][..],
            "match core::bytes_get(bytes, 9223372036854775807) { core::Result::Ok(_) => { return -1; }, core::Result::Err(core::BoundsError::OutOfBounds) => { return core::bytes_len(bytes); }, }",
            1,
        ),
        (
            &[][..],
            "match core::bytes_push(bytes, -1) { core::Result::Ok(_) => { return -1; }, core::Result::Err(core::BufferError::InvalidByte) => { return core::bytes_len(bytes); }, core::Result::Err(_) => { return -2; }, }",
            0,
        ),
        (
            &[][..],
            "match core::bytes_push(bytes, 256) { core::Result::Ok(_) => { return -1; }, core::Result::Err(core::BufferError::InvalidByte) => { return core::bytes_len(bytes); }, core::Result::Err(_) => { return -2; }, }",
            0,
        ),
        (
            &[][..],
            "match core::bytes_set(bytes, 0, 256) { core::Result::Ok(_) => { return -1; }, core::Result::Err(core::BufferError::Bounds) => { return core::bytes_len(bytes); }, core::Result::Err(_) => { return -2; }, }",
            0,
        ),
        (
            &[65][..],
            "match core::bytes_set(bytes, 0, 256) { core::Result::Ok(_) => { return -1; }, core::Result::Err(core::BufferError::InvalidByte) => { return core::result_unwrap::<i64, core::BoundsError>(core::bytes_get(bytes, 0)); }, core::Result::Err(_) => { return -2; }, }",
            65,
        ),
        (
            &[65][..],
            "match core::bytes_set(bytes, -1, 0) { core::Result::Ok(_) => { return -1; }, core::Result::Err(core::BufferError::Bounds) => { return core::result_unwrap::<i64, core::BoundsError>(core::bytes_get(bytes, 0)); }, core::Result::Err(_) => { return -2; }, }",
            65,
        ),
        (
            &[65][..],
            "let mut changed: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_set(bytes, 0, 255)); return core::result_unwrap::<i64, core::BoundsError>(core::bytes_get(bytes, 0));",
            255,
        ),
    ] {
        let program = checked(&bytes_program(pushes, tail));
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        assert_eq!(
            vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control()
            )
            .unwrap()
            .value,
            expected
        );
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn grow_oom_and_allocation_limits_are_typed_and_preserve_the_original_owner() {
    for limit in [false, true] {
        let error = if limit { "Limit" } else { "OutOfMemory" };
        let source = bytes_program(
            &[],
            &format!(
                "match core::bytes_push(bytes, 65) {{ core::Result::Ok(_) => {{ return -1; }}, core::Result::Err(core::BufferError::Allocation(core::AllocError::{error})) => {{ return core::bytes_len(bytes); }}, core::Result::Err(_) => {{ return -2; }}, }}"
            ),
        );
        let program = checked(&source);
        let typed = program.mir().activity().moved().typed();
        let layouts = lang_runtime::layout::Layouts::compile(
            typed.places().declarations().types(),
            Default::default(),
        )
        .unwrap();
        let plan = lang_runtime::layout::FramePlan::compile(
            &layouts,
            typed,
            lang_mir::unit::FunctionId(0),
            Default::default(),
            usize::MAX,
        )
        .unwrap();
        let budget = plan.requests().iter().map(|r| r.bytes()).sum::<u64>() + 24;
        let trace = Rc::new(RefCell::new(Trace {
            deny_growth: !limit,
            ..Default::default()
        }));
        let mut allocator = a::Allocator::new(
            Provider(Rc::clone(&trace)),
            a::Limits {
                cumulative_bytes: if limit {
                    budget
                } else {
                    a::Limits::default().cumulative_bytes
                },
                ..Default::default()
            },
        )
        .unwrap();
        let mut instance = c::Instance::default();
        assert_eq!(
            vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control()
            )
            .unwrap()
            .value,
            0
        );
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(
            trace.borrow().requests.len(),
            trace.borrow().released.len() + usize::from(!limit)
        );
        assert_eq!(
            trace
                .borrow()
                .requests
                .iter()
                .filter(|r| r.bytes() == 25)
                .count(),
            usize::from(!limit)
        );
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn valid_utf8_preserves_empty_and_embedded_zero_byte_lengths() {
    for data in [&[][..], &[0][..], &[65, 0, 66][..]] {
        let source = bytes_program(
            data,
            "let mut text: core::Text = core::result_unwrap::<core::Text, core::Utf8Error>(core::text_from_utf8(bytes)); return core::text_len(text);",
        );
        let program = checked(&source);
        let (mut allocator, trace) = allocator();
        assert_eq!(
            vm::execute_entry(
                program.failure_flow(),
                &mut c::Instance::default(),
                Some(&mut allocator),
                control()
            )
            .unwrap()
            .value,
            data.len() as i64
        );
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    }
}

#[test]
fn conversion_loops_recycle_both_success_and_error_results_before_fuel_exit() {
    for byte in [None, Some(65), Some(255)] {
        let push = byte.map_or(String::new(), |value| format!("let mut pushed: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(bytes, {value}));"));
        let source = format!(
            "fn main() -> i64 {{ let mut running: bool = true; while running {{ let mut copied: core::Option<i64> = core::Option::Some(0); let mut seen: i64 = core::option_unwrap::<i64>(copied); let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); {push} let mut converted: core::Result<core::Text, core::Utf8Error> = core::text_from_utf8(bytes); }} return 0; }}"
        );
        let program = checked(&source);
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(Limits {
                fuel: Some(200),
                ..Default::default()
            }),
        )
        .err()
        .unwrap();
        assert!(
            matches!(
                error.reason,
                vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
            ),
            "byte={byte:?}: {error:?}"
        );
        assert!(
            trace
                .borrow()
                .requests
                .iter()
                .filter(|r| r.bytes() == 24)
                .count()
                > 1
        );
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn utf8_errors_keep_the_first_invalid_offset_and_consume_the_source_buffer() {
    for (data, expected) in [(&[65, 255][..], 1), (&[227, 129][..], 0), (&[128][..], 0)] {
        let program = checked(&bytes_program(data, INVALID_UTF8));
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        assert_eq!(
            vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control()
            )
            .unwrap()
            .value,
            expected
        );
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn interruption_after_growth_or_conversion_release_cleans_current_handles() {
    for conversion in [false, true] {
        let source = if conversion {
            bytes_program(&[255], INVALID_UTF8)
        } else {
            bytes_program(&[65], "return core::bytes_len(bytes);")
        };
        let program = checked(&source);
        let flag = Rc::new(InterruptionFlag::new());
        let (mut allocator, trace) = allocator();
        if conversion {
            trace.borrow_mut().interrupt_release = Some(Rc::clone(&flag));
        } else {
            trace.borrow_mut().interrupt_growth = Some(Rc::clone(&flag));
        }
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control().and_interruption(&flag),
        )
        .err()
        .unwrap();
        assert!(flag.is_interrupted());
        assert!(matches!(
            error.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::Interrupted))
        ));
        assert!(error.report.unwrap().secondary().is_empty());
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn all_three_extractions_transfer_one_source_buffer_without_new_allocation() {
    let program = checked(OWNED_SOURCE);
    let typed = program.mir().activity().moved().typed();
    let layouts = lang_runtime::layout::Layouts::compile(
        typed.places().declarations().types(),
        Default::default(),
    )
    .unwrap();
    let plan = lang_runtime::layout::FramePlan::compile(
        &layouts,
        typed,
        lang_mir::unit::FunctionId(0),
        Default::default(),
        usize::MAX,
    )
    .unwrap();
    let admitted: Vec<_> = plan
        .requests()
        .iter()
        .copied()
        .filter(|r| r.bytes() != 0)
        .collect();
    for deny_buffer in [false, true] {
        let (mut allocator, trace) = allocator();
        trace.borrow_mut().deny_buffer = deny_buffer;
        let mut instance = c::Instance::default();
        let outcome = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .unwrap();
        assert_eq!(outcome.value, if deny_buffer { -1 } else { 0 });
        let trace = trace.borrow();
        assert_eq!(&trace.requests[..admitted.len()], admitted.as_slice());
        assert_eq!(trace.requests.len(), admitted.len() + 1);
        assert_eq!(trace.requests.last().unwrap().bytes(), 24);
        assert_eq!(
            outcome.allocations.requests as usize,
            plan.requests().len() + 1
        );
        assert_eq!(
            trace.requests.len(),
            trace.released.len() + usize::from(deny_buffer)
        );
        assert_eq!(outcome.allocations.live_allocations, 0);
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn extraction_frame_oom_rolls_back_every_admission_prefix_before_guest_entry() {
    let program = checked(OWNED_SOURCE);
    let (mut baseline, trace) = allocator();
    vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
    )
    .unwrap();
    let requests = trace.borrow().requests.len();
    for fail_at in 1..=requests {
        let (mut allocator, trace) = allocator();
        trace.borrow_mut().fail_at = fail_at;
        let mut instance = c::Instance::default();
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        );
        if fail_at == requests {
            assert_eq!(result.unwrap().value, -1);
        } else {
            let error = result.err().unwrap();
            assert!(matches!(error.reason, vm::Reason::Preparation(_)));
            assert_eq!(error.usage.fuel, 0);
        }
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(
            trace.borrow().requests.len(),
            trace.borrow().released.len() + 1
        );
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn repeated_sum_construction_and_extraction_recycle_admitted_cells_until_fuel_exit() {
    let program = checked(
        "fn main() -> i64 { let mut running: bool = true; while running { let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(made); let mut wrapped: core::Option<core::Bytes> = core::Option::Some(bytes); let mut taken: core::Bytes = core::option_unwrap::<core::Bytes>(wrapped); } return 0; }",
    );
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::with_limits(Limits {
            fuel: Some(200),
            ..Default::default()
        }),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
    ));
    assert_eq!(error.panic_code, None);
    assert!(
        trace
            .borrow()
            .requests
            .iter()
            .filter(|r| r.bytes() == 24)
            .count()
            > 1
    );
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(!instance.is_poisoned());
}

#[test]
fn copy_sum_construction_and_extraction_preserve_source_reuse() {
    let program = checked(
        "fn main() -> i64 { let mut optional: core::Option<i64> = core::Option::Some(7); let mut a: i64 = core::option_unwrap::<i64>(optional); let mut b: i64 = core::option_unwrap::<i64>(optional); let mut ok: core::Result<i64, i64> = core::Result::Ok(a + b); let mut value: i64 = core::result_unwrap::<i64, i64>(ok); let mut err: core::Result<i64, i64> = core::Result::Err(3); return value + core::result_unwrap_err::<i64, i64>(err); }",
    );
    let (mut allocator, _) = allocator();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 17);
    assert_eq!(outcome.allocations.live_allocations, 0);
}

#[test]
fn wrong_cases_keep_panic_codes_and_spans_and_clean_owning_payloads_without_poisoning() {
    for (body, operation, code) in [
        (
            "let mut missing: core::Option<core::Bytes> = core::Option::None; let mut bytes: core::Bytes = core::option_unwrap::<core::Bytes>(missing); return 0;",
            Operation::OptionUnwrap,
            -1,
        ),
        (
            "let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(made); let mut err: core::Result<i64, core::Bytes> = core::Result::Err(bytes); return core::result_unwrap::<i64, core::Bytes>(err);",
            Operation::ResultUnwrap,
            -2,
        ),
        (
            "let mut err: core::AllocError = core::result_unwrap_err::<core::Bytes, core::AllocError>(made); return 0;",
            Operation::ResultUnwrapErr,
            -3,
        ),
    ] {
        let source = format!(
            "fn main() -> i64 {{ let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); {body} }}"
        );
        let program = checked(&source);
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        assert_eq!(error.panic_code, Some(code));
        assert!(matches!(
            error.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::Panic))
        ));
        let span = error.fault.unwrap();
        assert!(
            source[span.start as usize..span.end as usize]
                .starts_with(&format!("core::{}::<", operation.name()))
        );
        assert_eq!(
            error.report.as_ref().unwrap().primary().unwrap().span,
            Some(span)
        );
        assert!(error.report.as_ref().unwrap().secondary().is_empty());
        assert!(!instance.is_poisoned());
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert_eq!(
            trace
                .borrow()
                .released
                .iter()
                .filter(|r| r.bytes() == 24)
                .count(),
            1
        );
        let again = checked(SOURCE);
        assert_eq!(
            vm::execute_entry(
                again.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control()
            )
            .unwrap()
            .value,
            0
        );
    }
}

#[test]
fn frame_admission_oom_prevents_entry_and_allows_a_later_retry() {
    let program = checked(SOURCE);
    let (mut allocator, trace) = allocator();
    trace.borrow_mut().fail_at = 1;
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        vm::Reason::Preparation(lang_runtime::layout::Error::Allocation(
            a::Error::OutOfMemory
        ))
    ));
    assert_eq!(error.usage.fuel, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(!instance.is_poisoned());
    assert_eq!(trace.borrow().requests.len(), 1);
    let retried = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(retried.value, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn repeated_bytes_loans_and_moves_reuse_frame_cells_until_fuel_exhaustion() {
    let program = checked(
        "fn main() -> i64 { let mut repeat: bool = true; while repeat { let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); match made { core::Result::Ok(value) => { let mut length: i64 = core::bytes_len(value); }, core::Result::Err(_) => {}, } } return 0; }",
    );
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::with_limits(Limits {
            fuel: Some(200),
            ..Default::default()
        }),
    )
    .err()
    .unwrap();
    assert!(
        matches!(
            error.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
        ),
        "{error:?}"
    );
    assert_eq!(error.usage.fuel, 200);
    assert!(
        trace
            .borrow()
            .requests
            .iter()
            .filter(|r| r.bytes() == 24)
            .count()
            > 1
    );
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    assert!(!instance.is_poisoned());
}

#[test]
fn copy_scalar_reads_and_arithmetic_coexist_with_a_live_owned_result() {
    let program = checked(
        "fn main() -> i64 { let mut baseline: i64 = 7; let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); match made { core::Result::Ok(value) => { let mut length: i64 = core::bytes_len(value); return baseline + length; }, core::Result::Err(_) => { return -1; }, } }",
    );
    let (mut allocator, _) = allocator();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 7);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn bytes_source_uses_real_storage_and_releases_every_region_exactly_once() {
    let program = checked(SOURCE);
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 0);
    assert_eq!(outcome.allocations.live_allocations, 0);
    assert_eq!(outcome.allocations.live_bytes, 0);
    assert!(outcome.usage.fuel > 0);
    assert_eq!(outcome.usage.peak_frames, 1);
    assert_eq!(outcome.usage.host_calls, 0);
    assert!(!instance.is_poisoned());
    let trace = trace.borrow();
    assert_eq!(trace.requests.iter().filter(|r| r.bytes() == 24).count(), 1);
    assert_eq!(trace.released.iter().filter(|r| r.bytes() == 24).count(), 1);
    assert_eq!(trace.requests.len(), trace.released.len());
    assert_eq!(
        outcome.allocations.allocations,
        outcome.allocations.deallocations
    );
}

#[test]
fn source_handles_bytes_oom_as_typed_err_without_allocating_an_error_result() {
    let program = checked(SOURCE);
    let (mut allocator, trace) = allocator();
    trace.borrow_mut().deny_buffer = true;
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, -1);
    assert!(!instance.is_poisoned());
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().live_bytes, 0);
    let trace = trace.borrow();
    assert_eq!(trace.requests.last().unwrap().bytes(), 24);
    assert_eq!(trace.requests.len(), trace.released.len() + 1);
    assert_eq!(
        allocator.stats().cumulative_bytes,
        trace.requests.iter().map(|r| r.bytes()).sum::<u64>()
    );
}

#[test]
fn every_fuel_prefix_cleans_source_owners_and_ends_loans_without_poisoning() {
    for source in [
        SOURCE.to_owned(),
        OWNED_SOURCE.to_owned(),
        UTF8_SOURCE.to_owned(),
        MUTATION_SOURCE.to_owned(),
        TEXT_SOURCE.to_owned(),
        bytes_program(&[65, 255], INVALID_UTF8),
    ] {
        let program = checked(&source);
        let (mut baseline, _) = text_allocator();
        let baseline = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut baseline),
            control(),
        )
        .unwrap();
        let fuel = baseline.usage.fuel;
        for available in 0..=fuel {
            let (mut allocator, trace) = text_allocator();
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
            if available < fuel {
                let error = result.err().unwrap();
                assert!(matches!(
                    error.reason,
                    vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
                ));
                assert_eq!(error.usage.fuel, available);
                let report = error.report.unwrap();
                assert!(report.secondary().is_empty());
                let span = error.fault.unwrap();
                let typed = program.mir().activity().moved().typed();
                let text = &typed.places().declarations().types().table().sources
                    [span.source.0 as usize]
                    .text;
                assert!(text.get(span.start as usize..span.end as usize).is_some());
            } else {
                assert_eq!(result.unwrap().value, baseline.value);
            }
            assert!(!instance.is_poisoned());
            assert_eq!(allocator.stats().live_allocations, 0);
            assert_eq!(allocator.stats().live_bytes, 0);
            assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        }
    }
}

#[test]
fn unsupported_core_operations_are_rejected_before_earlier_source_allocations() {
    for (later, expected, spelling) in [
        (
            "let mut later: core::Result<core::Vec<i64>, core::AllocError> = core::vec_new::<i64>(core::allocator());",
            Some(Operation::VecNew),
            "vec_new",
        ),
        (
            "let mut converted: core::Text = core::result_unwrap::<core::Text, core::Utf8Error>(core::text_from_utf8(core::result_unwrap::<core::Bytes, core::AllocError>(first))); let mut later: core::Result<shared core::Text, core::OwnershipError> = core::shared_new::<core::Text>(core::allocator(), converted);",
            Some(Operation::SharedNew),
            "shared_new",
        ),
        (
            "let mut later: core::Option<core::Vec<i64>> = core::Option::None;",
            None,
            "Option::None",
        ),
    ] {
        let program = checked(&format!(
            "fn main() -> i64 {{ let mut first: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); {later} return 0; }}"
        ));
        let (mut allocator, trace) = allocator();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        assert!(matches!(error.reason, vm::Reason::Unsupported { core } if core == expected));
        assert_eq!(allocator.stats(), a::Stats::default());
        assert!(trace.borrow().requests.is_empty());
        let span = error.fault.unwrap();
        let typed = program.mir().activity().moved().typed();
        let text =
            &typed.places().declarations().types().table().sources[span.source.0 as usize].text;
        assert!(text[span.start as usize..span.end as usize].contains(spelling));
    }
}

#[test]
fn entry_authority_poison_control_and_frame_cell_limits_prevent_callbacks() {
    let program = checked(SOURCE);
    let mut instance = c::Instance::default();
    let missing =
        vm::execute_entry::<Provider>(program.failure_flow(), &mut instance, None, control())
            .err()
            .unwrap();
    assert!(matches!(missing.reason, vm::Reason::MissingAllocator));
    let flag = InterruptionFlag::new();
    flag.interrupt();
    for control in [
        control().and_interruption(&flag),
        control().and_deadline(Instant::now()),
        Control::with_limits(Limits {
            frames: Some(0),
            ..Default::default()
        }),
        Control::with_limits(Limits {
            live_cells: Some(0),
            ..Default::default()
        }),
    ] {
        let (mut allocator, trace) = allocator();
        assert!(
            vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control
            )
            .is_err()
        );
        assert!(trace.borrow().requests.is_empty());
        assert_eq!(allocator.stats(), a::Stats::default());
    }
    let mut poisoned = c::Instance::default();
    let mut frame = poisoned
        .enter(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |_: &mut (), _| Err(1),
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    assert!(frame.initialize(c::Slot(0), (), None).is_ok());
    frame.finish(c::Exit::Return(None));
    let (mut allocator, trace) = allocator();
    let denied = vm::execute_entry(
        program.failure_flow(),
        &mut poisoned,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert!(matches!(denied.reason, vm::Reason::Poisoned));
    assert!(trace.borrow().requests.is_empty());
}
