#![no_main]
use lang::package::{lock, manifest::Manifest};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    // Both readers enforce byte/nesting bounds before any source access.
    // Encoding admission alone never authorizes lock replay.
    let _ = Manifest::parse(bytes);
    let _ = lock::validate_encoding(bytes);
});
