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
        for i in 0..verified.table().constructed.len() {
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
    }
});
