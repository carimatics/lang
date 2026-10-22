#![no_main]
use lang_mir::unit::SourceId;
use lang_runtime::component::{idl, wire};
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

fn signature() -> &'static wire::Signature<'static> {
    static CONTRACT: OnceLock<wire::Contract> = OnceLock::new();
    static SIGNATURE: OnceLock<wire::Signature<'static>> = OnceLock::new();
    let contract = CONTRACT.get_or_init(|| {
        let checked = idl::check(
            SourceId(0),
            include_bytes!("../../tests/fixtures/component_values.idl.json"),
            Default::default(),
        )
        .unwrap();
        wire::Contract::new(checked, vec![], &mut wire::Session::new(Default::default())).unwrap()
    });
    SIGNATURE.get_or_init(|| {
        wire::Signature::new(
            contract,
            &contract.document().interfaces[0].key.value,
            "apply",
            &mut wire::Session::new(Default::default()),
        )
        .unwrap()
    })
}
fuzz_target!(|input: &[u8]| {
    let Some((&kind, bytes)) = input.split_first() else {
        return;
    };
    let limits = wire::Limits {
        metadata: idl::Limits {
            source_bytes: 4096,
            depth: 32,
            declarations: 64,
            elements: 256,
            string_bytes: 256,
            work: 32768,
            allocation_bytes: 256 * 1024,
        },
        bytes: 4096,
        serialization_bytes: 4096,
        depth: 32,
        elements: 256,
        string_bytes: 256,
        work: 32768,
        allocation_bytes: 512 * 1024,
    };
    let mut session = wire::Session::new(limits);
    match kind % 4 {
        0 => {
            if let Ok(c) = wire::decode_contract(SourceId(0), bytes, &mut session)
                && let Ok(encoded) = wire::encode_contract(&c, &mut wire::Session::new(limits))
            {
                assert_eq!(encoded, bytes);
            }
        }
        1 => {
            if let Ok(r) = wire::decode_report(bytes, &mut session)
                && let Ok(encoded) = wire::encode_report(&r, &mut wire::Session::new(limits))
            {
                assert_eq!(encoded, bytes);
            }
        }
        n => {
            let kind = if n == 2 {
                wire::FrameKind::Parameters
            } else {
                wire::FrameKind::Result
            };
            if let Ok(values) = wire::decode_values(signature(), kind, bytes, &mut session) {
                assert!(values.roots.iter().all(|id| id.0 < values.nodes.len()));
                for (index, value) in values.nodes.iter().enumerate() {
                    match value {
                        wire::Value::List(ids) | wire::Value::Record(ids) => {
                            assert!(ids.iter().all(|id| id.0 < index))
                        }
                        wire::Value::Variant { payload, .. }
                        | wire::Value::Some(payload)
                        | wire::Value::Ok(payload)
                        | wire::Value::Err(payload) => assert!(payload.0 < index),
                        _ => {}
                    }
                }
                if let Ok(encoded) =
                    wire::encode_values(signature(), kind, &values, &mut wire::Session::new(limits))
                {
                    assert_eq!(encoded, bytes);
                }
            }
        }
    }
    let usage = session.usage();
    assert!(usage.work <= limits.work && usage.allocation_bytes <= limits.allocation_bytes);
    assert!(
        usage.elements <= limits.elements
            && usage.serialization_bytes <= limits.serialization_bytes
    );
});
