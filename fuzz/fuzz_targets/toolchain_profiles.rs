#![no_main]
use lang::profile::{self, allocation, runtime};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    // These bounded readers validate observation metadata only; decoding does
    // not execute a package, grant capabilities or authenticate an artifact.
    // Canonical encoding may add an explicit optional field at the byte limit;
    // refusing that larger output is valid, but every emitted report must round-trip.
    if let Ok(report) = profile::Report::decode(bytes)
        && let Ok(encoded) = report.encode()
    {
        assert_eq!(profile::Report::decode(&encoded).unwrap(), report);
    }
    if let Ok(report) = runtime::Report::decode(bytes)
        && let Ok(encoded) = report.encode()
    {
        assert_eq!(runtime::Report::decode(&encoded).unwrap(), report);
    }
    if let Ok(report) = allocation::Report::decode(bytes)
        && let Ok(encoded) = report.encode()
    {
        assert_eq!(allocation::Report::decode(&encoded).unwrap(), report);
    }
});
