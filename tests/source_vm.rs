//! Every source fixture runs after canonical wire encode/decode/verification.
use lang_bytecode::unit::{CodecLimits, Program, decode, encode};
use lang_mir::{HostFunctionId, unit as m};
use lang_runtime::HostFunctionRegistry;
use lang_vm::{
    InterruptionFlag,
    unit::{self as vm, Control, HostBindings, Limits, Reason, Value},
};
use std::cell::RefCell;
use std::time::{Duration, Instant};
#[path = "support/source.rs"]
mod source;
use source::Files;

fn portable(unit: m::VerifiedUnit) -> Program {
    let bytes = encode(&Program::from_verified(unit), CodecLimits::default()).unwrap();
    let program = decode(&bytes, CodecLimits::default()).unwrap();
    assert_eq!(encode(&program, CodecLimits::default()).unwrap(), bytes);
    program
}
fn program(text: &str) -> Program {
    portable(Files::new(text).unit())
}
fn function(program: &Program, name: &str) -> m::FunctionId {
    m::FunctionId(
        program
            .verified()
            .unit()
            .functions
            .iter()
            .position(|f| f.name == name)
            .unwrap() as u32,
    )
}
fn run(program: &Program) -> vm::Outcome {
    vm::execute_entry(
        program,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::default(),
    )
    .unwrap()
}
fn limited(program: &Program, limits: Limits) -> Result<vm::Outcome, vm::Failure> {
    vm::execute_entry(
        program,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(limits),
    )
}
fn snippet(program: &Program, span: m::Span) -> &str {
    &program.verified().unit().sources[span.source.0 as usize].text
        [span.start as usize..span.end as usize]
}

#[test]
fn direct_calls_returns_and_exact_no_hidden_fuel() {
    let p = program("fn main() -> i64 { return leaf(1); } fn leaf(x: i64) -> i64 { return x; }");
    let outcome = run(&p);
    assert_eq!(outcome.value.as_i64(), Some(1));
    assert_eq!(
        (
            outcome.usage.fuel,
            outcome.usage.host_calls,
            outcome.usage.peak_frames
        ),
        (4, 0, 2)
    );
    let cells: usize = p
        .verified()
        .facts()
        .functions()
        .iter()
        .map(|f| f.cells())
        .sum();
    assert_eq!(outcome.usage.peak_live_cells, cells);
    assert!(
        limited(
            &p,
            Limits {
                fuel: Some(4),
                live_cells: Some(cells),
                frames: Some(2),
                ..Limits::default()
            }
        )
        .is_ok()
    );
}

#[test]
fn returned_frames_release_their_cells_before_the_next_call() {
    let p = program(
        "fn main() -> i64 { return leaf(1) + leaf(2); } fn leaf(x: i64) -> i64 { return x; }",
    );
    let cells: usize = p
        .verified()
        .facts()
        .functions()
        .iter()
        .map(|f| f.cells())
        .sum();
    let outcome = limited(
        &p,
        Limits {
            live_cells: Some(cells),
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(outcome.value.as_i64(), Some(3));
    assert_eq!(outcome.usage.peak_live_cells, cells);
}

#[test]
fn refused_initial_entry_consumes_no_fuel_and_has_no_guest_stack() {
    let p = program("fn main() -> i64 { return 1; }");
    for (limits, reason) in [
        (
            Limits {
                frames: Some(0),
                live_cells: Some(0),
                ..Limits::default()
            },
            Reason::FrameLimit,
        ),
        (
            Limits {
                live_cells: Some(0),
                ..Limits::default()
            },
            Reason::LiveCellLimit,
        ),
    ] {
        let error = limited(&p, limits).unwrap_err();
        assert_eq!(error.reason, reason);
        assert_eq!(error.usage, vm::Usage::default());
        assert!(error.stack.is_empty());
        assert!(error.fault.is_none());
    }
}

#[test]
fn refused_guest_call_charges_call_once_and_excludes_unentered_callee() {
    let p = program("fn main() -> i64 { return leaf(1); } fn leaf(x: i64) -> i64 { return x; }");
    let initial = p.verified().facts().functions()[function(&p, "main").0 as usize].cells();
    for (limits, reason) in [
        (
            Limits {
                frames: Some(1),
                live_cells: Some(initial),
                ..Limits::default()
            },
            Reason::FrameLimit,
        ),
        (
            Limits {
                live_cells: Some(initial),
                ..Limits::default()
            },
            Reason::LiveCellLimit,
        ),
    ] {
        let error = limited(&p, limits).unwrap_err();
        assert_eq!(error.reason, reason);
        assert_eq!((error.usage.fuel, error.usage.host_calls), (2, 0));
        assert_eq!(error.stack.len(), 1);
        assert_eq!(error.stack[0].function, function(&p, "main"));
        assert_eq!(snippet(&p, error.fault.unwrap()), "leaf(1)");
    }
}

#[test]
fn fuel_failure_retains_innermost_fault_and_caller_call_sites() {
    let p = program(
        "fn main() -> i64 { return middle(1); } fn middle(x: i64) -> i64 { return leaf(x); } fn leaf(x: i64) -> i64 { return x; }",
    );
    let error = limited(
        &p,
        Limits {
            fuel: Some(3),
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::FuelExhausted);
    assert_eq!(error.usage.fuel, 3);
    assert!(error.trace_complete);
    let names: Vec<_> = error
        .stack
        .iter()
        .map(|f| {
            p.verified().unit().functions[f.function.0 as usize]
                .name
                .as_str()
        })
        .collect();
    assert_eq!(names, vec!["leaf", "middle", "main"]);
    let spans: Vec<_> = error
        .stack
        .iter()
        .map(|f| snippet(&p, f.span.unwrap()))
        .collect();
    assert_eq!(spans, vec!["return x;", "leaf(x)", "middle(1)"]);
    let error = limited(
        &p,
        Limits {
            fuel: Some(0),
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.usage.fuel, 0);
    assert_eq!(error.stack.len(), 1);
}

#[test]
fn recursive_and_mutually_recursive_functions_are_bounded_without_rust_recursion() {
    let p = program(
        "fn main() -> i64 { return sum(2000); } fn sum(n: i64) -> i64 { if n < 1 { return 0; } else { return n + sum(n + -1); } }",
    );
    let error = limited(&p, Limits::default()).unwrap_err();
    assert_eq!(error.reason, Reason::FrameLimit);
    assert_eq!(error.stack.len(), 256);
    assert_eq!(error.usage.peak_frames, 256);
    let outcome = limited(
        &p,
        Limits {
            frames: Some(3000),
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(outcome.value.as_i64(), Some(2_001_000));
    assert_eq!(outcome.usage.peak_frames, 2002);
    let p = program(
        "fn main() -> i64 { if even(101) { return 1; } else { return 0; } } fn even(n: i64) -> bool { if n < 1 { return true; } else { return odd(n + -1); } } fn odd(n: i64) -> bool { if n < 1 { return false; } else { return even(n + -1); } }",
    );
    assert_eq!(run(&p).value.as_i64(), Some(0));
    let error = limited(
        &p,
        Limits {
            frames: Some(9),
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::FrameLimit);
    assert_eq!(error.stack.len(), 9);
}

#[test]
fn aggregate_calls_pattern_order_and_loop_carried_copy_values_execute() {
    let p = program(
        "struct P { value: i64, flag: bool } enum E { None, Some(P) } fn wrap(p: P) -> E { return E::Some(p); } fn main() -> i64 { let mut p: P = P { flag: true, value: 0 }; let mut e: E = E::None; while p.value < 4 { e = wrap(p); p = P { value: p.value + 1, flag: false }; } match e { E::None => { return -1; }, E::Some(P { flag: true, value: _ }) => { return -2; }, E::Some(P { value: result, flag: false }) => { return result + p.value; }, } }",
    );
    assert_eq!(run(&p).value.as_i64(), Some(7));
    for text in [
        "struct Empty {} fn main() -> i64 { match (Empty {}) { Empty {} => { return 42; }, } }",
        "enum E { A, B(bool) } fn main() -> i64 { match E::A { _ => { return 42; }, E::B(flag) => { if flag { return 1; } else { return 2; } }, } }",
    ] {
        assert_eq!(run(&program(text)).value.as_i64(), Some(42));
    }
}

#[test]
fn nested_product_patterns_match_an_independent_finite_domain_oracle() {
    let p = program(
        "struct P { a: bool, b: bool } enum E { None, Pair(bool, P) } fn classify(e: E) -> i64 { match e { E::None => { return -1; }, E::Pair(false, _) => { return 0; }, E::Pair(true, P { a: false, b: _ }) => { return 1; }, E::Pair(true, P { b: false, a: true }) => { return 2; }, E::Pair(true, P { a: true, b: true }) => { return 3; }, } }",
    );
    let definition = p
        .verified()
        .unit()
        .definitions
        .iter()
        .position(|d| d.name == "E")
        .unwrap();
    let t = m::Type::Enum(m::DefinitionId(definition as u32));
    for (flag, a, b) in (0..8).map(|n| ((n & 4) != 0, (n & 2) != 0, (n & 1) != 0)) {
        let value = Value::from_cells(
            &p,
            t,
            vec![1, u64::from(flag), 0, u64::from(a), u64::from(b)],
        )
        .unwrap();
        let expected = if !flag {
            0
        } else if !a {
            1
        } else if !b {
            2
        } else {
            3
        };
        let outcome = vm::execute(
            &p,
            function(&p, "classify"),
            &[value],
            &HostFunctionRegistry::new(),
            &HostBindings::new(),
            Control::default(),
        )
        .unwrap();
        assert_eq!(outcome.value.as_i64(), Some(expected));
    }
    let none = Value::from_cells(&p, t, vec![0, 0, 0, 0, 0]).unwrap();
    assert_eq!(
        vm::execute(
            &p,
            function(&p, "classify"),
            &[none],
            &HostFunctionRegistry::new(),
            &HostBindings::new(),
            Control::default()
        )
        .unwrap()
        .value
        .as_i64(),
        Some(-1)
    );
}

thread_local! {static EVENTS:RefCell<Vec<i64>>=const {RefCell::new(Vec::new())};}
extern "C" fn visit(value: i64) -> i64 {
    EVENTS.with_borrow_mut(|events| events.push(value));
    value
}
fn hosts() -> (HostFunctionRegistry, HostBindings) {
    EVENTS.with_borrow_mut(Vec::clear);
    let mut hosts = HostFunctionRegistry::new();
    hosts.register_i64(HostFunctionId::new(17), visit).unwrap();
    let mut bindings = HostBindings::new();
    bindings.bind("visit", HostFunctionId::new(17)).unwrap();
    (hosts, bindings)
}

#[test]
fn arguments_and_record_fields_preserve_host_effect_order() {
    let p = program(
        "extern fn visit(x: i64) -> i64; struct P { a: i64, b: i64 } fn sum(x: i64, y: i64) -> i64 { return x + y; } fn main() -> i64 { let mut p: P = P { b: visit(2), a: visit(1) }; return sum(visit(p.a), visit(p.b)); }",
    );
    let (hosts, bindings) = hosts();
    let outcome = vm::execute_entry(&p, &hosts, &bindings, Control::default()).unwrap();
    assert_eq!(outcome.value.as_i64(), Some(3));
    assert_eq!(outcome.usage.host_calls, 4);
    EVENTS.with_borrow(|events| assert_eq!(events, &[2, 1, 1, 2]));
}

#[test]
fn integer_patterns_evaluate_the_scrutinee_once_and_test_source_order() {
    let p = program(
        "extern fn visit(x: i64) -> i64; fn value() -> i64 { return visit(7); } fn main() -> i64 { match value() { -9223372036854775808 => { return -1; }, 0 => { return 0; }, 7 => { return 42; }, _ => { return 9; }, } }",
    );
    let (hosts, bindings) = hosts();
    let outcome = vm::execute_entry(&p, &hosts, &bindings, Control::default()).unwrap();
    assert_eq!(outcome.value.as_i64(), Some(42));
    assert_eq!(outcome.usage.host_calls, 1);
    EVENTS.with_borrow(|events| assert_eq!(events, &[7]));
}

#[test]
fn enum_payloads_evaluate_left_to_right_and_pattern_bindings_are_copies() {
    let p = program(
        "extern fn visit(x: i64) -> i64; enum E { Pair(i64, i64) } fn main() -> i64 { let mut e: E = E::Pair(visit(1), visit(2)); match e { E::Pair(first, second) => { e = E::Pair(9, 9); return first + second; }, } }",
    );
    let (hosts, bindings) = hosts();
    let outcome = vm::execute_entry(&p, &hosts, &bindings, Control::default()).unwrap();
    assert_eq!(outcome.value.as_i64(), Some(3));
    EVENTS.with_borrow(|events| assert_eq!(events, &[1, 2]));
}

#[test]
fn all_imports_including_unused_qualified_imports_are_bound_before_guest_effects() {
    let files = Files::new(
        "pub mod api; extern fn visit(x: i64) -> i64; fn main() -> i64 { return visit(42); }",
    );
    std::fs::write(files.0.join("api.lang"), "extern fn unused(x: i64) -> i64;").unwrap();
    let p = portable(files.unit());
    let (hosts, mut bindings) = hosts();
    let error = vm::execute_entry(&p, &hosts, &bindings, Control::default()).unwrap_err();
    assert!(matches!(error.reason, Reason::MissingImport(_)));
    assert_eq!(error.usage.fuel, 0);
    assert!(error.stack.is_empty());
    EVENTS.with_borrow(|events| assert!(events.is_empty()));
    bindings
        .bind("api::unused", HostFunctionId::new(999))
        .unwrap();
    let error = vm::execute_entry(&p, &hosts, &bindings, Control::default()).unwrap_err();
    assert_eq!(error.reason, Reason::UnknownHost(HostFunctionId::new(999)));
    assert!(
        bindings
            .bind("api::unused", HostFunctionId::new(17))
            .is_err()
    );
}

#[test]
fn refused_host_calls_consume_instruction_fuel_without_callback_effects() {
    let p =
        program("extern fn visit(x: i64) -> i64; fn main() -> i64 { return visit(1) + visit(2); }");
    let (hosts, bindings) = hosts();
    let error = vm::execute_entry(
        &p,
        &hosts,
        &bindings,
        Control::with_limits(Limits {
            host_calls: Some(0),
            ..Limits::default()
        }),
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::HostCallLimit);
    assert_eq!((error.usage.fuel, error.usage.host_calls), (2, 0));
    EVENTS.with_borrow(|events| assert!(events.is_empty()));
    let error = vm::execute_entry(
        &p,
        &hosts,
        &bindings,
        Control::with_limits(Limits {
            host_calls: Some(1),
            ..Limits::default()
        }),
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::HostCallLimit);
    assert_eq!((error.usage.fuel, error.usage.host_calls), (4, 1));
    EVENTS.with_borrow(|events| assert_eq!(events, &[1]));
}

#[test]
fn control_precedes_entry_limits_and_interrupt_precedes_deadline() {
    let p = program("fn main() -> i64 { return 1; }");
    let flag = InterruptionFlag::new();
    flag.interrupt();
    let limits = Limits {
        fuel: Some(0),
        frames: Some(0),
        live_cells: Some(0),
        ..Limits::default()
    };
    let error = vm::execute_entry(
        &p,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(limits)
            .and_interruption(&flag)
            .and_deadline(Instant::now() - Duration::from_secs(1)),
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::Interrupted);
    assert_eq!(error.usage, vm::Usage::default());
    assert!(error.stack.is_empty());
    let error = vm::execute_entry(
        &p,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(limits).and_deadline(Instant::now() - Duration::from_secs(1)),
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::DeadlineExceeded);
    assert_eq!(error.usage.fuel, 0);
}

static INTERRUPT: InterruptionFlag = InterruptionFlag::new();
extern "C" fn interrupt(value: i64) -> i64 {
    INTERRUPT.interrupt();
    value
}
#[test]
fn callback_interruption_is_observed_after_completed_effect_at_next_safe_point() {
    let p = program("extern fn stop(x: i64) -> i64; fn main() -> i64 { return stop(1); }");
    let mut hosts = HostFunctionRegistry::new();
    hosts
        .register_i64(HostFunctionId::new(18), interrupt)
        .unwrap();
    let mut bindings = HostBindings::new();
    bindings.bind("stop", HostFunctionId::new(18)).unwrap();
    let error = vm::execute_entry(
        &p,
        &hosts,
        &bindings,
        Control::default().and_interruption(&INTERRUPT),
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::Interrupted);
    assert_eq!((error.usage.fuel, error.usage.host_calls), (2, 1));
    assert_eq!(snippet(&p, error.fault.unwrap()), "return stop(1);");
}

#[test]
fn loops_stop_at_fuel_limit_and_can_observe_a_running_deadline() {
    let p = program("fn main() -> i64 { while true {} return 1; }");
    let error = limited(
        &p,
        Limits {
            fuel: Some(17),
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::FuelExhausted);
    assert_eq!(error.usage.fuel, 17);
    let error = vm::execute_entry(
        &p,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(Limits::unbounded())
            .and_deadline(Instant::now() + Duration::from_millis(10)),
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::DeadlineExceeded);
    assert_eq!(error.stack.len(), 1);
}

#[test]
fn bad_embedding_arity_type_and_missing_entry_are_pre_execution_errors() {
    let p = program("fn sample(x: i64) -> i64 { return x; }");
    assert_eq!(
        vm::execute_entry(
            &p,
            &HostFunctionRegistry::new(),
            &HostBindings::new(),
            Control::default()
        )
        .unwrap_err()
        .reason,
        Reason::MissingEntry
    );
    for (arguments, reason) in [
        (
            vec![],
            Reason::Arity {
                expected: 1,
                actual: 0,
            },
        ),
        (
            vec![Value::boolean(true)],
            Reason::ArgumentType {
                argument: 0,
                expected: m::Type::I64,
                actual: m::Type::Bool,
            },
        ),
    ] {
        let error = vm::execute(
            &p,
            function(&p, "sample"),
            &arguments,
            &HostFunctionRegistry::new(),
            &HostBindings::new(),
            Control::default(),
        )
        .unwrap_err();
        assert_eq!(error.reason, reason);
        assert_eq!(error.usage.fuel, 0);
        assert!(error.stack.is_empty());
    }
}
