#![no_main]

use lang_mir::unit::{self as m, ownership::*};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let byte = |i: usize| bytes.get(i).copied().unwrap_or(0);
    let span = m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 6,
    };
    let ty = |b: u8| match b % 4 {
        0 => Type::I64,
        1 => Type::Bool,
        2 => Type::Nominal(m::DefinitionId(u32::from(b / 4))),
        _ => Type::Constructed(ConstructedId(u32::from(b / 4))),
    };
    let mut table = TypeTable {
        sources: vec![m::Source {
            path: "types.lang".into(),
            text: "abcdé".into(),
        }],
        ..TypeTable::default()
    };
    for i in 0..usize::from(byte(0) % 32) {
        let a = ty(byte(3 * i + 3));
        let b = ty(byte(3 * i + 4));
        let constructor = match byte(3 * i + 2) % 16 {
            0 => Constructor::Builtin(Builtin::Unit),
            1 => Constructor::Builtin(Builtin::Text),
            2 => Constructor::Builtin(Builtin::Bytes),
            3 => Constructor::Builtin(Builtin::Allocator),
            4 => Constructor::Option(a),
            5 => Constructor::Result(a, b),
            6 => Constructor::Vec(a),
            7 => Constructor::Map(a, b),
            8 => Constructor::Shared(a),
            9 => Constructor::Weak(a),
            10 => Constructor::Arena(a),
            11 => Constructor::Loan(LoanKind::Shared, a),
            12 => Constructor::Loan(LoanKind::Exclusive, a),
            13 => Constructor::ArenaOwner(ArenaOwner {
                function: m::FunctionId(u32::from(byte(i))),
                local: u32::from(byte(i + 1)),
            }),
            14 => Constructor::Builtin(Builtin::ArenaError),
            _ => Constructor::Builtin(Builtin::OwnershipError),
        };
        table.constructed.push(Constructed { constructor, span });
    }
    for i in 0..usize::from(byte(1) % 16) {
        let payload = TypeUse {
            ty: ty(byte(3 * i + 100)),
            span,
        };
        let kind = if byte(i + 150) & 1 == 0 {
            DefinitionKind::Record(vec![Field {
                name: "field".into(),
                span,
                ty: payload,
            }])
        } else {
            DefinitionKind::Enum(vec![Case {
                name: "Case".into(),
                span,
                payload: vec![payload],
            }])
        };
        table.definitions.push(Definition {
            name: format!("Type{i}"),
            public: byte(i + 200) & 1 != 0,
            span,
            kind,
        });
    }
    if byte(250) & 1 != 0
        && let Some(t) = table.constructed.first_mut()
    {
        t.span.start = u32::from(byte(251));
        t.span.end = u32::from(byte(252));
        t.span.source = m::SourceId(u32::from(byte(253)));
    }
    let mut limits = Limits::default();
    limits.unit.depth = 32;
    limits.unit.work = 100_000;
    limits.unit.allocations = 2 * 1024 * 1024;
    limits.unit.elements = 100_000;
    if let Ok(verified) = table.verify(limits) {
        let replay = verified.table().clone().verify(limits).unwrap();
        assert_eq!(verified, replay);
        let compiled = lang_runtime::layout::Layouts::compile(&verified, limits.unit);
        if let Err(error) = &compiled {
            assert!(
                matches!(
                    error,
                    lang_runtime::layout::Error::Resource { .. }
                        | lang_runtime::layout::Error::Overflow
                        | lang_runtime::layout::Error::Allocation(
                            lang_runtime::allocation::Error::Overflow
                        )
                ),
                "unexpected layout failure for verified types: {error}"
            );
        }
        if let Ok(layouts) = compiled {
            let replay = lang_runtime::layout::Layouts::compile(&verified, limits.unit).unwrap();
            let types = [Type::I64, Type::Bool]
                .into_iter()
                .chain(
                    (0..verified.table().definitions.len())
                        .map(|i| Type::Nominal(m::DefinitionId(i as u32))),
                )
                .chain(
                    (0..verified.table().constructed.len())
                        .map(|i| Type::Constructed(ConstructedId(i as u32))),
                );
            for ty in types {
                let layout = layouts.get(ty).unwrap();
                assert_eq!(Some(layout), replay.get(ty));
                assert!(layout.alignment.is_power_of_two());
                assert_eq!(layout.size % layout.alignment, 0);
                layout.storage_requests().unwrap();
                let fields = |aggregate: &lang_runtime::layout::Aggregate, payload: u64| {
                    for field in &aggregate.fields {
                        let child = layouts.get(field.ty).unwrap();
                        assert_eq!((payload + field.offset) % child.alignment, 0);
                        assert!(payload + field.offset + child.size <= layout.size);
                        assert!(
                            field.metadata_offset + child.metadata_bytes <= layout.metadata_bytes
                        );
                    }
                };
                match &layout.kind {
                    lang_runtime::layout::Kind::Record(aggregate) => fields(aggregate, 0),
                    lang_runtime::layout::Kind::Enum {
                        payload_offset,
                        cases,
                    } => {
                        for case in cases {
                            fields(case, *payload_offset);
                        }
                    }
                    _ => {}
                }
            }
        }
        for i in 0..verified.table().constructed.len() {
            let t = Type::Constructed(ConstructedId(i as u32));
            assert_eq!(verified.equivalent(t, t), Some(true));
            assert!(
                verified
                    .properties(Type::Constructed(ConstructedId(i as u32)))
                    .is_some()
            );
        }
        assert!(
            verified
                .properties(Type::Constructed(ConstructedId(u32::MAX)))
                .is_none()
        );
        use lang_mir::unit::ownership::declarations as d;
        let mut declarations = d::Declarations {
            types: verified.table().clone(),
            ..Default::default()
        };
        for i in 0..usize::from(byte(260) % 4) {
            let mut locals = vec![];
            for j in 0..usize::from(byte(i + 261) % 8) {
                locals.push(d::Local {
                    name: format!("local{j}"),
                    span,
                    kind: match byte(i * 8 + j + 280) % 4 {
                        0 => d::LocalKind::Parameter,
                        1 => d::LocalKind::Mutable,
                        2 => d::LocalKind::Pattern,
                        _ => d::LocalKind::ArenaOwner,
                    },
                    ty: TypeUse {
                        ty: ty(byte(i * 8 + j + 320)),
                        span,
                    },
                });
            }
            declarations.functions.push(d::Function {
                abi: d::FunctionAbi::Guest,
                name: format!("function{i}"),
                public: byte(i + 350) & 1 != 0,
                span,
                signature_span: span,
                parameters: (0..usize::from(byte(i + 360) % 5))
                    .map(|j| d::LocalId(u32::from(byte(i * 5 + j + 370) % 16)))
                    .collect(),
                result: TypeUse {
                    ty: ty(byte(i + 400)),
                    span,
                },
                locals,
            });
        }
        if let Ok(checked) = declarations.verify(limits) {
            assert_eq!(
                checked.clone().into_declarations().verify(limits).unwrap(),
                checked
            );
            let mut budget = m::Budget::new(limits.unit);
            for (i, f) in checked.functions().iter().enumerate() {
                let arguments: Vec<_> = f
                    .parameters
                    .iter()
                    .map(|p| f.locals[p.0 as usize].ty.ty)
                    .collect();
                assert_eq!(
                    checked
                        .check_call(m::FunctionId(i as u32), &arguments, span, &mut budget)
                        .unwrap(),
                    f.result.ty
                );
            }
            use lang_mir::unit::ownership::places as p;
            let mut table = p::PlaceTable {
                declarations: checked.into_declarations(),
                functions: vec![],
            };
            for i in 0..table.declarations.functions.len() {
                let mut f = p::FunctionPlaces::default();
                for j in 0..usize::from(byte(i + 410) % 8) {
                    f.values.push(TypeUse {
                        ty: ty(byte(i * 8 + j + 420)),
                        span,
                    });
                }
                for j in 0..usize::from(byte(i + 450) % 8) {
                    let mut projections = vec![];
                    for k in 0..usize::from(byte(i * 8 + j + 480) % 4) {
                        projections.push(p::Projection {
                            kind: if byte(i * 8 + j + k + 490) & 1 == 0 {
                                p::ProjectionKind::Dereference
                            } else {
                                p::ProjectionKind::Field(u32::from(byte(i * 8 + j + k + 500) % 16))
                            },
                            span,
                        });
                    }
                    f.places.push(p::Place {
                        origin: m::Register(u32::from(byte(i * 8 + j + 460) % 16)),
                        root: if byte(i * 8 + j + 470) & 1 == 0 {
                            p::Root::Local(d::LocalId(u32::from(byte(i * 8 + j + 475) % 16)))
                        } else {
                            p::Root::Value(m::Register(u32::from(byte(i * 8 + j + 475) % 16)))
                        },
                        projections,
                        span,
                    });
                }
                table.functions.push(f);
            }
            let place_limits = p::Limits {
                places: 32,
                ownership: limits,
            };
            if let Ok(places) = table.verify(place_limits) {
                assert_eq!(
                    places.clone().into_table().verify(place_limits).unwrap(),
                    places
                );
                for (i, f) in places.functions().iter().enumerate() {
                    for (j, _) in f.places.iter().enumerate() {
                        places
                            .check_access(
                                m::FunctionId(i as u32),
                                p::PlaceId(j as u32),
                                p::Access::Read,
                                span,
                                &mut budget,
                            )
                            .unwrap();
                        let _ = places.check_access(
                            m::FunctionId(i as u32),
                            p::PlaceId(j as u32),
                            p::Access::Move,
                            span,
                            &mut budget,
                        );
                    }
                }
            }
        }
    }
});
