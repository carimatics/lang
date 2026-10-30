use lang::frontend::component as c;
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::component::{
    bindings as b, grants as g, idl, native,
    negotiation::{InterfaceKey, PackageKey},
    serialization::{DigestAlgorithm, VersionedDigest},
    wire,
};
use serde_json::{Value, json};
use std::{cell::Cell, ffi::c_void, path::Path, ptr};

const SOURCE: &str = "pub struct Request{value:i64,flag:bool}\nextern \"component\" fn read(value:Request)->Request;\nextern \"component\" fn spare(value:i64)->i64;\npub fn run(value:i64)->i64{return value;}";
fn host_json() -> Value {
    serde_json::from_slice(include_bytes!(
        "fixtures/component_generated_script.idl.json"
    ))
    .unwrap()
}
fn scalar(key: &str, name: &str, capability: &str) -> Value {
    json!({"key":key,"resources":[],"types":[],"functions":[{"name":name,"effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":capability}]})
}
fn consumer_json() -> Value {
    let mut value = host_json();
    value["package"] = json!("demo:consumer");
    value["interfaces"].as_array_mut().unwrap().extend([
        scalar("demo:consumer/api", "query", "demo.consumer.query"),
        scalar("demo:spare/api", "lookup", "demo.spare.lookup"),
    ]);
    value["imports"] = json!([
        {"interface":"demo:script/api","version":{"exact":"1.0.0"},"members":["echo"]},
        {"interface":"demo:spare/api","version":{"exact":"1.0.0"},"members":["lookup"]}
    ]);
    value["exports"] = json!(["demo:consumer/api"]);
    value
}
fn checked(value: &Value, source: u32) -> idl::CheckedDocument {
    idl::check(
        m::SourceId(source),
        &serde_json::to_vec(value).unwrap(),
        Default::default(),
    )
    .unwrap()
}
fn contract(value: &Value) -> wire::Contract {
    wire::Contract::new(
        checked(value, 77),
        vec![],
        &mut wire::Session::new(Default::default()),
    )
    .unwrap()
}
fn key(package: &str, member: &str) -> b::MemberKey {
    b::MemberKey {
        interface: InterfaceKey::new(PackageKey::new("demo", package), "api"),
        member: member.into(),
    }
}
fn unit() -> b::Unit {
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display: Path::new("typed-grants.lang"),
        bytes: SOURCE.as_bytes(),
        root: None,
        compile: Default::default(),
        document: checked(&consumer_json(), 77),
        bindings: c::Input {
            version: 1,
            resources: vec![],
            imports: vec![
                c::Import {
                    source: "read".into(),
                    member: key("script", "echo"),
                },
                c::Import {
                    source: "spare".into(),
                    member: key("spare", "lookup"),
                },
            ],
            exports: vec![c::Export {
                source: "run".into(),
                member: key("consumer", "query"),
            }],
        },
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    out.result.unwrap()
}
fn signatures(unit: &b::Unit, contract: &wire::Contract) -> Vec<Vec<u8>> {
    let mut s = wire::Session::new(Default::default());
    unit.mappings()
        .imports
        .iter()
        .map(|i| {
            wire::Signature::new(contract, &i.member.interface, &i.member.member, &mut s)
                .unwrap()
                .preimage(&mut s)
                .unwrap()
        })
        .collect()
}
fn grants<'a>(unit: &'a b::Unit, bytes: &'a [Vec<u8>]) -> Vec<g::Grant<'a>> {
    unit.mappings()
        .imports
        .iter()
        .zip(bytes)
        .map(|(i, bytes)| g::Grant {
            member: &i.member,
            capability: if i.member.member == "echo" {
                "demo.script.echo"
            } else {
                "demo.spare.lookup"
            },
            signature: bytes,
        })
        .collect()
}
fn kind(error: g::Error) -> g::ErrorKind {
    match error {
        g::Error::Admission { kind, .. } => kind,
        other => panic!("{other:?}"),
    }
}
extern "C" fn callback(
    _: *mut c_void,
    _: *const native::Context,
    _: native::Bytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    panic!("preparation invoked callback")
}
fn method(user: *mut c_void) -> native::Method {
    /* SAFETY: retained code/user outlive these descriptors and all synchronous uses. */
    unsafe { native::Method::new(0, 0, callback, user, 1) }
}
fn digest() -> VersionedDigest {
    VersionedDigest::new(DigestAlgorithm::Sha256, vec![7; 32]).unwrap()
}
fn spare_json() -> Value {
    json!({"idl":1,"package":"demo:spare","version":"1.0.0","interfaces":[scalar("demo:spare/api","lookup","demo.spare.lookup")],"imports":[],"exports":["demo:spare/api"]})
}

#[test]
fn unused_rich_and_scalar_imports_bind_real_callbacks_owners_and_source_ids() {
    let unit = unit();
    let contract = contract(&consumer_json());
    let bytes = signatures(&unit, &contract);
    let grants = grants(&unit, &bytes);
    let host = contract_for_host();
    let spare = contract_for_spare();
    let digest = digest();
    let owners = [Cell::new(0), Cell::new(0)];
    let methods = [
        [method(ptr::from_ref(&owners[0]).cast_mut().cast())],
        [method(ptr::from_ref(&owners[1]).cast_mut().cast())],
    ];
    let mut s = wire::Session::new(Default::default());
    let providers = [
        native::Provider::new(
            wire::Provider {
                contract: &host,
                digest: &digest,
            },
            &methods[0],
            &owners[0],
            &mut s,
        )
        .unwrap(),
        native::Provider::new(
            wire::Provider {
                contract: &spare,
                digest: &digest,
            },
            &methods[1],
            &owners[1],
            &mut s,
        )
        .unwrap(),
    ];
    let lookups = Cell::new(0);
    let bound = g::prepare(
        &unit,
        &contract,
        &grants,
        || {
            lookups.set(lookups.get() + 1);
            &providers
        },
        &mut s,
    )
    .unwrap();
    assert_eq!(lookups.get(), 1);
    assert_eq!(bound.imports().len(), 2);
    assert!(ptr::eq(bound.unit(), &unit));
    assert!(ptr::eq(bound.contract(), &contract));
    for (i, owner) in owners.iter().enumerate() {
        let import = bound.resolve(m::ImportId(i as u32)).unwrap();
        assert_eq!(import.source(), m::ImportId(i as u32));
        assert!(import.provider().retains_code(owner));
        assert!(std::ptr::fn_addr_eq(
            import.method().invoke_fn(),
            callback as native::Invoke
        ));
        assert_eq!(
            import.method().user_data(),
            ptr::from_ref(owner).cast_mut().cast()
        );
        assert_eq!(owner.get(), 0);
    }
    assert!(bound.resolve(m::ImportId(2)).is_none());
}
fn contract_for_host() -> wire::Contract {
    contract(&host_json())
}
fn contract_for_spare() -> wire::Contract {
    contract(&spare_json())
}

#[test]
fn every_invalid_grant_including_later_unused_interface_precedes_inventory() {
    let unit = unit();
    let contract = contract(&consumer_json());
    let bytes = signatures(&unit, &contract);
    let good = grants(&unit, &bytes);
    let unknown = key("spare", "unknown");
    let cases = [
        (vec![], g::ErrorKind::MissingGrant),
        (vec![good[0]], g::ErrorKind::MissingGrant),
        (vec![good[0], good[0]], g::ErrorKind::DuplicateGrant),
        (
            vec![
                good[0],
                g::Grant {
                    member: &unknown,
                    ..good[1]
                },
            ],
            g::ErrorKind::UnknownGrant,
        ),
        (
            vec![
                good[0],
                g::Grant {
                    capability: "demo.script.echo",
                    ..good[1]
                },
            ],
            g::ErrorKind::Capability,
        ),
        (
            vec![
                good[0],
                g::Grant {
                    signature: good[0].signature,
                    ..good[1]
                },
            ],
            g::ErrorKind::Signature,
        ),
    ];
    for (grants, expected) in cases {
        let lookups = Cell::new(0);
        let error = g::prepare(
            &unit,
            &contract,
            &grants,
            || {
                lookups.set(lookups.get() + 1);
                &[]
            },
            &mut wire::Session::new(Default::default()),
        )
        .err()
        .unwrap();
        assert_eq!(kind(error), expected);
        assert_eq!(lookups.get(), 0);
    }
    let error = g::prepare(
        &unit,
        &contract,
        &good[..1],
        || panic!("inventory"),
        &mut wire::Session::new(Default::default()),
    )
    .err()
    .unwrap();
    let g::Error::Admission {
        import,
        source,
        contract,
        ..
    } = error
    else {
        panic!()
    };
    assert_eq!(import, Some(m::ImportId(1)));
    assert_eq!(source, Some(unit.code().declarations().imports()[1].span));
    let f = unit.imports()[1];
    assert_eq!(
        contract,
        Some(unit.document().document().interfaces[f.interface].functions[f.member].span)
    );
}

#[test]
fn exact_snapshot_pairing_refuses_source_identity_and_unused_metadata_swaps() {
    let unit = unit();
    let mut changed = consumer_json();
    changed["version"] = json!("1.0.1");
    let changed = contract(&changed);
    let different_source = wire::Contract::new(
        checked(&consumer_json(), 78),
        vec![],
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    for contract in [&changed, &different_source] {
        assert_eq!(
            kind(
                g::prepare(
                    &unit,
                    contract,
                    &[],
                    || panic!("inventory"),
                    &mut wire::Session::new(Default::default())
                )
                .err()
                .unwrap()
            ),
            g::ErrorKind::Snapshot
        );
    }
}

#[test]
fn forged_signature_bytes_cannot_authorize_an_independently_verified_import() {
    let unit = unit();
    let contract = contract(&consumer_json());
    let original = signatures(&unit, &contract);
    for bytes in [
        vec![],
        vec![0; 32],
        {
            let mut b = original[0].clone();
            b.push(0);
            b
        },
        {
            let mut b = original[0].clone();
            b[0] ^= 1;
            b
        },
    ] {
        let mut altered = original.clone();
        altered[0] = bytes;
        let grants = grants(&unit, &altered);
        assert_eq!(
            kind(
                g::prepare(
                    &unit,
                    &contract,
                    &grants,
                    || panic!("inventory"),
                    &mut wire::Session::new(Default::default())
                )
                .err()
                .unwrap()
            ),
            g::ErrorKind::Signature
        );
    }
}

#[test]
fn preparation_accepts_exact_usage_and_refuses_each_one_below_boundary() {
    let unit = unit();
    let contract = contract(&consumer_json());
    let bytes = signatures(&unit, &contract);
    let grants = grants(&unit, &bytes);
    let host = contract_for_host();
    let spare = contract_for_spare();
    let digest = digest();
    let owner = Cell::new(0);
    let methods = [method(ptr::null_mut())];
    let mut registration = wire::Session::new(Default::default());
    let providers = [
        native::Provider::new(
            wire::Provider {
                contract: &host,
                digest: &digest,
            },
            &methods,
            &owner,
            &mut registration,
        )
        .unwrap(),
        native::Provider::new(
            wire::Provider {
                contract: &spare,
                digest: &digest,
            },
            &methods,
            &owner,
            &mut registration,
        )
        .unwrap(),
    ];
    let mut s = wire::Session::new(Default::default());
    g::prepare(&unit, &contract, &grants, || &providers, &mut s).unwrap();
    let usage = s.usage();
    let exact = wire::Limits {
        work: usage.work,
        allocation_bytes: usage.allocation_bytes,
        elements: usage.elements,
        serialization_bytes: usage.serialization_bytes,
        ..Default::default()
    };
    let mut s = wire::Session::new(exact);
    g::prepare(&unit, &contract, &grants, || &providers, &mut s).unwrap();
    assert_eq!(s.usage(), usage);
    for limits in [
        wire::Limits {
            work: usage.work - 1,
            ..exact
        },
        wire::Limits {
            allocation_bytes: usage.allocation_bytes - 1,
            ..exact
        },
        wire::Limits {
            elements: usage.elements - 1,
            ..exact
        },
        wire::Limits {
            serialization_bytes: usage.serialization_bytes - 1,
            ..exact
        },
        wire::Limits {
            metadata: idl::Limits {
                declarations: contract.checked().usage().declarations - 1,
                ..Default::default()
            },
            ..exact
        },
    ] {
        let mut s = wire::Session::new(limits);
        assert!(matches!(
            g::prepare(&unit, &contract, &grants, || &providers, &mut s),
            Err(g::Error::Wire(_)) | Err(g::Error::Selection(wire::SelectionError::Wire(_)))
        ));
        assert!(s.usage().work <= limits.work);
        assert!(s.usage().allocation_bytes <= limits.allocation_bytes);
        assert_eq!(owner.get(), 0);
    }
    // A reused exhausted session cannot reset or refund preparation accounting.
    let mut s = wire::Session::new(exact);
    g::prepare(&unit, &contract, &grants, || &providers, &mut s).unwrap();
    assert!(
        g::prepare(
            &unit,
            &contract,
            &grants,
            || panic!("inventory after exhaustion"),
            &mut s
        )
        .is_err()
    );
    assert_eq!(s.usage(), usage);
}

#[test]
fn late_selection_failure_publishes_nothing_and_preserves_real_owners() {
    struct Owner<'a>(&'a Cell<usize>);
    impl Drop for Owner<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let dropped = Cell::new(0);
    {
        let unit = unit();
        let contract = contract(&consumer_json());
        let bytes = signatures(&unit, &contract);
        let grants = grants(&unit, &bytes);
        let host = contract_for_host();
        let spare = contract_for_spare();
        let digest = digest();
        let owners = [Owner(&dropped), Owner(&dropped)];
        let methods = [method(ptr::null_mut())];
        let mut s = wire::Session::new(Default::default());
        let providers = [
            native::Provider::new(
                wire::Provider {
                    contract: &host,
                    digest: &digest,
                },
                &methods,
                &owners[0],
                &mut s,
            )
            .unwrap(),
            native::Provider::new(
                wire::Provider {
                    contract: &spare,
                    digest: &digest,
                },
                &methods,
                &owners[1],
                &mut s,
            )
            .unwrap(),
        ];
        assert!(matches!(
            g::prepare(&unit, &contract, &grants, || &providers[..1], &mut s),
            Err(g::Error::Selection(_))
        ));
        assert_eq!(dropped.get(), 0);
        let bound = g::prepare(&unit, &contract, &grants, || &providers, &mut s).unwrap();
        for (index, owner) in owners.iter().enumerate() {
            assert!(
                bound
                    .resolve(m::ImportId(index as u32))
                    .unwrap()
                    .provider()
                    .retains_code(owner)
            );
        }
        assert_eq!(dropped.get(), 0);
    }
    assert_eq!(dropped.get(), 2);
}
