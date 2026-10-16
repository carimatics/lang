#![no_main]

use lang_runtime::component::codec::ComponentCodecLimits;
use lang_runtime::component::serialization::decode_versioned_component_failure_report;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = decode_versioned_component_failure_report(bytes, ComponentCodecLimits::default());
});
