#![no_main]
use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::component::{
    bindings as b, idl,
    negotiation::{InterfaceKey, PackageKey, ResourceTypeKey},
};
use libfuzzer_sys::fuzz_target;
use std::{path::Path, sync::OnceLock};

fn document() -> idl::CheckedDocument {
    idl::check(
        m::SourceId(77),
        include_bytes!("../../tests/fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap()
}
fn fixture() -> &'static b::Unit {
    static UNIT: OnceLock<b::Unit> = OnceLock::new();
    UNIT.get_or_init(|| {
        let source = include_bytes!("../../tests/fixtures/component_counter.lang");
        let interface = InterfaceKey::new(PackageKey::new("demo", "counter"), "api");
        let input = frontend::component::Input {
            version: 1,
            resources: vec![frontend::component::Resource {
                tag: "CounterKind".into(),
                resource: ResourceTypeKey::new(interface.clone(), "counter"),
            }],
            imports: vec![],
            exports: vec![frontend::component::Export {
                source: "apply".into(),
                member: b::MemberKey {
                    interface,
                    member: "apply".into(),
                },
            }],
        };
        let out = frontend::component::analyze(frontend::component::Request {
            sources: SourceMap::new(),
            display: Path::new("counter.lang"),
            bytes: source,
            root: None,
            compile: Default::default(),
            document: document(),
            bindings: input,
            limits: Default::default(),
        });
        assert!(out.diagnostics.is_empty());
        let unit = out.result.unwrap();
        b::Unit::new(
            document(),
            unit.code().clone(),
            unit.mappings().clone(),
            Default::default(),
        )
        .unwrap()
    })
}
fuzz_target!(|bytes: &[u8]| {
    let base = fixture();
    let byte = |i: usize| bytes.get(i).copied().unwrap_or(0);
    let number = u32::from_le_bytes([byte(1), byte(2), byte(3), byte(4)]);
    let mut map = base.mappings().clone();
    let mut limits = b::Limits::default();
    let input = &bytes[..bytes.len().min(128)];
    match byte(0) % 12 {
        0 => map.version = number,
        1 => map.resources[0].tag = m::DefinitionId(number),
        2 => map.exports[0].source = m::FunctionId(number),
        3 => map.exports[0].member.member = String::from_utf8_lossy(input).into_owned(),
        4 => {
            map.resources[0].resource = ResourceTypeKey::new(
                map.resources[0].resource.interface().clone(),
                String::from_utf8_lossy(input),
            )
        }
        5 => map.resources.clear(),
        6 => map.exports.clear(),
        7 => map.resources.push(map.resources[0].clone()),
        8 => map.exports.push(map.exports[0].clone()),
        9 => limits.budget.work = base.usage().work.saturating_sub(usize::from(byte(1))),
        10 => {
            limits.budget.allocations = base
                .usage()
                .allocation_bytes
                .saturating_sub(usize::from(byte(1)))
        }
        _ => limits.depth = usize::from(byte(1)),
    }
    if let Ok(unit) = b::Unit::new(document(), base.code().clone(), map, limits) {
        assert!(unit.usage().work <= limits.budget.work);
        assert!(unit.usage().allocation_bytes <= limits.budget.allocations);
        assert!(unit.usage().elements <= limits.budget.elements);
        assert!(unit.resource(m::DefinitionId(0)).is_some());
        assert_eq!(unit.exports().len(), 1);
        assert_eq!(unit.exports()[0].0, m::FunctionId(0));
    }
});
