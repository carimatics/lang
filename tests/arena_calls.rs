//! Source arena brands, caller stores and read loans through guest calls.
use lang::frontend;
use lang_diagnostics::{DiagnosticCode, SourceMap, codes};
use lang_runtime::{allocation as a, cleanup as c};
use lang_semantics::analysis::{OwnershipProgram, Program};
use lang_vm::unit::{Control, Limits, owned as vm};
use std::path::Path;

fn checked(source: &str) -> Box<OwnershipProgram> {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("arena-calls.lang"),
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
fn run(source: &str) -> i64 {
    let program = checked(source);
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        Control::default(),
    )
    .unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
    result.value
}

const READ: &str = r#"
struct Node { number: i64, bytes: core::Bytes, next: core::Option<arena Node> }
fn inspect(node: &Node) -> i64 { return node.number + core::bytes_len(node.bytes); }
fn forward(node: &Node) -> i64 { return inspect(node); }
fn remember(out: &mut core::Option<arena Node>, token: arena Node) -> i64 { (*out) = core::Option::Some(token); return 0; }
fn main() -> i64 { arena graph(core::allocator()) {
    let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
    let mut token: arena Node = core::result_unwrap::<arena Node, core::ArenaError>(core::arena_new::<Node>(graph, Node { number: 8, bytes: bytes, next: core::Option::None }));
    let mut stored: core::Option<arena Node> = core::Option::None;
    let mut called: i64 = remember(stored, token);
    match stored {
        core::Option::Some(saved) => { match core::arena_read::<Node>(graph, saved) {
            core::Result::Ok(value) => { return forward(value); },
            core::Result::Err(_) => { return -1; },
        } },
        core::Option::None => { return -2; },
    }
} }
"#;
#[test]
fn read_payload_and_caller_token_projection_survive_forwarded_guest_calls() {
    assert_eq!(run(READ), 9);
}

const PRODUCT: &str = r#"
struct Pair { left: arena i64, right: arena i64 }
fn set_left(out: &mut Pair, input: arena i64, n: i64) -> i64 {
    if n < 1 { out.left = input; return 0; }
    return set_left(out, input, n + -1);
}
fn main() -> i64 { arena one(core::allocator()) {
    let mut a: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(one, 1));
    arena two(core::allocator()) {
        let mut b: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(two, 2));
        let mut pair: Pair = Pair { left: a, right: a };
        let mut called: i64 = set_left(pair, b, 3);
        let mut changed_left: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<i64>(two, pair.left, 30);
        let mut changed_right: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<i64>(one, pair.right, 40);
        match core::arena_read::<i64>(two, pair.left) {
            core::Result::Ok(left_value) => { match core::arena_read::<i64>(one, pair.right) {
                core::Result::Ok(right_value) => { return (*left_value) + (*right_value); },
                core::Result::Err(_) => { return -1; },
            } }, core::Result::Err(_) => { return -2; },
        }
    }
} }
"#;
#[test]
fn recursive_caller_mutation_keeps_distinct_field_brands_and_runtime_targets() {
    assert_eq!(run(PRODUCT), 70);
}

#[test]
fn inactive_sum_payload_does_not_escape_through_a_guest_store() {
    let source = r#"enum Choice { A(arena i64), B(arena i64) }
    fn put(out: &mut core::Option<arena i64>, input: Choice) -> i64 {
        match input { Choice::A(token) => { (*out) = core::Option::Some(token); }, Choice::B(_) => { (*out) = core::Option::None; }, }
        return 0;
    }
    fn main() -> i64 { arena outer(core::allocator()) {
        let mut saved: core::Option<arena i64> = core::Option::None;
        arena inner(core::allocator()) {
            let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(inner, 1));
            let mut called: i64 = put(saved, Choice::B(token));
        }
        match saved { core::Option::None => { return 7; }, core::Option::Some(_) => { return -1; }, }
    } }"#;
    assert_eq!(run(source), 7);
}

const CLEAR: &str = r#"fn left(out: &mut core::Option<arena i64>, input: arena i64, n: i64) -> i64 {
    if n < 1 { (*out) = core::Option::Some(input); (*out) = core::Option::None; return 0; }
    return right(out, input, n + -1);
}
fn right(out: &mut core::Option<arena i64>, input: arena i64, n: i64) -> i64 { return left(out, input, n); }
fn main() -> i64 { let mut outside: core::Option<arena i64> = core::Option::None;
    arena graph(core::allocator()) {
        let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(graph, 1));
        let mut inside: core::Option<arena i64> = core::Option::None;
        let mut called: i64 = left(inside, token, 3);
        match inside { core::Option::None => { return 8; }, core::Option::Some(_) => { return -1; }, }
    }
}"#;
#[test]
fn mutual_recursive_transient_stores_clear_the_real_caller_storage() {
    assert_eq!(run(CLEAR), 8);
}

#[test]
fn nested_reference_arguments_mutate_the_original_caller_cell() {
    let source = r#"fn put(out: &mut &mut core::Option<arena i64>, input: arena i64) -> i64 { (*(*out)) = core::Option::Some(input); return 0; }
    fn main() -> i64 { arena graph(core::allocator()) {
        let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(graph, 12));
        let mut saved: core::Option<arena i64> = core::Option::None;
        let mut reference: &mut core::Option<arena i64> = &mut saved;
        let mut called: i64 = put(reference, token);
        match (*reference) { core::Option::Some(value) => { match core::arena_read::<i64>(graph, value) {
            core::Result::Ok(number) => { return (*number); }, core::Result::Err(_) => { return -1; },
        } }, core::Option::None => { return -2; }, }
    } }"#;
    assert_eq!(run(source), 12);
}

const SPIN: &str = r#"fn spin(out: &mut core::Option<arena i64>, input: arena i64) -> i64 { (*out) = core::Option::Some(input); return spin(out, input); }
fn main() -> i64 { let mut outside: core::Option<arena i64> = core::Option::None;
    arena graph(core::allocator()) {
        let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(graph, 1));
        let mut inside: core::Option<arena i64> = core::Option::None;
        return spin(inside, token);
    }
}"#;
#[test]
fn nonreturning_call_retains_valid_region_obligations_until_frame_failure() {
    let program = checked(SPIN);
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
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
        vm::Reason::Execution(c::FailureKind::Execution(c::ExecutionFailure::FrameLimit))
    ));
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
}

fn rejected(source: &str, expected: DiagnosticCode) -> frontend::Analysis {
    let mut sources = SourceMap::new();
    let old = sources.add("arena-calls.lang", "old revision").unwrap();
    let unrelated = sources.add("noise.lang", "unrelated").unwrap();
    let analysis = frontend::analyze_virtual(
        sources,
        Path::new("arena-calls.lang"),
        source.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(analysis.result.is_err(), "source unexpectedly accepted");
    let diagnostic = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == expected)
        .unwrap_or_else(|| panic!("{:?}", analysis.diagnostics));
    assert_ne!(diagnostic.primary.span.source(), old);
    assert_ne!(diagnostic.primary.span.source(), unrelated);
    assert!(
        !analysis
            .sources
            .slice(diagnostic.primary.span)
            .unwrap()
            .is_empty()
    );
    for related in &diagnostic.secondary {
        assert_ne!(related.span.source(), old);
        assert_ne!(related.span.source(), unrelated);
        analysis.sources.slice(related.span).unwrap();
    }
    analysis
}

#[test]
fn transient_and_nonreturning_guest_stores_cannot_escape_even_when_outputs_are_cleared() {
    rejected(
        &CLEAR.replace("left(inside, token, 3)", "left(outside, token, 3)"),
        codes::REGION_ESCAPE,
    );
    rejected(
        &SPIN.replace("spin(inside, token)", "spin(outside, token)"),
        codes::REGION_ESCAPE,
    );
}

#[test]
fn caller_projection_cannot_be_reused_with_a_distinct_arena_owner() {
    rejected(
        &PRODUCT.replace("two, pair.left, 30", "one, pair.left, 30"),
        codes::REGION_ESCAPE,
    );
}

#[test]
fn guest_stores_into_a_longer_lived_collection_cannot_hide_a_region() {
    let source = r#"fn put(out: &mut core::Vec<arena i64>, input: arena i64) -> i64 {
        let mut pushed: core::Result<core::Unit, core::AllocError> = core::vec_push::<arena i64>(out, input);
        let mut popped: core::Option<arena i64> = core::vec_pop::<arena i64>(out);
        return 0;
    }
    fn main() -> i64 {
        let mut outside: core::Vec<arena i64> = core::result_unwrap::<core::Vec<arena i64>, core::AllocError>(core::vec_new::<arena i64>(core::allocator()));
        arena graph(core::allocator()) {
            let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(graph, 1));
            return put(outside, token);
        }
    }"#;
    rejected(source, codes::REGION_ESCAPE);
}

#[test]
fn ordinary_function_results_and_shared_payloads_cannot_export_arena_brands() {
    rejected(
        "fn leak(input: arena i64) -> core::Option<arena i64> { return core::Option::Some(input); } fn main() -> i64 { return 0; }",
        codes::REGION_ESCAPE,
    );
    rejected(
        "fn share(input: arena i64) -> i64 { let mut made: core::Result<shared arena i64, core::OwnershipError> = core::shared_new::<arena i64>(core::allocator(), input); return 0; } fn main() -> i64 { return 0; }",
        codes::REGION_ESCAPE,
    );
    rejected(
        "fn weak_input(input: weak arena i64) -> i64 { return 0; } fn main() -> i64 { return 0; }",
        codes::REGION_ESCAPE,
    );
}

#[test]
fn a_forwarded_read_loan_still_excludes_arena_mutation() {
    let source = r#"fn inspect(value: &i64) -> i64 { return (*value); }
    fn main() -> i64 { arena graph(core::allocator()) {
        let mut token: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(graph, 1));
        match core::arena_read::<i64>(graph, token) {
            core::Result::Ok(value) => {
                let mut before: i64 = inspect(value);
                let mut changed: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<i64>(graph, token, 2);
                return before + (*value);
            }, core::Result::Err(_) => { return -1; },
        }
    } }"#;
    rejected(source, codes::INVALID_LOAN);
}
