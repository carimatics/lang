use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, places as p},
};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 32,
    }
}
fn r(i: u32) -> m::Register {
    m::Register(i)
}
fn c(i: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(i))
}
fn n(i: u32) -> o::Type {
    o::Type::Nominal(m::DefinitionId(i))
}
fn instruction(results: &[u32], op: b::Op) -> b::Instruction {
    b::Instruction {
        results: results.iter().copied().map(r).collect(),
        op,
        span: span(),
    }
}
fn target(block: u32, args: &[u32]) -> m::Target {
    m::Target {
        block: m::BlockId(block),
        arguments: args.iter().copied().map(r).collect(),
    }
}
fn block(parameters: &[u32], instructions: Vec<b::Instruction>, term: b::Term) -> b::Block {
    b::Block {
        parameters: parameters.iter().copied().map(r).collect(),
        instructions,
        terminator: b::Terminator {
            kind: term,
            span: span(),
        },
        span: span(),
    }
}
fn table(types: &[o::Type], constructors: &[o::Constructor], result: o::Type) -> b::BodyTable {
    b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "body.lang".into(),
                        text: "x".repeat(32),
                    }],
                    definitions: vec![],
                    constructed: constructors
                        .iter()
                        .map(|&constructor| o::Constructed {
                            constructor,
                            span: span(),
                        })
                        .collect(),
                },
                imports: vec![],
                functions: vec![d::Function {
                    effect: Default::default(),
                    abi: d::FunctionAbi::Guest,
                    name: "f".into(),
                    public: false,
                    span: span(),
                    signature_span: span(),
                    parameters: vec![],
                    result: o::TypeUse {
                        ty: result,
                        span: span(),
                    },
                    locals: vec![],
                }],
            },
            functions: vec![p::FunctionPlaces {
                values: types
                    .iter()
                    .map(|&ty| o::TypeUse { ty, span: span() })
                    .collect(),
                places: vec![],
            }],
        },
        functions: vec![b::FunctionBody::default()],
    }
}
fn add_local(t: &mut b::BodyTable, ty: o::Type, kind: d::LocalKind) -> u32 {
    let f = &mut t.places.declarations.functions[0];
    let id = f.locals.len() as u32;
    f.locals.push(d::Local {
        name: format!("local{id}"),
        span: span(),
        kind,
        ty: o::TypeUse { ty, span: span() },
    });
    if kind == d::LocalKind::Parameter {
        f.parameters.push(d::LocalId(id));
    }
    id
}
fn place(
    t: &mut b::BodyTable,
    origin: u32,
    root: p::Root,
    projections: &[p::ProjectionKind],
) -> p::PlaceId {
    let f = &mut t.places.functions[0];
    let id = p::PlaceId(f.places.len() as u32);
    f.places.push(p::Place {
        origin: r(origin),
        root,
        projections: projections
            .iter()
            .map(|&kind| p::Projection { kind, span: span() })
            .collect(),
        span: span(),
    });
    id
}
fn error(t: b::BodyTable) -> o::ErrorKind {
    t.verify(b::Limits::default()).unwrap_err().kind
}

#[test]
fn diamonds_define_typed_block_parameters_and_preserve_unused_origin_slots() {
    let mut t = table(
        &[
            o::Type::Bool,
            o::Type::I64,
            o::Type::I64,
            o::Type::I64,
            o::Type::I64,
        ],
        &[],
        o::Type::I64,
    );
    t.functions[0].blocks = vec![
        block(
            &[],
            vec![instruction(&[0], b::Op::Bool(true))],
            b::Term::Branch {
                condition: r(0),
                then_target: target(1, &[]),
                else_target: target(2, &[]),
            },
        ),
        block(
            &[],
            vec![instruction(&[1], b::Op::I64(1))],
            b::Term::Jump(target(3, &[1])),
        ),
        block(
            &[],
            vec![instruction(&[2], b::Op::I64(2))],
            b::Term::Jump(target(3, &[2])),
        ),
        block(&[3], vec![], b::Term::Return(r(3))),
    ];
    let v = t.verify(b::Limits::default()).unwrap();
    assert_eq!(v.facts()[0].immediate_dominators(), [0, 0, 0, 0]);
    assert_eq!(
        v.facts()[0].definitions()[3],
        Some(b::Site {
            block: m::BlockId(3),
            order: 0
        })
    );
    assert_eq!(v.facts()[0].definitions()[4], None);
}

#[test]
fn noncopy_block_arguments_and_loop_backedges_are_typed_without_claiming_flow_proof() {
    let mut t = table(
        &[c(0), o::Type::Bool, c(0), c(0)],
        &[o::Constructor::Builtin(o::Builtin::Text)],
        c(0),
    );
    let local = add_local(&mut t, c(0), d::LocalKind::Parameter);
    let p = place(&mut t, 0, p::Root::Local(d::LocalId(local)), &[]);
    t.functions[0].blocks = vec![
        block(
            &[],
            vec![
                instruction(&[0], b::Op::MovePlace(p)),
                instruction(&[1], b::Op::Bool(false)),
            ],
            b::Term::Jump(target(1, &[0])),
        ),
        block(
            &[2],
            vec![],
            b::Term::Branch {
                condition: r(1),
                then_target: target(2, &[]),
                else_target: target(3, &[]),
            },
        ),
        block(
            &[],
            vec![instruction(&[3], b::Op::Move(r(2)))],
            b::Term::Jump(target(1, &[3])),
        ),
        block(&[], vec![], b::Term::Return(r(2))),
    ];
    t.verify(b::Limits::default()).unwrap();
}

#[test]
fn generated_cfg_dominance_matches_an_independent_path_intersection_oracle() {
    for seed in 0..120usize {
        let count = 3 + seed % 5;
        let definition = (seed / 3) % count;
        let usage = (seed / 7) % count;
        let mut t = table(
            &[o::Type::Bool, o::Type::I64, o::Type::I64, o::Type::I64],
            &[],
            o::Type::I64,
        );
        let mut predecessors = vec![vec![]; count];
        for block_id in 0..count {
            let mut ops = vec![];
            if block_id == 0 {
                ops.push(instruction(&[0], b::Op::Bool(true)));
                ops.push(instruction(&[3], b::Op::I64(0)));
            }
            if block_id == definition {
                ops.push(instruction(&[1], b::Op::I64(7)));
            }
            if block_id == usage {
                ops.push(instruction(&[2], b::Op::Copy(r(1))));
            }
            let term = if block_id + 1 == count {
                b::Term::Return(r(3))
            } else {
                let a = block_id + 1;
                let z = (block_id + 2 + (seed % 3)).min(count - 1);
                predecessors[a].push(block_id);
                predecessors[z].push(block_id);
                b::Term::Branch {
                    condition: r(0),
                    then_target: target(a as u32, &[]),
                    else_target: target(z as u32, &[]),
                }
            };
            t.functions[0].blocks.push(block(&[], ops, term));
        }
        let mut dom = vec![vec![false; count]; count];
        dom[0][0] = true;
        for node in 1..count {
            let (prior, remaining) = dom.split_at_mut(node);
            for (ancestor, entry) in remaining[0].iter_mut().enumerate() {
                *entry = predecessors[node].iter().all(|p| prior[*p][ancestor]);
            }
            remaining[0][node] = true;
        }
        let result = t.verify(b::Limits::default());
        assert_eq!(
            result.is_ok(),
            dom[usage][definition],
            "seed {seed}, definition {definition}, use {usage}"
        );
    }
}

#[test]
fn duplicate_undefined_forward_values_and_bad_cfg_shapes_are_rejected() {
    let mut t = table(&[o::Type::I64], &[], o::Type::I64);
    t.functions[0].blocks = vec![block(&[], vec![], b::Term::Return(r(0)))];
    assert_eq!(
        error(t.clone()),
        o::ErrorKind::Mir(m::ErrorKind::UndefinedRegister(0))
    );
    let mut bad = t.clone();
    bad.functions[0].blocks[0].instructions = vec![instruction(&[0], b::Op::Copy(r(0)))];
    assert_eq!(
        error(bad),
        o::ErrorKind::Mir(m::ErrorKind::DoesNotDominate(0))
    );
    let mut bad = t.clone();
    bad.functions[0].blocks[0].instructions = vec![
        instruction(&[0], b::Op::I64(1)),
        instruction(&[0], b::Op::I64(2)),
    ];
    assert_eq!(
        error(bad),
        o::ErrorKind::Mir(m::ErrorKind::DuplicateRegister(0))
    );
    let mut bad = t.clone();
    bad.functions[0].blocks[0].parameters.push(r(0));
    assert_eq!(error(bad), o::ErrorKind::Mir(m::ErrorKind::EntryParameters));
    let mut bad = t.clone();
    bad.functions[0].blocks[0].terminator.kind = b::Term::Jump(target(0, &[]));
    assert_eq!(
        error(bad),
        o::ErrorKind::Mir(m::ErrorKind::EntryPredecessor)
    );
    let mut bad = t.clone();
    bad.functions[0].blocks[0].terminator.kind = b::Term::Jump(target(9, &[]));
    assert_eq!(error(bad), o::ErrorKind::Mir(m::ErrorKind::InvalidBlock(9)));
    let mut bad = t.clone();
    bad.functions[0].blocks.clear();
    assert_eq!(error(bad), o::ErrorKind::Mir(m::ErrorKind::EmptyFunction));
    let mut bad = t;
    bad.functions.clear();
    assert_eq!(
        error(bad),
        o::ErrorKind::FunctionBodyCount {
            expected: 1,
            actual: 0
        }
    );
}

fn loan_body() -> b::BodyTable {
    let mut t = table(
        &[o::Type::I64, c(0), o::Type::I64],
        &[o::Constructor::Loan(o::LoanKind::Exclusive, o::Type::I64)],
        o::Type::I64,
    );
    let l = add_local(&mut t, o::Type::I64, d::LocalKind::Mutable);
    let a = place(&mut t, 0, p::Root::Local(d::LocalId(l)), &[]);
    let z = place(
        &mut t,
        2,
        p::Root::Value(r(1)),
        &[p::ProjectionKind::Dereference],
    );
    t.functions[0].loans = 1;
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(&[0], b::Op::I64(7)),
            instruction(
                &[],
                b::Op::Initialize {
                    local: d::LocalId(l),
                    value: r(0),
                },
            ),
            instruction(
                &[1],
                b::Op::Borrow {
                    loan: b::LoanId(0),
                    kind: o::LoanKind::Exclusive,
                    place: a,
                },
            ),
            instruction(
                &[],
                b::Op::Replace {
                    place: z,
                    value: r(0),
                },
            ),
            instruction(&[2], b::Op::CopyPlace(z)),
            instruction(
                &[],
                b::Op::EndBorrow {
                    loan: b::LoanId(0),
                    mode: b::EndMode::Required,
                },
            ),
            instruction(&[], b::Op::DropLocal(d::LocalId(l))),
        ],
        b::Term::Return(r(2)),
    )];
    t
}
#[test]
fn place_value_dependencies_borrow_definitions_and_conditional_ends_are_checked() {
    let t = loan_body();
    let v = t.clone().verify(b::Limits::default()).unwrap();
    assert_eq!(
        v.facts()[0].loans()[0],
        Some(b::Site {
            block: m::BlockId(0),
            order: 3
        })
    );
    let mut bad = t.clone();
    bad.functions[0].blocks[0].instructions.remove(2);
    assert!(matches!(
        error(bad),
        o::ErrorKind::Mir(m::ErrorKind::UndefinedRegister(1))
    ));
    let mut bad = t.clone();
    bad.functions[0].blocks[0].instructions[2].results.clear();
    assert!(matches!(
        error(bad),
        o::ErrorKind::Mir(m::ErrorKind::Arity { .. })
    ));
    let mut bad = t.clone();
    bad.functions[0].blocks[0].instructions.insert(
        0,
        instruction(
            &[],
            b::Op::EndBorrow {
                loan: b::LoanId(0),
                mode: b::EndMode::Required,
            },
        ),
    );
    assert_eq!(error(bad), o::ErrorKind::UndefinedLoan(0));
    let mut good = t.clone();
    good.functions[0].blocks[0].instructions.insert(
        0,
        instruction(
            &[],
            b::Op::EndBorrow {
                loan: b::LoanId(0),
                mode: b::EndMode::IfActive,
            },
        ),
    );
    good.verify(b::Limits::default()).unwrap();
    let mut bad = t;
    bad.functions[0].blocks[0].instructions[5].op = b::Op::EndBorrow {
        loan: b::LoanId(9),
        mode: b::EndMode::IfActive,
    };
    assert_eq!(error(bad), o::ErrorKind::UndefinedLoan(9));
}

#[test]
fn arena_entry_exit_and_allocator_loan_types_are_explicit() {
    let mut t = table(
        &[c(2), o::Type::I64, c(1)],
        &[
            o::Constructor::ArenaOwner(o::ArenaOwner {
                function: m::FunctionId(0),
                local: 0,
            }),
            o::Constructor::Builtin(o::Builtin::Allocator),
            o::Constructor::Loan(o::LoanKind::Shared, c(1)),
        ],
        o::Type::I64,
    );
    add_local(&mut t, c(0), d::LocalKind::ArenaOwner);
    let a = add_local(&mut t, c(2), d::LocalKind::Parameter);
    let p = place(
        &mut t,
        2,
        p::Root::Local(d::LocalId(a)),
        &[p::ProjectionKind::Dereference],
    );
    t.functions[0].loans = 1;
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(
                &[0],
                b::Op::Borrow {
                    loan: b::LoanId(0),
                    kind: o::LoanKind::Shared,
                    place: p,
                },
            ),
            instruction(
                &[],
                b::Op::ArenaEnter {
                    owner: d::LocalId(0),
                    allocator: r(0),
                },
            ),
            instruction(
                &[],
                b::Op::ArenaExit {
                    owner: d::LocalId(0),
                },
            ),
            instruction(
                &[],
                b::Op::EndBorrow {
                    loan: b::LoanId(0),
                    mode: b::EndMode::Required,
                },
            ),
            instruction(&[1], b::Op::I64(0)),
        ],
        b::Term::Return(r(1)),
    )];
    t.clone().verify(b::Limits::default()).unwrap();
    t.functions[0].blocks[0].instructions[2].op = b::Op::ArenaExit {
        owner: d::LocalId(a),
    };
    assert_eq!(error(t), o::ErrorKind::InvalidInitialization(a));
}

fn case_body(borrow: bool) -> b::BodyTable {
    let mut t = table(
        &[
            o::Type::I64,
            c(0),
            n(0),
            o::Type::Bool,
            o::Type::I64,
            c(0),
            c(1),
        ],
        &[
            o::Constructor::Builtin(o::Builtin::Text),
            o::Constructor::Loan(o::LoanKind::Shared, c(0)),
        ],
        o::Type::I64,
    );
    t.places.declarations.types.definitions.push(o::Definition {
        name: "Sum".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Enum(vec![
            o::Case {
                name: "Empty".into(),
                span: span(),
                payload: vec![],
            },
            o::Case {
                name: "Item".into(),
                span: span(),
                payload: vec![
                    o::TypeUse {
                        ty: o::Type::I64,
                        span: span(),
                    },
                    o::TypeUse {
                        ty: c(0),
                        span: span(),
                    },
                ],
            },
        ]),
    });
    let l = add_local(&mut t, c(0), d::LocalKind::Parameter);
    let p = place(&mut t, 1, p::Root::Local(d::LocalId(l)), &[]);
    let ops = if borrow {
        t.functions[0].loans = 1;
        vec![
            instruction(
                &[6],
                b::Op::BorrowCaseField {
                    loan: b::LoanId(0),
                    value: r(2),
                    case: 1,
                    field: 1,
                    guard: r(3),
                },
            ),
            instruction(
                &[4],
                b::Op::CopyCaseField {
                    value: r(2),
                    case: 1,
                    field: 0,
                    guard: r(3),
                },
            ),
            instruction(
                &[],
                b::Op::EndBorrow {
                    loan: b::LoanId(0),
                    mode: b::EndMode::Required,
                },
            ),
        ]
    } else {
        vec![instruction(
            &[4, 5],
            b::Op::UnpackVariant {
                value: r(2),
                case: 1,
                guard: r(3),
            },
        )]
    };
    t.functions[0].blocks = vec![
        block(
            &[],
            vec![
                instruction(&[0], b::Op::I64(7)),
                instruction(&[1], b::Op::MovePlace(p)),
                instruction(
                    &[2],
                    b::Op::Variant {
                        case: 1,
                        payload: vec![r(0), r(1)],
                    },
                ),
                instruction(
                    &[3],
                    b::Op::IsCase {
                        value: r(2),
                        case: 1,
                    },
                ),
            ],
            b::Term::Branch {
                condition: r(3),
                then_target: target(1, &[]),
                else_target: target(2, &[]),
            },
        ),
        block(&[], ops, b::Term::Return(r(4))),
        block(&[], vec![], b::Term::Return(r(0))),
    ];
    t
}
#[test]
fn variant_payload_copy_borrow_and_whole_unpack_require_a_matching_true_edge_guard() {
    case_body(false).verify(b::Limits::default()).unwrap();
    case_body(true).verify(b::Limits::default()).unwrap();
    let mut t = case_body(false);
    t.functions[0].blocks[0].instructions[3].op = b::Op::Bool(true);
    assert_eq!(error(t), o::ErrorKind::CaseGuard);
    let mut t = case_body(false);
    t.functions[0].blocks[2].terminator.kind = b::Term::Jump(target(1, &[]));
    assert_eq!(error(t), o::ErrorKind::CaseGuard);
    let mut t = case_body(false);
    t.functions[0].blocks.pop();
    t.functions[0].blocks[0].terminator.kind = b::Term::Branch {
        condition: r(3),
        then_target: target(1, &[]),
        else_target: target(1, &[]),
    };
    assert_eq!(error(t), o::ErrorKind::CaseGuard);
    let mut t = case_body(true);
    t.functions[0].blocks[1].instructions[0] = instruction(
        &[5],
        b::Op::CopyCaseField {
            value: r(2),
            case: 1,
            field: 1,
            guard: r(3),
        },
    );
    assert_eq!(error(t), o::ErrorKind::NonCopyPlace(c(0)));
}

#[test]
fn every_core_error_case_and_option_result_payload_shape_is_checked() {
    for (builtin, count) in [
        (o::Builtin::AllocError, 3),
        (o::Builtin::BoundsError, 1),
        (o::Builtin::BufferError, 3),
        (o::Builtin::Utf8Error, 1),
        (o::Builtin::OwnershipError, 6),
        (o::Builtin::ArenaError, 4),
    ] {
        for case in 0..count {
            let mut t = table(
                &[c(0), o::Type::I64, c(1)],
                &[
                    o::Constructor::Builtin(o::Builtin::AllocError),
                    o::Constructor::Builtin(builtin),
                ],
                c(1),
            );
            let payload = if builtin == o::Builtin::Utf8Error {
                vec![r(1)]
            } else if (builtin == o::Builtin::BufferError && case == 2)
                || (matches!(builtin, o::Builtin::OwnershipError | o::Builtin::ArenaError)
                    && case == 0)
            {
                vec![r(0)]
            } else {
                vec![]
            };
            t.functions[0].blocks = vec![block(
                &[],
                vec![
                    instruction(
                        &[0],
                        b::Op::Variant {
                            case: 0,
                            payload: vec![],
                        },
                    ),
                    instruction(&[1], b::Op::I64(3)),
                    instruction(&[2], b::Op::Variant { case, payload }),
                ],
                b::Term::Return(r(2)),
            )];
            t.clone().verify(b::Limits::default()).unwrap();
            t.functions[0].blocks[0].instructions[2].op = b::Op::Variant {
                case: count,
                payload: vec![],
            };
            assert!(matches!(error(t), o::ErrorKind::InvalidCase { .. }));
        }
    }
    for (constructor, case, payload) in [
        (o::Constructor::Option(o::Type::I64), 0, vec![]),
        (o::Constructor::Option(o::Type::I64), 1, vec![r(0)]),
        (
            o::Constructor::Result(o::Type::I64, o::Type::Bool),
            0,
            vec![r(0)],
        ),
        (
            o::Constructor::Result(o::Type::Bool, o::Type::I64),
            1,
            vec![r(0)],
        ),
    ] {
        let mut t = table(&[o::Type::I64, c(0)], &[constructor], c(0));
        t.functions[0].blocks = vec![block(
            &[],
            vec![
                instruction(&[0], b::Op::I64(1)),
                instruction(&[1], b::Op::Variant { case, payload }),
            ],
            b::Term::Return(r(1)),
        )];
        t.verify(b::Limits::default()).unwrap();
    }
}

#[test]
fn records_calls_and_scalar_imports_use_the_verified_type_contract() {
    let mut t = table(&[o::Type::I64, n(0), o::Type::I64], &[], o::Type::I64);
    t.places.declarations.types.definitions.push(o::Definition {
        name: "Record".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Record(vec![o::Field {
            name: "field".into(),
            span: span(),
            ty: o::TypeUse {
                ty: o::Type::I64,
                span: span(),
            },
        }]),
    });
    let p = place(
        &mut t,
        2,
        p::Root::Value(r(1)),
        &[p::ProjectionKind::Field(0)],
    );
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(&[0], b::Op::I64(1)),
            instruction(
                &[1],
                b::Op::Record {
                    definition: m::DefinitionId(0),
                    fields: vec![r(0)],
                    initialization_order: vec![0],
                },
            ),
            instruction(&[2], b::Op::CopyPlace(p)),
        ],
        b::Term::Return(r(2)),
    )];
    t.verify(b::Limits::default()).unwrap();
    let mut t = table(&[o::Type::I64, o::Type::I64], &[], o::Type::I64);
    let mut f = t.places.declarations.functions[0].clone();
    f.name = "callee".into();
    t.places.declarations.functions.push(f);
    t.places.functions.push(t.places.functions[0].clone());
    t.places.declarations.imports.push(d::Import {
        abi: d::ImportAbi::Host,
        name: "host".into(),
        span: span(),
        parameters: vec![o::TypeUse {
            ty: o::Type::I64,
            span: span(),
        }],
        result: o::TypeUse {
            ty: o::Type::I64,
            span: span(),
        },
    });
    t.functions = vec![
        b::FunctionBody {
            task_scopes: Vec::new(),
            task_bindings: Vec::new(),
            loans: 0,
            prepared: 0,
            blocks: vec![block(
                &[],
                vec![
                    instruction(
                        &[0],
                        b::Op::Call {
                            function: m::FunctionId(1),
                            arguments: vec![],
                        },
                    ),
                    instruction(
                        &[1],
                        b::Op::Import {
                            import: m::ImportId(0),
                            arguments: vec![r(0)],
                        },
                    ),
                ],
                b::Term::Return(r(1)),
            )],
        },
        b::FunctionBody {
            task_scopes: Vec::new(),
            task_bindings: Vec::new(),
            loans: 0,
            prepared: 0,
            blocks: vec![block(
                &[],
                vec![instruction(&[0], b::Op::I64(7))],
                b::Term::Return(r(0)),
            )],
        },
    ];
    t.verify(b::Limits::default()).unwrap();
}

#[test]
fn structural_typing_deliberately_does_not_prove_move_or_loan_state() {
    let mut t = table(
        &[c(0), c(0), c(0)],
        &[o::Constructor::Builtin(o::Builtin::Text)],
        c(0),
    );
    let l = add_local(&mut t, c(0), d::LocalKind::Parameter);
    let p = place(&mut t, 0, p::Root::Local(d::LocalId(l)), &[]);
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(&[0], b::Op::MovePlace(p)),
            instruction(&[1], b::Op::Move(r(0))),
            instruction(&[2], b::Op::Move(r(0))),
        ],
        b::Term::Return(r(2)),
    )];
    // Value 0 has one dominating definition, but is consumed twice. The next
    // ownership-flow stage must reject this; TypedBodies grants no execution.
    t.clone().verify(b::Limits::default()).unwrap();
    t.functions[0].blocks[0].instructions[1].op = b::Op::Copy(r(0));
    assert_eq!(error(t), o::ErrorKind::NonCopyPlace(c(0)));
}

#[test]
fn scalar_unit_result_shapes_types_and_void_operations_are_checked() {
    let mut t = table(
        &[
            o::Type::I64,
            o::Type::I64,
            o::Type::I64,
            o::Type::Bool,
            o::Type::Bool,
            c(0),
        ],
        &[o::Constructor::Builtin(o::Builtin::Unit)],
        c(0),
    );
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(&[0], b::Op::I64(1)),
            instruction(&[1], b::Op::I64(2)),
            instruction(&[2], b::Op::Add(r(0), r(1))),
            instruction(&[3], b::Op::LessThan(r(0), r(2))),
            instruction(&[4], b::Op::EqualI64(r(1), r(2))),
            instruction(&[], b::Op::DropValue(r(2))),
            instruction(&[5], b::Op::Unit),
        ],
        b::Term::Return(r(5)),
    )];
    t.clone().verify(b::Limits::default()).unwrap();
    let mut bad = t.clone();
    bad.functions[0].blocks[0].instructions[2].op = b::Op::Add(r(3), r(1));
    assert!(matches!(
        error(bad),
        o::ErrorKind::ValueType {
            expected: o::Type::I64,
            actual: o::Type::Bool
        }
    ));
    let mut bad = t.clone();
    bad.functions[0].blocks[0].instructions[5].results = vec![r(0)];
    assert!(matches!(
        error(bad),
        o::ErrorKind::Mir(m::ErrorKind::Arity {
            expected: 0,
            actual: 1
        })
    ));
    let mut bad = t;
    bad.functions[0].blocks[0].instructions[6].results = vec![r(4)];
    assert!(matches!(
        error(bad),
        o::ErrorKind::ExpectedBuiltin {
            expected: o::Builtin::Unit,
            ..
        }
    ));
}

#[test]
fn all_metadata_cfg_and_instruction_budgets_are_cumulative_and_exact() {
    let t = loan_body();
    let v = t.clone().verify(b::Limits::default()).unwrap();
    for (kind, used) in [
        (m::LimitKind::Work, v.work_used()),
        (m::LimitKind::Allocations, v.requested_bytes()),
        (m::LimitKind::Elements, v.elements_used()),
    ] {
        let mut l = b::Limits::default();
        match kind {
            m::LimitKind::Work => l.places.ownership.unit.work = used,
            m::LimitKind::Allocations => l.places.ownership.unit.allocations = used,
            _ => l.places.ownership.unit.elements = used,
        }
        t.clone().verify(l).unwrap();
        match kind {
            m::LimitKind::Work => l.places.ownership.unit.work -= 1,
            m::LimitKind::Allocations => l.places.ownership.unit.allocations -= 1,
            _ => l.places.ownership.unit.elements -= 1,
        }
        assert!(
            matches!(error_with(t.clone(),l),o::ErrorKind::Mir(m::ErrorKind::Limit{kind:k,..}) if kind==k)
        );
    }
    let mut l = b::Limits::default();
    l.places.ownership.unit.blocks = 0;
    assert!(t.clone().verify(l).is_err());
    let mut l = b::Limits::default();
    l.places.ownership.unit.instructions = 0;
    assert!(t.clone().verify(l).is_err());
    assert!(matches!(
        error_with(
            t,
            b::Limits {
                loans: 0,
                ..Default::default()
            }
        ),
        o::ErrorKind::LoanLimit { .. }
    ));
}
fn error_with(t: b::BodyTable, l: b::Limits) -> o::ErrorKind {
    t.verify(l).unwrap_err().kind
}
