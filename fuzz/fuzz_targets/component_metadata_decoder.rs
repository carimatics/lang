#![no_main]

use lang_runtime::component::codec::{ComponentCodecLimits, decode_component_metadata};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let limits = ComponentCodecLimits::new(4096, 64, 3, 256);
    let _ = decode_component_metadata(bytes, limits);
});
