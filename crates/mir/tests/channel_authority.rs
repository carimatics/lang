use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, places as p, tasks as t},
};

fn s() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 20,
    }
}
fn ty(ty: o::Type) -> o::TypeUse {
    o::TypeUse { ty, span: s() }
}
fn ct(id: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(id))
}
fn r(id: u32) -> m::Register {
    m::Register(id)
}
fn i(results: &[u32], op: b::Op) -> b::Instruction {
    b::Instruction {
        results: results.iter().copied().map(r).collect(),
        op,
        span: s(),
    }
}

fn table(receive: bool) -> b::BodyTable {
    let role = if receive {
        b::ChannelRole::Receiver
    } else {
        b::ChannelRole::Sender
    };
    let loan = if receive {
        o::LoanKind::Exclusive
    } else {
        o::LoanKind::Shared
    };
    let constructors = [
        if receive {
            o::Constructor::Receiver(o::Type::I64)
        } else {
            o::Constructor::Sender(o::Type::I64)
        },
        o::Constructor::Loan(loan, ct(0)),
        if receive {
            o::Constructor::Option(o::Type::I64)
        } else {
            o::Constructor::SendResult(o::Type::I64)
        },
    ];
    let operation = if receive {
        b::Op::ChannelReceive {
            authority: b::PreparedPlaceId(0),
        }
    } else {
        b::Op::ChannelSend {
            authority: b::PreparedPlaceId(0),
            payload: r(2),
        }
    };
    b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "channel-authority.lang".into(),
                        text: "async f x i64 bool }".into(),
                    }],
                    constructed: constructors
                        .into_iter()
                        .map(|constructor| o::Constructed {
                            constructor,
                            span: s(),
                        })
                        .collect(),
                    definitions: vec![],
                },
                imports: vec![],
                functions: vec![d::Function {
                    effect: d::FunctionEffect::Asynchronous {
                        span: m::Span { end: 5, ..s() },
                    },
                    abi: d::FunctionAbi::Guest,
                    name: "f".into(),
                    public: false,
                    span: s(),
                    signature_span: s(),
                    parameters: vec![d::LocalId(0)],
                    result: ty(o::Type::I64),
                    locals: vec![
                        d::Local {
                            name: "argument".into(),
                            span: s(),
                            kind: d::LocalKind::Parameter,
                            ty: ty(ct(0)),
                        },
                        d::Local {
                            name: "endpoint".into(),
                            span: s(),
                            kind: d::LocalKind::Mutable,
                            ty: ty(ct(0)),
                        },
                    ],
                }],
            },
            functions: vec![p::FunctionPlaces {
                values: vec![
                    ty(ct(0)),
                    ty(ct(1)),
                    ty(o::Type::I64),
                    ty(ct(2)),
                    ty(o::Type::I64),
                    ty(ct(0)),
                ],
                places: (0..2)
                    .map(|id| p::Place {
                        origin: r(if id == 0 { 0 } else { 5 }),
                        root: p::Root::Local(d::LocalId(id)),
                        projections: vec![],
                        span: s(),
                    })
                    .collect(),
            }],
        },
        functions: vec![b::FunctionBody {
            loans: 1,
            prepared: 1,
            task_scopes: vec![b::TaskScope {
                parent: None,
                span: s(),
            }],
            blocks: vec![b::Block {
                parameters: vec![],
                span: s(),
                instructions: vec![
                    i(&[0], b::Op::MovePlace(p::PlaceId(0))),
                    i(
                        &[],
                        b::Op::Initialize {
                            local: d::LocalId(1),
                            value: r(0),
                        },
                    ),
                    i(
                        &[1],
                        b::Op::Borrow {
                            loan: b::LoanId(0),
                            kind: loan,
                            place: p::PlaceId(1),
                        },
                    ),
                    i(
                        &[],
                        b::Op::PrepareChannel {
                            destination: b::PreparedPlaceId(0),
                            endpoint: r(1),
                            role,
                            message_type: ty(o::Type::I64),
                        },
                    ),
                    i(
                        &[],
                        b::Op::EndBorrow {
                            loan: b::LoanId(0),
                            mode: b::EndMode::Required,
                        },
                    ),
                    i(&[2], b::Op::I64(20)),
                    i(&[3], operation),
                    i(&[], b::Op::DropValue(r(3))),
                    i(&[4], b::Op::I64(0)),
                    i(&[], b::Op::TaskSaveReturn(r(4))),
                    i(&[], b::Op::TaskScopeExit(b::TaskScopeId(0))),
                    i(&[], b::Op::DropLocalIfInitialized(d::LocalId(1))),
                ],
                terminator: b::Terminator {
                    kind: b::Term::ReturnSaved,
                    span: s(),
                },
            }],
            ..Default::default()
        }],
    }
}

#[test]
fn sender_and_receiver_captures_consume_exactly_once_after_the_argument_loan_ends() {
    for receive in [false, true] {
        let proof = t::verify(table(receive), Default::default()).unwrap();
        let captured = proof.typed().facts()[0].prepared()[0]
            .unwrap()
            .channel()
            .unwrap();
        assert_eq!(
            captured.role,
            if receive {
                b::ChannelRole::Receiver
            } else {
                b::ChannelRole::Sender
            }
        );
        assert_eq!(captured.message_type.ty, o::Type::I64);
        assert_eq!(captured.endpoint, r(1));
        assert_eq!(captured.span, s());
    }
}

#[test]
fn an_active_ordinary_loan_is_refused_even_after_authority_capture() {
    for receive in [false, true] {
        let mut raw = table(receive);
        let end = raw.functions[0].blocks[0].instructions.remove(4);
        raw.functions[0].blocks[0].instructions.insert(6, end);
        let error = t::verify(raw, Default::default()).unwrap_err();
        assert!(
            matches!(
                error.kind,
                t::ErrorKind::Loans(o::loans::ErrorKind::Suspension(_))
            ),
            "{error:?}"
        );
        assert_eq!(error.primary, Some(s()));
    }
}

#[test]
fn a_prepared_place_cannot_masquerade_as_channel_authority() {
    let mut raw = table(false);
    raw.functions[0].blocks[0].instructions[3] = i(
        &[],
        b::Op::PreparePlace {
            destination: b::PreparedPlaceId(0),
            place: p::PlaceId(1),
        },
    );
    let error = t::verify(raw, Default::default()).unwrap_err();
    assert_eq!(
        error.kind,
        t::ErrorKind::Types(o::ErrorKind::PreparedKind(0))
    );
}

#[test]
fn forged_role_message_or_result_types_fail_independently() {
    for variant in 0..3 {
        let mut raw = table(false);
        match variant {
            0 => {
                if let b::Op::PrepareChannel { role, .. } =
                    &mut raw.functions[0].blocks[0].instructions[3].op
                {
                    *role = b::ChannelRole::Receiver;
                }
            }
            1 => {
                if let b::Op::PrepareChannel { message_type, .. } =
                    &mut raw.functions[0].blocks[0].instructions[3].op
                {
                    *message_type = ty(o::Type::Bool);
                }
            }
            _ => raw.places.functions[0].values[3] = ty(o::Type::Bool),
        }
        let error = t::verify(raw, Default::default()).unwrap_err();
        assert!(
            matches!(
                error.kind,
                t::ErrorKind::Types(
                    o::ErrorKind::InvalidChannelEndpoint(_) | o::ErrorKind::InvalidChannelResult(_)
                )
            ),
            "{error:?}"
        );
    }
}

#[test]
fn capture_cannot_be_consumed_twice_or_abandoned_on_a_normal_exit() {
    let mut raw = table(false);
    raw.places.functions[0].values.push(ty(ct(2)));
    raw.functions[0].blocks[0].instructions.insert(
        7,
        i(
            &[6],
            b::Op::ChannelSend {
                authority: b::PreparedPlaceId(0),
                payload: r(2),
            },
        ),
    );
    let error = t::verify(raw, Default::default()).unwrap_err();
    assert!(matches!(error.kind, t::ErrorKind::Moves(_)), "{error:?}");
    let mut raw = table(false);
    raw.functions[0].blocks[0].instructions.drain(5..8);
    let error = t::verify(raw, Default::default()).unwrap_err();
    assert!(
        matches!(
            error.kind,
            t::ErrorKind::Moves(o::moves::ErrorKind::UnfinishedPrepared(_))
        ),
        "{error:?}"
    );
}
