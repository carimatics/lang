use lang::{artifact::component as codec, frontend::component as f};
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::{
    allocation as a,
    component::{
        bindings as b, execution, grants, idl,
        negotiation::{InterfaceKey, PackageKey},
        program, storage, wire,
    },
    execution as e,
    layout::Immediate,
    task::runner::EntryArgument,
};
use std::path::Path;
const SOURCE: &str = "pub fn relay(n:i64)->i64{return n+1;}";
const IDL: &str = r#"{
 "idl":1,"package":"demo:artifact","version":"1.0.0",
 "interfaces":[{"key":"demo:artifact/api","resources":[],"types":[],
 "functions":[{"name":"relay","effect":"sync","params":[{"name":"n","type":"i64"}],
 "result":"i64","capability":"demo.artifact.relay"}]}],
 "imports":[],"exports":["demo:artifact/api"]
}"#;
fn unit() -> b::Unit {
    unit_for(SOURCE, IDL)
}
fn unit_for(source: &str, idl: &str) -> b::Unit {
    let out = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("artifact-guest.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: idl::check(m::SourceId(77), idl.as_bytes(), Default::default()).unwrap(),
        bindings: f::Input {
            version: 1,
            resources: vec![],
            imports: vec![],
            exports: vec![f::Export {
                source: "relay".into(),
                member: b::MemberKey {
                    interface: InterfaceKey::new(PackageKey::new("demo", "artifact"), "api"),
                    member: "relay".into(),
                },
            }],
        },
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    out.result.unwrap()
}
fn snapshot() -> m::Source {
    m::Source {
        path: "artifact.idl.json".into(),
        text: IDL.into(),
    }
}
#[test]
fn stored_component_export_revalidates_original_origins_and_executes_on_both_engines() {
    let unit = unit();
    assert_eq!(unit.document().document().source, m::SourceId(77));
    let bytes = codec::encode(unit, snapshot(), None, Default::default()).unwrap();
    assert_eq!(bytes[16], 1);
    let lang::artifact::Program::Component(loaded) =
        lang::artifact::decode(&bytes, Default::default()).unwrap()
    else {
        panic!("component profile")
    };
    let unit = loaded.into_unit();
    assert_eq!(unit.document().document().source, m::SourceId(1));
    let sources = &unit.code().declarations().types().table().sources;
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].text, SOURCE);
    assert_eq!(sources[1], snapshot());
    execute_both(&unit);
    assert_eq!(
        codec::encode(unit, snapshot(), None, Default::default()).unwrap(),
        bytes
    );
}

fn execute_both(unit: &b::Unit) {
    let mut session = wire::Session::new(Default::default());
    let contract = wire::artifact::source_contract(unit, &mut session, 65_536).unwrap();
    assert_eq!(contract.document(), unit.document().document());
    let bindings = grants::prepare(unit, &contract, &[], || &[], &mut session).unwrap();
    let program = program::Program::prepare(&bindings, Default::default()).unwrap();
    let native = lang_codegen_clif::tasks::CompiledProgram::compile_component(
        &program,
        m::FunctionId(0),
        Default::default(),
    )
    .unwrap();
    let mut previous = None;
    for compiled in [false, true] {
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut budget = m::Budget::new(Default::default());
        let mut resources = storage::Resources::prepare(&allocator, 2, 4, &mut budget).unwrap();
        let control = e::Control::with_limits(Default::default());
        let mut engine = lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
        let mut bridge = engine.begin(control).unwrap();
        let mut instance = execution::Instance::new(&program);
        let mut call = execution::Invocation::prepare(
            &mut instance,
            m::FunctionId(0),
            &mut resources,
            &mut bridge,
            &mut budget,
        )
        .unwrap();
        let args = [EntryArgument::I64(41)];
        let output = if compiled {
            native.execute_component(
                &mut allocator,
                control,
                Default::default(),
                &args,
                &mut call,
            )
        } else {
            lang_vm::unit::tasks::execute_component(
                &program,
                m::FunctionId(0),
                &mut allocator,
                control,
                Default::default(),
                &args,
                &mut call,
            )
        }
        .unwrap();
        assert!(matches!(
            output.value,
            execution::Value::Copy(Immediate::I64(42))
        ));
        assert!(!output.poisoned);
        assert!(output.report.is_none());
        if let Some(usage) = previous {
            assert_eq!(output.usage, usage)
        } else {
            previous = Some(output.usage)
        }
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn stored_task_component_drives_the_existing_tree_without_source_reparse() {
    let source = SOURCE
        .replace("pub fn", "pub async fn")
        .replace("{return", "{yield;return");
    let idl = IDL.replace("\"effect\":\"sync\"", "\"effect\":\"task\"");
    let bytes = codec::encode(
        unit_for(&source, &idl),
        m::Source {
            path: "artifact.idl.json".into(),
            text: idl,
        },
        None,
        Default::default(),
    )
    .unwrap();
    let loaded = codec::decode(&bytes, Default::default()).unwrap();
    execute_both(loaded.unit());
}

fn reference() -> Vec<u8> {
    include_str!("fixtures/component_artifact_v5.hex")
        .split_whitespace()
        .map(|s| u8::from_str_radix(s, 16).unwrap())
        .collect()
}
#[test]
fn independent_complete_component_vector_freezes_tags_effects_versions_and_unused_types() {
    let bytes = reference();
    let loaded = codec::decode(&bytes, Default::default()).unwrap();
    let doc = loaded.unit().document().document();
    assert_eq!(doc.type_nodes.len(), 13);
    assert!(matches!(doc.type_nodes[0].kind, idl::TypeKind::Unit));
    assert!(matches!(doc.type_nodes[1].kind, idl::TypeKind::I64));
    assert!(matches!(doc.type_nodes[9].kind, idl::TypeKind::Own(_)));
    assert!(matches!(
        doc.type_nodes[10].kind,
        idl::TypeKind::Borrow {
            access: idl::Access::Shared,
            ..
        }
    ));
    assert!(matches!(
        doc.type_nodes[11].kind,
        idl::TypeKind::Borrow {
            access: idl::Access::Exclusive,
            ..
        }
    ));
    assert_eq!(loaded.unit().imports().len(), 1);
    assert!(matches!(
        loaded
            .unit()
            .code()
            .declarations()
            .types()
            .table()
            .constructed[0]
            .constructor,
        lang_mir::unit::ownership::Constructor::ComponentResource(m::DefinitionId(0))
    ));
    assert!(idl::check(m::SourceId(1), b"{}", Default::default()).is_err());
    let snapshot = m::Source {
        path: "reference.idl.json".into(),
        text: "{}".into(),
    };
    assert_eq!(
        codec::encode(loaded.into_unit(), snapshot, None, Default::default()).unwrap(),
        bytes
    );
}

#[test]
fn component_decode_and_all_proofs_share_exact_aggregate_ceilings() {
    let bytes = reference();
    let usage = codec::decode(&bytes, Default::default()).unwrap().usage();
    for (kind, used) in [
        (0, usage.work),
        (1, usage.allocation_bytes),
        (2, usage.elements),
    ] {
        assert!(used > 0);
        for decrement in [0, 1] {
            let mut limits = codec::Limits::default();
            let common = &mut limits.tasks.bodies.places.ownership.unit;
            match kind {
                0 => common.work = used - decrement,
                1 => common.allocations = used - decrement,
                _ => common.elements = used - decrement,
            }
            assert_eq!(
                codec::decode(&bytes, limits).is_ok(),
                decrement == 0,
                "kind {kind}, used {used}, decrement {decrement}"
            );
        }
    }
}
#[test]
fn corrupted_origins_unused_import_abi_bindings_and_versions_fail_before_admission() {
    let bytes = reference();
    let raw = lang_bytecode::component::decode(&bytes, Default::default()).unwrap();
    let base = raw.metadata_offset;
    let host = bytes
        .windows(b"host_apply".len())
        .position(|v| v == b"host_apply")
        .unwrap()
        + b"host_apply".len();
    let map = bytes.len() - 139;
    for (offset, value) in [
        (base, u32::MAX),
        (base + 4, 3),
        (base + 28 + 8, 3),
        (map, 2),
        (map + 8, 99),
        (map + 55, 99),
    ] {
        let mut broken = bytes.clone();
        broken[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(
            codec::decode(&broken, Default::default()).is_err(),
            "offset {offset}"
        );
    }
    let mut abi = bytes.clone();
    abi[host] = 0;
    assert!(matches!(
        codec::decode(&abi, Default::default()).err().unwrap(),
        codec::Error::Bindings(_)
    ));
    let mut pre = bytes.clone();
    pre[base + 68] = 2;
    assert!(matches!(
        codec::decode(&pre, Default::default()).err().unwrap(),
        codec::Error::Metadata(_)
    ));
    let mut zero = bytes.clone();
    zero[base + 74] = 0;
    zero[base + 79..base + 81].copy_from_slice(b"01");
    assert!(codec::decode(&zero, Default::default()).is_err());
    let source = bytes.windows(2).position(|v| v == b"{}").unwrap();
    let mut boundary = bytes.clone();
    boundary[source..source + 2].copy_from_slice("é".as_bytes());
    boundary[base + 28 + 4..base + 28 + 8].copy_from_slice(&1u32.to_le_bytes());
    assert!(matches!(
        codec::decode(&boundary, Default::default()).err().unwrap(),
        codec::Error::Metadata(_)
    ));
    for prefix in 0..bytes.len() {
        assert!(codec::decode(&bytes[..prefix], Default::default()).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    let length = (trailing.len() - 16) as u32;
    trailing[12..16].copy_from_slice(&length.to_le_bytes());
    assert!(codec::decode(&trailing, Default::default()).is_err());
}
#[test]
fn serialized_dictionary_rechecks_unreachable_nodes_and_rejects_noncanonical_order() {
    let bytes = reference();
    let loaded = codec::decode(&bytes, Default::default()).unwrap();
    let sources = &loaded.unit().code().declarations().types().table().sources;
    let original = loaded.unit().document().document();
    let mappings = loaded.unit().mappings();
    let mut cases = Vec::new();
    let mut d = original.clone();
    d.interfaces.reverse();
    cases.push(d);
    let mut d = original.clone();
    let id = d.type_nodes.len();
    d.type_nodes.push(idl::TypeNode {
        kind: idl::TypeKind::Option(idl::TypeId(id)),
        span: d.package.span,
    });
    cases.push(d);
    let mut d = original.clone();
    d.type_nodes.push(idl::TypeNode {
        kind: idl::TypeKind::List(idl::TypeId(11)),
        span: d.package.span,
    });
    cases.push(d);
    let mut d = original.clone();
    d.type_nodes[9].kind = idl::TypeKind::Own(idl::Spanned {
        value: lang_runtime::component::negotiation::ResourceTypeKey::new(
            InterfaceKey::new(PackageKey::new("demo", "artifact"), "host"),
            "unknown",
        ),
        span: d.package.span,
    });
    cases.push(d);
    for document in cases {
        assert!(
            wire::artifact::prepare(
                document,
                mappings.clone(),
                sources,
                &mut wire::Session::new(Default::default()),
                65_536
            )
            .is_err()
        );
    }
}

#[test]
fn multifile_code_snapshots_keep_indices_and_original_signature_spans() {
    use lang_runtime::component::negotiation::ResourceTypeKey;
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/component_bindings_modules");
    let interface = InterfaceKey::new(PackageKey::new("demo", "counter"), "api");
    let out = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("main.lang"),
        bytes: include_bytes!("fixtures/component_bindings_modules/main.lang"),
        root: Some(&root),
        compile: Default::default(),
        document: idl::check(
            m::SourceId(77),
            include_bytes!("fixtures/component_counter.idl.json"),
            Default::default(),
        )
        .unwrap(),
        bindings: f::Input {
            version: 1,
            resources: vec![f::Resource {
                tag: "inner::CounterKind".into(),
                resource: ResourceTypeKey::new(interface.clone(), "counter"),
            }],
            imports: vec![],
            exports: vec![f::Export {
                source: "inner::apply".into(),
                member: b::MemberKey {
                    interface,
                    member: "apply".into(),
                },
            }],
        },
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let unit = out.result.unwrap();
    let expected = unit.code().declarations().types().table().sources.clone();
    let span = unit.code().declarations().functions()[0].signature_span;
    assert!(expected.len() > 1);
    let bytes = codec::encode(
        unit,
        m::Source {
            path: "counter.idl.json".into(),
            text: include_str!("fixtures/component_counter.idl.json").into(),
        },
        None,
        Default::default(),
    )
    .unwrap();
    let loaded = codec::decode(&bytes, Default::default()).unwrap();
    let declarations = loaded.unit().code().declarations();
    let sources = &declarations.types().table().sources;
    assert_eq!(&sources[..expected.len()], &expected);
    assert_eq!(declarations.functions()[0].signature_span, span);
    assert_eq!(
        loaded.unit().document().document().source.0 as usize,
        expected.len()
    );
    let source = &sources[span.source.0 as usize];
    assert!(source.path.ends_with("inner.lang"));
    assert!(source.text[span.start as usize..span.end as usize].contains("pub fn apply"));
}

#[test]
fn metadata_preflight_admits_name_and_version_comparisons_before_scanning() {
    use lang_runtime::component::negotiation::{
        PrereleaseIdentifier, SemanticVersion, VersionRequirement,
    };
    let loaded = codec::decode(&reference(), Default::default()).unwrap();
    let sources = &loaded.unit().code().declarations().types().table().sources;
    let original = loaded.unit().document().document();
    let invalid_name = format!("{}!", "a".repeat(65_536));
    let mut cases = Vec::new();
    let mut document = original.clone();
    document.package.value = PackageKey::new(invalid_name.clone(), "artifact");
    cases.push(("package", document));
    let mut document = original.clone();
    document.interfaces[0].functions[0].capability.value = invalid_name;
    cases.push(("capability", document));
    let mut document = original.clone();
    let version = SemanticVersion::new(1, 0, 0)
        .with_prerelease(vec![PrereleaseIdentifier::AlphaNumeric("a".repeat(65_536))]);
    document.imports[0].requirement.value = VersionRequirement::Interval {
        min_inclusive: version.clone(),
        max_exclusive: version,
        allow_prerelease: true,
    };
    cases.push(("interval", document));
    for (case, document) in cases {
        let mut session = wire::Session::new(wire::Limits {
            work: 10_000,
            ..Default::default()
        });
        let error = wire::artifact::prepare(
            document,
            loaded.unit().mappings().clone(),
            sources,
            &mut session,
            65_536,
        )
        .err()
        .expect("work admission must precede malformed metadata inspection");
        assert!(
            matches!(error.kind, wire::ErrorKind::Budget(_)),
            "{case}: {error:?}"
        );
        assert!(session.usage().work <= 10_000);
    }
}

#[test]
fn retained_metadata_depth_and_snapshot_byte_caps_cannot_be_bypassed_by_an_old_proof() {
    let bytes = reference();
    let mut limits = codec::Limits::default();
    limits.metadata.depth = 1;
    assert!(codec::decode(&bytes, limits).is_err());
    let loaded = codec::decode(&bytes, Default::default()).unwrap();
    assert!(
        codec::encode(
            loaded.into_unit(),
            m::Source {
                path: "reference.idl.json".into(),
                text: "{}".into()
            },
            None,
            limits
        )
        .is_err()
    );
    let mut limits = codec::Limits::default();
    limits.metadata.metadata.source_bytes = 1;
    assert!(codec::decode(&bytes, limits).is_err());
    let mut limits = codec::Limits::default();
    limits.bindings.bindings = 2;
    assert!(codec::decode(&bytes, limits).is_err());
    let limits = codec::Limits {
        bytes: bytes.len() - 1,
        ..Default::default()
    };
    assert!(codec::decode(&bytes, limits).is_err());
}
