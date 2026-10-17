#![no_main]

use lang_bytecode::unit::{CodecLimits, decode, encode};
use lang_mir::unit::Limits;
use libfuzzer_sys::fuzz_target;

fn check(bytes: &[u8], limits: CodecLimits) {
    if let Ok(program) = decode(bytes, limits) {
        // Encoding is a separate operation with its own work requirements.
        let encoded = encode(
            &program,
            CodecLimits {
                bytes: limits.bytes,
                ..CodecLimits::default()
            },
        )
        .unwrap();
        assert_eq!(encoded, bytes);
        // No host authority is supplied. Guest instructions, recursion, and
        // aggregate copies remain bounded even for valid adversarial units.
        let _ = lang_vm::unit::execute_entry(
            &program,
            &lang_runtime::HostFunctionRegistry::new(),
            &lang_vm::unit::HostBindings::new(),
            lang_vm::unit::Control::with_limits(lang_vm::unit::Limits {
                fuel: Some(128),
                host_calls: Some(0),
                frames: Some(8),
                live_cells: Some(4096),
            }),
        );
    }
}

fuzz_target!(|bytes: &[u8]| {
    let limits = CodecLimits {
        bytes: 16_384,
        validation: Limits {
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
            work: 10_000,
            allocations: 128 * 1024,
            elements: 4096,
        },
    };
    check(bytes, limits);
    // Also mutate a valid payload so short inputs exercise deeper decoding
    // without first having to discover the magic/version/length prefix.
    let hex: String = include_str!("../../crates/bytecode/tests/fixtures/unit-v1-main-42.hex")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let mut seed: Vec<u8> = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    for pair in bytes.chunks(2).take(128) {
        let index = 16 + usize::from(pair[0]) % (seed.len() - 16);
        seed[index] ^= pair.get(1).copied().unwrap_or(1);
    }
    check(&seed, limits);
});
