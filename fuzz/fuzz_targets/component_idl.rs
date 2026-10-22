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
        let generation = lang::component_bindings::Limits {
            output_bytes: 64 * 1024,
            depth: 32,
            budget: lang_mir::unit::Limits {
                work: 256 * 1024,
                allocations: 2 * 1024 * 1024,
                elements: 16384,
                ..Default::default()
            },
        };
        if let Ok(guest) = lang::component_bindings::guest(&checked, generation) {
            assert!(guest.declarations.len() <= guest.text_bytes);
            assert!(guest.text_bytes <= generation.output_bytes);
            assert!(guest.usage.work <= generation.budget.work);
        }
        let mut session = lang_runtime::component::wire::Session::new(Default::default());
        if let Ok(contract) =
            lang_runtime::component::wire::Contract::new(checked, vec![], &mut session)
            && let Ok(c) = lang::component_bindings::c::generate(
                &contract,
                lang::component_bindings::abi::Target::MacosAarch64,
                generation,
            )
        {
            assert!(c.header.len() <= c.text_bytes);
            assert!(c.text_bytes <= generation.output_bytes);
            assert!(c.usage.work <= generation.budget.work);
        }
    }
});
