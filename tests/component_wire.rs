use lang_mir::unit::SourceId;
use lang_runtime::component::idl;
use lang_runtime::component::wire::*;

fn admission() -> Report {
    Report {
        policy: MessagePolicy::Redacted,
        primary: Record {
            category: Category::Admission,
            phase: Phase::Admission,
            diagnostic: Diagnostic::InvalidWire,
            message: None,
            limit: None,
            origin: None,
        },
        suppressed: vec![],
        poisoned: false,
        omitted: 0,
        usage: ReportUsage::default(),
        frames: vec![],
    }
}
fn session() -> Session {
    Session::new(Limits::default())
}
fn cleanup_packet() -> Report {
    let mut report = admission();
    report.policy = MessagePolicy::Sanitized;
    report.primary = Record {
        category: Category::Destructor,
        phase: Phase::Cleanup,
        diagnostic: Diagnostic::Destructor,
        message: Some("private cleanup λ".into()),
        limit: None,
        origin: Some(Frame {
            mode: Mode::Vm,
            digest: [17; 32],
            member: 99,
            span: Some(lang_mir::unit::Span {
                source: SourceId(0),
                start: 3,
                end: 11,
            }),
        }),
    };
    report.suppressed.push(Record {
        category: Category::HostContract,
        diagnostic: Diagnostic::HostContract,
        ..report.primary.clone()
    });
    report.frames.push(report.primary.origin.clone().unwrap());
    report.poisoned = true;
    report.omitted = 7;
    report.usage.host_calls = 9999;
    report
}

#[test]
fn cleanup_report_decoder_validates_without_any_decoder_allocation() {
    let bytes = encode_report(&cleanup_packet(), &mut session()).unwrap();
    let mut bounded = Session::new(Limits {
        allocation_bytes: 0,
        ..Default::default()
    });
    let report = decode_cleanup_report(&bytes, &mut bounded).unwrap();
    assert_eq!(
        report.records(),
        &[Diagnostic::Destructor, Diagnostic::HostContract]
    );
    assert_eq!(report.omitted(), 8);
    assert_eq!(bounded.usage().allocation_bytes, 0);
    assert!(
        decode_report(
            &bytes,
            &mut Session::new(Limits {
                allocation_bytes: 0,
                ..Default::default()
            })
        )
        .is_err()
    );
    assert!(
        decode_cleanup_report(
            &encode_report(&admission(), &mut session()).unwrap(),
            &mut session()
        )
        .is_err()
    );
}

#[test]
fn cleanup_report_decoder_keeps_order_and_validates_even_the_omitted_suffix() {
    let mut report = cleanup_packet();
    report.suppressed = (0..70)
        .map(|index| Record {
            category: if index % 2 == 0 {
                Category::HostContract
            } else {
                Category::Destructor
            },
            diagnostic: if index % 2 == 0 {
                Diagnostic::HostContract
            } else {
                Diagnostic::Destructor
            },
            ..report.primary.clone()
        })
        .collect();
    report.omitted = 5;
    let mut bytes = encode_report(&report, &mut session()).unwrap();
    let decoded = decode_cleanup_report(&bytes, &mut session()).unwrap();
    assert_eq!(decoded.records().len(), 64);
    assert_eq!(decoded.omitted(), 13);
    for (index, diagnostic) in decoded.records().iter().enumerate() {
        assert_eq!(
            *diagnostic,
            if index % 2 == 0 {
                Diagnostic::Destructor
            } else {
                Diagnostic::HostContract
            }
        );
    }
    let name = Diagnostic::Destructor.as_str().as_bytes();
    let at = bytes
        .windows(name.len())
        .rposition(|part| part == name)
        .unwrap();
    bytes[at - 5] = Phase::Call as u8;
    assert!(decode_cleanup_report(&bytes, &mut session()).is_err());
    report.omitted = u64::MAX;
    let bytes = encode_report(&report, &mut session()).unwrap();
    assert_eq!(
        decode_cleanup_report(&bytes, &mut session())
            .unwrap()
            .omitted(),
        u64::MAX
    );
}

#[test]
fn cleanup_report_decoder_refuses_truncation_trailing_and_invalid_schema_claims() {
    let bytes = encode_report(&cleanup_packet(), &mut session()).unwrap();
    for length in 0..bytes.len() {
        assert!(
            decode_cleanup_report(&bytes[..length], &mut session()).is_err(),
            "prefix {length}"
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_cleanup_report(&trailing, &mut session()).is_err());
    for at in 0..bytes.len() {
        for value in [0, 1, 255] {
            let mut changed = bytes.clone();
            changed[at] = value;
            if let Ok(decoded) = decode_cleanup_report(&changed, &mut session()) {
                let complete = decode_report(&changed, &mut session()).unwrap();
                let records: Vec<_> = std::iter::once(&complete.primary)
                    .chain(&complete.suppressed)
                    .collect();
                assert!(records.iter().all(|r| r.phase == Phase::Cleanup
                    && matches!(r.category, Category::Destructor | Category::HostContract)));
                assert_eq!(
                    decoded.records(),
                    records.iter().map(|r| r.diagnostic).collect::<Vec<_>>()
                );
            }
        }
    }
}
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .flat_map(|w| {
            (0..w.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&w[i..i + 2], 16).unwrap())
                .collect::<Vec<_>>()
        })
        .collect()
}
#[test]
fn independent_schema3_report_vector_and_every_truncated_prefix() {
    let expected = hex(include_str!("fixtures/component_report_v3.hex"));
    let report = admission();
    assert_eq!(encode_report(&report, &mut session()).unwrap(), expected);
    assert_eq!(decode_report(&expected, &mut session()).unwrap(), report);
    for n in 0..expected.len() {
        assert!(
            decode_report(&expected[..n], &mut session()).is_err(),
            "prefix {n}"
        );
    }
    let mut trailing = expected.clone();
    trailing.push(0);
    assert_eq!(
        decode_report(&trailing, &mut session()).unwrap_err().kind,
        ErrorKind::Trailing
    );
}
#[test]
fn reports_validate_poison_cleanup_limit_policy_and_spans() {
    let mut r = admission();
    r.primary.diagnostic = Diagnostic::InstancePoisoned;
    assert_eq!(
        encode_report(&r, &mut session()).unwrap_err().kind,
        ErrorKind::InvalidReport
    );
    r.poisoned = true;
    encode_report(&r, &mut session()).unwrap();
    r.suppressed.push(r.primary.clone());
    assert!(encode_report(&r, &mut session()).is_err());
    r.suppressed[0] = Record {
        category: Category::Destructor,
        phase: Phase::Cleanup,
        diagnostic: Diagnostic::Destructor,
        message: None,
        limit: None,
        origin: None,
    };
    let bytes = encode_report(&r, &mut session()).unwrap();
    assert_eq!(decode_report(&bytes, &mut session()).unwrap(), r);
    r.primary = Record {
        category: Category::Limit,
        phase: Phase::Call,
        diagnostic: Diagnostic::Limit,
        message: Some("bounded".into()),
        limit: Some(LimitDetail {
            kind: ReportLimitKind::Fuel,
            limit: 3,
            attempted: 4,
        }),
        origin: None,
    };
    assert!(encode_report(&r, &mut session()).is_err());
    r.policy = MessagePolicy::Sanitized;
    encode_report(&r, &mut session()).unwrap();
    r.primary.limit = None;
    assert!(encode_report(&r, &mut session()).is_err());
}
#[test]
fn independent_rich_contract_vector_round_trip_and_unused_reference_validation() {
    let checked = idl::check(
        SourceId(7),
        include_bytes!("fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap();
    let contract = Contract::new(checked, vec![], &mut session()).unwrap();
    let expected = hex(include_str!("fixtures/component_contract_v3.hex"));
    assert_eq!(
        encode_contract(&contract, &mut session()).unwrap(),
        expected
    );
    let decoded = decode_contract(SourceId(91), &expected, &mut session()).unwrap();
    assert_eq!(encode_contract(&decoded, &mut session()).unwrap(), expected);
    assert!(
        decoded
            .document()
            .type_nodes
            .iter()
            .all(|n| n.span.source == SourceId(91) && n.span.end as usize <= expected.len())
    );
    for n in 0..expected.len() {
        assert!(
            decode_contract(SourceId(91), &expected[..n], &mut session()).is_err(),
            "prefix {n}"
        );
    }
    let mut unknown = expected.clone();
    let pos = unknown.windows(7).position(|s| s == b"request").unwrap();
    unknown[pos..pos + 7].copy_from_slice(b"missing");
    assert!(matches!(
        decode_contract(SourceId(91), &unknown, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::Declaration(_)
    ));
    let mut bad_tag = expected.clone();
    bad_tag[pos + 7] = 255;
    assert_eq!(
        decode_contract(SourceId(91), &bad_tag, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::Tag
    );
}
fn data_contract() -> Contract {
    let checked = idl::check(
        SourceId(7),
        include_bytes!("fixtures/component_values.idl.json"),
        Default::default(),
    )
    .unwrap();
    Contract::new(checked, vec![], &mut session()).unwrap()
}
fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/component_values_v1_vectors.json")).unwrap()
}
fn data_values() -> Values {
    use Value::*;
    Values {
        roots: vec![ValueId(22)],
        nodes: vec![
            Unit,
            I64(-42),
            Bool(true),
            Text("あ😀".into()),
            Bytes(vec![0, 255, 1]),
            Unit,
            Variant {
                case: 0,
                payload: ValueId(5),
            },
            I64(-7),
            Bool(false),
            Text("x".into()),
            Bytes(vec![2, 3]),
            Record(vec![ValueId(7), ValueId(8), ValueId(9), ValueId(10)]),
            Variant {
                case: 1,
                payload: ValueId(11),
            },
            List(vec![ValueId(6), ValueId(12)]),
            I64(9),
            Bool(true),
            Text("".into()),
            Bytes(vec![]),
            Record(vec![ValueId(14), ValueId(15), ValueId(16), ValueId(17)]),
            Some(ValueId(18)),
            Text("declared".into()),
            Err(ValueId(20)),
            Record(vec![
                ValueId(0),
                ValueId(1),
                ValueId(2),
                ValueId(3),
                ValueId(4),
                ValueId(13),
                ValueId(19),
                ValueId(21),
            ]),
            Ok(ValueId(22)),
        ],
    }
}
#[test]
fn independent_signature_and_all_rich_value_frames_match_reference_packer() {
    let c = data_contract();
    let key = &c.document().interfaces[0].key.value;
    let sig = Signature::new(&c, key, "apply", &mut session()).unwrap();
    let v = vectors();
    assert!(sig.is_data_only());
    assert_eq!(
        sig.preimage(&mut session()).unwrap(),
        hex(v["preimage"].as_str().unwrap())
    );
    assert_eq!(sig.digest().as_slice(), hex(v["digest"].as_str().unwrap()));
    let mut values = data_values();
    for (name, kind) in [
        ("parameters", FrameKind::Parameters),
        ("result_ok", FrameKind::Result),
        ("result_err", FrameKind::Result),
        ("parameters_none", FrameKind::Parameters),
    ] {
        values.roots[0] = match name {
            "result_ok" => ValueId(23),
            "result_err" => ValueId(21),
            _ => ValueId(22),
        };
        if name == "parameters_none" {
            values.nodes[19] = Value::None;
            values.nodes[13] = Value::List(vec![]);
            values.nodes[21] = Value::Ok(ValueId(18));
        }
        let expected = hex(v[name].as_str().unwrap());
        assert_eq!(
            encode_values(&sig, kind, &values, &mut session()).unwrap(),
            expected,
            "{name}"
        );
        let decoded = decode_values(&sig, kind, &expected, &mut session()).unwrap();
        assert_eq!(
            encode_values(&sig, kind, &decoded, &mut session()).unwrap(),
            expected,
            "{name}"
        );
        for n in 0..expected.len() {
            assert!(
                decode_values(&sig, kind, &expected[..n], &mut session()).is_err(),
                "{name} prefix {n}"
            );
        }
    }
}
#[test]
fn signature_omits_alias_and_parameter_spellings_and_unrelated_declarations() {
    let mut v: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_values.idl.json")).unwrap();
    v["interfaces"][0]["functions"][0]["params"][0]["name"] = serde_json::json!("renamed");
    v["interfaces"][0]["types"][2]["name"] = serde_json::json!("batched");
    v["interfaces"][0]["types"][4]["record"][5]["type"] = serde_json::json!("batched");
    v["interfaces"][0]["types"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name":"unused","record":[]}));
    let checked = idl::check(
        SourceId(8),
        &serde_json::to_vec(&v).unwrap(),
        Default::default(),
    )
    .unwrap();
    let c = Contract::new(checked, vec![], &mut session()).unwrap();
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "apply",
        &mut session(),
    )
    .unwrap();
    assert_eq!(
        sig.digest().as_slice(),
        hex(vectors()["digest"].as_str().unwrap())
    );
}
#[test]
fn data_codec_refuses_resource_schema_and_wrong_signature_before_value_allocation() {
    let checked = idl::check(
        SourceId(7),
        include_bytes!("fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap();
    let c = Contract::new(checked, vec![], &mut session()).unwrap();
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "apply",
        &mut session(),
    )
    .unwrap();
    assert!(!sig.is_data_only());
    let mut s = Session::new(Limits {
        work: 0,
        allocation_bytes: 0,
        ..Default::default()
    });
    assert_eq!(
        encode_values(&sig, FrameKind::Parameters, &Values::default(), &mut s)
            .unwrap_err()
            .kind,
        ErrorKind::ResourceContextRequired
    );
    assert_eq!(s.usage(), Usage::default());
    let c = data_contract();
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "apply",
        &mut session(),
    )
    .unwrap();
    let mut wrong = hex(vectors()["parameters"].as_str().unwrap());
    wrong[9] ^= 1;
    let mut s = Session::new(Limits {
        allocation_bytes: 0,
        ..Default::default()
    });
    assert_eq!(
        decode_values(&sig, FrameKind::Parameters, &wrong, &mut s)
            .unwrap_err()
            .kind,
        ErrorKind::Signature
    );
    assert_eq!(s.usage().allocation_bytes, 0);
}
#[test]
fn malformed_values_reject_noncanonical_tags_lengths_utf8_and_indices() {
    let c = data_contract();
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "apply",
        &mut session(),
    )
    .unwrap();
    let base = hex(vectors()["parameters"].as_str().unwrap());
    for (at, byte, kind) in [
        (0, b'X', ErrorKind::Magic),
        (4, 2, ErrorKind::Version),
        (8, 2, ErrorKind::Kind),
        (53, 2, ErrorKind::Tag),
        (58, 255, ErrorKind::Utf8),
        (76, 2, ErrorKind::Tag),
    ] {
        let mut b = base.clone();
        b[at] = byte;
        assert_eq!(
            decode_values(&sig, FrameKind::Parameters, &b, &mut session())
                .unwrap_err()
                .kind,
            kind,
            "offset {at}"
        );
    }
    let mut long = base.clone();
    long[41..45].copy_from_slice(&(base.len() as u32 - 44).to_le_bytes());
    assert_eq!(
        decode_values(&sig, FrameKind::Parameters, &long, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::Truncated
    );
    let mut trailing = base.clone();
    trailing.push(0);
    assert_eq!(
        decode_values(&sig, FrameKind::Parameters, &trailing, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::Trailing
    );
    let mut huge = base;
    huge[72..76].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut bounded = Session::new(Limits {
        allocation_bytes: 16384,
        ..Default::default()
    });
    assert_eq!(
        decode_values(&sig, FrameKind::Parameters, &huge, &mut bounded)
            .unwrap_err()
            .kind,
        ErrorKind::Truncated
    );
    let mut values = data_values();
    values.roots[0] = ValueId(usize::MAX);
    assert_eq!(
        encode_values(&sig, FrameKind::Parameters, &values, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::ValueType
    );
}
fn list_unit_contract() -> Contract {
    let mut d: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_values.idl.json")).unwrap();
    d["interfaces"][0]["types"] = serde_json::json!([]);
    d["interfaces"][0]["functions"][0]["params"] =
        serde_json::json!([{"name":"items","type":{"list":"unit"}}]);
    d["interfaces"][0]["functions"][0]["result"] = serde_json::json!("unit");
    let checked = idl::check(
        SourceId(1),
        &serde_json::to_vec(&d).unwrap(),
        Default::default(),
    )
    .unwrap();
    Contract::new(checked, vec![], &mut session()).unwrap()
}
#[test]
fn zero_sized_elements_are_counted_before_work_buffer_allocation() {
    let c = list_unit_contract();
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "apply",
        &mut session(),
    )
    .unwrap();
    let mut frame = b"LCVL\x01\0\0\0\0".to_vec();
    frame.extend_from_slice(&sig.digest());
    frame.extend_from_slice(&4u32.to_le_bytes());
    frame.extend_from_slice(&u32::MAX.to_le_bytes());
    let limits = Limits {
        elements: 8,
        allocation_bytes: 4096,
        ..Default::default()
    };
    let mut s = Session::new(limits);
    assert!(matches!(
        decode_values(&sig, FrameKind::Parameters, &frame, &mut s)
            .unwrap_err()
            .kind,
        ErrorKind::Limit {
            kind: LimitKind::Elements,
            ..
        }
    ));
    assert!(s.usage().allocation_bytes < 1024);
    let values = Values {
        roots: vec![ValueId(0)],
        nodes: vec![Value::List(vec![ValueId(1); 1000]), Value::Unit],
    };
    let mut s = Session::new(limits);
    assert!(matches!(
        encode_values(&sig, FrameKind::Parameters, &values, &mut s)
            .unwrap_err()
            .kind,
        ErrorKind::Limit {
            kind: LimitKind::Elements,
            ..
        }
    ));
    assert!(s.usage().allocation_bytes < 1024);
    let cyclic = Values {
        roots: vec![ValueId(0)],
        nodes: vec![Value::List(vec![ValueId(0)])],
    };
    assert_eq!(
        encode_values(&sig, FrameKind::Parameters, &cyclic, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::ValueType
    );
}
#[test]
fn codec_session_keeps_cumulative_bytes_work_elements_and_allocations() {
    let c = data_contract();
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "apply",
        &mut session(),
    )
    .unwrap();
    let values = data_values();
    let mut first = session();
    let bytes = encode_values(&sig, FrameKind::Parameters, &values, &mut first).unwrap();
    let cost = first.usage();
    let mut s = Session::new(Limits {
        serialization_bytes: bytes.len() * 2,
        ..Default::default()
    });
    encode_values(&sig, FrameKind::Parameters, &values, &mut s).unwrap();
    encode_values(&sig, FrameKind::Parameters, &values, &mut s).unwrap();
    assert_eq!(s.usage().work, cost.work * 2);
    assert_eq!(s.usage().elements, cost.elements * 2);
    assert_eq!(s.usage().allocation_bytes, cost.allocation_bytes * 2);
    assert!(matches!(
        encode_values(&sig, FrameKind::Parameters, &values, &mut s)
            .unwrap_err()
            .kind,
        ErrorKind::Limit {
            kind: LimitKind::SerializationBytes,
            ..
        }
    ));
    assert_eq!(s.usage().serialization_bytes, bytes.len() * 2);
}
#[test]
fn every_value_codec_limit_accepts_exact_and_refuses_one_below() {
    let c = data_contract();
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "apply",
        &mut session(),
    )
    .unwrap();
    let bytes = hex(vectors()["parameters"].as_str().unwrap());
    let values = data_values();
    for decode in [false, true] {
        let run = |limits: Limits| -> Result<Usage, Error> {
            let mut s = Session::new(limits);
            if decode {
                decode_values(&sig, FrameKind::Parameters, &bytes, &mut s)?;
            } else {
                encode_values(&sig, FrameKind::Parameters, &values, &mut s)?;
            }
            Ok(s.usage())
        };
        let cost = run(Limits::default()).unwrap();
        for field in 0..7 {
            let mut exact = Limits::default();
            match field {
                0 => exact.bytes = bytes.len(),
                1 => exact.serialization_bytes = cost.serialization_bytes,
                2 => exact.elements = cost.elements,
                3 => exact.work = cost.work,
                4 => exact.allocation_bytes = cost.allocation_bytes,
                5 => {
                    exact.depth = (1..128)
                        .find(|n| {
                            run(Limits {
                                depth: *n,
                                ..Default::default()
                            })
                            .is_ok()
                        })
                        .unwrap()
                }
                _ => exact.string_bytes = 8,
            }
            run(exact).unwrap_or_else(|e| panic!("decode={decode} field={field}: {e:?}"));
            match field {
                0 => exact.bytes -= 1,
                1 => exact.serialization_bytes -= 1,
                2 => exact.elements -= 1,
                3 => exact.work -= 1,
                4 => exact.allocation_bytes -= 1,
                5 => exact.depth -= 1,
                _ => exact.string_bytes -= 1,
            }
            assert!(run(exact).is_err(), "decode={decode} field={field}");
        }
    }
}
#[test]
fn metadata_limits_accept_exact_and_refuse_one_below_in_both_directions() {
    let checked = idl::check(
        SourceId(7),
        include_bytes!("fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap();
    let c = Contract::new(checked, vec![], &mut session()).unwrap();
    let bytes = hex(include_str!("fixtures/component_contract_v3.hex"));
    let proof = c.checked().usage();
    for decode in [false, true] {
        let run = |limits: Limits| -> Result<Usage, Error> {
            let mut s = Session::new(limits);
            if decode {
                decode_contract(SourceId(2), &bytes, &mut s)?;
            } else {
                encode_contract(&c, &mut s)?;
            }
            Ok(s.usage())
        };
        let cost = run(Limits::default()).unwrap();
        for field in 0..7 {
            let mut exact = Limits::default();
            let metadata = &mut exact.metadata;
            match field {
                0 => metadata.source_bytes = bytes.len(),
                1 => metadata.declarations = proof.declarations,
                2 => metadata.elements = proof.elements,
                3 => metadata.depth = proof.data_depth,
                4 => metadata.string_bytes = 18,
                5 => metadata.work = cost.work,
                _ => metadata.allocation_bytes = cost.allocation_bytes,
            }
            run(exact).unwrap_or_else(|e| panic!("decode={decode} field={field}: {e:?}"));
            let metadata = &mut exact.metadata;
            match field {
                0 => metadata.source_bytes -= 1,
                1 => metadata.declarations -= 1,
                2 => metadata.elements -= 1,
                3 => metadata.depth -= 1,
                4 => metadata.string_bytes -= 1,
                5 => metadata.work -= 1,
                _ => metadata.allocation_bytes -= 1,
            }
            assert!(run(exact).is_err(), "decode={decode} field={field}");
        }
    }
}
#[test]
fn failed_metadata_scope_restores_outer_limits_without_refunding_work_or_bytes() {
    let bytes = hex(include_str!("fixtures/component_contract_v3.hex"));
    let mut limits = Limits::default();
    limits.metadata.work = 5;
    let mut s = Session::new(limits);
    assert!(decode_contract(SourceId(0), &bytes, &mut s).is_err());
    let before = s.usage();
    assert_eq!(before.work, 4);
    assert_eq!(before.serialization_bytes, bytes.len());
    let mut isolated = session();
    let report = encode_report(&admission(), &mut isolated).unwrap();
    let cost = isolated.usage();
    assert_eq!(encode_report(&admission(), &mut s).unwrap(), report);
    assert_eq!(s.usage().work, before.work + cost.work);
    assert_eq!(
        s.usage().serialization_bytes,
        before.serialization_bytes + cost.serialization_bytes
    );
}
#[test]
fn checked_metadata_cannot_bypass_a_stricter_transitive_depth_limit() {
    let types=(0..5).map(|i|serde_json::json!({"name":format!("n{i}"),"record":[{"name":"next","type":if i==4 {"unit".into()} else {format!("n{}",i+1)}}]})).collect::<Vec<_>>();
    let d = serde_json::json!({"idl":1,"package":"demo:depth","version":"1.0.0",
        "interfaces":[{"key":"demo:depth/api","resources":[],"types":types,"functions":[
            {"name":"apply","effect":"sync","params":[{"name":"arg","type":"n0"}],"result":"unit","capability":"demo.depth.apply"}]}],"imports":[],"exports":[]});
    let checked = idl::check(
        SourceId(1),
        &serde_json::to_vec(&d).unwrap(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(checked.usage().data_depth, 6);
    let c = Contract::new(checked, vec![], &mut session()).unwrap();
    let mut limits = Limits::default();
    limits.metadata.depth = 5;
    assert!(matches!(
        encode_contract(&c, &mut Session::new(limits))
            .unwrap_err()
            .kind,
        ErrorKind::Limit {
            kind: LimitKind::Depth,
            ..
        }
    ));
    assert!(matches!(
        Signature::new(
            &c,
            &c.document().interfaces[0].key.value,
            "apply",
            &mut Session::new(limits)
        )
        .unwrap_err()
        .kind,
        ErrorKind::Limit {
            kind: LimitKind::Depth,
            ..
        }
    ));
}
#[test]
fn deeply_nested_value_encoding_decoding_and_cleanup_are_iterative() {
    let depth = 1024;
    let mut source = String::from(
        r#"{"idl":1,"package":"demo:deep","version":"1.0.0","interfaces":[{"key":"demo:deep/api","resources":[],"types":[],"functions":[{"name":"apply","effect":"sync","params":[{"name":"arg","type":"#,
    );
    source.push_str(&"{\"list\":".repeat(depth));
    source.push_str("\"unit\"");
    source.push_str(&"}".repeat(depth));
    source.push_str(
        r#"}],"result":"unit","capability":"demo.deep.apply"}]}],"imports":[],"exports":[]}"#,
    );
    let checked = idl::check(
        SourceId(1),
        source.as_bytes(),
        idl::Limits {
            depth: 4096,
            ..Default::default()
        },
    )
    .unwrap();
    let c = Contract::new(checked, vec![], &mut session()).unwrap();
    let mut limits = Limits {
        depth: 4096,
        ..Default::default()
    };
    limits.metadata.depth = 4096;
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "apply",
        &mut Session::new(limits),
    )
    .unwrap();
    let mut values = Values {
        roots: vec![ValueId(depth)],
        nodes: vec![Value::Unit],
    };
    for n in 0..depth {
        values.nodes.push(Value::List(vec![ValueId(n)]));
    }
    assert!(matches!(
        encode_values(&sig, FrameKind::Parameters, &values, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::Limit {
            kind: LimitKind::Depth,
            ..
        }
    ));
    let bytes = encode_values(
        &sig,
        FrameKind::Parameters,
        &values,
        &mut Session::new(limits),
    )
    .unwrap();
    let decoded = decode_values(
        &sig,
        FrameKind::Parameters,
        &bytes,
        &mut Session::new(limits),
    )
    .unwrap();
    assert_eq!(decoded.nodes.len(), depth + 1);
    assert_eq!(
        encode_values(
            &sig,
            FrameKind::Parameters,
            &decoded,
            &mut Session::new(limits)
        )
        .unwrap(),
        bytes
    );
}
fn rich_report() -> Report {
    use lang_mir::unit::Span;
    let mut r = admission();
    r.policy = MessagePolicy::Sanitized;
    r.poisoned = true;
    r.omitted = u64::MAX;
    r.primary = Record {
        category: Category::Limit,
        phase: Phase::Call,
        diagnostic: Diagnostic::Limit,
        message: Some("bounded".into()),
        limit: Some(LimitDetail {
            kind: ReportLimitKind::TraceFrames,
            limit: 128,
            attempted: 129,
        }),
        origin: Some(Frame {
            mode: Mode::Aot,
            digest: std::array::from_fn(|i| i as u8),
            member: 42,
            span: Some(Span {
                source: SourceId(7),
                start: 3,
                end: 9,
            }),
        }),
    };
    r.suppressed = vec![Record {
        category: Category::Destructor,
        phase: Phase::Cleanup,
        diagnostic: Diagnostic::Destructor,
        message: None,
        limit: None,
        origin: Some(Frame {
            mode: Mode::Host,
            digest: [3; 32],
            member: 13,
            span: None,
        }),
    }];
    r.usage = ReportUsage {
        fuel: 1,
        service_work: 2,
        host_calls: 3,
        peak_heap_bytes: 4,
        peak_live_allocations: 5,
        peak_frames: 6,
        peak_live_cells: 7,
        peak_live_tasks: 8,
        canonical_indices: 9,
    };
    r.frames = [Mode::Vm, Mode::Jit, Mode::Aot, Mode::Host, Mode::Adapter]
        .into_iter()
        .enumerate()
        .map(|(i, mode)| Frame {
            mode,
            digest: [i as u8; 32],
            member: i as u32 + 10,
            span: (i % 2 == 0).then_some(Span {
                source: SourceId(7),
                start: i as u32,
                end: i as u32 + 1,
            }),
        })
        .collect();
    r
}
#[test]
fn independent_rich_report_proves_all_frames_usage_order_and_strict_flags() {
    let v = vectors();
    let expected = hex(v["report_rich"].as_str().unwrap());
    let report = rich_report();
    assert_eq!(encode_report(&report, &mut session()).unwrap(), expected);
    assert_eq!(decode_report(&expected, &mut session()).unwrap(), report);
    for n in 0..expected.len() {
        assert!(
            decode_report(&expected[..n], &mut session()).is_err(),
            "report prefix {n}"
        );
    }
    for (name, bad) in [
        ("policy", 2),
        ("primary_category", 255),
        ("primary_phase", 255),
        ("primary_limit_kind", 17),
        ("primary_message_present", 2),
        ("primary_limit_present", 2),
        ("primary_origin_present", 2),
        ("primary_origin_mode", 5),
        ("primary_origin_span_present", 2),
        ("suppressed_origin_present", 2),
        ("poisoned", 2),
        ("frame_0_mode", 5),
        ("frame_0_span_present", 2),
    ] {
        let mut bytes = expected.clone();
        bytes[v["report_offsets"][name].as_u64().unwrap() as usize] = bad;
        assert_eq!(
            decode_report(&bytes, &mut session()).unwrap_err().kind,
            ErrorKind::Tag,
            "{name}"
        );
    }
    for (name, bad) in [
        ("policy", 0),
        ("poisoned", 0),
        ("primary_phase", 1),
        ("suppressed_phase", 0),
    ] {
        let mut bytes = expected.clone();
        bytes[v["report_offsets"][name].as_u64().unwrap() as usize] = bad;
        assert_eq!(
            decode_report(&bytes, &mut session()).unwrap_err().kind,
            ErrorKind::InvalidReport,
            "{name}"
        );
    }
    let mut span = expected;
    let at = v["report_offsets"]["primary_origin_span_start"]
        .as_u64()
        .unwrap() as usize;
    span[at..at + 4].copy_from_slice(&10u32.to_le_bytes());
    assert_eq!(
        decode_report(&span, &mut session()).unwrap_err().kind,
        ErrorKind::InvalidReport
    );
}
#[test]
fn every_report_limit_accepts_exact_and_refuses_one_below() {
    let report = rich_report();
    let bytes = hex(vectors()["report_rich"].as_str().unwrap());
    for decode in [false, true] {
        let run = |limits: Limits| -> Result<Usage, Error> {
            let mut s = Session::new(limits);
            if decode {
                decode_report(&bytes, &mut s)?;
            } else {
                encode_report(&report, &mut s)?;
            }
            Ok(s.usage())
        };
        let cost = run(Limits::default()).unwrap();
        for field in 0..7 {
            let mut exact = Limits::default();
            match field {
                0 => exact.bytes = bytes.len(),
                1 => exact.serialization_bytes = cost.serialization_bytes,
                2 => exact.elements = cost.elements,
                3 => exact.work = cost.work,
                4 => exact.allocation_bytes = cost.allocation_bytes,
                5 => exact.depth = 4,
                _ => exact.string_bytes = 37,
            }
            run(exact).unwrap_or_else(|e| panic!("decode={decode} field={field}: {e:?}"));
            match field {
                0 => exact.bytes -= 1,
                1 => exact.serialization_bytes -= 1,
                2 => exact.elements -= 1,
                3 => exact.work -= 1,
                4 => exact.allocation_bytes -= 1,
                5 => exact.depth -= 1,
                _ => exact.string_bytes -= 1,
            }
            assert!(run(exact).is_err(), "decode={decode} field={field}");
        }
    }
}
#[test]
fn every_report_diagnostic_has_exact_category_phase_and_poison_requirements() {
    let admission_diagnostics = [
        Diagnostic::InvalidRequest,
        Diagnostic::UnsupportedVersion,
        Diagnostic::MissingCapability,
        Diagnostic::IncompatibleInterface,
        Diagnostic::InvalidMetadata,
        Diagnostic::InvalidWire,
        Diagnostic::Resource,
        Diagnostic::Reentry,
        Diagnostic::UnsupportedProfile,
        Diagnostic::UnknownExport,
        Diagnostic::InstancePoisoned,
    ];
    let mut cases = admission_diagnostics
        .into_iter()
        .map(|d| (d, Category::Admission, vec![Phase::Admission]))
        .collect::<Vec<_>>();
    cases.extend([
        (
            Diagnostic::Limit,
            Category::Limit,
            vec![Phase::Call, Phase::Admission],
        ),
        (
            Diagnostic::Cancelled,
            Category::Cancellation,
            vec![Phase::Call],
        ),
        (
            Diagnostic::Interrupted,
            Category::Interruption,
            vec![Phase::Call],
        ),
        (
            Diagnostic::HostContract,
            Category::HostContract,
            vec![Phase::Call, Phase::Cleanup],
        ),
        (Diagnostic::Panic, Category::Panic, vec![Phase::Call]),
        (Diagnostic::Trap, Category::Trap, vec![Phase::Call]),
        (
            Diagnostic::Destructor,
            Category::Destructor,
            vec![Phase::Cleanup],
        ),
    ]);
    for (diagnostic, category, phases) in cases {
        for actual in [
            Category::Panic,
            Category::Trap,
            Category::Destructor,
            Category::Cancellation,
            Category::Interruption,
            Category::Limit,
            Category::Admission,
            Category::HostContract,
        ] {
            for phase in [Phase::Call, Phase::Cleanup, Phase::Admission] {
                let mut report = admission();
                report.poisoned = true;
                report.primary = Record {
                    category: actual,
                    phase,
                    diagnostic,
                    message: None,
                    limit: (actual == Category::Limit).then_some(LimitDetail {
                        kind: ReportLimitKind::Fuel,
                        limit: 0,
                        attempted: 1,
                    }),
                    origin: None,
                };
                assert_eq!(
                    encode_report(&report, &mut session()).is_ok(),
                    actual == category && phases.contains(&phase),
                    "{diagnostic:?}/{actual:?}/{phase:?}"
                );
            }
        }
    }
    let mut report = admission();
    report.primary = Record {
        category: Category::Panic,
        phase: Phase::Call,
        diagnostic: Diagnostic::Panic,
        message: None,
        limit: None,
        origin: Some(Frame {
            mode: Mode::Host,
            digest: [0; 32],
            member: 0,
            span: None,
        }),
    };
    assert_eq!(
        encode_report(&report, &mut session()).unwrap_err().kind,
        ErrorKind::InvalidReport
    );
    report.poisoned = true;
    encode_report(&report, &mut session()).unwrap();
}
fn contract_vectors() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/component_contract_v3_vectors.json")).unwrap()
}
fn locked_contract() -> Contract {
    use lang_runtime::component::negotiation::{
        BuildIdentifier, PrereleaseIdentifier, SemanticVersion,
    };
    use lang_runtime::component::serialization::{
        ComponentContractLock, DigestAlgorithm, VersionedDigest,
    };
    let checked = idl::check(
        SourceId(1),
        include_bytes!("fixtures/component_values.idl.json"),
        Default::default(),
    )
    .unwrap();
    let log = checked
        .document()
        .imports
        .iter()
        .find(|i| i.interface.value.interface() == "log")
        .unwrap()
        .interface
        .value
        .clone();
    let metrics = checked
        .document()
        .imports
        .iter()
        .find(|i| i.interface.value.interface() == "metrics")
        .unwrap()
        .interface
        .value
        .clone();
    let log_version = SemanticVersion::new(1, 4, 0)
        .with_prerelease(vec![
            PrereleaseIdentifier::alphanumeric("beta").unwrap(),
            PrereleaseIdentifier::numeric("2").unwrap(),
        ])
        .with_build(vec![BuildIdentifier::new("provider").unwrap()]);
    let metrics_version = SemanticVersion::new(3, 2, 1).with_build(vec![
        BuildIdentifier::new("audit").unwrap(),
        BuildIdentifier::new("001").unwrap(),
    ]);
    let locks = vec![
        ComponentContractLock::new(
            metrics,
            metrics_version,
            VersionedDigest::new(DigestAlgorithm::Sha256, vec![165; 32]).unwrap(),
        ),
        ComponentContractLock::new(
            log,
            log_version,
            VersionedDigest::new(DigestAlgorithm::Sha256, (0u8..32).collect::<Vec<_>>()).unwrap(),
        ),
    ];
    Contract::new(checked, locks, &mut session()).unwrap()
}
#[test]
fn independent_complete_contract_vectors_cover_versions_imports_locks_and_all_types() {
    let v = contract_vectors();
    for (name, c) in [("unlocked", data_contract()), ("locked", locked_contract())] {
        let bytes = hex(v[name].as_str().unwrap());
        assert_eq!(
            encode_contract(&c, &mut session()).unwrap(),
            bytes,
            "{name}"
        );
        let decoded = decode_contract(SourceId(9), &bytes, &mut session()).unwrap();
        assert_eq!(
            encode_contract(&decoded, &mut session()).unwrap(),
            bytes,
            "{name}"
        );
        assert_eq!(
            decoded.document().version.value.to_string(),
            "1.2.3-alpha.1+build.001"
        );
        assert_eq!(decoded.document().imports.len(), 2);
        assert_eq!(decoded.locks().len(), if name == "locked" { 2 } else { 0 });
        for n in 0..bytes.len() {
            assert!(
                decode_contract(SourceId(9), &bytes[..n], &mut session()).is_err(),
                "{name} prefix {n}"
            );
        }
    }
}
#[test]
fn contract_decoder_refuses_noncanonical_order_duplicates_and_invalid_version_policy() {
    let v = contract_vectors();
    let base = hex(v["locked"].as_str().unwrap());
    for section in [
        "resources",
        "types",
        "interfaces",
        "members",
        "requirements",
        "locks",
    ] {
        let ranges = &v["offsets"]["ranges"][section];
        let a = ranges[0][0].as_u64().unwrap() as usize;
        let b = ranges[0][1].as_u64().unwrap() as usize;
        let c = ranges[1][1].as_u64().unwrap() as usize;
        assert_eq!(ranges[1][0].as_u64().unwrap() as usize, b);
        let bytes = [&base[..a], &base[b..c], &base[a..b], &base[c..]].concat();
        assert_eq!(
            decode_contract(SourceId(1), &bytes, &mut session())
                .unwrap_err()
                .kind,
            ErrorKind::Noncanonical,
            "{section}"
        );
    }
    let fields = &v["offsets"]["fields"];
    for name in ["package_version_pre_0_tag", "package_version_pre_1_tag"] {
        let mut bytes = base.clone();
        let at = fields[name].as_u64().unwrap() as usize;
        bytes[at] ^= 1;
        assert_eq!(
            decode_contract(SourceId(1), &bytes, &mut session())
                .unwrap_err()
                .kind,
            ErrorKind::InvalidSchema,
            "{name}"
        );
    }
    let mut bytes = base.clone();
    let at = fields["requirement_0_min_core"].as_u64().unwrap() as usize;
    bytes[at..at + 8].copy_from_slice(&3u64.to_le_bytes());
    assert_eq!(
        decode_contract(SourceId(1), &bytes, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidSchema
    );
    for (name, bad, kind) in [
        ("requirement_0_prerelease", 2, ErrorKind::Tag),
        ("requirement_0_prerelease", 0, ErrorKind::InvalidSchema),
        ("lock_0_digest_tag", 1, ErrorKind::Tag),
    ] {
        let mut bytes = base.clone();
        bytes[fields[name].as_u64().unwrap() as usize] = bad;
        assert_eq!(
            decode_contract(SourceId(1), &bytes, &mut session())
                .unwrap_err()
                .kind,
            kind,
            "{name}"
        );
    }
    let mut duplicate = base.clone();
    let at = fields["exports_count"].as_u64().unwrap() as usize;
    duplicate[at..at + 4].copy_from_slice(&2u32.to_le_bytes());
    let start = v["offsets"]["ranges"]["exports"][0][0].as_u64().unwrap() as usize;
    duplicate.extend_from_slice(&base[start..]);
    assert_eq!(
        decode_contract(SourceId(1), &duplicate, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::Noncanonical
    );
}
#[test]
fn legacy_schema2_bytes_stay_exact_and_cross_profile_decoders_refuse() {
    use lang_runtime::component::codec::ComponentCodecLimits;
    use lang_runtime::component::serialization::*;
    let limits = ComponentCodecLimits::default();
    let old_contract = hex(include_str!(
        "../crates/runtime/src/component/fixtures/schema2_contract.hex"
    ));
    let old = decode_versioned_component_contract(&old_contract, limits).unwrap();
    assert_eq!(
        encode_versioned_component_contract(&old, limits).unwrap(),
        old_contract
    );
    assert_eq!(
        decode_contract(SourceId(0), &old_contract, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::Version
    );
    assert!(
        decode_versioned_component_contract(
            &hex(contract_vectors()["locked"].as_str().unwrap()),
            limits
        )
        .is_err()
    );
    let old_report = hex(include_str!(
        "../crates/runtime/src/component/fixtures/schema2_failure_report.hex"
    ));
    let old = decode_versioned_component_failure_report(&old_report, limits).unwrap();
    assert_eq!(
        encode_versioned_component_failure_report(&old, limits).unwrap(),
        old_report
    );
    assert_eq!(
        decode_report(&old_report, &mut session()).unwrap_err().kind,
        ErrorKind::Version
    );
    assert!(
        decode_versioned_component_failure_report(
            &hex(vectors()["report_rich"].as_str().unwrap()),
            limits
        )
        .is_err()
    );
}
#[test]
fn independent_resource_and_task_signatures_and_empty_tuple_frames_are_exact() {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/component_signature_v1_vectors.json")).unwrap();
    let checked = idl::check(
        SourceId(7),
        include_bytes!("fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap();
    let c = Contract::new(checked, vec![], &mut session()).unwrap();
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "apply",
        &mut session(),
    )
    .unwrap();
    assert_eq!(
        sig.preimage(&mut session()).unwrap(),
        hex(v["counter_preimage"].as_str().unwrap())
    );
    assert_eq!(
        sig.digest().as_slice(),
        hex(v["counter_digest"].as_str().unwrap())
    );
    let c = data_contract();
    let sig = Signature::new(
        &c,
        &c.document().interfaces[0].key.value,
        "tick",
        &mut session(),
    )
    .unwrap();
    assert_eq!(
        sig.preimage(&mut session()).unwrap(),
        hex(v["tick_preimage"].as_str().unwrap())
    );
    assert_eq!(
        sig.digest().as_slice(),
        hex(v["tick_digest"].as_str().unwrap())
    );
    let values = Values::default();
    let expected = hex(v["tick_parameters"].as_str().unwrap());
    assert_eq!(
        encode_values(&sig, FrameKind::Parameters, &values, &mut session()).unwrap(),
        expected
    );
    assert_eq!(
        decode_values(&sig, FrameKind::Parameters, &expected, &mut session()).unwrap(),
        values
    );
    let values = Values {
        roots: vec![ValueId(0)],
        nodes: vec![Value::I64(i64::MIN)],
    };
    let expected = hex(v["tick_result"].as_str().unwrap());
    assert_eq!(
        encode_values(&sig, FrameKind::Result, &values, &mut session()).unwrap(),
        expected
    );
    assert_eq!(
        decode_values(&sig, FrameKind::Result, &expected, &mut session()).unwrap(),
        values
    );
}
#[test]
fn all_report_limit_tags_are_exact_and_minimum_record_bytes_precede_allocation() {
    let v = vectors();
    let base = hex(v["report_rich"].as_str().unwrap());
    let fields = &v["report_offsets"];
    let tags = [
        ReportLimitKind::Fuel,
        ReportLimitKind::ServiceWork,
        ReportLimitKind::HostCalls,
        ReportLimitKind::HeapBytes,
        ReportLimitKind::LiveAllocations,
        ReportLimitKind::Frames,
        ReportLimitKind::LiveCells,
        ReportLimitKind::LiveTasks,
        ReportLimitKind::CanonicalIndices,
        ReportLimitKind::PreparationWork,
        ReportLimitKind::MetadataBytes,
        ReportLimitKind::ValueElements,
        ReportLimitKind::SerializationBytes,
        ReportLimitKind::Depth,
        ReportLimitKind::DecoderAllocationBytes,
        ReportLimitKind::SuppressedRecords,
        ReportLimitKind::TraceFrames,
    ];
    for (tag, kind) in tags.into_iter().enumerate() {
        let mut bytes = base.clone();
        bytes[fields["primary_limit_kind"].as_u64().unwrap() as usize] = tag as u8;
        let r = decode_report(&bytes, &mut session()).unwrap();
        assert_eq!(r.primary.limit.unwrap().kind, kind);
        assert_eq!(encode_report(&r, &mut session()).unwrap(), bytes);
    }
    let mut huge = base;
    let at = fields["suppressed_count"].as_u64().unwrap() as usize;
    huge[at..at + 4].copy_from_slice(&20u32.to_le_bytes());
    let mut s = session();
    assert_eq!(
        decode_report(&huge, &mut s).unwrap_err().kind,
        ErrorKind::Truncated
    );
    assert_eq!(s.usage().allocation_bytes, 22);
}
#[test]
fn binary_metadata_rechecks_unused_cycles_loans_and_duplicate_field_spans() {
    let v = contract_vectors();
    let base = hex(v["locked"].as_str().unwrap());
    let ranges = &v["offsets"]["ranges"]["types"];
    let start = ranges[2][0].as_u64().unwrap() as usize;
    let end = ranges[2][1].as_u64().unwrap() as usize;
    let name = start
        + base[start..end]
            .windows(12)
            .position(|s| s == b"hidden-owner")
            .unwrap();
    let resource = start
        + base[start..end]
            .windows(12)
            .position(|s| s == b"unused-owner")
            .unwrap();
    let tag = name + 13;
    assert_eq!(base[tag], 2);
    let mut cycle = base.clone();
    cycle[tag] = 11;
    cycle[resource..resource + 12].copy_from_slice(b"hidden-owner");
    let error = decode_contract(SourceId(77), &cycle, &mut session()).unwrap_err();
    assert!(matches!(
        error.kind,
        ErrorKind::Declaration(idl::Error {
            reason: idl::Reason::CyclicType,
            span: lang_mir::unit::Span {
                source: SourceId(77),
                ..
            },
            ..
        })
    ));
    let mut loan = base.clone();
    loan[tag] = 3;
    assert!(matches!(
        decode_contract(SourceId(77), &loan, &mut session())
            .unwrap_err()
            .kind,
        ErrorKind::Declaration(idl::Error {
            reason: idl::Reason::InvalidBorrow,
            ..
        })
    ));
    let start = ranges[3][0].as_u64().unwrap() as usize;
    let end = ranges[3][1].as_u64().unwrap() as usize;
    let flag = start
        + base[start..end]
            .windows(4)
            .position(|s| s == b"flag")
            .unwrap();
    let mut duplicate = base;
    duplicate[flag..flag + 4].copy_from_slice(b"code");
    let error = decode_contract(SourceId(77), &duplicate, &mut session()).unwrap_err();
    assert!(matches!(
        error.kind,
        ErrorKind::Declaration(idl::Error {
            reason: idl::Reason::Duplicate,
            related: Some(_),
            ..
        })
    ));
}
