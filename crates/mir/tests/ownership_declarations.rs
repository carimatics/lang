use lang_mir::unit::{
    self as m,
    ownership::{self as o, declarations as d},
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

fn span(start: u32, end: u32) -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start,
        end,
    }
}
fn c(i: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(i))
}
fn n(i: u32) -> o::Type {
    o::Type::Nominal(m::DefinitionId(i))
}
fn table(constructors: &[o::Constructor]) -> o::TypeTable {
    o::TypeTable {
        sources: vec![m::Source {
            path: "declarations.lang".into(),
            text: "abcdefghij".into(),
        }],
        definitions: vec![],
        constructed: constructors
            .iter()
            .map(|&constructor| o::Constructed {
                constructor,
                span: span(0, 10),
            })
            .collect(),
    }
}
fn declarations(
    constructors: &[o::Constructor],
    parameters: &[o::Type],
    result: o::Type,
) -> d::Declarations {
    d::Declarations {
        types: table(constructors),
        imports: vec![],
        functions: vec![d::Function {
            abi: d::FunctionAbi::Guest,
            name: "f".into(),
            public: false,
            span: span(0, 10),
            signature_span: span(0, 3),
            parameters: (0..parameters.len())
                .map(|i| d::LocalId(i as u32))
                .collect(),
            result: o::TypeUse {
                ty: result,
                span: span(2, 3),
            },
            locals: parameters
                .iter()
                .enumerate()
                .map(|(i, &ty)| d::Local {
                    name: format!("p{i}"),
                    span: span(0, 1),
                    kind: d::LocalKind::Parameter,
                    ty: o::TypeUse {
                        ty,
                        span: span(1, 2),
                    },
                })
                .collect(),
        }],
    }
}
fn nominal(name: &str, public: bool) -> o::Definition {
    o::Definition {
        name: name.into(),
        public,
        span: span(0, 10),
        kind: o::DefinitionKind::Record(vec![]),
    }
}

#[test]
fn recursive_constructed_equivalence_preserves_nominal_loan_and_owner_identity() {
    use o::{Builtin as B, Constructor as C, LoanKind as L};
    let mut t = table(&[
        C::Builtin(B::Text),
        C::Builtin(B::Text),
        C::Option(c(0)),
        C::Option(c(1)),
        C::Vec(c(2)),
        C::Vec(c(3)),
        C::Loan(L::Shared, c(4)),
        C::Loan(L::Shared, c(5)),
        C::Loan(L::Exclusive, c(5)),
        C::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(0),
            local: 1,
        }),
        C::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(0),
            local: 1,
        }),
        C::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(1),
            local: 1,
        }),
    ]);
    t.definitions = vec![nominal("A", false), nominal("B", false)];
    let v = t.verify(o::Limits::default()).unwrap();
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (9, 10)] {
        assert_eq!(v.equivalent(c(a), c(b)), Some(true));
        assert_eq!(v.canonical(c(b)), Some(c(a)));
    }
    assert_eq!(v.equivalent(c(7), c(8)), Some(false));
    assert_eq!(v.equivalent(c(9), c(11)), Some(false));
    assert_eq!(v.equivalent(n(0), n(1)), Some(false));
    assert_eq!(v.equivalent(o::Type::I64, o::Type::Bool), Some(false));
    assert_eq!(v.equivalent(c(999), o::Type::I64), None);
}

#[test]
fn hash_bucket_collisions_never_merge_distinct_types_and_probes_are_budgeted() {
    let mut constructors = vec![];
    // The 32-entry table reserves 64 buckets. Deliberately collect distinct
    // owner symbols that collide in one bucket; full key equality must decide.
    for i in 0..100_000 {
        let key = o::Constructor::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(i),
            local: 9,
        });
        let mut h = DefaultHasher::new();
        key.hash(&mut h);
        if h.finish() & 63 == 0 {
            constructors.push(key);
            if constructors.len() == 32 {
                break;
            }
        }
    }
    assert_eq!(constructors.len(), 32);
    let t = table(&constructors);
    let v = t.clone().verify(o::Limits::default()).unwrap();
    for i in 0..32 {
        for j in 0..32 {
            assert_eq!(v.equivalent(c(i), c(j)), Some(i == j));
        }
    }
    let mut l = o::Limits::default();
    l.unit.work = v.work_used() - 1;
    assert!(matches!(
        t.verify(l).unwrap_err().kind,
        o::ErrorKind::Mir(m::ErrorKind::Limit {
            kind: m::LimitKind::Work,
            ..
        })
    ));
}

#[test]
fn generated_equivalence_matches_an_independent_expanded_type_key_oracle() {
    use o::{Builtin as B, Constructor as C};
    for seed in 0..80u64 {
        let mut state = seed + 7;
        let mut t = table(&[]);
        let mut keys: Vec<String> = vec![];
        for i in 0..24 {
            state = state
                .wrapping_mul(2862933555777941757)
                .wrapping_add(3037000493);
            let prior = if i == 0 {
                None
            } else {
                Some((state as usize) % i)
            };
            let (constructor, key) = match (state % 6, prior) {
                (0, Some(j)) => (t.constructed[j].constructor, keys[j].clone()),
                (1, Some(j)) => (C::Option(c(j as u32)), format!("option({})", keys[j])),
                (2, Some(j)) => (C::Vec(c(j as u32)), format!("vec({})", keys[j])),
                (3, Some(j)) => (
                    C::Result(o::Type::I64, c(j as u32)),
                    format!("result(i64,{})", keys[j]),
                ),
                (4, _) => (C::Builtin(B::Text), "text".into()),
                _ => (C::Builtin(B::Unit), "unit".into()),
            };
            t.constructed.push(o::Constructed {
                constructor,
                span: span(0, 10),
            });
            keys.push(key);
        }
        let v = t.verify(o::Limits::default()).unwrap();
        for i in 0..keys.len() {
            for j in 0..keys.len() {
                assert_eq!(
                    v.equivalent(c(i as u32), c(j as u32)),
                    Some(keys[i] == keys[j]),
                    "seed {seed} pair {i},{j}"
                );
            }
        }
    }
}

#[test]
fn callable_types_use_equivalence_and_never_erase_loan_kind_or_nominal_identity() {
    use o::{Builtin as B, Constructor as C, LoanKind as L, Type as T};
    let constructors = [
        C::Builtin(B::Text),
        C::Builtin(B::Text),
        C::Option(c(0)),
        C::Option(c(1)),
    ];
    let v = declarations(&constructors, &[c(2)], c(0))
        .verify(o::Limits::default())
        .unwrap();
    let mut budget = m::Budget::new(m::Limits::default());
    assert_eq!(
        v.check_call(m::FunctionId(0), &[c(3)], span(3, 4), &mut budget)
            .unwrap(),
        c(0)
    );
    assert!(matches!(
        v.check_call(m::FunctionId(0), &[], span(3, 4), &mut budget)
            .unwrap_err()
            .kind,
        o::ErrorKind::CallArity {
            expected: 1,
            actual: 0
        }
    ));
    assert_eq!(
        v.check_call(m::FunctionId(0), &[c(99)], span(3, 4), &mut budget)
            .unwrap_err()
            .kind,
        o::ErrorKind::InvalidType(c(99))
    );
    let mut a = declarations(&[], &[n(0)], T::I64);
    a.types.definitions = vec![nominal("A", false), nominal("B", false)];
    let v = a.verify(o::Limits::default()).unwrap();
    assert_eq!(
        v.check_call(m::FunctionId(0), &[n(1)], span(3, 4), &mut budget)
            .unwrap_err()
            .kind,
        o::ErrorKind::CallType {
            expected: n(0),
            actual: n(1)
        }
    );
    let v = declarations(
        &[C::Loan(L::Shared, T::I64), C::Loan(L::Exclusive, T::I64)],
        &[c(0)],
        T::I64,
    )
    .verify(o::Limits::default())
    .unwrap();
    assert_eq!(
        v.check_call(m::FunctionId(0), &[c(1)], span(3, 4), &mut budget)
            .unwrap_err()
            .kind,
        o::ErrorKind::CallType {
            expected: c(0),
            actual: c(1)
        }
    );
    assert!(matches!(
        v.check_call(m::FunctionId(99), &[], span(3, 4), &mut budget)
            .unwrap_err()
            .kind,
        o::ErrorKind::InvalidFunction(_)
    ));
    assert!(matches!(
        v.check_call(m::FunctionId(0), &[c(0)], span(3, 99), &mut budget)
            .unwrap_err()
            .kind,
        o::ErrorKind::Mir(m::ErrorKind::InvalidSpan)
    ));
    let zero = m::Limits {
        work: 0,
        ..m::Limits::default()
    };
    assert!(
        v.check_call(
            m::FunctionId(0),
            &[c(0)],
            span(3, 4),
            &mut m::Budget::new(zero)
        )
        .is_err()
    );
}

#[test]
fn user_signatures_reject_escaping_borrow_region_allocator_and_hidden_owner_types() {
    use o::{Builtin as B, Constructor as C, LoanKind as L, Type as T};
    for constructor in [
        C::Loan(L::Shared, T::I64),
        C::Arena(T::I64),
        C::Builtin(B::Allocator),
        C::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(0),
            local: 0,
        }),
    ] {
        let error = declarations(&[constructor], &[], c(0))
            .verify(o::Limits::default())
            .unwrap_err();
        assert_eq!(error.kind, o::ErrorKind::InvalidSignatureType(c(0)));
        assert_eq!(error.span, Some(span(2, 3)));
    }
    let nested = [
        C::Loan(L::Shared, T::I64),
        C::Option(c(0)),
        C::Loan(L::Shared, c(1)),
        C::Loan(L::Shared, c(0)),
    ];
    for t in [c(1), c(2)] {
        assert_eq!(
            declarations(&nested, &[t], T::I64)
                .verify(o::Limits::default())
                .unwrap_err()
                .kind,
            o::ErrorKind::InvalidSignatureType(t)
        );
    }
    declarations(&nested, &[c(3)], T::I64)
        .verify(o::Limits::default())
        .unwrap();
    assert!(
        declarations(&[C::Builtin(B::Allocator)], &[c(0)], T::I64)
            .verify(o::Limits::default())
            .is_err()
    );
    declarations(
        &[C::Builtin(B::Allocator), C::Loan(L::Shared, c(0))],
        &[c(1)],
        T::I64,
    )
    .verify(o::Limits::default())
    .unwrap();
    declarations(&[C::Arena(T::I64)], &[c(0)], T::I64)
        .verify(o::Limits::default())
        .unwrap();
}

#[test]
fn public_signatures_reject_transitive_private_types() {
    let mut a = declarations(&[o::Constructor::Vec(n(0))], &[c(0)], o::Type::I64);
    a.types.definitions.push(nominal("Hidden", false));
    a.functions[0].public = true;
    assert_eq!(
        a.verify(o::Limits::default()).unwrap_err().kind,
        o::ErrorKind::PrivateSignatureType(c(0))
    );
}

#[test]
fn arena_symbols_resolve_to_the_exact_function_local_owner_declaration() {
    let owner = o::ArenaOwner {
        function: m::FunctionId(0),
        local: 0,
    };
    let mut a = declarations(&[o::Constructor::ArenaOwner(owner)], &[], o::Type::I64);
    a.functions[0].locals.push(d::Local {
        name: "graph".into(),
        span: span(4, 5),
        kind: d::LocalKind::ArenaOwner,
        ty: o::TypeUse {
            ty: c(0),
            span: span(4, 5),
        },
    });
    a.clone().verify(o::Limits::default()).unwrap();
    let mut bad = a.clone();
    bad.functions[0].locals[0].kind = d::LocalKind::Mutable;
    assert_eq!(
        bad.verify(o::Limits::default()).unwrap_err().kind,
        o::ErrorKind::InvalidLocalType(c(0))
    );
    let mut bad = a.clone();
    bad.functions[0].locals[0].ty.ty = o::Type::I64;
    assert_eq!(
        bad.verify(o::Limits::default()).unwrap_err().kind,
        o::ErrorKind::InvalidOwner(owner)
    );
    let mut bad = a.clone();
    bad.functions.clear();
    assert_eq!(
        bad.verify(o::Limits::default()).unwrap_err().kind,
        o::ErrorKind::InvalidFunction(m::FunctionId(0))
    );
    let mut bad = a;
    bad.functions[0].locals.clear();
    assert_eq!(
        bad.verify(o::Limits::default()).unwrap_err().kind,
        o::ErrorKind::InvalidLocal {
            function: m::FunctionId(0),
            local: 0
        }
    );
}

#[test]
fn parameter_bijection_names_types_and_spans_are_independently_checked() {
    let original = declarations(&[], &[o::Type::I64, o::Type::Bool], o::Type::I64);
    for (mut a, which) in [
        (original.clone(), 0),
        (original.clone(), 1),
        (original.clone(), 2),
        (original.clone(), 3),
        (original.clone(), 4),
    ] {
        match which {
            0 => a.functions[0].parameters.push(d::LocalId(0)),
            1 => {
                a.functions[0].parameters.pop();
            }
            2 => a.functions[0].locals[0].kind = d::LocalKind::Mutable,
            3 => a.functions[0].parameters[0] = d::LocalId(99),
            _ => a.functions[0].locals[0].ty.ty = c(99),
        };
        assert!(a.verify(o::Limits::default()).is_err());
    }
    let mut a = original.clone();
    a.functions[0].locals[1].name = "p0".into();
    assert_eq!(
        a.verify(o::Limits::default()).unwrap_err().kind,
        o::ErrorKind::Mir(m::ErrorKind::DuplicateName)
    );
    let mut a = original.clone();
    a.functions[0].locals[1].ty.span = span(9, 11);
    assert!(matches!(
        a.verify(o::Limits::default()).unwrap_err().kind,
        o::ErrorKind::Mir(m::ErrorKind::InvalidSpan)
    ));
    let mut a = original;
    a.functions[0].signature_span = span(0, 1);
    assert_eq!(
        a.verify(o::Limits::default()).unwrap_err().kind,
        o::ErrorKind::SpanOutsideOwner
    );
}

#[test]
fn imports_remain_the_rfc0006_scalar_host_contract() {
    let mut a = declarations(&[], &[], o::Type::I64);
    a.imports.push(d::Import {
        abi: d::ImportAbi::Host,
        name: "host".into(),
        span: span(0, 10),
        parameters: vec![o::TypeUse {
            ty: o::Type::I64,
            span: span(0, 1),
        }],
        result: o::TypeUse {
            ty: o::Type::I64,
            span: span(2, 3),
        },
    });
    let v = a.clone().verify(o::Limits::default()).unwrap();
    let mut budget = m::Budget::new(m::Limits::default());
    assert_eq!(
        v.check_import(m::ImportId(0), &[o::Type::I64], span(3, 4), &mut budget)
            .unwrap(),
        o::Type::I64
    );
    assert!(
        v.check_import(m::ImportId(2), &[o::Type::I64], span(3, 4), &mut budget)
            .is_err()
    );
    a.imports[0].result.ty = o::Type::Bool;
    assert_eq!(
        a.verify(o::Limits::default()).unwrap_err().kind,
        o::ErrorKind::InvalidHostSignature
    );
}

fn foreign_import(
    constructors: &[o::Constructor],
    parameters: &[o::Type],
    result: o::Type,
) -> d::Declarations {
    let mut table = declarations(constructors, &[], o::Type::I64);
    table.imports.push(d::Import {
        abi: d::ImportAbi::C,
        name: "foreign".into(),
        span: span(0, 10),
        parameters: parameters
            .iter()
            .map(|&ty| o::TypeUse {
                ty,
                span: span(0, 1),
            })
            .collect(),
        result: o::TypeUse {
            ty: result,
            span: span(2, 3),
        },
    });
    table
}

#[test]
fn c_signatures_admit_finite_copy_values_without_widening_legacy_hosts() {
    use o::{Builtin as B, Constructor as C};
    let constructors = [
        C::Builtin(B::Unit),
        C::Builtin(B::AllocError),
        C::Option(o::Type::Bool),
        C::Result(c(2), c(1)),
    ];
    let mut table = foreign_import(&constructors, &[o::Type::Bool, c(0), c(3), n(0)], c(3));
    table.types.definitions.push(o::Definition {
        name: "Pair".into(),
        public: true,
        span: span(0, 10),
        kind: o::DefinitionKind::Record(vec![o::Field {
            name: "value".into(),
            span: span(0, 2),
            ty: o::TypeUse {
                ty: o::Type::I64,
                span: span(1, 2),
            },
        }]),
    });
    assert_eq!(
        table
            .clone()
            .verify(Default::default())
            .unwrap()
            .foreign_interface_span(),
        Some(span(0, 10))
    );
    table.imports[0].abi = d::ImportAbi::Host;
    assert_eq!(
        table.verify(Default::default()).unwrap_err().kind,
        o::ErrorKind::InvalidHostSignature
    );
    assert!(
        foreign_import(&[C::Builtin(B::Unit)], &[], c(0))
            .verify(Default::default())
            .is_ok()
    );
}

#[test]
fn c_signatures_reject_owners_and_even_copy_loans() {
    use o::{Builtin as B, Constructor as C};
    for constructor in [
        C::Builtin(B::Allocator),
        C::Builtin(B::Text),
        C::Builtin(B::Bytes),
        C::Vec(o::Type::I64),
        C::Map(o::Type::I64, o::Type::I64),
        C::Shared(n(0)),
        C::Weak(n(0)),
        C::Arena(n(0)),
        C::Loan(o::LoanKind::Shared, o::Type::I64),
    ] {
        let mut table = foreign_import(&[constructor], &[c(0)], o::Type::I64);
        table.types.definitions.push(nominal("Node", true));
        let error = table.verify(Default::default()).unwrap_err();
        assert_eq!(
            error.kind,
            o::ErrorKind::InvalidCSignatureType(c(0)),
            "{constructor:?}"
        );
        assert_eq!(error.span, Some(span(0, 1)));
    }
    let table = foreign_import(
        &[C::Loan(o::LoanKind::Shared, o::Type::I64), C::Option(c(0))],
        &[c(1)],
        o::Type::I64,
    );
    assert_eq!(
        table.verify(Default::default()).unwrap_err().kind,
        o::ErrorKind::InvalidCSignatureType(c(1))
    );
}

#[test]
fn c_profile_depth_and_parameter_bounds_remain_hard_with_relaxed_general_limits() {
    let mut constructors = vec![o::Constructor::Builtin(o::Builtin::Unit)];
    for i in 0..64 {
        constructors.push(o::Constructor::Option(c(i)));
    }
    let mut limits = o::Limits::default();
    limits.unit.depth = 128;
    let boundary = foreign_import(&constructors[..64], &[c(63)], o::Type::I64).verify(limits);
    assert!(boundary.is_ok(), "{:?}", boundary.err());
    assert!(matches!(
        foreign_import(&constructors, &[c(64)], o::Type::I64)
            .verify(limits)
            .unwrap_err()
            .kind,
        o::ErrorKind::CProfileLimit {
            kind: o::CProfileLimit::Depth,
            requested: 65,
            limit: 64
        }
    ));
    limits.unit.fields = 300;
    assert!(
        foreign_import(&[], &vec![o::Type::I64; 256], o::Type::I64)
            .verify(limits)
            .is_ok()
    );
    assert!(matches!(
        foreign_import(&[], &vec![o::Type::I64; 257], o::Type::I64)
            .verify(limits)
            .unwrap_err()
            .kind,
        o::ErrorKind::CProfileLimit {
            kind: o::CProfileLimit::Parameters,
            requested: 257,
            limit: 256
        }
    ));
}

#[test]
fn c_exports_require_public_root_names_and_valid_signature_spans() {
    let mut table = declarations(&[], &[o::Type::Bool], o::Type::I64);
    table.functions[0].abi = d::FunctionAbi::CExport;
    assert_eq!(
        table.clone().verify(Default::default()).unwrap_err().kind,
        o::ErrorKind::InvalidCExport(m::FunctionId(0))
    );
    table.functions[0].public = true;
    assert!(table.clone().verify(Default::default()).is_ok());
    for name in ["nested::f".to_owned(), "a".repeat(129)] {
        let mut forged = table.clone();
        forged.functions[0].name = name;
        assert_eq!(
            forged.verify(Default::default()).unwrap_err().kind,
            o::ErrorKind::InvalidCExport(m::FunctionId(0))
        );
    }
    table.functions[0].result.span = span(3, 4);
    assert_eq!(
        table.verify(Default::default()).unwrap_err().kind,
        o::ErrorKind::SpanOutsideOwner
    );
}

#[test]
fn declaration_and_type_budgets_are_cumulative_and_exact() {
    let a = declarations(&[o::Constructor::Builtin(o::Builtin::Text)], &[c(0)], c(0));
    let v = a.clone().verify(o::Limits::default()).unwrap();
    for (k, used) in [
        (m::LimitKind::Work, v.work_used()),
        (m::LimitKind::Allocations, v.requested_bytes()),
        (m::LimitKind::Elements, v.elements_used()),
    ] {
        let mut l = o::Limits::default();
        match k {
            m::LimitKind::Work => l.unit.work = used,
            m::LimitKind::Allocations => l.unit.allocations = used,
            _ => l.unit.elements = used,
        }
        a.clone().verify(l).unwrap();
        match k {
            m::LimitKind::Work => l.unit.work -= 1,
            m::LimitKind::Allocations => l.unit.allocations -= 1,
            _ => l.unit.elements -= 1,
        }
        assert!(
            matches!(a.clone().verify(l).unwrap_err().kind,o::ErrorKind::Mir(m::ErrorKind::Limit{kind,..}) if kind==k)
        );
    }
    let mut l = o::Limits::default();
    l.unit.functions = 0;
    assert!(a.clone().verify(l).is_err());
    let mut l = o::Limits::default();
    l.unit.registers = 0;
    assert!(a.clone().verify(l).is_err());
    let mut l = o::Limits::default();
    l.unit.fields = 0;
    assert!(a.verify(l).is_err());
}
