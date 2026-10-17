use super::*;
use lang_mir::unit::ownership::{self as o, bodies as b, places as p};

const SOURCE: &str = include_str!("../../examples/scripts/owned_replace.lang");
fn call_allocator() -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    allocator_with_limits(a::Limits {
        live_allocations: 4096,
        ..Default::default()
    })
}
fn execute(source: &str, expected: i64) {
    let program = checked(source);
    let (mut allocator, trace) = call_allocator();
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, expected);
    assert_all_released(&allocator, &trace.borrow());
    assert_eq!(
        trace.borrow().requests.len(),
        trace.borrow().released_ids.len()
    );
    assert!(!instance.is_poisoned());
}

#[test]
fn source_root_field_and_scalar_loop_replacements_execute_without_transfer_allocations() {
    let program = checked(SOURCE);
    let (mut allocator, trace) = call_allocator();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 3);
    assert_eq!(outcome.usage.peak_frames, 3);
    // Five empty buffers: one initial, one whole replacement, three field writes.
    assert_eq!(
        trace
            .borrow()
            .requests
            .iter()
            .filter(|request| request.bytes() == 24)
            .count(),
        5
    );
    assert_all_released(&allocator, &trace.borrow());
    execute(&SOURCE.replace("i < 3", "i < 23"), 23);
}

#[test]
fn moved_root_reinitialization_and_reference_rebinding_preserve_unique_owners() {
    execute(
        "fn carry(value: core::Bytes) -> core::Bytes { return value; } fn main() -> i64 { let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); bytes = carry(bytes); return core::bytes_len(bytes); }",
        0,
    );
    execute(
        "fn main() -> i64 { let mut x: i64 = 1; let mut y: i64 = 2; if true { let mut parent: &mut i64 = &mut x; let mut child: &mut i64 = parent; parent = &mut y; (*child) = 3; (*parent) = 4; } return x + y; }",
        7,
    );
    execute(
        "fn main() -> i64 { let mut x: i64 = 1; let mut y: i64 = 2; if true { let mut parent: &i64 = &x; let mut child: &i64 = parent; parent = &y; return (*child) + (*parent); } return 0; }",
        3,
    );
}

#[test]
fn nested_field_and_whole_text_replacement_preserve_parent_and_borrowed_owners() {
    execute(
        "struct Inner { value: core::Text } struct Outer { inner: Inner, kept: core::Bytes } fn text() -> core::Text { return core::result_unwrap::<core::Text, core::Utf8Error>(core::text_from_utf8(core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())))); } fn change(value: &mut core::Text, incoming: core::Text) -> core::Unit { (*value) = incoming; return core::Unit {}; } fn main() -> i64 { let mut value: Outer = Outer { kept: core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())), inner: Inner { value: text() } }; let mut reference: &mut core::Text = &mut value.inner.value; let mut done: core::Unit = change(reference,text()); return core::text_len(reference); }",
        0,
    );
}

#[test]
fn copy_records_enums_and_mutable_scalar_parameters_remain_reusable_after_replacement() {
    execute(
        "struct Record { n: i64, flag: bool } enum Case { Empty, Value(Record) } fn update(value: &mut Record) -> i64 { (*value).n = (*value).n + 1; return (*value).n; } fn main() -> i64 { let mut owned: core::Option<core::Bytes> = core::Option::None; let mut value: Record = Record { n: 1, flag: true }; let mut i: i64 = 0; while i < 20 { let mut result: i64 = update(&mut value); i = i + 1; } let mut choice: Case = Case::Empty; choice = Case::Value(value); match choice { Case::Value(payload) => { return payload.n + value.n; }, Case::Empty => { return 0; }, } }",
        42,
    );
}

fn flow(table: b::BodyTable) -> o::failure::Elaborated {
    let typed = table.verify(Default::default()).unwrap();
    let moved = o::moves::analyze(typed, Default::default()).unwrap();
    let active = o::loan_activity::analyze(moved, Default::default()).unwrap();
    let loaned = o::loans::analyze(active, Default::default()).unwrap();
    o::failure::elaborate(loaned, Default::default()).unwrap()
}

#[test]
fn captured_reference_path_is_not_reread_after_rhs_rebinds_the_reference_cell() {
    let program = checked(
        "fn main() -> i64 { let mut x: i64 = 1; let mut y: i64 = 2; if true { let mut pointer: &mut i64 = &mut x; (*pointer) = 9; pointer = &mut y; } return x + y; }",
    );
    let mut table = program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    let place = table.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|i| match i.op {
            b::Op::PreparePlace { destination, place }
                if table.places.functions[0].places[place.0 as usize]
                    .projections
                    .iter()
                    .any(|projection| projection.kind == p::ProjectionKind::Dereference) =>
            {
                Some(destination)
            }
            _ => None,
        })
        .unwrap();
    let block = table.functions[0]
        .blocks
        .iter_mut()
        .find(|block| {
            block.instructions.iter().any(
                |i| matches!(i.op,b::Op::ReplacePrepared{destination,..} if destination==place),
            )
        })
        .unwrap();
    let first = block
        .instructions
        .iter()
        .position(|i| matches!(i.op,b::Op::ReplacePrepared{destination,..} if destination==place))
        .unwrap();
    let store = block.instructions.remove(first);
    let after = block
        .instructions
        .iter()
        .position(|i| matches!(i.op, b::Op::ReplacePrepared { .. }))
        .unwrap();
    block.instructions.insert(after + 1, store);
    let flow = flow(table);
    let (mut allocator, trace) = call_allocator();
    let outcome = vm::execute_entry(
        &flow,
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 11); // x=9, y=2; rereading pointer would produce 10.
    assert_all_released(&allocator, &trace.borrow());
}

#[test]
fn legacy_direct_replace_obeys_the_same_owner_publication_contract() {
    let program = checked(SOURCE);
    let mut table = program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    for function in &mut table.functions {
        let mut places = std::collections::BTreeMap::new();
        for block in &mut function.blocks {
            block.instructions.retain(|instruction| {
                if let b::Op::PreparePlace { destination, place } = instruction.op {
                    places.insert(destination.0, place);
                    false
                } else {
                    true
                }
            });
            for instruction in &mut block.instructions {
                if let b::Op::ReplacePrepared { destination, value } = instruction.op {
                    instruction.op = b::Op::Replace {
                        place: places[&destination.0],
                        value,
                    };
                }
            }
        }
    }
    let flow = flow(table);
    let (mut allocator, trace) = call_allocator();
    assert_eq!(
        vm::execute_entry(
            &flow,
            &mut c::Instance::default(),
            Some(&mut allocator),
            control()
        )
        .unwrap()
        .value,
        3
    );
    assert_all_released(&allocator, &trace.borrow());
}

#[test]
fn every_prepared_replacement_fuel_prefix_cancels_captures_and_releases_owners() {
    let program = checked(SOURCE);
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
    for prefix in 0..fuel {
        let (mut allocator, trace) = call_allocator();
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(Limits {
                fuel: Some(prefix),
                ..Default::default()
            }),
        )
        .err()
        .unwrap();
        assert_eq!(error.usage.fuel, prefix);
        assert_eq!(
            error.report.unwrap().primary().unwrap().kind,
            c::FailureKind::Execution(ExecutionFailure::FuelLimit)
        );
        assert!(!instance.is_poisoned());
        assert_all_released(&allocator, &trace.borrow());
    }
}

#[test]
fn every_replacement_admission_and_rhs_oom_preserves_cleanup_and_instance_reuse() {
    let program = checked(SOURCE);
    let (mut baseline, trace) = call_allocator();
    vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
    )
    .unwrap();
    for fail_at in 1..=trace.borrow().requests.len() {
        let (mut allocator, trace) = call_allocator();
        trace.borrow_mut().fail_at = fail_at;
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        let report = error.report.unwrap();
        assert!(matches!(
            report.primary().unwrap().kind,
            c::FailureKind::Execution(ExecutionFailure::AllocationLimit | ExecutionFailure::Panic)
        ));
        assert!(report.secondary().is_empty());
        assert!(!instance.is_poisoned());
        assert_all_released(&allocator, &trace.borrow());
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
            3
        );
        assert_all_released(&allocator, &trace.borrow());
    }
}
