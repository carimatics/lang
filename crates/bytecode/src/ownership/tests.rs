use super::*;
use o::{bodies as b, declarations as d, places as p};

const SPAN: m::Span = m::Span {
    source: m::SourceId(0),
    start: 0,
    end: 1,
};
fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn type_use(ty: o::Type) -> o::TypeUse {
    o::TypeUse { ty, span: SPAN }
}
fn hex(source: &str) -> Vec<u8> {
    let source: String = source.chars().filter(|c| !c.is_whitespace()).collect();
    source
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn scalar() -> b::BodyTable {
    b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "x".into(),
                        text: "x".into(),
                    }],
                    ..Default::default()
                },
                imports: vec![],
                functions: vec![d::Function {
                    name: "main".into(),
                    public: false,
                    span: SPAN,
                    signature_span: SPAN,
                    parameters: vec![],
                    result: type_use(o::Type::I64),
                    locals: vec![],
                }],
            },
            functions: vec![p::FunctionPlaces {
                values: vec![type_use(o::Type::I64)],
                places: vec![],
            }],
        },
        functions: vec![b::FunctionBody {
            loans: 0,
            prepared: 0,
            blocks: vec![b::Block {
                parameters: vec![],
                instructions: vec![b::Instruction {
                    results: vec![m::Register(0)],
                    op: b::Op::I64(42),
                    span: SPAN,
                }],
                terminator: b::Terminator {
                    kind: b::Term::Return(m::Register(0)),
                    span: SPAN,
                },
                span: SPAN,
            }],
        }],
    }
}
fn bytes_table() -> b::BodyTable {
    let mut table = scalar();
    let c = |id| o::Type::Constructed(o::ConstructedId(id));
    table.places.declarations.types.constructed = [
        o::Constructor::Builtin(o::Builtin::Allocator),
        o::Constructor::Loan(o::LoanKind::Shared, c(0)),
        o::Constructor::Builtin(o::Builtin::Bytes),
        o::Constructor::Builtin(o::Builtin::AllocError),
        o::Constructor::Result(c(2), c(3)),
    ]
    .into_iter()
    .map(|constructor| o::Constructed {
        constructor,
        span: SPAN,
    })
    .collect();
    table.places.functions[0].values = vec![type_use(c(1)), type_use(c(4)), type_use(o::Type::I64)];
    let core = |operation, arguments, result_loan| {
        b::Op::CoreCall(o::core::Call {
            operation,
            type_arguments: vec![],
            arguments,
            result_loan,
        })
    };
    let ins = |result, op| b::Instruction {
        results: result,
        op,
        span: SPAN,
    };
    table.functions[0].loans = 1;
    let block = &mut table.functions[0].blocks[0];
    block.instructions = vec![
        ins(
            vec![m::Register(0)],
            core(o::core::Operation::Allocator, vec![], Some(b::LoanId(0))),
        ),
        ins(
            vec![m::Register(1)],
            core(o::core::Operation::BytesNew, vec![m::Register(0)], None),
        ),
        ins(
            vec![],
            b::Op::EndBorrow {
                loan: b::LoanId(0),
                mode: b::EndMode::Required,
            },
        ),
        ins(vec![], b::Op::DropValue(m::Register(1))),
        ins(vec![m::Register(2)], b::Op::I64(0)),
    ];
    block.terminator.kind = b::Term::Return(m::Register(2));
    table
}

#[test]
fn independent_scalar_and_bytes_vectors_freeze_schema_two() {
    for (raw, text, len) in [
        (
            scalar(),
            include_str!("../../tests/fixtures/unit-v2-main-42.hex"),
            212,
        ),
        (
            bytes_table(),
            include_str!("../../tests/fixtures/unit-v2-bytes.hex"),
            443,
        ),
    ] {
        let expected = hex(text);
        assert_eq!(expected.len(), len);
        let encoded = encode(raw.clone(), Some(m::FunctionId(0)), CodecLimits::default()).unwrap();
        assert!(
            encoded == expected,
            "first byte difference: {:?}",
            encoded.iter().zip(&expected).position(|(a, b)| a != b)
        );
        let program = decode(&expected, CodecLimits::default()).unwrap();
        let (decoded, entry) = program.into_parts();
        assert_eq!(decoded, raw);
        assert_eq!(entry, Some(m::FunctionId(0)));
        assert_eq!(
            encode(decoded, entry, CodecLimits::default()).unwrap(),
            expected
        );
    }
}

#[test]
fn old_and_new_schemas_reject_each_others_version() {
    let old = hex(include_str!("../../tests/fixtures/unit-v1-main-42.hex"));
    assert!(matches!(
        decode(&old, CodecLimits::default()).err().unwrap().kind,
        CodecErrorKind::Version(1)
    ));
    let new = hex(include_str!("../../tests/fixtures/unit-v2-main-42.hex"));
    assert!(matches!(
        crate::unit::decode(&new, crate::unit::CodecLimits::default())
            .err()
            .unwrap()
            .kind,
        crate::unit::CodecErrorKind::Version(2)
    ));
}

#[test]
fn every_ownership_opcode_has_specified_operand_bytes() {
    use b::Op as O;
    let r = m::Register;
    let place = p::PlaceId;
    let local = d::LocalId;
    let loan = b::LoanId;
    let prepared = b::PreparedPlaceId;
    let mut cleanup = words(&[4]);
    let steps = [
        b::CleanupTarget::Local(local(3)),
        b::CleanupTarget::Value(r(5)),
        b::CleanupTarget::Loan(loan(7)),
        b::CleanupTarget::Arena(local(9)),
    ];
    for (tag, id) in [(0, 3), (1, 5), (2, 7), (3, 9)] {
        cleanup.push(tag);
        cleanup.extend(words(&[id, 0, 0, 1]));
    }
    let core = o::core::Call {
        operation: o::core::Operation::BytesPush,
        type_arguments: vec![type_use(o::Type::I64)],
        arguments: vec![r(5), r(7)],
        result_loan: Some(loan(9)),
    };
    let core_bytes = [
        vec![11],
        words(&[1]),
        vec![0],
        words(&[0, 0, 1, 2, 5, 7]),
        vec![1],
        words(&[9]),
    ]
    .concat();
    let cases = vec![
        (O::I64(-42), 0, (-42_i64).to_le_bytes().to_vec()),
        (O::Bool(true), 1, vec![1]),
        (O::Unit, 2, vec![]),
        (O::Add(r(3), r(5)), 3, words(&[3, 5])),
        (O::LessThan(r(3), r(5)), 4, words(&[3, 5])),
        (O::EqualI64(r(3), r(5)), 5, words(&[3, 5])),
        (O::Copy(r(3)), 6, words(&[3])),
        (O::Move(r(3)), 7, words(&[3])),
        (O::CopyPlace(place(3)), 8, words(&[3])),
        (O::MovePlace(place(3)), 9, words(&[3])),
        (
            O::Initialize {
                local: local(3),
                value: r(5),
            },
            10,
            words(&[3, 5]),
        ),
        (
            O::Replace {
                place: place(3),
                value: r(5),
            },
            11,
            words(&[3, 5]),
        ),
        (
            O::PreparePlace {
                destination: prepared(3),
                place: place(5),
            },
            12,
            words(&[3, 5]),
        ),
        (
            O::ReplacePrepared {
                destination: prepared(3),
                value: r(5),
            },
            13,
            words(&[3, 5]),
        ),
        (O::DiscardPrepared(prepared(3)), 14, words(&[3])),
        (O::DropValue(r(3)), 15, words(&[3])),
        (O::DropLocal(local(3)), 16, words(&[3])),
        (O::DropLocalIfInitialized(local(3)), 17, words(&[3])),
        (O::DropValueIfAvailable(r(3)), 18, words(&[3])),
        (
            O::Cleanup(b::Cleanup {
                steps: steps
                    .into_iter()
                    .map(|target| b::CleanupStep { target, span: SPAN })
                    .collect(),
            }),
            19,
            cleanup,
        ),
        (
            O::Borrow {
                loan: loan(3),
                kind: o::LoanKind::Exclusive,
                place: place(5),
            },
            20,
            [words(&[3]), vec![1], words(&[5])].concat(),
        ),
        (
            O::EndBorrow {
                loan: loan(3),
                mode: b::EndMode::IfActive,
            },
            21,
            [words(&[3]), vec![1]].concat(),
        ),
        (
            O::ArenaEnter {
                owner: local(3),
                allocator: r(5),
            },
            22,
            words(&[3, 5]),
        ),
        (O::ArenaExit { owner: local(3) }, 23, words(&[3])),
        (
            O::Call {
                function: m::FunctionId(3),
                arguments: vec![r(5), r(7)],
            },
            24,
            words(&[3, 2, 5, 7]),
        ),
        (
            O::Import {
                import: m::ImportId(3),
                arguments: vec![r(5), r(7)],
            },
            25,
            words(&[3, 2, 5, 7]),
        ),
        (O::CoreCall(core), 26, core_bytes),
        (
            O::Record {
                definition: m::DefinitionId(3),
                fields: vec![r(5), r(7)],
                initialization_order: vec![1, 0],
            },
            27,
            words(&[3, 2, 5, 7, 2, 1, 0]),
        ),
        (
            O::Variant {
                case: 3,
                payload: vec![r(5), r(7)],
            },
            28,
            words(&[3, 2, 5, 7]),
        ),
        (
            O::IsCase {
                value: r(3),
                case: 5,
            },
            29,
            words(&[3, 5]),
        ),
        (
            O::CopyCaseField {
                value: r(3),
                case: 5,
                field: 7,
                guard: r(9),
            },
            30,
            words(&[3, 5, 7, 9]),
        ),
        (
            O::BorrowCaseField {
                loan: loan(3),
                value: r(5),
                case: 7,
                field: 9,
                guard: r(11),
            },
            31,
            words(&[3, 5, 7, 9, 11]),
        ),
        (
            O::UnpackVariant {
                value: r(3),
                case: 5,
                guard: r(7),
            },
            32,
            words(&[3, 5, 7]),
        ),
    ];
    assert_eq!(cases.len(), 33);
    for (op, tag, payload) in cases {
        // These are transport-only instruction vectors, deliberately distinct
        // from whole-unit semantic admission and executable artifact evidence.
        let instruction = b::Instruction {
            results: vec![r(11), r(13)],
            op,
            span: SPAN,
        };
        let expected = [words(&[2, 11, 13]), vec![tag], payload, words(&[0, 0, 1])].concat();
        assert_eq!(
            writer::instruction_fixture(&instruction),
            expected,
            "opcode {tag}"
        );
        assert_eq!(
            reader::instruction_fixture(&expected).unwrap(),
            instruction,
            "opcode {tag}"
        );
    }
}

fn patch_length(bytes: &mut [u8]) {
    let n = u32::try_from(bytes.len() - 16).unwrap();
    bytes[12..16].copy_from_slice(&n.to_le_bytes());
}

#[test]
fn malformed_envelopes_tags_counts_spans_and_every_truncation_are_refused() {
    let fixture = hex(include_str!("../../tests/fixtures/unit-v2-main-42.hex"));
    for (offset, value) in [
        (0, 0),
        (8, 3),
        (10, 1),
        (24, 255),
        (24, b'.'),
        (24, b'/'),
        (29, 255),
        (29, 0),
        (50, 0),
        (54, 2),
        (83, 255),
        (108, 255),
        (157, 255),
        (178, 255),
        (207, 2),
    ] {
        let mut bad = fixture.clone();
        bad[offset] = value;
        assert!(
            decode(&bad, CodecLimits::default()).is_err(),
            "byte {offset}"
        );
    }
    for offset in [55, 153, 179, 208] {
        let mut bad = fixture.clone();
        bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(
            decode(&bad, CodecLimits::default()).is_err(),
            "ID/span {offset}"
        );
    }
    let mut bad = fixture.clone();
    bad[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode(&bad, CodecLimits::default()).err().unwrap().kind,
        CodecErrorKind::Limit {
            table: Table::Sources,
            ..
        }
    ));
    let mut bad = fixture.clone();
    bad[104..108].copy_from_slice(&1_000_000_u32.to_le_bytes());
    assert!(matches!(
        decode(&bad, CodecLimits::default()).err().unwrap().kind,
        CodecErrorKind::Truncated
    ));
    let mut trailing = fixture.clone();
    trailing.push(0);
    patch_length(&mut trailing);
    assert!(matches!(
        decode(&trailing, CodecLimits::default())
            .err()
            .unwrap()
            .kind,
        CodecErrorKind::TrailingBytes
    ));
    assert!(matches!(
        decode(
            &fixture,
            CodecLimits {
                bytes: fixture.len() - 1,
                ..Default::default()
            }
        )
        .err()
        .unwrap()
        .kind,
        CodecErrorKind::TooLarge { .. }
    ));
    for text in [
        include_str!("../../tests/fixtures/unit-v2-main-42.hex"),
        include_str!("../../tests/fixtures/unit-v2-bytes.hex"),
    ] {
        let bytes = hex(text);
        for end in 0..bytes.len() {
            assert!(
                decode(&bytes[..end], CodecLimits::default()).is_err(),
                "original length, prefix {end}"
            );
            if end >= 16 {
                let mut repaired = bytes[..end].to_vec();
                patch_length(&mut repaired);
                assert!(
                    decode(&repaired, CodecLimits::default()).is_err(),
                    "repaired length, prefix {end}"
                );
            }
        }
        for offset in 16..bytes.len() {
            let mut changed = bytes.clone();
            changed[offset] ^= 0xff;
            if let Ok(program) = decode(&changed, CodecLimits::default()) {
                let (raw, entry) = program.into_parts();
                assert_eq!(
                    encode(raw, entry, CodecLimits::default()).unwrap(),
                    changed,
                    "canonical mutation {offset}"
                );
            }
        }
    }
}

#[test]
fn decode_and_all_proofs_share_exact_work_byte_and_element_limits() {
    let bytes = hex(include_str!("../../tests/fixtures/unit-v2-bytes.hex"));
    let usage = decode(&bytes, CodecLimits::default()).unwrap().usage();
    for kind in [
        m::LimitKind::Work,
        m::LimitKind::Allocations,
        m::LimitKind::Elements,
    ] {
        let exact = match kind {
            m::LimitKind::Work => usage.work,
            m::LimitKind::Allocations => usage.allocation_bytes,
            _ => usage.elements,
        };
        for (limit, allowed) in [(exact, true), (exact - 1, false), (0, false)] {
            let mut cfg = CodecLimits::default();
            // Tighten only the final phase. It must cap earlier decoding too,
            // and cannot give each proof stage a fresh allowance.
            match kind {
                m::LimitKind::Work => cfg.verification.failure.work = limit,
                m::LimitKind::Allocations => cfg.verification.failure.allocations = limit,
                _ => cfg.verification.failure.elements = limit,
            }
            assert_eq!(decode(&bytes, cfg).is_ok(), allowed, "{kind:?} {limit}");
        }
    }
    let mut invalid = bytes_table();
    invalid.functions[0].blocks[0].instructions.insert(
        4,
        b::Instruction {
            results: vec![],
            op: b::Op::DropValue(m::Register(1)),
            span: SPAN,
        },
    );
    assert!(matches!(
        encode(invalid, Some(m::FunctionId(0)), CodecLimits::default())
            .err()
            .unwrap()
            .kind,
        CodecErrorKind::Moves(_)
    ));
    for entry in [None, Some(m::FunctionId(1))] {
        let result = encode(scalar(), entry, CodecLimits::default());
        assert_eq!(result.is_ok(), entry.is_none());
    }
}

#[test]
fn forged_typed_operations_and_repeated_end_or_drop_reach_independent_proofs() {
    let seed = hex(include_str!("../../tests/fixtures/unit-v2-bytes.hex"));
    let mut wrong_signature = seed.clone();
    wrong_signature[276] = 7;
    assert!(matches!(
        decode(&wrong_signature, CodecLimits::default())
            .err()
            .unwrap()
            .kind,
        CodecErrorKind::Types(_)
    ));
    // Fixed independent vector offsets: count at 263; EndBorrow instruction
    // [337,359), DropValue [359,380). Update only framing and instruction count.
    for (start, end, activity) in [(337, 359, true), (359, 380, false)] {
        let mut bytes = seed.clone();
        bytes.splice(end..end, seed[start..end].iter().copied());
        bytes[263..267].copy_from_slice(&6_u32.to_le_bytes());
        patch_length(&mut bytes);
        let err = decode(&bytes, CodecLimits::default()).err().unwrap();
        assert!(
            if activity {
                matches!(err.kind, CodecErrorKind::Activity(_))
            } else {
                matches!(err.kind, CodecErrorKind::Moves(_))
            },
            "{err:?}"
        );
    }
}

#[test]
fn all_core_operation_tags_are_frozen_by_the_wire_contract() {
    use o::core::Operation as C;
    let expected = [
        C::Allocator,
        C::TextFromUtf8,
        C::TextClone,
        C::TextLen,
        C::TextBytes,
        C::TextEqual,
        C::TextConcat,
        C::BytesNew,
        C::BytesClone,
        C::BytesLen,
        C::BytesGet,
        C::BytesPush,
        C::BytesSet,
        C::VecNew,
        C::VecLen,
        C::VecPush,
        C::VecPop,
        C::VecGet,
        C::VecGetMut,
        C::MapNew,
        C::MapLen,
        C::MapInsert,
        C::MapGet,
        C::MapRemove,
        C::SharedNew,
        C::SharedClone,
        C::SharedRead,
        C::SharedReplace,
        C::WeakNew,
        C::WeakClone,
        C::WeakUpgrade,
        C::ArenaNew,
        C::ArenaRead,
        C::ArenaReplace,
        C::OptionUnwrap,
        C::ResultUnwrap,
        C::ResultUnwrapErr,
    ];
    for (tag, operation) in expected.into_iter().enumerate() {
        let ins = b::Instruction {
            results: vec![],
            op: b::Op::CoreCall(o::core::Call {
                operation,
                type_arguments: vec![],
                arguments: vec![],
                result_loan: None,
            }),
            span: SPAN,
        };
        let bytes = writer::instruction_fixture(&ins);
        let expected = [
            words(&[0]),
            vec![26, tag as u8],
            words(&[0, 0]),
            vec![0],
            words(&[0, 0, 1]),
        ]
        .concat();
        assert_eq!(bytes, expected);
        assert_eq!(reader::instruction_fixture(&expected).unwrap(), ins);
    }
}

#[test]
fn encoder_limits_cover_proofs_both_passes_and_output_reservation() {
    let cfg = CodecLimits::default();
    let usage = verify(
        bytes_table(),
        Some(m::FunctionId(0)),
        m::Budget::new(cfg.budget_limits()),
        cfg,
        0,
    )
    .unwrap()
    .usage();
    for kind in [m::LimitKind::Work, m::LimitKind::Allocations] {
        let mut cap = cfg;
        match kind {
            m::LimitKind::Work => cap.verification.failure.work = usage.work,
            _ => cap.verification.failure.allocations = usage.allocation_bytes,
        }
        assert!(
            encode(bytes_table(), Some(m::FunctionId(0)), cap).is_err(),
            "{kind:?}: proof alone cannot cover encoding"
        );
    }
    for (maximum, ok) in [(443, true), (442, false)] {
        assert_eq!(
            encode(
                bytes_table(),
                Some(m::FunctionId(0)),
                CodecLimits {
                    bytes: maximum,
                    ..cfg
                }
            )
            .is_ok(),
            ok
        );
    }
    let mut cap = cfg;
    cap.verification.failure.elements = usage.elements;
    assert!(encode(bytes_table(), Some(m::FunctionId(0)), cap).is_ok());
    cap.verification.failure.elements -= 1;
    assert!(encode(bytes_table(), Some(m::FunctionId(0)), cap).is_err());
    let mut cap = cfg;
    cap.verification.bodies.loans = 0;
    assert!(encode(bytes_table(), Some(m::FunctionId(0)), cap).is_err());
    let bytes = hex(include_str!("../../tests/fixtures/unit-v2-bytes.hex"));
    assert!(matches!(
        decode(&bytes, cap).err().unwrap().kind,
        CodecErrorKind::Limit {
            table: Table::Loans,
            ..
        }
    ));
}

#[test]
fn schema_two_preserves_unsorted_global_order_and_checked_entry_identity() {
    let mut raw = scalar();
    let mut extra = raw.places.declarations.functions[0].clone();
    extra.name = "aaa".into();
    raw.places.declarations.functions.push(extra);
    raw.places.functions.push(raw.places.functions[0].clone());
    raw.functions.push(raw.functions[0].clone());
    let encoded = encode(raw.clone(), Some(m::FunctionId(1)), CodecLimits::default()).unwrap();
    let decoded = decode(&encoded, CodecLimits::default()).unwrap();
    assert_eq!(decoded.entry(), Some(m::FunctionId(1)));
    let (table, entry) = decoded.into_parts();
    assert_eq!(table, raw);
    assert_eq!(
        encode(table, entry, CodecLimits::default()).unwrap(),
        encoded
    );
}

#[test]
fn cfg_arguments_preserve_repeated_copy_inputs_and_distinct_target_arities() {
    let mut raw = scalar();
    raw.places.functions[0].values = vec![
        type_use(o::Type::I64),
        type_use(o::Type::Bool),
        type_use(o::Type::I64),
        type_use(o::Type::I64),
        type_use(o::Type::I64),
        type_use(o::Type::I64),
    ];
    let block = &mut raw.functions[0].blocks[0];
    block.instructions.push(b::Instruction {
        results: vec![m::Register(1)],
        op: b::Op::Bool(true),
        span: SPAN,
    });
    block.terminator.kind = b::Term::Branch {
        condition: m::Register(1),
        then_target: m::Target {
            block: m::BlockId(1),
            arguments: vec![m::Register(0)],
        },
        else_target: m::Target {
            block: m::BlockId(2),
            arguments: vec![m::Register(0), m::Register(0)],
        },
    };
    raw.functions[0].blocks.extend([
        b::Block {
            parameters: vec![m::Register(2)],
            instructions: vec![],
            terminator: b::Terminator {
                kind: b::Term::Return(m::Register(2)),
                span: SPAN,
            },
            span: SPAN,
        },
        b::Block {
            parameters: vec![m::Register(3), m::Register(4)],
            instructions: vec![b::Instruction {
                results: vec![m::Register(5)],
                op: b::Op::Add(m::Register(3), m::Register(4)),
                span: SPAN,
            }],
            terminator: b::Terminator {
                kind: b::Term::Return(m::Register(5)),
                span: SPAN,
            },
            span: SPAN,
        },
    ]);
    let bytes = encode(raw.clone(), Some(m::FunctionId(0)), CodecLimits::default()).unwrap();
    let (decoded, entry) = decode(&bytes, CodecLimits::default()).unwrap().into_parts();
    assert_eq!(decoded, raw);
    assert_eq!(
        encode(decoded, entry, CodecLimits::default()).unwrap(),
        bytes
    );
}
