use super::*;

#[test]
fn native_guarded_whole_extraction_matches_vm_and_invalid_guard_is_rejected() {
    for invalid_guard in [false, true] {
        let program = source(
            "fn main() -> i64 { let mut unused: core::Option<core::Bytes> = core::Option::None; let mut value: core::Option<i64> = core::Option::Some(42); match value { core::Option::Some(x) => { return x; }, core::Option::None => { return 0; }, } }",
        );
        let mut table = program
            .mir()
            .activity()
            .moved()
            .typed()
            .clone()
            .into_table();
        let mut replaced = 0;
        for block in 0..table.functions[0].blocks.len() {
            let original = std::mem::take(&mut table.functions[0].blocks[block].instructions);
            let mut rewritten = Vec::new();
            for mut i in original {
                if let b::Op::CopyCaseField {
                    value: from,
                    case,
                    field: 0,
                    guard,
                } = i.op
                {
                    let guard = if invalid_guard {
                        let forged = value(&mut table, o::Type::Bool);
                        rewritten.push(instruction(&table, Some(forged), b::Op::Bool(false)));
                        forged
                    } else {
                        guard
                    };
                    i.op = b::Op::UnpackVariant {
                        value: from,
                        case,
                        guard,
                    };
                    replaced += 1;
                }
                rewritten.push(i);
            }
            table.functions[0].blocks[block].instructions = rewritten;
        }
        assert_eq!(replaced, 1);
        if invalid_guard {
            assert!(table.verify(Default::default()).is_err());
            continue;
        }
        let flow = flow(table);
        let native = execute_flow(&flow, &[], None, 1, 1000, None, false);
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let vm = lang_vm::unit::owned::execute_entry(
            &flow,
            &mut instance,
            Some(&mut allocator),
            Default::default(),
        )
        .unwrap();
        assert_eq!(native.scalar, Some(42), "{:?}", native.failure);
        assert_eq!(vm.value, 42);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

fn owner_flow(mapping: [usize; 3], rounds: i64) -> o::failure::Elaborated {
    let program = source(
        "fn relay(a: core::Bytes, b: core::Bytes, c: core::Bytes) -> core::Bytes { return sink(a,b,c); } fn sink(a: core::Bytes, b: core::Bytes, c: core::Bytes) -> core::Bytes { return a; }",
    );
    let mut table = program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    let mut initial: Vec<_> = table.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter(|i| matches!(i.op, b::Op::MovePlace(_)))
        .take(3)
        .cloned()
        .collect();
    let input: Vec<_> = initial.iter().map(|i| i.results[0]).collect();
    assert_eq!(input.len(), 3);
    let ty = table.places.functions[0].values[input[0].0 as usize].ty;
    let header = [
        value(&mut table, ty),
        value(&mut table, ty),
        value(&mut table, ty),
    ];
    let exit = [
        value(&mut table, ty),
        value(&mut table, ty),
        value(&mut table, ty),
    ];
    let zero = value(&mut table, o::Type::I64);
    let count = value(&mut table, o::Type::I64);
    let one = value(&mut table, o::Type::I64);
    let next = value(&mut table, o::Type::I64);
    let limit = value(&mut table, o::Type::I64);
    let again = value(&mut table, o::Type::Bool);
    initial.push(instruction(&table, Some(zero), b::Op::I64(0)));
    initial.push(instruction(&table, Some(limit), b::Op::I64(rounds + 1)));
    table.functions[0].blocks = vec![
        block(
            &table,
            vec![],
            initial,
            b::Term::Jump(target(1, input.into_iter().chain([zero]))),
        ),
        block(
            &table,
            header.into_iter().chain([count]).collect(),
            vec![
                instruction(&table, Some(one), b::Op::I64(1)),
                instruction(&table, Some(next), b::Op::Add(count, one)),
                instruction(&table, Some(again), b::Op::LessThan(next, limit)),
            ],
            b::Term::Branch {
                condition: again,
                then_target: target(1, mapping.map(|i| header[i]).into_iter().chain([next])),
                else_target: target(2, header),
            },
        ),
        block(
            &table,
            exit.to_vec(),
            vec![
                instruction(&table, None, b::Op::DropValue(exit[2])),
                instruction(&table, None, b::Op::DropValue(exit[1])),
            ],
            b::Term::Return(exit[0]),
        ),
    ];
    flow(table)
}

#[test]
fn native_parallel_owner_cycles_keep_each_buffer_and_cleanup_order() {
    let inputs: &[&[u8]] = &[b"A", b"B", b"C"];
    for mapping in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        for rounds in [1, 2] {
            let mut expected = [0, 1, 2];
            for _ in 0..rounds {
                let old = expected;
                expected = mapping.map(|i| old[i]);
            }
            let flow = owner_flow(mapping, rounds);
            let native = execute_flow(&flow, inputs, None, 1, 1000, None, false);
            assert_eq!(
                native.value.as_deref(),
                Some(inputs[expected[0]]),
                "{:?}",
                native.failure
            );
            assert_eq!(native.retired, [expected[2], expected[1]]);
            if mapping == [2, 0, 1] && rounds == 2 {
                for fuel in 0..native.fuel {
                    let failed = execute_flow(&flow, inputs, Some(fuel), 1, 1000, None, false);
                    assert!(failed.value.is_none());
                    let mut retired = failed.retired;
                    retired.sort_unstable();
                    assert_eq!(retired, [0, 1, 2]);
                }
            }
        }
    }
}
