use super::*;
use std::collections::BTreeMap;

fn empty_inventory() -> storage::Inventory {
    storage::Inventory {
        bytes: 0,
        entries: 0,
        immutable: BTreeMap::new(),
        bindings: BTreeMap::new(),
        staged: Vec::new(),
    }
}
#[test]
fn peak_bytes_entries_reuse_and_checked_overflow_boundaries() {
    for delta in [-1i64, 0, 1] {
        let mut inventory = empty_inventory();
        assert_eq!(
            inventory
                .charge((storage::MAX_BYTES as i64 + delta) as u64, 0)
                .is_ok(),
            delta <= 0
        );
        let mut inventory = empty_inventory();
        assert_eq!(
            inventory
                .charge(0, (storage::MAX_ENTRIES as i64 + delta) as usize)
                .is_ok(),
            delta <= 0
        );
    }
    let mut inventory = empty_inventory();
    inventory.bytes = u64::MAX;
    assert_eq!(inventory.charge(1, 0).unwrap_err().kind, ErrorKind::Limit);
    let mut inventory = empty_inventory();
    inventory.entries = usize::MAX;
    assert_eq!(inventory.charge(0, 1).unwrap_err().kind, ErrorKind::Limit);
    let hash = "ab".repeat(32);
    let candidate = [1u8; 20];
    let mut inventory = empty_inventory();
    inventory.bytes = storage::MAX_BYTES - 30;
    inventory.entries = storage::MAX_ENTRIES - 2;
    assert!(
        inventory
            .reserve(&[(storage::Space::Object, &hash, &candidate)], 10)
            .is_ok()
    );
    assert_eq!(
        inventory
            .reserve(&[(storage::Space::Object, &hash, &candidate)], 11)
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    inventory.entries += 1;
    assert_eq!(
        inventory
            .reserve(&[(storage::Space::Object, &hash, &candidate)], 10)
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    inventory.immutable.insert(
        (storage::Space::Object, hash.clone()),
        candidate.len() as u64,
    );
    assert!(
        inventory
            .reserve(&[(storage::Space::Object, &hash, &candidate)], 10)
            .is_ok()
    );
}

#[test]
fn canonical_record_bytes_depth_and_closed_fields() {
    for limit in [records::MAX_HEAD, records::MAX_AUDIT, records::MAX_STATE] {
        for extra in [0, 1] {
            let value = "x".repeat(limit - 3 + extra); // quotes plus final LF
            let encoded = records::encode(&value, limit);
            assert_eq!(encoded.is_ok(), extra == 0);
            let mut bytes = serde_json::to_vec(&value).unwrap();
            bytes.push(b'\n');
            assert_eq!(records::decode::<String>(&bytes, limit).is_ok(), extra == 0);
        }
    }
    for depth in [15, 16, 17] {
        let mut value = serde_json::Value::Null;
        for _ in 0..depth {
            value = serde_json::json!([value]);
        }
        let bytes = records::encode(&value, records::MAX_STATE).unwrap();
        assert_eq!(
            records::decode::<serde_json::Value>(&bytes, records::MAX_STATE).is_ok(),
            depth <= 16
        );
    }
    let hash = "ab".repeat(32);
    let good = head_bytes(&hash).unwrap();
    for bad in [
        good[..good.len() - 1].to_vec(),
        String::from_utf8(good.clone())
            .unwrap()
            .replace("\"schema\": 1", "\"schema\": 1,\n  \"schema\": 1")
            .into_bytes(),
        String::from_utf8(good.clone())
            .unwrap()
            .replace("\"schema\": 1", "\"schema\": 1,\n  \"unknown\": 0")
            .into_bytes(),
        String::from_utf8(good.clone())
            .unwrap()
            .replace("\"schema\": 1", "\"schema\": 1.0")
            .into_bytes(),
    ] {
        assert!(records::decode::<records::Head>(&bad, records::MAX_HEAD).is_err());
    }
    assert!(records::decode::<records::Head>(&good, records::MAX_HEAD).is_ok());
}

fn base_state() -> records::State {
    records::State {
        format: "lang-registry-state".into(),
        schema: 1,
        registry: "example".into(),
        origin: "https://registry.example".into(),
        revision: records::MAX_EVENTS,
        time_floor: 100,
        authority: Authority::admit(&authority_bytes(), &identity())
            .unwrap()
            .into_records(),
        packages: Vec::new(),
        last_audit_sha256: "ab".repeat(32),
    }
}
fn fill(state: &mut records::State, names: usize, versions: usize) {
    let release = records::Release::from_archive(&archive("0.1.0"));
    for i in 0..names {
        let name = format!("package{i:04}");
        let releases = (0..versions)
            .map(|v| {
                let mut r = release.clone();
                r.version = format!("0.0.{v}");
                r
            })
            .collect();
        state.packages.push(records::Package {
            name: name.clone(),
            revision: versions as u64,
            releases,
        });
        state.authority.packages.push(authority::codec::Package {
            name,
            owners: vec![PRINCIPAL.into()],
        });
    }
}
#[test]
fn package_version_release_and_event_count_boundaries() {
    for names in [1023, 1024, 1025] {
        let mut state = base_state();
        fill(&mut state, names, 1);
        assert_eq!(state.validate(&identity()).is_ok(), names <= 1024);
    }
    for versions in [127, 128, 129] {
        let mut state = base_state();
        fill(&mut state, 1, versions);
        assert_eq!(state.validate(&identity()).is_ok(), versions <= 128);
    }
    let mut state = base_state();
    fill(&mut state, 64, 128);
    assert!(state.validate(&identity()).is_ok());
    state.packages[0].releases.pop();
    assert!(state.validate(&identity()).is_ok());
    let last = state.packages[1].releases[127].clone();
    state.packages[0].releases.push(records::Release {
        version: "0.0.127".into(),
        ..last
    });
    state.packages.push(records::Package {
        name: "zzlast".into(),
        revision: 1,
        releases: vec![state.packages[0].releases[0].clone()],
    });
    state.authority.packages.push(authority::codec::Package {
        name: "zzlast".into(),
        owners: vec![PRINCIPAL.into()],
    });
    assert_eq!(
        state.validate(&identity()).unwrap_err().kind,
        ErrorKind::Limit
    );
    for revision in [
        0,
        1,
        records::MAX_EVENTS - 1,
        records::MAX_EVENTS,
        records::MAX_EVENTS + 1,
    ] {
        let mut state = base_state();
        state.revision = revision;
        assert_eq!(
            state.validate(&identity()).is_ok(),
            revision > 0 && revision <= records::MAX_EVENTS
        );
    }
}

#[test]
fn wrong_service_secret_and_scope_never_publish() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut store = initialize(&parent);
    let before = snapshot(&tmp.path().join("registry"));
    let wrong = Bearer::from_input("22".repeat(32).as_bytes()).unwrap();
    assert_eq!(
        store
            .publish(
                &wrong,
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(110)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Authentication
    );
    assert_eq!(snapshot(&tmp.path().join("registry")), before);
    drop(store);
    let other = ServiceIdentity::new("other", "https://registry.other").unwrap();
    assert!(Registry::open(&parent, "registry", &other).is_err());
    assert_eq!(snapshot(&tmp.path().join("registry")), before);

    let tmp = Temp::new();
    let parent = super::parent(tmp.path());
    let mut authority = Authority::admit(&authority_bytes(), &identity())
        .unwrap()
        .into_records();
    authority.tokens[0].grants.remove(0); // publish alone cannot claim an absent name
    let mut store = Registry::initialize(
        &parent,
        "registry",
        &identity(),
        &records::encode(&authority, authority::MAX_BYTES).unwrap(),
        PRINCIPAL,
        &At(100),
    )
    .unwrap();
    assert_eq!(
        store
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(110)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Authorization
    );
    assert_eq!(store.revision().unwrap(), 1);
}

#[test]
fn oversized_physical_records_refuse_before_cleanup() {
    for (path, limit) in [
        ("HEAD".into(), records::MAX_HEAD),
        ("LOCK".into(), 0),
        (
            format!("objects/{}", "aa".repeat(32)),
            archive::MAX_ARCHIVE_BYTES,
        ),
        (format!("states/{}", "aa".repeat(32)), records::MAX_STATE),
        (format!("audits/{}", "aa".repeat(32)), records::MAX_AUDIT),
        (
            format!("staging/head-{}", "aa".repeat(32)),
            records::MAX_HEAD,
        ),
    ] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        drop(initialize(&parent));
        let root = tmp.path().join("registry");
        std::fs::File::create(root.join(path))
            .unwrap()
            .set_len(limit as u64 + 1)
            .unwrap();
        std::fs::write(
            root.join(format!("staging/object-{}", "bb".repeat(32))),
            b"partial",
        )
        .unwrap();
        refuse_without_cleanup(&tmp, &parent);
    }
    let mut prior = base_state();
    prior.revision = records::MAX_EVENTS - 1;
    assert!(
        history::advance(
            &prior,
            PRINCIPAL,
            "answer",
            records::Release::from_archive(&archive("0.1.0")),
            &"ab".repeat(32),
            110
        )
        .is_ok()
    );
    prior.revision += 1;
    assert!(matches!(
        history::advance(
            &prior,
            PRINCIPAL,
            "answer",
            records::Release::from_archive(&archive("0.1.0")),
            &"ab".repeat(32),
            110
        ),
        Err(Error {
            kind: ErrorKind::Limit,
            ..
        })
    ));
}
