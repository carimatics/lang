#![no_main]

use lang_bytecode::ownership::{CodecLimits, decode, encode};
use libfuzzer_sys::fuzz_target;

fn limits() -> CodecLimits {
    let mut cfg = CodecLimits {
        bytes: 16_384,
        ..Default::default()
    };
    let b = &mut cfg.verification.bodies;
    b.loans = 128;
    b.prepared = 128;
    b.places.places = 128;
    b.places.ownership.constructed = 128;
    b.places.ownership.unit = lang_mir::unit::Limits {
        sources: 8,
        source_bytes: 1024,
        total_source_bytes: 4096,
        definitions: 8,
        functions: 8,
        imports: 8,
        fields: 8,
        registers: 128,
        blocks: 64,
        instructions: 256,
        depth: 8,
        value_cells: 64,
        work: 20_000,
        allocations: 256 * 1024,
        elements: 8192,
    };
    cfg
}
fn check(bytes: &[u8]) {
    if let Ok(program) = decode(bytes, limits()) {
        let (table, entry) = program.into_parts();
        // Encoding is a new operation with a new budget. No runtime allocator,
        // host binding, callback, guest execution or filesystem access occurs.
        let encoded = encode(table, entry, CodecLimits::default()).unwrap();
        assert_eq!(encoded, bytes);
    }
}
fuzz_target!(|bytes: &[u8]| {
    check(bytes);
    for source in [
        include_str!("../../crates/bytecode/tests/fixtures/unit-v2-main-42.hex"),
        include_str!("../../crates/bytecode/tests/fixtures/unit-v2-bytes.hex"),
    ] {
        let mut seed: Vec<u8> = source
            .split_whitespace()
            .map(|s| u8::from_str_radix(s, 16).unwrap())
            .collect();
        for pair in bytes.chunks(2).take(128) {
            let index = 16 + usize::from(pair[0]) % (seed.len() - 16);
            seed[index] ^= pair.get(1).copied().unwrap_or(1);
        }
        check(&seed);
    }
});
