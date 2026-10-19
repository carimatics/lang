use super::*;
use lang_mir::unit as m;

const SOURCE: &str = include_str!("../../examples/analysis/repl_preview.lang");
fn function(program: &OwnershipProgram, name: &str) -> m::FunctionId {
    m::FunctionId(
        program
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .functions()
            .iter()
            .position(|function| function.name == name)
            .unwrap() as u32,
    )
}
fn allocator() -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    allocator_with_limits(a::Limits {
        live_allocations: 4096,
        ..Default::default()
    })
}

#[test]
fn named_owned_source_results_render_and_release_before_preview_publication() {
    let program = checked(SOURCE);
    for (name, expected) in [
        ("fresh_bytes", "core::Bytes []"),
        ("fresh_text", "core::Text \"\""),
        (
            "preview_message",
            "core::Result::Ok(PreviewMessage { bytes: core::Bytes [], text: core::Text \"\", count: 7 })",
        ),
    ] {
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let output = vm::execute_preview(
            program.failure_flow(),
            function(&program, name),
            &mut instance,
            Some(&mut allocator),
            control(),
            65536,
        )
        .unwrap();
        assert_eq!(output.value, expected);
        assert_all_released(&allocator, &trace.borrow());
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert!(!instance.is_poisoned());
    }
}

fn byte_source(data: &[u8], text: bool) -> String {
    let mut code = String::from("fn value() -> core::");
    code.push_str(if text { "Text" } else { "Bytes" });
    code.push_str(" { let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));");
    for (index, byte) in data.iter().enumerate() {
        code.push_str(&format!("let mut pushed{index}: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(&mut bytes,{byte}));"));
    }
    code.push_str(if text {
        "return core::result_unwrap::<core::Text, core::Utf8Error>(core::text_from_utf8(bytes)); }"
    } else {
        "return bytes; }"
    });
    code
}
fn preview_value(source: &str, name: &str, limit: usize) -> String {
    let program = checked(source);
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let output = vm::execute_preview(
        program.failure_flow(),
        function(&program, name),
        &mut instance,
        Some(&mut allocator),
        control(),
        limit,
    )
    .unwrap_or_else(|error| panic!("source: {source}\n{error:?}"));
    assert_all_released(&allocator, &trace.borrow());
    assert!(!instance.is_poisoned());
    output.value
}

#[test]
fn all_bytes_and_exact_ascii_text_escapes_are_canonical_without_retaining_buffers() {
    let source = "fn value() -> core::Bytes { let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); let mut i: i64 = 0; while i < 256 { let mut pushed: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(&mut bytes,i)); i = i + 1; } return bytes; }";
    let expected = format!(
        "core::Bytes [{}]",
        (0..=255)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert_eq!(preview_value(source, "value", 65536), expected);
    for (text, expected) in [
        ("A\"\\\n\r\t\0", "core::Text \"A\\\"\\\\\\n\\r\\t\\u{0}\""),
        ("あ", "core::Text \"\\u{3042}\""),
        ("\u{10ffff}", "core::Text \"\\u{10ffff}\""),
        ("é", "core::Text \"\\u{e9}\""),
        ("e\u{301}", "core::Text \"e\\u{301}\""),
        (
            "\u{7f}\u{85}\u{202e}",
            "core::Text \"\\u{7f}\\u{85}\\u{202e}\"",
        ),
    ] {
        assert_eq!(
            preview_value(&byte_source(text.as_bytes(), true), "value", 65536),
            expected
        );
    }
}

#[test]
fn scalar_nominal_and_finite_core_previews_use_the_accepted_source_names() {
    let prefix = "fn unused(value: core::Bytes) -> core::Bytes { return value; } ";
    for (ty, value, expected) in [
        ("i64", "-9223372036854775808", "-9223372036854775808"),
        ("bool", "false", "false"),
        ("core::Unit", "core::Unit {}", "core::Unit {}"),
        (
            "core::AllocError",
            "core::AllocError::OutOfMemory",
            "core::AllocError::OutOfMemory",
        ),
        (
            "core::BoundsError",
            "core::BoundsError::OutOfBounds",
            "core::BoundsError::OutOfBounds",
        ),
        (
            "core::BufferError",
            "core::BufferError::Allocation(core::AllocError::Overflow)",
            "core::BufferError::Allocation(core::AllocError::Overflow)",
        ),
        (
            "core::Utf8Error",
            "core::Utf8Error::Invalid(-7)",
            "core::Utf8Error::Invalid(-7)",
        ),
        (
            "core::OwnershipError",
            "core::OwnershipError::StrongCycle",
            "core::OwnershipError::StrongCycle",
        ),
        (
            "core::ArenaError",
            "core::ArenaError::Allocation(core::AllocError::Limit)",
            "core::ArenaError::Allocation(core::AllocError::Limit)",
        ),
        (
            "core::Option<i64>",
            "core::Option::None",
            "core::Option::None",
        ),
        (
            "core::Result<core::Unit, core::AllocError>",
            "core::Result::Err(core::AllocError::Limit)",
            "core::Result::Err(core::AllocError::Limit)",
        ),
    ] {
        assert_eq!(
            preview_value(
                &format!("{prefix} fn value() -> {ty} {{ return {value}; }}"),
                "value",
                65536
            ),
            expected
        );
    }
    let source = format!(
        "{prefix} struct Empty {{}} struct Pair {{ first: i64, last: bool }} enum Case {{ Empty, Value(Pair, Empty) }} fn value() -> Case {{ return Case::Value(Pair {{ last: true, first: 7 }},Empty {{}}); }}"
    );
    assert_eq!(
        preview_value(&source, "value", 65536),
        "Case::Value(Pair { first: 7, last: true }, Empty {})"
    );
}

#[test]
fn exact_one_below_and_zero_output_caps_release_results_and_allow_instance_reuse() {
    for (source, name) in [
        (SOURCE.to_string(), "preview_message"),
        (INDIRECT.to_string(), "vector"),
        (INDIRECT.to_string(), "table"),
        (INDIRECT.to_string(), "pair"),
        (INDIRECT.to_string(), "expired"),
        (INDIRECT.to_string(), "cycle"),
        (byte_source(b"A\0", false), "value"),
        (byte_source("あ\n".as_bytes(), true), "value"),
    ] {
        let program = checked(&source);
        let id = function(&program, name);
        let expected = preview_value(&source, name, 65536);
        assert_eq!(preview_value(&source, name, expected.len()), expected);
        for limit in [0, expected.len() - 1] {
            let (mut allocator, trace) = allocator();
            let mut instance = c::Instance::default();
            let failure = vm::execute_preview(
                program.failure_flow(),
                id,
                &mut instance,
                Some(&mut allocator),
                control(),
                limit,
            )
            .err()
            .unwrap();
            assert!(matches!(
                failure.reason,
                vm::Reason::Preview(lang_runtime::layout::PreviewError::Limit)
            ));
            let report = failure.report.unwrap();
            assert_eq!(
                report.primary().unwrap().kind,
                c::FailureKind::Execution(ExecutionFailure::AllocationLimit)
            );
            assert!(report.secondary().is_empty());
            assert_all_released(&allocator, &trace.borrow());
            assert!(!instance.is_poisoned());
            assert_eq!(
                vm::execute_preview(
                    program.failure_flow(),
                    id,
                    &mut instance,
                    Some(&mut allocator),
                    control(),
                    65536
                )
                .unwrap()
                .value,
                expected
            );
            assert_all_released(&allocator, &trace.borrow());
        }
    }
}

#[test]
fn entry_authority_and_host_preview_budget_reject_before_callbacks() {
    let program = checked(SOURCE);
    let id = function(&program, "fresh_bytes");
    let failure = vm::execute_preview::<Provider>(
        program.failure_flow(),
        id,
        &mut c::Instance::default(),
        None,
        control(),
        65536,
    )
    .err()
    .unwrap();
    assert!(matches!(failure.reason, vm::Reason::MissingAllocator));
    assert_eq!(failure.usage.fuel, 0);
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    for (selected, limit) in [(m::FunctionId(u32::MAX), 65536), (id, usize::MAX)] {
        let failure = vm::execute_preview(
            program.failure_flow(),
            selected,
            &mut instance,
            Some(&mut allocator),
            control(),
            limit,
        )
        .err()
        .unwrap();
        assert!(matches!(
            failure.reason,
            vm::Reason::Entry | vm::Reason::Preparation(_)
        ));
        assert_eq!(failure.usage.fuel, 0);
        assert!(trace.borrow().requests.is_empty());
        assert_all_released(&allocator, &trace.borrow());
        assert!(!instance.is_poisoned());
    }
    let source = "fn main(value: i64) -> i64 { return value; } fn value() -> core::Bytes { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); }";
    let library = checked(source);
    let selected = function(&library, "value");
    assert_eq!(
        vm::execute_preview(
            library.failure_flow(),
            selected,
            &mut instance,
            Some(&mut allocator),
            control(),
            65536
        )
        .unwrap()
        .value,
        "core::Bytes []"
    );
    assert_all_released(&allocator, &trace.borrow());
    let before = trace.borrow().requests.len();
    assert!(matches!(
        vm::execute_entry(
            library.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control()
        )
        .err()
        .unwrap()
        .reason,
        vm::Reason::Entry
    ));
    assert_eq!(trace.borrow().requests.len(), before);
    let source = "fn value() -> core::Vec<i64> { return core::result_unwrap::<core::Vec<i64>, core::AllocError>(core::vec_new::<i64>(core::allocator())); }";
    assert_eq!(preview_value(source, "value", 65536), "core::Vec []");
    assert_eq!(trace.borrow().requests.len(), before);
}

#[test]
fn every_preview_fuel_prefix_and_provider_oom_keeps_the_original_failure_and_releases_all() {
    let program = checked(SOURCE);
    let id = function(&program, "preview_message");
    let (mut baseline, trace) = allocator();
    let output = vm::execute_preview(
        program.failure_flow(),
        id,
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
        65536,
    )
    .unwrap();
    let requests = trace.borrow().requests.len();
    for prefix in 0..output.usage.fuel {
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let failure = vm::execute_preview(
            program.failure_flow(),
            id,
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(Limits {
                fuel: Some(prefix),
                ..Default::default()
            }),
            65536,
        )
        .err()
        .unwrap();
        assert_eq!(failure.usage.fuel, prefix);
        assert_eq!(
            failure.report.unwrap().primary().unwrap().kind,
            c::FailureKind::Execution(ExecutionFailure::FuelLimit)
        );
        assert!(!instance.is_poisoned());
        assert_all_released(&allocator, &trace.borrow());
    }
    for fail_at in 1..=requests {
        let (mut allocator, trace) = allocator();
        trace.borrow_mut().fail_at = fail_at;
        let mut instance = c::Instance::default();
        let failure = vm::execute_preview(
            program.failure_flow(),
            id,
            &mut instance,
            Some(&mut allocator),
            control(),
            65536,
        )
        .err()
        .unwrap();
        assert!(matches!(
            failure.reason,
            vm::Reason::Preparation(_)
                | vm::Reason::Execution(c::FailureKind::Execution(
                    ExecutionFailure::AllocationLimit | ExecutionFailure::Panic
                ))
        ));
        assert!(failure.report.unwrap().secondary().is_empty());
        assert!(!instance.is_poisoned());
        assert_all_released(&allocator, &trace.borrow());
        trace.borrow_mut().fail_at = 0;
        assert_eq!(
            vm::execute_preview(
                program.failure_flow(),
                id,
                &mut instance,
                Some(&mut allocator),
                control(),
                65536
            )
            .unwrap()
            .value,
            output.value
        );
        assert_all_released(&allocator, &trace.borrow());
    }
}

const INDIRECT: &str = include_str!("../../examples/analysis/repl_indirect.lang");

#[test]
fn indirect_previews_use_logical_values_canonical_map_order_and_checked_shared_custody() {
    for (name, expected) in [
        ("vector", "core::Vec [9, -1]"),
        ("table", "core::Map { 1: false, 2: true, 10: true }"),
        ("empty_map", "core::Map {}"),
        (
            "pair",
            "shared(Pair { strong: shared(7), back: weak::Live })",
        ),
        ("expired", "weak::Expired"),
        (
            "cycle",
            "shared(Node { next: core::Option::Some(weak::Live) })",
        ),
    ] {
        assert_eq!(preview_value(INDIRECT, name, 65536), expected);
    }
    let nested = include_str!("../../examples/analysis/native_shared_custody.lang");
    let source = format!(
        "{} return owner; }}",
        nested.split("    let mut query:").next().unwrap().replace(
            "fn main() -> i64",
            "fn value() -> shared core::Map<i64, core::Vec<shared i64>>"
        )
    );
    assert_eq!(
        preview_value(&source, "value", 65536),
        "shared(core::Map { 0: core::Vec [shared(7)] })"
    );
}

#[test]
fn text_map_order_uses_original_utf8_bytes_and_not_escaped_display_or_insertion_order() {
    let keys: [&[u8]; 5] = ["あ".as_bytes(), b"z", b"a", b"\n", b"\0"];
    let mut source = String::new();
    for (i, key) in keys.iter().enumerate() {
        source.push_str(&byte_source(key, true).replacen("fn value()", &format!("fn key{i}()"), 1));
    }
    source.push_str("fn value() -> core::Map<core::Text,i64> { let mut v: core::Map<core::Text,i64> = core::result_unwrap::<core::Map<core::Text,i64>,core::AllocError>(core::map_new::<core::Text,i64>(core::allocator()));");
    for i in 0..keys.len() {
        source.push_str(&format!("let mut p{i}: core::Option<i64> = core::result_unwrap::<core::Option<i64>,core::AllocError>(core::map_insert::<core::Text,i64>(v,key{i}(),{i}));"));
    }
    source.push_str("return v; }");
    let expected = r#"core::Map { core::Text "\u{0}": 4, core::Text "\n": 3, core::Text "a": 2, core::Text "z": 1, core::Text "\u{3042}": 0 }"#;
    assert_eq!(preview_value(&source, "value", 65536), expected);
    assert_eq!(preview_value(&source, "value", expected.len()), expected);
}

#[test]
fn nested_map_order_scratch_preserves_outer_cursors_and_recursive_vectors_use_an_iterative_stack() {
    let source = format!("{INDIRECT} fn value() -> core::Map<i64,core::Map<i64,bool>> {{
        let mut v: core::Map<i64,core::Map<i64,bool>> = core::result_unwrap::<core::Map<i64,core::Map<i64,bool>>,core::AllocError>(core::map_new::<i64,core::Map<i64,bool>>(core::allocator()));
        let mut a: core::Option<core::Map<i64,bool>> = core::result_unwrap::<core::Option<core::Map<i64,bool>>,core::AllocError>(core::map_insert::<i64,core::Map<i64,bool>>(v,2,table()));
        let mut b: core::Option<core::Map<i64,bool>> = core::result_unwrap::<core::Option<core::Map<i64,bool>>,core::AllocError>(core::map_insert::<i64,core::Map<i64,bool>>(v,1,table()));
        return v;
    }}");
    assert_eq!(
        preview_value(&source, "value", 65536),
        "core::Map { 1: core::Map { 1: false, 2: true, 10: true }, 2: core::Map { 1: false, 2: true, 10: true } }"
    );
    let source = "struct Link { next: core::Vec<Link> }
        fn value() -> core::Vec<Link> {
            let mut current: core::Vec<Link> = core::result_unwrap::<core::Vec<Link>,core::AllocError>(core::vec_new::<Link>(core::allocator()));
            let mut i: i64 = 0;
            while i < 128 {
                let mut next: core::Vec<Link> = core::result_unwrap::<core::Vec<Link>,core::AllocError>(core::vec_new::<Link>(core::allocator()));
                let mut p: core::Unit = core::result_unwrap::<core::Unit,core::AllocError>(core::vec_push::<Link>(next,Link { next: current }));
                current = next;
                i = i + 1;
            }
            return current;
        }";
    let expected = format!(
        "{}core::Vec []{}",
        "core::Vec [Link { next: ".repeat(128),
        " }]".repeat(128)
    );
    assert_eq!(preview_value(source, "value", expected.len()), expected);
}
