use lang_mir::unit::{
    self as m,
    ownership::{self as o, declarations as d, places as p},
};

fn span(start: u32, end: u32) -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start,
        end,
    }
}
fn c(id: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(id))
}
fn ty(ty: o::Type) -> o::TypeUse {
    o::TypeUse {
        ty,
        span: span(10, 13),
    }
}
fn types(constructors: &[o::Constructor]) -> o::TypeTable {
    o::TypeTable {
        sources: vec![m::Source {
            path: "tasks.lang".into(),
            text: "async f x i64 bool }".into(),
        }],
        definitions: vec![],
        constructed: constructors
            .iter()
            .map(|&constructor| o::Constructed {
                constructor,
                span: span(10, 13),
            })
            .collect(),
    }
}
fn declarations(constructors: &[o::Constructor], asynchronous: bool) -> d::Declarations {
    d::Declarations {
        types: types(constructors),
        imports: vec![],
        functions: vec![d::Function {
            effect: if asynchronous {
                d::FunctionEffect::Asynchronous { span: span(0, 5) }
            } else {
                d::FunctionEffect::Synchronous
            },
            abi: d::FunctionAbi::Guest,
            name: "f".into(),
            public: false,
            span: span(0, 20),
            signature_span: span(0, 18),
            parameters: vec![],
            result: ty(o::Type::I64),
            locals: vec![],
        }],
    }
}
fn local(ty_: o::Type) -> d::Local {
    d::Local {
        name: "claim".into(),
        span: span(8, 9),
        kind: d::LocalKind::Mutable,
        ty: ty(ty_),
    }
}
fn kind(d: d::Declarations) -> o::ErrorKind {
    d.verify(Default::default()).unwrap_err().kind
}

#[test]
fn task_claims_and_error_values_have_distinct_properties_and_canonical_identity() {
    let t = types(&[
        o::Constructor::Task(o::Type::I64),
        o::Constructor::Builtin(o::Builtin::TaskError),
        o::Constructor::Task(o::Type::I64),
        o::Constructor::Result(o::Type::I64, c(1)),
    ])
    .verify(Default::default())
    .unwrap();
    let claim = t.properties(c(0)).unwrap();
    assert!(!claim.is_copy && claim.requires_drop && claim.contains_task);
    assert!(!claim.contains_loan && !claim.contains_arena && !claim.contains_allocator);
    assert_eq!(t.equivalent(c(0), c(2)), Some(true));
    for id in [1, 3] {
        let p = t.properties(c(id)).unwrap();
        assert!(p.is_copy && !p.requires_drop && !p.contains_task);
    }
    assert_eq!(t.task_feature_span(), Some(span(10, 13)));
}

#[test]
fn claims_cannot_be_hidden_in_containers_loans_or_nested_claims() {
    for wrap in [
        o::Constructor::Option(c(0)),
        o::Constructor::Result(o::Type::I64, c(0)),
        o::Constructor::Vec(c(0)),
        o::Constructor::Map(o::Type::I64, c(0)),
        o::Constructor::Shared(c(0)),
        o::Constructor::Weak(c(0)),
        o::Constructor::Arena(c(0)),
        o::Constructor::Loan(o::LoanKind::Shared, c(0)),
        o::Constructor::Loan(o::LoanKind::Exclusive, c(0)),
        o::Constructor::Task(c(0)),
    ] {
        let e = types(&[o::Constructor::Task(o::Type::I64), wrap])
            .verify(Default::default())
            .unwrap_err();
        assert!(matches!(e.kind, o::ErrorKind::InvalidTaskType(_)), "{e:?}");
        assert_eq!(e.span, Some(span(10, 13)));
    }
}

#[test]
fn claims_cannot_be_hidden_in_nominal_fields_or_payloads() {
    for enumeration in [false, true] {
        let mut t = types(&[o::Constructor::Task(o::Type::I64)]);
        t.definitions.push(o::Definition {
            name: "R".into(),
            public: false,
            span: span(0, 20),
            kind: if enumeration {
                o::DefinitionKind::Enum(vec![o::Case {
                    name: "Case".into(),
                    span: span(8, 9),
                    payload: vec![ty(c(0))],
                }])
            } else {
                o::DefinitionKind::Record(vec![o::Field {
                    name: "field".into(),
                    span: span(8, 9),
                    ty: ty(c(0)),
                }])
            },
        });
        assert!(matches!(
            t.verify(Default::default()).unwrap_err().kind,
            o::ErrorKind::InvalidTaskType(_)
        ));
    }
}

#[test]
fn task_payloads_refuse_loans_arenas_and_allocator_authority() {
    for payload in [
        o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
        o::Constructor::Loan(o::LoanKind::Exclusive, o::Type::I64),
        o::Constructor::Arena(o::Type::I64),
        o::Constructor::Builtin(o::Builtin::Allocator),
    ] {
        assert!(matches!(
            types(&[payload, o::Constructor::Task(c(0))])
                .verify(Default::default())
                .unwrap_err()
                .kind,
            o::ErrorKind::InvalidTaskType(_)
        ));
    }
    // A type graph is not the pending general Send/Share transfer proof.
    types(&[
        o::Constructor::Vec(o::Type::I64),
        o::Constructor::Task(c(0)),
    ])
    .verify(Default::default())
    .unwrap();
}

#[test]
fn only_async_mutable_locals_can_hold_task_claims() {
    let mut valid = declarations(&[o::Constructor::Task(o::Type::I64)], true);
    valid.functions[0].locals.push(local(c(0)));
    let checked = valid.clone().verify(Default::default()).unwrap();
    assert!(checked.functions()[0].effect.is_async());
    assert_eq!(checked.task_feature_span(), Some(span(10, 13)));
    let mut sync = valid.clone();
    sync.functions[0].effect = d::FunctionEffect::Synchronous;
    assert!(matches!(kind(sync), o::ErrorKind::InvalidLocalType(_)));
    let mut pattern = valid;
    pattern.functions[0].locals[0].kind = d::LocalKind::Pattern;
    assert!(matches!(kind(pattern), o::ErrorKind::InvalidLocalType(_)));
}

#[test]
fn task_parameters_and_results_are_independently_invalid() {
    let mut result = declarations(&[o::Constructor::Task(o::Type::I64)], true);
    result.functions[0].result = ty(c(0));
    assert!(matches!(
        kind(result),
        o::ErrorKind::InvalidSignatureType(_)
    ));
    let mut parameter = declarations(&[o::Constructor::Task(o::Type::I64)], true);
    let mut l = local(c(0));
    l.kind = d::LocalKind::Parameter;
    parameter.functions[0].locals.push(l);
    parameter.functions[0].parameters.push(d::LocalId(0));
    assert!(matches!(
        kind(parameter),
        o::ErrorKind::InvalidSignatureType(_)
    ));
}

#[test]
fn effect_spans_and_async_c_exports_are_not_trusted() {
    let mut wrong = declarations(&[], true);
    wrong.functions[0].effect = d::FunctionEffect::Asynchronous { span: span(6, 7) };
    assert!(matches!(kind(wrong), o::ErrorKind::InvalidAsyncEffect(_)));
    let mut outside = declarations(&[], true);
    outside.functions[0].signature_span = span(6, 18);
    assert!(matches!(kind(outside), o::ErrorKind::SpanOutsideOwner));
    let mut forged = declarations(&[], true);
    forged.functions[0].effect = d::FunctionEffect::Asynchronous {
        span: m::Span {
            source: m::SourceId(99),
            start: 0,
            end: 5,
        },
    };
    assert!(matches!(kind(forged), o::ErrorKind::Mir(_)));
    let mut c = declarations(&[], true);
    c.functions[0].abi = d::FunctionAbi::CExport;
    c.functions[0].public = true;
    assert!(matches!(kind(c), o::ErrorKind::InvalidAsyncEffect(_)));
}

#[test]
fn ordinary_call_queries_cannot_erase_async_effects() {
    let d = declarations(&[], true).verify(Default::default()).unwrap();
    let mut budget = m::Budget::new(Default::default());
    let e = d
        .check_call(m::FunctionId(0), &[], span(6, 7), &mut budget)
        .unwrap_err();
    assert!(matches!(e.kind, o::ErrorKind::UnsupportedTaskProfile));
    let d = declarations(&[], false).verify(Default::default()).unwrap();
    assert_eq!(
        d.check_call(m::FunctionId(0), &[], span(6, 7), &mut budget)
            .unwrap(),
        o::Type::I64
    );
}

#[test]
fn task_error_is_copy_but_not_part_of_the_old_c_profile() {
    for parameter in [false, true] {
        let mut d = declarations(&[o::Constructor::Builtin(o::Builtin::TaskError)], false);
        d.functions[0].abi = d::FunctionAbi::CExport;
        d.functions[0].public = true;
        if parameter {
            let mut l = local(c(0));
            l.kind = d::LocalKind::Parameter;
            d.functions[0].locals.push(l);
            d.functions[0].parameters.push(d::LocalId(0));
        } else {
            d.functions[0].result = ty(c(0));
        }
        assert!(matches!(kind(d), o::ErrorKind::InvalidCSignatureType(_)));
    }
}

#[test]
fn unused_task_metadata_and_pure_async_are_refused_before_places_or_bodies() {
    for d in [
        declarations(&[], true),
        declarations(&[o::Constructor::Task(o::Type::I64)], false),
        declarations(&[o::Constructor::Builtin(o::Builtin::TaskError)], false),
    ] {
        d.clone().verify(Default::default()).unwrap();
        let e = p::PlaceTable {
            declarations: d,
            functions: vec![p::FunctionPlaces {
                values: vec![],
                places: vec![],
            }],
        }
        .verify(Default::default())
        .unwrap_err();
        assert!(
            matches!(e.kind, o::ErrorKind::UnsupportedTaskProfile),
            "{e:?}"
        );
        assert!(e.span.is_some());
    }
}

#[test]
fn task_graph_ids_depth_work_and_allocation_limits_fail_closed() {
    assert!(matches!(
        types(&[o::Constructor::Task(c(99))])
            .verify(Default::default())
            .unwrap_err()
            .kind,
        o::ErrorKind::ForwardConstructed(_)
    ));
    for limits in [
        o::Limits {
            constructed: 0,
            ..Default::default()
        },
        o::Limits {
            unit: m::Limits {
                work: 0,
                ..Default::default()
            },
            ..Default::default()
        },
        o::Limits {
            unit: m::Limits {
                allocations: 0,
                ..Default::default()
            },
            ..Default::default()
        },
        o::Limits {
            unit: m::Limits {
                depth: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    ] {
        assert!(
            types(&[o::Constructor::Task(o::Type::I64)])
                .verify(limits)
                .is_err()
        );
    }
}
