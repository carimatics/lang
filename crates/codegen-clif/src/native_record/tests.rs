use super::*;

const SEMANTIC: &[u8] = b"mir";
fn fixture(kind: Kind, target: &str) -> Record {
    Record::new(
        target.parse().unwrap(),
        kind,
        b"obj".to_vec(),
        vec![FunctionCode {
            function: m::FunctionId(0),
            symbol: "f".into(),
            code_size: 3,
            source_ranges: vec![SourceRange {
                start: 0,
                end: 3,
                span: m::Span {
                    source: m::SourceId(0),
                    start: 0,
                    end: 1,
                },
            }],
        }],
    )
}
fn hex(text: &str) -> Vec<u8> {
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn native_record_golden_freezes_framing_digests_and_metadata() {
    let record = fixture(Kind::Copy, "aarch64-apple-darwin");
    let bytes = record.encode(SEMANTIC, Limits::default()).unwrap();
    // SHA-256 values were independently computed with Python hashlib.
    let expected = hex("
        4c414e474e415400 0100 0000 72000000 00 00
        02d087933efc5d76e06f75adf1db3c14dbe271b1769d6b267eaeae29ec57330d
        772a5fb04f9bad38681a2f56ddfdbd6a15185753df8dcc029788d02bf3b6825b
        03000000 6f626a 01000000
        00000000 01000000 66 03000000 01000000
        00000000 03000000 00000000 00000000 01000000
    ");
    assert_eq!(bytes, expected);
    let loaded = Record::decode(&expected, SEMANTIC, Limits::default()).unwrap();
    assert_eq!(loaded.object(), b"obj");
    assert_eq!(loaded.functions(), record.functions());
    assert_eq!(loaded.into_copy().unwrap().functions, record.functions);
}
#[test]
fn task_component_revision_two_has_independent_profile_and_entry_vectors() {
    let task = hex("
        4c414e474e415400 0200 0000 76000000 00 00 04030201
        02d087933efc5d76e06f75adf1db3c14dbe271b1769d6b267eaeae29ec57330d
        772a5fb04f9bad38681a2f56ddfdbd6a15185753df8dcc029788d02bf3b6825b
        03000000 6f626a 01000000
        00000000 01000000 66 03000000 01000000
        00000000 03000000 00000000 00000000 01000000
    ");
    for (target, kind, expected) in [
        (
            "aarch64-apple-darwin",
            Kind::Task(m::FunctionId(0x0102_0304)),
            task.clone(),
        ),
        (
            "x86_64-pc-windows-msvc",
            Kind::Component(m::FunctionId(0x1122_3344)),
            {
                let mut component = task;
                component[16] = 1;
                component[17] = 1;
                component[18..22].copy_from_slice(&[0x44, 0x33, 0x22, 0x11]);
                component
            },
        ),
    ] {
        let record = fixture(kind, target);
        assert_eq!(
            record.encode(SEMANTIC, Limits::default()).unwrap(),
            expected
        );
        let decoded = Record::decode(&expected, SEMANTIC, Limits::default()).unwrap();
        assert_eq!(decoded.kind(), kind);
        assert_eq!(decoded.functions(), record.functions());
        assert_eq!(decoded.into_copy().unwrap_err().kind, ErrorKind::Profile);
        let decoded = Record::decode(&expected, SEMANTIC, Limits::default()).unwrap();
        assert_eq!(
            crate::owned::RecordedObject::from_record(decoded)
                .err()
                .unwrap()
                .kind,
            ErrorKind::Profile
        );
        let mut invalid = expected.clone();
        invalid[17] = 2;
        assert_eq!(
            Record::decode(&invalid, SEMANTIC, Limits::default()).unwrap_err(),
            error(17, ErrorKind::Kind(2))
        );
        // A revision-2 profile never reuses revision-1's payload interpretation.
        invalid = expected.clone();
        invalid[8] = 1;
        assert!(Record::decode(&invalid, SEMANTIC, Limits::default()).is_err());
    }
}

#[test]
fn both_targets_and_all_native_scopes_round_trip_without_erasing_c_identity() {
    for (target, tag) in [("aarch64-apple-darwin", 0), ("x86_64-pc-windows-msvc", 1)] {
        for (kind, tag_kind) in [
            (Kind::Copy, 0),
            (Kind::OwnershipFunction, 1),
            (Kind::OwnershipUnit, 2),
            (Kind::CUnit, 3),
            (Kind::CObject(Digest([7; 32])), 4),
            (Kind::Task(m::FunctionId(0)), 0),
            (Kind::Component(m::FunctionId(0)), 1),
        ] {
            let record = fixture(kind, target);
            let bytes = record.encode(SEMANTIC, Limits::default()).unwrap();
            assert_eq!(bytes[16], tag);
            assert_eq!(bytes[17], tag_kind);
            if let Kind::CObject(d) = kind {
                assert_eq!(bytes[18..50], d.0);
            }
            let decoded = Record::decode(&bytes, SEMANTIC, Limits::default()).unwrap();
            assert_eq!(decoded.kind(), kind);
            assert_eq!(decoded.target(), record.target());
            assert_eq!(decoded.functions(), record.functions());
            assert_eq!(decoded.encode(SEMANTIC, Limits::default()).unwrap(), bytes);
            if kind != Kind::Copy {
                assert_eq!(decoded.into_copy().unwrap_err().kind, ErrorKind::Profile);
                let owned = crate::owned::RecordedObject::from_record(
                    Record::decode(&bytes, SEMANTIC, Limits::default()).unwrap(),
                );
                if matches!(kind, Kind::Task(_) | Kind::Component(_)) {
                    assert!(matches!(
                        owned,
                        Err(Error {
                            kind: ErrorKind::Profile,
                            ..
                        })
                    ));
                } else {
                    assert_eq!(owned.unwrap().into_record().kind(), kind);
                }
            } else {
                assert!(matches!(
                    crate::owned::RecordedObject::from_record(decoded),
                    Err(Error {
                        kind: ErrorKind::Profile,
                        ..
                    })
                ));
            }
            for end in 0..bytes.len() {
                assert!(Record::decode(&bytes[..end], SEMANTIC, Limits::default()).is_err());
                if end >= HEADER {
                    let mut prefix = bytes[..end].to_vec();
                    prefix[12..16].copy_from_slice(&((end - HEADER) as u32).to_le_bytes());
                    assert!(Record::decode(&prefix, SEMANTIC, Limits::default()).is_err());
                }
            }
        }
    }
}
#[test]
fn malformed_native_records_fail_at_the_field_without_loading_code() {
    let bytes = fixture(Kind::Copy, "aarch64-apple-darwin")
        .encode(SEMANTIC, Limits::default())
        .unwrap();
    for (offset, data, expected_offset, kind) in [
        (0, vec![0], 0, ErrorKind::Magic),
        (8, vec![3, 0], 8, ErrorKind::Version(3)),
        (10, vec![1, 0], 10, ErrorKind::Flags(1)),
        (12, vec![0, 0, 0, 0], 12, ErrorKind::Length),
        (16, vec![2], 16, ErrorKind::Target),
        (17, vec![255], 17, ErrorKind::Kind(255)),
        (18, vec![255], 18, ErrorKind::SemanticDigest),
        (50, vec![255], 50, ErrorKind::ObjectDigest),
        (86, vec![255], 50, ErrorKind::ObjectDigest),
        (89, u32::MAX.to_le_bytes().to_vec(), 89, ErrorKind::Count),
        (97, vec![0, 0, 0, 0], 97, ErrorKind::Symbol),
        (101, vec![255], 97, ErrorKind::Symbol),
        (101, vec![0], 97, ErrorKind::Symbol),
        (102, vec![0, 0, 0, 0], 102, ErrorKind::CodeRange),
        (106, u32::MAX.to_le_bytes().to_vec(), 106, ErrorKind::Count),
        (114, vec![4, 0, 0, 0], 110, ErrorKind::CodeRange),
        (122, vec![2, 0, 0, 0], 122, ErrorKind::SourceRange),
    ] {
        let mut invalid = bytes.clone();
        invalid[offset..offset + data.len()].copy_from_slice(&data);
        let err = Record::decode(&invalid, SEMANTIC, Limits::default()).unwrap_err();
        assert_eq!(err.offset, expected_offset);
        assert_eq!(err.kind, kind);
    }
    assert_eq!(
        Record::decode(&bytes, b"other", Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::SemanticDigest
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    let length = (trailing.len() - HEADER) as u32;
    trailing[12..16].copy_from_slice(&length.to_le_bytes());
    assert_eq!(
        Record::decode(&trailing, SEMANTIC, Limits::default()).unwrap_err(),
        error(bytes.len(), ErrorKind::Length)
    );
}
fn cap(limits: &mut Limits, field: usize, value: usize) {
    match field {
        0 => limits.validation.work = value,
        1 => limits.validation.allocations = value,
        2 => limits.validation.elements = value,
        _ => unreachable!(),
    }
}
#[test]
fn exact_record_limits_cover_hashes_both_writer_passes_and_retained_buffers() {
    for kind in [
        Kind::Copy,
        Kind::Task(m::FunctionId(0)),
        Kind::Component(m::FunctionId(0)),
    ] {
        exact_record_limits(kind);
    }
}
fn exact_record_limits(kind: Kind) {
    let record = fixture(kind, "aarch64-apple-darwin");
    let bytes = record.encode(SEMANTIC, Limits::default()).unwrap();
    for maximum in [bytes.len(), bytes.len() - 1] {
        let limits = Limits {
            bytes: maximum,
            ..Default::default()
        };
        assert_eq!(
            record.encode(SEMANTIC, limits).is_ok(),
            maximum == bytes.len()
        );
        assert_eq!(
            Record::decode(&bytes, SEMANTIC, limits).is_ok(),
            maximum == bytes.len()
        );
    }
    let decoded = Record::decode(&bytes, SEMANTIC, Limits::default()).unwrap();
    for (field, exact) in [
        decoded.usage.work,
        decoded.usage.allocation_bytes,
        decoded.usage.elements,
    ]
    .into_iter()
    .enumerate()
    {
        for (value, success) in [(exact, true), (exact - 1, false), (0, false)] {
            let mut limits = Limits::default();
            cap(&mut limits, field, value);
            let result = Record::decode(&bytes, SEMANTIC, limits);
            assert_eq!(result.is_ok(), success, "decode field {field} cap {value}");
            if !success {
                assert!(matches!(result.unwrap_err().kind, ErrorKind::Resource(_)));
            }
        }
        let mut low = 0;
        let mut high = 128 * 1024 * 1024;
        while low < high {
            let mid = low + (high - low) / 2;
            let mut limits = Limits::default();
            cap(&mut limits, field, mid);
            if record.encode(SEMANTIC, limits).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        let mut limits = Limits::default();
        cap(&mut limits, field, low);
        assert!(record.encode(SEMANTIC, limits).is_ok());
        cap(&mut limits, field, low - 1);
        assert!(matches!(
            record.encode(SEMANTIC, limits).unwrap_err().kind,
            ErrorKind::Resource(_)
        ));
    }
    let mut limits = Limits::default();
    limits.validation.functions = 0;
    assert_eq!(
        Record::decode(&bytes, SEMANTIC, limits).unwrap_err().kind,
        ErrorKind::Count
    );
    assert_eq!(
        record.encode(SEMANTIC, limits).unwrap_err().kind,
        ErrorKind::Count
    );
}
#[test]
fn encoder_rejects_invalid_native_metadata_and_unsupported_targets() {
    let mut record = fixture(Kind::Copy, "aarch64-apple-darwin");
    record.functions.push(record.functions[0].clone());
    assert_eq!(
        record.encode(SEMANTIC, Limits::default()).unwrap_err().kind,
        ErrorKind::FunctionOrder
    );
    record.functions.pop();
    record.functions[0].symbol = "x".repeat(SYMBOL_BYTES + 1);
    assert_eq!(
        record.encode(SEMANTIC, Limits::default()).unwrap_err().kind,
        ErrorKind::Symbol
    );
    record.functions[0].symbol = "f".into();
    record.functions[0].source_ranges[0].end = 4;
    assert_eq!(
        record.encode(SEMANTIC, Limits::default()).unwrap_err().kind,
        ErrorKind::CodeRange
    );
    let other = fixture(Kind::Copy, "aarch64-unknown-linux-gnu");
    assert_eq!(
        other.encode(SEMANTIC, Limits::default()).unwrap_err().kind,
        ErrorKind::Target
    );
}
