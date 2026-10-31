use lang::registry::archive::{
    AdmittedArchive, ErrorKind, MAX_ARCHIVE_BYTES, MAX_DESCRIPTOR_BYTES, MAX_FILES,
    MAX_SOURCE_BYTES,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

const SOURCE: &[u8] = b"fn main() -> i64 { return 42; }\r\n";

#[derive(Clone, Serialize)]
struct Package {
    name: String,
    version: String,
    edition: String,
    entry: String,
}

#[derive(Clone, Serialize)]
struct Edge {
    alias: String,
    registry: String,
    name: String,
    version: String,
    archive_sha256: String,
    targets: Vec<String>,
}

#[derive(Clone, Serialize)]
struct File {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Serialize)]
struct Metadata {
    format: String,
    schema: u32,
    package: Package,
    tests: Vec<String>,
    dependencies: Vec<Edge>,
    files: Vec<File>,
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn metadata(sources: &[(String, Vec<u8>)]) -> Metadata {
    Metadata {
        format: "lang-registry-source".into(),
        schema: 1,
        package: Package {
            name: "answer".into(),
            version: "0.1.0".into(),
            edition: "2026".into(),
            entry: sources[0].0.clone(),
        },
        tests: vec![],
        dependencies: vec![],
        files: sources
            .iter()
            .map(|(path, bytes)| File {
                path: path.clone(),
                bytes: bytes.len() as u64,
                sha256: hash(bytes),
            })
            .collect(),
    }
}

fn json(meta: &Metadata) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(meta).unwrap();
    bytes.push(b'\n');
    bytes
}

/// Independent tar-rs writer, not the reader's private canonical-header code.
fn archive(meta: &[u8], sources: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, data) in std::iter::once((".lang-package.json", meta)).chain(
        sources
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice())),
    ) {
        let mut header = tar::Header::new_ustar();
        header.set_path(path).unwrap();
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_size(data.len() as u64);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_device_major(0).unwrap();
        header.set_device_minor(0).unwrap();
        header.set_cksum();
        // tar-rs writes seven octal digits plus NUL. RFC 0046 deliberately
        // requires six digits, NUL, space; retain the independently computed sum.
        let checksum = header.cksum().unwrap();
        header.as_mut_bytes()[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
        builder.append(&header, data).unwrap();
    }
    builder.into_inner().unwrap()
}

fn simple() -> (Metadata, Vec<(String, Vec<u8>)>) {
    let sources = vec![("src/main.lang".into(), SOURCE.to_vec())];
    (metadata(&sources), sources)
}

fn refused(bytes: &[u8], kind: ErrorKind) -> lang::registry::archive::Error {
    let error = AdmittedArchive::admit(bytes).unwrap_err();
    assert_eq!(error.kind, kind, "{error}");
    assert!(error.to_string().len() < 256);
    error
}

#[test]
fn independent_ustar_fixture_matches_retained_hash_and_executes_original_source() {
    let (meta, sources) = simple();
    let mut encoded = archive(&json(&meta), &sources);
    assert_eq!(encoded.len(), 3072);
    assert_eq!(
        hash(&encoded),
        "ca4fe329e4da43f690e2236c65a5888d5791275a86b22bf11ac9bf5cced433c4"
    );
    let admitted = AdmittedArchive::admit(&encoded).unwrap();
    assert_eq!(admitted.as_bytes(), encoded);
    assert_eq!(admitted.sha256(), hash(&encoded));
    assert_eq!(admitted.descriptor_sha256(), hash(&json(&meta)));
    assert_eq!(admitted.descriptor().package().name(), "answer");
    assert_eq!(admitted.descriptor().package().version(), "0.1.0");
    assert_eq!(admitted.descriptor().package().edition(), "2026");
    assert_eq!(admitted.source("src/main.lang"), Some(SOURCE));
    encoded.fill(0);
    assert_eq!(admitted.source("src/main.lang"), Some(SOURCE));
    assert!(admitted.source("absent.lang").is_none());

    let analysis = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        std::path::Path::new("src/main.lang"),
        admitted.source("src/main.lang").unwrap(),
        None,
        Default::default(),
        None,
    );
    let lang_semantics::analysis::Program::Copy(typed) = analysis.result.unwrap() else {
        panic!("scalar fixture");
    };
    let program =
        lang::frontend::compile_script(&typed, &analysis.sources, Default::default()).unwrap();
    let result = lang_vm::unit::execute_entry(
        &program,
        &lang_runtime::HostFunctionRegistry::new(),
        &lang_vm::unit::HostBindings::new(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value.as_i64(), Some(42));
}

#[test]
fn hostile_headers_padding_trailers_and_truncations_are_refused() {
    let (meta, sources) = simple();
    let good = archive(&json(&meta), &sources);
    let mut alternate_checksum = good.clone();
    let checksum = tar::Header::from_byte_slice(&good[..512]).cksum().unwrap();
    alternate_checksum[148..156].copy_from_slice(format!("{checksum:07o}\0").as_bytes());
    refused(&alternate_checksum, ErrorKind::Malformed);
    for (offset, byte) in [
        (100, b'7'),
        (108, b'1'),
        (136, b'1'),
        (156, b'1'),
        (156, b'2'),
        (156, b'3'),
        (156, b'5'),
        (156, b'L'),
        (156, b'x'),
        (157, b'/'),
        (257, b'X'),
        (263, b'1'),
        (265, b'x'),
        (345, b'x'),
        (500, b'x'),
    ] {
        let mut bad = good.clone();
        bad[offset] = byte;
        bad[148..156].fill(b' ');
        let sum: usize = bad[..512].iter().map(|b| usize::from(*b)).sum();
        bad[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        refused(&bad, ErrorKind::Malformed);
    }
    for offset in [
        124,
        135,
        148,
        512 + json(&meta).len(),
        1536 + SOURCE.len(),
        good.len() - 1,
    ] {
        let mut bad = good.clone();
        bad[offset] = 255;
        refused(&bad, ErrorKind::Malformed);
    }
    for end in [
        0, 1, 511, 512, 513, 883, 1023, 1024, 1535, 1536, 1568, 2047, 2048, 3071,
    ] {
        assert!(
            AdmittedArchive::admit(&good[..end]).is_err(),
            "truncation {end}"
        );
    }
    for extra in [1, 512, 1024, good.len()] {
        let mut bad = good.clone();
        bad.resize(good.len() + extra, 0);
        refused(&bad, ErrorKind::Malformed);
    }
}

#[test]
fn source_digests_sizes_and_utf8_are_checked_without_compilation() {
    let (mut meta, mut sources) = simple();
    sources[0].1[0] = b'X';
    assert_eq!(
        refused(&archive(&json(&meta), &sources), ErrorKind::Integrity).code(),
        "REG0006"
    );
    sources[0].1.push(b' ');
    meta.files[0].sha256 = hash(&sources[0].1);
    refused(&archive(&json(&meta), &sources), ErrorKind::Integrity);
    sources[0].1 = vec![255];
    meta.files[0].sha256 = hash(&sources[0].1);
    meta.files[0].bytes = 1;
    assert_eq!(
        refused(&archive(&json(&meta), &sources), ErrorKind::Malformed).message,
        "source is not UTF-8"
    );
    sources[0].1 = b"deliberately not valid language syntax".to_vec();
    let meta = metadata(&sources);
    assert!(AdmittedArchive::admit(&archive(&json(&meta), &sources)).is_ok());
}

#[test]
fn portable_paths_refuse_escapes_devices_and_all_case_or_prefix_aliases() {
    let (base, sources) = simple();
    for path in [
        "/a.lang",
        "../a.lang",
        "a/../b.lang",
        "a//b.lang",
        "a\\b.lang",
        "C:/a.lang",
        "a./b.lang",
        "CON.lang",
        "a/NuL.txt/b.lang",
        "LPT9.lang",
        "com1.lang",
        ".git/a.lang",
        ".CoDeX/a.lang",
        "TARGET/a.lang",
        ".lang-package.json/a.lang",
        "src/あ.lang",
        "a.txt",
    ] {
        let mut meta = base.clone();
        meta.files[0].path = path.into();
        meta.package.entry = path.into();
        refused(&archive(&json(&meta), &sources), ErrorKind::Malformed);
    }
    for paths in [
        ["Src/a.lang", "src/b.lang"],
        ["a.lang", "a.lang/b.lang"],
        ["A.lang", "a.lang"],
        ["same.lang", "same.lang"],
    ] {
        let files: Vec<_> = paths
            .iter()
            .map(|p| (p.to_string(), SOURCE.to_vec()))
            .collect();
        refused(
            &archive(&json(&metadata(&files)), &files),
            ErrorKind::Malformed,
        );
    }
    let mut meta = base.clone();
    meta.files.push(meta.files[0].clone());
    refused(&archive(&json(&meta), &sources), ErrorKind::Malformed);
    let path = format!("{}.lang", "x".repeat(95));
    let files = vec![(path.clone(), SOURCE.to_vec())];
    assert!(AdmittedArchive::admit(&archive(&json(&metadata(&files)), &files)).is_ok());
    let mut meta = metadata(&files);
    meta.files[0].path.insert(0, 'x');
    meta.package.entry = meta.files[0].path.clone();
    refused(&archive(&json(&meta), &files), ErrorKind::Malformed);
}

#[test]
fn closed_canonical_metadata_rejects_duplicates_unknowns_and_missing_roots() {
    let (base, sources) = simple();
    let original = String::from_utf8(json(&base)).unwrap();
    for bad in [
        original.replace("\"schema\": 1,", "\"schema\": 1,\n  \"schema\": 1,"),
        original.replace("\"schema\": 1,", "\"schema\": 1,\n  \"extra\": false,"),
        original.replace("\"schema\": 1", "\"schema\": 1.0"),
        original.replace("\"schema\": 1", "\"schema\": 2"),
        original.replace("  \"schema\"", " \"schema\""),
        original.trim_end().to_string(),
    ] {
        refused(&archive(bad.as_bytes(), &sources), ErrorKind::Malformed);
    }
    for kind in 0..7 {
        let mut meta = base.clone();
        match kind {
            0 => meta.package.name = "Bad".into(),
            1 => meta.package.version = "01.0.0".into(),
            2 => meta.package.edition = "2027".into(),
            3 => meta.package.entry = "missing.lang".into(),
            4 => meta.tests = vec!["missing.lang".into()],
            5 => meta.tests = vec!["src/main.lang".into(), "src/main.lang".into()],
            _ => meta.files.clear(),
        }
        refused(&archive(&json(&meta), &sources), ErrorKind::Malformed);
    }
}

#[test]
fn exact_byte_record_depth_and_source_aggregate_limits_are_observable() {
    let (base, sources) = simple();
    for length in [MAX_DESCRIPTOR_BYTES - 1, MAX_DESCRIPTOR_BYTES] {
        refused(&archive(&vec![b' '; length], &[]), ErrorKind::Malformed);
    }
    refused(
        &archive(&vec![b' '; MAX_DESCRIPTOR_BYTES + 1], &[]),
        ErrorKind::Limit,
    );
    for length in [MAX_ARCHIVE_BYTES - 1, MAX_ARCHIVE_BYTES] {
        refused(&vec![0; length], ErrorKind::Malformed);
    }
    refused(&vec![0; MAX_ARCHIVE_BYTES + 1], ErrorKind::Limit);
    for depth in [15, 16, 17] {
        let nested = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        refused(
            &archive(nested.as_bytes(), &[]),
            if depth == 17 {
                ErrorKind::Limit
            } else {
                ErrorKind::Malformed
            },
        );
    }
    for count in [MAX_FILES - 1, MAX_FILES, MAX_FILES + 1] {
        let files: Vec<_> = (0..count)
            .map(|i| (format!("src/f{i:03}.lang"), vec![b' ']))
            .collect();
        let encoded = archive(&json(&metadata(&files)), &files);
        if count <= MAX_FILES {
            assert_eq!(
                AdmittedArchive::admit(&encoded)
                    .unwrap()
                    .descriptor()
                    .files()
                    .len(),
                count
            );
        } else {
            refused(&encoded, ErrorKind::Limit);
        }
    }
    for size in [MAX_SOURCE_BYTES - 1, MAX_SOURCE_BYTES] {
        let files = vec![("main.lang".into(), vec![b' '; size])];
        assert!(AdmittedArchive::admit(&archive(&json(&metadata(&files)), &files)).is_ok());
    }
    let mut oversized = base.clone();
    oversized.files[0].bytes = (MAX_SOURCE_BYTES + 1) as u64;
    refused(&archive(&json(&oversized), &sources), ErrorKind::Limit);
    let files: Vec<_> = (0..16)
        .map(|i| (format!("f{i:02}.lang"), vec![b' '; MAX_SOURCE_BYTES]))
        .collect();
    let mut meta = metadata(&files);
    assert!(AdmittedArchive::admit(&archive(&json(&meta), &files)).is_ok());
    meta.files.push(File {
        path: "z.lang".into(),
        bytes: 1,
        sha256: hash(b" "),
    });
    refused(&archive(&json(&meta), &[]), ErrorKind::Limit);
}

#[test]
fn declared_test_and_target_counts_and_observed_payload_sizes_are_bounded() {
    let files: Vec<_> = (0..129)
        .map(|i| (format!("src/f{i:03}.lang"), vec![b' ']))
        .collect();
    for count in [127, 128, 129] {
        let mut meta = metadata(&files);
        meta.tests = files[..count]
            .iter()
            .map(|(path, _)| path.clone())
            .collect();
        let bytes = archive(&json(&meta), &files);
        if count <= 128 {
            assert_eq!(
                AdmittedArchive::admit(&bytes)
                    .unwrap()
                    .descriptor()
                    .tests()
                    .len(),
                count
            );
        } else {
            refused(&bytes, ErrorKind::Limit);
        }
    }
    let (mut meta, sources) = simple();
    meta.dependencies = vec![edge("helper".into())];
    meta.dependencies[0].targets.push("unknown".into());
    refused(&archive(&json(&meta), &sources), ErrorKind::Limit);

    let (meta, sources) = simple();
    let mut bytes = archive(&json(&meta), &sources);
    let start = 1024;
    bytes[start + 124..start + 136]
        .copy_from_slice(format!("{:011o}\0", MAX_SOURCE_BYTES + 1).as_bytes());
    bytes[start + 148..start + 156].fill(b' ');
    let checksum: usize = bytes[start..start + 512]
        .iter()
        .map(|b| usize::from(*b))
        .sum();
    bytes[start + 148..start + 156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
    assert_eq!(refused(&bytes, ErrorKind::Limit).offset, Some(start));
}

fn edge(alias: String) -> Edge {
    Edge {
        alias,
        registry: "public".into(),
        name: "helper".into(),
        version: "0.1.0".into(),
        archive_sha256: "a".repeat(64),
        targets: vec![
            "aarch64-apple-darwin".into(),
            "x86_64-pc-windows-msvc".into(),
        ],
    }
}

#[test]
fn dependencies_preserve_explicit_origin_and_reject_conflicting_versions_or_bounds() {
    let (base, sources) = simple();
    for count in [127, 128, 129] {
        let mut meta = base.clone();
        meta.dependencies = (0..count).map(|i| edge(format!("d{i:03}"))).collect();
        let bytes = archive(&json(&meta), &sources);
        if count == 129 {
            refused(&bytes, ErrorKind::Limit);
        } else {
            let archive = AdmittedArchive::admit(&bytes).unwrap();
            let d = &archive.descriptor().dependencies()[0];
            assert_eq!(d.registry(), "public");
            assert_eq!(d.name(), "helper");
            assert_eq!(d.version(), "0.1.0");
            assert_eq!(d.alias(), "d000");
            assert_eq!(d.archive_sha256(), "a".repeat(64));
            assert_eq!(d.targets().len(), 2);
        }
    }
    let mut meta = base.clone();
    meta.dependencies = vec![edge("a".into()), edge("b".into())];
    meta.dependencies[1].archive_sha256 = "b".repeat(64);
    refused(&archive(&json(&meta), &sources), ErrorKind::Integrity);
    meta.dependencies[1].registry = "another".into();
    assert!(AdmittedArchive::admit(&archive(&json(&meta), &sources)).is_ok());
    for kind in 0..5 {
        let mut meta = base.clone();
        meta.dependencies = vec![edge("a".into())];
        match kind {
            0 => meta.dependencies[0].targets.clear(),
            1 => meta.dependencies[0].targets.reverse(),
            2 => meta.dependencies[0].targets[0] = "unknown".into(),
            3 => meta.dependencies[0].version = "4294967296.0.0".into(),
            _ => meta.dependencies[0].archive_sha256 = "A".repeat(64),
        }
        refused(&archive(&json(&meta), &sources), ErrorKind::Malformed);
    }
}

#[test]
fn deterministic_mutation_corpus_has_no_panics_and_never_mutates_input() {
    let (meta, sources) = simple();
    let good = archive(&json(&meta), &sources);
    for i in 0..512 {
        let mut candidate = good.clone();
        let index = (i * 137 + 17) % candidate.len();
        candidate[index] ^= (i % 255 + 1) as u8;
        let original = candidate.clone();
        let _ = AdmittedArchive::admit(&candidate);
        assert_eq!(candidate, original);
    }
}
