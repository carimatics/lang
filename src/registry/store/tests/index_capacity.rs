use super::*;
use crate::registry::archive::Descriptor;
use sha2::{Digest, Sha256};

#[derive(serde::Serialize)]
struct OracleIndex<'a> {
    format: &'static str,
    schema: u32,
    registry: &'static str,
    name: &'static str,
    revision: u64,
    releases: &'a [records::Release],
}

// Copy only our own closed, validated fixture after releasing its lock. These
// are independent files, never hardlink aliases or an untrusted extraction.
fn copy_fixture(source: &std::path::Path, destination: &std::path::Path) {
    std::fs::create_dir(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &target);
        } else {
            assert!(entry.file_type().unwrap().is_file());
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

// Build the prior D02 private format directly as a legacy fixture. Production
// publication has no bypass of the derived-index guard.
fn legacy_publication(registry: &Registry, archive: &AdmittedArchive) {
    let view = history::load(&registry.storage, &identity()).unwrap();
    let (mut state, result) = history::advance(
        &view.state,
        PRINCIPAL,
        "answer",
        records::Release::from_archive(archive),
        "",
        110,
    )
    .unwrap();
    let audit = records::Audit {
        format: "lang-registry-audit".into(),
        schema: 1,
        sequence: state.revision,
        previous_audit_sha256: Some(view.state.last_audit_sha256.clone()),
        prior_state_sha256: Some(view.head),
        time: 110,
        principal: PRINCIPAL.into(),
        credential_id: Some("01".repeat(16)),
        operation: records::AuditOperation::Publish {
            name: "answer".into(),
            version: archive.descriptor().package().version().into(),
            archive_sha256: archive.sha256().into(),
        },
        request_id: Some(format!("{:032x}", 29)),
        request_sha256: Some(
            records::request_digest(
                "answer",
                archive.descriptor().package().version(),
                ExpectedRevision::Revision(29),
                archive.sha256(),
                archive.as_bytes().len() as u64,
            )
            .unwrap(),
        ),
        reason: "publish".into(),
        result: records::AuditResult::Publish(result),
    };
    let audit = records::encode(&audit, records::MAX_AUDIT).unwrap();
    state.last_audit_sha256 = digest(&audit);
    let state = records::encode(&state, records::MAX_STATE).unwrap();
    registry
        .storage
        .install(storage::Space::Object, archive.sha256(), archive.as_bytes())
        .unwrap();
    registry
        .storage
        .install(storage::Space::Audit, &digest(&audit), &audit)
        .unwrap();
    registry
        .storage
        .install(storage::Space::State, &digest(&state), &state)
        .unwrap();
    registry
        .storage
        .replace_head(
            &digest(&state),
            &head_bytes(&digest(&state)).unwrap(),
            || Ok(()),
        )
        .unwrap();
}

#[test]
fn exact_index_capacity_is_admitted_before_any_candidate_write() {
    let base = Temp::new();
    let parent = parent(base.path());
    let mut registry = super::yank::initialize_yank(&parent);
    for version in 0..29 {
        registry
            .publish(
                &bearer(),
                &large_archive(version, 0),
                &format!("{version:032x}"),
                if version == 0 {
                    ExpectedRevision::Absent
                } else {
                    ExpectedRevision::Revision(u64::from(version))
                },
                &At(110),
            )
            .unwrap();
    }
    assert_eq!(registry.index("answer").unwrap().len(), 2_033_722);
    drop(registry);
    for (trim, length, sha256) in [
        (
            6695,
            2_097_151,
            "982e693f4b13246d27cd7f0f39adbabd0fa2975663fd2fa872ee4b26c9a2d934",
        ),
        (
            6694,
            2_097_152,
            "d2697d8e0de1c56783ca1e45becd2ecfdc55d4327f0c6f84ed05a272b2b7e78d",
        ),
        (
            6693,
            2_097_153,
            "aa8b6297244e99eaaabdf18f750a260d1c60eadb31e783e38b04fbe748b179fd",
        ),
    ] {
        let tmp = Temp::new();
        copy_fixture(&base.path().join("registry"), &tmp.path().join("registry"));
        let mut registry = reopen(&super::parent(tmp.path()));
        let candidate = large_archive(29, trim);
        let view = history::load(&registry.storage, &identity()).unwrap();
        let (state, _) = history::advance(
            &view.state,
            PRINCIPAL,
            "answer",
            records::Release::from_archive(&candidate),
            "",
            110,
        )
        .unwrap();
        // The independent preflight supplies exact expected lengths and hashes.
        // Encode this test oracle above the production limit to include +1.
        let encoded = json(&OracleIndex {
            format: "lang-registry-index",
            schema: 1,
            registry: "example",
            name: "answer",
            revision: 30,
            releases: &state.packages[0].releases,
        });
        assert_eq!(encoded.len(), length);
        assert_eq!(digest(&encoded), sha256);
        assert_eq!(
            Index::admit(&encoded, &identity(), "answer").is_ok(),
            length <= MAX_INDEX_BYTES
        );
        let before = snapshot(&tmp.path().join("registry"));
        let result = registry.publish(
            &bearer(),
            &candidate,
            &format!("{:032x}", 29),
            ExpectedRevision::Revision(29),
            &At(110),
        );
        if length <= MAX_INDEX_BYTES {
            result.unwrap();
            let index = registry.index("answer").unwrap();
            assert_eq!(index.len(), length);
            assert_eq!(digest(&index), sha256);
            assert_eq!(
                registry.archive(candidate.sha256()).unwrap().as_bytes(),
                candidate.as_bytes()
            );
        } else {
            let failure = result.unwrap_err();
            assert_eq!(failure.kind, ErrorKind::Limit);
            assert_eq!(failure.outcome, PublicationOutcome::NotPublished);
            assert_eq!(snapshot(&tmp.path().join("registry")), before);
            assert_eq!(registry.revision().unwrap(), 30);
            assert_eq!(
                registry.archive(candidate.sha256()).unwrap_err().kind,
                ErrorKind::NotFound
            );
            legacy_publication(&registry, &candidate);
            drop(registry);
            let mut registry = reopen(&super::parent(tmp.path()));
            let legacy = snapshot(&tmp.path().join("registry"));
            assert_eq!(registry.index("answer").unwrap_err().kind, ErrorKind::Limit);
            assert_eq!(
                registry.archive(candidate.sha256()).unwrap().as_bytes(),
                candidate.as_bytes()
            );
            let next = archive("0.0.30");
            assert_eq!(
                registry
                    .publish(
                        &bearer(),
                        &next,
                        &format!("{:032x}", 30),
                        ExpectedRevision::Revision(30),
                        &At(110)
                    )
                    .unwrap_err()
                    .kind,
                ErrorKind::Limit
            );
            assert_eq!(snapshot(&tmp.path().join("registry")), legacy);
            let reads = CapturedReads::capture(registry).unwrap();
            assert_eq!(reads.index("answer").unwrap_err().kind, ErrorKind::Limit);
            assert_eq!(
                reads.archive(candidate.sha256()).unwrap(),
                candidate.as_bytes()
            );
            assert_eq!(snapshot(&tmp.path().join("registry")), legacy);
        }
    }
    unyank_cannot_make_an_exact_capacity_index_unservable(base.path());
}

fn unyank_cannot_make_an_exact_capacity_index_unservable(base: &std::path::Path) {
    let tmp = Temp::new();
    copy_fixture(&base.join("registry"), &tmp.path().join("registry"));
    let parent = parent(tmp.path());
    let mut registry = reopen(&parent);
    let first = large_archive(0, 0);
    let yank = YankAction::new(
        "answer",
        "0.0.0",
        &"f".repeat(32),
        29,
        YankActionKind::Yank,
        "capacity fixture",
    )
    .unwrap();
    registry.yank(&bearer(), &yank, &At(110)).unwrap();
    // true is one byte shorter than false. The independent +1 publication
    // fixture now fits exactly, so clearing that flag must refuse before writes.
    let candidate = large_archive(29, 6693);
    registry
        .publish(
            &bearer(),
            &candidate,
            &format!("{:032x}", 29),
            ExpectedRevision::Revision(30),
            &At(110),
        )
        .unwrap();
    let index = registry.index("answer").unwrap();
    assert_eq!(index.len(), MAX_INDEX_BYTES);
    let before = snapshot(&tmp.path().join("registry"));
    let unyank = YankAction::new(
        "answer",
        "0.0.0",
        &"e".repeat(32),
        31,
        YankActionKind::Unyank,
        "capacity fixture",
    )
    .unwrap();
    let failure = registry.yank(&bearer(), &unyank, &At(110)).unwrap_err();
    assert_eq!(failure.kind, ErrorKind::Limit);
    assert_eq!(failure.outcome, PublicationOutcome::NotPublished);
    assert_eq!(snapshot(&tmp.path().join("registry")), before);
    assert_eq!(registry.index("answer").unwrap(), index);
    assert_eq!(registry.revision().unwrap(), 32);
    assert_eq!(
        registry.archive(first.sha256()).unwrap().as_bytes(),
        first.as_bytes()
    );
    let admitted = Index::admit(&index, &identity(), "answer").unwrap();
    assert!(admitted.release("0.0.0").unwrap().yanked());
    let view = history::load(&registry.storage, &identity()).unwrap();
    let (next, _) = history::advance_yank(&view.state, "answer", "0.0.0", false, "", 110).unwrap();
    assert_eq!(
        json(&OracleIndex {
            format: "lang-registry-index",
            schema: 1,
            registry: "example",
            name: "answer",
            revision: 32,
            releases: &next.packages[0].releases
        })
        .len(),
        MAX_INDEX_BYTES + 1
    );
    // A valid synthetic transition preflight isolates revision digit growth;
    // it is not claimed to be a history reached through 68 extra transactions.
    let mut prior = view.state;
    prior.revision = 100;
    prior.packages[0].revision = 99;
    let target = "00000000000000000000000000000002";
    prior
        .authority
        .principals
        .push(authority::codec::Principal {
            id: target.into(),
            active: true,
        });
    prior.validate(&identity()).unwrap();
    assert_eq!(
        super::super::reads::encode_package("example", &prior.packages[0])
            .unwrap()
            .len(),
        MAX_INDEX_BYTES
    );
    let (next, _) = history::advance_owner(&prior, "answer", target, true, "", 110).unwrap();
    assert_eq!(
        json(&OracleIndex {
            format: "lang-registry-index",
            schema: 1,
            registry: "example",
            name: "answer",
            revision: 100,
            releases: &next.packages[0].releases,
        })
        .len(),
        MAX_INDEX_BYTES + 1
    );
    assert_eq!(
        super::super::reads::encode_package("example", &next.packages[0])
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    assert_eq!(snapshot(&tmp.path().join("registry")), before);
}

fn json(value: &impl Serialize) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    bytes
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn large_archive(version: u32, trim: usize) -> AdmittedArchive {
    let source = b"fn main() -> i64 { return 42; }\r\n";
    let deps: Vec<_> = (0..128).map(|i| serde_json::json!({
        "alias":format!("d{i:03}{}", "a".repeat(60)), "registry":"a".repeat(64 - trim.saturating_sub(i * 63).min(63)), "name":"a".repeat(64),
        "version":"4294967295.4294967295.4294967295", "archive_sha256":"ab".repeat(32),
        "targets":["aarch64-apple-darwin", "x86_64-pc-windows-msvc"]
    })).collect();
    let descriptor: Descriptor = serde_json::from_value(serde_json::json!({
        "format":"lang-registry-source", "schema":1,
        "package":{"name":"answer", "version":format!("0.0.{version}"), "edition":"2026", "entry":"main.lang"},
        "tests":[], "dependencies":deps, "files":[{"path":"main.lang", "bytes":source.len(), "sha256":hash(source)}]
    })).unwrap();
    let bytes = json(&descriptor);
    assert!(bytes.len() < 128 * 1024);
    let mut builder = tar::Builder::new(Vec::new());
    for (path, data) in [
        (".lang-package.json", bytes.as_slice()),
        ("main.lang", source.as_slice()),
    ] {
        let mut h = tar::Header::new_ustar();
        h.set_path(path).unwrap();
        h.set_mode(0o644);
        h.set_uid(0);
        h.set_gid(0);
        h.set_mtime(0);
        h.set_size(data.len() as u64);
        h.set_entry_type(tar::EntryType::Regular);
        h.set_device_major(0).unwrap();
        h.set_device_minor(0).unwrap();
        h.set_cksum();
        let sum = h.cksum().unwrap();
        h.as_mut_bytes()[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        builder.append(&h, data).unwrap();
    }
    AdmittedArchive::admit(&builder.into_inner().unwrap()).unwrap()
}
