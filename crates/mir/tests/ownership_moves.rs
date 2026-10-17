use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, moves as a, places as p},
};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 64,
    }
}
fn at(i: u32) -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: i,
        end: i + 1,
    }
}
fn r(i: u32) -> m::Register {
    m::Register(i)
}
fn c(i: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(i))
}
fn instruction(results: &[u32], op: b::Op, position: u32) -> b::Instruction {
    b::Instruction {
        results: results.iter().copied().map(r).collect(),
        op,
        span: at(position),
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
            span: at(60),
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
                        path: "moves.lang".into(),
                        text: "x".repeat(64),
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
fn local(t: &mut b::BodyTable, ty: o::Type, kind: d::LocalKind) -> d::LocalId {
    let f = &mut t.places.declarations.functions[0];
    let id = d::LocalId(f.locals.len() as u32);
    f.locals.push(d::Local {
        name: format!("local{}", id.0),
        span: span(),
        kind,
        ty: o::TypeUse { ty, span: span() },
    });
    if kind == d::LocalKind::Parameter {
        f.parameters.push(id);
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
fn analyze(t: b::BodyTable) -> Result<a::MoveCheckedBodies, a::Error> {
    a::analyze(
        t.verify(b::Limits::default()).unwrap(),
        a::Limits::default(),
    )
}
fn text(t: &[o::Type], result: o::Type) -> b::BodyTable {
    table(t, &[o::Constructor::Builtin(o::Builtin::Text)], result)
}

#[test]
fn double_moves_are_rejected_with_use_and_possible_consumption_spans() {
    let mut t = text(&[c(0), c(0), c(0)], c(0));
    let l = local(&mut t, c(0), d::LocalKind::Parameter);
    let p = place(&mut t, 0, p::Root::Local(l), &[]);
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(&[0], b::Op::MovePlace(p), 1),
            instruction(&[1], b::Op::Move(r(0)), 2),
            instruction(&[2], b::Op::Move(r(0)), 3),
        ],
        b::Term::Return(r(2)),
    )];
    let error = analyze(t).unwrap_err();
    assert_eq!(
        error.kind,
        a::ErrorKind::Unavailable {
            slot: a::Slot::Value(r(0)),
            state: a::Availability::Vacant
        }
    );
    assert_eq!(error.primary, Some(at(3)));
    assert_eq!(error.secondary, Some(at(2)));
}

#[test]
fn local_initialization_whole_replacement_and_copy_use_follow_slot_state() {
    let mut t = text(&[c(0), c(0), o::Type::I64, c(0)], c(0));
    let source = local(&mut t, c(0), d::LocalKind::Parameter);
    let destination = local(&mut t, c(0), d::LocalKind::Mutable);
    let src = place(&mut t, 0, p::Root::Local(source), &[]);
    let dst = place(&mut t, 1, p::Root::Local(destination), &[]);
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(&[0], b::Op::MovePlace(src), 1),
            instruction(
                &[],
                b::Op::Initialize {
                    local: destination,
                    value: r(0),
                },
                2,
            ),
            instruction(&[1], b::Op::MovePlace(dst), 3),
            instruction(
                &[],
                b::Op::Replace {
                    place: dst,
                    value: r(1),
                },
                4,
            ),
            instruction(&[3], b::Op::MovePlace(dst), 5),
        ],
        b::Term::Return(r(3)),
    )];
    let v = analyze(t).unwrap();
    assert!(v.functions()[0].exits()[0].owners().is_empty());
    let mut t = table(&[o::Type::I64, o::Type::I64], &[], o::Type::I64);
    let l = local(&mut t, o::Type::I64, d::LocalKind::Mutable);
    let p = place(&mut t, 1, p::Root::Local(l), &[]);
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(&[0], b::Op::I64(7), 1),
            instruction(
                &[],
                b::Op::Initialize {
                    local: l,
                    value: r(0),
                },
                2,
            ),
            instruction(&[1], b::Op::CopyPlace(p), 3),
            instruction(&[], b::Op::DropValue(r(0)), 4),
        ],
        b::Term::Return(r(0)),
    )];
    analyze(t).unwrap();
}

#[test]
fn uninitialized_reads_borrows_and_repeated_initialization_fail() {
    let mut t = table(
        &[o::Type::I64, o::Type::I64, c(0)],
        &[o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64)],
        o::Type::I64,
    );
    let l = local(&mut t, o::Type::I64, d::LocalKind::Mutable);
    let p = place(&mut t, 1, p::Root::Local(l), &[]);
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(&[0], b::Op::I64(7), 1),
            instruction(&[1], b::Op::CopyPlace(p), 2),
        ],
        b::Term::Return(r(1)),
    )];
    assert!(matches!(
        analyze(t.clone()).unwrap_err().kind,
        a::ErrorKind::Unavailable {
            slot: a::Slot::Local(_),
            ..
        }
    ));
    let mut bad = t.clone();
    bad.functions[0].loans = 1;
    bad.functions[0].blocks[0].instructions[1] = instruction(
        &[2],
        b::Op::Borrow {
            loan: b::LoanId(0),
            kind: o::LoanKind::Shared,
            place: p,
        },
        2,
    );
    bad.functions[0].blocks[0].terminator.kind = b::Term::Return(r(0));
    assert!(analyze(bad).is_err());
    let mut bad = t;
    bad.functions[0].blocks[0].instructions = vec![
        instruction(&[0], b::Op::I64(7), 1),
        instruction(
            &[],
            b::Op::Initialize {
                local: l,
                value: r(0),
            },
            2,
        ),
        instruction(
            &[],
            b::Op::Initialize {
                local: l,
                value: r(0),
            },
            3,
        ),
    ];
    bad.functions[0].blocks[0].terminator.kind = b::Term::Return(r(0));
    assert_eq!(
        analyze(bad).unwrap_err().kind,
        a::ErrorKind::InitializeLive(a::Slot::Local(l))
    );
}

fn branch_owner(read_after: bool) -> b::BodyTable {
    let mut t = text(&[o::Type::Bool, c(0), c(0), o::Type::I64], o::Type::I64);
    let l = local(&mut t, c(0), d::LocalKind::Parameter);
    let p = place(&mut t, 1, p::Root::Local(l), &[]);
    let mut joined = vec![];
    if read_after {
        joined.push(instruction(&[2], b::Op::MovePlace(p), 8));
        joined.push(instruction(&[], b::Op::DropValue(r(2)), 9));
    }
    joined.push(instruction(&[3], b::Op::I64(0), 10));
    t.functions[0].blocks = vec![
        block(
            &[],
            vec![instruction(&[0], b::Op::Bool(true), 1)],
            b::Term::Branch {
                condition: r(0),
                then_target: target(1, &[]),
                else_target: target(2, &[]),
            },
        ),
        block(
            &[],
            vec![
                instruction(&[1], b::Op::MovePlace(p), 2),
                instruction(&[], b::Op::DropValue(r(1)), 3),
            ],
            b::Term::Jump(target(3, &[])),
        ),
        block(&[], vec![], b::Term::Jump(target(3, &[]))),
        block(&[], joined, b::Term::Return(r(3))),
    ];
    t
}
#[test]
fn joins_require_definite_initialization_and_record_conditional_cleanup_obligations() {
    let v = analyze(branch_owner(false)).unwrap();
    assert_eq!(
        v.functions()[0].entry(m::BlockId(3), a::Slot::Local(d::LocalId(0))),
        Some(a::Availability::MaybeAvailable)
    );
    assert_eq!(
        v.functions()[0].exits()[0].owners(),
        [a::Obligation {
            slot: a::Slot::Local(d::LocalId(0)),
            state: a::Availability::MaybeAvailable
        }]
    );
    assert_eq!(
        analyze(branch_owner(true)).unwrap_err().kind,
        a::ErrorKind::Unavailable {
            slot: a::Slot::Local(d::LocalId(0)),
            state: a::Availability::MaybeAvailable
        }
    );
}

#[test]
fn branch_arguments_consume_only_the_chosen_edge_and_returned_owners_transfer() {
    let mut t = text(&[o::Type::Bool, c(0), c(0), c(0)], c(0));
    let l = local(&mut t, c(0), d::LocalKind::Parameter);
    let p = place(&mut t, 1, p::Root::Local(l), &[]);
    t.functions[0].blocks = vec![
        block(
            &[],
            vec![
                instruction(&[0], b::Op::Bool(true), 1),
                instruction(&[1], b::Op::MovePlace(p), 2),
            ],
            b::Term::Branch {
                condition: r(0),
                then_target: target(1, &[1]),
                else_target: target(2, &[1]),
            },
        ),
        block(&[2], vec![], b::Term::Return(r(2))),
        block(&[3], vec![], b::Term::Return(r(3))),
    ];
    let v = analyze(t).unwrap();
    assert!(
        v.functions()[0]
            .exits()
            .iter()
            .all(|e| e.owners().is_empty())
    );
}

#[test]
fn noncopy_self_edge_swaps_read_all_arguments_before_rebinding_parameters() {
    let mut t = text(&[c(0), c(0), c(0), c(0), o::Type::Bool, c(0), c(0)], c(0));
    let x = local(&mut t, c(0), d::LocalKind::Parameter);
    let y = local(&mut t, c(0), d::LocalKind::Parameter);
    let px = place(&mut t, 0, p::Root::Local(x), &[]);
    let py = place(&mut t, 1, p::Root::Local(y), &[]);
    t.functions[0].blocks = vec![
        block(
            &[],
            vec![
                instruction(&[0], b::Op::MovePlace(px), 1),
                instruction(&[1], b::Op::MovePlace(py), 2),
                instruction(&[4], b::Op::Bool(false), 3),
            ],
            b::Term::Jump(target(1, &[0, 1])),
        ),
        block(
            &[2, 3],
            vec![],
            b::Term::Branch {
                condition: r(4),
                then_target: target(1, &[3, 2]),
                else_target: target(2, &[2, 3]),
            },
        ),
        block(
            &[5, 6],
            vec![instruction(&[], b::Op::DropValue(r(6)), 4)],
            b::Term::Return(r(5)),
        ),
    ];
    analyze(t.clone()).unwrap();
    if let b::Term::Branch { then_target, .. } = &mut t.functions[0].blocks[1].terminator.kind {
        then_target.arguments = vec![r(2), r(2)];
    }
    assert!(analyze(t).is_err());
}

fn owner_loop(restore: bool) -> b::BodyTable {
    let mut t = text(&[c(0), c(0), o::Type::Bool, o::Type::I64], o::Type::I64);
    let x = local(&mut t, c(0), d::LocalKind::Parameter);
    let y = local(&mut t, c(0), d::LocalKind::Mutable);
    let px = place(&mut t, 0, p::Root::Local(x), &[]);
    let py = place(&mut t, 1, p::Root::Local(y), &[]);
    t.functions[0].blocks = vec![
        block(
            &[],
            vec![
                instruction(&[0], b::Op::MovePlace(px), 1),
                instruction(
                    &[],
                    b::Op::Initialize {
                        local: y,
                        value: r(0),
                    },
                    2,
                ),
                instruction(&[2], b::Op::Bool(false), 3),
                instruction(&[3], b::Op::I64(0), 4),
            ],
            b::Term::Jump(target(1, &[])),
        ),
        block(
            &[],
            vec![],
            b::Term::Branch {
                condition: r(2),
                then_target: target(2, &[]),
                else_target: target(3, &[]),
            },
        ),
        block(
            &[],
            vec![
                instruction(&[1], b::Op::MovePlace(py), 5),
                instruction(
                    &[],
                    if restore {
                        b::Op::Replace {
                            place: py,
                            value: r(1),
                        }
                    } else {
                        b::Op::DropValue(r(1))
                    },
                    6,
                ),
            ],
            b::Term::Jump(target(1, &[])),
        ),
        block(
            &[],
            vec![instruction(&[], b::Op::DropLocal(y), 7)],
            b::Term::Return(r(3)),
        ),
    ];
    t
}
#[test]
fn loop_feedback_restores_local_and_owner_availability() {
    analyze(owner_loop(true)).unwrap();
    assert!(matches!(
        analyze(owner_loop(false)).unwrap_err().kind,
        a::ErrorKind::LoopState {
            header: m::BlockId(1),
            slot: a::Slot::Local(d::LocalId(1)),
            ..
        }
    ));
    let mut t = table(&[o::Type::I64, o::Type::Bool], &[], o::Type::I64);
    let l = local(&mut t, o::Type::I64, d::LocalKind::Mutable);
    let p = place(&mut t, 0, p::Root::Local(l), &[]);
    t.functions[0].blocks = vec![
        block(
            &[],
            vec![
                instruction(&[0], b::Op::I64(0), 1),
                instruction(&[1], b::Op::Bool(false), 2),
            ],
            b::Term::Jump(target(1, &[])),
        ),
        block(
            &[],
            vec![],
            b::Term::Branch {
                condition: r(1),
                then_target: target(2, &[]),
                else_target: target(3, &[]),
            },
        ),
        block(
            &[],
            vec![instruction(
                &[],
                b::Op::Replace {
                    place: p,
                    value: r(0),
                },
                3,
            )],
            b::Term::Jump(target(1, &[])),
        ),
        block(&[], vec![], b::Term::Return(r(0))),
    ];
    assert!(matches!(
        analyze(t).unwrap_err().kind,
        a::ErrorKind::LoopState {
            expected: a::Availability::Vacant,
            actual: a::Availability::Available,
            ..
        }
    ));
}

#[test]
fn irreducible_cycles_are_analyzed_without_assuming_a_single_loop_entry() {
    let mut t = table(&[o::Type::Bool, o::Type::I64], &[], o::Type::I64);
    t.functions[0].blocks = vec![
        block(
            &[],
            vec![
                instruction(&[0], b::Op::Bool(false), 1),
                instruction(&[1], b::Op::I64(0), 2),
            ],
            b::Term::Branch {
                condition: r(0),
                then_target: target(1, &[]),
                else_target: target(2, &[]),
            },
        ),
        block(&[], vec![], b::Term::Jump(target(3, &[]))),
        block(&[], vec![], b::Term::Jump(target(3, &[]))),
        block(
            &[],
            vec![],
            b::Term::Branch {
                condition: r(0),
                then_target: target(1, &[]),
                else_target: target(2, &[]),
            },
        ),
    ];
    analyze(t).unwrap();
}

#[test]
fn generated_local_sequences_match_an_independent_single_owner_oracle() {
    for seed in 0..160u64 {
        let mut random = seed + 1;
        let mut t = table(
            &[c(0), c(0), c(0), o::Type::I64],
            &[
                o::Constructor::Builtin(o::Builtin::Text),
                o::Constructor::Loan(o::LoanKind::Shared, c(0)),
            ],
            o::Type::I64,
        );
        let source = local(&mut t, c(0), d::LocalKind::Parameter);
        let destination = local(&mut t, c(0), d::LocalKind::Mutable);
        let src = place(&mut t, 0, p::Root::Local(source), &[]);
        let dst = place(&mut t, 1, p::Root::Local(destination), &[]);
        let mut ops = vec![
            instruction(&[2], b::Op::MovePlace(src), 1),
            instruction(
                &[],
                b::Op::Initialize {
                    local: destination,
                    value: r(2),
                },
                2,
            ),
            instruction(&[3], b::Op::I64(0), 3),
        ];
        let (mut initialized, mut value_live, mut valid, mut latest, mut loans) =
            (true, false, true, 2u32, 0u32);
        for step in 0..8u32 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            match (random >> 32) % 4 {
                0 => {
                    valid &= initialized;
                    initialized = false;
                    value_live = true;
                    latest = t.places.functions[0].values.len() as u32;
                    t.places.functions[0].values.push(o::TypeUse {
                        ty: c(0),
                        span: span(),
                    });
                    ops.push(instruction(&[latest], b::Op::MovePlace(dst), step + 10));
                }
                1 => {
                    valid &= value_live;
                    value_live = false;
                    initialized = true;
                    ops.push(instruction(
                        &[],
                        b::Op::Replace {
                            place: dst,
                            value: r(latest),
                        },
                        step + 10,
                    ));
                }
                2 => {
                    valid &= initialized;
                    let reg = t.places.functions[0].values.len() as u32;
                    t.places.functions[0].values.push(o::TypeUse {
                        ty: c(1),
                        span: span(),
                    });
                    ops.push(instruction(
                        &[reg],
                        b::Op::Borrow {
                            loan: b::LoanId(loans),
                            kind: o::LoanKind::Shared,
                            place: dst,
                        },
                        step + 10,
                    ));
                    ops.push(instruction(
                        &[],
                        b::Op::EndBorrow {
                            loan: b::LoanId(loans),
                            mode: b::EndMode::Required,
                        },
                        step + 30,
                    ));
                    loans += 1;
                }
                _ => {
                    valid &= initialized;
                    initialized = false;
                    ops.push(instruction(&[], b::Op::DropLocal(destination), step + 10));
                }
            }
        }
        t.functions[0].loans = loans as usize;
        t.functions[0].blocks = vec![block(&[], ops, b::Term::Return(r(3)))];
        assert_eq!(analyze(t).is_ok(), valid, "seed {seed}");
    }
}

#[test]
fn indirect_access_requires_the_reference_value_and_root_cell_to_be_available() {
    let mut t = table(
        &[o::Type::I64, c(0), o::Type::I64],
        &[o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64)],
        o::Type::I64,
    );
    let l = local(&mut t, o::Type::I64, d::LocalKind::Mutable);
    let p = place(&mut t, 0, p::Root::Local(l), &[]);
    let referent = place(
        &mut t,
        2,
        p::Root::Value(r(1)),
        &[p::ProjectionKind::Dereference],
    );
    t.functions[0].loans = 1;
    t.functions[0].blocks = vec![block(
        &[],
        vec![
            instruction(&[0], b::Op::I64(7), 1),
            instruction(
                &[],
                b::Op::Initialize {
                    local: l,
                    value: r(0),
                },
                2,
            ),
            instruction(
                &[1],
                b::Op::Borrow {
                    loan: b::LoanId(0),
                    kind: o::LoanKind::Shared,
                    place: p,
                },
                3,
            ),
            instruction(&[], b::Op::DropValue(r(1)), 4),
            instruction(&[2], b::Op::CopyPlace(referent), 5),
        ],
        b::Term::Return(r(2)),
    )];
    assert!(matches!(
        analyze(t).unwrap_err().kind,
        a::ErrorKind::Unavailable {
            slot: a::Slot::Value(m::Register(1)),
            ..
        }
    ));
}

#[test]
fn arena_activation_state_is_tracked_without_claiming_region_reference_safety() {
    let mut t = table(
        &[c(2), c(1), o::Type::I64],
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
    let owner = local(&mut t, c(0), d::LocalKind::ArenaOwner);
    let allocator = local(&mut t, c(2), d::LocalKind::Parameter);
    let p = place(
        &mut t,
        1,
        p::Root::Local(allocator),
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
                1,
            ),
            instruction(
                &[],
                b::Op::ArenaEnter {
                    owner,
                    allocator: r(0),
                },
                2,
            ),
            instruction(&[], b::Op::ArenaExit { owner }, 3),
            instruction(&[2], b::Op::I64(0), 4),
        ],
        b::Term::Return(r(2)),
    )];
    analyze(t.clone()).unwrap();
    t.functions[0].blocks[0].instructions[2].op = b::Op::ArenaEnter {
        owner,
        allocator: r(0),
    };
    assert_eq!(
        analyze(t).unwrap_err().kind,
        a::ErrorKind::InitializeLive(a::Slot::Local(owner))
    );
}

#[test]
fn flow_limits_resume_typed_usage_and_bound_state_matrices_exactly() {
    let t = owner_loop(true).verify(b::Limits::default()).unwrap();
    let v = a::analyze(t.clone(), a::Limits::default()).unwrap();
    assert!(v.work_used() > t.work_used());
    for (kind, used) in [
        (m::LimitKind::Work, v.work_used()),
        (m::LimitKind::Allocations, v.requested_bytes()),
        (m::LimitKind::Elements, v.elements_used()),
    ] {
        let mut l = a::Limits::default();
        match kind {
            m::LimitKind::Work => l.work = used,
            m::LimitKind::Allocations => l.allocations = used,
            _ => l.elements = used,
        }
        a::analyze(t.clone(), l).unwrap();
        match kind {
            m::LimitKind::Work => l.work -= 1,
            m::LimitKind::Allocations => l.allocations -= 1,
            _ => l.elements -= 1,
        }
        assert!(
            matches!(a::analyze(t.clone(),l).unwrap_err().kind,a::ErrorKind::Resource(m::ErrorKind::Limit{kind:k,..}) if k==kind)
        );
    }
    assert!(matches!(
        a::analyze(
            t.clone(),
            a::Limits {
                work: t.work_used() - 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        a::ErrorKind::Resource(m::ErrorKind::Limit {
            kind: m::LimitKind::Work,
            ..
        })
    ));
    assert!(matches!(
        a::analyze(
            t,
            a::Limits {
                state_cells: 0,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        a::ErrorKind::StateCells { .. }
    ));
}
