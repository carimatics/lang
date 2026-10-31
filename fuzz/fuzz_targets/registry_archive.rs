#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(archive) = lang::registry::archive::AdmittedArchive::admit(bytes) {
        assert_eq!(archive.as_bytes(), bytes);
        for file in archive.descriptor().files() {
            let source = archive.source(file.path()).expect("admitted source");
            assert_eq!(source.len() as u64, file.bytes());
            assert!(std::str::from_utf8(source).is_ok());
        }
    }
});
