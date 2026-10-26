#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
use lang::native::{component as c, link, manifest as m};
use lang_codegen_clif::{native_record, sdk};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
#[path = "support/component_object_program.rs"]
mod fixture;

struct Dir(PathBuf);
#[test]
fn producer_respects_record_framing_limits_before_publication() {
    let semantic = fixture::artifact();
    fixture::with_artifact(&semantic, |program, _, entry| {
        let result = c::compile(
            &semantic,
            program,
            entry,
            lang_codegen_clif::Triple::host(),
            c::Limits {
                record: native_record::Limits {
                    bytes: 0,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert!(matches!(result, Err(c::Error::Record(_))));
    });
}
impl Dir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang component packages {}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn package(semantic: &[u8]) -> lang::native::Package {
    fixture::with_artifact(semantic, |program, _, entry| {
        c::compile(
            semantic,
            program,
            entry,
            lang_codegen_clif::Triple::host(),
            Default::default(),
        )
        .unwrap()
    })
}
fn sdk_fixture(dir: &Dir, version: u32) -> PathBuf {
    let root = dir.0.join(format!("sdk-{version}"));
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir(root.join("lib")).unwrap();
    let files: Vec<_> = [
        ("compiler", "bin/lang", b"compiler".as_slice()),
        ("runtime", "lib/runtime.rlib", b"runtime".as_slice()),
    ]
    .into_iter()
    .map(|(role, path, bytes)| {
        fs::write(root.join(path), bytes).unwrap();
        json!({"role":role,"path":path,"bytes":bytes.len(),"sha256":sdk::digest(bytes)})
    })
    .collect();
    let target = lang_codegen_clif::Triple::host();
    fs::write(root.join("sdk.json"),serde_json::to_vec(&json!({"format":"lang-native-sdk","version":version,"target":target.to_string(),"build_id":sdk::build_id(&target),"rustc":{"release":sdk::RUSTC_RELEASE,"commit":sdk::RUSTC_COMMIT},"build_profile":sdk::BUILD_PROFILE,"feature_policy":sdk::FEATURE_POLICY,"files":files})).unwrap()).unwrap();
    root
}

#[test]
fn strict_component_revision_roles_and_export_are_independent_from_script_entry() {
    let semantic = fixture::artifact();
    assert!(
        lang::artifact::component::decode(&semantic, Default::default())
            .unwrap()
            .entry()
            .is_none()
    );
    let p = package(&semantic);
    let bytes = &p
        .files()
        .iter()
        .find(|f| f.role == "manifest")
        .unwrap()
        .bytes;
    let original: Value = serde_json::from_slice(bytes).unwrap();
    let parsed = m::parse_program(bytes, Default::default()).unwrap();
    assert_eq!(parsed.version(), 3);
    assert_eq!(parsed.profile(), m::Profile::Component);
    assert!(parsed.entry().is_some());
    assert_eq!(parsed.files().len(), 4);
    for version in [1, 2, 4] {
        let mut bad = original.clone();
        bad["version"] = json!(version);
        assert!(m::parse_program(&serde_json::to_vec(&bad).unwrap(), Default::default()).is_err());
    }
    for profile in ["copy", "owned", "c", "task"] {
        let mut bad = original.clone();
        bad["profile"] = json!(profile);
        assert!(m::parse_program(&serde_json::to_vec(&bad).unwrap(), Default::default()).is_err());
    }
    let mut bad = original.clone();
    bad["entry"] = Value::Null;
    assert!(m::parse_program(&serde_json::to_vec(&bad).unwrap(), Default::default()).is_err());
    for role in ["main", "c_header"] {
        let mut bad = original.clone();
        let mut extra = bad["files"][0].clone();
        extra["role"] = json!(role);
        extra["path"] = json!(if role == "main" {
            "main.rs"
        } else {
            "program.h"
        });
        bad["files"].as_array_mut().unwrap().push(extra);
        assert!(m::parse_program(&serde_json::to_vec(&bad).unwrap(), Default::default()).is_err());
    }
}

#[test]
fn complete_semantic_correspondence_and_preparation_refuse_before_resolution() {
    let semantic = fixture::artifact();
    let p = package(&semantic);
    let native = &p
        .files()
        .iter()
        .find(|f| f.role == "native_record")
        .unwrap()
        .bytes;
    let target = lang_codegen_clif::Triple::host();
    let target_text = target.to_string();
    let build = sdk::build_id(&target);
    let mut different = semantic.clone();
    let at = different
        .windows(b"return value+1".len())
        .position(|s| s == b"return value+1")
        .unwrap();
    different[at + b"return value+".len()] = b'2';
    // Only the stored source snapshot changes; independent decoding succeeds.
    lang::artifact::component::decode(&different, Default::default()).unwrap();
    fixture::with_artifact(&semantic, |program, _, entry| {
        assert!(matches!(
            c::compile(
                &different,
                program,
                entry,
                target.clone(),
                Default::default()
            ),
            Err(c::Error::Correspondence)
        ));
        assert!(matches!(
            c::compile(
                &semantic,
                program,
                lang_mir::unit::FunctionId(u32::MAX),
                target.clone(),
                Default::default()
            ),
            Err(c::Error::Entry)
        ));
        let other: lang_codegen_clif::Triple = if cfg!(windows) {
            "aarch64-apple-darwin"
        } else {
            "x86_64-pc-windows-msvc"
        }
        .parse()
        .unwrap();
        assert!(matches!(
            c::compile(&semantic, program, entry, other, Default::default()),
            Err(c::Error::Target { .. })
        ));
        let input = sdk::runtime::Input {
            target: &target_text,
            build_id: &build,
            semantic: &semantic,
            native,
        };
        let calls = std::cell::Cell::new(0);
        let refuse = |input, limits| {
            // SAFETY: every case must refuse before resolving any address.
            unsafe {
                c::bind(
                    input,
                    program,
                    (),
                    |_, _| {
                        calls.set(calls.get() + 1);
                        None
                    },
                    limits,
                )
            }
            .err()
            .unwrap()
        };
        assert!(matches!(
            refuse(
                sdk::runtime::Input {
                    semantic: &different,
                    ..input
                },
                Default::default()
            ),
            c::Error::Correspondence
        ));
        assert!(matches!(
            refuse(
                input,
                c::Limits {
                    plans: lang_mir::unit::Limits {
                        work: 0,
                        ..Default::default()
                    },
                    ..Default::default()
                }
            ),
            c::Error::Preparation(_)
        ));
        let mut bad = native.clone();
        bad[17] = 0;
        assert!(matches!(
            native_record::Record::decode(&bad, &semantic, Default::default())
                .unwrap()
                .kind(),
            native_record::Kind::Task(_)
        ));
        assert!(matches!(
            refuse(
                sdk::runtime::Input {
                    native: &bad,
                    ..input
                },
                Default::default()
            ),
            c::Error::Entry
        ));
        let mut bad = native.clone();
        bad[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            refuse(
                sdk::runtime::Input {
                    native: &bad,
                    ..input
                },
                Default::default()
            ),
            c::Error::Entry
        ));
        assert!(matches!(
            refuse(
                sdk::runtime::Input {
                    build_id: "wrong",
                    ..input
                },
                Default::default()
            ),
            c::Error::Build { .. }
        ));
        let mut bad = native.clone();
        let length = u32::from_le_bytes(bad[86..90].try_into().unwrap()) as usize;
        let symbol = 90 + length + 12;
        bad[symbol] = b'x';
        assert!(matches!(
            refuse(
                sdk::runtime::Input {
                    native: &bad,
                    ..input
                },
                Default::default()
            ),
            c::Error::Plan(_)
        ));
        assert!(matches!(
            refuse(
                sdk::runtime::Input {
                    semantic: b"malformed",
                    ..input
                },
                Default::default()
            ),
            c::Error::Artifact(_)
        ));
        assert_eq!(calls.get(), 0);
    });
}

#[test]
fn snapshots_require_component_sdk_three_and_explicit_host_before_tools() {
    let dir = Dir::new();
    let semantic = fixture::artifact();
    let root = dir.0.join("package");
    package(&semantic).publish(&root).unwrap();
    let stage = dir.0.join("stage");
    fs::create_dir(&stage).unwrap();
    for version in [1, 2, 3] {
        let sdk_root = sdk_fixture(&dir, version);
        let sdk = m::SdkSnapshot::open(&sdk_root, &stage, Default::default()).unwrap();
        let result = m::ProgramSnapshot::open(&root, &stage, &sdk, Default::default());
        if version == 3 {
            assert_eq!(result.unwrap().manifest().profile(), m::Profile::Component);
        } else {
            assert!(matches!(
                result.err().unwrap().kind,
                m::ErrorKind::Mismatch {
                    field: "component SDK version",
                    ..
                }
            ));
        }
        let request = link::Request {
            rustc: Some(dir.0.join("must-not-start-rustc").into()),
            sdk: sdk_root,
            package: root.clone(),
            host: None,
            objects: vec![],
            output: dir.0.join(format!("app-{version}")),
        };
        let failure = link::link(&request, Default::default()).unwrap_err();
        if version == 3 {
            assert!(matches!(failure, link::Error::HostRequired));
        } else {
            assert!(matches!(failure, link::Error::Manifest(_)));
        }
        assert!(!request.output.exists());
    }
    let sdk = m::SdkSnapshot::open(&dir.0.join("sdk-3"), &stage, Default::default()).unwrap();
    let original: Value =
        serde_json::from_slice(&fs::read(root.join("package.json")).unwrap()).unwrap();
    let mut bad = original.clone();
    bad["entry"] = json!(u32::MAX);
    fs::write(root.join("package.json"), serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(m::ProgramSnapshot::open(&root, &stage, &sdk, Default::default()).is_err());
    fs::write(
        root.join("package.json"),
        serde_json::to_vec(&original).unwrap(),
    )
    .unwrap();
    assert!(m::ProgramSnapshot::open(&root, &stage, &sdk, Default::default()).is_ok());
}
