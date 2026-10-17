use lang_mir::{ResourceTypeId, unit::*};

fn instruction(op: Op) -> Instruction {
    Instruction { op, span: None }
}
fn block(instructions: Vec<Op>, kind: Term) -> Block {
    Block {
        parameters: vec![],
        instructions: instructions.into_iter().map(instruction).collect(),
        terminator: Terminator { kind, span: None },
    }
}
fn target(block: u32, arguments: &[u32]) -> Target {
    Target {
        block: BlockId(block),
        arguments: arguments.iter().map(|r| Register(*r)).collect(),
    }
}
fn function(
    name: &str,
    parameters: Vec<Type>,
    result: Type,
    registers: Vec<Type>,
    blocks: Vec<Block>,
) -> Function {
    Function {
        name: name.into(),
        signature: Signature { parameters, result },
        registers,
        blocks,
        span: None,
    }
}
fn unit(functions: Vec<Function>) -> Unit {
    Unit {
        sources: vec![],
        definitions: vec![],
        imports: vec![],
        functions,
        entry: None,
    }
}
fn scalar() -> Unit {
    unit(vec![function(
        "main",
        vec![],
        Type::I64,
        vec![Type::I64],
        vec![block(
            vec![Op::I64Constant {
                destination: Register(0),
                value: 42,
            }],
            Term::Return(Register(0)),
        )],
    )])
}
fn kind(unit: Unit) -> ErrorKind {
    unit.verify(Limits::default()).unwrap_err().kind
}

#[test]
fn validates_scalars_forward_calls_and_optional_script_entry() {
    let mut source = scalar();
    source.entry = Some(FunctionId(0));
    let verified = source.verify(Limits::default()).unwrap();
    assert_eq!(verified.facts().functions()[0].cells(), 1);
    let first = function(
        "first",
        vec![Type::I64],
        Type::I64,
        vec![Type::I64, Type::I64],
        vec![block(
            vec![Op::Call {
                destination: Register(1),
                function: FunctionId(1),
                arguments: vec![Register(0)],
            }],
            Term::Return(Register(1)),
        )],
    );
    let second = function(
        "second",
        vec![Type::I64],
        Type::I64,
        vec![Type::I64],
        vec![block(vec![], Term::Return(Register(0)))],
    );
    unit(vec![first, second]).verify(Limits::default()).unwrap();
    let mut invalid = scalar();
    invalid.functions[0].name = "library::main".into();
    invalid.entry = Some(FunctionId(0));
    assert_eq!(kind(invalid), ErrorKind::InvalidEntry);
    let empty = unit(vec![]).verify(Limits::default()).unwrap();
    assert!(empty.unit().entry.is_none());
}

fn aggregate_definitions() -> Vec<Definition> {
    vec![
        Definition {
            name: "Pair".into(),
            kind: DefinitionKind::Record(vec![
                Field {
                    name: "left".into(),
                    value_type: Type::I64,
                },
                Field {
                    name: "right".into(),
                    value_type: Type::Bool,
                },
            ]),
        },
        Definition {
            name: "Maybe".into(),
            kind: DefinitionKind::Enum(vec![
                Case {
                    name: "None".into(),
                    payload: vec![],
                },
                Case {
                    name: "Some".into(),
                    payload: vec![Type::Record(DefinitionId(0))],
                },
            ]),
        },
    ]
}

#[test]
fn validates_nominal_operations_and_records_static_cell_offsets() {
    let mut source = unit(vec![function(
        "aggregate",
        vec![],
        Type::I64,
        vec![
            Type::I64,
            Type::Bool,
            Type::Record(DefinitionId(0)),
            Type::Enum(DefinitionId(1)),
            Type::Bool,
            Type::Record(DefinitionId(0)),
            Type::I64,
            Type::Bool,
        ],
        vec![block(
            vec![
                Op::I64Constant {
                    destination: Register(0),
                    value: 1,
                },
                Op::BoolConstant {
                    destination: Register(1),
                    value: true,
                },
                Op::Record {
                    destination: Register(2),
                    definition: DefinitionId(0),
                    fields: vec![Register(0), Register(1)],
                },
                Op::Enum {
                    destination: Register(3),
                    definition: DefinitionId(1),
                    case: 1,
                    payload: vec![Register(2)],
                },
                Op::IsCase {
                    destination: Register(4),
                    receiver: Register(3),
                    case: 1,
                },
                Op::EnumField {
                    destination: Register(5),
                    receiver: Register(3),
                    case: 1,
                    field: 0,
                },
                Op::Field {
                    destination: Register(6),
                    receiver: Register(5),
                    field: 0,
                },
                Op::EqualI64 {
                    destination: Register(7),
                    left: Register(0),
                    right: Register(6),
                },
            ],
            Term::Return(Register(6)),
        )],
    )]);
    source.definitions = aggregate_definitions();
    let verified = source.verify(Limits::default()).unwrap();
    let layouts = verified.facts().layouts();
    assert_eq!(
        (layouts[0].cells(), layouts[0].depth(), layouts[0].fields()),
        (3, 2, [1, 2].as_slice())
    );
    assert_eq!((layouts[1].cells(), layouts[1].depth()), (4, 3));
    assert_eq!(layouts[1].cases(), [vec![], vec![1]]);
    assert_eq!(verified.facts().functions()[0].cells(), 15);
    assert_eq!(
        verified.facts().type_cells(Type::Record(DefinitionId(1))),
        None
    );
}

#[test]
fn checked_enum_projection_is_valid_without_a_static_case_guard() {
    let mut source = unit(vec![function(
        "projection",
        vec![Type::Enum(DefinitionId(1))],
        Type::Record(DefinitionId(0)),
        vec![Type::Enum(DefinitionId(1)), Type::Record(DefinitionId(0))],
        vec![block(
            vec![Op::EnumField {
                destination: Register(1),
                receiver: Register(0),
                case: 1,
                field: 0,
            }],
            Term::Return(Register(1)),
        )],
    )]);
    source.definitions = aggregate_definitions();
    source.verify(Limits::default()).unwrap();
}

#[test]
fn duplicate_and_mismatched_nominal_members_and_cycles_are_rejected() {
    let mut source = unit(vec![]);
    source.definitions = aggregate_definitions();
    let DefinitionKind::Record(fields) = &mut source.definitions[0].kind else {
        unreachable!()
    };
    fields[1].name = "left".into();
    assert_eq!(kind(source), ErrorKind::DuplicateName);
    let mut source = unit(vec![]);
    source.definitions = vec![Definition {
        name: "Recursive".into(),
        kind: DefinitionKind::Record(vec![Field {
            name: "next".into(),
            value_type: Type::Record(DefinitionId(0)),
        }]),
    }];
    assert_eq!(kind(source), ErrorKind::RecursiveType);
    let mut source = unit(vec![]);
    source.definitions = vec![Definition {
        name: "Empty".into(),
        kind: DefinitionKind::Enum(vec![]),
    }];
    assert_eq!(kind(source), ErrorKind::EmptyEnum);
    let mut source = unit(vec![]);
    source.definitions = aggregate_definitions();
    let DefinitionKind::Record(fields) = &mut source.definitions[0].kind else {
        unreachable!()
    };
    fields[0].value_type = Type::Record(DefinitionId(1));
    assert_eq!(
        kind(source),
        ErrorKind::ExpectedRecord(Type::Enum(DefinitionId(1)))
    );
}

#[test]
fn source_spans_validate_utf8_and_positions_preserve_lf_scalar_columns() {
    let mut source = scalar();
    let text = "a😀é\r\n次\n";
    source.sources.push(Source {
        path: "src/unicode.lang".into(),
        text: text.into(),
    });
    source.functions[0].span = Some(Span {
        source: SourceId(0),
        start: 0,
        end: text.len() as u32,
    });
    source.functions[0].blocks[0].instructions[0].span = Some(Span {
        source: SourceId(0),
        start: 1,
        end: 7,
    });
    let verified = source.clone().verify(Limits::default()).unwrap();
    let facts = &verified.facts().sources()[0];
    assert_eq!(facts.position(&verified.unit().sources[0], 7), Some((1, 4)));
    assert_eq!(facts.position(&verified.unit().sources[0], 9), Some((2, 1)));
    assert_eq!(
        facts.position(&verified.unit().sources[0], 13),
        Some((3, 1))
    );
    source.functions[0].blocks[0].instructions[0]
        .span
        .as_mut()
        .unwrap()
        .start = 2;
    assert_eq!(kind(source), ErrorKind::InvalidSpan);
}

#[test]
fn source_paths_encoding_and_names_reject_nonportable_or_ambiguous_metadata() {
    for path in [
        "/absolute",
        "../escape",
        "a//b",
        "a/./b",
        "a/../b",
        "a\\b",
        "C:/absolute",
        "C:relative",
        "",
        "a\0b",
    ] {
        let mut source = scalar();
        source.sources.push(Source {
            path: path.into(),
            text: String::new(),
        });
        assert_eq!(kind(source), ErrorKind::InvalidPath, "{path:?}");
    }
    for text in ["\u{feff}fn", "// \0"] {
        let mut source = scalar();
        source.sources.push(Source {
            path: "main.lang".into(),
            text: text.into(),
        });
        assert_eq!(kind(source), ErrorKind::InvalidSourceText);
    }
    let mut source = scalar();
    source.functions[0].name = "bad::".into();
    assert_eq!(kind(source), ErrorKind::InvalidName);
    let mut source = scalar();
    source.imports.push(Import {
        name: "main".into(),
        signature: Signature {
            parameters: vec![Type::I64],
            result: Type::I64,
        },
    });
    assert_eq!(kind(source), ErrorKind::DuplicateName);
}

#[test]
fn rejects_unknown_ids_wrong_types_arity_and_duplicate_definitions() {
    let mut source = scalar();
    source.functions[0].blocks[0].terminator.kind = Term::Return(Register(9));
    assert_eq!(kind(source), ErrorKind::InvalidRegister(9));
    let mut source = scalar();
    source.functions[0].blocks[0].instructions[0].op = Op::BoolConstant {
        destination: Register(0),
        value: true,
    };
    assert!(matches!(kind(source), ErrorKind::TypeMismatch { .. }));
    let mut source = scalar();
    source.functions[0].blocks[0]
        .instructions
        .push(instruction(Op::I64Constant {
            destination: Register(0),
            value: 2,
        }));
    assert_eq!(kind(source), ErrorKind::DuplicateRegister(0));
    let mut source = scalar();
    source.functions[0].blocks[0].instructions.clear();
    assert_eq!(kind(source), ErrorKind::UndefinedRegister(0));
    let mut source = scalar();
    source.functions[0].blocks[0].instructions[0].op = Op::Call {
        destination: Register(0),
        function: FunctionId(99),
        arguments: vec![],
    };
    assert_eq!(kind(source), ErrorKind::InvalidFunction(99));
}

#[test]
fn phi_parameters_validate_types_parallel_edges_and_dominance() {
    let join = Block {
        parameters: vec![Register(3)],
        instructions: vec![],
        terminator: Terminator {
            kind: Term::Return(Register(3)),
            span: None,
        },
    };
    let source = unit(vec![function(
        "choose",
        vec![Type::Bool],
        Type::I64,
        vec![Type::Bool, Type::I64, Type::I64, Type::I64],
        vec![
            block(
                vec![],
                Term::Branch {
                    condition: Register(0),
                    then_target: target(1, &[]),
                    else_target: target(2, &[]),
                },
            ),
            block(
                vec![Op::I64Constant {
                    destination: Register(1),
                    value: 1,
                }],
                Term::Jump(target(3, &[1])),
            ),
            block(
                vec![Op::I64Constant {
                    destination: Register(2),
                    value: 2,
                }],
                Term::Jump(target(3, &[2])),
            ),
            join,
        ],
    )]);
    source.clone().verify(Limits::default()).unwrap();
    let mut invalid = source.clone();
    invalid.functions[0].blocks[3].terminator.kind = Term::Return(Register(1));
    assert_eq!(kind(invalid), ErrorKind::DoesNotDominate(1));
    let mut invalid = source.clone();
    let Term::Jump(jump) = &mut invalid.functions[0].blocks[1].terminator.kind else {
        unreachable!()
    };
    jump.arguments.clear();
    assert!(matches!(kind(invalid), ErrorKind::Arity { .. }));
    let mut invalid = source;
    invalid.functions[0].blocks[3].terminator.kind = Term::Jump(target(0, &[]));
    assert_eq!(kind(invalid), ErrorKind::EntryPredecessor);
}

#[test]
fn rejects_unreachable_blocks_and_same_instruction_self_use() {
    let mut source = scalar();
    source.functions[0]
        .blocks
        .push(block(vec![], Term::Return(Register(0))));
    assert_eq!(kind(source), ErrorKind::UnreachableBlock(1));
    let mut source = scalar();
    source.functions[0].blocks[0].instructions[0].op = Op::Add {
        destination: Register(0),
        left: Register(0),
        right: Register(0),
    };
    assert_eq!(kind(source), ErrorKind::DoesNotDominate(0));
}

#[test]
fn preserves_one_block_owned_identity_and_rejects_owned_aggregates_and_calls() {
    let own = Type::Own(ResourceTypeId::new(7));
    let identity = function(
        "identity",
        vec![own],
        own,
        vec![own],
        vec![block(vec![], Term::Return(Register(0)))],
    );
    unit(vec![identity.clone()])
        .verify(Limits::default())
        .unwrap();
    let mut source = unit(vec![identity.clone()]);
    source.functions[0]
        .blocks
        .push(block(vec![], Term::Return(Register(0))));
    assert_eq!(kind(source), ErrorKind::OwnedControlFlow);
    let mut source = unit(vec![]);
    source.definitions = vec![Definition {
        name: "Bad".into(),
        kind: DefinitionKind::Record(vec![Field {
            name: "owned".into(),
            value_type: own,
        }]),
    }];
    assert_eq!(kind(source), ErrorKind::OwnedAggregate);
    let caller = function(
        "caller",
        vec![own],
        own,
        vec![own, own],
        vec![block(
            vec![Op::Call {
                destination: Register(1),
                function: FunctionId(1),
                arguments: vec![Register(0)],
            }],
            Term::Return(Register(1)),
        )],
    );
    assert_eq!(kind(unit(vec![caller, identity])), ErrorKind::OwnedCall);
    let discard = function(
        "discard",
        vec![own],
        Type::I64,
        vec![own, Type::I64],
        vec![block(
            vec![Op::I64Constant {
                destination: Register(1),
                value: 1,
            }],
            Term::Return(Register(1)),
        )],
    );
    assert_eq!(kind(unit(vec![discard])), ErrorKind::OwnedNotConsumed(0));
}

#[test]
fn work_allocation_elements_and_structural_limits_fail_before_publication() {
    for limits in [
        Limits {
            work: 0,
            ..Limits::default()
        },
        Limits {
            allocations: 0,
            ..Limits::default()
        },
        Limits {
            elements: 0,
            ..Limits::default()
        },
        Limits {
            functions: 0,
            ..Limits::default()
        },
        Limits {
            registers: 0,
            ..Limits::default()
        },
        Limits {
            blocks: 0,
            ..Limits::default()
        },
        Limits {
            instructions: 0,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            scalar().verify(limits),
            Err(Error {
                kind: ErrorKind::Limit { .. },
                ..
            })
        ));
    }
    let mut budget = Budget::new(Limits {
        allocations: 8,
        elements: 2,
        work: 1,
        ..Limits::default()
    });
    budget.allocate::<u32>(2).unwrap();
    assert_eq!(budget.allocation_bytes(), 8);
    assert_eq!(budget.elements_used(), 2);
    assert!(budget.allocate::<u32>(1).is_err());
    assert_eq!(budget.allocation_bytes(), 8);
    budget.charge(1).unwrap();
    assert!(budget.charge(1).is_err());
    assert_eq!(budget.work_used(), 1);
    assert!(matches!(
        Budget::new(Limits::default()).allocate::<u64>(usize::MAX),
        Err(Error {
            kind: ErrorKind::ArithmeticOverflow,
            ..
        })
    ));
}

#[test]
fn allocation_budget_is_cumulative_over_temporary_function_buffers() {
    let one = scalar();
    let mut budget = Budget::new(Limits::default());
    one.clone().verify_with_budget(&mut budget).unwrap();
    let first = budget.allocation_bytes();
    assert!(first > 0);
    let mut two = one;
    let mut other = two.functions[0].clone();
    other.name = "second".into();
    two.functions.push(other);
    assert!(matches!(
        two.verify(Limits {
            allocations: first,
            ..Limits::default()
        }),
        Err(Error {
            kind: ErrorKind::Limit {
                kind: LimitKind::Allocations,
                ..
            },
            ..
        })
    ));
}

#[test]
fn dominance_matches_an_independent_path_removal_oracle_on_loops_and_diamonds() {
    const COUNT: usize = 6;
    let mut random = 0x182f_3384_u32;
    for _ in 0..256 {
        let mut edges = vec![vec![]; COUNT];
        for (block, outgoing) in edges.iter_mut().enumerate().take(COUNT - 1) {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            *outgoing = vec![block + 1, 1 + ((random >> 16) as usize % (COUNT - 1))];
        }
        for definition in 0..COUNT {
            let dominates = if definition == 0 || definition == COUNT - 1 {
                true
            } else {
                let mut seen = [false; COUNT];
                let mut pending = vec![0];
                while let Some(block) = pending.pop() {
                    if block == definition || seen[block] {
                        continue;
                    }
                    seen[block] = true;
                    pending.extend(&edges[block]);
                }
                !seen[COUNT - 1]
            };
            let blocks = (0..COUNT)
                .map(|index| {
                    block(
                        vec![Op::I64Constant {
                            destination: Register(index as u32 + 1),
                            value: index as i64,
                        }],
                        if index == COUNT - 1 {
                            Term::Return(Register(definition as u32 + 1))
                        } else {
                            Term::Branch {
                                condition: Register(0),
                                then_target: target(edges[index][0] as u32, &[]),
                                else_target: target(edges[index][1] as u32, &[]),
                            }
                        },
                    )
                })
                .collect();
            let mut registers = vec![Type::I64; COUNT + 1];
            registers[0] = Type::Bool;
            let result = unit(vec![function(
                "oracle",
                vec![Type::Bool],
                Type::I64,
                registers,
                blocks,
            )])
            .verify(Limits::default());
            assert_eq!(
                result.is_ok(),
                dominates,
                "{edges:?}, definition={definition}"
            );
            if let Err(error) = result {
                assert!(matches!(error.kind, ErrorKind::DoesNotDominate(_)));
            }
        }
    }
}

#[test]
fn nominal_depth_and_cells_use_the_same_64_and_4096_boundaries_as_source_hir() {
    let mut source = unit(vec![]);
    for index in 0..65 {
        let fields = if index == 0 {
            vec![]
        } else {
            vec![Field {
                name: "next".into(),
                value_type: Type::Record(DefinitionId(index - 1)),
            }]
        };
        source.definitions.push(Definition {
            name: format!("R{index}"),
            kind: DefinitionKind::Record(fields),
        });
    }
    let mut exact = source.clone();
    exact.definitions.pop();
    assert_eq!(
        exact.verify(Limits::default()).unwrap().facts().layouts()[63].depth(),
        64
    );
    assert!(matches!(
        kind(source),
        ErrorKind::Limit {
            kind: LimitKind::Depth,
            ..
        }
    ));
    let mut source = unit(vec![]);
    source.definitions.push(Definition {
        name: "R0".into(),
        kind: DefinitionKind::Record(vec![]),
    });
    for index in 1..=10 {
        source.definitions.push(Definition {
            name: format!("R{index}"),
            kind: DefinitionKind::Record(vec![
                Field {
                    name: "left".into(),
                    value_type: Type::Record(DefinitionId(index - 1)),
                },
                Field {
                    name: "right".into(),
                    value_type: Type::Record(DefinitionId(index - 1)),
                },
            ]),
        });
    }
    source.definitions.push(Definition {
        name: "Limit".into(),
        kind: DefinitionKind::Record(vec![
            Field {
                name: "a".into(),
                value_type: Type::Record(DefinitionId(10)),
            },
            Field {
                name: "b".into(),
                value_type: Type::Record(DefinitionId(10)),
            },
            Field {
                name: "scalar".into(),
                value_type: Type::I64,
            },
        ]),
    });
    assert_eq!(
        source
            .clone()
            .verify(Limits::default())
            .unwrap()
            .facts()
            .layouts()
            .last()
            .unwrap()
            .cells(),
        4096
    );
    source.definitions.push(Definition {
        name: "TooLarge".into(),
        kind: DefinitionKind::Record(vec![Field {
            name: "value".into(),
            value_type: Type::Record(DefinitionId(11)),
        }]),
    });
    assert!(matches!(
        kind(source),
        ErrorKind::Limit {
            kind: LimitKind::ValueCells,
            ..
        }
    ));
}

#[test]
fn signature_host_and_operand_work_is_charged_before_repeated_visits() {
    let source = scalar();
    let mut budget = Budget::new(Limits::default());
    source.clone().verify_with_budget(&mut budget).unwrap();
    let exact = budget.work_used();
    source
        .clone()
        .verify(Limits {
            work: exact,
            ..Limits::default()
        })
        .unwrap();
    assert!(matches!(
        source.verify(Limits {
            work: exact - 1,
            ..Limits::default()
        }),
        Err(Error {
            kind: ErrorKind::Limit {
                kind: LimitKind::Work,
                ..
            },
            ..
        })
    ));
    let mut source = scalar();
    source.imports.push(Import {
        name: "next".into(),
        signature: Signature {
            parameters: vec![Type::Bool],
            result: Type::I64,
        },
    });
    assert_eq!(kind(source), ErrorKind::InvalidHostSignature);
}
