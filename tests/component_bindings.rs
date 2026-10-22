use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::component::{
    bindings as b, idl,
    negotiation::{InterfaceKey, PackageKey, ResourceTypeKey},
};
use lang_semantics::analysis::Program;
use std::path::Path;

const SOURCE: &str = r#"
pub struct CounterKind {}
pub struct Request {delta:i64,label:core::Text}
pub fn apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text> {
    return core::Result::Ok(request.delta);
}
"#;
fn code(text: &str) -> b::Code {
    let out = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("component-binding.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let Program::Ownership(program) = out.result.unwrap() else {
        panic!("ownership proof")
    };
    b::Code::Owned((*program).into_failure_flow())
}
fn document() -> idl::CheckedDocument {
    idl::check(
        m::SourceId(77),
        include_bytes!("fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap()
}
fn mapping() -> b::Map {
    let interface = InterfaceKey::new(PackageKey::new("demo", "counter"), "api");
    b::Map {
        version: 1,
        resources: vec![b::ResourceBinding {
            tag: m::DefinitionId(0),
            resource: ResourceTypeKey::new(interface.clone(), "counter"),
        }],
        imports: vec![],
        exports: vec![b::ExportBinding {
            source: m::FunctionId(0),
            member: b::MemberKey {
                interface,
                member: "apply".into(),
            },
        }],
    }
}

#[test]
fn counter_contract_independently_matches_resource_record_result_and_guest_export() {
    let unit = b::Unit::new(document(), code(SOURCE), mapping(), Default::default()).unwrap();
    assert_eq!(
        unit.resource(m::DefinitionId(0)),
        Some(b::Resource {
            interface: 0,
            resource: 0
        })
    );
    assert_eq!(
        unit.definition(m::DefinitionId(1)),
        Some(b::Definition {
            interface: 0,
            definition: 0
        })
    );
    assert!(unit.imports().is_empty());
    assert_eq!(
        unit.exports(),
        &[(
            m::FunctionId(0),
            b::Function {
                interface: 0,
                member: 0
            }
        )]
    );
    assert_eq!(unit.document().document().source, m::SourceId(77));
    assert_eq!(unit.mappings().version, 1);
    assert!(unit.usage().work > 0);
}

#[test]
fn invalid_unused_mapping_entries_cannot_turn_metadata_into_a_binding() {
    let mut cases = Vec::new();
    let mut map = mapping();
    map.version = 2;
    cases.push((map, b::ErrorKind::Version(2)));
    let mut map = mapping();
    map.resources.clear();
    cases.push((map, b::ErrorKind::Missing));
    let mut map = mapping();
    map.exports.clear();
    cases.push((map, b::ErrorKind::Missing));
    let mut map = mapping();
    map.resources[0].tag = m::DefinitionId(u32::MAX);
    cases.push((map, b::ErrorKind::InvalidSource));
    let mut map = mapping();
    map.resources.push(map.resources[0].clone());
    cases.push((map, b::ErrorKind::Duplicate));
    let mut map = mapping();
    map.exports.push(map.exports[0].clone());
    cases.push((map, b::ErrorKind::Duplicate));
    let mut map = mapping();
    map.exports[0].member.member = "missing".into();
    cases.push((map, b::ErrorKind::UnknownMember));
    for (map, kind) in cases {
        assert_eq!(
            b::Unit::new(document(), code(SOURCE), map, Default::default())
                .unwrap_err()
                .kind,
            kind
        );
    }
}

#[test]
fn source_binding_names_are_resolved_before_independent_mir_comparison() {
    use frontend::component as c;
    let raw = mapping();
    let input = c::Input {
        version: 1,
        resources: vec![c::Resource {
            tag: "CounterKind".into(),
            resource: raw.resources[0].resource.clone(),
        }],
        imports: vec![],
        exports: vec![c::Export {
            source: "apply".into(),
            member: raw.exports[0].member.clone(),
        }],
    };
    let mut sources = SourceMap::new();
    let old = sources.add("component-binding.lang", SOURCE).unwrap();
    let out = c::analyze(c::Request {
        sources,
        display: Path::new("component-binding.lang"),
        bytes: SOURCE.as_bytes(),
        root: None,
        compile: Default::default(),
        document: document(),
        bindings: input,
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let unit = out.result.unwrap();
    assert_eq!(unit.exports()[0].0, m::FunctionId(0));
    assert_eq!(
        out.code_sources.len(),
        unit.code().declarations().types().table().sources.len()
    );
    assert_ne!(
        out.code_sources[0], old,
        "the current source identity must not be replaced by an identical old snapshot"
    );
    assert!(unit.usage().work > 0);
}

fn echo_document(
    ty: serde_json::Value,
    definitions: Vec<serde_json::Value>,
    effect: &str,
) -> idl::CheckedDocument {
    let json = serde_json::json!({"idl":1,"package":"demo:echo","version":"1.0.0","interfaces":[{"key":"demo:echo/api","resources":[],"types":definitions,"functions":[{"name":"echo","effect":effect,"params":[{"name":"external-name","type":ty.clone()}],"result":ty,"capability":"demo.echo.call"}]}],"imports":[],"exports":["demo:echo/api"]});
    idl::check(
        m::SourceId(77),
        &serde_json::to_vec(&json).unwrap(),
        Default::default(),
    )
    .unwrap()
}
fn echo_input() -> frontend::component::Input {
    frontend::component::Input {
        version: 1,
        resources: vec![],
        imports: vec![],
        exports: vec![frontend::component::Export {
            source: "echo".into(),
            member: b::MemberKey {
                interface: InterfaceKey::new(PackageKey::new("demo", "echo"), "api"),
                member: "echo".into(),
            },
        }],
    }
}
fn echo(
    source: &str,
    document: idl::CheckedDocument,
) -> Result<b::Unit, frontend::component::Error> {
    let out = frontend::component::analyze(frontend::component::Request {
        sources: SourceMap::new(),
        display: Path::new("echo.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document,
        bindings: echo_input(),
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    out.result
}

#[test]
fn all_data_families_aliases_variants_and_encoded_names_match_source() {
    use serde_json::json;
    for (ty, definitions, declarations, source_ty) in [
        (json!("i64"), vec![], "", "i64"),
        (json!("bool"), vec![], "", "bool"),
        (json!("unit"), vec![], "", "core::Unit"),
        (json!("text"), vec![], "", "core::Text"),
        (json!("bytes"), vec![], "", "core::Bytes"),
        (
            json!({"list":"bytes"}),
            vec![],
            "",
            "core::Vec<core::Bytes>",
        ),
        (
            json!({"option":"text"}),
            vec![],
            "",
            "core::Option<core::Text>",
        ),
        (
            json!({"result":{"ok":"i64","err":"text"}}),
            vec![],
            "",
            "core::Result<i64,core::Text>",
        ),
        (
            json!("renamed"),
            vec![json!({"name":"renamed","alias":"i64"})],
            "",
            "i64",
        ),
        (
            json!("request"),
            vec![json!({"name":"request","record":[{"name":"ready-state","type":"i64"}]})],
            "pub struct Request{component_72656164792d7374617465:i64}",
            "Request",
        ),
        (
            json!("choice"),
            vec![
                json!({"name":"choice","variant":[{"name":"none","type":"unit"},{"name":"number","type":"i64"}]}),
            ],
            "pub enum Choice{none,number(i64)}",
            "Choice",
        ),
        (
            json!("choice"),
            vec![
                json!({"name":"choice","variant":[{"name":"none","type":"unit"},{"name":"number","type":"i64"}]}),
            ],
            "pub enum Choice{none(core::Unit),number(i64)}",
            "Choice",
        ),
    ] {
        let source =
            format!("{declarations} pub fn echo(value:{source_ty})->{source_ty}{{return value;}}");
        echo(&source, echo_document(ty, definitions, "sync")).unwrap();
    }
}

#[test]
fn task_export_effect_is_checked_without_erasing_the_task_proof() {
    use serde_json::json;
    let source = "pub async fn echo(value:i64)->i64{return value;}";
    let unit = echo(source, echo_document(json!("i64"), vec![], "task")).unwrap();
    assert!(matches!(unit.code(), b::Code::Tasks(_)));
    let error = echo(source, echo_document(json!("i64"), vec![], "sync")).unwrap_err();
    assert!(matches!(
        error,
        frontend::component::Error::Bindings(b::Error {
            kind: b::ErrorKind::Effect,
            ..
        })
    ));
    let b::Code::Tasks(proof) = unit.code() else {
        panic!("task")
    };
    let error = b::Unit::new(
        echo_document(json!("i64"), vec![], "task"),
        b::Code::Owned(proof.plan().clone()),
        unit.mappings().clone(),
        Default::default(),
    )
    .unwrap_err();
    assert_eq!(error.kind, b::ErrorKind::Effect);
}

#[test]
fn independent_limits_admit_exact_usage_and_refuse_every_one_below_boundary() {
    let unit = b::Unit::new(document(), code(SOURCE), mapping(), Default::default()).unwrap();
    let usage = unit.usage();
    let depth = document().usage().data_depth;
    let exact = b::Limits {
        bindings: 2,
        depth,
        budget: m::Limits {
            work: usage.work,
            allocations: usage.allocation_bytes,
            elements: usage.elements,
            ..Default::default()
        },
    };
    assert_eq!(
        b::Unit::new(document(), code(SOURCE), mapping(), exact)
            .unwrap()
            .usage(),
        usage
    );
    for limits in [
        b::Limits {
            bindings: 1,
            ..exact
        },
        b::Limits {
            depth: depth - 1,
            ..exact
        },
        b::Limits {
            budget: m::Limits {
                work: usage.work - 1,
                ..exact.budget
            },
            ..exact
        },
        b::Limits {
            budget: m::Limits {
                allocations: usage.allocation_bytes - 1,
                ..exact.budget
            },
            ..exact
        },
        b::Limits {
            budget: m::Limits {
                elements: usage.elements - 1,
                ..exact.budget
            },
            ..exact
        },
    ] {
        assert!(b::Unit::new(document(), code(SOURCE), mapping(), limits).is_err());
    }
    let extra = m::Usage {
        work: 1,
        allocation_bytes: 0,
        elements: 0,
    };
    assert!(matches!(
        b::Unit::with_usage(document(), code(SOURCE), mapping(), exact, extra)
            .unwrap_err()
            .kind,
        b::ErrorKind::Budget(_)
    ));
}

fn raw_code(table: lang_mir::unit::ownership::bodies::BodyTable) -> b::Code {
    use lang_mir::unit::ownership as o;
    let typed = table.verify(Default::default()).unwrap();
    let moved = o::moves::analyze(typed, Default::default()).unwrap();
    let activity = o::loan_activity::analyze(moved, Default::default()).unwrap();
    let loaned = o::loans::analyze(activity, Default::default()).unwrap();
    b::Code::Owned(o::failure::elaborate(loaned, Default::default()).unwrap())
}

#[test]
fn forged_mir_field_types_and_public_flags_are_checked_without_hir() {
    use serde_json::json;
    let unit = echo(
        "pub struct Request{value:i64}pub fn echo(value:Request)->Request{return value;}",
        echo_document(
            json!("request"),
            vec![json!({"name":"request","record":[{"name":"value","type":"i64"}]})],
            "sync",
        ),
    )
    .unwrap();
    let b::Code::Owned(flow) = unit.code() else {
        panic!("owned")
    };
    let raw = flow
        .source()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    let mut table = raw.clone();
    let lang_mir::unit::ownership::DefinitionKind::Record(fields) =
        &mut table.places.declarations.types.definitions[0].kind
    else {
        panic!("record")
    };
    let span = fields[0].ty.span;
    fields[0].ty.ty = lang_mir::unit::ownership::Type::Bool;
    let error = b::Unit::new(
        echo_document(
            json!("request"),
            vec![json!({"name":"request","record":[{"name":"value","type":"i64"}]})],
            "sync",
        ),
        raw_code(table),
        unit.mappings().clone(),
        Default::default(),
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        b::ErrorKind::Type {
            actual: lang_mir::unit::ownership::Type::Bool
        }
    );
    assert_eq!(error.source, Some(span));
    assert_eq!(error.contract.unwrap().source, m::SourceId(77));
    let mut table = raw;
    table.places.declarations.functions[0].public = false;
    assert_eq!(
        b::Unit::new(
            echo_document(
                json!("request"),
                vec![json!({"name":"request","record":[{"name":"value","type":"i64"}]})],
                "sync"
            ),
            raw_code(table),
            unit.mappings().clone(),
            Default::default()
        )
        .unwrap_err()
        .kind,
        b::ErrorKind::Abi
    );
}

fn imports_document() -> idl::CheckedDocument {
    let function = |name: &str, capability: &str| serde_json::json!({"name":name,"effect":"sync","params":[{"name":"needle","type":"i64"}],"result":"i64","capability":capability});
    let json = serde_json::json!({"idl":1,"package":"demo:consumer","version":"1.0.0","interfaces":[
        {"key":"demo:consumer/api","resources":[],"types":[],"functions":[function("query","demo.consumer.query")]},
        {"key":"demo:host/api","resources":[],"types":[],"functions":[function("lookup","demo.host.lookup")]}
    ],"imports":[{"interface":"demo:host/api","version":{"exact":"1.0.0"},"members":["lookup"]}],"exports":["demo:consumer/api"]});
    idl::check(
        m::SourceId(77),
        &serde_json::to_vec(&json).unwrap(),
        Default::default(),
    )
    .unwrap()
}
fn imports_input() -> frontend::component::Input {
    use frontend::component as c;
    c::Input {
        version: 1,
        resources: vec![],
        imports: vec![c::Import {
            source: "read".into(),
            member: b::MemberKey {
                interface: InterfaceKey::new(PackageKey::new("demo", "host"), "api"),
                member: "lookup".into(),
            },
        }],
        exports: vec![c::Export {
            source: "run".into(),
            member: b::MemberKey {
                interface: InterfaceKey::new(PackageKey::new("demo", "consumer"), "api"),
                member: "query".into(),
            },
        }],
    }
}
fn imported(
    source: &str,
    input: frontend::component::Input,
) -> Result<b::Unit, frontend::component::Error> {
    let out = frontend::component::analyze(frontend::component::Request {
        sources: SourceMap::new(),
        display: Path::new("imported.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: imports_document(),
        bindings: input,
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    out.result
}

#[test]
fn source_imports_match_required_projection_even_when_unused_and_never_substitute_host_or_c() {
    for expression in ["value", "read(value)"] {
        let source = format!(
            "extern \"component\" fn read(value:i64)->i64;pub fn run(value:i64)->i64{{return {expression};}}"
        );
        let unit = imported(&source, imports_input()).unwrap();
        assert_eq!(
            unit.imports(),
            &[b::Function {
                interface: 1,
                member: 0
            }]
        );
        let mut input = imports_input();
        input.imports.clear();
        assert!(matches!(
            imported(&source, input),
            Err(frontend::component::Error::Bindings(b::Error {
                kind: b::ErrorKind::Missing,
                ..
            }))
        ));
    }
    for marker in ["", "\"C\" "] {
        let source = format!(
            "extern {marker}fn read(value:i64)->i64;pub fn run(value:i64)->i64{{return value;}}"
        );
        assert!(matches!(
            imported(&source, imports_input()),
            Err(frontend::component::Error::Bindings(b::Error {
                kind: b::ErrorKind::Abi,
                ..
            }))
        ));
    }
}

#[test]
fn distinct_idl_nominals_cannot_share_a_source_definition_or_split_one_identity() {
    use serde_json::json;
    for (definitions, params, source_defs, source_params) in [
        (
            vec![
                json!({"name":"left","record":[{"name":"value","type":"i64"}]}),
                json!({"name":"right","record":[{"name":"value","type":"i64"}]}),
            ],
            vec![
                json!({"name":"a","type":"left"}),
                json!({"name":"b","type":"right"}),
            ],
            "pub struct Same{value:i64}",
            "a:Same,b:Same",
        ),
        (
            vec![json!({"name":"shared","record":[{"name":"value","type":"i64"}]})],
            vec![
                json!({"name":"a","type":"shared"}),
                json!({"name":"b","type":"shared"}),
            ],
            "pub struct Left{value:i64}pub struct Right{value:i64}",
            "a:Left,b:Right",
        ),
    ] {
        let json = json!({"idl":1,"package":"demo:echo","version":"1.0.0","interfaces":[{"key":"demo:echo/api","resources":[],"types":definitions,"functions":[{"name":"echo","effect":"sync","params":params,"result":"i64","capability":"demo.echo.call"}]}],"imports":[],"exports":["demo:echo/api"]});
        let document = idl::check(
            m::SourceId(77),
            &serde_json::to_vec(&json).unwrap(),
            Default::default(),
        )
        .unwrap();
        let source = format!("{source_defs}pub fn echo({source_params})->i64{{return 0;}}");
        assert!(matches!(
            echo(&source, document),
            Err(frontend::component::Error::Bindings(b::Error {
                kind: b::ErrorKind::Nominal,
                ..
            }))
        ));
    }
}

fn resource_document(
    parameter: serde_json::Value,
    result: serde_json::Value,
) -> idl::CheckedDocument {
    let json = serde_json::json!({"idl":1,"package":"demo:echo","version":"1.0.0","interfaces":[{"key":"demo:echo/api","resources":["counter"],"types":[],"functions":[{"name":"echo","effect":"sync","params":[{"name":"value","type":parameter}],"result":result,"capability":"demo.echo.call"}]}],"imports":[],"exports":["demo:echo/api"]});
    idl::check(
        m::SourceId(77),
        &serde_json::to_vec(&json).unwrap(),
        Default::default(),
    )
    .unwrap()
}

#[test]
fn resource_keys_own_containers_and_both_loan_modes_are_nominal() {
    use frontend::component as c;
    use serde_json::json;
    for (parameter, result, source_type, result_type, body) in [
        (
            json!({"own":"counter"}),
            json!({"own":"counter"}),
            "core::ComponentResource<Tag>",
            "core::ComponentResource<Tag>",
            "return value;",
        ),
        (
            json!({"list":{"own":"counter"}}),
            json!({"list":{"own":"counter"}}),
            "core::Vec<core::ComponentResource<Tag>>",
            "core::Vec<core::ComponentResource<Tag>>",
            "return value;",
        ),
        (
            json!({"borrow":"counter"}),
            json!("i64"),
            "&core::ComponentResource<Tag>",
            "i64",
            "return 0;",
        ),
        (
            json!({"borrow-mut":"counter"}),
            json!("i64"),
            "&mut core::ComponentResource<Tag>",
            "i64",
            "return 0;",
        ),
    ] {
        let mut input = echo_input();
        input.resources.push(c::Resource {
            tag: "Tag".into(),
            resource: ResourceTypeKey::new(
                InterfaceKey::new(PackageKey::new("demo", "echo"), "api"),
                "counter",
            ),
        });
        let source =
            format!("pub struct Tag{{}}pub fn echo(value:{source_type})->{result_type}{{{body}}}");
        let out = c::analyze(c::Request {
            sources: SourceMap::new(),
            display: Path::new("resource-binding.lang"),
            bytes: source.as_bytes(),
            root: None,
            compile: Default::default(),
            document: resource_document(parameter.clone(), result.clone()),
            bindings: input,
            limits: Default::default(),
        });
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        out.result.unwrap();
    }
    let source =
        "pub struct Tag{}pub fn echo(value:&mut core::ComponentResource<Tag>)->i64{return 0;}";
    let mut input = echo_input();
    input.resources.push(c::Resource {
        tag: "Tag".into(),
        resource: ResourceTypeKey::new(
            InterfaceKey::new(PackageKey::new("demo", "echo"), "api"),
            "counter",
        ),
    });
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display: Path::new("wrong-loan.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: resource_document(json!({"borrow":"counter"}), json!("i64")),
        bindings: input,
        limits: Default::default(),
    });
    assert!(matches!(
        out.result,
        Err(c::Error::Bindings(b::Error {
            kind: b::ErrorKind::Type { .. },
            ..
        }))
    ));
}

#[test]
fn deep_data_matching_uses_explicit_stacks_and_the_retained_depth_profile() {
    use serde_json::json;
    let mut ty = json!("i64");
    let mut source_ty = "i64".to_string();
    for _ in 0..40 {
        ty = json!({"option":ty});
        source_ty = format!("core::Option<{source_ty}>");
    }
    let document = echo_document(ty, vec![], "sync");
    let depth = document.usage().data_depth;
    let source = format!("pub fn echo(value:{source_ty})->{source_ty}{{return value;}}");
    let unit = echo(&source, document).unwrap();
    assert!(depth >= 40);
    assert!(unit.usage().work > 0);
}

fn counter_input() -> frontend::component::Input {
    let raw = mapping();
    frontend::component::Input {
        version: 1,
        resources: vec![frontend::component::Resource {
            tag: "CounterKind".into(),
            resource: raw.resources[0].resource.clone(),
        }],
        imports: vec![],
        exports: vec![frontend::component::Export {
            source: "apply".into(),
            member: raw.exports[0].member.clone(),
        }],
    }
}

#[test]
fn compiler_binding_headers_and_missing_names_fail_before_publication() {
    use frontend::component as c;
    let mut input = counter_input();
    input.version = 2;
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display: Path::new("not-parsed.lang"),
        bytes: b"invalid source",
        root: None,
        compile: Default::default(),
        document: document(),
        bindings: input,
        limits: Default::default(),
    });
    assert!(out.sources.iter().next().is_none());
    assert!(matches!(
        out.result,
        Err(c::Error::Bindings(b::Error {
            kind: b::ErrorKind::Version(2),
            ..
        }))
    ));
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display: Path::new("not-parsed.lang"),
        bytes: b"invalid source",
        root: None,
        compile: Default::default(),
        document: document(),
        bindings: counter_input(),
        limits: b::Limits {
            bindings: 1,
            ..Default::default()
        },
    });
    assert!(out.sources.iter().next().is_none());
    assert!(matches!(
        out.result,
        Err(c::Error::Bindings(b::Error {
            kind: b::ErrorKind::BindingLimit,
            ..
        }))
    ));
    for section in [b::Section::Resources, b::Section::Exports] {
        let mut input = counter_input();
        match section {
            b::Section::Resources => input.resources[0].tag = "missing".into(),
            b::Section::Exports => input.exports[0].source = "missing".into(),
            _ => unreachable!(),
        }
        let out = c::analyze(c::Request {
            sources: SourceMap::new(),
            display: Path::new("names.lang"),
            bytes: SOURCE.as_bytes(),
            root: None,
            compile: Default::default(),
            document: document(),
            bindings: input,
            limits: Default::default(),
        });
        assert!(
            matches!(out.result,Err(c::Error::Name{section:actual,index:0}) if actual==section)
        );
    }
}

#[test]
fn qualified_module_names_retain_the_actual_file_source_identity() {
    use frontend::component as c;
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/component_bindings_modules");
    let display = Path::new("main.lang");
    let mut input = counter_input();
    input.resources[0].tag = "inner::CounterKind".into();
    input.exports[0].source = "inner::apply".into();
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display,
        bytes: include_bytes!("fixtures/component_bindings_modules/main.lang"),
        root: Some(&root),
        compile: Default::default(),
        document: document(),
        bindings: input,
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let unit = out.result.unwrap();
    let declarations = unit.code().declarations();
    assert_eq!(declarations.functions()[0].name, "inner::apply");
    let span = declarations.functions()[0].signature_span;
    let original = out.code_sources[span.source.0 as usize];
    assert!(
        out.sources
            .file(original)
            .unwrap()
            .name()
            .ends_with("inner.lang")
    );
    assert!(
        out.sources.file(original).unwrap().text()[span.start as usize..span.end as usize]
            .contains("pub fn apply")
    );
}
