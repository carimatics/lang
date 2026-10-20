//! SDK assembly boundaries use small inputs; one real Cargo selection proves
//! the installed compiler, selected Rust libraries and ownership startup together.
#![cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
use lang::native::{
    manifest::{self as m, Limits},
    sdk_package as s,
};
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
        let path = std::env::temp_dir().join(format!(
            "lang SDK assembly-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
fn artifact(name: &str, kind: &str, files: &[PathBuf], exe: Option<&Path>) -> Value {
    json!({"reason":"compiler-artifact","target":{"name":name,"kind":[kind]},"profile":{"test":false},"filenames":files,"executable":exe})
}
fn fixture(dir: &Dir) -> (Vec<Value>, PathBuf) {
    let root = dir.0.join("build");
    fs::create_dir(&root).unwrap();
    let compiler = root.join(if cfg!(windows) { "lang.exe" } else { "lang" });
    fs::write(&compiler, b"compiler fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o751)).unwrap();
    }
    let runtime = root.join("liblang_codegen_clif-test.rlib");
    fs::write(&runtime, b"runtime fixture").unwrap();
    let dependency = root.join("liblang_runtime-test.rlib");
    fs::write(&dependency, b"dependency fixture").unwrap();
    let proc_macro = root.join(if cfg!(windows) {
        "derive-test.dll"
    } else {
        "libderive-test.dylib"
    });
    fs::write(&proc_macro, b"proc-macro fixture").unwrap();
    let native = root.join("native");
    fs::create_dir(&native).unwrap();
    fs::write(
        native.join("windows.0.52.0.lib"),
        b"Windows native import archive",
    )
    .unwrap();
    fs::write(
        native.join(if cfg!(windows) {
            "vendor.lib"
        } else {
            "libvendor.a"
        }),
        b"static native archive",
    )
    .unwrap();
    let records = vec![
        artifact(
            "lang",
            "bin",
            std::slice::from_ref(&compiler),
            Some(&compiler),
        ),
        artifact("lang_codegen_clif", "lib", &[runtime], None),
        artifact("lang_runtime", "lib", &[dependency], None),
        artifact("derive", "proc-macro", &[proc_macro], None),
        json!({"reason":"build-script-executed","linked_paths":[format!("native={}",native.display()),format!("native={}",native.display())],"linked_libs":["static=vendor"]}),
        json!({"reason":"build-finished","success":true}),
    ];
    (records, root)
}
fn encode(records: &[Value]) -> Vec<u8> {
    records
        .iter()
        .map(|v| serde_json::to_string(v).unwrap() + "\n")
        .collect::<String>()
        .into_bytes()
}
fn no_stages(dir: &Dir) {
    assert!(!fs::read_dir(&dir.0).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lang-")
    }));
}

#[test]
fn assembly_keeps_native_proc_macro_resources_permissions_and_verifiable_independent_files() {
    let dir = Dir::new();
    let (records, build) = fixture(&dir);
    let output = dir.0.join("sdk");
    let mut bytes = b"ordinary build-tool output\n".to_vec();
    bytes.extend(encode(&records));
    let sdk = s::assemble(&bytes, &output, Limits::default()).unwrap();
    assert_eq!(
        sdk.files()
            .iter()
            .filter(|f| f.role() == m::Role::Runtime)
            .count(),
        1
    );
    assert!(output.join("native/windows.0.52.0.lib").is_file());
    assert!(
        sdk.files()
            .iter()
            .any(|f| f.path().ends_with("derive-test.dll")
                || f.path().ends_with("derive-test.dylib"))
    );
    for file in [
        "startup/copy.rs",
        "startup/owned.rs",
        "startup/c-host.rs",
        "docs/native-sdk.md",
        "docs/ja/native-sdk.md",
        "include/lang/runtime.h",
    ] {
        assert!(output.join(file).is_file());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = sdk
            .files()
            .iter()
            .find(|f| f.role() == m::Role::Compiler)
            .unwrap();
        assert_eq!(
            fs::metadata(output.join(file.path()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o751
        );
    }
    fs::remove_dir_all(build).unwrap();
    let snapshot = m::SdkSnapshot::open(&output, &dir.0, Limits::default()).unwrap();
    assert_eq!(snapshot.manifest().files().len(), sdk.files().len());
    drop(snapshot);
    no_stages(&dir);
    // The packaged common header is independently usable C11, including paths with spaces.
    let source = dir.0.join("header.c");
    fs::write(
        &source,
        "#include <lang/runtime.h>\nint sdk_header(void){return LANG_STATUS_SUCCESS;}\n",
    )
    .unwrap();
    let mut cc = Command::new(if cfg!(windows) { "cl" } else { "cc" });
    cc.current_dir(&dir.0);
    if cfg!(windows) {
        cc.args(["/nologo", "/std:c11", "/W4", "/WX", "/c", "header.c"])
            .arg(format!("/I{}", output.join("include").display()))
            .arg("/Foheader.obj");
    } else {
        cc.args([
            "-std=c11", "-Werror", "-c", "header.c", "-o", "header.o", "-I",
        ])
        .arg(output.join("include"));
    }
    let result = cc.output().unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn invalid_builds_missing_native_archives_and_collisions_never_publish() {
    let dir = Dir::new();
    let (records, build) = fixture(&dir);
    for change in 0..5 {
        let mut records = records.clone();
        match change {
            0 => {
                records.pop();
            }
            1 => records.last_mut().unwrap()["success"] = false.into(),
            2 => records.push(records[0].clone()),
            3 => records[4]["linked_libs"] = json!(["static=missing"]),
            _ => {
                let duplicate = build.join("other");
                fs::create_dir(&duplicate).unwrap();
                let path = duplicate.join("liblang_runtime-test.rlib");
                fs::write(&path, b"different dependency").unwrap();
                records.push(artifact("other", "lib", &[path], None));
            }
        }
        let out = dir.0.join(format!("sdk-{change}"));
        assert!(s::assemble(&encode(&records), &out, Limits::default()).is_err());
        assert!(!out.exists());
        no_stages(&dir);
    }
    let out = dir.0.join("bad-json");
    assert!(s::assemble(b"{bad json\n", &out, Limits::default()).is_err());
    assert!(!out.exists());
}

#[test]
fn exact_component_manifest_and_total_limits_refuse_without_partial_output() {
    let dir = Dir::new();
    let (records, _) = fixture(&dir);
    let bytes = encode(&records);
    let reference = dir.0.join("reference");
    let sdk = s::assemble(&bytes, &reference, Limits::default()).unwrap();
    let manifest = fs::read(reference.join("sdk.json")).unwrap();
    let max = sdk.files().iter().map(m::File::bytes).max().unwrap();
    let total = sdk.files().iter().map(m::File::bytes).sum::<u64>() + manifest.len() as u64;
    let limits = Limits {
        sdk_file: max,
        sdk_total: total,
        sdk_manifest: manifest.len(),
        ..Default::default()
    };
    s::assemble(&bytes, &dir.0.join("exact"), limits).unwrap();
    for (index, limits) in [
        Limits {
            sdk_file: max - 1,
            ..limits
        },
        Limits {
            sdk_total: total - 1,
            ..limits
        },
        Limits {
            sdk_manifest: manifest.len() - 1,
            ..limits
        },
    ]
    .into_iter()
    .enumerate()
    {
        let out = dir.0.join(format!("refused-{index}"));
        assert!(s::assemble(&bytes, &out, limits).is_err());
        assert!(!out.exists());
        no_stages(&dir);
    }
    assert!(s::assemble(&bytes, &reference, Limits::default()).is_err());
    assert_eq!(fs::read(reference.join("sdk.json")).unwrap(), manifest);
    no_stages(&dir);
}

#[test]
fn concurrent_sdk_publication_has_one_complete_winner_and_no_staging_leak() {
    let dir = Dir::new();
    let (records, _) = fixture(&dir);
    let bytes = encode(&records);
    let output = dir.0.join("sdk");
    let barrier = std::sync::Barrier::new(2);
    let outcomes = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            barrier.wait();
            s::assemble(&bytes, &output, Limits::default())
        });
        let second = scope.spawn(|| {
            barrier.wait();
            s::assemble(&bytes, &output, Limits::default())
        });
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    no_stages(&dir);
    drop(m::SdkSnapshot::open(&output, &dir.0, Limits::default()).unwrap());
    no_stages(&dir);
}

fn c_client(sdk_root: &Path, package: &Path) -> PathBuf {
    let bytes = fs::read(package.join("program.lbc")).unwrap();
    let program = lang_bytecode::ownership::decode(&bytes, Default::default()).unwrap();
    let target = if cfg!(windows) {
        lang_runtime::c_abi::Target::WindowsX86_64
    } else {
        lang_runtime::c_abi::Target::MacosAarch64
    };
    let artifact = lang_runtime::c_abi::Artifact::compile(
        program
            .flow()
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations(),
        target,
        Default::default(),
    )
    .unwrap();
    let symbol = &artifact
        .functions()
        .iter()
        .find(|f| f.name == "entry")
        .unwrap()
        .symbol;
    let digest = artifact
        .digest()
        .0
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let source = fs::read_to_string(sdk_root.join("startup/c-client.c.in"))
        .unwrap()
        .replace("@DIGEST_BYTES@", &digest)
        .replace("@ENTRY_SYMBOL@", symbol)
        .replace("@ID@", &artifact.digest().to_string());
    fs::write(package.join("client.c"), source).unwrap();
    let object = package.join(if cfg!(windows) {
        "client.obj"
    } else {
        "client.o"
    });
    let mut cc = Command::new(if cfg!(windows) { "cl" } else { "cc" });
    cc.current_dir(package);
    if cfg!(windows) {
        cc.args(["/nologo", "/std:c11", "/MD", "/c", "client.c"])
            .arg(format!("/Fo{}", object.display()));
    } else {
        cc.args(["-std=c11", "-Werror", "-c", "client.c", "-o"])
            .arg(&object);
    }
    let out = cc.output().unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    object
}

#[test]
fn real_sdk_compiler_and_startup_templates_link_all_profiles_with_only_sdk_dependencies() {
    let dir = Dir::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let build = Command::new("cargo")
        .current_dir(root)
        .args([
            "build",
            "--locked",
            "--bin",
            "lang",
            "--example",
            "native-sdk",
            "--message-format=json",
        ])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let sdk_root = dir.0.join("sdk");
    let sdk = s::assemble(&build.stdout, &sdk_root, Limits::default()).unwrap();
    let compiler = sdk
        .files()
        .iter()
        .find(|f| f.role() == m::Role::Compiler)
        .unwrap();
    let c_source = fs::read_to_string(sdk_root.join("startup/c-guest.lang")).unwrap();
    for (name, text, template) in [
        (
            "copy",
            "fn first()->i64{return 7;} fn main()->i64{return 42;}",
            "copy.rs",
        ),
        (
            "owned",
            include_str!("../docs/experiments/sdk-owned.lang"),
            "owned.rs",
        ),
        ("c", c_source.as_str(), "c-host.rs"),
    ] {
        let source = dir.0.join(format!("{name}.lang"));
        fs::write(&source, text).unwrap();
        let package = dir.0.join(name);
        let out = Command::new(sdk_root.join(compiler.path()))
            .current_dir(&dir.0)
            .arg("native")
            .arg("--output")
            .arg(&package)
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        fs::remove_file(source).unwrap();
        fs::copy(
            sdk_root.join("startup").join(template),
            package.join("main.rs"),
        )
        .unwrap();
        let c_object = if name == "c" {
            Some(c_client(&sdk_root, &package))
        } else {
            None
        };
        let executable = dir.0.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            format!("{name}-application")
        });
        let mut rustc = Command::new("rustc");
        rustc
            .current_dir(&dir.0)
            .arg("--edition=2024")
            .arg(package.join("main.rs"))
            .arg("-o")
            .arg(&executable)
            .arg("-L")
            .arg(format!("dependency={}", sdk_root.join("lib").display()));
        if sdk_root.join("native").exists() {
            rustc
                .arg("-L")
                .arg(format!("native={}", sdk_root.join("native").display()));
        }
        for name in [
            "lang_codegen_clif",
            "lang_runtime",
            "lang_mir",
            "lang_bytecode",
        ] {
            let files: Vec<_> = sdk
                .files()
                .iter()
                .filter(|f| {
                    f.path().starts_with(&format!("lib/lib{name}-")) && f.path().ends_with(".rlib")
                })
                .collect();
            assert_eq!(files.len(), 1);
            rustc.arg("--extern").arg(format!(
                "{name}={}",
                sdk_root.join(files[0].path()).display()
            ));
        }
        rustc.arg("-C").arg(format!(
            "link-arg={}",
            package
                .join(if cfg!(windows) {
                    "program.obj"
                } else {
                    "program.o"
                })
                .display()
        ));
        if let Some(object) = c_object {
            rustc
                .arg("-C")
                .arg(format!("link-arg={}", object.display()));
        }
        let out = rustc.output().unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let out = Command::new(executable)
            .current_dir(&dir.0)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            if name == "c" { "" } else { "42" }
        );
        no_stages(&dir);
    }
}
