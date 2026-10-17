//! Source-derived, independently reverified MIR exercises CFG arguments that
//! the current ownership source lowering does not yet generate.
use super::*;
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, places as p},
};

fn value(table: &mut b::BodyTable, ty: o::Type) -> m::Register {
    let values = &mut table.places.functions[0].values;
    let result = m::Register(values.len() as u32);
    values.push(o::TypeUse {
        ty,
        span: table.places.declarations.functions[0].span,
    });
    result
}
fn instruction(table: &b::BodyTable, result: Option<m::Register>, op: b::Op) -> b::Instruction {
    b::Instruction {
        results: result.into_iter().collect(),
        op,
        span: table.places.declarations.functions[0].span,
    }
}
fn block(
    table: &b::BodyTable,
    parameters: Vec<m::Register>,
    instructions: Vec<b::Instruction>,
    term: b::Term,
) -> b::Block {
    let span = table.places.declarations.functions[0].span;
    b::Block {
        parameters,
        instructions,
        terminator: b::Terminator { kind: term, span },
        span,
    }
}
fn target(block: u32, arguments: impl IntoIterator<Item = m::Register>) -> m::Target {
    m::Target {
        block: m::BlockId(block),
        arguments: arguments.into_iter().collect(),
    }
}
fn flow(table: b::BodyTable) -> o::failure::Elaborated {
    let typed = table.verify(Default::default()).unwrap();
    let moved = o::moves::analyze(typed, Default::default()).unwrap();
    let active = o::loan_activity::analyze(moved, Default::default()).unwrap();
    let loans = o::loans::analyze(active, Default::default()).unwrap();
    o::failure::elaborate(loans, Default::default()).unwrap()
}
fn copy_flow(mapping: [usize; 3], rounds: i64, selected: usize) -> o::failure::Elaborated {
    let program = checked(
        "fn main() -> i64 { let mut unused: core::Option<core::Bytes> = core::Option::None; return 0; }",
    );
    let mut table = program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    let input = [
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
    ];
    let header = [
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
    ];
    let exit = [
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
    ];
    let zero = value(&mut table, o::Type::I64);
    let count = value(&mut table, o::Type::I64);
    let one = value(&mut table, o::Type::I64);
    let next = value(&mut table, o::Type::I64);
    let limit = value(&mut table, o::Type::I64);
    let again = value(&mut table, o::Type::Bool);
    let mut initial: Vec<_> = input
        .into_iter()
        .zip([17, 29, 41])
        .map(|(r, n)| instruction(&table, Some(r), b::Op::I64(n)))
        .collect();
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
            vec![],
            b::Term::Return(exit[selected]),
        ),
    ];
    flow(table)
}

#[test]
fn parallel_copy_edges_match_an_independent_snapshot_oracle_for_cycles_and_fanout() {
    for a in 0..3 {
        for b in 0..3 {
            for c in 0..3 {
                for rounds in 0..=2 {
                    for selected in 0..3 {
                        let mapping = [a, b, c];
                        let mut expected = [17, 29, 41];
                        for _ in 0..rounds {
                            let old = expected;
                            expected = mapping.map(|i| old[i]);
                        }
                        let flow = copy_flow(mapping, rounds, selected);
                        let (mut allocator, trace) = text_allocator();
                        let mut instance = c::Instance::default();
                        let result = vm::execute_entry(
                            &flow,
                            &mut instance,
                            Some(&mut allocator),
                            control(),
                        )
                        .unwrap();
                        assert_eq!(
                            result.value, expected[selected],
                            "mapping {mapping:?}, rounds {rounds}, selected {selected}"
                        );
                        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
                        assert_all_released(&allocator, &trace.borrow());
                        assert!(!instance.is_poisoned());
                    }
                }
            }
        }
    }
}

#[test]
fn copy_enum_snapshots_preserve_tags_and_payloads_through_cyclic_edges() {
    for rounds in 0..=2 {
        for selected in 0..3 {
            let original = copy_flow([1, 2, 0], rounds, selected);
            let mut table = original
                .source()
                .activity()
                .moved()
                .typed()
                .clone()
                .into_table();
            let span = table.places.declarations.functions[0].span;
            let ty = o::Type::Nominal(m::DefinitionId(
                table.places.declarations.types.definitions.len() as u32,
            ));
            table
                .places
                .declarations
                .types
                .definitions
                .push(o::Definition {
                    name: "Choice".into(),
                    public: false,
                    span,
                    kind: o::DefinitionKind::Enum(vec![
                        o::Case {
                            name: "Empty".into(),
                            span,
                            payload: vec![],
                        },
                        o::Case {
                            name: "Value".into(),
                            span,
                            payload: vec![o::TypeUse {
                                ty: o::Type::I64,
                                span,
                            }],
                        },
                    ]),
                });
            let inputs: Vec<_> = table.functions[0].blocks[0]
                .terminator
                .kind
                .targets()
                .next()
                .unwrap()
                .arguments[..3]
                .to_vec();
            let parameters: Vec<_> = table.functions[0].blocks[1].parameters[..3]
                .iter()
                .chain(&table.functions[0].blocks[2].parameters)
                .copied()
                .collect();
            for register in inputs.iter().chain(&parameters) {
                table.places.functions[0].values[register.0 as usize].ty = ty;
            }
            let old = std::mem::take(&mut table.functions[0].blocks[0].instructions);
            let mut rewritten = Vec::new();
            for (i, old) in old.into_iter().enumerate() {
                if i < 3 {
                    let b::Op::I64(n) = old.op else {
                        unreachable!()
                    };
                    let payload = value(&mut table, o::Type::I64);
                    rewritten.push(instruction(&table, Some(payload), b::Op::I64(n)));
                    rewritten.push(instruction(
                        &table,
                        Some(inputs[i]),
                        b::Op::Variant {
                            case: 1,
                            payload: vec![payload],
                        },
                    ));
                } else {
                    rewritten.push(old);
                }
            }
            table.functions[0].blocks[0].instructions = rewritten;
            let chosen = table.functions[0].blocks[2].parameters[selected];
            let guard = value(&mut table, o::Type::Bool);
            let payload = value(&mut table, o::Type::I64);
            let wrong = value(&mut table, o::Type::I64);
            table.functions[0].blocks[2].instructions = vec![instruction(
                &table,
                Some(guard),
                b::Op::IsCase {
                    value: chosen,
                    case: 1,
                },
            )];
            table.functions[0].blocks[2].terminator.kind = b::Term::Branch {
                condition: guard,
                then_target: target(3, []),
                else_target: target(4, []),
            };
            let yes = block(
                &table,
                vec![],
                vec![instruction(
                    &table,
                    Some(payload),
                    b::Op::CopyCaseField {
                        value: chosen,
                        case: 1,
                        field: 0,
                        guard,
                    },
                )],
                b::Term::Return(payload),
            );
            let no = block(
                &table,
                vec![],
                vec![instruction(&table, Some(wrong), b::Op::I64(-1))],
                b::Term::Return(wrong),
            );
            table.functions[0].blocks.extend([yes, no]);
            let flow = flow(table);
            let mut expected = [17, 29, 41];
            for _ in 0..rounds {
                expected = [expected[1], expected[2], expected[0]];
            }
            let (mut allocator, trace) = text_allocator();
            assert_eq!(
                vm::execute_entry(
                    &flow,
                    &mut c::Instance::default(),
                    Some(&mut allocator),
                    control()
                )
                .unwrap()
                .value,
                expected[selected]
            );
            assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
            assert_all_released(&allocator, &trace.borrow());
        }
    }
}

fn owner_flow(mapping: [usize; 3], rounds: i64) -> o::failure::Elaborated {
    let mut source = "struct Owned { data: core::Bytes, tag: i64 } fn main() -> i64 {".to_owned();
    for n in 0..3 {
        source.push_str(&format!("let mut bytes{n}: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));"));
        for i in 0..=n {
            source.push_str(&format!("let mut push{n}_{i}: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(bytes{n},65));"));
        }
        source.push_str(&format!(
            "let mut owner{n}: Owned = Owned {{ tag: {n}, data: bytes{n} }};"
        ));
    }
    source.push_str("return core::bytes_len(owner0.data); }");
    let program = checked(&source);
    let mut table = program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    let locals: Vec<_> = (0..3)
        .map(|n| {
            table.places.declarations.functions[0]
                .locals
                .iter()
                .position(|l| l.name == format!("owner{n}"))
                .unwrap()
        })
        .collect();
    let end = table.functions[0].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.op,b::Op::Initialize{local,..} if local.0 as usize==locals[2]))
        .unwrap()
        + 1;
    assert_eq!(table.functions[0].blocks.len(), 1);
    let mut original = table.functions[0].blocks[0].clone();
    let suffix = original.instructions.split_off(end);
    let ty = table.places.declarations.functions[0].locals[locals[0]]
        .ty
        .ty;
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
    let mut inputs = Vec::new();
    for local in &locals {
        let origin = value(&mut table, ty);
        let result = value(&mut table, ty);
        let place = p::PlaceId(table.places.functions[0].places.len() as u32);
        table.places.functions[0].places.push(p::Place {
            origin,
            root: p::Root::Local(o::declarations::LocalId(*local as u32)),
            projections: vec![],
            span: original.span,
        });
        original
            .instructions
            .push(instruction(&table, Some(result), b::Op::MovePlace(place)));
        inputs.push(result);
    }
    let zero = value(&mut table, o::Type::I64);
    let count = value(&mut table, o::Type::I64);
    let one = value(&mut table, o::Type::I64);
    let next = value(&mut table, o::Type::I64);
    let limit = value(&mut table, o::Type::I64);
    let again = value(&mut table, o::Type::Bool);
    original
        .instructions
        .push(instruction(&table, Some(zero), b::Op::I64(0)));
    original
        .instructions
        .push(instruction(&table, Some(limit), b::Op::I64(rounds + 1)));
    let mut restore: Vec<_> = exit
        .into_iter()
        .zip(&locals)
        .map(|(value, local)| {
            instruction(
                &table,
                None,
                b::Op::Initialize {
                    local: o::declarations::LocalId(*local as u32),
                    value,
                },
            )
        })
        .collect();
    restore.extend(suffix);
    table.functions[0].blocks = vec![
        block(
            &table,
            vec![],
            original.instructions,
            b::Term::Jump(target(1, inputs.into_iter().chain([zero]))),
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
        block(&table, exit.to_vec(), restore, original.terminator.kind),
    ];
    flow(table)
}

#[test]
fn affine_edge_cycles_preserve_nested_buffer_owners_and_reverse_drop_order() {
    for mapping in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        for rounds in 0..=3 {
            let mut expected = [1, 2, 3];
            for _ in 0..rounds {
                let old = expected;
                expected = mapping.map(|i| old[i]);
            }
            let flow = owner_flow(mapping, rounds);
            let (mut allocator, trace) = allocator_with_limits(a::Limits {
                live_allocations: 512,
                ..Default::default()
            });
            let mut instance = c::Instance::default();
            let result =
                vm::execute_entry(&flow, &mut instance, Some(&mut allocator), control()).unwrap();
            assert_eq!(result.value, expected[0]);
            let trace = trace.borrow();
            // Growth replacements were released before any of the owners.
            let live_buffers: Vec<_> = trace
                .released
                .iter()
                .zip(&trace.released_ids)
                .filter_map(|(r, id)| {
                    (r.bytes() >= 25 && r.bytes() <= 28).then_some((r.bytes(), *id))
                })
                .rev()
                .take(3)
                .collect();
            assert_eq!(
                live_buffers
                    .iter()
                    .map(|(bytes, _)| *bytes)
                    .collect::<Vec<_>>(),
                expected.map(|n| 24 + (n as u64).next_power_of_two())
            );
            assert_eq!(trace.requests.len(), trace.released.len());
            assert_all_released(&allocator, &trace);
            assert!(!instance.is_poisoned());
        }
    }
}

fn admission(flow: &o::failure::Elaborated) -> (Vec<a::Request>, usize) {
    let typed = flow.source().activity().moved().typed();
    let layouts = lang_runtime::layout::Layouts::compile(
        typed.places().declarations().types(),
        Default::default(),
    )
    .unwrap();
    let plan = lang_runtime::layout::FramePlan::compile(
        &layouts,
        typed,
        m::FunctionId(0),
        Default::default(),
        usize::MAX,
    )
    .unwrap();
    let ordinary = lang_runtime::cleanup::MirLayout::new(typed, m::FunctionId(0), 0).unwrap();
    for (offset, parameter) in typed.functions()[0]
        .blocks
        .iter()
        .flat_map(|b| &b.parameters)
        .enumerate()
    {
        let cell = plan.scratch_cell(*parameter).unwrap();
        assert_eq!(cell, plan.return_cell() + 1 + offset);
        assert_eq!(plan.scratch_slot(*parameter), Some(ordinary.end() + offset));
        assert_eq!(
            plan.cell_type(cell),
            Some(typed.places().functions()[0].values[parameter.0 as usize])
        );
    }
    (plan.requests().to_vec(), plan.cleanup_slots() + 1)
}

#[test]
fn edge_snapshots_are_admitted_once_and_count_toward_the_live_cell_limit() {
    let mut baseline = None;
    for rounds in [0, 6] {
        let flow = owner_flow([1, 2, 0], rounds);
        let (requests, cells) = admission(&flow);
        let (mut allocator, trace) = allocator_with_limits(a::Limits {
            live_allocations: 512,
            ..Default::default()
        });
        let mut instance = c::Instance::default();
        let result = vm::execute_entry(
            &flow,
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(Limits {
                live_cells: Some(cells),
                ..Default::default()
            }),
        )
        .unwrap();
        assert_eq!(result.usage.peak_live_cells, cells);
        assert_eq!(result.allocations.requests as usize, requests.len() + 9);
        let nonzero: Vec<_> = requests.into_iter().filter(|r| r.bytes() != 0).collect();
        assert_eq!(
            &trace.borrow().requests[..nonzero.len()],
            nonzero.as_slice()
        );
        if let Some(previous) = &baseline {
            assert_eq!(previous, &trace.borrow().requests);
        } else {
            baseline = Some(trace.borrow().requests.clone());
        }
        assert_all_released(&allocator, &trace.borrow());
        let (mut denied, denied_trace) = text_allocator();
        let error = vm::execute_entry(
            &flow,
            &mut c::Instance::default(),
            Some(&mut denied),
            Control::with_limits(Limits {
                live_cells: Some(cells - 1),
                ..Default::default()
            }),
        )
        .err()
        .unwrap();
        assert!(matches!(
            error.reason,
            vm::Reason::Preparation(lang_runtime::layout::Error::CellLimit { .. })
        ));
        assert!(denied_trace.borrow().requests.is_empty());
    }
}

#[test]
fn every_edge_admission_and_guest_allocation_oom_releases_snapshots_and_owners_once() {
    let flow = owner_flow([2, 0, 1], 3);
    let (requests, _) = admission(&flow);
    let admitted = requests.iter().filter(|r| r.bytes() != 0).count();
    for failed in 1..=admitted + 9 {
        let (mut allocator, trace) = allocator_with_limits(a::Limits {
            live_allocations: 512,
            ..Default::default()
        });
        trace.borrow_mut().fail_at = failed;
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(&flow, &mut instance, Some(&mut allocator), control())
            .err()
            .unwrap();
        if failed <= admitted {
            assert!(matches!(error.reason, vm::Reason::Preparation(_)));
        } else {
            assert_eq!(error.panic_code, Some(-2));
        }
        assert!(error.report.unwrap().secondary().is_empty());
        assert_eq!(
            trace.borrow().requests.len(),
            trace.borrow().released.len() + 1
        );
        assert_all_released(&allocator, &trace.borrow());
        assert!(!instance.is_poisoned());
    }
    let (mut denied, trace) = allocator_with_limits(a::Limits {
        live_allocations: 512,
        live_bytes: requests.iter().map(|r| r.bytes()).sum::<u64>() - 1,
        ..Default::default()
    });
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(&flow, &mut instance, Some(&mut denied), control())
        .err()
        .unwrap();
    assert!(matches!(error.reason, vm::Reason::Preparation(_)));
    assert!(trace.borrow().requests.is_empty());
    assert!(!instance.is_poisoned());
}

#[test]
fn every_fuel_prefix_preserves_affine_edge_cleanup_and_copy_cycles() {
    for flow in [
        owner_flow([1, 2, 0], 3),
        copy_flow([1, 1, 0], 3, 2),
        loan_flow(2),
    ] {
        let (mut allocator, _) = allocator_with_limits(a::Limits {
            live_allocations: 512,
            ..Default::default()
        });
        let baseline = vm::execute_entry(
            &flow,
            &mut c::Instance::default(),
            Some(&mut allocator),
            control(),
        )
        .unwrap();
        for fuel in 0..=baseline.usage.fuel {
            let (mut allocator, trace) = allocator_with_limits(a::Limits {
                live_allocations: 512,
                ..Default::default()
            });
            let mut instance = c::Instance::default();
            let result = vm::execute_entry(
                &flow,
                &mut instance,
                Some(&mut allocator),
                Control::with_limits(Limits {
                    fuel: Some(fuel),
                    ..Default::default()
                }),
            );
            if fuel == baseline.usage.fuel {
                assert_eq!(result.unwrap().value, baseline.value);
            } else {
                let error = result.err().unwrap();
                assert_eq!(error.usage.fuel, fuel);
                assert!(matches!(
                    error.reason,
                    vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
                ));
                assert!(error.report.unwrap().secondary().is_empty());
            }
            assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
            assert_all_released(&allocator, &trace.borrow());
            assert!(!instance.is_poisoned());
        }
    }
}

fn loan_flow(rounds: i64) -> o::failure::Elaborated {
    let program = checked(
        "fn main() -> i64 { let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); let mut pushed: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(bytes,65)); let mut first_view: &core::Bytes = &bytes; let mut second_view: &core::Bytes = &bytes; return core::bytes_len(second_view); }",
    );
    let mut table = program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    let locals: Vec<_> = ["first_view", "second_view"]
        .map(|name| {
            table.places.declarations.functions[0]
                .locals
                .iter()
                .position(|l| l.name == name)
                .unwrap()
        })
        .into();
    let end = table.functions[0].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.op,b::Op::Initialize{local,..} if local.0 as usize==locals[1]))
        .unwrap()
        + 1;
    let mut original = table.functions[0].blocks[0].clone();
    let mut suffix = original.instructions.split_off(end);
    let borrowed = original
        .instructions
        .iter()
        .filter_map(|i| {
            if let b::Op::Borrow { place, .. } = i.op {
                Some(place)
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    let new_loan = b::LoanId(table.functions[0].loans as u32);
    table.functions[0].loans += 1;
    let b::Op::Cleanup(cleanup) = &mut suffix
        .iter_mut()
        .rev()
        .find(|i| matches!(i.op, b::Op::Cleanup(_)))
        .unwrap()
        .op
    else {
        unreachable!()
    };
    cleanup.steps.insert(
        0,
        b::CleanupStep {
            target: b::CleanupTarget::Loan(new_loan),
            span: original.span,
        },
    );
    let ty = table.places.declarations.functions[0].locals[locals[0]]
        .ty
        .ty;
    let header = value(&mut table, ty);
    let exit = value(&mut table, ty);
    let fresh = value(&mut table, ty);
    for local in &locals[1..] {
        let origin = value(&mut table, ty);
        let result = value(&mut table, ty);
        let place = p::PlaceId(table.places.functions[0].places.len() as u32);
        table.places.functions[0].places.push(p::Place {
            origin,
            root: p::Root::Local(o::declarations::LocalId(*local as u32)),
            projections: vec![],
            span: original.span,
        });
        original
            .instructions
            .push(instruction(&table, Some(result), b::Op::MovePlace(place)));
    }
    let zero = value(&mut table, o::Type::I64);
    let count = value(&mut table, o::Type::I64);
    let one = value(&mut table, o::Type::I64);
    let next = value(&mut table, o::Type::I64);
    let limit = value(&mut table, o::Type::I64);
    let again = value(&mut table, o::Type::Bool);
    original
        .instructions
        .push(instruction(&table, Some(zero), b::Op::I64(0)));
    original
        .instructions
        .push(instruction(&table, Some(limit), b::Op::I64(rounds + 1)));
    let mut restore = vec![instruction(
        &table,
        None,
        b::Op::Initialize {
            local: o::declarations::LocalId(locals[1] as u32),
            value: exit,
        },
    )];
    restore.extend(suffix);
    table.functions[0].blocks = vec![
        block(
            &table,
            vec![],
            original.instructions,
            b::Term::Jump(target(1, [zero])),
        ),
        block(
            &table,
            vec![count],
            vec![instruction(
                &table,
                Some(fresh),
                b::Op::Borrow {
                    loan: new_loan,
                    kind: o::LoanKind::Shared,
                    place: borrowed,
                },
            )],
            b::Term::Jump(target(2, [fresh])),
        ),
        block(
            &table,
            vec![header],
            vec![
                instruction(&table, Some(one), b::Op::I64(1)),
                instruction(&table, Some(next), b::Op::Add(count, one)),
                instruction(&table, Some(again), b::Op::LessThan(next, limit)),
            ],
            b::Term::Branch {
                condition: again,
                then_target: target(3, []),
                else_target: target(4, [header]),
            },
        ),
        block(
            &table,
            vec![],
            vec![instruction(
                &table,
                None,
                b::Op::EndBorrow {
                    loan: new_loan,
                    mode: b::EndMode::Required,
                },
            )],
            b::Term::Jump(target(1, [next])),
        ),
        block(&table, vec![exit], restore, original.terminator.kind),
    ];
    flow(table)
}

#[test]
fn affine_loan_parameters_can_retire_unused_old_tokens_before_rebinding() {
    for rounds in 0..=3 {
        let flow = loan_flow(rounds);
        let (mut allocator, trace) = text_allocator();
        let mut instance = c::Instance::default();
        let result =
            vm::execute_entry(&flow, &mut instance, Some(&mut allocator), control()).unwrap();
        assert_eq!(result.value, 1);
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert_all_released(&allocator, &trace.borrow());
        assert!(!instance.is_poisoned());
    }
}
