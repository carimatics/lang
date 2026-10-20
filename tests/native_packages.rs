#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
use lang::native::manifest::{self as m, Limits, ProgramSnapshot, SdkSnapshot};
use lang_codegen_clif::sdk;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "lang package verification {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn target() -> &'static str {
    if cfg!(windows) {
        "x86_64-pc-windows-msvc"
    } else {
        "aarch64-apple-darwin"
    }
}
fn component(role: &str, path: &str, bytes: &[u8]) -> Value {
    json!({"role":role,"path":path,"bytes":bytes.len(),"sha256":sdk::digest(bytes)})
}
fn sdk_manifest() -> Value {
    json!({"format":"lang-native-sdk","version":1,"build_id":sdk::build_id(&target().parse().unwrap()),"target":target(),"rustc":{"release":sdk::RUSTC_RELEASE,"commit":sdk::RUSTC_COMMIT},"build_profile":sdk::BUILD_PROFILE,"feature_policy":sdk::FEATURE_POLICY,"files":[component("compiler","bin/lang",b"compiler fixture"),component("runtime","lib/runtime.rlib",b"runtime fixture")]})
}
fn write_manifest(root: &Path, name: &str, v: &Value) {
    fs::write(root.join(name), serde_json::to_vec(v).unwrap()).unwrap();
}
fn sdk_fixture(dir: &Dir) -> (PathBuf, PathBuf) {
    let root = dir.0.join("sdk");
    let stages = dir.0.join("stages");
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::create_dir(&stages).unwrap();
    fs::write(root.join("bin/lang"), b"compiler fixture").unwrap();
    fs::write(root.join("lib/runtime.rlib"), b"runtime fixture").unwrap();
    write_manifest(&root, "sdk.json", &sdk_manifest());
    (root, stages)
}
fn count(path: &Path) -> usize {
    fs::read_dir(path).unwrap().count()
}
fn program(dir: &Dir, name: &str, text: &str) -> PathBuf {
    let input = dir.0.join(format!("{name}.lang"));
    fs::write(&input, text).unwrap();
    let package = dir.0.join(name);
    let out = Command::new(env!("CARGO_BIN_EXE_lang"))
        .arg("native")
        .arg("--output")
        .arg(&package)
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    package
}

#[test]
fn manifest_exact_fields_paths_roles_and_bounds_are_enforced() {
    let base = sdk_manifest();
    let encoded = serde_json::to_vec(&base).unwrap();
    m::parse_sdk(&encoded, Limits::default()).unwrap();
    let mut limits = Limits {
        sdk_manifest: encoded.len(),
        ..Default::default()
    };
    m::parse_sdk(&encoded, limits).unwrap();
    limits.sdk_manifest -= 1;
    assert!(m::parse_sdk(&encoded, limits).is_err());
    for path in [
        "../outside",
        "/absolute",
        "C:ads",
        "dir\\file",
        "a//b",
        "a/./b",
        "a/../b",
        "NUL.txt",
        "aux",
        "COM¹.txt",
        "Lpt9.log",
        "file.",
        "file ",
        "x\0y",
        "sdk.json",
        "a?b",
    ] {
        let mut v = base.clone();
        v["files"][0]["path"] = json!(path);
        assert!(
            m::parse_sdk(&serde_json::to_vec(&v).unwrap(), Limits::default()).is_err(),
            "{path}"
        );
    }
    for mutate in [0, 1, 2, 3, 4, 5, 6, 7, 8] {
        let mut v = base.clone();
        match mutate {
            0 => v["unknown"] = json!(0),
            1 => v["rustc"]["unknown"] = json!(0),
            2 => v["files"][0]["unknown"] = json!(0),
            3 => v["files"][1]["role"] = json!("compiler"),
            4 => v["files"][1]["path"] = json!("BIN/LANG"),
            5 => v["files"][1]["role"] = json!("object"),
            6 => v["build_id"] = json!("A".repeat(64)),
            7 => v["files"][0]["bytes"] = json!(2u64 * 1024 * 1024 * 1024 + 1),
            8 => v["feature_policy"] = json!("not an identifier"),
            _ => unreachable!(),
        }
        assert!(
            m::parse_sdk(&serde_json::to_vec(&v).unwrap(), Limits::default()).is_err(),
            "case {mutate}"
        );
    }
    for (old, new) in [
        ("\"version\":1", "\"version\":1,\"version\":1"),
        ("\"release\":", "\"release\":\"fake\",\"release\":"),
        (
            "\"role\":\"compiler\"",
            "\"role\":\"compiler\",\"role\":\"compiler\"",
        ),
    ] {
        let text = String::from_utf8(encoded.clone()).unwrap();
        let duplicate = text.replace(old, new);
        assert_ne!(text, duplicate);
        assert!(m::parse_sdk(duplicate.as_bytes(), Limits::default()).is_err());
    }
    for end in 0..encoded.len() {
        assert!(m::parse_sdk(&encoded[..end], Limits::default()).is_err());
    }
}
#[test]
fn sdk_streamed_snapshot_is_independent_and_identity_refuses_before_staging() {
    let dir = Dir::new();
    let (root, stages) = sdk_fixture(&dir);
    for field in [
        "build_id",
        "target",
        "build_profile",
        "feature_policy",
        "rustc",
    ] {
        let mut v = sdk_manifest();
        match field {
            "build_id" => v[field] = json!("0".repeat(64)),
            "target" => {
                v[field] = json!(if cfg!(windows) {
                    "aarch64-apple-darwin"
                } else {
                    "x86_64-pc-windows-msvc"
                })
            }
            "build_profile" => {
                v[field] = json!(if sdk::BUILD_PROFILE == "debug" {
                    "release"
                } else {
                    "debug"
                })
            }
            "feature_policy" => v[field] = json!("other"),
            _ => v["rustc"]["commit"] = json!("unknown"),
        }
        write_manifest(&root, "sdk.json", &v);
        assert!(
            SdkSnapshot::open(&root, &stages, Limits::default()).is_err(),
            "{field}"
        );
        assert_eq!(count(&stages), 0);
    }
    write_manifest(&root, "sdk.json", &sdk_manifest());
    let manifest_size = fs::metadata(root.join("sdk.json")).unwrap().len();
    let total = manifest_size
        + fs::metadata(root.join("bin/lang")).unwrap().len()
        + fs::metadata(root.join("lib/runtime.rlib")).unwrap().len();
    let mut limits = Limits {
        sdk_total: total,
        ..Default::default()
    };
    let snapshot = SdkSnapshot::open(&root, &stages, limits).unwrap();
    let copied = snapshot.root().to_owned();
    assert_eq!(count(&stages), 1);
    limits.sdk_total -= 1;
    assert!(SdkSnapshot::open(&root, &stages, limits).is_err());
    assert_eq!(count(&stages), 1);
    fs::write(root.join("bin/lang"), b"untrusted change").unwrap();
    assert_eq!(
        fs::read(copied.join("bin/lang")).unwrap(),
        b"compiler fixture"
    );
    assert!(SdkSnapshot::open(&root, &stages, Limits::default()).is_err());
    assert_eq!(count(&stages), 1);
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(
        fs::read(copied.join("lib/runtime.rlib")).unwrap(),
        b"runtime fixture"
    );
    drop(snapshot);
    assert!(!copied.exists());
    assert_eq!(count(&stages), 0);
}
#[test]
fn compiled_profiles_become_verified_source_free_program_snapshots() {
    let dir = Dir::new();
    let (root, stages) = sdk_fixture(&dir);
    let sdk = SdkSnapshot::open(&root, &stages, Limits::default()).unwrap();
    for (name, source, profile) in [
        (
            "copy",
            "fn first()->i64{return 0;} fn main()->i64{return 42;}",
            m::Profile::Copy,
        ),
        (
            "owned",
            include_str!("../docs/experiments/sdk-owned.lang"),
            m::Profile::Owned,
        ),
        (
            "c",
            include_str!("../examples/analysis/c_owned_exports.lang"),
            m::Profile::C,
        ),
    ] {
        let package = program(&dir, name, source);
        let snapshot = ProgramSnapshot::open(&package, &stages, &sdk, Limits::default()).unwrap();
        assert_eq!(snapshot.manifest().profile(), profile);
        let semantic = fs::read(package.join("program.lbc")).unwrap();
        assert_eq!(snapshot.semantic(), semantic);
        let native = fs::read(package.join("program.native")).unwrap();
        assert_eq!(snapshot.native_record(), native);
        fs::remove_dir_all(package).unwrap();
        fs::remove_file(dir.0.join(format!("{name}.lang"))).unwrap();
        assert_eq!(
            fs::read(snapshot.root().join("program.lbc")).unwrap(),
            semantic
        );
        let copied = snapshot.root().to_owned();
        drop(snapshot);
        assert!(!copied.exists());
        assert_eq!(count(&stages), 1);
    }
}
#[test]
fn program_role_entry_hash_and_original_object_mismatches_roll_back() {
    let dir = Dir::new();
    let (root, stages) = sdk_fixture(&dir);
    let sdk = SdkSnapshot::open(&root, &stages, Limits::default()).unwrap();
    let package = program(&dir, "copy", "fn main()->i64{return 42;}");
    let base: Value =
        serde_json::from_slice(&fs::read(package.join("package.json")).unwrap()).unwrap();
    for case in 0..9 {
        let mut v = base.clone();
        match case {
            0 => {
                v.as_object_mut().unwrap().remove("entry");
            }
            1 => v["entry"] = Value::Null,
            2 => v["entry"] = json!(100),
            3 => v["profile"] = json!("owned"),
            4 => v["build_id"] = json!("0".repeat(64)),
            5 => v["files"][0]["sha256"] = json!("0".repeat(64)),
            6 => v["files"][0]["path"] = json!("wrong.lbc"),
            7 => v["files"][0]["bytes"] = json!(0),
            8 => v["entry"] = json!(-1),
            _ => unreachable!(),
        }
        write_manifest(&package, "package.json", &v);
        assert!(
            ProgramSnapshot::open(&package, &stages, &sdk, Limits::default()).is_err(),
            "{case}"
        );
        assert_eq!(count(&stages), 1);
    }
    let mut v = base;
    let file = v["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["role"] == "object")
        .unwrap();
    let name = file["path"].as_str().unwrap();
    let mut bytes = fs::read(package.join(name)).unwrap();
    bytes[0] ^= 1;
    fs::write(package.join(name), &bytes).unwrap();
    file["sha256"] = json!(sdk::digest(&bytes));
    write_manifest(&package, "package.json", &v);
    let failure = ProgramSnapshot::open(&package, &stages, &sdk, Limits::default())
        .err()
        .expect("different original object");
    assert!(matches!(
        failure.kind,
        m::ErrorKind::Invalid {
            field: "original object/record bytes"
        }
    ));
    assert_eq!(count(&stages), 1);
}
#[test]
fn sdk_symlink_or_junction_escape_is_refused_without_a_snapshot() {
    let dir = Dir::new();
    let (root, stages) = sdk_fixture(&dir);
    let outside = dir.0.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("runtime.rlib"), b"runtime fixture").unwrap();
    let link = root.join("escape");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    #[cfg(windows)]
    {
        let out = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let mut v = sdk_manifest();
    v["files"][1]["path"] = json!("escape/runtime.rlib");
    write_manifest(&root, "sdk.json", &v);
    assert!(SdkSnapshot::open(&root, &stages, Limits::default()).is_err());
    assert_eq!(count(&stages), 0);
    assert_eq!(
        fs::read(outside.join("runtime.rlib")).unwrap(),
        b"runtime fixture"
    );
}

#[test]
fn sdk_table_and_utf8_path_limits_admit_exact_format_bounds() {
    let mut v = sdk_manifest();
    for i in 0..4094 {
        v["files"].as_array_mut().unwrap().push(component(
            "dependency",
            &format!("lib/x{i}.rlib"),
            b"",
        ));
    }
    m::parse_sdk(&serde_json::to_vec(&v).unwrap(), Limits::default()).unwrap();
    v["files"]
        .as_array_mut()
        .unwrap()
        .push(component("dependency", "lib/extra.rlib", b""));
    assert!(m::parse_sdk(&serde_json::to_vec(&v).unwrap(), Limits::default()).is_err());
    let mut v = sdk_manifest();
    let exact = "é/".repeat(1365) + "x";
    assert_eq!(exact.len(), 4096);
    v["files"][0]["path"] = json!(exact);
    m::parse_sdk(&serde_json::to_vec(&v).unwrap(), Limits::default()).unwrap();
    v["files"][0]["path"] = json!(exact + "x");
    assert!(m::parse_sdk(&serde_json::to_vec(&v).unwrap(), Limits::default()).is_err());
}

#[test]
fn sdk_stream_checks_bytes_beyond_a_chunk_and_cleans_up_on_hash_failure() {
    let dir = Dir::new();
    let (root, stages) = sdk_fixture(&dir);
    let mut data: Vec<u8> = (0..65549).map(|n| (n % 251) as u8).collect();
    fs::write(root.join("lib/large.rlib"), &data).unwrap();
    let mut v = sdk_manifest();
    v["files"]
        .as_array_mut()
        .unwrap()
        .push(component("dependency", "lib/large.rlib", &data));
    write_manifest(&root, "sdk.json", &v);
    let mut limits = Limits {
        sdk_file: data.len() as u64,
        ..Default::default()
    };
    let snapshot = SdkSnapshot::open(&root, &stages, limits).unwrap();
    assert_eq!(
        fs::read(snapshot.root().join("lib/large.rlib")).unwrap(),
        data
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(snapshot.root()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    limits.sdk_file -= 1;
    assert!(SdkSnapshot::open(&root, &stages, limits).is_err());
    assert_eq!(count(&stages), 1);
    data[65536] ^= 1;
    fs::write(root.join("lib/large.rlib"), data).unwrap();
    let err = SdkSnapshot::open(&root, &stages, Limits::default())
        .err()
        .unwrap();
    assert!(matches!(
        err.kind,
        m::ErrorKind::Mismatch {
            field: "component SHA-256",
            ..
        }
    ));
    assert!(err.path.ends_with("lib/large.rlib"));
    assert_eq!(count(&stages), 1);
}

#[test]
fn concurrent_source_changes_cannot_publish_unverified_snapshot_bytes() {
    use std::sync::{Arc, Barrier};
    let dir = Dir::new();
    let (root, stages) = sdk_fixture(&dir);
    let source = root.join("bin/lang");
    let start = Arc::new(Barrier::new(2));
    let writer_start = start.clone();
    let writer_path = source.clone();
    let writer = std::thread::spawn(move || {
        writer_start.wait();
        for i in 0..200 {
            fs::write(
                &writer_path,
                if i % 2 == 0 {
                    b"compiler fixture"
                } else {
                    b"modified fixture"
                },
            )
            .unwrap();
        }
    });
    start.wait();
    for _ in 0..40 {
        if let Ok(snapshot) = SdkSnapshot::open(&root, &stages, Limits::default()) {
            assert_eq!(
                fs::read(snapshot.root().join("bin/lang")).unwrap(),
                b"compiler fixture"
            );
        }
        assert_eq!(count(&stages), 0);
    }
    writer.join().unwrap();
    fs::write(source, b"compiler fixture").unwrap();
    drop(SdkSnapshot::open(&root, &stages, Limits::default()).unwrap());
    assert_eq!(count(&stages), 0);
}

#[test]
fn independently_valid_native_record_from_another_program_is_rejected() {
    let dir = Dir::new();
    let (root, stages) = sdk_fixture(&dir);
    let sdk = SdkSnapshot::open(&root, &stages, Limits::default()).unwrap();
    let a = program(&dir, "a", "fn main()->i64{return 42;}");
    let b = program(&dir, "b", "fn main()->i64{return 7;}");
    let bytes = fs::read(b.join("program.native")).unwrap();
    fs::write(a.join("program.native"), &bytes).unwrap();
    let mut v: Value = serde_json::from_slice(&fs::read(a.join("package.json")).unwrap()).unwrap();
    let f = v["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["role"] == "native_record")
        .unwrap();
    f["sha256"] = json!(sdk::digest(&bytes));
    f["bytes"] = json!(bytes.len());
    write_manifest(&a, "package.json", &v);
    let failure = ProgramSnapshot::open(&a, &stages, &sdk, Limits::default())
        .err()
        .unwrap();
    assert!(matches!(
        failure.kind,
        m::ErrorKind::Record(lang_codegen_clif::native_record::Error {
            kind: lang_codegen_clif::native_record::ErrorKind::SemanticDigest,
            ..
        })
    ));
    assert!(failure.path.ends_with("program.native"));
    assert_eq!(count(&stages), 1);
}
