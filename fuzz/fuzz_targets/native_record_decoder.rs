#![no_main]
use lang_codegen_clif::native_record::{Limits, Record};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let limits = Limits {
        bytes: 16_384,
        validation: lang_mir::unit::Limits {
            work: 100_000,
            allocations: 1_048_576,
            elements: 16_384,
            functions: 64,
            instructions: 1_024,
            ..Default::default()
        },
    };
    // Framing/data only: never reconstruct executable pointers or bind code.
    for semantic in [b"".as_slice(), b"mir".as_slice(), bytes] {
        if let Ok(record) = Record::decode(bytes, semantic, limits)
            && let Ok(encoded) = record.encode(semantic, limits)
        {
            assert_eq!(encoded, bytes);
            let decoded = Record::decode(&encoded, semantic, limits).unwrap();
            assert_eq!(decoded.kind(), record.kind());
            assert_eq!(decoded.target(), record.target());
            assert_eq!(decoded.object(), record.object());
            assert_eq!(decoded.functions(), record.functions());
        }
    }
});
