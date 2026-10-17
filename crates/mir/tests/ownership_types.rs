use lang_mir::unit::{self as m, ownership::*};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    }
}
fn c(i: u32) -> Type {
    Type::Constructed(ConstructedId(i))
}
fn n(i: u32) -> Type {
    Type::Nominal(m::DefinitionId(i))
}
fn table(constructors: &[Constructor]) -> TypeTable {
    TypeTable {
        sources: vec![m::Source {
            path: "types.lang".into(),
            text: "x".into(),
        }],
        definitions: vec![],
        constructed: constructors
            .iter()
            .map(|&constructor| Constructed {
                constructor,
                span: span(),
            })
            .collect(),
    }
}
fn record(name: &str, public: bool, types: &[Type]) -> Definition {
    Definition {
        name: name.into(),
        public,
        span: span(),
        kind: DefinitionKind::Record(
            types
                .iter()
                .enumerate()
                .map(|(i, &ty)| Field {
                    name: format!("field_{i}"),
                    span: span(),
                    ty: TypeUse { ty, span: span() },
                })
                .collect(),
        ),
    }
}
fn kind(table: TypeTable) -> ErrorKind {
    table.verify(Limits::default()).unwrap_err().kind
}

#[test]
fn every_builtin_and_constructor_preserves_ownership_categories() {
    let mut t = table(&[
        Constructor::Builtin(Builtin::Unit),
        Constructor::Builtin(Builtin::Text),
        Constructor::Builtin(Builtin::Bytes),
        Constructor::Builtin(Builtin::Allocator),
        Constructor::Builtin(Builtin::AllocError),
        Constructor::Builtin(Builtin::BoundsError),
        Constructor::Builtin(Builtin::BufferError),
        Constructor::Builtin(Builtin::Utf8Error),
        Constructor::Builtin(Builtin::OwnershipError),
        Constructor::Builtin(Builtin::ArenaError),
        Constructor::Option(Type::I64),
        Constructor::Result(Type::Bool, c(1)),
        Constructor::Vec(Type::I64),
        Constructor::Map(c(1), c(2)),
        Constructor::Shared(Type::I64),
        Constructor::Weak(Type::I64),
        Constructor::Arena(Type::I64),
        Constructor::Loan(LoanKind::Shared, Type::I64),
        Constructor::Loan(LoanKind::Exclusive, c(3)),
        Constructor::ArenaOwner(ArenaOwner {
            function: m::FunctionId(2),
            local: 7,
        }),
        Constructor::Loan(LoanKind::Exclusive, c(19)),
        Constructor::Result(c(17), c(8)),
    ]);
    t.definitions
        .push(record("root::Record", true, &[c(10), c(11)]));
    t.definitions.push(Definition {
        name: "root::Sum".into(),
        public: true,
        span: span(),
        kind: DefinitionKind::Enum(vec![Case {
            name: "Item".into(),
            span: span(),
            payload: vec![TypeUse {
                ty: c(16),
                span: span(),
            }],
        }]),
    });
    let v = t.verify(Limits::default()).unwrap();
    for i in [0, 4, 5, 6, 7, 8, 9, 10, 16] {
        assert!(v.properties(c(i)).unwrap().is_copy);
    }
    for i in [1, 2, 11, 12, 13, 14, 15, 19] {
        let p = v.properties(c(i)).unwrap();
        assert!(!p.is_copy && p.requires_drop);
    }
    assert!(v.properties(c(16)).unwrap().contains_arena);
    assert!(!v.properties(c(16)).unwrap().requires_drop);
    assert!(v.properties(c(18)).unwrap().contains_allocator);
    assert!(v.properties(c(20)).unwrap().contains_owner);
    let read_result = v.properties(c(21)).unwrap();
    assert!(!read_result.is_copy && read_result.contains_loan && !read_result.requires_drop);
    assert!(!v.properties(n(0)).unwrap().is_copy);
    assert!(v.properties(n(1)).unwrap().contains_arena);
}

#[test]
fn recursion_requires_indirection_and_propagates_inspected_properties() {
    let mut t = table(&[Constructor::Arena(n(0)), Constructor::Option(c(0))]);
    t.definitions.push(record("Node", false, &[c(1)]));
    let v = t.verify(Limits::default()).unwrap();
    assert!(v.properties(n(0)).unwrap().is_copy);
    assert!(v.properties(n(0)).unwrap().contains_arena);
    let mut t = table(&[Constructor::Vec(n(0))]);
    t.definitions.push(record("Node", false, &[c(0)]));
    let p = t
        .verify(Limits::default())
        .unwrap()
        .properties(n(0))
        .unwrap();
    assert!(!p.is_copy && p.requires_drop);
    let mut t = table(&[]);
    t.definitions.push(record("Recursive", false, &[n(0)]));
    assert_eq!(kind(t), ErrorKind::RecursiveInline(n(0)));
    let mut t = table(&[Constructor::Option(n(0))]);
    t.definitions.push(record("Recursive", false, &[c(0)]));
    assert!(matches!(kind(t), ErrorKind::RecursiveInline(_)));
}

#[test]
fn heap_and_nominal_storage_reject_transitive_loans_allocators_and_owners() {
    for seed in [
        Constructor::Loan(LoanKind::Shared, Type::I64),
        Constructor::Builtin(Builtin::Allocator),
        Constructor::ArenaOwner(ArenaOwner {
            function: m::FunctionId(0),
            local: 0,
        }),
    ] {
        for wrapper in [
            Constructor::Vec(c(0)),
            Constructor::Map(Type::I64, c(0)),
            Constructor::Shared(c(0)),
            Constructor::Weak(c(0)),
            Constructor::Arena(c(0)),
        ] {
            assert!(matches!(
                kind(table(&[seed, wrapper])),
                ErrorKind::InvalidStorage(_)
            ));
        }
        let mut t = table(&[seed]);
        t.definitions.push(record("Bad", false, &[c(0)]));
        assert_eq!(kind(t), ErrorKind::InvalidStorage(c(0)));
    }
    let mut t = table(&[
        Constructor::Loan(LoanKind::Shared, Type::I64),
        Constructor::Option(c(0)),
        Constructor::Result(c(1), Type::I64),
    ]);
    t.definitions.push(record("Bad", false, &[c(2)]));
    assert_eq!(kind(t), ErrorKind::InvalidStorage(c(2)));
}

#[test]
fn arena_brands_never_enter_shared_or_weak_storage() {
    for constructor in [Constructor::Shared(c(1)), Constructor::Weak(c(1))] {
        let t = table(&[
            Constructor::Arena(Type::I64),
            Constructor::Option(c(0)),
            constructor,
        ]);
        assert_eq!(kind(t), ErrorKind::ArenaInShared(c(1)));
    }
}

#[test]
fn map_keys_are_exactly_i64_or_text() {
    table(&[
        Constructor::Builtin(Builtin::Text),
        Constructor::Map(c(0), Type::Bool),
        Constructor::Map(Type::I64, c(0)),
    ])
    .verify(Limits::default())
    .unwrap();
    for key in [Type::Bool, c(0), n(0)] {
        let mut t = table(&[
            Constructor::Builtin(Builtin::Bytes),
            Constructor::Map(key, Type::I64),
        ]);
        t.definitions.push(record("Wrapper", false, &[Type::I64]));
        assert_eq!(kind(t), ErrorKind::InvalidMapKey(key));
    }
}

#[test]
fn malformed_indices_spans_names_and_sources_fail_before_facts_are_published() {
    let error = table(&[Constructor::Option(n(42))])
        .verify(Limits::default())
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidType(n(42)));
    assert_eq!(error.span, Some(span()));
    assert_eq!(
        kind(table(&[Constructor::Option(c(0))])),
        ErrorKind::ForwardConstructed(ConstructedId(0))
    );
    assert_eq!(
        kind(table(&[
            Constructor::Option(c(1)),
            Constructor::Builtin(Builtin::Unit)
        ])),
        ErrorKind::ForwardConstructed(ConstructedId(1))
    );
    let mut t = table(&[Constructor::Builtin(Builtin::Unit)]);
    t.constructed[0].span.end = 2;
    assert_eq!(kind(t), ErrorKind::Mir(m::ErrorKind::InvalidSpan));
    let mut t = table(&[]);
    t.sources[0].text = "é".into();
    t.definitions.push(record("Unicode", false, &[]));
    assert_eq!(kind(t), ErrorKind::Mir(m::ErrorKind::InvalidSpan));
    let mut t = table(&[]);
    t.sources[0].path = "../escape".into();
    assert_eq!(kind(t), ErrorKind::Mir(m::ErrorKind::InvalidPath));
    let mut t = table(&[]);
    t.definitions = vec![record("Same", false, &[]), record("Same", false, &[])];
    assert_eq!(kind(t), ErrorKind::Mir(m::ErrorKind::DuplicateName));
    let mut t = table(&[]);
    t.definitions.push(record("", false, &[]));
    assert_eq!(kind(t), ErrorKind::Mir(m::ErrorKind::InvalidName));
    let mut t = table(&[]);
    let mut d = record("DuplicateField", false, &[Type::I64, Type::Bool]);
    if let DefinitionKind::Record(f) = &mut d.kind {
        f[1].name = f[0].name.clone();
    }
    t.definitions.push(d);
    assert_eq!(kind(t), ErrorKind::Mir(m::ErrorKind::DuplicateName));
    let mut t = table(&[]);
    t.definitions.push(Definition {
        name: "Empty".into(),
        public: false,
        span: span(),
        kind: DefinitionKind::Enum(vec![]),
    });
    assert_eq!(kind(t), ErrorKind::EmptyEnum);
    let mut t = table(&[]);
    let mut d = record("Outside", false, &[Type::I64]);
    d.span.end = 0;
    t.definitions.push(d);
    assert_eq!(kind(t), ErrorKind::SpanOutsideOwner);
}

#[test]
fn private_types_are_checked_through_indirection_without_poisoning_public_self_recursion() {
    let mut t = table(&[Constructor::Vec(n(1))]);
    t.definitions = vec![
        record("Public", true, &[c(0)]),
        record("Private", false, &[]),
    ];
    assert_eq!(kind(t), ErrorKind::PrivateInPublic(c(0)));
    let mut t = table(&[Constructor::Vec(n(0))]);
    t.definitions.push(record("Public", true, &[c(0)]));
    t.verify(Limits::default()).unwrap();
}

#[test]
fn exact_work_allocation_element_and_shape_limits_are_enforced() {
    let mut t = table(&[
        Constructor::Builtin(Builtin::Text),
        Constructor::Option(c(0)),
    ]);
    t.definitions.push(record("Record", false, &[c(1)]));
    let v = t.clone().verify(Limits::default()).unwrap();
    for (kind, used) in [
        (m::LimitKind::Work, v.work_used()),
        (m::LimitKind::Allocations, v.requested_bytes()),
        (m::LimitKind::Elements, v.elements_used()),
    ] {
        let mut l = Limits::default();
        match kind {
            m::LimitKind::Work => l.unit.work = used,
            m::LimitKind::Allocations => l.unit.allocations = used,
            _ => l.unit.elements = used,
        }
        t.clone().verify(l).unwrap();
        match kind {
            m::LimitKind::Work => l.unit.work -= 1,
            m::LimitKind::Allocations => l.unit.allocations -= 1,
            _ => l.unit.elements -= 1,
        }
        assert!(
            matches!(t.clone().verify(l).unwrap_err().kind,ErrorKind::Mir(m::ErrorKind::Limit{kind:k,..}) if k==kind)
        );
    }
    for k in [
        m::LimitKind::Sources,
        m::LimitKind::SourceBytes,
        m::LimitKind::TotalSourceBytes,
        m::LimitKind::Definitions,
        m::LimitKind::Fields,
        m::LimitKind::Depth,
    ] {
        let mut l = Limits::default();
        match k {
            m::LimitKind::Sources => l.unit.sources = 0,
            m::LimitKind::SourceBytes => l.unit.source_bytes = 0,
            m::LimitKind::TotalSourceBytes => l.unit.total_source_bytes = 0,
            m::LimitKind::Definitions => l.unit.definitions = 0,
            m::LimitKind::Fields => l.unit.fields = 0,
            _ => l.unit.depth = 0,
        }
        assert!(
            matches!(t.clone().verify(l).unwrap_err().kind,ErrorKind::Mir(m::ErrorKind::Limit{kind,..}) if kind==k)
        );
    }
    assert!(matches!(
        t.verify(Limits {
            constructed: 1,
            ..Limits::default()
        })
        .unwrap_err()
        .kind,
        ErrorKind::ConstructedLimit {
            requested: 2,
            limit: 1
        }
    ));
}

#[test]
fn deep_nominal_graphs_use_an_explicit_stack() {
    let mut t = table(&[]);
    for i in 0..400 {
        t.definitions.push(record(
            &format!("Type{i}"),
            false,
            &[if i == 399 { Type::I64 } else { n(i + 1) }],
        ));
    }
    assert!(matches!(
        t.clone().verify(Limits::default()).unwrap_err().kind,
        ErrorKind::Mir(m::ErrorKind::Limit {
            kind: m::LimitKind::Depth,
            ..
        })
    ));
    let mut l = Limits::default();
    l.unit.depth = 512;
    assert!(t.verify(l).unwrap().properties(n(0)).unwrap().is_copy);
}

#[test]
fn generated_type_dags_match_an_independent_recursive_copy_and_drop_oracle() {
    // No nominal recursion in this oracle: each new constructor refers only
    // to earlier nodes, so a separate scalar pair calculation is sufficient.
    for seed in 0..200u64 {
        let mut state = seed + 1;
        let mut t = table(&[]);
        let mut expected: Vec<(bool, bool)> = vec![];
        for index in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let child = if index == 0 {
                Type::I64
            } else {
                c((state as u32) % index)
            };
            let prior = if let Type::Constructed(id) = child {
                expected[id.0 as usize]
            } else {
                (true, false)
            };
            let (constructor, p) = match state % 6 {
                0 => (Constructor::Builtin(Builtin::Text), (false, true)),
                1 => (Constructor::Option(child), prior),
                2 => (Constructor::Result(Type::Bool, child), prior),
                3 => (Constructor::Vec(child), (false, true)),
                4 => (Constructor::Shared(child), (false, true)),
                _ => (Constructor::Weak(child), (false, true)),
            };
            t.constructed.push(Constructed {
                constructor,
                span: span(),
            });
            expected.push(p);
        }
        let v = t.verify(Limits::default()).unwrap();
        for (i, want) in expected.into_iter().enumerate() {
            let p = v.properties(c(i as u32)).unwrap();
            assert_eq!((p.is_copy, p.requires_drop), want, "seed {seed}, type {i}");
        }
    }
}
