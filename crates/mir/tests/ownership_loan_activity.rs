use lang_mir::unit::{
    self as m,
    ownership::{
        self as o, bodies as b, declarations as d, loan_activity as a, moves, places as p,
    },
};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 128,
    }
}
fn at(n: u32) -> m::Span {
    m::Span {
        start: n,
        end: n + 1,
        ..span()
    }
}
fn r(n: u32) -> m::Register {
    m::Register(n)
}
fn loan() -> b::LoanId {
    b::LoanId(0)
}
fn instruction(results: &[u32], op: b::Op, n: u32) -> b::Instruction {
    b::Instruction {
        results: results.iter().copied().map(r).collect(),
        op,
        span: at(n),
    }
}
fn begin() -> b::Instruction {
    instruction(
        &[1],
        b::Op::Borrow {
            loan: loan(),
            kind: o::LoanKind::Shared,
            place: p::PlaceId(0),
        },
        4,
    )
}
fn end(mode: b::EndMode, n: u32) -> b::Instruction {
    instruction(&[], b::Op::EndBorrow { loan: loan(), mode }, n)
}
fn jump(n: u32) -> b::Term {
    b::Term::Jump(m::Target {
        block: m::BlockId(n),
        arguments: vec![],
    })
}
fn branch(x: u32, y: u32) -> b::Term {
    b::Term::Branch {
        condition: r(3),
        then_target: m::Target {
            block: m::BlockId(x),
            arguments: vec![],
        },
        else_target: m::Target {
            block: m::BlockId(y),
            arguments: vec![],
        },
    }
}
fn block(instructions: Vec<b::Instruction>, term: b::Term) -> b::Block {
    b::Block {
        parameters: vec![],
        instructions,
        terminator: b::Terminator {
            kind: term,
            span: at(120),
        },
        span: span(),
    }
}
fn entry(mut instructions: Vec<b::Instruction>, term: b::Term) -> b::Block {
    let mut prefix = vec![
        instruction(&[0], b::Op::I64(7), 1),
        instruction(&[3], b::Op::Bool(true), 2),
    ];
    prefix.append(&mut instructions);
    block(prefix, term)
}
fn table(instructions: Vec<b::Instruction>) -> b::BodyTable {
    let reference = o::Type::Constructed(o::ConstructedId(0));
    b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "activity.lang".into(),
                        text: "x".repeat(128),
                    }],
                    definitions: vec![],
                    constructed: vec![o::Constructed {
                        constructor: o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
                        span: span(),
                    }],
                },
                imports: vec![],
                functions: vec![d::Function {
                    abi: d::FunctionAbi::Guest,
                    name: "f".into(),
                    public: false,
                    span: span(),
                    signature_span: span(),
                    parameters: vec![d::LocalId(0)],
                    result: o::TypeUse {
                        ty: o::Type::I64,
                        span: span(),
                    },
                    locals: vec![d::Local {
                        name: "value".into(),
                        span: span(),
                        kind: d::LocalKind::Parameter,
                        ty: o::TypeUse {
                            ty: o::Type::I64,
                            span: span(),
                        },
                    }],
                }],
            },
            functions: vec![p::FunctionPlaces {
                values: [
                    o::Type::I64,
                    reference,
                    reference,
                    o::Type::Bool,
                    o::Type::Bool,
                    o::Type::I64,
                ]
                .into_iter()
                .map(|ty| o::TypeUse { ty, span: span() })
                .collect(),
                places: vec![p::Place {
                    origin: r(0),
                    root: p::Root::Local(d::LocalId(0)),
                    projections: vec![],
                    span: span(),
                }],
            }],
        },
        functions: vec![b::FunctionBody {
            loans: 2,
            prepared: 0,
            blocks: vec![entry(instructions, b::Term::Return(r(0)))],
        }],
    }
}
fn moved(t: b::BodyTable) -> moves::MoveCheckedBodies {
    moves::analyze(t.verify(Default::default()).unwrap(), Default::default()).unwrap()
}
fn analyze(t: b::BodyTable) -> Result<a::ActivityBodies, a::Error> {
    a::analyze(moved(t), Default::default())
}

#[test]
fn explicit_ends_are_required_once_and_report_the_creation_span() {
    analyze(table(vec![begin(), end(b::EndMode::Required, 5)])).unwrap();
    let e = analyze(table(vec![
        begin(),
        end(b::EndMode::Required, 5),
        end(b::EndMode::Required, 6),
    ]))
    .unwrap_err();
    assert_eq!(
        e.kind,
        a::ErrorKind::NotActive {
            loan: loan(),
            state: a::Activity::Inactive
        }
    );
    assert_eq!(e.primary, Some(at(6)));
    assert_eq!(e.secondary, Some(at(4)));
    let e = analyze(table(vec![begin()])).unwrap_err();
    assert_eq!(
        e.kind,
        a::ErrorKind::Unended {
            loan: loan(),
            state: a::Activity::Active
        }
    );
    assert_eq!(e.primary, Some(at(120)));
    assert_eq!(e.secondary, Some(at(4)));
}

#[test]
fn panic_defers_active_loans_to_failure_retirement_only_without_a_success_exit() {
    let mut t = table(vec![begin(), instruction(&[], b::Op::Panic(r(0)), 9)]);
    t.functions[0].blocks[0].terminator.kind = b::Term::Unreachable;
    analyze(t.clone()).unwrap();

    // Merely changing a return into unreachable must not waive explicit ends.
    let mut bare = t.clone();
    bare.functions[0].blocks[0].instructions.pop();
    assert_eq!(
        analyze(bare).unwrap_err().kind,
        a::ErrorKind::Unended {
            loan: loan(),
            state: a::Activity::Active,
        }
    );

    // A panic cannot bypass normal-exit checks by retaining a success path.
    t.functions[0].blocks[0].terminator.kind = b::Term::Return(r(0));
    assert_eq!(
        t.verify(Default::default()).unwrap_err().kind,
        o::ErrorKind::InvalidPanicContinuation
    );
}

#[test]
fn conditional_cleanup_is_idempotent_but_does_not_make_a_later_required_end_valid() {
    analyze(table(vec![
        end(b::EndMode::IfActive, 3),
        begin(),
        end(b::EndMode::Required, 5),
    ]))
    .unwrap();
    analyze(table(vec![
        begin(),
        end(b::EndMode::IfActive, 5),
        end(b::EndMode::IfActive, 6),
    ]))
    .unwrap();
    assert_eq!(
        analyze(table(vec![
            begin(),
            end(b::EndMode::IfActive, 5),
            end(b::EndMode::Required, 6)
        ]))
        .unwrap_err()
        .kind,
        a::ErrorKind::NotActive {
            loan: loan(),
            state: a::Activity::Inactive
        }
    );
}

#[test]
fn joins_retain_maybe_active_state_and_require_conditional_cleanup() {
    let mut t = table(vec![]);
    t.functions[0].blocks = vec![
        entry(vec![begin()], branch(1, 2)),
        block(vec![end(b::EndMode::Required, 5)], jump(3)),
        block(vec![], jump(3)),
        block(vec![end(b::EndMode::IfActive, 6)], b::Term::Return(r(0))),
    ];
    let out = analyze(t.clone()).unwrap();
    assert_eq!(
        out.functions()[0].entry(m::BlockId(3), loan()),
        Some(a::Activity::MaybeActive)
    );
    assert_eq!(out.functions()[0].entry(m::BlockId(0), b::LoanId(2)), None);
    assert_eq!(out.functions()[0].entry(m::BlockId(4), loan()), None);
    t.functions[0].blocks[3].instructions = vec![end(b::EndMode::Required, 6)];
    assert_eq!(
        analyze(t.clone()).unwrap_err().kind,
        a::ErrorKind::NotActive {
            loan: loan(),
            state: a::Activity::MaybeActive
        }
    );
    t.functions[0].blocks[3].instructions.clear();
    assert_eq!(
        analyze(t).unwrap_err().kind,
        a::ErrorKind::Unended {
            loan: loan(),
            state: a::Activity::MaybeActive
        }
    );
}

#[test]
fn a_loan_created_only_on_one_arm_can_be_conditionally_ended_after_the_join() {
    let mut t = table(vec![]);
    t.functions[0].blocks = vec![
        entry(vec![], branch(1, 2)),
        block(vec![begin()], jump(3)),
        block(vec![], jump(3)),
        block(vec![end(b::EndMode::IfActive, 6)], b::Term::Return(r(0))),
    ];
    let out = analyze(t).unwrap();
    assert_eq!(
        out.functions()[0].entry(m::BlockId(3), loan()),
        Some(a::Activity::MaybeActive)
    );
}

#[test]
fn loops_restore_activation_state_and_allow_an_outer_loan_to_remain_active() {
    let mut t = table(vec![]);
    t.functions[0].blocks = vec![
        entry(vec![], jump(1)),
        block(vec![begin(), end(b::EndMode::Required, 5)], branch(1, 2)),
        block(vec![], b::Term::Return(r(0))),
    ];
    analyze(t.clone()).unwrap();
    t.functions[0].blocks[1].instructions.pop();
    let e = analyze(t).unwrap_err();
    assert!(matches!(
        e.kind,
        a::ErrorKind::LoopState {
            loan: b::LoanId(0),
            expected: a::Activity::Inactive,
            actual: a::Activity::Active,
            ..
        }
    ));
    let mut t = table(vec![]);
    t.functions[0].blocks = vec![
        entry(vec![begin()], jump(1)),
        block(vec![], branch(1, 2)),
        block(vec![end(b::EndMode::Required, 6)], b::Term::Return(r(0))),
    ];
    let out = analyze(t).unwrap();
    assert_eq!(
        out.functions()[0].entry(m::BlockId(1), loan()),
        Some(a::Activity::Active)
    );
}

#[test]
fn irreducible_cycles_restore_loan_state_without_requiring_a_natural_loop_header() {
    let mut t = table(vec![]);
    t.functions[0].blocks = vec![
        entry(vec![], branch(1, 2)),
        block(vec![begin(), end(b::EndMode::Required, 5)], jump(2)),
        block(vec![], branch(1, 3)),
        block(vec![], b::Term::Return(r(0))),
    ];
    analyze(t.clone()).unwrap();
    t.functions[0].blocks[1].instructions.pop();
    assert!(matches!(
        analyze(t).unwrap_err().kind,
        a::ErrorKind::LoopState { .. }
    ));
}

#[test]
fn guarded_case_payload_borrows_use_the_same_activation_state_machine() {
    let mut t = table(vec![]);
    t.places
        .declarations
        .types
        .constructed
        .push(o::Constructed {
            constructor: o::Constructor::Option(o::Type::I64),
            span: span(),
        });
    t.places.functions[0].values.push(o::TypeUse {
        ty: o::Type::Constructed(o::ConstructedId(1)),
        span: span(),
    });
    t.functions[0].blocks = vec![
        entry(
            vec![
                instruction(
                    &[6],
                    b::Op::Variant {
                        case: 1,
                        payload: vec![r(0)],
                    },
                    3,
                ),
                instruction(
                    &[4],
                    b::Op::IsCase {
                        value: r(6),
                        case: 1,
                    },
                    4,
                ),
            ],
            b::Term::Branch {
                condition: r(4),
                then_target: m::Target {
                    block: m::BlockId(1),
                    arguments: vec![],
                },
                else_target: m::Target {
                    block: m::BlockId(2),
                    arguments: vec![],
                },
            },
        ),
        block(
            vec![
                instruction(
                    &[1],
                    b::Op::BorrowCaseField {
                        loan: loan(),
                        value: r(6),
                        case: 1,
                        field: 0,
                        guard: r(4),
                    },
                    5,
                ),
                end(b::EndMode::Required, 6),
            ],
            b::Term::Return(r(0)),
        ),
        block(vec![], b::Term::Return(r(0))),
    ];
    analyze(t.clone()).unwrap();
    t.functions[0].blocks[1].instructions.pop();
    assert_eq!(
        analyze(t).unwrap_err().kind,
        a::ErrorKind::Unended {
            loan: loan(),
            state: a::Activity::Active
        }
    );
}

#[test]
fn all_eighty_one_diamonds_match_an_independent_concrete_path_end_oracle() {
    for code in 0..81usize {
        let mut n = code;
        let actions: [usize; 4] = std::array::from_fn(|_| {
            let a = n % 3;
            n /= 3;
            a
        });
        let valid = |path: &[usize]| {
            let mut active = true;
            for action in path {
                match action {
                    1 if !active => return false,
                    1 | 2 => active = false,
                    _ => {}
                }
            }
            !active
        };
        let expected = valid(&actions[..2]) && valid(&actions[2..]);
        let ops = |path: &[usize]| {
            path.iter()
                .enumerate()
                .filter_map(|(i, action)| match action {
                    1 => Some(end(b::EndMode::Required, 7 + i as u32)),
                    2 => Some(end(b::EndMode::IfActive, 7 + i as u32)),
                    _ => None,
                })
                .collect()
        };
        let mut t = table(vec![]);
        t.functions[0].blocks = vec![
            entry(vec![begin()], branch(1, 2)),
            block(ops(&actions[..2]), jump(3)),
            block(ops(&actions[2..]), jump(3)),
            block(vec![], b::Term::Return(r(0))),
        ];
        assert_eq!(analyze(t).is_ok(), expected, "graph {code}: {actions:?}");
    }
}

#[test]
fn exact_resource_limits_resume_move_usage_and_state_cells_accumulate_across_functions() {
    let t = table(vec![begin(), end(b::EndMode::Required, 5)]);
    let moved = moved(t.clone());
    let out = a::analyze(moved.clone(), Default::default()).unwrap();
    assert_eq!(
        a::analyze(out.clone().into_moved(), Default::default()).unwrap(),
        out
    );
    for dimension in 0..3 {
        let mut limits = a::Limits::default();
        match dimension {
            0 => limits.work = out.work_used(),
            1 => limits.allocations = out.requested_bytes(),
            _ => limits.elements = out.elements_used(),
        }
        assert_eq!(a::analyze(moved.clone(), limits).unwrap(), out);
        match dimension {
            0 => limits.work -= 1,
            1 => limits.allocations -= 1,
            _ => limits.elements -= 1,
        }
        assert!(matches!(
            a::analyze(moved.clone(), limits).unwrap_err().kind,
            a::ErrorKind::Resource(_)
        ));
    }
    assert!(matches!(
        a::analyze(
            moved.clone(),
            a::Limits {
                work: moved.work_used() - 1,
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
    assert_eq!(
        a::analyze(
            moved,
            a::Limits {
                state_cells: 3,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        a::ErrorKind::StateCells {
            requested: 4,
            limit: 3
        }
    );
    let mut pair = t.clone();
    let mut declaration = pair.places.declarations.functions[0].clone();
    declaration.name = "g".into();
    pair.places.declarations.functions.push(declaration);
    pair.places.functions.push(pair.places.functions[0].clone());
    pair.functions.push(pair.functions[0].clone());
    assert_eq!(
        a::analyze(
            self::moved(pair),
            a::Limits {
                state_cells: 7,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        a::ErrorKind::StateCells {
            requested: 8,
            limit: 7
        }
    );
}

#[test]
fn activity_facts_do_not_promote_an_ended_reference_or_implicit_parameter_loan_to_safe_access() {
    let mut t = table(vec![]);
    t.places.functions[0].places.push(p::Place {
        origin: r(5),
        root: p::Root::Value(r(1)),
        projections: vec![p::Projection {
            kind: p::ProjectionKind::Dereference,
            span: span(),
        }],
        span: span(),
    });
    t.functions[0].blocks = vec![entry(
        vec![
            begin(),
            end(b::EndMode::Required, 5),
            instruction(&[5], b::Op::CopyPlace(p::PlaceId(1)), 6),
        ],
        b::Term::Return(r(5)),
    )];
    // Activation/end state alone grants no reference-liveness proof; the
    // subsequent provenance checker must reject the ended reference.
    let activity = analyze(t).unwrap();
    assert!(matches!(
        lang_mir::unit::ownership::loans::analyze(activity, Default::default())
            .unwrap_err()
            .kind,
        lang_mir::unit::ownership::loans::ErrorKind::EndedReference(_)
    ));
    let mut t = table(vec![]);
    t.functions[0].loans = 0;
    let out = analyze(t).unwrap();
    assert_eq!(out.functions()[0].entry(m::BlockId(0), loan()), None);
}
