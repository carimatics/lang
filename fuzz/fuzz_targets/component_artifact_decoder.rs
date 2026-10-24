#![no_main]
use lang::artifact::component::{self, Limits};
use libfuzzer_sys::fuzz_target;
fn check(bytes: &[u8]) {
    let mut limits = Limits {
        bytes: 16_384,
        ..Default::default()
    };
    let common = &mut limits.tasks.bodies.places.ownership.unit;
    common.sources = 8;
    common.source_bytes = 4096;
    common.total_source_bytes = 8192;
    common.definitions = 32;
    common.imports = 32;
    common.functions = 32;
    common.fields = 32;
    common.registers = 128;
    common.blocks = 64;
    common.instructions = 256;
    common.depth = 16;
    common.work = 100_000;
    common.allocations = 2 * 1024 * 1024;
    common.elements = 32_768;
    limits.tasks.scopes = 128;
    limits.tasks.bindings = 256;
    limits.tasks.state_cells = 4096;
    limits.tasks.loan_nodes = 1024;
    limits.tasks.loan_words = 4096;
    limits.tasks.failure_nodes = 1024;
    limits.metadata.metadata.declarations = 128;
    limits.metadata.metadata.elements = 1024;
    limits.metadata.metadata.depth = 16;
    limits.bindings.bindings = 128;
    if let Ok(program) = component::decode(bytes, limits) {
        let id = program.unit().document().document().source.0 as usize;
        let snapshot = program.unit().code().declarations().types().table().sources[id].clone();
        let entry = program.entry();
        let encoded =
            component::encode(program.into_unit(), snapshot, entry, Limits::default()).unwrap();
        assert_eq!(encoded, bytes);
    }
}
fuzz_target!(|bytes: &[u8]| {
    check(bytes);
    let mut seed: Vec<u8> = include_str!("../../tests/fixtures/component_artifact_v5.hex")
        .split_whitespace()
        .map(|s| u8::from_str_radix(s, 16).unwrap())
        .collect();
    for pair in bytes.chunks(3).take(128) {
        let index = (usize::from(pair[0]) * 256 + usize::from(pair.get(1).copied().unwrap_or(0)))
            % seed.len();
        seed[index] ^= pair.get(2).copied().unwrap_or(1);
    }
    check(&seed);
});
