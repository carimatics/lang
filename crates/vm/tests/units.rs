use lang_bytecode::unit::Program;
use lang_mir::{ResourceTypeId, unit as m};
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{self as vm, Control, HostBindings, Limits, Reason, Value};
fn r(n: u32) -> m::Register {
    m::Register(n)
}
fn op(op: m::Op) -> m::Instruction {
    m::Instruction { op, span: None }
}
fn block(parameters: Vec<m::Register>, ops: Vec<m::Op>, kind: m::Term) -> m::Block {
    m::Block {
        parameters,
        instructions: ops.into_iter().map(op).collect(),
        terminator: m::Terminator { kind, span: None },
    }
}
fn target(block: u32, args: &[u32]) -> m::Target {
    m::Target {
        block: m::BlockId(block),
        arguments: args.iter().copied().map(r).collect(),
    }
}
fn program(definitions: Vec<m::Definition>, f: m::Function) -> Program {
    Program::lower(
        m::Unit {
            sources: vec![],
            definitions,
            imports: vec![],
            functions: vec![f],
            entry: None,
        },
        m::Limits::default(),
    )
    .unwrap()
}
fn execute(p: &Program, args: &[Value]) -> Result<vm::Outcome, vm::Failure> {
    vm::execute(
        p,
        m::FunctionId(0),
        args,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::default(),
    )
}
fn rotation() -> Program {
    let record = m::Type::Record(m::DefinitionId(0));
    program(
        vec![m::Definition {
            name: "R".into(),
            kind: m::DefinitionKind::Record(vec![
                m::Field {
                    name: "a".into(),
                    value_type: m::Type::I64,
                },
                m::Field {
                    name: "b".into(),
                    value_type: m::Type::Bool,
                },
            ]),
        }],
        m::Function {
            name: "rotate".into(),
            signature: m::Signature {
                parameters: vec![record, record, record, m::Type::I64],
                result: record,
            },
            span: None,
            registers: vec![
                record,
                record,
                record,
                m::Type::I64,
                record,
                record,
                record,
                m::Type::I64,
                m::Type::I64,
                m::Type::Bool,
                m::Type::I64,
                m::Type::I64,
            ],
            blocks: vec![
                block(vec![], vec![], m::Term::Jump(target(1, &[0, 1, 2, 3]))),
                block(
                    vec![r(4), r(5), r(6), r(7)],
                    vec![
                        m::Op::I64Constant {
                            destination: r(8),
                            value: 0,
                        },
                        m::Op::LessThan {
                            destination: r(9),
                            left: r(8),
                            right: r(7),
                        },
                    ],
                    m::Term::Branch {
                        condition: r(9),
                        then_target: target(2, &[]),
                        else_target: target(3, &[]),
                    },
                ),
                block(
                    vec![],
                    vec![
                        m::Op::I64Constant {
                            destination: r(10),
                            value: -1,
                        },
                        m::Op::Add {
                            destination: r(11),
                            left: r(7),
                            right: r(10),
                        },
                    ],
                    m::Term::Jump(target(1, &[5, 4, 4, 11])),
                ),
                block(vec![], vec![], m::Term::Return(r(6))),
            ],
        },
    )
}

#[test]
fn parallel_aggregate_arguments_rotate_cycles_after_copying_fanout() {
    let p = rotation();
    let record = m::Type::Record(m::DefinitionId(0));
    for n in [0, 1, 2, 3, 4, 99, 1000] {
        let args = [
            Value::from_cells(&p, record, vec![0, 10, 1]).unwrap(),
            Value::from_cells(&p, record, vec![0, 20, 0]).unwrap(),
            Value::from_cells(&p, record, vec![0, 30, 1]).unwrap(),
            Value::i64(n),
        ];
        let outcome = execute(&p, &args).unwrap();
        let expected = if n == 0 {
            &[0, 30, 1]
        } else if n % 2 == 1 {
            &[0, 10, 1]
        } else {
            &[0, 20, 0]
        };
        assert_eq!(outcome.value.cells(), expected);
        assert_eq!(outcome.usage.peak_live_cells, 24);
        assert_eq!(outcome.usage.peak_frames, 1);
    }
}

#[test]
fn parallel_copy_matches_a_simultaneous_assignment_oracle_for_all_three_value_maps() {
    let seed = rotation().into_verified().into_unit();
    for mapping in 0..27 {
        let sources = [mapping % 3, (mapping / 3) % 3, mapping / 9];
        for selected in 0..3 {
            let mut unit = seed.clone();
            unit.functions[0].blocks[2].terminator.kind = m::Term::Jump(target(
                1,
                &[4 + sources[0], 4 + sources[1], 4 + sources[2], 11],
            ));
            unit.functions[0].blocks[3].terminator.kind = m::Term::Return(r(4 + selected));
            let p = Program::lower(unit, m::Limits::default()).unwrap();
            let record = m::Type::Record(m::DefinitionId(0));
            for iterations in 0..4 {
                let mut expected = [10u64, 20, 30];
                for _ in 0..iterations {
                    expected = sources.map(|source| expected[source as usize]);
                }
                let args = [
                    Value::from_cells(&p, record, vec![0, 10, 1]).unwrap(),
                    Value::from_cells(&p, record, vec![0, 20, 0]).unwrap(),
                    Value::from_cells(&p, record, vec![0, 30, 1]).unwrap(),
                    Value::i64(iterations),
                ];
                let outcome = execute(&p, &args).unwrap();
                assert_eq!(outcome.value.cells()[1], expected[selected as usize]);
                assert_eq!(outcome.usage.peak_live_cells, 24);
            }
        }
    }
}

#[test]
fn scalar_and_aggregate_input_cells_must_be_canonical() {
    let p = rotation();
    let record = m::Type::Record(m::DefinitionId(0));
    for cells in [vec![], vec![0, 10], vec![0, 10, 2], vec![1, 10, 1]] {
        assert_eq!(
            Value::from_cells(&p, record, cells),
            Err(Reason::InvalidValue)
        );
    }
    assert_eq!(
        Value::from_cells(&p, m::Type::Bool, vec![2]),
        Err(Reason::InvalidValue)
    );
    assert_eq!(
        Value::from_cells(&p, m::Type::Record(m::DefinitionId(99)), vec![0]),
        Err(Reason::InvalidValue)
    );
    let own = m::Type::Own(ResourceTypeId::new(1));
    for cells in [vec![0], vec![u64::from(u32::MAX) + 1]] {
        assert_eq!(Value::from_cells(&p, own, cells), Err(Reason::InvalidValue));
    }
}

fn projection() -> Program {
    let enumeration = m::Type::Enum(m::DefinitionId(0));
    program(
        vec![m::Definition {
            name: "E".into(),
            kind: m::DefinitionKind::Enum(vec![
                m::Case {
                    name: "Some".into(),
                    payload: vec![m::Type::I64],
                },
                m::Case {
                    name: "None".into(),
                    payload: vec![],
                },
            ]),
        }],
        m::Function {
            name: "project".into(),
            signature: m::Signature {
                parameters: vec![],
                result: m::Type::I64,
            },
            registers: vec![enumeration, m::Type::I64],
            span: None,
            blocks: vec![block(
                vec![],
                vec![
                    m::Op::Enum {
                        destination: r(0),
                        definition: m::DefinitionId(0),
                        case: 1,
                        payload: vec![],
                    },
                    m::Op::EnumField {
                        destination: r(1),
                        receiver: r(0),
                        case: 0,
                        field: 0,
                    },
                ],
                m::Term::Return(r(1)),
            )],
        },
    )
}

#[test]
fn checked_projection_has_a_defined_trap_for_a_valid_hand_built_unit() {
    let p = projection();
    let error = execute(&p, &[]).unwrap_err();
    assert_eq!(
        error.reason,
        Reason::InvalidVariantProjection {
            expected: 0,
            actual: 1
        }
    );
    assert_eq!(error.usage.fuel, 2);
    assert_eq!(error.usage.peak_live_cells, 3);
    assert_eq!(error.stack.len(), 1);
    let error = vm::execute(
        &p,
        m::FunctionId(0),
        &[],
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(Limits {
            fuel: Some(1),
            ..Limits::default()
        }),
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::FuelExhausted);
}

#[test]
fn enum_tags_and_inactive_payload_padding_are_validated() {
    let p = projection();
    let enumeration = m::Type::Enum(m::DefinitionId(0));
    for cells in [vec![2, 0], vec![u64::MAX, 0], vec![1, 42]] {
        assert_eq!(
            Value::from_cells(&p, enumeration, cells),
            Err(Reason::InvalidValue)
        );
    }
    assert!(Value::from_cells(&p, enumeration, vec![0, 42]).is_ok());
    assert!(Value::from_cells(&p, enumeration, vec![1, 0]).is_ok());
    assert_eq!(
        Value::from_cells(&p, m::Type::Record(m::DefinitionId(0)), vec![0, 0]),
        Err(Reason::InvalidValue)
    );
}

#[test]
fn owned_identity_preserves_nominal_type_and_nonzero_index() {
    let resource = ResourceTypeId::new(17);
    let owned = m::Type::Own(resource);
    let p = program(
        vec![],
        m::Function {
            name: "transfer".into(),
            signature: m::Signature {
                parameters: vec![owned],
                result: owned,
            },
            registers: vec![owned],
            span: None,
            blocks: vec![block(vec![], vec![], m::Term::Return(r(0)))],
        },
    );
    let value = Value::own_resource(resource, 42).unwrap();
    let outcome = execute(&p, std::slice::from_ref(&value)).unwrap();
    assert_eq!(outcome.value, value);
    assert_eq!(outcome.usage.fuel, 1);
    assert_eq!(outcome.value.owned_index(), Some(42));
    assert_eq!(Value::own_resource(resource, 0), Err(Reason::InvalidValue));
    assert!(matches!(
        execute(
            &p,
            &[Value::own_resource(ResourceTypeId::new(18), 42).unwrap()]
        )
        .unwrap_err()
        .reason,
        Reason::ArgumentType { .. }
    ));
}

#[test]
fn enum_registers_reserve_the_largest_case_even_for_an_empty_active_case() {
    let mut u = projection().into_verified().into_unit();
    u.functions[0].blocks[0].instructions.pop();
    u.functions[0].blocks[0].terminator.kind = m::Term::Return(r(0));
    u.functions[0].registers.pop();
    u.functions[0].signature.result = m::Type::Enum(m::DefinitionId(0));
    let p = Program::lower(u, m::Limits::default()).unwrap();
    let outcome = execute(&p, &[]).unwrap();
    assert_eq!(outcome.usage.peak_live_cells, 2);
    assert_eq!(outcome.value.cells(), &[1, 0]);
    let error = vm::execute(
        &p,
        m::FunctionId(0),
        &[],
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(Limits {
            live_cells: Some(1),
            ..Limits::default()
        }),
    )
    .unwrap_err();
    assert_eq!(error.reason, Reason::LiveCellLimit);
    assert_eq!(error.usage.fuel, 0);
    assert!(error.stack.is_empty());
}
