use lang_bytecode::{ownership, tasks};
use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{allocation as a, execution as e};
use std::path::Path;

fn source_table(text: &str) -> o::bodies::BodyTable {
    let analysis = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("stored-task.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let lang_semantics::analysis::Program::Tasks(program) = analysis.result.unwrap() else {
        panic!("task fixture")
    };
    program
        .into_verified()
        .into_plan()
        .into_source()
        .into_activity()
        .into_moved()
        .into_typed()
        .into_table()
}
fn encode(text: &str) -> Vec<u8> {
    let table = source_table(text);
    let entry = table
        .places
        .declarations
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    tasks::encode(table, Some(m::FunctionId(entry as u32)), Default::default()).unwrap()
}
fn execute_both(program: &tasks::Program, expected: i64) {
    let entry = program.entry().unwrap();
    let native = lang_codegen_clif::tasks::CompiledProgram::compile_function(
        program.proof(),
        entry,
        Default::default(),
    )
    .unwrap();
    let limits = lang_runtime::task::runner::Limits {
        trace_events: 4096,
        ..Default::default()
    };
    let mut vm_allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let vm = lang_vm::unit::tasks::execute_function(
        program.proof(),
        entry,
        &mut vm_allocator,
        e::Control::with_limits(Default::default()),
        limits,
    )
    .unwrap();
    let mut native_allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let generated = native
        .execute(
            &mut native_allocator,
            e::Control::with_limits(Default::default()),
            limits,
        )
        .unwrap();
    assert_eq!(vm.value, expected);
    assert_eq!(generated.value, vm.value);
    assert_eq!(generated.usage, vm.usage);
    assert_eq!(generated.tasks, vm.tasks);
    assert_eq!(generated.allocations, vm.allocations);
    assert_eq!(generated.events, vm.events);
    assert_eq!(generated.poisoned, vm.poisoned);
    assert_eq!(
        format!("{:?}", generated.report),
        format!("{:?}", vm.report)
    );
    assert!(!vm.poisoned);
    assert!(vm.report.is_none());
    for allocator in [&vm_allocator, &native_allocator] {
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
    }
}

#[test]
fn source_tasks_owning_channels_and_services_execute_from_revalidated_stored_mir() {
    for source in [
        include_str!("../examples/scripts/tasks.lang"),
        include_str!("fixtures/task_owned_transfer_all.lang"),
        include_str!("fixtures/task_owned_integrated.lang"),
        include_str!("fixtures/task_channels.lang"),
        include_str!("fixtures/task_channel_copy.lang"),
        include_str!("fixtures/task_channel_mixed.lang"),
        include_str!("fixtures/task_service.lang"),
        include_str!("fixtures/task_service_owned_state.lang"),
    ] {
        let bytes = encode(source);
        assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 5);
        assert_eq!(bytes[16], 0);
        assert!(matches!(
            ownership::decode(&bytes, Default::default())
                .err()
                .unwrap()
                .kind,
            ownership::CodecErrorKind::Version(5)
        ));
        let program = tasks::decode(&bytes, Default::default()).unwrap();
        execute_both(&program, 42);
        let (table, entry) = program.into_parts();
        assert_eq!(
            tasks::encode(table, entry, Default::default()).unwrap(),
            bytes
        );
    }
}

// This packer uses only the normative fields/numbers, not production writer,
// decoder, enum discriminants or guest parser. All origins name the token async.
struct Reference {
    bytes: Vec<u8>,
    effect: usize,
    scope_exit: usize,
    saved: usize,
}
fn word(v: &mut Vec<u8>, n: u32) {
    v.extend_from_slice(&n.to_le_bytes());
}
fn string(v: &mut Vec<u8>, s: &str) {
    word(v, s.len() as u32);
    v.extend_from_slice(s.as_bytes());
}
fn span(v: &mut Vec<u8>) {
    for n in [0, 0, 5] {
        word(v, n);
    }
}
fn ty(v: &mut Vec<u8>) {
    v.push(0);
    span(v);
}
fn reference() -> Reference {
    let mut v = b"LANGBC\0\0".to_vec();
    v.extend_from_slice(&[5, 0, 0, 0, 0, 0, 0, 0]);
    v.push(0); // Task profile.
    word(&mut v, 1);
    string(&mut v, "reference.lang");
    string(&mut v, "async");
    word(&mut v, 0);
    word(&mut v, 0);
    word(&mut v, 0); // definitions/types/imports.
    word(&mut v, 1);
    string(&mut v, "main");
    v.extend_from_slice(&[0, 0]);
    let effect = v.len();
    v.push(1);
    span(&mut v);
    span(&mut v);
    span(&mut v);
    word(&mut v, 0);
    ty(&mut v);
    word(&mut v, 0); // params/result/locals.
    word(&mut v, 1);
    word(&mut v, 1);
    ty(&mut v);
    word(&mut v, 0); // places.
    word(&mut v, 1);
    word(&mut v, 0);
    word(&mut v, 0); // bodies/loans/prepared.
    word(&mut v, 1);
    v.push(0);
    span(&mut v);
    word(&mut v, 0); // scopes/bindings.
    word(&mut v, 1);
    word(&mut v, 0);
    word(&mut v, 4); // blocks/params/instructions.
    word(&mut v, 1);
    word(&mut v, 0);
    v.push(0);
    v.extend_from_slice(&42i64.to_le_bytes());
    span(&mut v);
    word(&mut v, 0);
    v.push(39);
    span(&mut v);
    word(&mut v, 0);
    v.push(41);
    let saved = v.len();
    word(&mut v, 0);
    span(&mut v);
    word(&mut v, 0);
    v.push(35);
    let scope_exit = v.len();
    word(&mut v, 0);
    span(&mut v);
    v.push(4);
    span(&mut v);
    span(&mut v);
    v.push(1);
    word(&mut v, 0);
    let length = (v.len() - 16) as u32;
    v[12..16].copy_from_slice(&length.to_le_bytes());
    Reference {
        bytes: v,
        effect,
        scope_exit,
        saved,
    }
}

#[test]
fn independent_reference_vector_executes_without_a_frontend_and_round_trips() {
    let r = reference();
    let golden: Vec<u8> = include_str!("../crates/bytecode/tests/fixtures/unit-v5-task-42.hex")
        .split_whitespace()
        .map(|s| u8::from_str_radix(s, 16).unwrap())
        .collect();
    assert_eq!(r.bytes, golden);
    let program = tasks::decode(&r.bytes, Default::default()).unwrap();
    execute_both(&program, 42);
    let (table, entry) = program.into_parts();
    assert_eq!(
        tasks::encode(table, entry, Default::default()).unwrap(),
        r.bytes
    );
}

#[test]
fn malformed_effects_claims_and_saved_returns_are_independently_refused() {
    let r = reference();
    let mut bytes = r.bytes.clone();
    bytes[r.effect] = 2;
    assert!(matches!(
        tasks::decode(&bytes, Default::default())
            .err()
            .unwrap()
            .kind,
        ownership::CodecErrorKind::InvalidTag {
            tag: ownership::Tag::FunctionEffect,
            value: 2
        }
    ));
    let source = bytes.windows(5).position(|v| v == b"async").unwrap();
    let mut forged_effect = r.bytes.clone();
    forged_effect[source..source + 5].copy_from_slice(b"xxxxx");
    assert!(matches!(
        tasks::decode(&forged_effect, Default::default())
            .err()
            .unwrap()
            .kind,
        ownership::CodecErrorKind::Tasks(_) | ownership::CodecErrorKind::Types(_)
    ));
    for offset in [r.scope_exit, r.saved] {
        let mut bytes = r.bytes.clone();
        bytes[offset..offset + 4].copy_from_slice(&99u32.to_le_bytes());
        assert!(matches!(
            tasks::decode(&bytes, Default::default())
                .err()
                .unwrap()
                .kind,
            ownership::CodecErrorKind::Tasks(_) | ownership::CodecErrorKind::Types(_)
        ));
    }
    for prefix in 0..r.bytes.len() {
        assert!(tasks::decode(&r.bytes[..prefix], Default::default()).is_err());
    }
}

#[test]
fn empty_library_matches_rfc_vector_and_profile_flags_length_are_closed() {
    let mut bytes = b"LANGBC\0\0".to_vec();
    bytes.extend_from_slice(&[5, 0, 0, 0, 30, 0, 0, 0]);
    bytes.extend_from_slice(&[0; 30]);
    let program = tasks::decode(&bytes, Default::default()).unwrap();
    assert_eq!(program.entry(), None);
    let (table, entry) = program.into_parts();
    assert_eq!(
        tasks::encode(table, entry, Default::default()).unwrap(),
        bytes
    );
    for (offset, value) in [(8, 6), (10, 1), (12, 29), (16, 1), (16, 2)] {
        let mut broken = bytes.clone();
        broken[offset] = value;
        assert!(tasks::decode(&broken, Default::default()).is_err());
    }
    assert!(matches!(
        tasks::decode(
            &{
                let mut v = bytes.clone();
                v[16] = 1;
                v
            },
            Default::default()
        )
        .err()
        .unwrap()
        .kind,
        ownership::CodecErrorKind::UnsupportedProfile(1)
    ));
}

#[test]
fn read_and_independent_proof_share_exact_work_allocation_and_element_ceilings() {
    let bytes = reference().bytes;
    let usage = tasks::decode(&bytes, Default::default()).unwrap().usage();
    for (kind, used) in [
        (0, usage.work),
        (1, usage.allocation_bytes),
        (2, usage.elements),
    ] {
        assert!(used > 0);
        for decrement in [0, 1] {
            let mut limits = tasks::CodecLimits::default();
            let common = &mut limits.verification.bodies.places.ownership.unit;
            match kind {
                0 => common.work = used - decrement,
                1 => common.allocations = used - decrement,
                _ => common.elements = used - decrement,
            }
            assert_eq!(
                tasks::decode(&bytes, limits).is_ok(),
                decrement == 0,
                "kind {kind}, used {used}, decrement {decrement}"
            );
        }
    }
}

#[test]
fn encoded_entry_identity_is_preserved_in_both_engines() {
    let table = source_table(
        "async fn other()->i64{yield;return 43;} async fn main()->i64{yield;return 42;}",
    );
    let other = table
        .places
        .declarations
        .functions
        .iter()
        .position(|f| f.name == "other")
        .unwrap();
    let bytes =
        tasks::encode(table, Some(m::FunctionId(other as u32)), Default::default()).unwrap();
    execute_both(&tasks::decode(&bytes, Default::default()).unwrap(), 43);
}

#[test]
fn decoded_owned_task_failures_keep_controls_origins_and_cleanup_on_both_engines() {
    let bytes = encode(include_str!("fixtures/task_owned_transfer_all.lang"));
    let program = tasks::decode(&bytes, Default::default()).unwrap();
    let native = lang_codegen_clif::tasks::CompiledProgram::compile_function(
        program.proof(),
        program.entry().unwrap(),
        Default::default(),
    )
    .unwrap();
    for (fuel, work) in [
        (0, None),
        (1, None),
        (8, None),
        (32, None),
        (64, None),
        (128, None),
        (512, None),
        (10000, Some(0)),
        (10000, Some(8)),
    ] {
        let mut left_allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut right_allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let control = || {
            e::Control::with_limits(e::Limits {
                fuel: Some(fuel),
                ..Default::default()
            })
            .and_service_work(work)
        };
        let limits = lang_runtime::task::runner::Limits {
            trace_events: 4096,
            ..Default::default()
        };
        let left = lang_vm::unit::tasks::execute_function(
            program.proof(),
            program.entry().unwrap(),
            &mut left_allocator,
            control(),
            limits,
        );
        let right = native.execute(&mut right_allocator, control(), limits);
        match (left, right) {
            (Ok(left), Ok(right)) => {
                assert_eq!(left.value, right.value);
                assert_eq!(left.usage, right.usage);
                assert_eq!(left.events, right.events);
                assert!(!left.poisoned && !right.poisoned);
            }
            (Err(left), Err(right)) => {
                assert_eq!(format!("{:?}", left.reason), format!("{:?}", right.reason));
                assert_eq!(left.function, right.function);
                assert_eq!(left.span, right.span);
                assert_eq!(left.usage, right.usage);
                assert_eq!(left.tasks, right.tasks);
                assert_eq!(left.events, right.events);
                assert_eq!(left.poisoned, right.poisoned);
                assert_eq!(format!("{:?}", left.report), format!("{:?}", right.report));
            }
            (left, right) => panic!(
                "VM/native mismatch: left ok {}, right ok {}",
                left.is_ok(),
                right.is_ok()
            ),
        }
        for allocator in [&left_allocator, &right_allocator] {
            assert_eq!(allocator.stats().live_allocations, 0);
            assert_eq!(allocator.stats().live_bytes, 0);
        }
    }
}
