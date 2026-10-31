use super::*;

#[test]
fn admitted_index_binds_every_release_field_without_claiming_signature_trust() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = initialize(&parent);
    let archive = archive("1.2.0");
    registry
        .publish(
            &bearer(),
            &archive,
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    let bytes = registry.index("answer").unwrap();
    let index = Index::admit(&bytes, &identity(), "answer").ok().unwrap();
    assert_eq!(index.as_bytes().unwrap(), bytes);
    assert_eq!(index.identity(), &identity());
    assert_eq!(index.revision(), 1);
    assert_eq!(index.releases().len(), 1);
    assert!(index.release("1.2.00").is_none());
    assert!(index.release("1.2.1").is_none());
    let release = index.release("1.2.0").unwrap();
    assert_eq!(
        release
            .admit_archive(archive.as_bytes())
            .unwrap()
            .as_bytes(),
        archive.as_bytes()
    );
    let mut damaged = archive.as_bytes().to_vec();
    damaged[600] ^= 1;
    assert_eq!(
        release.admit_archive(&damaged).unwrap_err().kind,
        ErrorKind::Integrity
    );
    assert!(Index::admit(&bytes, &identity(), "other").is_err());
    assert!(
        Index::admit(
            &bytes,
            &ServiceIdentity::new("other", "https://registry.example").unwrap(),
            "answer"
        )
        .is_err()
    );
    let original = String::from_utf8(bytes.clone()).unwrap();
    let excessive = original.replace(
        &format!("\"archive_bytes\": {}", archive.as_bytes().len()),
        &format!("\"archive_bytes\": {}", archive::MAX_ARCHIVE_BYTES + 1),
    );
    assert_eq!(
        Index::admit(excessive.as_bytes(), &identity(), "answer")
            .err()
            .unwrap()
            .kind,
        ErrorKind::Limit
    );
    let mut excessive: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    excessive["releases"] = serde_json::Value::Array(vec![excessive["releases"][0].clone(); 129]);
    assert_eq!(
        Index::admit(
            &serde_json::to_vec(&excessive).unwrap(),
            &identity(),
            "answer"
        )
        .err()
        .unwrap()
        .kind,
        ErrorKind::Limit
    );
    for changed in [
        original.replace("\"schema\": 1", "\"schema\": 2"),
        original.replace("\"revision\": 1", "\"revision\": 0"),
        original.replace("\"version\": \"1.2.0\"", "\"version\": \"01.2.0\""),
        original.replace("\"edition\": \"2026\"", "\"edition\": \"2099\""),
        original.replace("\"schema\": 1", "\"schema\": 1,\n  \"schema\": 1"),
        original.trim_end().to_owned(),
    ] {
        assert!(Index::admit(changed.as_bytes(), &identity(), "answer").is_err());
    }
    for changed in [
        original.replace(archive.descriptor_sha256(), &"ab".repeat(32)),
        original.replace("\"version\": \"1.2.0\"", "\"version\": \"1.2.1\""),
    ] {
        let index = Index::admit(changed.as_bytes(), &identity(), "answer")
            .ok()
            .unwrap();
        assert_eq!(
            index
                .releases()
                .next()
                .unwrap()
                .admit_archive(archive.as_bytes())
                .unwrap_err()
                .kind,
            ErrorKind::Integrity
        );
    }
    let yanked = original.replace("\"yanked\": false", "\"yanked\": true");
    let index = Index::admit(yanked.as_bytes(), &identity(), "answer")
        .ok()
        .unwrap();
    let release = index.releases().next().unwrap();
    assert!(release.yanked());
    assert!(release.admit_archive(archive.as_bytes()).is_ok());
    let quarantined = original.replace("\"quarantined\": false", "\"quarantined\": true");
    let index = Index::admit(quarantined.as_bytes(), &identity(), "answer")
        .ok()
        .unwrap();
    assert_eq!(
        index
            .releases()
            .next()
            .unwrap()
            .admit_archive(archive.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
}

#[test]
fn committed_index_and_archive_reads_never_expose_unpublished_objects() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = initialize(&parent);
    let first = archive("0.1.0");
    registry
        .storage
        .install(storage::Space::Object, first.sha256(), first.as_bytes())
        .unwrap();
    let before = snapshot(&tmp.path().join("registry"));
    assert_eq!(
        registry.index("answer").unwrap_err().kind,
        ErrorKind::NotFound
    );
    assert_eq!(
        registry.archive(first.sha256()).unwrap_err().kind,
        ErrorKind::NotFound
    );
    assert_eq!(snapshot(&tmp.path().join("registry")), before);
    registry
        .publish(
            &bearer(),
            &first,
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    let captured = registry.index("answer").unwrap();
    assert_eq!(captured.last(), Some(&b'\n'));
    let value: serde_json::Value = serde_json::from_slice(&captured).unwrap();
    assert_eq!(value["format"], "lang-registry-index");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["registry"], "example");
    assert_eq!(value["name"], "answer");
    assert_eq!(value["revision"], 1);
    assert_eq!(value["releases"][0]["archive_sha256"], first.sha256());
    assert_eq!(
        registry.archive(first.sha256()).unwrap().as_bytes(),
        first.as_bytes()
    );
    registry
        .publish(
            &bearer(),
            &archive("0.2.0"),
            &"02".repeat(16),
            ExpectedRevision::Revision(1),
            &At(110),
        )
        .unwrap();
    assert_ne!(captured, registry.index("answer").unwrap());
    assert_eq!(value["releases"].as_array().unwrap().len(), 1);
    drop(registry);
    let registry = reopen(&parent);
    assert_eq!(
        registry
            .archive(first.sha256())
            .unwrap()
            .source(first.descriptor().package().entry()),
        Some(SOURCE)
    );
    assert_eq!(
        registry.index("../answer").unwrap_err().kind,
        ErrorKind::Malformed
    );
    assert_eq!(
        registry.archive("../HEAD").unwrap_err().kind,
        ErrorKind::Malformed
    );
}
