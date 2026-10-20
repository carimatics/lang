#![no_main]
use lang::native::manifest::{Limits, parse_program, parse_sdk};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    let limits = Limits {
        sdk_manifest: 16384,
        program_manifest: 16384,
        depth: 8,
        ..Limits::default()
    };
    // Pure bounded parsing: no file opens, tools, symbol resolution or code.
    let _ = parse_program(bytes, limits);
    let _ = parse_sdk(bytes, limits);
});
