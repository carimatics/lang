use lang_mir::unit as m;
use lang_runtime::component::{
    CapabilityGrants, idl,
    serialization::{ComponentContractLock, DigestAlgorithm, VersionedDigest},
    wire::{self, Provider, SelectionError, SelectionErrorKind as K},
};

fn fixture() -> serde_json::Value {
    serde_json::from_slice(include_bytes!(
        "fixtures/component_generated_script.idl.json"
    ))
    .unwrap()
}
fn contract(value: &serde_json::Value) -> wire::Contract {
    let doc = idl::check(
        m::SourceId(71),
        &serde_json::to_vec(value).unwrap(),
        Default::default(),
    )
    .unwrap();
    wire::Contract::new(doc, vec![], &mut wire::Session::new(Default::default())).unwrap()
}
fn consumer_json() -> serde_json::Value {
    let mut json = fixture();
    json["package"] = serde_json::json!("demo:consumer");
    json["exports"] = serde_json::json!([]);
    json["imports"] = serde_json::json!([{"interface":"demo:script/api","version":{"interval":{"min":"1.0.0","max":"2.0.0","prerelease":false}},"members":["echo"]}]);
    json
}
fn consumer() -> wire::Contract {
    contract(&consumer_json())
}
fn provider(version: &str, incompatible: bool) -> wire::Contract {
    let mut json = fixture();
    json["version"] = serde_json::json!(version);
    if incompatible {
        json["interfaces"][0]["types"][0]["record"][0]["type"] = serde_json::json!("bool");
    }
    contract(&json)
}
fn digest(value: u8) -> VersionedDigest {
    VersionedDigest::new(DigestAlgorithm::Sha256, vec![value; 32]).unwrap()
}
fn grants() -> CapabilityGrants {
    let mut grants = CapabilityGrants::new();
    grants.grant("demo.script.echo");
    grants
}
fn kind(error: SelectionError) -> K {
    let SelectionError::Metadata { kind, .. } = error else {
        panic!("unexpected wire failure: {error:?}")
    };
    kind
}

#[test]
fn rich_record_projection_selects_highest_compatible_version_before_any_execution() {
    let consumer = consumer();
    let low = provider("1.1.0", false);
    let high = provider("1.5.0", false);
    let bad = provider("1.9.0", true);
    let a = digest(1);
    let b = digest(5);
    let c = digest(9);
    let providers = [
        Provider {
            contract: &bad,
            digest: &c,
        },
        Provider {
            contract: &low,
            digest: &a,
        },
        Provider {
            contract: &high,
            digest: &b,
        },
    ];
    let mut session = wire::Session::new(Default::default());
    let selected =
        wire::select_provider(&consumer, 0, &providers, &grants(), &mut session).unwrap();
    assert_eq!(selected.provider_index(), 2);
    assert_eq!(selected.provider().digest, &b);
    assert_eq!(
        selected.members(),
        [wire::MemberSelection {
            expected: 0,
            actual: 0
        }]
    );
    assert!(!selected.is_locked());
    assert!(session.usage().work > 0);
}

#[test]
fn missing_capability_precedes_candidate_inventory_limits() {
    let consumer = consumer();
    let provider = provider("1.0.0", false);
    let digest = digest(1);
    let limit = consumer.checked().usage().declarations;
    let entries = vec![
        Provider {
            contract: &provider,
            digest: &digest
        };
        limit + 1
    ];
    let mut session = wire::Session::new(wire::Limits {
        metadata: idl::Limits {
            declarations: limit,
            ..Default::default()
        },
        ..Default::default()
    });
    assert_eq!(
        kind(
            wire::select_provider(
                &consumer,
                0,
                &entries,
                &CapabilityGrants::new(),
                &mut session
            )
            .unwrap_err()
        ),
        K::MissingCapability
    );
}

fn select_kind(consumer: &wire::Contract, providers: &[Provider<'_>]) -> K {
    kind(
        wire::select_provider(
            consumer,
            0,
            providers,
            &grants(),
            &mut wire::Session::new(Default::default()),
        )
        .unwrap_err(),
    )
}
fn locked(version: &wire::Contract, digest: &VersionedDigest) -> wire::Contract {
    let doc = idl::check(
        m::SourceId(71),
        &serde_json::to_vec(&consumer_json()).unwrap(),
        Default::default(),
    )
    .unwrap();
    let key = doc.document().imports[0].interface.value.clone();
    wire::Contract::new(
        doc,
        vec![ComponentContractLock::new(
            key,
            version.document().version.value.clone(),
            digest.clone(),
        )],
        &mut wire::Session::new(Default::default()),
    )
    .unwrap()
}

#[test]
fn exact_locks_pin_identity_and_never_fall_back_after_missing_digest_or_projection() {
    let low = provider("1.1.0", false);
    let high = provider("1.9.0", false);
    let incompatible = provider("1.1.0", true);
    let d = digest(1);
    let other = digest(2);
    let consumer = locked(&low, &d);
    let low_entry = Provider {
        contract: &low,
        digest: &d,
    };
    let high_entry = Provider {
        contract: &high,
        digest: &other,
    };
    let mut session = wire::Session::new(Default::default());
    let plan = wire::select_provider(
        &consumer,
        0,
        &[high_entry, low_entry],
        &grants(),
        &mut session,
    )
    .unwrap();
    assert_eq!(plan.provider().digest, &d);
    assert!(plan.is_locked());
    assert_eq!(
        select_kind(&consumer, &[high_entry]),
        K::LockedProviderUnavailable
    );
    assert_eq!(
        select_kind(
            &consumer,
            &[
                Provider {
                    contract: &low,
                    digest: &other
                },
                high_entry
            ]
        ),
        K::LockDigestMismatch
    );
    assert_eq!(
        select_kind(
            &consumer,
            &[
                Provider {
                    contract: &incompatible,
                    digest: &d
                },
                high_entry
            ]
        ),
        K::LockedProviderIncompatible
    );
    assert_eq!(
        select_kind(&consumer, &[low_entry, high_entry, low_entry]),
        K::DuplicateProvider
    );
}

#[test]
fn exact_versions_are_build_sensitive_and_intervals_refuse_ambiguous_or_duplicate_top_identities() {
    let a = provider("1.5.0+one", false);
    let b = provider("1.5.0+two", false);
    let low = provider("1.1.0", false);
    let da = digest(1);
    let db = digest(2);
    let x = Provider {
        contract: &a,
        digest: &da,
    };
    let y = Provider {
        contract: &b,
        digest: &db,
    };
    let l = Provider {
        contract: &low,
        digest: &da,
    };
    assert_eq!(select_kind(&consumer(), &[x, y]), K::AmbiguousProvider);
    assert_eq!(select_kind(&consumer(), &[x, x, y]), K::DuplicateProvider);
    assert_eq!(
        select_kind(
            &consumer(),
            &[
                x,
                Provider {
                    contract: &a,
                    digest: &db
                }
            ]
        ),
        K::AmbiguousProvider
    );
    assert_eq!(
        select_kind(
            &consumer(),
            &[
                x,
                Provider {
                    contract: &b,
                    digest: &da
                }
            ]
        ),
        K::AmbiguousProvider
    );
    let mut json = consumer_json();
    json["imports"][0]["version"] = serde_json::json!({"exact":"1.5.0+one"});
    let exact = contract(&json);
    let plan = wire::select_provider(
        &exact,
        0,
        &[y, x],
        &grants(),
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    assert_eq!(plan.provider().digest, &da);
    assert_eq!(select_kind(&exact, &[y]), K::NoCompatibleVersion);
    let interval = consumer();
    let plan = wire::select_provider(
        &interval,
        0,
        &[l, l, x],
        &grants(),
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    assert_eq!(plan.provider().digest, &da);
    assert_eq!(
        plan.provider().contract.document().version.value,
        a.document().version.value
    );
}

#[test]
fn prerelease_policy_and_arbitrary_precision_numeric_order_reuse_the_existing_rules() {
    let a = provider(
        "1.2.0-alpha.9999999999999999999999999999999999999999",
        false,
    );
    let b = provider(
        "1.2.0-alpha.10000000000000000000000000000000000000000",
        false,
    );
    let da = digest(1);
    let db = digest(2);
    let entries = [
        Provider {
            contract: &a,
            digest: &da,
        },
        Provider {
            contract: &b,
            digest: &db,
        },
    ];
    assert_eq!(select_kind(&consumer(), &entries), K::PrereleaseRejected);
    let mut json = consumer_json();
    json["imports"][0]["version"]["interval"]["prerelease"] = serde_json::json!(true);
    let allowed = contract(&json);
    let plan = wire::select_provider(
        &allowed,
        0,
        &entries,
        &grants(),
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    assert_eq!(plan.provider().digest, &db);
    json["imports"][0]["version"] =
        serde_json::json!({"exact":"1.2.0-alpha.9999999999999999999999999999999999999999"});
    let exact = contract(&json);
    let plan = wire::select_provider(
        &exact,
        0,
        &entries,
        &grants(),
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    assert_eq!(plan.provider().digest, &da);
}

#[test]
fn aliases_parameter_spelling_and_unrelated_valid_members_do_not_change_projection() {
    let consumer = consumer();
    let mut json = fixture();
    json["interfaces"][0]["types"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name":"renamed","alias":"request"}));
    json["interfaces"][0]["functions"][0]["params"][0] =
        serde_json::json!({"name":"different","type":"renamed"});
    let mut extra = json["interfaces"][0]["functions"][0].clone();
    extra["name"] = serde_json::json!("extra");
    json["interfaces"][0]["functions"]
        .as_array_mut()
        .unwrap()
        .push(extra);
    let provider = contract(&json);
    let d = digest(1);
    let plan = wire::select_provider(
        &consumer,
        0,
        &[Provider {
            contract: &provider,
            digest: &d,
        }],
        &grants(),
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    assert_eq!(plan.members().len(), 1);
}

#[test]
fn mismatches_retain_precise_consumer_provider_spans_and_distinct_reasons() {
    for (change, expected) in [
        ("capability", wire::SelectionMismatch::Capability),
        ("effect", wire::SelectionMismatch::Effect),
        ("field", wire::SelectionMismatch::Signature),
        ("member", wire::SelectionMismatch::MissingMember),
        ("nominal", wire::SelectionMismatch::Signature),
    ] {
        let consumer = consumer();
        let mut json = fixture();
        match change {
            "capability" => {
                json["interfaces"][0]["functions"][0]["capability"] =
                    serde_json::json!("demo.script.other")
            }
            "effect" => json["interfaces"][0]["functions"][0]["effect"] = serde_json::json!("task"),
            "field" => {
                json["interfaces"][0]["types"][0]["record"][0]["name"] = serde_json::json!("other")
            }
            "member" => json["interfaces"][0]["functions"][0]["name"] = serde_json::json!("other"),
            "nominal" => {
                json["interfaces"][0]["types"][0]["name"] = serde_json::json!("reply");
                json["interfaces"][0]["functions"][0]["params"][0]["type"] =
                    serde_json::json!("reply");
                json["interfaces"][0]["functions"][0]["result"] = serde_json::json!("reply");
            }
            _ => unreachable!(),
        }
        let doc = idl::check(
            m::SourceId(72),
            &serde_json::to_vec(&json).unwrap(),
            Default::default(),
        )
        .unwrap();
        let provider =
            wire::Contract::new(doc, vec![], &mut wire::Session::new(Default::default())).unwrap();
        let d = digest(1);
        let SelectionError::Metadata {
            kind, rejections, ..
        } = wire::select_provider(
            &consumer,
            0,
            &[Provider {
                contract: &provider,
                digest: &d,
            }],
            &grants(),
            &mut wire::Session::new(Default::default()),
        )
        .unwrap_err()
        else {
            panic!("structured mismatch")
        };
        assert_eq!(kind, K::StructuralMismatch);
        assert_eq!(rejections.len(), 1);
        assert_eq!(rejections[0].kind, expected);
        assert_eq!(rejections[0].expected.source, m::SourceId(71));
        if let Some(actual) = rejections[0].actual {
            assert_eq!(actual.source, m::SourceId(72));
        } else {
            assert_eq!(expected, wire::SelectionMismatch::MissingMember);
        }
    }
}

#[test]
fn every_quota_is_cumulative_accepts_exact_and_refuses_one_below() {
    let consumer = consumer();
    let a = provider("1.1.0", false);
    let b = provider("1.5.0", false);
    let d = digest(1);
    let providers = [
        Provider {
            contract: &a,
            digest: &d,
        },
        Provider {
            contract: &b,
            digest: &d,
        },
    ];
    let mut s = wire::Session::new(Default::default());
    wire::select_provider(&consumer, 0, &providers, &grants(), &mut s).unwrap();
    let used = s.usage();
    let exact = wire::Limits {
        work: used.work,
        allocation_bytes: used.allocation_bytes,
        elements: used.elements,
        serialization_bytes: used.serialization_bytes,
        ..Default::default()
    };
    wire::select_provider(
        &consumer,
        0,
        &providers,
        &grants(),
        &mut wire::Session::new(exact),
    )
    .unwrap();
    for which in 0..4 {
        let mut limit = exact;
        match which {
            0 => limit.work -= 1,
            1 => limit.allocation_bytes -= 1,
            2 => limit.elements -= 1,
            3 => limit.serialization_bytes -= 1,
            _ => unreachable!(),
        }
        let mut s = wire::Session::new(limit);
        assert!(
            matches!(
                wire::select_provider(&consumer, 0, &providers, &grants(), &mut s),
                Err(SelectionError::Wire(_))
            ),
            "quota {which}"
        );
        assert!(s.usage().work > 0);
    }
    let mut s = wire::Session::new(wire::Limits {
        work: used.work * 2 - 1,
        ..Default::default()
    });
    wire::select_provider(&consumer, 0, &providers, &grants(), &mut s).unwrap();
    assert!(wire::select_provider(&consumer, 0, &providers, &grants(), &mut s).is_err());
    assert!(s.usage().work >= used.work);
}

#[test]
fn stricter_limits_apply_to_unused_metadata_even_in_ineligible_candidates() {
    let consumer = consumer();
    let mut json = fixture();
    json["version"] = serde_json::json!("9.0.0");
    json["interfaces"][0]["types"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name":"unreachable-and-too-long-for-this-profile","record":[]}));
    let provider = contract(&json);
    let d = digest(1);
    let mut session = wire::Session::new(wire::Limits {
        metadata: idl::Limits {
            string_bytes: 20,
            ..Default::default()
        },
        ..Default::default()
    });
    assert!(matches!(
        wire::select_provider(
            &consumer,
            0,
            &[Provider {
                contract: &provider,
                digest: &d
            }],
            &grants(),
            &mut session
        ),
        Err(SelectionError::Wire(_))
    ));
    let mut json = fixture();
    let mut ty = serde_json::json!("unit");
    for _ in 0..12 {
        ty = serde_json::json!({"option":ty});
    }
    json["interfaces"][0]["types"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name":"unused","alias":ty}));
    let provider = contract(&json);
    let mut session = wire::Session::new(wire::Limits {
        metadata: idl::Limits {
            depth: consumer.checked().usage().data_depth,
            ..Default::default()
        },
        ..Default::default()
    });
    assert!(matches!(
        wire::select_provider(
            &consumer,
            0,
            &[Provider {
                contract: &provider,
                digest: &d
            }],
            &grants(),
            &mut session
        ),
        Err(SelectionError::Wire(_))
    ));
}

fn import_projection(mut value: serde_json::Value, member: &str) -> serde_json::Value {
    let key = value["interfaces"][0]["key"].clone();
    value["package"] = serde_json::json!("demo:consumer");
    value["exports"] = serde_json::json!([]);
    value["imports"] = serde_json::json!([{"interface":key,"version":{"interval":{"min":"1.0.0","max":"2.0.0","prerelease":false}},"members":[member]}]);
    value
}

#[test]
fn rich_list_option_result_variant_and_alias_signatures_are_not_reduced_to_scalars() {
    let base: serde_json::Value = serde_json::from_slice(include_bytes!(
        "fixtures/component_generated_values.idl.json"
    ))
    .unwrap();
    let consumer = contract(&import_projection(base.clone(), "echo"));
    let compatible = contract(&base);
    let mut caps = CapabilityGrants::new();
    caps.grant("demo.generated.echo");
    let d = digest(1);
    wire::select_provider(
        &consumer,
        0,
        &[Provider {
            contract: &compatible,
            digest: &d,
        }],
        &caps,
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    for variant in [false, true] {
        let mut changed = base.clone();
        if variant {
            changed["interfaces"][0]["types"][0]["variant"][1]["type"] = serde_json::json!("bool");
        } else {
            changed["interfaces"][0]["types"][1]["record"][4]["type"] =
                serde_json::json!({"list":"i64"});
        }
        let provider = contract(&changed);
        let error = wire::select_provider(
            &consumer,
            0,
            &[Provider {
                contract: &provider,
                digest: &d,
            }],
            &caps,
            &mut wire::Session::new(Default::default()),
        )
        .unwrap_err();
        assert_eq!(kind(error), K::StructuralMismatch);
    }
}

#[test]
fn resource_keys_and_shared_exclusive_or_owned_access_are_part_of_the_projection() {
    let base: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_counter.idl.json")).unwrap();
    let consumer = contract(&import_projection(base.clone(), "apply"));
    let compatible = contract(&base);
    let mut caps = CapabilityGrants::new();
    caps.grant("demo.counter.apply");
    let d = digest(1);
    wire::select_provider(
        &consumer,
        0,
        &[Provider {
            contract: &compatible,
            digest: &d,
        }],
        &caps,
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    for access in ["borrow", "own", "different-key"] {
        let mut changed = base.clone();
        changed["interfaces"][0]["functions"][0]["params"][0]["type"] = match access {
            "borrow" => serde_json::json!({"borrow":"counter"}),
            "own" => serde_json::json!({"own":"counter"}),
            _ => {
                changed["interfaces"][0]["resources"] = serde_json::json!(["other"]);
                serde_json::json!({"borrow-mut":"other"})
            }
        };
        let provider = contract(&changed);
        assert_eq!(
            kind(
                wire::select_provider(
                    &consumer,
                    0,
                    &[Provider {
                        contract: &provider,
                        digest: &d
                    }],
                    &caps,
                    &mut wire::Session::new(Default::default())
                )
                .unwrap_err()
            ),
            K::StructuralMismatch
        );
    }
}

#[test]
fn selection_and_rejections_are_stable_under_candidate_permutation_and_never_match_unexported_declarations()
 {
    let consumer = consumer();
    let low = provider("1.1.0", false);
    let high = provider("1.9.0", false);
    let bad = provider("1.8.0", true);
    let d1 = digest(1);
    let d2 = digest(9);
    let d3 = digest(8);
    let entries = [
        Provider {
            contract: &low,
            digest: &d1,
        },
        Provider {
            contract: &high,
            digest: &d2,
        },
        Provider {
            contract: &bad,
            digest: &d3,
        },
    ];
    for order in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
        let providers = order.map(|i| entries[i]);
        let selected = wire::select_provider(
            &consumer,
            0,
            &providers,
            &grants(),
            &mut wire::Session::new(Default::default()),
        )
        .unwrap();
        assert_eq!(selected.provider().digest, &d2);
    }
    let mut json = fixture();
    json["exports"] = serde_json::json!([]);
    let unexported = contract(&json);
    assert_eq!(
        select_kind(
            &consumer,
            &[Provider {
                contract: &unexported,
                digest: &d1
            }]
        ),
        K::NoCompatibleVersion
    );
    let error = wire::select_provider(
        &consumer,
        9,
        &entries,
        &grants(),
        &mut wire::Session::new(Default::default()),
    )
    .unwrap_err();
    assert_eq!(kind(error), K::InvalidImport);
    let a = provider("1.1.0", true);
    let b = provider("1.5.0", true);
    let failed = [
        Provider {
            contract: &a,
            digest: &d1,
        },
        Provider {
            contract: &b,
            digest: &d2,
        },
    ];
    for providers in [failed, [failed[1], failed[0]]] {
        let SelectionError::Metadata { rejections, .. } = wire::select_provider(
            &consumer,
            0,
            &providers,
            &grants(),
            &mut wire::Session::new(Default::default()),
        )
        .unwrap_err() else {
            panic!("mismatches")
        };
        assert_eq!(rejections.len(), 2);
        assert_eq!(providers[rejections[0].provider].digest, &d2);
    }
}
