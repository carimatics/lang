use lang_mir::unit::{
    self as m,
    ownership::{self as o, declarations as d, places as p},
};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 4,
        end: 5,
    }
}
fn whole() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 16,
    }
}
fn c(i: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(i))
}
fn n() -> o::Type {
    o::Type::Nominal(m::DefinitionId(0))
}
fn table(
    constructors: &[o::Constructor],
    local: o::Type,
    kind: d::LocalKind,
    values: &[o::Type],
) -> p::PlaceTable {
    p::PlaceTable {
        declarations: d::Declarations {
            types: o::TypeTable {
                sources: vec![m::Source {
                    path: "places.lang".into(),
                    text: "0123456789abcdef".into(),
                }],
                definitions: vec![],
                constructed: constructors
                    .iter()
                    .map(|&constructor| o::Constructed {
                        constructor,
                        span: whole(),
                    })
                    .collect(),
            },
            imports: vec![],
            functions: vec![d::Function {
                name: "f".into(),
                public: false,
                span: whole(),
                signature_span: whole(),
                parameters: if kind == d::LocalKind::Parameter {
                    vec![d::LocalId(0)]
                } else {
                    vec![]
                },
                result: o::TypeUse {
                    ty: o::Type::I64,
                    span: span(),
                },
                locals: vec![d::Local {
                    name: "local".into(),
                    span: span(),
                    kind,
                    ty: o::TypeUse {
                        ty: local,
                        span: span(),
                    },
                }],
            }],
        },
        functions: vec![p::FunctionPlaces {
            values: values
                .iter()
                .map(|&ty| o::TypeUse { ty, span: span() })
                .collect(),
            places: vec![],
        }],
    }
}
fn place(origin: u32, root: p::Root, steps: &[p::ProjectionKind]) -> p::Place {
    p::Place {
        origin: m::Register(origin),
        root,
        projections: steps
            .iter()
            .map(|&kind| p::Projection { kind, span: span() })
            .collect(),
        span: span(),
    }
}
fn local() -> p::Root {
    p::Root::Local(d::LocalId(0))
}
fn record_table(kind: d::LocalKind) -> p::PlaceTable {
    let mut t = table(
        &[
            o::Constructor::Builtin(o::Builtin::Text),
            o::Constructor::Builtin(o::Builtin::Text),
        ],
        n(),
        kind,
        &[n(), o::Type::I64, c(0)],
    );
    t.declarations.types.definitions.push(o::Definition {
        name: "Record".into(),
        public: false,
        span: whole(),
        kind: o::DefinitionKind::Record(vec![
            o::Field {
                name: "number".into(),
                span: span(),
                ty: o::TypeUse {
                    ty: o::Type::I64,
                    span: span(),
                },
            },
            o::Field {
                name: "text".into(),
                span: span(),
                ty: o::TypeUse {
                    ty: c(0),
                    span: span(),
                },
            },
        ]),
    });
    t.functions[0].places = vec![
        place(0, local(), &[]),
        place(1, local(), &[p::ProjectionKind::Field(0)]),
        place(2, local(), &[p::ProjectionKind::Field(1)]),
    ];
    t
}
fn access(v: &p::VerifiedPlaces, id: u32, access: p::Access) -> Result<(), o::Error> {
    v.check_access(
        m::FunctionId(0),
        p::PlaceId(id),
        access,
        span(),
        &mut m::Budget::new(m::Limits::default()),
    )
}

#[test]
fn local_and_field_access_distinguish_copy_whole_moves_partial_moves_and_replacement() {
    let v = record_table(d::LocalKind::Mutable)
        .verify(p::Limits::default())
        .unwrap();
    access(&v, 0, p::Access::Move).unwrap();
    access(&v, 0, p::Access::Replace(n())).unwrap();
    assert_eq!(
        access(&v, 0, p::Access::Copy).unwrap_err().kind,
        o::ErrorKind::NonCopyPlace(n())
    );
    access(&v, 1, p::Access::Copy).unwrap();
    access(&v, 1, p::Access::Move).unwrap();
    access(&v, 2, p::Access::Replace(c(1))).unwrap();
    assert_eq!(
        access(&v, 2, p::Access::Move).unwrap_err().kind,
        o::ErrorKind::PartialMove(c(0))
    );
    assert_eq!(
        access(&v, 2, p::Access::Copy).unwrap_err().kind,
        o::ErrorKind::NonCopyPlace(c(0))
    );
    assert_eq!(
        access(&v, 2, p::Access::Replace(o::Type::Bool))
            .unwrap_err()
            .kind,
        o::ErrorKind::PlaceType {
            expected: c(0),
            actual: o::Type::Bool
        }
    );
    assert_eq!(
        access(&v, 2, p::Access::Replace(c(999))).unwrap_err().kind,
        o::ErrorKind::InvalidType(c(999))
    );
}

#[test]
fn owning_parameters_and_pattern_bindings_are_immutable_but_can_move_whole_values() {
    for kind in [d::LocalKind::Parameter, d::LocalKind::Pattern] {
        let v = record_table(kind).verify(p::Limits::default()).unwrap();
        access(&v, 0, p::Access::Move).unwrap();
        assert_eq!(
            access(&v, 1, p::Access::Replace(o::Type::I64))
                .unwrap_err()
                .kind,
            o::ErrorKind::ImmutablePlace
        );
        assert_eq!(
            access(&v, 1, p::Access::Borrow(o::LoanKind::Exclusive))
                .unwrap_err()
                .kind,
            o::ErrorKind::ImmutablePlace
        );
        access(&v, 1, p::Access::Borrow(o::LoanKind::Shared)).unwrap();
    }
}

#[test]
fn nested_loan_paths_match_an_independent_permission_intersection_oracle() {
    for depth in 1..=4u32 {
        for mask in 0..(1u32 << depth) {
            for value_root in [false, true] {
                let mut constructors = vec![];
                let mut ty = o::Type::I64;
                for layer in 0..depth {
                    constructors.push(o::Constructor::Loan(
                        if mask & (1 << layer) == 0 {
                            o::LoanKind::Shared
                        } else {
                            o::LoanKind::Exclusive
                        },
                        ty,
                    ));
                    ty = c(layer);
                }
                let values = if value_root {
                    vec![ty, o::Type::I64]
                } else {
                    vec![o::Type::I64]
                };
                let mut t = table(&constructors, ty, d::LocalKind::Parameter, &values);
                t.functions[0].places.push(place(
                    if value_root { 1 } else { 0 },
                    if value_root {
                        p::Root::Value(m::Register(0))
                    } else {
                        local()
                    },
                    &vec![p::ProjectionKind::Dereference; depth as usize],
                ));
                let v = t.verify(p::Limits::default()).unwrap();
                let expected = mask == (1 << depth) - 1;
                assert_eq!(
                    v.facts(m::FunctionId(0), p::PlaceId(0))
                        .unwrap()
                        .exclusive_borrow,
                    expected
                );
                assert_eq!(
                    access(&v, 0, p::Access::Replace(o::Type::I64)).is_ok(),
                    expected
                );
                access(&v, 0, p::Access::Copy).unwrap();
            }
        }
    }
}

#[test]
fn noncopy_values_cannot_move_out_through_an_ordinary_loan() {
    let mut t = table(
        &[
            o::Constructor::Builtin(o::Builtin::Text),
            o::Constructor::Loan(o::LoanKind::Exclusive, c(0)),
        ],
        c(1),
        d::LocalKind::Parameter,
        &[c(0)],
    );
    t.functions[0]
        .places
        .push(place(0, local(), &[p::ProjectionKind::Dereference]));
    let v = t.verify(p::Limits::default()).unwrap();
    assert_eq!(
        access(&v, 0, p::Access::Move).unwrap_err().kind,
        o::ErrorKind::MoveThroughLoan(c(0))
    );
    access(&v, 0, p::Access::Replace(c(0))).unwrap();
}

#[test]
fn authority_places_allow_exclusive_borrow_without_move_or_replacement() {
    let mut t = table(
        &[o::Constructor::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(0),
            local: 0,
        })],
        c(0),
        d::LocalKind::ArenaOwner,
        &[c(0)],
    );
    t.functions[0].places.push(place(0, local(), &[]));
    let v = t.verify(p::Limits::default()).unwrap();
    access(&v, 0, p::Access::Borrow(o::LoanKind::Exclusive)).unwrap();
    assert_eq!(
        access(&v, 0, p::Access::Move).unwrap_err().kind,
        o::ErrorKind::MoveAuthority(c(0))
    );
    assert_eq!(
        access(&v, 0, p::Access::Replace(c(0))).unwrap_err().kind,
        o::ErrorKind::ReplaceAuthority(c(0))
    );
    let mut t = table(
        &[
            o::Constructor::Builtin(o::Builtin::Allocator),
            o::Constructor::Loan(o::LoanKind::Exclusive, c(0)),
        ],
        c(1),
        d::LocalKind::Parameter,
        &[c(0)],
    );
    t.functions[0]
        .places
        .push(place(0, local(), &[p::ProjectionKind::Dereference]));
    let v = t.verify(p::Limits::default()).unwrap();
    assert_eq!(
        access(&v, 0, p::Access::Move).unwrap_err().kind,
        o::ErrorKind::MoveAuthority(c(0))
    );
    assert_eq!(
        access(&v, 0, p::Access::Replace(c(0))).unwrap_err().kind,
        o::ErrorKind::ReplaceAuthority(c(0))
    );
}

#[test]
fn raw_dereference_never_opens_shared_weak_or_arena_storage() {
    for constructor in [
        o::Constructor::Shared(o::Type::I64),
        o::Constructor::Weak(o::Type::I64),
        o::Constructor::Arena(o::Type::I64),
        o::Constructor::Builtin(o::Builtin::Text),
    ] {
        let mut t = table(&[constructor], c(0), d::LocalKind::Mutable, &[o::Type::I64]);
        t.functions[0]
            .places
            .push(place(0, local(), &[p::ProjectionKind::Dereference]));
        let error = t.verify(p::Limits::default()).unwrap_err();
        assert_eq!(error.kind, o::ErrorKind::InvalidDereference(c(0)));
        assert_eq!(error.span, Some(span()));
    }
}

#[test]
fn temporary_value_roots_are_immutable_and_record_fields_still_reject_partial_moves() {
    let mut t = record_table(d::LocalKind::Mutable);
    for place in &mut t.functions[0].places {
        place.root = p::Root::Value(m::Register(0));
    }
    let v = t.verify(p::Limits::default()).unwrap();
    access(&v, 0, p::Access::Move).unwrap();
    access(&v, 1, p::Access::Read).unwrap();
    assert_eq!(
        access(&v, 1, p::Access::Replace(o::Type::I64))
            .unwrap_err()
            .kind,
        o::ErrorKind::ImmutablePlace
    );
    assert_eq!(
        access(&v, 2, p::Access::Move).unwrap_err().kind,
        o::ErrorKind::PartialMove(c(0))
    );
}

#[test]
fn malformed_roots_fields_origin_types_duplicates_and_spans_fail_before_facts() {
    let original = record_table(d::LocalKind::Mutable);
    let mut t = original.clone();
    t.functions[0].places[1].root = p::Root::Local(d::LocalId(9));
    assert!(matches!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::InvalidLocal { local: 9, .. }
    ));
    let mut t = original.clone();
    t.functions[0].places[1].root = p::Root::Value(m::Register(99));
    assert_eq!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::InvalidValue(m::Register(99))
    );
    let mut t = original.clone();
    t.functions[0].places[1].projections[0].kind = p::ProjectionKind::Field(99);
    assert_eq!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::InvalidField {
            definition: m::DefinitionId(0),
            field: 99
        }
    );
    let mut t = original.clone();
    t.functions[0].values[1].ty = o::Type::Bool;
    assert_eq!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::PlaceType {
            expected: o::Type::Bool,
            actual: o::Type::I64
        }
    );
    let mut t = original.clone();
    t.functions[0].values[1].ty = c(99);
    assert_eq!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::InvalidType(c(99))
    );
    let mut t = original.clone();
    let duplicate = t.functions[0].places[0].clone();
    t.functions[0].places.push(duplicate);
    assert_eq!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::DuplicatePlace(m::Register(0))
    );
    let mut t = original.clone();
    t.functions[0].places[0].span.start = 3;
    assert_eq!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::SpanOutsideOwner
    );
    let mut t = original.clone();
    t.functions[0].places[1].projections[0].span.start = 3;
    assert_eq!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::SpanOutsideOwner
    );
    let mut t = original.clone();
    t.functions.clear();
    assert_eq!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::FunctionBodyCount {
            expected: 1,
            actual: 0
        }
    );
    let mut t = original;
    t.declarations.types.definitions[0].kind = o::DefinitionKind::Enum(vec![o::Case {
        name: "Case".into(),
        span: span(),
        payload: vec![],
    }]);
    assert_eq!(
        t.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::ExpectedRecord(n())
    );
}

#[test]
fn cumulative_place_frame_depth_work_and_allocation_limits_are_exact() {
    let t = record_table(d::LocalKind::Mutable);
    let v = t.clone().verify(p::Limits::default()).unwrap();
    for (kind, used) in [
        (m::LimitKind::Work, v.work_used()),
        (m::LimitKind::Allocations, v.requested_bytes()),
        (m::LimitKind::Elements, v.elements_used()),
    ] {
        let mut l = p::Limits::default();
        match kind {
            m::LimitKind::Work => l.ownership.unit.work = used,
            m::LimitKind::Allocations => l.ownership.unit.allocations = used,
            _ => l.ownership.unit.elements = used,
        }
        t.clone().verify(l).unwrap();
        match kind {
            m::LimitKind::Work => l.ownership.unit.work -= 1,
            m::LimitKind::Allocations => l.ownership.unit.allocations -= 1,
            _ => l.ownership.unit.elements -= 1,
        }
        assert!(
            matches!(t.clone().verify(l).unwrap_err().kind,o::ErrorKind::Mir(m::ErrorKind::Limit{kind:k,..}) if k==kind)
        );
    }
    let l = p::Limits {
        places: 2,
        ..Default::default()
    };
    assert!(matches!(
        t.clone().verify(l).unwrap_err().kind,
        o::ErrorKind::PlaceLimit {
            requested: 3,
            limit: 2
        }
    ));
    let mut l = p::Limits::default();
    l.ownership.unit.registers = 3;
    assert!(t.clone().verify(l).is_err());
    let mut bad = t;
    bad.functions[0].places[0].projections = vec![
        p::Projection {
            kind: p::ProjectionKind::Dereference,
            span: span()
        };
        65
    ];
    assert!(matches!(
        bad.verify(p::Limits::default()).unwrap_err().kind,
        o::ErrorKind::Mir(m::ErrorKind::Limit {
            kind: m::LimitKind::Depth,
            ..
        })
    ));
}

#[test]
fn queries_reject_invalid_handles_and_preserve_operation_spans_and_work_limits() {
    let v = record_table(d::LocalKind::Mutable)
        .verify(p::Limits::default())
        .unwrap();
    let mut budget = m::Budget::new(m::Limits::default());
    assert!(v.facts(m::FunctionId(0), p::PlaceId(9)).is_none());
    assert_eq!(
        v.check_access(
            m::FunctionId(0),
            p::PlaceId(9),
            p::Access::Read,
            span(),
            &mut budget
        )
        .unwrap_err()
        .kind,
        o::ErrorKind::InvalidPlace(9)
    );
    let error = v
        .check_access(
            m::FunctionId(9),
            p::PlaceId(0),
            p::Access::Read,
            span(),
            &mut budget,
        )
        .unwrap_err();
    assert_eq!(error.kind, o::ErrorKind::InvalidFunction(m::FunctionId(9)));
    assert_eq!(error.span, Some(span()));
    assert!(
        v.check_access(
            m::FunctionId(0),
            p::PlaceId(0),
            p::Access::Read,
            span(),
            &mut m::Budget::new(m::Limits {
                work: 0,
                ..Default::default()
            })
        )
        .is_err()
    );
}
