use lang_mir::unit::{
    self as m,
    ownership::{
        self as o, bodies as b, core as c, declarations as d, failure as f, places as p, tasks as t,
    },
};
fn s() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 20,
    }
}
fn r(i: u32) -> m::Register {
    m::Register(i)
}
fn ty(ty: o::Type) -> o::TypeUse {
    o::TypeUse { ty, span: s() }
}
fn ct(i: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(i))
}
fn i(results: &[u32], op: b::Op) -> b::Instruction {
    b::Instruction {
        results: results.iter().map(|v| r(*v)).collect(),
        op,
        span: s(),
    }
}
fn block(ins: Vec<b::Instruction>, term: b::Term) -> b::Block {
    b::Block {
        parameters: vec![],
        instructions: ins,
        terminator: b::Terminator {
            kind: term,
            span: s(),
        },
        span: s(),
    }
}
fn scope(parent: Option<u32>) -> b::TaskScope {
    b::TaskScope {
        parent: parent.map(b::TaskScopeId),
        span: s(),
    }
}
fn binding(value: u32, scope: u32) -> b::TaskBinding {
    b::TaskBinding {
        target: b::CleanupTarget::Value(r(value)),
        scope: b::TaskScopeId(scope),
    }
}
fn unwrap(value: u32) -> b::Op {
    b::Op::CoreCall(c::Call {
        operation: c::Operation::ResultUnwrap,
        type_arguments: vec![ty(o::Type::I64), ty(ct(1))],
        arguments: vec![r(value)],
        result_loan: None,
    })
}
fn function(name: &str, locals: Vec<d::Local>, parameters: Vec<d::LocalId>) -> d::Function {
    d::Function {
        effect: d::FunctionEffect::Asynchronous {
            span: m::Span { end: 5, ..s() },
        },
        abi: d::FunctionAbi::Guest,
        name: name.into(),
        public: false,
        span: s(),
        signature_span: s(),
        parameters,
        result: ty(o::Type::I64),
        locals,
    }
}
fn fixture() -> b::BodyTable {
    let constructors = [
        o::Constructor::Task(o::Type::I64),
        o::Constructor::Builtin(o::Builtin::TaskError),
        o::Constructor::Result(o::Type::I64, ct(1)),
    ];
    b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "tasks.lang".into(),
                        text: "async f x i64 bool }".into(),
                    }],
                    definitions: vec![],
                    constructed: constructors
                        .into_iter()
                        .map(|constructor| o::Constructed {
                            constructor,
                            span: s(),
                        })
                        .collect(),
                },
                imports: vec![],
                functions: vec![
                    function(
                        "worker",
                        vec![d::Local {
                            name: "arg".into(),
                            span: s(),
                            kind: d::LocalKind::Parameter,
                            ty: ty(o::Type::I64),
                        }],
                        vec![d::LocalId(0)],
                    ),
                    function("main", vec![], vec![]),
                ],
            },
            functions: vec![
                p::FunctionPlaces {
                    values: vec![ty(o::Type::I64)],
                    places: vec![p::Place {
                        origin: r(0),
                        root: p::Root::Local(d::LocalId(0)),
                        projections: vec![],
                        span: s(),
                    }],
                },
                p::FunctionPlaces {
                    values: [
                        o::Type::I64,
                        ct(0),
                        o::Type::I64,
                        ct(0),
                        ct(2),
                        ct(2),
                        o::Type::I64,
                        o::Type::I64,
                        o::Type::I64,
                    ]
                    .into_iter()
                    .map(ty)
                    .collect(),
                    places: vec![p::Place {
                        origin: r(1),
                        root: p::Root::Value(r(1)),
                        projections: vec![],
                        span: s(),
                    }],
                },
            ],
        },
        functions: vec![
            b::FunctionBody {
                task_scopes: vec![scope(None)],
                blocks: vec![block(
                    vec![
                        i(&[0], b::Op::CopyPlace(p::PlaceId(0))),
                        i(&[], b::Op::TaskYield),
                        i(&[], b::Op::TaskSaveReturn(r(0))),
                        i(&[], b::Op::TaskScopeExit(b::TaskScopeId(0))),
                    ],
                    b::Term::ReturnSaved,
                )],
                ..Default::default()
            },
            b::FunctionBody {
                task_scopes: vec![scope(None)],
                task_bindings: vec![binding(1, 0), binding(3, 0)],
                blocks: vec![block(
                    vec![
                        i(&[0], b::Op::I64(20)),
                        i(
                            &[1],
                            b::Op::TaskSpawn {
                                function: m::FunctionId(0),
                                arguments: vec![r(0)],
                            },
                        ),
                        i(&[2], b::Op::I64(22)),
                        i(
                            &[3],
                            b::Op::TaskSpawn {
                                function: m::FunctionId(0),
                                arguments: vec![r(2)],
                            },
                        ),
                        i(&[4], b::Op::TaskAwait(r(1))),
                        i(&[5], b::Op::TaskAwait(r(3))),
                        i(&[6], unwrap(4)),
                        i(&[7], unwrap(5)),
                        i(&[8], b::Op::Add(r(6), r(7))),
                        i(&[], b::Op::TaskSaveReturn(r(8))),
                        i(&[], b::Op::TaskScopeExit(b::TaskScopeId(0))),
                    ],
                    b::Term::ReturnSaved,
                )],
                ..Default::default()
            },
        ],
    }
}
fn reject(table: b::BodyTable) -> t::Error {
    t::verify(table, Default::default()).unwrap_err()
}

fn diamond() -> b::BodyTable {
    let mut table = fixture();
    table.places.functions[1].values = [
        o::Type::I64,
        ct(0),
        o::Type::I64,
        ct(0),
        o::Type::Bool,
        ct(0),
        ct(0),
        ct(2),
        ct(2),
        ct(2),
        o::Type::I64,
    ]
    .into_iter()
    .map(ty)
    .collect();
    table.places.functions[1].places.clear();
    table.functions[1].task_bindings = [1, 3, 5, 6].into_iter().map(|v| binding(v, 0)).collect();
    let target = |block, arguments: Vec<m::Register>| m::Target {
        block: m::BlockId(block),
        arguments,
    };
    table.functions[1].blocks = vec![
        block(
            vec![
                i(&[0], b::Op::I64(20)),
                i(
                    &[1],
                    b::Op::TaskSpawn {
                        function: m::FunctionId(0),
                        arguments: vec![r(0)],
                    },
                ),
                i(&[2], b::Op::I64(22)),
                i(
                    &[3],
                    b::Op::TaskSpawn {
                        function: m::FunctionId(0),
                        arguments: vec![r(2)],
                    },
                ),
                i(&[4], b::Op::Bool(true)),
            ],
            b::Term::Branch {
                condition: r(4),
                then_target: target(1, vec![r(1)]),
                else_target: target(2, vec![r(3)]),
            },
        ),
        b::Block {
            parameters: vec![r(5)],
            ..block(
                vec![i(&[7], b::Op::TaskAwait(r(5)))],
                b::Term::Jump(target(3, vec![r(7)])),
            )
        },
        b::Block {
            parameters: vec![r(6)],
            ..block(
                vec![i(&[8], b::Op::TaskAwait(r(6)))],
                b::Term::Jump(target(3, vec![r(8)])),
            )
        },
        b::Block {
            parameters: vec![r(9)],
            ..block(
                vec![
                    i(&[10], unwrap(9)),
                    i(&[], b::Op::TaskSaveReturn(r(10))),
                    i(&[], b::Op::TaskScopeExit(b::TaskScopeId(0))),
                ],
                b::Term::ReturnSaved,
            )
        },
    ];
    table
}

#[test]
fn cfg_arguments_transfer_claims_and_retire_unclaimed_alternatives() {
    t::verify(diamond(), Default::default()).unwrap();
}

#[test]
fn duplicated_cfg_arguments_do_not_duplicate_a_claim() {
    let mut table = diamond();
    table.places.functions[1]
        .values
        .extend([ty(ct(0)), ty(ct(2))]);
    table.functions[1].task_bindings.push(binding(11, 0));
    let b::Term::Branch { then_target, .. } = &mut table.functions[1].blocks[0].terminator.kind
    else {
        unreachable!()
    };
    then_target.arguments.push(r(1));
    table.functions[1].blocks[1].parameters.push(r(11));
    table.functions[1].blocks[1]
        .instructions
        .push(i(&[12], b::Op::TaskAwait(r(11))));
    let error = reject(table);
    assert!(matches!(error.kind, t::ErrorKind::Moves(_)), "{error:?}");
}

#[test]
fn cfg_merges_require_identical_scope_stacks() {
    let mut table = fixture();
    table.places.declarations.functions.truncate(1);
    table.places.declarations.functions[0].locals.clear();
    table.places.declarations.functions[0].parameters.clear();
    table.places.functions = vec![p::FunctionPlaces {
        values: vec![ty(o::Type::Bool), ty(o::Type::I64)],
        places: vec![],
    }];
    table.functions = vec![b::FunctionBody {
        task_scopes: vec![scope(None), scope(Some(0))],
        blocks: vec![
            block(
                vec![i(&[0], b::Op::Bool(true))],
                b::Term::Branch {
                    condition: r(0),
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
                vec![i(&[], b::Op::TaskScopeEnter(b::TaskScopeId(1)))],
                b::Term::Jump(m::Target {
                    block: m::BlockId(3),
                    arguments: vec![],
                }),
            ),
            block(
                vec![],
                b::Term::Jump(m::Target {
                    block: m::BlockId(3),
                    arguments: vec![],
                }),
            ),
            block(vec![i(&[1], b::Op::I64(1))], b::Term::Unreachable),
        ],
        ..Default::default()
    }];
    assert!(matches!(reject(table).kind, t::ErrorKind::Stack));
}

#[test]
fn loop_backedges_restore_the_lexical_scope_stack() {
    let mut table = fixture();
    table.places.functions[0].values.push(ty(o::Type::Bool));
    let body = &mut table.functions[0];
    body.task_scopes.push(scope(Some(0)));
    body.blocks = vec![
        block(
            vec![i(&[0], b::Op::CopyPlace(p::PlaceId(0)))],
            b::Term::Jump(m::Target {
                block: m::BlockId(1),
                arguments: vec![],
            }),
        ),
        block(
            vec![
                i(&[], b::Op::TaskScopeEnter(b::TaskScopeId(1))),
                i(&[], b::Op::TaskYield),
                i(&[], b::Op::TaskScopeExit(b::TaskScopeId(1))),
                i(&[1], b::Op::Bool(false)),
            ],
            b::Term::Branch {
                condition: r(1),
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
                i(&[], b::Op::TaskSaveReturn(r(0))),
                i(&[], b::Op::TaskScopeExit(b::TaskScopeId(0))),
            ],
            b::Term::ReturnSaved,
        ),
    ];
    t::verify(table.clone(), Default::default()).unwrap();
    table.functions[0].blocks[1].instructions.remove(2);
    assert!(matches!(reject(table).kind, t::ErrorKind::Stack));
}

#[test]
fn scope_ids_bindings_and_metadata_spans_are_validated() {
    let mut missing = fixture();
    missing.functions[1].task_bindings.pop();
    assert!(matches!(reject(missing).kind, t::ErrorKind::Claim));
    let mut duplicate = fixture();
    duplicate.functions[1].task_bindings.push(binding(1, 0));
    assert!(matches!(reject(duplicate).kind, t::ErrorKind::Binding(_)));
    let mut wrong = fixture();
    wrong.functions[1].task_bindings[0].target = b::CleanupTarget::Value(r(999));
    assert!(matches!(reject(wrong).kind, t::ErrorKind::Binding(_)));
    let mut forged = fixture();
    forged.functions[0].task_scopes[0].parent = Some(b::TaskScopeId(0));
    assert!(matches!(reject(forged).kind, t::ErrorKind::Scope(_)));
    let mut span = fixture();
    span.functions[0].task_scopes[0].span.source = m::SourceId(999);
    assert!(reject(span).primary.is_none());
    let mut invalid = fixture();
    invalid.functions[0].blocks[0].instructions[3].op = b::Op::TaskScopeExit(b::TaskScopeId(999));
    assert!(matches!(reject(invalid).kind, t::ErrorKind::Scope(999)));
}

#[test]
fn every_suspension_point_checks_live_loans() {
    for operation in [
        b::Op::TaskYield,
        b::Op::TaskCheck,
        b::Op::TaskScopeExit(b::TaskScopeId(0)),
    ] {
        let mut table = fixture();
        table
            .places
            .declarations
            .types
            .constructed
            .push(o::Constructed {
                constructor: o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
                span: s(),
            });
        table.places.functions[0].values.push(ty(ct(3)));
        table.functions[0].loans = 1;
        table.functions[0].blocks[0].instructions = vec![
            i(&[0], b::Op::CopyPlace(p::PlaceId(0))),
            i(
                &[1],
                b::Op::Borrow {
                    loan: b::LoanId(0),
                    kind: o::LoanKind::Shared,
                    place: p::PlaceId(0),
                },
            ),
            i(&[], b::Op::TaskSaveReturn(r(0))),
        ];
        // Saving return permits only scope exit/cleanup; yield/check occur before the save.
        let position = if matches!(operation, b::Op::TaskScopeExit(_)) {
            3
        } else {
            2
        };
        table.functions[0].blocks[0]
            .instructions
            .insert(position, i(&[], operation));
        table.functions[0].blocks[0].instructions.push(i(
            &[],
            b::Op::EndBorrow {
                loan: b::LoanId(0),
                mode: b::EndMode::Required,
            },
        ));
        if !matches!(
            table.functions[0].blocks[0].instructions[position].op,
            b::Op::TaskScopeExit(_)
        ) {
            table.functions[0].blocks[0]
                .instructions
                .push(i(&[], b::Op::TaskScopeExit(b::TaskScopeId(0))));
        }
        let error = reject(table);
        assert!(matches!(error.kind, t::ErrorKind::Loans(_)), "{error:?}");
    }
}

#[test]
fn cancellation_and_failure_cleanup_exist_with_saved_or_unsaved_results() {
    for save in [false, true] {
        let mut table = fixture();
        table.places.functions[0].places.clear();
        let mut ins = vec![i(&[0], b::Op::I64(7))];
        if save {
            ins.push(i(&[], b::Op::TaskSaveReturn(r(0))));
        }
        // Unreachable has a contained invariant failure and the same mandatory task cleanup.
        table.functions[0].blocks[0] = block(ins, b::Term::Unreachable);
        let verified = t::verify(table, Default::default()).unwrap();
        assert_eq!(
            verified.plan().functions()[0].cancellation(),
            Some(f::NodeId(2))
        );
        assert!(matches!(
            verified.plan().functions()[0].nodes()[8].kind,
            f::Kind::FinishTask
        ));
    }
}

#[test]
fn task_specific_and_shared_budgets_fail_closed() {
    let mut work = t::Limits::default();
    work.bodies.places.ownership.unit.work = 0;
    let mut allocation = t::Limits::default();
    allocation.bodies.places.ownership.unit.allocations = 0;
    for limits in [
        t::Limits {
            scopes: 0,
            ..Default::default()
        },
        t::Limits {
            bindings: 0,
            ..Default::default()
        },
        t::Limits {
            state_cells: 0,
            ..Default::default()
        },
        t::Limits {
            failure_nodes: 0,
            ..Default::default()
        },
        work,
        allocation,
    ] {
        assert!(t::verify(fixture(), limits).is_err(), "{limits:?}");
    }
}

#[test]
fn claims_move_through_locals_and_prepared_destinations_without_copying() {
    let mut table = fixture();
    table.places.declarations.functions[1]
        .locals
        .push(d::Local {
            name: "child".into(),
            span: s(),
            kind: d::LocalKind::Mutable,
            ty: ty(ct(0)),
        });
    table.places.functions[1].values.push(ty(ct(0)));
    table.places.functions[1].places.push(p::Place {
        origin: r(9),
        root: p::Root::Local(d::LocalId(0)),
        projections: vec![],
        span: s(),
    });
    table.functions[1].task_bindings.extend([
        b::TaskBinding {
            target: b::CleanupTarget::Local(d::LocalId(0)),
            scope: b::TaskScopeId(0),
        },
        binding(9, 0),
    ]);
    table.functions[1].blocks[0].instructions.insert(
        2,
        i(
            &[],
            b::Op::Initialize {
                local: d::LocalId(0),
                value: r(1),
            },
        ),
    );
    let at = table.functions[1].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.op, b::Op::TaskAwait(_)))
        .unwrap();
    table.functions[1].blocks[0].instructions[at].op = b::Op::TaskAwait(r(9));
    table.functions[1].blocks[0]
        .instructions
        .insert(at, i(&[9], b::Op::MovePlace(p::PlaceId(1))));
    table.functions[1].blocks[0]
        .instructions
        .insert(at, i(&[], b::Op::TaskCancel(p::PlaceId(1))));
    t::verify(table.clone(), Default::default()).unwrap();
    // A prepared whole-place replacement still transfers the claim once.
    table.functions[1].prepared = 1;
    table.functions[1].blocks[0].instructions[2].op = b::Op::ReplacePrepared {
        destination: b::PreparedPlaceId(0),
        value: r(1),
    };
    table.functions[1].blocks[0].instructions.insert(
        2,
        i(
            &[],
            b::Op::PreparePlace {
                destination: b::PreparedPlaceId(0),
                place: p::PlaceId(1),
            },
        ),
    );
    t::verify(table, Default::default()).unwrap();
}

#[test]
fn await_independently_checks_loans_and_copying_a_claim_is_invalid() {
    let mut table = fixture();
    table
        .places
        .declarations
        .types
        .constructed
        .push(o::Constructed {
            constructor: o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
            span: s(),
        });
    table.places.declarations.functions[1]
        .locals
        .push(d::Local {
            name: "owner".into(),
            span: s(),
            kind: d::LocalKind::Mutable,
            ty: ty(o::Type::I64),
        });
    table.places.functions[1].values.push(ty(ct(3)));
    table.places.functions[1].places.push(p::Place {
        origin: r(0),
        root: p::Root::Local(d::LocalId(0)),
        projections: vec![],
        span: s(),
    });
    table.functions[1].loans = 1;
    table.functions[1].blocks[0].instructions.insert(
        1,
        i(
            &[],
            b::Op::Initialize {
                local: d::LocalId(0),
                value: r(0),
            },
        ),
    );
    table.functions[1].blocks[0].instructions.insert(
        5,
        i(
            &[9],
            b::Op::Borrow {
                loan: b::LoanId(0),
                kind: o::LoanKind::Shared,
                place: p::PlaceId(1),
            },
        ),
    );
    table.functions[1].blocks[0].instructions.insert(
        8,
        i(
            &[],
            b::Op::EndBorrow {
                loan: b::LoanId(0),
                mode: b::EndMode::Required,
            },
        ),
    );
    let error = reject(table);
    assert!(matches!(error.kind, t::ErrorKind::Loans(_)), "{error:?}");
    let mut copy = fixture();
    copy.places.functions[1].values.push(ty(ct(0)));
    copy.functions[1].task_bindings.push(binding(9, 0));
    copy.functions[1].blocks[0]
        .instructions
        .insert(2, i(&[9], b::Op::Copy(r(1))));
    assert!(matches!(reject(copy).kind, t::ErrorKind::Types(_)));
}

#[test]
fn unused_owned_async_transfer_is_an_unavailable_profile_not_a_send_assertion() {
    let mut table = fixture();
    table
        .places
        .declarations
        .types
        .constructed
        .push(o::Constructed {
            constructor: o::Constructor::Vec(o::Type::I64),
            span: s(),
        });
    let mut bad = function("owned", vec![], vec![]);
    bad.result = ty(ct(3));
    table.places.declarations.functions.push(bad);
    table.places.functions.push(p::FunctionPlaces {
        values: vec![ty(o::Type::I64)],
        places: vec![],
    });
    table.functions.push(b::FunctionBody {
        task_scopes: vec![scope(None)],
        blocks: vec![block(
            vec![i(&[0], b::Op::I64(7)), i(&[], b::Op::Panic(r(0)))],
            b::Term::Unreachable,
        )],
        ..Default::default()
    });
    assert!(matches!(
        reject(table).kind,
        t::ErrorKind::UnsupportedTransfer(_)
    ));
}

#[test]
fn task_fixture_has_independent_claim_loan_region_and_cleanup_evidence() {
    let source = fixture();
    assert!(matches!(
        source.clone().verify(Default::default()).unwrap_err().kind,
        o::ErrorKind::UnsupportedTaskProfile
    ));
    let verified = t::verify(source, Default::default()).unwrap();
    for function in verified.plan().functions() {
        assert_eq!(function.cancellation(), Some(f::NodeId(2)));
        let nodes = function.nodes();
        assert!(matches!(
            nodes[3].kind,
            f::Kind::EndTaskLoans { next: f::NodeId(4) }
        ));
        assert!(matches!(
            nodes[4].kind,
            f::Kind::CancelTaskScopes { next: f::NodeId(5) }
        ));
        assert!(matches!(
            nodes[5].kind,
            f::Kind::JoinTaskScopes { next: f::NodeId(6) }
        ));
        assert!(matches!(
            nodes[6].kind,
            f::Kind::RetireFailure { next: f::NodeId(7) }
        ));
        assert!(matches!(nodes[8].kind, f::Kind::FinishTask));
        assert!(
            nodes
                .iter()
                .any(|n| matches!(n.kind, f::Kind::PublishReturn))
        );
        for n in nodes {
            for target in n.kind.targets() {
                assert!((target.0 as usize) < nodes.len());
            }
        }
    }
}

#[test]
fn cancel_observes_a_whole_claim_place_without_consumption() {
    let mut table = fixture();
    table.functions[1].blocks[0]
        .instructions
        .insert(4, i(&[], b::Op::TaskCancel(p::PlaceId(0))));
    t::verify(table.clone(), Default::default()).unwrap();
    table.functions[1].blocks[0]
        .instructions
        .insert(6, i(&[], b::Op::TaskCancel(p::PlaceId(0))));
    assert!(matches!(reject(table).kind, t::ErrorKind::Claim));
}

#[test]
fn claimed_values_cannot_escape_a_nested_scope_or_survive_its_exit() {
    let mut table = fixture();
    table.functions[1].task_scopes.push(scope(Some(0)));
    table.functions[1].blocks[0]
        .instructions
        .insert(0, i(&[], b::Op::TaskScopeEnter(b::TaskScopeId(1))));
    assert!(matches!(reject(table.clone()).kind, t::ErrorKind::Escape));
    for binding in &mut table.functions[1].task_bindings {
        binding.scope = b::TaskScopeId(1);
    }
    table.functions[1].blocks[0]
        .instructions
        .insert(5, i(&[], b::Op::TaskScopeExit(b::TaskScopeId(1))));
    assert!(matches!(reject(table).kind, t::ErrorKind::Claim));
}

#[test]
fn nested_return_escrow_closes_inner_then_root_scopes() {
    let mut table = fixture();
    table.functions[1].task_scopes.push(scope(Some(0)));
    for binding in &mut table.functions[1].task_bindings {
        binding.scope = b::TaskScopeId(1);
    }
    table.functions[1].blocks[0]
        .instructions
        .insert(0, i(&[], b::Op::TaskScopeEnter(b::TaskScopeId(1))));
    let len = table.functions[1].blocks[0].instructions.len();
    table.functions[1].blocks[0]
        .instructions
        .insert(len - 1, i(&[], b::Op::TaskScopeExit(b::TaskScopeId(1))));
    t::verify(table, Default::default()).unwrap();
}

#[test]
fn normal_task_return_requires_one_saved_value_and_all_scopes_closed() {
    let mut missing = fixture();
    missing.functions[1].blocks[0].instructions.remove(9);
    assert!(matches!(reject(missing).kind, t::ErrorKind::ReturnEscrow));
    let mut twice = fixture();
    twice.functions[1].blocks[0]
        .instructions
        .insert(10, i(&[], b::Op::TaskSaveReturn(r(8))));
    assert!(matches!(reject(twice).kind, t::ErrorKind::ReturnEscrow));
    let mut open = fixture();
    open.functions[1].blocks[0].instructions.pop();
    assert!(matches!(reject(open).kind, t::ErrorKind::ReturnEscrow));
    let mut old_return = fixture();
    old_return.functions[1].blocks[0].terminator.kind = b::Term::Return(r(8));
    assert!(matches!(
        reject(old_return).kind,
        t::ErrorKind::ReturnEscrow
    ));
}

#[test]
fn live_loans_are_independently_rejected_at_task_suspension() {
    for kind in [o::LoanKind::Shared, o::LoanKind::Exclusive] {
        let mut table = fixture();
        table
            .places
            .declarations
            .types
            .constructed
            .push(o::Constructed {
                constructor: o::Constructor::Loan(kind, o::Type::I64),
                span: s(),
            });
        // Parameter places are immutable; use a dedicated initialized mutable local for exclusive loans.
        let local = d::LocalId(1);
        table.places.declarations.functions[0]
            .locals
            .push(d::Local {
                name: "owned".into(),
                span: s(),
                kind: d::LocalKind::Mutable,
                ty: ty(o::Type::I64),
            });
        table.places.functions[0].places.push(p::Place {
            origin: r(0),
            root: p::Root::Local(local),
            projections: vec![],
            span: s(),
        });
        table.places.functions[0].places[1].origin = r(2);
        table.places.functions[0]
            .values
            .extend([ty(ct(3)), ty(o::Type::I64)]);
        table.functions[0].loans = 1;
        table.functions[0].blocks[0]
            .instructions
            .insert(1, i(&[], b::Op::Initialize { local, value: r(0) }));
        table.functions[0].blocks[0].instructions.insert(
            2,
            i(
                &[1],
                b::Op::Borrow {
                    loan: b::LoanId(0),
                    kind,
                    place: p::PlaceId(1),
                },
            ),
        );
        table.functions[0].blocks[0].instructions.insert(
            4,
            i(
                &[],
                b::Op::EndBorrow {
                    loan: b::LoanId(0),
                    mode: b::EndMode::Required,
                },
            ),
        );
        let error = reject(table.clone());
        assert!(matches!(error.kind, t::ErrorKind::Loans(_)), "{error:?}");
        table.functions[0].blocks[0].instructions.remove(4);
        table.functions[0].blocks[0].instructions.insert(
            3,
            i(
                &[],
                b::Op::EndBorrow {
                    loan: b::LoanId(0),
                    mode: b::EndMode::Required,
                },
            ),
        );
        t::verify(table, Default::default()).unwrap();
    }
}
