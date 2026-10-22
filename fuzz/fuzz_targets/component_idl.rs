#![no_main]
use lang_mir::unit::SourceId;
use lang_runtime::component::idl::{self, Limits};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let limits = Limits {
        source_bytes: 4096,
        depth: 32,
        declarations: 64,
        elements: 256,
        string_bytes: 256,
        work: 32768,
        allocation_bytes: 256 * 1024,
    };
    if let Ok(checked) = idl::check(SourceId(0), bytes, limits) {
        assert_eq!(checked.document().source_bytes as usize, bytes.len());
        assert!(checked.usage().declarations <= limits.declarations);
        assert!(checked.usage().elements <= limits.elements);
        for ty in &checked.document().type_nodes {
            assert!(ty.span.start <= ty.span.end);
            assert!(ty.span.end as usize <= bytes.len());
        }
    }
});
