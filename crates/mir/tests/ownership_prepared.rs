use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, moves as f, places as p},
};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 256,
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
fn id() -> b::PreparedPlaceId {
    b::PreparedPlaceId(0)
}
fn instruction(results: &[u32], op: b::Op, n: u32) -> b::Instruction {
    b::Instruction {
        results: results.iter().copied().map(r).collect(),
        op,
        span: at(n),
    }
}
fn prepare() -> b::Instruction {
    instruction(
        &[],
        b::Op::PreparePlace {
            destination: id(),
            place: p::PlaceId(0),
        },
        10,
    )
}
fn replace() -> b::Instruction {
    instruction(
        &[],
        b::Op::ReplacePrepared {
            destination: id(),
            value: r(1),
        },
        11,
    )
}
fn discard() -> b::Instruction {
    instruction(&[], b::Op::DiscardPrepared(id()), 12)
}
fn jump(n: u32) -> b::Term {
    b::Term::Jump(m::Target {
        block: m::BlockId(n),
        arguments: vec![],
    })
}
fn branch(a: u32, z: u32) -> b::Term {
    b::Term::Branch {
        condition: r(2),
        then_target: m::Target {
            block: m::BlockId(a),
            arguments: vec![],
        },
        else_target: m::Target {
            block: m::BlockId(z),
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
            span: at(240),
        },
        span: span(),
    }
}
fn entry(mut instructions: Vec<b::Instruction>, term: b::Term) -> b::Block {
    let mut prefix = vec![
        instruction(&[0], b::Op::I64(1), 1),
        instruction(&[1], b::Op::I64(2), 2),
        instruction(&[2], b::Op::Bool(true), 3),
    ];
    prefix.append(&mut instructions);
    block(prefix, term)
}
fn table(instructions: Vec<b::Instruction>) -> b::BodyTable {
    b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "prepared.lang".into(),
                        text: "x".repeat(256),
                    }],
                    definitions: vec![],
                    constructed: vec![],
                },
                imports: vec![],
                functions: vec![d::Function {
                    name: "f".into(),
                    public: false,
                    span: span(),
                    signature_span: span(),
                    parameters: vec![],
                    result: o::TypeUse {
                        ty: o::Type::I64,
                        span: span(),
                    },
                    locals: vec![d::Local {
                        name: "x".into(),
                        span: span(),
                        kind: d::LocalKind::Mutable,
                        ty: o::TypeUse {
                            ty: o::Type::I64,
                            span: span(),
                        },
                    }],
                }],
            },
            functions: vec![p::FunctionPlaces {
                values: [o::Type::I64, o::Type::I64, o::Type::Bool]
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
            loans: 0,
            prepared: 1,
            blocks: vec![entry(instructions, b::Term::Return(r(1)))],
        }],
    }
}
fn typed(t: b::BodyTable) -> Result<b::TypedBodies, o::Error> {
    t.verify(b::Limits::default())
}
fn moved(t: b::BodyTable) -> Result<f::MoveCheckedBodies, f::Error> {
    f::analyze(typed(t).unwrap(), f::Limits::default())
}

#[test]
fn captured_whole_local_reinitializes_storage_and_keeps_separate_identity() {
    let mut t = table(vec![prepare(), replace()]);
    // Inspect the vacant destination and live capture at a block boundary.
    t.functions[0].blocks = vec![
        entry(vec![prepare()], jump(1)),
        block(vec![replace()], b::Term::Return(r(1))),
    ];
    let out = moved(t).unwrap();
    let fact = out.typed().facts()[0].prepared()[0].unwrap();
    assert_eq!(fact.place, p::PlaceId(0));
    assert_eq!(fact.span, at(10));
    assert_eq!(fact.site.block, m::BlockId(0));
    let flow = &out.functions()[0];
    assert_eq!(
        flow.entry(m::BlockId(1), f::Slot::Local(d::LocalId(0))),
        Some(f::Availability::Vacant)
    );
    assert_eq!(
        flow.entry(m::BlockId(1), f::Slot::Prepared(id())),
        Some(f::Availability::Available)
    );
    assert_eq!(flow.entry(m::BlockId(1), f::Slot::Value(r(3))), None);
    assert_eq!(
        flow.entry(m::BlockId(1), f::Slot::Prepared(b::PreparedPlaceId(1))),
        None
    );
    assert_eq!(flow.consumption_span(f::Slot::Prepared(id())), Some(at(11)));
    assert!(flow.exits()[0].owners().is_empty());
    assert_eq!(
        f::analyze(out.clone().into_typed(), Default::default()).unwrap(),
        out
    );
}

#[test]
fn publication_and_discard_consume_a_capture_exactly_once() {
    for operations in [
        vec![prepare(), replace(), replace()],
        vec![prepare(), discard(), replace()],
        vec![prepare(), replace(), discard()],
    ] {
        let error = moved(table(operations)).unwrap_err();
        assert!(matches!(
            error.kind,
            f::ErrorKind::Unavailable {
                slot: f::Slot::Prepared(_),
                state: f::Availability::Vacant
            }
        ));
        assert!(error.secondary == Some(at(11)) || error.secondary == Some(at(12)));
    }
    moved(table(vec![prepare(), discard()])).unwrap();
    assert_eq!(
        moved(table(vec![prepare()])).unwrap_err().kind,
        f::ErrorKind::UnfinishedPrepared(id())
    );
}

#[test]
fn capture_id_definition_type_permission_and_result_shape_are_validated() {
    let mut t = table(vec![prepare(), replace()]);
    t.functions[0].prepared = 0;
    assert_eq!(typed(t).unwrap_err().kind, o::ErrorKind::InvalidPrepared(0));
    assert_eq!(
        typed(table(vec![replace()])).unwrap_err().kind,
        o::ErrorKind::UndefinedPrepared(0)
    );
    assert_eq!(
        typed(table(vec![prepare(), prepare(), discard()]))
            .unwrap_err()
            .kind,
        o::ErrorKind::DuplicatePrepared(0)
    );
    let mut t = table(vec![prepare(), replace()]);
    t.functions[0].blocks[0].instructions[4].op = b::Op::ReplacePrepared {
        destination: id(),
        value: r(2),
    };
    assert!(matches!(
        typed(t).unwrap_err().kind,
        o::ErrorKind::PlaceType { .. }
    ));
    let mut t = table(vec![prepare(), replace()]);
    t.places.declarations.functions[0].locals[0].kind = d::LocalKind::Parameter;
    t.places.declarations.functions[0]
        .parameters
        .push(d::LocalId(0));
    assert_eq!(typed(t).unwrap_err().kind, o::ErrorKind::ImmutablePlace);
    let mut t = table(vec![prepare(), discard()]);
    t.functions[0].blocks[0].instructions[3].results.push(r(0));
    assert!(matches!(
        typed(t).unwrap_err().kind,
        o::ErrorKind::Mir(m::ErrorKind::Arity {
            expected: 0,
            actual: 1
        })
    ));
}

#[test]
fn capture_must_precede_and_dominate_every_publication_or_discard() {
    for op in [replace(), discard()] {
        assert_eq!(
            typed(table(vec![op.clone(), prepare()])).unwrap_err().kind,
            o::ErrorKind::PreparedDoesNotDominate(0)
        );
        let mut t = table(vec![]);
        t.functions[0].blocks = vec![
            entry(vec![], branch(1, 2)),
            block(vec![prepare(), discard()], b::Term::Return(r(1))),
            block(vec![op], b::Term::Return(r(1))),
        ];
        assert_eq!(
            typed(t).unwrap_err().kind,
            o::ErrorKind::PreparedDoesNotDominate(0)
        );
    }
}

#[test]
fn both_branch_outcomes_must_finish_the_capture_and_only_selected_edge_consumes_it() {
    let mut t = table(vec![]);
    t.functions[0].blocks = vec![
        entry(vec![prepare()], branch(1, 2)),
        block(vec![replace()], jump(3)),
        block(vec![discard()], jump(3)),
        block(vec![], b::Term::Return(r(1))),
    ];
    let out = moved(t.clone()).unwrap();
    assert_eq!(
        out.functions()[0].entry(m::BlockId(3), f::Slot::Prepared(id())),
        Some(f::Availability::Vacant)
    );
    t.functions[0].blocks[2].instructions.clear();
    assert_eq!(
        moved(t.clone()).unwrap_err().kind,
        f::ErrorKind::UnfinishedPrepared(id())
    );
    t.functions[0].blocks[3].instructions.push(replace());
    assert!(matches!(
        moved(t).unwrap_err().kind,
        f::ErrorKind::Unavailable {
            slot: f::Slot::Prepared(_),
            state: f::Availability::MaybeAvailable
        }
    ));
}

#[test]
fn loops_restore_capture_state_and_allow_a_pending_destination_around_rhs_iteration() {
    let mut t = table(vec![]);
    t.functions[0].blocks = vec![
        entry(
            vec![instruction(
                &[],
                b::Op::Initialize {
                    local: d::LocalId(0),
                    value: r(0),
                },
                4,
            )],
            jump(1),
        ),
        block(vec![prepare(), replace()], branch(1, 2)),
        block(vec![], b::Term::Return(r(1))),
    ];
    moved(t.clone()).unwrap();
    t.functions[0].blocks[1].instructions.pop();
    let error = moved(t).unwrap_err();
    assert!(matches!(
        error.kind,
        f::ErrorKind::LoopState {
            slot: f::Slot::Prepared(_),
            ..
        }
    ));
    let mut t = table(vec![]);
    t.functions[0].blocks = vec![
        entry(vec![prepare()], jump(1)),
        block(vec![], branch(1, 2)),
        block(vec![replace()], b::Term::Return(r(1))),
    ];
    // The destination is evaluated outside an RHS loop and stays pending,
    // unchanged, until the RHS finishes. Back-edges must preserve that state.
    moved(t).unwrap();
}

fn indirect() -> b::BodyTable {
    let mut t = table(vec![]);
    let loan = o::Type::Constructed(o::ConstructedId(0));
    t.places
        .declarations
        .types
        .constructed
        .push(o::Constructed {
            constructor: o::Constructor::Loan(o::LoanKind::Exclusive, o::Type::I64),
            span: span(),
        });
    t.places.declarations.functions[0].locals.push(d::Local {
        name: "reference".into(),
        span: span(),
        kind: d::LocalKind::Mutable,
        ty: o::TypeUse {
            ty: loan,
            span: span(),
        },
    });
    t.places.functions[0].values.extend([
        o::TypeUse {
            ty: loan,
            span: span(),
        },
        o::TypeUse {
            ty: o::Type::I64,
            span: span(),
        },
    ]);
    t.places.functions[0].places.push(p::Place {
        origin: r(4),
        root: p::Root::Local(d::LocalId(1)),
        projections: vec![p::Projection {
            kind: p::ProjectionKind::Dereference,
            span: span(),
        }],
        span: span(),
    });
    t.functions[0].loans = 1;
    t.functions[0].blocks = vec![entry(
        vec![
            instruction(
                &[],
                b::Op::Initialize {
                    local: d::LocalId(0),
                    value: r(0),
                },
                4,
            ),
            instruction(
                &[3],
                b::Op::Borrow {
                    loan: b::LoanId(0),
                    kind: o::LoanKind::Exclusive,
                    place: p::PlaceId(0),
                },
                5,
            ),
            instruction(
                &[],
                b::Op::Initialize {
                    local: d::LocalId(1),
                    value: r(3),
                },
                6,
            ),
            instruction(
                &[],
                b::Op::PreparePlace {
                    destination: id(),
                    place: p::PlaceId(1),
                },
                10,
            ),
            replace(),
        ],
        b::Term::Return(r(1)),
    )];
    t
}

#[test]
fn an_indirect_destination_reads_its_reference_at_capture_and_needs_later_loan_validation() {
    let mut t = indirect();
    moved(t.clone()).unwrap();
    t.functions[0].blocks[0].instructions.remove(5); // no reference-cell initialization
    assert!(matches!(
        moved(t).unwrap_err().kind,
        f::ErrorKind::Unavailable {
            slot: f::Slot::Local(d::LocalId(1)),
            ..
        }
    ));
    let mut t = indirect();
    t.functions[0].blocks[0]
        .instructions
        .insert(7, instruction(&[], b::Op::DropLocal(d::LocalId(1)), 9));
    // Shape/move evidence only: publication must use the captured path rather
    // than reload this now-vacant cell. Its dependencies are not yet proved.
    let out = moved(t).unwrap();
    let op = &out.typed().functions()[0].blocks[0].instructions[8].op;
    assert_eq!(op.place(), None);
    assert_eq!(op.operands().collect::<Vec<_>>(), vec![r(1)]);
}

#[test]
fn value_roots_are_uses_at_capture_not_at_publication() {
    let mut t = indirect();
    t.places.functions[0].places[1].root = p::Root::Value(r(3));
    // Keep the loan SSA live instead of moving it into local reference storage.
    t.functions[0].blocks[0].instructions.remove(5);
    moved(t.clone()).unwrap();
    t.functions[0].blocks[0].instructions.swap(4, 5);
    assert_eq!(
        typed(t).unwrap_err().kind,
        o::ErrorKind::Mir(m::ErrorKind::DoesNotDominate(3))
    );
}

#[test]
fn all_eighty_one_diamonds_agree_with_an_independent_path_consumption_oracle() {
    for code in 0..81usize {
        let mut digits = code;
        let actions: [usize; 4] = std::array::from_fn(|_| {
            let a = digits % 3;
            digits /= 3;
            a
        });
        let path_valid = |path: &[usize]| {
            let mut pending = true;
            for action in path {
                if *action != 0 {
                    if !pending {
                        return false;
                    }
                    pending = false;
                }
            }
            !pending
        };
        let expected = path_valid(&actions[..2]) && path_valid(&actions[2..]);
        let ops = |path: &[usize]| {
            path.iter()
                .filter_map(|a| match a {
                    1 => Some(replace()),
                    2 => Some(discard()),
                    _ => None,
                })
                .collect()
        };
        let mut t = table(vec![]);
        t.functions[0].blocks = vec![
            entry(vec![prepare()], branch(1, 2)),
            block(ops(&actions[..2]), jump(3)),
            block(ops(&actions[2..]), jump(3)),
            block(vec![], b::Term::Return(r(1))),
        ];
        assert_eq!(moved(t).is_ok(), expected, "graph {code}: {actions:?}");
    }
}

#[test]
fn capture_limits_and_state_accounting_are_cumulative_and_replayable() {
    let t = table(vec![prepare(), replace()]);
    let err = t
        .clone()
        .verify(b::Limits {
            prepared: 0,
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(
        err.kind,
        o::ErrorKind::PreparedLimit {
            requested: 1,
            limit: 0
        }
    );
    let mut pair = t.clone();
    let mut declaration = pair.places.declarations.functions[0].clone();
    declaration.name = "g".into();
    pair.places.declarations.functions.push(declaration);
    pair.places.functions.push(pair.places.functions[0].clone());
    pair.functions.push(pair.functions[0].clone());
    assert_eq!(
        pair.verify(b::Limits {
            prepared: 1,
            ..Default::default()
        })
        .unwrap_err()
        .kind,
        o::ErrorKind::PreparedLimit {
            requested: 2,
            limit: 1
        }
    );
    let checked = typed(t).unwrap();
    let out = f::analyze(checked.clone(), Default::default()).unwrap();
    for dimension in 0..3 {
        let mut l = f::Limits::default();
        match dimension {
            0 => l.work = out.work_used(),
            1 => l.allocations = out.requested_bytes(),
            _ => l.elements = out.elements_used(),
        }
        assert_eq!(f::analyze(checked.clone(), l).unwrap(), out);
        match dimension {
            0 => l.work -= 1,
            1 => l.allocations -= 1,
            _ => l.elements -= 1,
        }
        assert!(matches!(
            f::analyze(checked.clone(), l).unwrap_err().kind,
            f::ErrorKind::Resource(_)
        ));
    }
    assert!(matches!(
        f::analyze(
            checked,
            f::Limits {
                state_cells: 9,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        f::ErrorKind::StateCells {
            requested: 10,
            limit: 9
        }
    ));
}

#[test]
fn a_prepared_direct_field_requires_initialized_parent_storage_at_publication() {
    let mut t = table(vec![]);
    t.places.declarations.types.definitions.push(o::Definition {
        name: "Record".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Record(vec![o::Field {
            name: "value".into(),
            span: span(),
            ty: o::TypeUse {
                ty: o::Type::I64,
                span: span(),
            },
        }]),
    });
    let record = o::Type::Nominal(m::DefinitionId(0));
    t.places.declarations.functions[0].locals[0].ty.ty = record;
    t.places.functions[0].values.push(o::TypeUse {
        ty: record,
        span: span(),
    });
    t.places.functions[0].places[0]
        .projections
        .push(p::Projection {
            kind: p::ProjectionKind::Field(0),
            span: span(),
        });
    t.functions[0].blocks = vec![entry(
        vec![
            instruction(
                &[3],
                b::Op::Record {
                    definition: m::DefinitionId(0),
                    fields: vec![r(0)],
                },
                4,
            ),
            instruction(
                &[],
                b::Op::Initialize {
                    local: d::LocalId(0),
                    value: r(3),
                },
                5,
            ),
            prepare(),
            instruction(&[], b::Op::DropLocal(d::LocalId(0)), 6),
            replace(),
        ],
        b::Term::Return(r(1)),
    )];
    assert!(matches!(
        moved(t).unwrap_err().kind,
        f::ErrorKind::Unavailable {
            slot: f::Slot::Local(d::LocalId(0)),
            state: f::Availability::Vacant
        }
    ));
}
