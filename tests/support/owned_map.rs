use super::*;
use lang_mir::unit::ownership::{self as o, bodies as b};

#[test]
fn independently_rechecked_map_calls_can_reuse_one_copy_register_for_both_key_and_value() {
    let program = checked(include_str!("../../examples/scripts/map.lang"));
    let mut table = program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    let mut changed = 0;
    for function in &mut table.functions {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if let b::Op::CoreCall(call) = &mut instruction.op
                    && call.operation == Operation::MapInsert
                {
                    call.arguments[2] = call.arguments[1];
                    changed += 1;
                }
            }
        }
    }
    assert_eq!(changed, 2);
    let typed = table.verify(Default::default()).unwrap();
    let moved = o::moves::analyze(typed, Default::default()).unwrap();
    let active = o::loan_activity::analyze(moved, Default::default()).unwrap();
    let loaned = o::loans::analyze(active, Default::default()).unwrap();
    let flow = o::failure::elaborate(loaned, Default::default()).unwrap();
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    let outcome = vm::execute_entry(
        &flow,
        &mut c::Instance::default(),
        Some(&mut allocator),
        Control::default(),
    )
    .unwrap();
    assert_eq!(outcome.value, 3);
    assert_all_released(&allocator, &trace.borrow());
}

fn run(source: &str) -> i64 {
    let program = checked(source);
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
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
    outcome.value
}
#[test]
fn source_map_integer_new_insert_replace_read_remove_and_len_execute_through_guest_calls() {
    assert_eq!(run(include_str!("../../examples/scripts/map.lang")), 49);
}
#[test]
fn source_map_text_keys_and_owning_records_grow_replace_read_and_remove_without_losing_owners() {
    assert_eq!(
        run(include_str!("../../examples/scripts/map_owned.lang")),
        33
    );
}
#[test]
fn source_map_copy_key_value_reuse_and_absent_or_unused_read_arms_release_every_registration() {
    assert_eq!(
        run(r#"fn main() -> i64 {
        let mut values: core::Map<i64, i64> = core::result_unwrap::<core::Map<i64, i64>, core::AllocError>(core::map_new::<i64, i64>(core::allocator()));
        let mut key: i64 = 7;
        match core::map_remove::<i64, i64>(values, key) {core::Option::None => {}, core::Option::Some(_) => {return -1;},}
        match core::map_get::<i64, i64>(values, key) {core::Option::None => {}, core::Option::Some(_) => {return -2;},}
        let mut inserted: core::Result<core::Option<i64>, core::AllocError> = core::map_insert::<i64, i64>(values, key, key);
        match core::map_get::<i64, i64>(values, key) {core::Option::None => {return -3;}, core::Option::Some(_) => {},}
        let mut replaced: core::Result<core::Option<i64>, core::AllocError> = core::map_insert::<i64, i64>(values, key, key + 1);
        let mut old: i64 = core::option_unwrap::<i64>(core::result_unwrap::<core::Option<i64>, core::AllocError>(replaced));
        let mut removed: i64 = core::option_unwrap::<i64>(core::map_remove::<i64, i64>(values, key));
        return key + old + removed + core::map_len::<i64, i64>(values);
    }"#),
        22
    );
}

fn option_check(expression: &str, want: Option<i64>, borrowed: bool, code: usize) -> String {
    match want {
        Some(n) => format!(
            "match {expression} {{core::Option::Some(value{code})=>{{if {read} < {n} {{return -{code};}} if {n} < {read} {{return -{code};}}}},core::Option::None=>{{return -{code};}},}}\n",
            read = if borrowed {
                format!("(*value{code})")
            } else {
                format!("value{code}")
            }
        ),
        None => format!(
            "match {expression} {{core::Option::Some(_)=>{{return -{code};}},core::Option::None=>{{}},}}\n"
        ),
    }
}
#[test]
fn source_text_map_exact_utf8_and_mixed_mutations_match_an_independent_byte_key_oracle() {
    let keys: [&[u8]; 7] = [
        b"",
        b"a",
        b"a\0",
        b"a\0b",
        "é".as_bytes(),
        "e\u{301}".as_bytes(),
        b"b",
    ];
    let mut source = String::new();
    for (id, bytes) in keys.iter().enumerate() {
        source.push_str(&format!("fn key{id}() -> core::Text {{let mut data:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));"));
        for byte in *bytes {
            source.push_str(&format!("let mut pushed{byte}:core::Result<core::Unit,core::BufferError>=core::bytes_push(data,{byte});"));
        }
        source.push_str("return core::result_unwrap::<core::Text,core::Utf8Error>(core::text_from_utf8(data));}\n");
    }
    let mut expected = std::collections::BTreeMap::<Vec<u8>, i64>::new();
    for step in 0..39usize {
        // Keep every operation while bounding each function's analysis graph.
        source.push_str(&format!(
            "fn step{step}(values:&mut core::Map<core::Text,i64>)->i64 {{\n"
        ));
        let id = if step < 7 { step } else { (step * 5 + 3) % 7 };
        if step < 7 || step % 4 < 2 {
            let value = 100 + step as i64;
            let old = expected.insert(keys[id].to_vec(), value);
            source.push_str(&option_check(&format!("core::result_unwrap::<core::Option<i64>,core::AllocError>(core::map_insert::<core::Text,i64>(values,key{id}(),{value}))"),old,false,1000+step));
        } else {
            source.push_str(&format!("let mut query{step}:core::Text=key{id}();"));
            let (operation, old, borrowed) = if step % 4 == 2 {
                ("map_remove", expected.remove(keys[id]), false)
            } else {
                ("map_get", expected.get(keys[id]).copied(), true)
            };
            source.push_str(&option_check(
                &format!("core::{operation}::<core::Text,i64>(values,query{step})"),
                old,
                borrowed,
                1000 + step,
            ));
        }
        source.push_str(&format!(
            "if core::map_len::<core::Text,i64>(values) < {len} {{return -{code};}} if {len} < core::map_len::<core::Text,i64>(values) {{return -{code};}}\n",
            len = expected.len(),
            code = 2000 + step
        ));
        source.push_str("return 0;}\n");
    }
    source.push_str("fn main()->i64 {let mut values:core::Map<core::Text,i64>=core::result_unwrap::<core::Map<core::Text,i64>,core::AllocError>(core::map_new::<core::Text,i64>(core::allocator()));let mut score:i64=0;\n");
    for step in 0..39usize {
        source.push_str(&format!("score=score+step{step}(&mut values);\n"));
    }
    source.push_str("if score < 0 {return score;}\n");
    for id in 0..keys.len() {
        source.push_str(&format!("let mut final{id}:core::Text=key{id}();match core::map_get::<core::Text,i64>(values,final{id}) {{core::Option::Some(final_value{id})=>{{score=score+(*final_value{id});}},core::Option::None=>{{}},}}\n"));
    }
    source.push_str("return score + core::map_len::<core::Text,i64>(values);}");
    assert_eq!(
        run(&source),
        expected.values().sum::<i64>() + expected.len() as i64
    );
}

#[test]
fn source_map_copy_aggregate_operands_remain_reusable_through_replace_and_remove() {
    assert_eq!(
        run(r#"struct Record { n:i64, flag:bool }
    fn read(values:&core::Map<i64,Record>, key:&i64)->i64 {
        match core::map_get::<i64,Record>(values,key) {core::Option::Some(item)=>{if item.flag {return -100;} return item.n;},core::Option::None=>{return -100;},}
    }
    fn main()->i64 {
        let mut values:core::Map<i64,Record>=core::result_unwrap::<core::Map<i64,Record>,core::AllocError>(core::map_new::<i64,Record>(core::allocator()));
        let mut key:i64=1;let mut value:Record=Record{n:9,flag:true};
        let mut first:core::Result<core::Option<Record>,core::AllocError>=core::map_insert::<i64,Record>(values,key,value);
        let mut replaced:core::Result<core::Option<Record>,core::AllocError>=core::map_insert::<i64,Record>(values,key,Record{n:10,flag:false});
        let mut old:Record=core::option_unwrap::<Record>(core::result_unwrap::<core::Option<Record>,core::AllocError>(replaced));
        let mut seen:i64=read(values,key);let mut removed:Record=core::option_unwrap::<Record>(core::map_remove::<i64,Record>(values,key));
        if old.flag {} else {return -1;} return value.n+old.n+seen+removed.n;
    }"#),
        38
    );
}

#[test]
fn source_map_inline_enum_payloads_use_registered_case_and_field_reads() {
    assert_eq!(
        run(r#"enum Item { Empty, Full(core::Bytes) }
    fn main()->i64 {
        let mut data:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
        let mut pushed:core::Result<core::Unit,core::BufferError>=core::bytes_push(data,65);
        let mut values:core::Map<i64,Item>=core::result_unwrap::<core::Map<i64,Item>,core::AllocError>(core::map_new::<i64,Item>(core::allocator()));
        let mut key:i64=1;let mut first:core::Result<core::Option<Item>,core::AllocError>=core::map_insert::<i64,Item>(values,key,Item::Full(data));
        let mut score:i64=0;
        match core::map_get::<i64,Item>(values,key) {core::Option::Some(item)=>{match item {Item::Empty=>{return -1;},Item::Full(value)=>{score=core::bytes_len(value);},}},core::Option::None=>{return -2;},}
        let mut removed:Item=core::option_unwrap::<Item>(core::map_remove::<i64,Item>(values,key));
        match removed {Item::Empty=>{return -3;},Item::Full(last)=>{return score+core::bytes_len(last);},}
    }"#),
        2
    );
}

const CLONED_RETURN: &str = r#"fn copy_out()->core::Bytes {
    let mut data:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed:core::Result<core::Unit,core::BufferError>=core::bytes_push(data,65);
    let mut values:core::Map<i64,core::Bytes>=core::result_unwrap::<core::Map<i64,core::Bytes>,core::AllocError>(core::map_new::<i64,core::Bytes>(core::allocator()));
    let mut key:i64=1;let mut first:core::Result<core::Option<core::Bytes>,core::AllocError>=core::map_insert::<i64,core::Bytes>(values,key,data);
    match core::map_get::<i64,core::Bytes>(values,key) {core::Option::Some(value)=>{return core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_clone(core::allocator(),value));},core::Option::None=>{return core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));},}
}
fn main()->i64 {let mut result:core::Bytes=copy_out();return core::bytes_len(result);}"#;
#[test]
fn source_map_read_clone_survives_map_teardown_and_a_protected_guest_return() {
    assert_eq!(run(CLONED_RETURN), 1);
}

const RECURSIVE: &str = r#"fn carry(values:core::Map<i64,core::Bytes>,count:i64)->core::Map<i64,core::Bytes> {
    if count < 1 {return values;} return carry(values,count+(-1));
}
fn main()->i64 {
    let mut data:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed:core::Result<core::Unit,core::BufferError>=core::bytes_push(data,65);
    let mut values:core::Map<i64,core::Bytes>=core::result_unwrap::<core::Map<i64,core::Bytes>,core::AllocError>(core::map_new::<i64,core::Bytes>(core::allocator()));
    let mut first:core::Result<core::Option<core::Bytes>,core::AllocError>=core::map_insert::<i64,core::Bytes>(values,1,data);
    let mut returned:core::Map<i64,core::Bytes>=carry(values,10);let mut key:i64=1;
    let mut last:core::Bytes=core::option_unwrap::<core::Bytes>(core::map_remove::<i64,core::Bytes>(returned,key));return core::bytes_len(last);
}"#;
#[test]
fn source_map_recursive_owner_returns_and_frame_refusal_release_all_storage() {
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
}

#[test]
fn every_source_map_provider_oom_releases_moved_arguments_and_allows_instance_reuse() {
    let program = checked(include_str!("../../examples/scripts/map_owned.lang"));
    let (mut baseline, trace) = allocator_with_limits(Default::default());
    let normal = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        Control::default(),
    )
    .unwrap();
    assert_eq!(normal.value, 33);
    assert_all_released(&baseline, &trace.borrow());
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
        if let Err(e) = result {
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
            33
        );
        assert_all_released(&allocator, &trace.borrow());
    }
}

#[test]
fn every_source_map_fuel_prefix_ends_read_loans_and_releases_old_values_and_keys() {
    let program = checked(include_str!("../../examples/scripts/map_owned.lang"));
    let (mut baseline, trace) = allocator_with_limits(Default::default());
    let fuel = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        Control::default(),
    )
    .unwrap()
    .usage
    .fuel;
    assert_all_released(&baseline, &trace.borrow());
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
            assert_eq!(result.unwrap().value, 33);
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
fn source_map_allocator_parameters_reborrow_the_same_explicit_authority() {
    assert_eq!(
        run(
            r#"fn table(allocator:&core::Allocator)->core::Map<i64,i64> {
        return core::result_unwrap::<core::Map<i64,i64>,core::AllocError>(core::map_new::<i64,i64>(allocator));
    }
    fn main()->i64 {let mut authority:&core::Allocator=core::allocator();let mut first:core::Map<i64,i64>=table(authority);let mut second:core::Map<i64,i64>=table(authority);
        let mut a:core::Result<core::Option<i64>,core::AllocError>=core::map_insert::<i64,i64>(first,1,7);
        let mut b:core::Result<core::Option<i64>,core::AllocError>=core::map_insert::<i64,i64>(second,2,9);
        let mut key:i64=1;return core::option_unwrap::<i64>(core::map_remove::<i64,i64>(first,key))+core::map_len::<i64,i64>(second);
    }"#
        ),
        8
    );
}

#[test]
fn source_map_and_service_specific_profiles_reject_inactive_nested_services_before_effects() {
    for later in [
        "let mut later:core::Option<core::Map<i64,core::Vec<i64>>>=core::Option::None;",
        "let mut later:core::Option<core::Map<i64,shared i64>>=core::Option::None;",
        "let mut later:core::Option<core::Vec<core::Map<i64,i64>>>=core::Option::None;",
        "let mut later:core::Option<shared Payload>=core::Option::None;",
        "let mut later:core::Result<shared Payload,core::OwnershipError>=core::shared_new::<Payload>(core::allocator(),Payload{values:core::Option::None});",
        "arena graph(core::allocator()) {let mut later:core::Result<arena Payload,core::ArenaError>=core::arena_new::<Payload>(graph,Payload{values:core::Option::None});}",
    ] {
        let source = format!(
            "struct Payload {{values:core::Option<core::Map<i64,i64>>}} fn main()->i64 {{let mut earlier:core::Result<core::Bytes,core::AllocError>=core::bytes_new(core::allocator());{later}return 0;}}"
        );
        let program = checked(&source);
        let (mut allocator, trace) = allocator_with_limits(Default::default());
        let failure = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            Control::default(),
        )
        .err()
        .unwrap();
        assert!(matches!(failure.reason, vm::Reason::Unsupported { .. }));
        assert_eq!(failure.usage.fuel, 0);
        assert_eq!(allocator.stats(), a::Stats::default());
        assert!(trace.borrow().requests.is_empty());
    }
}

#[test]
fn source_map_registration_exhaustion_unwinds_every_read_and_owner() {
    let program = checked(
        r#"fn dive(values:&core::Map<i64,i64>,key:&i64,count:i64)->i64 {
        match core::map_get::<i64,i64>(values,key) {core::Option::Some(value)=>{if count < 1 {return (*value);} let mut answer:i64=dive(values,key,count+(-1));return answer+(*value);},core::Option::None=>{return -1;},}
    }
    fn main()->i64 {let mut values:core::Map<i64,i64>=core::result_unwrap::<core::Map<i64,i64>,core::AllocError>(core::map_new::<i64,i64>(core::allocator()));let mut inserted:core::Result<core::Option<i64>,core::AllocError>=core::map_insert::<i64,i64>(values,1,1);let mut key:i64=1;return dive(values,key,300);}"#,
    );
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    let mut instance = c::Instance::default();
    let failure = vm::execute_entry(
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
            failure.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::AllocationLimit))
        ),
        "{failure:?}"
    );
    assert_all_released(&allocator, &trace.borrow());
    assert!(!instance.is_poisoned());
}

#[test]
fn interruption_during_source_map_teardown_discards_the_protected_clone_and_releases_all_storage() {
    let program = checked(CLONED_RETURN);
    let flag = Rc::new(InterruptionFlag::new());
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    trace.borrow_mut().interrupt_release = Some(flag.clone());
    let mut instance = c::Instance::default();
    let failure = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::default().and_interruption(&flag),
    )
    .err()
    .unwrap();
    assert!(
        matches!(
            failure.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::Interrupted))
        ),
        "{failure:?}"
    );
    assert_all_released(&allocator, &trace.borrow());
    assert!(!instance.is_poisoned());
}

#[test]
fn source_map_fields_keep_the_owner_and_allocator_identity_through_guest_calls() {
    assert_eq!(
        run(r#"struct Holder { values:core::Map<i64,i64> }
    fn add(values:&mut core::Map<i64,i64>)->i64 {
        let mut old:core::Result<core::Option<i64>,core::AllocError>=core::map_insert::<i64,i64>(values,3,9);return core::map_len::<i64,i64>(values);
    }
    fn main()->i64 {
        let mut holder:Holder=Holder{values:core::result_unwrap::<core::Map<i64,i64>,core::AllocError>(core::map_new::<i64,i64>(core::allocator()))};
        let mut count:i64=add(holder.values);let mut key:i64=3;return count+core::option_unwrap::<i64>(core::map_remove::<i64,i64>(holder.values,key));
    }"#),
        10
    );
}

#[test]
fn source_map_query_can_borrow_another_maps_value_without_transferring_that_owner() {
    assert_eq!(
        run(r#"fn main()->i64 {
        let mut source:core::Map<i64,i64>=core::result_unwrap::<core::Map<i64,i64>,core::AllocError>(core::map_new::<i64,i64>(core::allocator()));
        let mut target:core::Map<i64,i64>=core::result_unwrap::<core::Map<i64,i64>,core::AllocError>(core::map_new::<i64,i64>(core::allocator()));
        let mut first:core::Result<core::Option<i64>,core::AllocError>=core::map_insert::<i64,i64>(source,7,9);
        let mut second:core::Result<core::Option<i64>,core::AllocError>=core::map_insert::<i64,i64>(target,9,11);
        let mut key:i64=7;match core::map_get::<i64,i64>(source,key) {core::Option::Some(query)=>{let mut removed:i64=core::option_unwrap::<i64>(core::map_remove::<i64,i64>(target,query));return removed+(*query)+core::map_len::<i64,i64>(target);},core::Option::None=>{return -1;},}
    }"#),
        20
    );
}

#[test]
fn source_map_moves_arguments_and_keeps_getter_values_read_only() {
    for source in [
        "fn main()->i64 {let mut values:core::Map<core::Text,i64>=core::result_unwrap::<core::Map<core::Text,i64>,core::AllocError>(core::map_new::<core::Text,i64>(core::allocator()));let mut key:core::Text=core::result_unwrap::<core::Text,core::Utf8Error>(core::text_from_utf8(core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()))));let mut result:core::Result<core::Option<i64>,core::AllocError>=core::map_insert::<core::Text,i64>(values,key,1);return core::text_len(key);}",
        "fn main()->i64 {let mut values:core::Map<i64,core::Bytes>=core::result_unwrap::<core::Map<i64,core::Bytes>,core::AllocError>(core::map_new::<i64,core::Bytes>(core::allocator()));let mut data:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut result:core::Result<core::Option<core::Bytes>,core::AllocError>=core::map_insert::<i64,core::Bytes>(values,1,data);return core::bytes_len(data);}",
        "fn main()->i64 {let mut values:core::Map<i64,i64>=core::result_unwrap::<core::Map<i64,i64>,core::AllocError>(core::map_new::<i64,i64>(core::allocator()));let mut key:i64=1;match core::map_get::<i64,i64>(values,key) {core::Option::Some(value)=>{let mut inserted:core::Result<core::Option<i64>,core::AllocError>=core::map_insert::<i64,i64>(values,key,2);return (*value);},core::Option::None=>{return 0;},}}",
    ] {
        let analysis = frontend::analyze_virtual(
            SourceMap::new(),
            Path::new("map-rejected.lang"),
            source.as_bytes(),
            None,
            Default::default(),
            None,
        );
        assert!(analysis.result.is_err());
        assert!(!analysis.diagnostics.is_empty());
        assert!(
            analysis
                .diagnostics
                .iter()
                .all(|d| d.code.as_str().starts_with("SRC002")),
            "{:?}",
            analysis.diagnostics
        );
    }
}
