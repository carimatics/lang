use lang_mir::unit::{
    self as m,
    ownership::{
        self as o, bodies as b, declarations as d, loan_activity as a, loans as l, moves,
        places as p,
    },
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
fn i(results: &[m::Register], op: b::Op, n: u32) -> b::Instruction {
    b::Instruction {
        results: results.to_vec(),
        op,
        span: at(n),
    }
}
fn block(instructions: Vec<b::Instruction>, term: b::Term) -> b::Block {
    b::Block {
        parameters: vec![],
        instructions,
        terminator: b::Terminator {
            kind: term,
            span: at(250),
        },
        span: span(),
    }
}
fn entry(mut ops: Vec<b::Instruction>, term: b::Term) -> b::Block {
    let mut prefix = vec![
        i(&[r(0)], b::Op::I64(7), 1),
        i(&[r(1)], b::Op::I64(11), 2),
        i(&[r(2)], b::Op::Bool(true), 3),
        i(
            &[],
            b::Op::Initialize {
                local: d::LocalId(0),
                value: r(0),
            },
            4,
        ),
        i(
            &[],
            b::Op::Initialize {
                local: d::LocalId(1),
                value: r(1),
            },
            5,
        ),
    ];
    prefix.append(&mut ops);
    block(prefix, term)
}
fn jump(to: u32, arguments: &[m::Register]) -> b::Term {
    b::Term::Jump(m::Target {
        block: m::BlockId(to),
        arguments: arguments.to_vec(),
    })
}
fn branch(x: u32, y: u32) -> b::Term {
    b::Term::Branch {
        condition: r(2),
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
fn base() -> b::BodyTable {
    b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "loans.lang".into(),
                        text: "x".repeat(256),
                    }],
                    definitions: vec![],
                    constructed: vec![],
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
                        ty: o::Type::I64,
                        span: span(),
                    },
                    locals: (0..2)
                        .map(|n| d::Local {
                            name: format!("local{n}"),
                            span: span(),
                            kind: d::LocalKind::Mutable,
                            ty: o::TypeUse {
                                ty: o::Type::I64,
                                span: span(),
                            },
                        })
                        .collect(),
                }],
            },
            functions: vec![p::FunctionPlaces {
                values: [o::Type::I64, o::Type::I64, o::Type::Bool]
                    .into_iter()
                    .map(|ty| o::TypeUse { ty, span: span() })
                    .collect(),
                places: vec![],
            }],
        },
        functions: vec![b::FunctionBody::default()],
    }
}
fn value(t: &mut b::BodyTable, ty: o::Type) -> m::Register {
    let v = &mut t.places.functions[0].values;
    let id = r(v.len() as u32);
    v.push(o::TypeUse { ty, span: span() });
    id
}
fn ty(t: &mut b::BodyTable, constructor: o::Constructor) -> o::Type {
    let v = &mut t.places.declarations.types.constructed;
    let id = o::Type::Constructed(o::ConstructedId(v.len() as u32));
    v.push(o::Constructed {
        constructor,
        span: span(),
    });
    id
}
fn local(t: &mut b::BodyTable, ty: o::Type, parameter: bool) -> d::LocalId {
    let f = &mut t.places.declarations.functions[0];
    let id = d::LocalId(f.locals.len() as u32);
    f.locals.push(d::Local {
        name: format!("local{}", id.0),
        span: span(),
        kind: if parameter {
            d::LocalKind::Parameter
        } else {
            d::LocalKind::Mutable
        },
        ty: o::TypeUse { ty, span: span() },
    });
    if parameter {
        f.parameters.push(id);
    }
    id
}
fn place(
    t: &mut b::BodyTable,
    root: p::Root,
    projections: &[p::ProjectionKind],
    result: o::Type,
) -> p::PlaceId {
    let origin = value(t, result);
    let v = &mut t.places.functions[0].places;
    let id = p::PlaceId(v.len() as u32);
    v.push(p::Place {
        origin,
        root,
        projections: projections
            .iter()
            .map(|kind| p::Projection {
                kind: *kind,
                span: span(),
            })
            .collect(),
        span: span(),
    });
    id
}
fn root(t: &mut b::BodyTable, local: u32, ty: o::Type) -> p::PlaceId {
    place(t, p::Root::Local(d::LocalId(local)), &[], ty)
}
fn borrow(
    t: &mut b::BodyTable,
    p: p::PlaceId,
    kind: o::LoanKind,
    target: o::Type,
    n: u32,
) -> (b::LoanId, m::Register, b::Instruction) {
    let ty = ty(t, o::Constructor::Loan(kind, target));
    let result = value(t, ty);
    let id = b::LoanId(t.functions[0].loans as u32);
    t.functions[0].loans += 1;
    (
        id,
        result,
        i(
            &[result],
            b::Op::Borrow {
                loan: id,
                kind,
                place: p,
            },
            n,
        ),
    )
}
fn end(id: b::LoanId, n: u32) -> b::Instruction {
    i(
        &[],
        b::Op::EndBorrow {
            loan: id,
            mode: b::EndMode::IfActive,
        },
        n,
    )
}
fn finish(t: &mut b::BodyTable, ops: Vec<b::Instruction>) {
    t.functions[0].blocks = vec![entry(ops, b::Term::Return(r(0)))];
}
fn activity(t: b::BodyTable) -> a::ActivityBodies {
    a::analyze(
        moves::analyze(t.verify(Default::default()).unwrap(), Default::default()).unwrap(),
        Default::default(),
    )
    .unwrap()
}
fn analyze(t: b::BodyTable) -> Result<l::LoanCheckedBodies, l::Error> {
    l::analyze(activity(t), Default::default())
}

#[test]
fn shared_reads_and_copy_moves_are_allowed_but_mutation_and_destruction_conflict() {
    for action in 0..4 {
        let mut t = base();
        let p = root(&mut t, 0, o::Type::I64);
        let (id, _, begin) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
        let copied = value(&mut t, o::Type::I64);
        let op = match action {
            0 => i(&[copied], b::Op::CopyPlace(p), 11),
            1 => i(&[copied], b::Op::MovePlace(p), 11),
            2 => i(
                &[],
                b::Op::Replace {
                    place: p,
                    value: r(1),
                },
                11,
            ),
            _ => i(&[], b::Op::DropLocal(d::LocalId(0)), 11),
        };
        finish(&mut t, vec![begin, op, end(id, 12)]);
        let out = analyze(t);
        assert_eq!(out.is_ok(), action < 2);
        if let Err(e) = out {
            assert!(matches!(
                e.kind,
                l::ErrorKind::Conflict {
                    owner: l::Owner::Local(d::LocalId(0)),
                    ..
                }
            ));
            assert_eq!(e.primary, Some(at(11)));
            assert_eq!(e.secondary, Some(at(10)));
        }
    }
    let mut t = base();
    let p = place(&mut t, p::Root::Value(r(0)), &[], o::Type::I64);
    let (id, _, begin) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
    finish(
        &mut t,
        vec![begin, i(&[], b::Op::DropValue(r(0)), 11), end(id, 12)],
    );
    analyze(t).unwrap();
}

#[test]
fn exclusive_access_requires_its_loan_and_ended_references_cannot_be_read_or_forwarded() {
    for ended in [false, true] {
        let mut t = base();
        let p = root(&mut t, 0, o::Type::I64);
        let (id, reference, begin) = borrow(&mut t, p, o::LoanKind::Exclusive, o::Type::I64, 10);
        let deref = place(
            &mut t,
            p::Root::Value(reference),
            &[p::ProjectionKind::Dereference],
            o::Type::I64,
        );
        let read = value(&mut t, o::Type::I64);
        let mut ops = vec![begin];
        if ended {
            ops.push(end(id, 11));
        }
        ops.push(i(&[read], b::Op::CopyPlace(deref), 12));
        ops.push(end(id, 13));
        finish(&mut t, ops);
        let out = analyze(t);
        assert_eq!(out.is_ok(), !ended);
        if let Err(e) = out {
            assert_eq!(e.kind, l::ErrorKind::EndedReference(l::ReferenceId(id.0)));
        }
    }
    let mut t = base();
    let p = root(&mut t, 0, o::Type::I64);
    let (id, reference, begin) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
    let reference_ty = t.places.functions[0].values[reference.0 as usize].ty;
    let forwarded = value(&mut t, reference_ty);
    finish(
        &mut t,
        vec![
            begin,
            end(id, 11),
            i(&[forwarded], b::Op::Move(reference), 12),
        ],
    );
    assert!(matches!(
        analyze(t).unwrap_err().kind,
        l::ErrorKind::EndedReference(_)
    ));
}

#[test]
fn reborrows_freeze_exclusive_access_and_prevent_ending_a_live_parent() {
    for action in 0..3 {
        let mut t = base();
        let p = root(&mut t, 0, o::Type::I64);
        let (parent, reference, begin) =
            borrow(&mut t, p, o::LoanKind::Exclusive, o::Type::I64, 10);
        let referent = place(
            &mut t,
            p::Root::Value(reference),
            &[p::ProjectionKind::Dereference],
            o::Type::I64,
        );
        let (child, _, reborrow) = borrow(&mut t, referent, o::LoanKind::Shared, o::Type::I64, 11);
        let result = value(&mut t, o::Type::I64);
        let op = match action {
            0 => i(&[result], b::Op::CopyPlace(referent), 12),
            1 => i(
                &[],
                b::Op::Replace {
                    place: referent,
                    value: r(1),
                },
                12,
            ),
            _ => end(parent, 12),
        };
        finish(
            &mut t,
            vec![begin, reborrow, op, end(child, 13), end(parent, 14)],
        );
        let out = analyze(t);
        assert_eq!(out.is_ok(), action == 0);
        if let Err(e) = out {
            assert!(matches!(
                e.kind,
                l::ErrorKind::FrozenParent { .. } | l::ErrorKind::EndParent { .. }
            ));
        }
    }
}

#[test]
fn reference_cell_rebinding_keeps_old_lexical_loans_live() {
    let mut t = base();
    let p = root(&mut t, 0, o::Type::I64);
    let q = root(&mut t, 1, o::Type::I64);
    let (a, first, begin_a) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
    let (b, second, begin_b) = borrow(&mut t, q, o::LoanKind::Shared, o::Type::I64, 11);
    let loan_ty = t.places.functions[0].values[first.0 as usize].ty;
    let cell = local(&mut t, loan_ty, false);
    let cell_place = root(&mut t, cell.0, loan_ty);
    finish(
        &mut t,
        vec![
            begin_a,
            i(
                &[],
                b::Op::Initialize {
                    local: cell,
                    value: first,
                },
                12,
            ),
            begin_b,
            i(
                &[],
                b::Op::Replace {
                    place: cell_place,
                    value: second,
                },
                13,
            ),
            i(
                &[],
                b::Op::Replace {
                    place: p,
                    value: r(1),
                },
                14,
            ),
            end(b, 15),
            end(a, 16),
        ],
    );
    assert!(matches!(
        analyze(t).unwrap_err().kind,
        l::ErrorKind::Conflict {
            owner: l::Owner::Local(d::LocalId(0)),
            ..
        }
    ));
}

#[test]
fn nested_reference_cells_retain_transitive_parent_and_cell_dependencies() {
    let mut t = base();
    let p = root(&mut t, 0, o::Type::I64);
    let (inner, reference, first) = borrow(&mut t, p, o::LoanKind::Exclusive, o::Type::I64, 10);
    let loan_ty = t.places.functions[0].values[reference.0 as usize].ty;
    let cell = local(&mut t, loan_ty, false);
    let cell_place = root(&mut t, cell.0, loan_ty);
    let (outer, outer_ref, second) =
        borrow(&mut t, cell_place, o::LoanKind::Exclusive, loan_ty, 12);
    let path = place(
        &mut t,
        p::Root::Value(outer_ref),
        &[
            p::ProjectionKind::Dereference,
            p::ProjectionKind::Dereference,
        ],
        o::Type::I64,
    );
    let (child, _, third) = borrow(&mut t, path, o::LoanKind::Shared, o::Type::I64, 13);
    finish(
        &mut t,
        vec![
            first,
            i(
                &[],
                b::Op::Initialize {
                    local: cell,
                    value: reference,
                },
                11,
            ),
            second,
            third,
            end(outer, 14),
            end(child, 15),
            end(inner, 16),
        ],
    );
    assert!(
        matches!(analyze(t).unwrap_err().kind, l::ErrorKind::EndParent { parent, child: _ } if parent == l::ReferenceId(outer.0))
    );
}

#[test]
fn caller_reference_chains_have_distinct_cells_and_cannot_receive_local_loans() {
    let mut t = base();
    let inner = ty(
        &mut t,
        o::Constructor::Loan(o::LoanKind::Exclusive, o::Type::I64),
    );
    let outer = ty(&mut t, o::Constructor::Loan(o::LoanKind::Exclusive, inner));
    let parameter = local(&mut t, outer, true);
    let deref = place(
        &mut t,
        p::Root::Local(parameter),
        &[
            p::ProjectionKind::Dereference,
            p::ProjectionKind::Dereference,
        ],
        o::Type::I64,
    );
    finish(
        &mut t,
        vec![i(
            &[],
            b::Op::Replace {
                place: deref,
                value: r(1),
            },
            10,
        )],
    );
    let out = analyze(t).unwrap();
    let root = out.functions()[0]
        .root_cell(moves::Slot::Local(parameter))
        .unwrap();
    assert_eq!(
        out.functions()[0].cell_owner(root),
        Some(l::Owner::Local(parameter))
    );
    assert_eq!(
        out.functions()[0].references(
            m::BlockId(0),
            moves::Slot::Local(parameter),
            l::ReferenceId(0)
        ),
        Some(true)
    );
    let mut t = base();
    let p = root_place(&mut t, 0);
    let (id, reference, begin) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
    let inner = t.places.functions[0].values[reference.0 as usize].ty;
    let outer = ty(&mut t, o::Constructor::Loan(o::LoanKind::Exclusive, inner));
    let parameter = local(&mut t, outer, true);
    let destination = place(
        &mut t,
        p::Root::Local(parameter),
        &[p::ProjectionKind::Dereference],
        inner,
    );
    finish(
        &mut t,
        vec![
            begin,
            i(
                &[],
                b::Op::Replace {
                    place: destination,
                    value: reference,
                },
                11,
            ),
            end(id, 12),
        ],
    );
    assert!(matches!(
        analyze(t).unwrap_err().kind,
        l::ErrorKind::CallerCellEscape(_)
    ));
}
fn root_place(t: &mut b::BodyTable, id: u32) -> p::PlaceId {
    root(t, id, o::Type::I64)
}

#[test]
fn prepared_destinations_keep_the_original_referent_and_reject_ended_dependencies() {
    for end_early in [false, true] {
        let mut t = base();
        let p = root_place(&mut t, 0);
        let q = root_place(&mut t, 1);
        let (a, first, begin_a) = borrow(&mut t, p, o::LoanKind::Exclusive, o::Type::I64, 10);
        let (b, second, begin_b) = borrow(&mut t, q, o::LoanKind::Exclusive, o::Type::I64, 11);
        let loan_ty = t.places.functions[0].values[first.0 as usize].ty;
        let cell = local(&mut t, loan_ty, false);
        let cell_place = root(&mut t, cell.0, loan_ty);
        let destination = place(
            &mut t,
            p::Root::Local(cell),
            &[p::ProjectionKind::Dereference],
            o::Type::I64,
        );
        t.functions[0].prepared = 1;
        let mut ops = vec![
            begin_a,
            i(
                &[],
                b::Op::Initialize {
                    local: cell,
                    value: first,
                },
                12,
            ),
            i(
                &[],
                b::Op::PreparePlace {
                    destination: b::PreparedPlaceId(0),
                    place: destination,
                },
                13,
            ),
            begin_b,
            i(
                &[],
                b::Op::Replace {
                    place: cell_place,
                    value: second,
                },
                14,
            ),
        ];
        if end_early {
            ops.push(end(a, 15));
        }
        ops.extend([
            i(
                &[],
                b::Op::ReplacePrepared {
                    destination: b::PreparedPlaceId(0),
                    value: r(1),
                },
                16,
            ),
            end(b, 17),
            end(a, 18),
        ]);
        finish(&mut t, ops);
        let out = analyze(t);
        assert_eq!(out.is_ok(), !end_early);
        if let Err(e) = out {
            assert_eq!(e.kind, l::ErrorKind::InvalidPrepared(b::PreparedPlaceId(0)));
        }
    }
}

#[test]
fn branch_reference_arguments_preserve_path_correlated_activity_and_simultaneous_transfers() {
    let mut t = base();
    let p = root_place(&mut t, 0);
    let q = root_place(&mut t, 1);
    let (a, first, begin_a) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
    let (b, second, begin_b) = borrow(&mut t, q, o::LoanKind::Shared, o::Type::I64, 11);
    let loan_ty = t.places.functions[0].values[first.0 as usize].ty;
    let joined = value(&mut t, loan_ty);
    let read_place = place(
        &mut t,
        p::Root::Value(joined),
        &[p::ProjectionKind::Dereference],
        o::Type::I64,
    );
    let read = value(&mut t, o::Type::I64);
    let mut join = block(
        vec![
            i(&[read], b::Op::CopyPlace(read_place), 15),
            end(a, 16),
            end(b, 17),
        ],
        b::Term::Return(read),
    );
    join.parameters.push(joined);
    t.functions[0].blocks = vec![
        entry(vec![], branch(1, 2)),
        block(vec![begin_a], jump(3, &[first])),
        block(vec![begin_b], jump(3, &[second])),
        join,
    ];
    let out = analyze(t).unwrap();
    for id in [a, b] {
        assert_eq!(
            out.functions()[0].references(
                m::BlockId(3),
                moves::Slot::Value(joined),
                l::ReferenceId(id.0)
            ),
            Some(true)
        );
    }
}

#[test]
fn conditional_ends_invalidate_reference_values_even_when_the_identity_may_still_be_active() {
    let mut t = base();
    let p = root_place(&mut t, 0);
    let (id, reference, begin) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
    let path = place(
        &mut t,
        p::Root::Value(reference),
        &[p::ProjectionKind::Dereference],
        o::Type::I64,
    );
    let result = value(&mut t, o::Type::I64);
    t.functions[0].blocks = vec![
        entry(vec![begin], branch(1, 2)),
        block(vec![end(id, 11)], jump(3, &[])),
        block(vec![], jump(3, &[])),
        block(
            vec![i(&[result], b::Op::CopyPlace(path), 12), end(id, 13)],
            b::Term::Return(result),
        ),
    ];
    assert!(matches!(
        analyze(t).unwrap_err().kind,
        l::ErrorKind::EndedReference(_)
    ));
}

#[test]
fn stale_reference_cells_cannot_revive_when_a_loop_reuses_a_creation_identity() {
    let mut t = base();
    let p = root_place(&mut t, 0);
    let q = root_place(&mut t, 1);
    let (outer, first, begin_outer) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
    let (loop_id, next, begin_loop) = borrow(&mut t, q, o::LoanKind::Shared, o::Type::I64, 11);
    let loan_ty = t.places.functions[0].values[first.0 as usize].ty;
    let cell = local(&mut t, loan_ty, false);
    let cell_place = root(&mut t, cell.0, loan_ty);
    let path = place(
        &mut t,
        p::Root::Local(cell),
        &[p::ProjectionKind::Dereference],
        o::Type::I64,
    );
    let read = value(&mut t, o::Type::I64);
    t.functions[0].blocks = vec![
        entry(
            vec![
                begin_outer,
                i(
                    &[],
                    b::Op::Initialize {
                        local: cell,
                        value: first,
                    },
                    12,
                ),
            ],
            jump(1, &[]),
        ),
        block(
            vec![
                begin_loop,
                i(&[read], b::Op::CopyPlace(path), 13),
                i(
                    &[],
                    b::Op::Replace {
                        place: cell_place,
                        value: next,
                    },
                    14,
                ),
                end(loop_id, 15),
            ],
            branch(1, 2),
        ),
        block(vec![end(outer, 16)], b::Term::Return(r(0))),
    ];
    assert!(matches!(
        analyze(t).unwrap_err().kind,
        l::ErrorKind::LoopState { .. }
    ));
}

#[test]
fn exact_budgets_node_and_state_limits_are_checked_and_success_replays() {
    let mut t = base();
    let p = root_place(&mut t, 0);
    let (id, _, begin) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
    finish(&mut t, vec![begin, end(id, 11)]);
    let activity = activity(t.clone());
    let out = l::analyze(activity.clone(), Default::default()).unwrap();
    assert_eq!(
        l::analyze(out.clone().into_activity(), Default::default()).unwrap(),
        out
    );
    for dimension in 0..3 {
        let mut limits = l::Limits::default();
        match dimension {
            0 => limits.work = out.work_used(),
            1 => limits.allocations = out.requested_bytes(),
            _ => limits.elements = out.elements_used(),
        };
        assert_eq!(l::analyze(activity.clone(), limits).unwrap(), out);
        match dimension {
            0 => limits.work -= 1,
            1 => limits.allocations -= 1,
            _ => limits.elements -= 1,
        };
        assert!(matches!(
            l::analyze(activity.clone(), limits).unwrap_err().kind,
            l::ErrorKind::Resource(_)
        ));
    }
    assert!(matches!(
        l::analyze(
            activity.clone(),
            l::Limits {
                nodes: 0,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        l::ErrorKind::Nodes { .. }
    ));
    assert!(matches!(
        l::analyze(
            activity.clone(),
            l::Limits {
                state_words: 0,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        l::ErrorKind::StateWords { .. }
    ));
    assert!(matches!(
        l::analyze(
            activity.clone(),
            l::Limits {
                work: activity.work_used() - 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        l::ErrorKind::Resource(m::ErrorKind::Limit {
            kind: m::LimitKind::Work,
            ..
        })
    ));
    let nodes = (0..)
        .take_while(|n| out.functions()[0].cell_owner(l::CellId(*n)).is_some())
        .count();
    let l::ErrorKind::StateWords {
        requested: words, ..
    } = l::analyze(
        activity,
        l::Limits {
            state_words: 0,
            ..Default::default()
        },
    )
    .unwrap_err()
    .kind
    else {
        unreachable!()
    };
    let mut declaration = t.places.declarations.functions[0].clone();
    declaration.name = "g".into();
    t.places.declarations.functions.push(declaration);
    t.places.functions.push(t.places.functions[0].clone());
    t.functions.push(t.functions[0].clone());
    let pair = self::activity(t);
    assert!(
        matches!(l::analyze(pair.clone(), l::Limits { nodes: nodes * 2 - 1, ..Default::default() }).unwrap_err().kind, l::ErrorKind::Nodes { requested, limit } if requested == nodes * 2 && limit == nodes * 2 - 1)
    );
    assert!(
        matches!(l::analyze(pair, l::Limits { state_words: words * 2 - 1, ..Default::default() }).unwrap_err().kind, l::ErrorKind::StateWords { requested, limit } if requested == words * 2 && limit == words * 2 - 1)
    );
    let mut t = base();
    let mut nested = o::Type::I64;
    for _ in 0..6 {
        nested = ty(&mut t, o::Constructor::Loan(o::LoanKind::Shared, nested));
    }
    local(&mut t, nested, true);
    finish(&mut t, vec![]);
    let nested = self::activity(t);
    l::analyze(nested.clone(), Default::default()).unwrap();
    assert!(matches!(
        l::analyze(
            nested,
            l::Limits {
                depth: 5,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        l::ErrorKind::Resource(m::ErrorKind::Limit {
            kind: m::LimitKind::Depth,
            ..
        })
    ));
}

#[test]
fn ninety_six_linear_programs_match_an_independent_owner_interval_oracle() {
    for mask in 0..96usize {
        let mut n = mask;
        let kind_a = n % 2;
        n /= 2;
        let kind_b = n % 2;
        n /= 2;
        let same = n % 2 == 0;
        n /= 2;
        let action = n % 3;
        n /= 3;
        let early_a = n % 2 == 0;
        n /= 2;
        let early_b = n % 2 == 0;
        let owner_b = usize::from(!same);
        let kinds = [kind_a, kind_b];
        let owners = [0, owner_b];
        let mut active = [false; 2];
        let mut valid = true;
        for id in 0..2 {
            if id == 1 && early_a {
                active[0] = false;
            }
            for old in 0..id {
                if active[old] && owners[old] == owners[id] && (kinds[old] == 1 || kinds[id] == 1) {
                    valid = false;
                }
            }
            active[id] = true;
        }
        if early_b {
            active[1] = false;
        }
        match action {
            0 => {
                for id in 0..2 {
                    if active[id] && owners[id] == 0 && kinds[id] == 1 {
                        valid = false;
                    }
                }
            }
            1 => {
                if (0..2).any(|id| active[id] && owners[id] == 0) {
                    valid = false;
                }
            }
            _ => {
                if !active[0] {
                    valid = false;
                }
            }
        }
        let mut t = base();
        let p = root_place(&mut t, 0);
        let q = root_place(&mut t, owner_b as u32);
        let kinds = [
            if kind_a == 0 {
                o::LoanKind::Shared
            } else {
                o::LoanKind::Exclusive
            },
            if kind_b == 0 {
                o::LoanKind::Shared
            } else {
                o::LoanKind::Exclusive
            },
        ];
        let (a, reference, begin_a) = borrow(&mut t, p, kinds[0], o::Type::I64, 10);
        let (b, _, begin_b) = borrow(&mut t, q, kinds[1], o::Type::I64, 11);
        let read = value(&mut t, o::Type::I64);
        let path = place(
            &mut t,
            p::Root::Value(reference),
            &[p::ProjectionKind::Dereference],
            o::Type::I64,
        );
        let mut ops = vec![begin_a];
        if early_a {
            ops.push(end(a, 12));
        }
        ops.push(begin_b);
        if early_b {
            ops.push(end(b, 13));
        }
        ops.push(match action {
            0 => i(&[read], b::Op::CopyPlace(p), 14),
            1 => i(
                &[],
                b::Op::Replace {
                    place: p,
                    value: r(1),
                },
                14,
            ),
            _ => i(&[read], b::Op::CopyPlace(path), 14),
        });
        ops.extend([end(b, 15), end(a, 16)]);
        finish(&mut t, ops);
        assert_eq!(
            analyze(t).is_ok(),
            valid,
            "program {mask}: {kind_a} {kind_b} {same} {action} {early_a} {early_b}"
        );
    }
}

#[test]
fn balanced_and_irreducible_loops_do_not_reject_new_reference_temporaries() {
    for irreducible in [false, true] {
        let mut t = base();
        let p = root_place(&mut t, 0);
        let (id, reference, begin) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
        let path = place(
            &mut t,
            p::Root::Value(reference),
            &[p::ProjectionKind::Dereference],
            o::Type::I64,
        );
        let read = value(&mut t, o::Type::I64);
        let ops = vec![begin, i(&[read], b::Op::CopyPlace(path), 11), end(id, 12)];
        t.functions[0].blocks = if irreducible {
            vec![
                entry(vec![], branch(1, 2)),
                block(ops, jump(2, &[])),
                block(vec![], branch(1, 3)),
                block(vec![], b::Term::Return(r(0))),
            ]
        } else {
            vec![
                entry(vec![], jump(1, &[])),
                block(ops, branch(1, 2)),
                block(vec![], b::Term::Return(r(0))),
            ]
        };
        analyze(t).unwrap();
    }
}

#[test]
fn an_outer_reference_can_flow_through_a_simultaneous_self_edge_and_exit_parameter() {
    let mut t = base();
    let p = root_place(&mut t, 0);
    let (id, reference, begin) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
    let reference_ty = t.places.functions[0].values[reference.0 as usize].ty;
    let header = value(&mut t, reference_ty);
    let exit = value(&mut t, reference_ty);
    let path = place(
        &mut t,
        p::Root::Value(exit),
        &[p::ProjectionKind::Dereference],
        o::Type::I64,
    );
    let read = value(&mut t, o::Type::I64);
    let mut loop_block = block(
        vec![],
        b::Term::Branch {
            condition: r(2),
            then_target: m::Target {
                block: m::BlockId(1),
                arguments: vec![header],
            },
            else_target: m::Target {
                block: m::BlockId(2),
                arguments: vec![header],
            },
        },
    );
    loop_block.parameters.push(header);
    let mut exit_block = block(
        vec![i(&[read], b::Op::CopyPlace(path), 11), end(id, 12)],
        b::Term::Return(read),
    );
    exit_block.parameters.push(exit);
    t.functions[0].blocks = vec![
        entry(vec![begin], jump(1, &[reference])),
        loop_block,
        exit_block,
    ];
    analyze(t).unwrap();
}

#[test]
fn loan_sum_payload_views_retain_parent_dependencies_and_cannot_end_the_parent_first() {
    for early in [false, true] {
        let mut t = base();
        let p = root_place(&mut t, 0);
        let (parent, reference, begin) = borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10);
        let reference_ty = t.places.functions[0].values[reference.0 as usize].ty;
        let sum_ty = ty(&mut t, o::Constructor::Result(reference_ty, o::Type::I64));
        let sum = value(&mut t, sum_ty);
        let guard = value(&mut t, o::Type::Bool);
        let view_ty = ty(
            &mut t,
            o::Constructor::Loan(o::LoanKind::Shared, reference_ty),
        );
        let view = value(&mut t, view_ty);
        let child = b::LoanId(t.functions[0].loans as u32);
        t.functions[0].loans += 1;
        let path = place(
            &mut t,
            p::Root::Value(view),
            &[
                p::ProjectionKind::Dereference,
                p::ProjectionKind::Dereference,
            ],
            o::Type::I64,
        );
        let read = value(&mut t, o::Type::I64);
        let mut success = vec![
            // The payload view borrows its cell. Access to the contained
            // reference creates the transitive pointee dependency below.
            i(
                &[view],
                b::Op::BorrowCaseField {
                    loan: child,
                    value: sum,
                    case: 0,
                    field: 0,
                    guard,
                },
                13,
            ),
            i(&[read], b::Op::CopyPlace(path), 14),
        ];
        let (grandchild, _, borrow_payload) =
            borrow(&mut t, path, o::LoanKind::Shared, o::Type::I64, 14);
        success.insert(1, borrow_payload);
        if early {
            success.push(end(parent, 15));
        }
        success.extend([end(grandchild, 16), end(child, 16), end(parent, 17)]);
        t.functions[0].blocks = vec![
            entry(
                vec![
                    begin,
                    i(
                        &[sum],
                        b::Op::Variant {
                            case: 0,
                            payload: vec![reference],
                        },
                        11,
                    ),
                    i(
                        &[guard],
                        b::Op::IsCase {
                            value: sum,
                            case: 0,
                        },
                        12,
                    ),
                ],
                b::Term::Branch {
                    condition: guard,
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
            block(success, b::Term::Return(read)),
            block(vec![end(parent, 18)], b::Term::Return(r(0))),
        ];
        let out = analyze(t);
        assert_eq!(out.is_ok(), !early);
        if let Err(e) = out {
            assert!(matches!(e.kind, l::ErrorKind::EndParent { .. }));
        }
    }
}

#[test]
fn reference_and_target_bitsets_cover_word_boundaries_and_report_the_exact_ended_identity() {
    for early in [false, true] {
        let mut t = base();
        let p = root_place(&mut t, 0);
        let mut ops = vec![];
        let mut loans = vec![];
        for n in 0..70 {
            let (id, reference, begin) =
                borrow(&mut t, p, o::LoanKind::Shared, o::Type::I64, 10 + n);
            loans.push((id, reference));
            ops.push(begin);
        }
        let (last, reference) = loans[69];
        let path = place(
            &mut t,
            p::Root::Value(reference),
            &[p::ProjectionKind::Dereference],
            o::Type::I64,
        );
        let read = value(&mut t, o::Type::I64);
        if early {
            ops.push(end(last, 81));
        }
        ops.push(i(&[read], b::Op::CopyPlace(path), 82));
        for (n, (id, _)) in loans.into_iter().enumerate() {
            ops.push(end(id, 90 + n as u32));
        }
        finish(&mut t, ops);
        let out = analyze(t);
        assert_eq!(out.is_ok(), !early);
        if let Err(e) = out {
            assert_eq!(e.kind, l::ErrorKind::EndedReference(l::ReferenceId(69)));
        }
    }
}

#[test]
fn borrowing_a_reference_cell_and_passing_nested_access_authority_have_distinct_effects() {
    for scenario in 0..3 {
        let invoke = scenario != 0;
        let mut t = base();
        let p = root_place(&mut t, 0);
        let (inner, reference, begin_inner) =
            borrow(&mut t, p, o::LoanKind::Exclusive, o::Type::I64, 10);
        let inner_ty = t.places.functions[0].values[reference.0 as usize].ty;
        let cell = local(&mut t, inner_ty, false);
        let cell_place = root(&mut t, cell.0, inner_ty);
        let referent = place(
            &mut t,
            p::Root::Local(cell),
            &[p::ProjectionKind::Dereference],
            o::Type::I64,
        );
        let (sibling, _, begin_sibling) =
            borrow(&mut t, referent, o::LoanKind::Shared, o::Type::I64, 12);
        let (outer, argument, begin_outer) =
            borrow(&mut t, cell_place, o::LoanKind::Exclusive, inner_ty, 13);
        let mut ops = vec![
            begin_inner,
            i(
                &[],
                b::Op::Initialize {
                    local: cell,
                    value: reference,
                },
                11,
            ),
            begin_sibling,
            begin_outer,
        ];
        if invoke {
            if scenario == 2 {
                ops.push(end(sibling, 14));
            }
            let outer_ty = t.places.functions[0].values[argument.0 as usize].ty;
            let mut callee = t.places.declarations.functions[0].clone();
            callee.name = "callee".into();
            callee.parameters = vec![d::LocalId(0)];
            callee.locals = vec![d::Local {
                name: "argument".into(),
                span: span(),
                kind: d::LocalKind::Parameter,
                ty: o::TypeUse {
                    ty: outer_ty,
                    span: span(),
                },
            }];
            t.places.declarations.functions.push(callee);
            t.places.functions.push(p::FunctionPlaces {
                values: vec![o::TypeUse {
                    ty: o::Type::I64,
                    span: span(),
                }],
                places: vec![],
            });
            t.functions.push(b::FunctionBody {
                loans: 0,
                prepared: 0,
                blocks: vec![block(
                    vec![i(&[r(0)], b::Op::I64(0), 1)],
                    b::Term::Return(r(0)),
                )],
            });
            let result = value(&mut t, o::Type::I64);
            ops.push(i(
                &[result],
                b::Op::Call {
                    function: m::FunctionId(1),
                    arguments: vec![argument],
                },
                14,
            ));
        }
        ops.extend([end(outer, 15), end(sibling, 16), end(inner, 17)]);
        finish(&mut t, ops);
        let out = analyze(t);
        assert_eq!(out.is_ok(), scenario != 1);
        if let Err(e) = out {
            assert!(matches!(e.kind, l::ErrorKind::FrozenParent { .. }));
        }
    }
}
