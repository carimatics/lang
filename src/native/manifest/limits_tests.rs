use super::*;
#[test]
fn json_depth_and_byte_limits_admit_the_exact_boundary() {
    let max = Limits::default().program_manifest;
    let mut bytes = b"true".to_vec();
    bytes.resize(max, b' ');
    parse::<de::IgnoredAny>(&bytes, max, 32).unwrap();
    bytes.push(b' ');
    assert!(matches!(
        parse::<de::IgnoredAny>(&bytes, max, 32),
        Err(Error {
            kind: ErrorKind::Limit {
                resource: "manifest bytes",
                ..
            },
            ..
        })
    ));
    for n in [32, 33] {
        let input = format!("{}0{}", "[".repeat(n), "]".repeat(n));
        let result = parse::<de::IgnoredAny>(input.as_bytes(), max, 32);
        if n == 32 {
            result.unwrap();
        } else {
            assert!(matches!(
                result,
                Err(Error {
                    kind: ErrorKind::Limit {
                        resource: "JSON depth",
                        actual: 33,
                        maximum: 32
                    },
                    ..
                })
            ));
        }
    }
    parse::<de::IgnoredAny>(br#"["[[[\\\"}}}{{{"]"#, max, 2).unwrap();
}
#[test]
fn table_and_utf8_string_limits_refuse_before_growing_past_the_boundary() {
    for n in [8, 9] {
        let text = format!("[{}]", vec!["null"; n].join(","));
        let result = serde_json::from_str::<Table<de::IgnoredAny, 8>>(&text);
        assert_eq!(result.is_ok(), n == 8);
    }
    let exact = "é".repeat(2048);
    let encoded = serde_json::to_vec(&exact).unwrap();
    let text: Text<4096> = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(text.0.len(), 4096);
    assert!(
        serde_json::from_slice::<Text<4096>>(&serde_json::to_vec(&(exact + "x")).unwrap()).is_err()
    );
}
