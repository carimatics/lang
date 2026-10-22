use lang_mir::unit as m;
use lang_runtime::component::idl;

const COUNTER: &str = include_str!("fixtures/component_counter.idl.json");
fn counter() -> serde_json::Value {
    serde_json::from_str(COUNTER).unwrap()
}
fn check(value: &serde_json::Value) -> Result<idl::CheckedDocument, idl::Error> {
    idl::check(
        m::SourceId(7),
        &serde_json::to_vec(value).unwrap(),
        Default::default(),
    )
}
fn rejected(value: serde_json::Value, code: idl::Code) -> idl::Error {
    let error = check(&value).expect_err("invalid IDL");
    assert_eq!(error.code, code, "{error:?}");
    assert_eq!(error.span.source, m::SourceId(7));
    error
}

#[test]
fn counter_interface_checks_with_original_source_identity_and_no_execution_authority() {
    let checked = idl::check(m::SourceId(7), COUNTER.as_bytes(), Default::default()).unwrap();
    assert_eq!(checked.document().package.value.namespace(), "demo");
    assert_eq!(checked.document().package.value.package(), "counter");
    assert_eq!(checked.document().version.value.to_string(), "1.0.0");
    assert_eq!(checked.document().interfaces.len(), 1);
    assert_eq!(checked.document().interfaces[0].functions.len(), 1);
    assert_eq!(
        checked.document().interfaces[0].key.span.source,
        m::SourceId(7)
    );
    assert!(checked.usage().budget.work > 0);
}

#[test]
fn rich_values_forward_aliases_and_both_resource_access_modes_are_checked() {
    use serde_json::json;
    let mut value = counter();
    value["interfaces"][0]["types"] = json!([
        {"name":"reply","variant":[{"name":"ok","type":"request"},{"name":"none","type":"unit"},{"name":"owned","type":{"list":{"own":"counter"}}}]},
        {"name":"request","record":[{"name":"delta","type":"i64"},{"name":"flag","type":"bool"},{"name":"body","type":"body"}]},
        {"name":"body","alias":{"option":{"list":"bytes"}}}
    ]);
    value["interfaces"][0]["functions"][0]["result"] =
        json!({"result":{"ok":"reply","err":"text"}});
    value["interfaces"][0]["functions"][0]["params"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"reader","type":{"borrow":"counter"}}));
    let checked = check(&value).unwrap();
    let doc = checked.document();
    assert_eq!(doc.interfaces[0].definitions[0].key.value.name, "body");
    let idl::DefinitionKind::Record(fields) = &doc.interfaces[0].definitions[2].kind else {
        panic!("request")
    };
    assert_eq!(
        fields
            .iter()
            .map(|f| f.name.value.as_str())
            .collect::<Vec<_>>(),
        ["delta", "flag", "body"]
    );
    let params = &doc.interfaces[0].functions[0].params;
    assert!(matches!(
        doc.type_nodes[params[0].ty.0].kind,
        idl::TypeKind::Borrow {
            access: idl::Access::Exclusive,
            ..
        }
    ));
    assert!(matches!(
        doc.type_nodes[params[2].ty.0].kind,
        idl::TypeKind::Borrow {
            access: idl::Access::Shared,
            ..
        }
    ));
}

#[test]
fn interface_projection_and_semver_preserve_exact_and_interval_policy() {
    use serde_json::json;
    let mut value = counter();
    let mut host = value["interfaces"][0].clone();
    host["key"] = json!("demo:host/log");
    value["interfaces"].as_array_mut().unwrap().insert(0, host);
    value["imports"] = json!([{"interface":"demo:host/log","version":{"exact":"1.2.3-99999999999999999999999+build.001"},"members":["apply"]}]);
    let checked = check(&value).unwrap();
    assert_eq!(
        checked.document().interfaces[0]
            .key
            .value
            .package()
            .package(),
        "counter"
    );
    assert_eq!(
        checked.document().imports[0].requirement.value.to_string(),
        "=1.2.3-99999999999999999999999+build.001"
    );
    value["imports"][0]["version"] =
        json!({"interval":{"min":"1.0.0","max":"2.0.0","prerelease":false}});
    check(&value).unwrap();
    value["imports"][0]["version"]["interval"]["max"] = json!("1.0.0+other");
    rejected(value, idl::Code::Version);
}

#[test]
fn names_references_and_duplicate_declarations_keep_related_spans() {
    use serde_json::json;
    for replacement in ["Missing", "missing", "demo:other/api.absent"] {
        let mut v = counter();
        v["interfaces"][0]["types"][0]["record"][0]["type"] = json!(replacement);
        rejected(v, idl::Code::Name);
    }
    let mut v = counter();
    v["interfaces"][0]["resources"] = json!(["counter", "counter"]);
    assert!(rejected(v, idl::Code::Name).related.is_some());
    let mut v = counter();
    let duplicate = v["interfaces"][0]["functions"][0].clone();
    v["interfaces"][0]["functions"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(rejected(v, idl::Code::Name).related.is_some());
    let mut v = counter();
    v["interfaces"][0]["types"][0]["record"][1]["name"] = json!("delta");
    assert!(rejected(v, idl::Code::Name).related.is_some());
}

#[test]
fn unused_cycles_and_borrowed_aggregate_or_result_are_not_hidden() {
    use serde_json::json;
    let invalid = include_bytes!("fixtures/component_invalid_cycle.idl.json");
    assert_eq!(
        idl::check(m::SourceId(9), invalid, Default::default())
            .unwrap_err()
            .reason,
        idl::Reason::CyclicType
    );
    let mut v = counter();
    v["interfaces"][0]["types"].as_array_mut().unwrap().extend([
        json!({"name":"a","alias":{"list":"b"}}),
        json!({"name":"b","alias":{"option":"a"}}),
    ]);
    assert_eq!(rejected(v, idl::Code::Type).reason, idl::Reason::CyclicType);
    for ty in [
        json!({"borrow":"counter"}),
        json!({"option":{"borrow-mut":"counter"}}),
    ] {
        let mut v = counter();
        v["interfaces"][0]["types"][0]["record"][0]["type"] = ty;
        assert_eq!(
            rejected(v, idl::Code::Type).reason,
            idl::Reason::InvalidBorrow
        );
    }
    let mut v = counter();
    v["interfaces"][0]["functions"][0]["result"] = json!({"borrow":"counter"});
    rejected(v, idl::Code::Type);
}

#[test]
fn task_signatures_require_send_and_native_imports_remain_synchronous() {
    use serde_json::json;
    let mut v = counter();
    v["interfaces"][0]["functions"][0]["effect"] = json!("task");
    assert!(rejected(v.clone(), idl::Code::Type).related.is_some());
    v["interfaces"][0]["functions"][0]["params"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    check(&v).unwrap();
    v["imports"] =
        json!([{"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["apply"]}]);
    rejected(v, idl::Code::Type);
    let mut v = counter();
    v["interfaces"][0]["functions"][0]["effect"] = json!("async");
    rejected(v, idl::Code::Type);
}

#[test]
fn projection_duplicates_and_unknown_members_or_exports_are_refused() {
    use serde_json::json;
    let import =
        json!({"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["absent"]});
    let mut v = counter();
    v["imports"] = json!([import]);
    rejected(v, idl::Code::Projection);
    let mut v = counter();
    v["exports"] = json!(["demo:counter/absent"]);
    rejected(v, idl::Code::Projection);
    let mut v = counter();
    v["exports"] = json!(["demo:counter/api", "demo:counter/api"]);
    assert!(rejected(v, idl::Code::Projection).related.is_some());
    let mut v = counter();
    v["imports"] = json!([{"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["apply","apply"]}]);
    assert!(rejected(v, idl::Code::Projection).related.is_some());
}

#[test]
fn json_shape_and_version_errors_have_stable_codes() {
    use serde_json::json;
    let mut v = counter();
    v["grant"] = json!(true);
    rejected(v, idl::Code::Field);
    let mut v = counter();
    v.as_object_mut().unwrap().remove("exports");
    rejected(v, idl::Code::Field);
    let mut v = counter();
    v["idl"] = json!(2);
    rejected(v, idl::Code::Version);
    let v = COUNTER.replacen("\"idl\": 1", "\"idl\": 1.0", 1);
    assert_eq!(
        idl::check(m::SourceId(0), v.as_bytes(), Default::default())
            .err()
            .unwrap()
            .code,
        idl::Code::Version
    );
    for version in [
        "01.0.0",
        "1.0",
        "1.2.3.4",
        "1.0.0-01",
        "1.0.0+bad..tag",
        "18446744073709551616.0.0",
    ] {
        let mut v = counter();
        v["version"] = json!(version);
        rejected(v, idl::Code::Version);
    }
    for bytes in [
        b"\xef\xbb\xbf{}".as_slice(),
        b"\xff",
        br#"{"idl":1,}"#,
        br#"{"x":"\uD800"}"#,
        br#"{"x":"\uDC00"}"#,
    ] {
        assert_eq!(
            idl::check(m::SourceId(0), bytes, Default::default())
                .err()
                .unwrap()
                .code,
            idl::Code::Syntax
        );
    }
}

#[test]
fn every_idl_quota_accepts_exact_and_refuses_one_below() {
    let baseline = idl::check(m::SourceId(7), COUNTER.as_bytes(), Default::default())
        .unwrap()
        .usage();
    for kind in 0..7 {
        let mut exact = idl::Limits::default();
        match kind {
            0 => exact.source_bytes = COUNTER.len(),
            1 => exact.depth = 9,
            2 => exact.declarations = baseline.declarations,
            3 => exact.elements = baseline.elements,
            4 => exact.string_bytes = "demo.counter.apply".len(),
            5 => exact.work = baseline.budget.work,
            6 => exact.allocation_bytes = baseline.budget.allocation_bytes,
            _ => unreachable!(),
        };
        idl::check(m::SourceId(7), COUNTER.as_bytes(), exact).unwrap();
        match kind {
            0 => exact.source_bytes -= 1,
            1 => exact.depth -= 1,
            2 => exact.declarations -= 1,
            3 => exact.elements -= 1,
            4 => exact.string_bytes -= 1,
            5 => exact.work -= 1,
            6 => exact.allocation_bytes -= 1,
            _ => unreachable!(),
        };
        let error = idl::check(m::SourceId(7), COUNTER.as_bytes(), exact)
            .err()
            .unwrap();
        assert_eq!(error.code, idl::Code::Resource, "quota {kind}: {error:?}");
    }
}

#[test]
fn deep_valid_type_syntax_and_cleanup_remain_iterative() {
    let value = counter();
    let mut source = serde_json::to_string(&value).unwrap();
    source = source.replacen(
        "\"i64\"",
        &format!("{}\"i64\"{}", "{\"list\":".repeat(2048), "}".repeat(2048)),
        1,
    );
    let limits = idl::Limits {
        depth: 4096,
        allocation_bytes: 64 * 1024 * 1024,
        work: 5_000_000,
        ..Default::default()
    };
    let checked = idl::check(m::SourceId(7), source.as_bytes(), limits).unwrap();
    assert!(checked.document().type_nodes.len() > 2048);
}

#[test]
fn long_forward_alias_chains_do_not_consume_native_stack_or_data_depth() {
    use serde_json::json;
    let mut value = counter();
    for i in 0..2000 {
        value["interfaces"][0]["types"].as_array_mut().unwrap().push(json!({"name":format!("a{i:04}"),"alias":if i==1999{"unit".to_owned()}else{format!("a{:04}",i+1)}}));
    }
    let source = serde_json::to_vec(&value).unwrap();
    let checked = idl::check(
        m::SourceId(7),
        &source,
        idl::Limits {
            work: 10_000_000,
            allocation_bytes: 64 * 1024 * 1024,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(checked.document().interfaces[0].definitions.len(), 2001);
}

#[test]
fn duplicate_scopes_and_unused_borrow_aliases_cannot_bypass_validation() {
    use serde_json::json;
    let mut value = counter();
    let duplicate = value["interfaces"][0].clone();
    value["interfaces"].as_array_mut().unwrap().push(duplicate);
    assert!(rejected(value, idl::Code::Name).related.is_some());
    let mut value = counter();
    value["interfaces"][0]["functions"][0]["params"][1]["name"] = json!("state");
    assert!(rejected(value, idl::Code::Name).related.is_some());
    let mut value = counter();
    value["interfaces"][0]["types"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"unused","alias":{"borrow":"counter"}}));
    assert_eq!(
        rejected(value, idl::Code::Type).reason,
        idl::Reason::InvalidBorrow
    );
    let mut value = counter();
    value["interfaces"][0]["functions"][0]["params"][0]["type"] = json!({"borrow":"absent"});
    assert_eq!(
        rejected(value, idl::Code::Name).reason,
        idl::Reason::Unresolved
    );
}

#[test]
fn task_result_checks_all_sum_arms_and_import_shapes_are_closed() {
    use serde_json::json;
    let mut value = counter();
    value["interfaces"][0]["functions"][0]["params"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    value["interfaces"][0]["functions"][0]["effect"] = json!("task");
    value["interfaces"][0]["functions"][0]["result"] =
        json!({"result":{"ok":"unit","err":{"option":{"own":"counter"}}}});
    assert_eq!(
        rejected(value, idl::Code::Type).reason,
        idl::Reason::InvalidEffect
    );
    for version in [
        json!({"exact":"1.0.0","interval":{"min":"1.0.0","max":"2.0.0","prerelease":false}}),
        json!({"interval":{"min":"1.0.0","max":"2.0.0","prerelease":null}}),
    ] {
        let mut value = counter();
        value["imports"] =
            json!([{"interface":"demo:counter/api","version":version,"members":["apply"]}]);
        assert!(check(&value).is_err());
    }
    let mut value = counter();
    value["interfaces"][0]["types"][0] = json!({"name":"request","variant":[]});
    rejected(value, idl::Code::Type);
}
