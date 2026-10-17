use lang_bytecode::unit::{CodecErrorKind, CodecLimits, Program, Table, Tag, decode, encode};
use lang_mir::{ResourceTypeId, unit as m};

fn op(op: m::Op, span: Option<m::Span>) -> m::Instruction {
    m::Instruction { op, span }
}
fn block(ops: Vec<m::Op>, kind: m::Term) -> m::Block {
    m::Block {
        parameters: vec![],
        instructions: ops.into_iter().map(|o| op(o, None)).collect(),
        terminator: m::Terminator { kind, span: None },
    }
}
fn function(
    name: &str,
    parameters: Vec<m::Type>,
    result: m::Type,
    registers: Vec<m::Type>,
    blocks: Vec<m::Block>,
) -> m::Function {
    m::Function {
        name: name.into(),
        signature: m::Signature { parameters, result },
        registers,
        span: None,
        blocks,
    }
}
fn unit(functions: Vec<m::Function>) -> m::Unit {
    m::Unit {
        sources: vec![],
        definitions: vec![],
        imports: vec![],
        functions,
        entry: None,
    }
}
fn scalar() -> m::Unit {
    let mut u = unit(vec![function(
        "main",
        vec![],
        m::Type::I64,
        vec![m::Type::I64],
        vec![block(
            vec![m::Op::I64Constant {
                destination: m::Register(0),
                value: 42,
            }],
            m::Term::Return(m::Register(0)),
        )],
    )]);
    u.entry = Some(m::FunctionId(0));
    u
}
fn encoded(unit: m::Unit) -> Vec<u8> {
    encode(
        &Program::lower(unit, m::Limits::default()).unwrap(),
        CodecLimits::default(),
    )
    .unwrap()
}
fn payload(bytes: &mut [u8]) {
    let length = (bytes.len() - 16) as u32;
    bytes[12..16].copy_from_slice(&length.to_le_bytes());
}
fn golden() -> Vec<u8> {
    let hex: String = include_str!("fixtures/unit-v1-main-42.hex")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let (pairs, remainder) = hex.as_bytes().as_chunks::<2>();
    assert!(remainder.is_empty());
    pairs
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn schema_1_has_fixed_golden_bytes_and_exact_reencoding() {
    let bytes = encoded(scalar());
    assert_eq!(bytes.len(), 88);
    assert_eq!(bytes, golden());
    let program = decode(&bytes, CodecLimits::default()).unwrap();
    assert_eq!(program.verified().unit(), &scalar());
    assert_eq!(encode(&program, CodecLimits::default()).unwrap(), bytes);
}

fn comprehensive() -> m::Unit {
    use m::{DefinitionId as D, Register as R, Type as T};
    let main = function(
        "main",
        vec![],
        T::I64,
        vec![
            T::I64,
            T::I64,
            T::Bool,
            T::Record(D(1)),
            T::Enum(D(0)),
            T::Bool,
            T::Record(D(1)),
            T::I64,
            T::Bool,
            T::Bool,
            T::I64,
        ],
        vec![block(
            vec![
                m::Op::I64Constant {
                    destination: R(0),
                    value: 42,
                },
                m::Op::Call {
                    destination: R(1),
                    function: m::FunctionId(2),
                    arguments: vec![R(0)],
                },
                m::Op::BoolConstant {
                    destination: R(2),
                    value: true,
                },
                m::Op::Record {
                    destination: R(3),
                    definition: D(1),
                    fields: vec![R(1), R(2)],
                },
                m::Op::Enum {
                    destination: R(4),
                    definition: D(0),
                    case: 1,
                    payload: vec![R(3)],
                },
                m::Op::IsCase {
                    destination: R(5),
                    receiver: R(4),
                    case: 1,
                },
                m::Op::EnumField {
                    destination: R(6),
                    receiver: R(4),
                    case: 1,
                    field: 0,
                },
                m::Op::Field {
                    destination: R(7),
                    receiver: R(6),
                    field: 0,
                },
                m::Op::EqualI64 {
                    destination: R(8),
                    left: R(0),
                    right: R(7),
                },
                m::Op::LessThan {
                    destination: R(9),
                    left: R(7),
                    right: R(0),
                },
                m::Op::HostCall {
                    destination: R(10),
                    import: m::ImportId(0),
                    argument: R(7),
                },
            ],
            m::Term::Return(R(10)),
        )],
    );
    let zeta = function(
        "zeta",
        vec![T::I64],
        T::I64,
        vec![T::I64; 4],
        vec![block(
            vec![
                m::Op::I64Constant {
                    destination: R(1),
                    value: i64::MIN,
                },
                m::Op::HostCall {
                    destination: R(2),
                    import: m::ImportId(1),
                    argument: R(0),
                },
                m::Op::Add {
                    destination: R(3),
                    left: R(1),
                    right: R(2),
                },
            ],
            m::Term::Return(R(3)),
        )],
    );
    let alpha = function(
        "alpha",
        vec![T::I64],
        T::I64,
        vec![T::I64],
        vec![block(vec![], m::Term::Return(R(0)))],
    );
    let mut unit = unit(vec![main, zeta, alpha]);
    unit.entry = Some(m::FunctionId(0));
    unit.definitions = vec![
        m::Definition {
            name: "Z".into(),
            kind: m::DefinitionKind::Enum(vec![
                m::Case {
                    name: "None".into(),
                    payload: vec![],
                },
                m::Case {
                    name: "Some".into(),
                    payload: vec![T::Record(D(1))],
                },
            ]),
        },
        m::Definition {
            name: "A".into(),
            kind: m::DefinitionKind::Record(vec![
                m::Field {
                    name: "left".into(),
                    value_type: T::I64,
                },
                m::Field {
                    name: "right".into(),
                    value_type: T::Bool,
                },
            ]),
        },
    ];
    unit.imports = ["z_host", "a_host"]
        .into_iter()
        .map(|name| m::Import {
            name: name.into(),
            signature: m::Signature {
                parameters: vec![T::I64],
                result: T::I64,
            },
        })
        .collect();
    unit.sources = vec![
        m::Source {
            path: "z.lang".into(),
            text: "z\n".into(),
        },
        m::Source {
            path: "a.lang".into(),
            text: "a😀\n".into(),
        },
    ];
    unit.functions[0].span = Some(m::Span {
        source: m::SourceId(1),
        start: 1,
        end: 5,
    });
    for instruction in &mut unit.functions[0].blocks[0].instructions {
        instruction.span = Some(m::Span {
            source: m::SourceId(0),
            start: 0,
            end: 1,
        });
    }
    unit
}

#[test]
fn canonical_sorting_remaps_all_table_ids_spans_calls_and_entry() {
    let bytes = encoded(comprehensive());
    let decoded = decode(&bytes, CodecLimits::default()).unwrap();
    let unit = decoded.verified().unit();
    assert_eq!(
        unit.sources
            .iter()
            .map(|s| s.path.as_str())
            .collect::<Vec<_>>(),
        ["a.lang", "z.lang"]
    );
    assert_eq!(
        unit.definitions
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["A", "Z"]
    );
    assert_eq!(
        unit.imports
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>(),
        ["a_host", "z_host"]
    );
    assert_eq!(
        unit.functions
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "main", "zeta"]
    );
    assert_eq!(unit.entry, Some(m::FunctionId(1)));
    let main = &unit.functions[1];
    assert_eq!(main.registers[3], m::Type::Record(m::DefinitionId(0)));
    assert_eq!(main.registers[4], m::Type::Enum(m::DefinitionId(1)));
    assert_eq!(main.span.unwrap().source, m::SourceId(0));
    assert_eq!(
        main.blocks[0].instructions[0].span.unwrap().source,
        m::SourceId(1)
    );
    assert!(matches!(
        main.blocks[0].instructions[1].op,
        m::Op::Call {
            function: m::FunctionId(0),
            ..
        }
    ));
    assert!(matches!(
        main.blocks[0].instructions[3].op,
        m::Op::Record {
            definition: m::DefinitionId(0),
            ..
        }
    ));
    assert!(matches!(
        main.blocks[0].instructions[4].op,
        m::Op::Enum {
            definition: m::DefinitionId(1),
            case: 1,
            ..
        }
    ));
    assert!(matches!(
        main.blocks[0].instructions[10].op,
        m::Op::HostCall {
            import: m::ImportId(1),
            ..
        }
    ));
    let m::DefinitionKind::Enum(cases) = &unit.definitions[1].kind else {
        unreachable!()
    };
    assert_eq!(cases[1].payload, [m::Type::Record(m::DefinitionId(0))]);
    assert_eq!(
        decoded.verified().facts().sources()[0].position(&unit.sources[0], 5),
        Some((1, 3))
    );
    assert_eq!(encode(&decoded, CodecLimits::default()).unwrap(), bytes);
}

#[test]
fn every_opcode_signed_extreme_bool_and_optional_entry_survive_roundtrips() {
    let bytes = encoded(comprehensive());
    let decoded = decode(&bytes, CodecLimits::default()).unwrap();
    assert!(matches!(
        decoded.verified().unit().functions[2].blocks[0].instructions[0].op,
        m::Op::I64Constant {
            value: i64::MIN,
            ..
        }
    ));
    let empty = encoded(unit(vec![]));
    let decoded = decode(&empty, CodecLimits::default()).unwrap();
    assert!(decoded.verified().unit().entry.is_none());
    assert_eq!(encode(&decoded, CodecLimits::default()).unwrap(), empty);
    let own = m::Type::Own(ResourceTypeId::new(99));
    let source = unit(vec![function(
        "identity",
        vec![own],
        own,
        vec![own],
        vec![block(vec![], m::Term::Return(m::Register(0)))],
    )]);
    assert_eq!(
        decode(&encoded(source.clone()), CodecLimits::default())
            .unwrap()
            .verified()
            .unit(),
        &source
    );
}

#[test]
fn blocks_arguments_branches_and_nonsequential_register_definitions_are_preserved() {
    use m::Register as R;
    let target = |block, args: &[u32]| m::Target {
        block: m::BlockId(block),
        arguments: args.iter().map(|r| R(*r)).collect(),
    };
    let mut header = block(
        vec![
            m::Op::I64Constant {
                destination: R(3),
                value: 1,
            },
            m::Op::LessThan {
                destination: R(5),
                left: R(2),
                right: R(0),
            },
        ],
        m::Term::Branch {
            condition: R(5),
            then_target: target(2, &[]),
            else_target: target(3, &[]),
        },
    );
    header.parameters = vec![R(2)];
    let source = unit(vec![function(
        "loop",
        vec![m::Type::I64],
        m::Type::I64,
        vec![
            m::Type::I64,
            m::Type::I64,
            m::Type::I64,
            m::Type::I64,
            m::Type::I64,
            m::Type::Bool,
        ],
        vec![
            block(
                vec![m::Op::I64Constant {
                    destination: R(1),
                    value: 0,
                }],
                m::Term::Jump(target(1, &[1])),
            ),
            header,
            block(
                vec![m::Op::Add {
                    destination: R(4),
                    left: R(2),
                    right: R(3),
                }],
                m::Term::Jump(target(1, &[4])),
            ),
            block(vec![], m::Term::Return(R(2))),
        ],
    )]);
    let bytes = encoded(source.clone());
    let decoded = decode(&bytes, CodecLimits::default()).unwrap();
    assert_eq!(decoded.verified().unit(), &source);
    assert_eq!(encode(&decoded, CodecLimits::default()).unwrap(), bytes);
}

#[test]
fn headers_versions_flags_lengths_and_trailing_bytes_are_rejected() {
    let bytes = golden();
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end], CodecLimits::default()).is_err());
        if end >= 16 {
            let mut shortened = bytes[..end].to_vec();
            payload(&mut shortened);
            assert!(decode(&shortened, CodecLimits::default()).is_err());
        }
    }
    let mut bad = bytes.clone();
    bad[0] = 0;
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::BadMagic
    );
    let mut bad = bytes.clone();
    bad[8] = 2;
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::Version(2)
    );
    let mut bad = bytes.clone();
    bad[10] = 1;
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::Flags(1)
    );
    let mut bad = bytes.clone();
    bad.push(0);
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::LengthMismatch
    );
    payload(&mut bad);
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::TrailingBytes
    );
}

#[test]
fn malformed_tags_utf8_bool_and_semantic_registers_never_create_programs() {
    let bytes = golden();
    let mut bad = bytes.clone();
    bad[36] = 255;
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::InvalidUtf8
    );
    let mut bad = bytes.clone();
    bad[44] = 255;
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::InvalidTag {
            tag: Tag::Type,
            value: 255
        }
    );
    let mut bad = bytes.clone();
    bad[63] = 255;
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::InvalidTag {
            tag: Tag::Instruction,
            value: 255
        }
    );
    let mut bad = bytes.clone();
    bad[77] = 255;
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::InvalidTag {
            tag: Tag::Terminator,
            value: 255
        }
    );
    let mut bad = bytes.clone();
    bad[83] = 2;
    assert_eq!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::InvalidTag {
            tag: Tag::Entry,
            value: 2
        }
    );
    let mut bad = bytes.clone();
    bad[64..68].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(
        decode(&bad, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::Validation(m::Error {
            kind: m::ErrorKind::InvalidRegister(99),
            ..
        })
    ));
    let mut source = scalar();
    source.entry = None;
    source.functions[0].signature.result = m::Type::Bool;
    source.functions[0].registers[0] = m::Type::Bool;
    source.functions[0].blocks[0].instructions[0].op = m::Op::BoolConstant {
        destination: m::Register(0),
        value: false,
    };
    let mut bytes = encoded(source);
    bytes[68] = 2;
    assert_eq!(
        decode(&bytes, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::InvalidBoolean(2)
    );
}

#[test]
fn noncanonical_table_order_is_rejected_instead_of_silently_remapping_indices() {
    let mut source = unit(vec![]);
    source.sources = vec![
        m::Source {
            path: "a.lang".into(),
            text: String::new(),
        },
        m::Source {
            path: "z.lang".into(),
            text: String::new(),
        },
    ];
    let mut bytes = encoded(source);
    let first = 24;
    let second = 38;
    assert_eq!(bytes[first], b'a');
    assert_eq!(bytes[second], b'z');
    bytes[first] = b'z';
    bytes[second] = b'a';
    assert_eq!(
        decode(&bytes, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::NonCanonicalOrder(Table::Sources)
    );
}

#[test]
fn byte_count_element_allocation_and_work_limits_cover_both_codec_directions() {
    let bytes = golden();
    for validation in [
        m::Limits {
            sources: 0,
            functions: 0,
            ..m::Limits::default()
        },
        m::Limits {
            work: 0,
            ..m::Limits::default()
        },
        m::Limits {
            allocations: 0,
            ..m::Limits::default()
        },
        m::Limits {
            elements: 0,
            ..m::Limits::default()
        },
    ] {
        assert!(
            decode(
                &bytes,
                CodecLimits {
                    validation,
                    ..CodecLimits::default()
                }
            )
            .is_err()
        );
    }
    assert!(matches!(
        decode(
            &bytes,
            CodecLimits {
                bytes: bytes.len() - 1,
                ..CodecLimits::default()
            }
        )
        .unwrap_err()
        .kind,
        CodecErrorKind::TooLarge { .. }
    ));
    let program = Program::lower(scalar(), m::Limits::default()).unwrap();
    assert!(matches!(
        encode(
            &program,
            CodecLimits {
                bytes: bytes.len() - 1,
                ..CodecLimits::default()
            }
        )
        .unwrap_err()
        .kind,
        CodecErrorKind::TooLarge { .. }
    ));
    assert_eq!(
        encode(
            &program,
            CodecLimits {
                bytes: bytes.len(),
                ..CodecLimits::default()
            }
        )
        .unwrap(),
        bytes
    );
    let mut bad = bytes;
    bad[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(&bad, CodecLimits::default()).is_err());
}

#[test]
fn decoder_and_verifier_share_a_cumulative_allocation_budget() {
    let program = Program::lower(scalar(), m::Limits::default()).unwrap();
    let mut verifier = m::Budget::new(m::Limits::default());
    program.verified().revalidate(&mut verifier).unwrap();
    let bytes = encode(&program, CodecLimits::default()).unwrap();
    let error = decode(
        &bytes,
        CodecLimits {
            validation: m::Limits {
                allocations: verifier.allocation_bytes(),
                ..m::Limits::default()
            },
            ..CodecLimits::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        error.kind,
        CodecErrorKind::Budget(m::Error {
            kind: m::ErrorKind::Limit {
                kind: m::LimitKind::Allocations,
                ..
            },
            ..
        }) | CodecErrorKind::Validation(m::Error {
            kind: m::ErrorKind::Limit {
                kind: m::LimitKind::Allocations,
                ..
            },
            ..
        })
    ));
}

#[test]
fn mutated_and_arbitrary_byte_streams_fail_or_reencode_canonically() {
    let original = encoded(comprehensive());
    for index in 0..original.len() {
        let mut bytes = original.clone();
        bytes[index] ^= 0xa5;
        if let Ok(program) = decode(&bytes, CodecLimits::default()) {
            assert_eq!(encode(&program, CodecLimits::default()).unwrap(), bytes);
        }
    }
    let mut state = 0x8719_34ba_u32;
    for length in 0..256 {
        let mut bytes = Vec::new();
        for _ in 0..length {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            bytes.push((state >> 16) as u8);
        }
        let _ = decode(&bytes, CodecLimits::default());
    }
}

#[test]
fn cumulative_source_text_and_nested_counts_are_checked_before_allocation() {
    let mut source = unit(vec![]);
    source.sources = vec![
        m::Source {
            path: "a.lang".into(),
            text: "abc".into(),
        },
        m::Source {
            path: "b.lang".into(),
            text: "def".into(),
        },
    ];
    let bytes = encoded(source);
    assert!(
        decode(
            &bytes,
            CodecLimits {
                validation: m::Limits {
                    source_bytes: 3,
                    total_source_bytes: 6,
                    ..m::Limits::default()
                },
                ..CodecLimits::default()
            }
        )
        .is_ok()
    );
    let error = decode(
        &bytes,
        CodecLimits {
            validation: m::Limits {
                source_bytes: 3,
                total_source_bytes: 5,
                ..m::Limits::default()
            },
            ..CodecLimits::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        error.kind,
        CodecErrorKind::Budget(m::Error {
            kind: m::ErrorKind::Limit {
                kind: m::LimitKind::TotalSourceBytes,
                ..
            },
            ..
        })
    ));
    let mut bytes = golden();
    bytes[45..49].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode(&bytes, CodecLimits::default()).unwrap_err().kind,
        CodecErrorKind::Budget(m::Error {
            kind: m::ErrorKind::Limit {
                kind: m::LimitKind::Registers,
                ..
            },
            ..
        })
    ));
}
