#![no_main]

use lang_runtime::component::codec::ComponentCodecLimits;
use lang_runtime::component::serialization::decode_versioned_component_failure_report;
use lang_runtime::component::wire;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = decode_versioned_component_failure_report(bytes, ComponentCodecLimits::default());
    let mut session = wire::Session::new(wire::Limits {
        bytes: 16 * 1024,
        serialization_bytes: 16 * 1024,
        allocation_bytes: 0,
        ..Default::default()
    });
    if wire::decode_cleanup_report(bytes, &mut session).is_ok() {
        assert!(wire::decode_report(bytes, &mut wire::Session::new(Default::default())).is_ok());
    }
});
