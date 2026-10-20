#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
#[path = "support/rust_artifacts.rs"]
mod artifacts;
use artifacts::library;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "lang native compiler {} {}",
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
fn native(input: &Path, out: &Path, target: Option<&str>) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_lang"));
    c.arg("native").arg("--output").arg(out);
    if let Some(t) = target {
        c.arg("--target").arg(t);
    }
    c.arg(input).output().unwrap()
}

#[cfg(target_os = "macos")]
#[test]
fn source_native_copy_owned_and_c_objects_have_verified_function_debug_locations() {
    use object::{Object as _, ObjectSection as _, ObjectSymbol as _};
    let root = Dir::new();
    let cases = [
        (
            "copy",
            "fn increment(n:i64)->i64{return n+1;} fn main()->i64{return increment(41);}",
            vec!["increment", "main"],
        ),
        (
            "owned",
            "fn make(n:i64)->core::Bytes { let mut b:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator())); let mut p:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(b,n)); return b; } fn main()->i64 { let mut b:core::Bytes=make(41); return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(b,0))+1; }",
            vec!["make", "main"],
        ),
        (
            "c",
            "fn add(n:i64)->i64{return n+1;} pub extern \"C\" fn entry(n:i64)->i64 { let mut b:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator())); let mut p:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(b,n)); return add(core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(b,0))); }",
            vec!["add", "entry"],
        ),
    ];
    for (name, source, names) in cases {
        let input = root.0.join(format!("{name}.lang"));
        fs::write(&input, format!("// é 😀\n{source}\n")).unwrap();
        let package = root.0.join(name);
        let emitted = native(&input, &package, None);
        assert!(
            emitted.status.success(),
            "{}",
            String::from_utf8_lossy(&emitted.stderr)
        );
        let object = package.join("program.o");
        let verify = Command::new("xcrun")
            .args(["llvm-dwarfdump", "--verify"])
            .arg(&object)
            .output()
            .unwrap();
        assert!(
            verify.status.success(),
            "{}{}",
            String::from_utf8_lossy(&verify.stdout),
            String::from_utf8_lossy(&verify.stderr)
        );
        let dump = Command::new("xcrun")
            .args(["llvm-dwarfdump", "--debug-info"])
            .arg(&object)
            .output()
            .unwrap();
        assert!(dump.status.success());
        let info = String::from_utf8(dump.stdout).unwrap();
        assert_eq!(
            info.matches("DW_TAG_compile_unit").count(),
            names.len(),
            "{info}"
        );
        assert_eq!(
            info.matches("DW_TAG_subprogram").count(),
            names.len(),
            "{info}"
        );
        assert!(
            !info.contains("DW_TAG_formal_parameter"),
            "private ABI arguments must not be invented as source variables"
        );
        for function in names {
            assert!(info.contains(&format!("(\"{function}\")")), "{info}");
        }
        let bytes = fs::read(object).unwrap();
        let parsed = object::File::parse(bytes.as_slice()).unwrap();
        let debug = parsed.section_by_name("__debug_info").unwrap();
        let targets: std::collections::HashSet<_> = debug
            .relocations()
            .filter_map(|(_, r)| match r.target() {
                object::RelocationTarget::Symbol(i) => parsed
                    .symbol_by_index(i)
                    .ok()
                    .and_then(|s| s.name().ok())
                    .map(str::to_owned),
                _ => None,
            })
            .collect();
        for line in info.lines().filter(|l| l.contains("DW_AT_linkage_name")) {
            let symbol = line.split('"').nth(1).unwrap();
            // Mach-O's leading underscore is not part of the linker spelling.
            assert!(
                targets.contains(symbol) || targets.contains(&format!("_{symbol}")),
                "{symbol}: {targets:?}"
            );
        }
    }
}
fn build(package: &Path, name: &str) -> PathBuf {
    build_extra(package, name, &[])
}
#[cfg(windows)]
fn windows_native_paths() -> &'static Vec<PathBuf> {
    static PATHS: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();
    PATHS.get_or_init(|| {
        // In-tree conformance only: Cargo normally forwards these native
        // import-library paths. Installed SDK linking must bundle them.
        let metadata = Command::new(env!("CARGO"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "metadata",
                "--locked",
                "--offline",
                "--format-version=1",
                "--filter-platform=x86_64-pc-windows-msvc",
            ])
            .output()
            .unwrap();
        assert!(metadata.status.success(), "{metadata:?}");
        let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
        metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|package| package["name"] == "windows_x86_64_msvc")
            .map(|package| {
                let path = PathBuf::from(package["manifest_path"].as_str().unwrap())
                    .parent()
                    .unwrap()
                    .join("lib");
                assert!(path.is_dir());
                path
            })
            .collect()
    })
}
fn build_extra(package: &Path, name: &str, extra: &[&str]) -> PathBuf {
    let exe = package.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let mut c = Command::new("rustc");
    c.current_dir(package)
        .args([
            "--edition=2024",
            if package.join("main.rs").exists() {
                "main.rs"
            } else {
                "host.rs"
            },
            "-L",
        ])
        .arg(format!("dependency={}", deps.display()));
    #[cfg(windows)]
    for path in windows_native_paths() {
        let mut search = std::ffi::OsString::from("native=");
        search.push(path);
        c.arg("-L").arg(search);
    }
    for name in [
        "lang_mir",
        "lang_runtime",
        "lang_bytecode",
        "lang_codegen_clif",
    ] {
        c.arg("--extern")
            .arg(format!("{name}={}", library(name).display()));
    }
    c.arg("-C")
        .arg(format!(
            "link-arg=program.{}",
            if cfg!(windows) { "obj" } else { "o" }
        ))
        .arg("-o")
        .arg(&exe);
    for file in extra {
        c.arg("-C").arg(format!("link-arg={file}"));
    }
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{:?}\n{}",
        c,
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}
fn check(package: &Path) -> serde_json::Value {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(package.join("package.json")).unwrap()).unwrap();
    assert_eq!(manifest["format"], "lang-native-program");
    assert_eq!(manifest["version"], 1);
    let target = manifest["target"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        manifest["build_id"],
        lang_codegen_clif::sdk::build_id(&target)
    );
    for f in manifest["files"].as_array().unwrap() {
        let bytes = fs::read(package.join(f["path"].as_str().unwrap())).unwrap();
        assert_eq!(f["bytes"], bytes.len());
        assert_eq!(f["sha256"], lang_codegen_clif::sdk::digest(&bytes));
    }
    let semantic = fs::read(package.join("program.lbc")).unwrap();
    let raw = fs::read(package.join("program.native")).unwrap();
    let record =
        lang_codegen_clif::native_record::Record::decode(&raw, &semantic, Default::default())
            .unwrap();
    assert_eq!(record.target(), &target);
    assert_eq!(
        record.object(),
        fs::read(
            package.join(if target.to_string() == "x86_64-pc-windows-msvc" {
                "program.obj"
            } else {
                "program.o"
            })
        )
        .unwrap()
    );
    manifest
}
#[test]
fn source_native_copy_and_owned_outputs_execute_generated_drivers_after_source_removal() {
    for (source, profile) in [
        (
            "fn helper(x:i64)->i64{return x+2;} fn main()->i64{return helper(40);}",
            "copy",
        ),
        (include_str!("../docs/experiments/sdk-owned.lang"), "owned"),
    ] {
        let dir = Dir::new();
        let input = dir.0.join("input.lang");
        let package = dir.0.join("program files");
        fs::write(&input, source).unwrap();
        let out = native(&input, &package, None);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let manifest = check(&package);
        assert_eq!(manifest["profile"], profile);
        if profile == "copy" {
            assert_ne!(manifest["entry"], 0);
        }
        let exe = build(&package, "application");
        fs::remove_file(input).unwrap();
        let out = Command::new(exe).current_dir(&dir.0).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "42");
    }
}
#[test]
fn source_native_c_library_preserves_explicit_abi_without_default_entry() {
    let dir = Dir::new();
    let input = dir.0.join("input.lang");
    let package = dir.0.join("c program");
    fs::write(
        &input,
        include_str!("../examples/analysis/c_owned_exports.lang"),
    )
    .unwrap();
    let out = native(&input, &package, None);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let manifest = check(&package);
    assert_eq!(manifest["profile"], "c");
    assert!(manifest["entry"].is_null());
    assert!(!package.join("main.rs").exists());
    assert!(package.join("program.h").is_file());
    let semantic = fs::read(package.join("program.lbc")).unwrap();
    assert_eq!(&semantic[8..10], &4u16.to_le_bytes());
    let decoded = lang_bytecode::ownership::decode(&semantic, Default::default()).unwrap();
    assert!(decoded.entry().is_none());
}
#[test]
fn source_native_failures_preserve_inputs_outputs_and_reject_unknown_options() {
    let dir = Dir::new();
    let input = dir.0.join("input.lang");
    let package = dir.0.join("existing");
    fs::write(&input, "fn main()->i64{return 7;}").unwrap();
    fs::create_dir(&package).unwrap();
    fs::write(package.join("keep"), b"original").unwrap();
    assert!(!native(&input, &package, None).status.success());
    assert_eq!(fs::read(package.join("keep")).unwrap(), b"original");
    assert!(!native(&input, &input, None).status.success());
    assert_eq!(
        fs::read_to_string(&input).unwrap(),
        "fn main()->i64{return 7;}"
    );
    let fresh = dir.0.join("fresh");
    assert!(
        !native(&input, &fresh, Some("x86_64-unknown-linux-gnu"))
            .status
            .success()
    );
    assert!(!fresh.exists());
    fs::write(&input, "fn main()->i64{return false;}").unwrap();
    let out = native(&input, &fresh, None);
    assert!(!out.status.success());
    assert!(!fresh.exists());
    let out = Command::new(env!("CARGO_BIN_EXE_lang"))
        .args(["native", "--fuel", "1"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
#[test]
fn source_native_concurrent_publication_has_one_complete_winner() {
    let dir = Dir::new();
    let input = dir.0.join("input.lang");
    let package = dir.0.join("winner");
    fs::write(&input, "fn main()->i64{return 42;}").unwrap();
    let mut children = Vec::new();
    for _ in 0..2 {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_lang"))
                .arg("native")
                .arg("--output")
                .arg(&package)
                .arg(&input)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    let successes: usize = children
        .into_iter()
        .map(|mut c| usize::from(c.wait().unwrap().success()))
        .sum();
    assert_eq!(successes, 1);
    check(&package);
    assert!(!fs::read_dir(&dir.0).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lang-native-")
    }));
}

#[test]
fn source_native_c_bindings_run_a_real_c_caller_with_original_allocator() {
    let dir = Dir::new();
    let input = dir.0.join("input.lang");
    let package = dir.0.join("c executable");
    fs::write(&input,r#"pub extern "C" fn entry(value:i64)->i64 {
        let mut bytes:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
        let mut pushed:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(bytes,value));
        return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(bytes,0))+1;
    }"#).unwrap();
    let out = native(&input, &package, None);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    check(&package);
    let sem = fs::read(package.join("program.lbc")).unwrap();
    let p = lang_bytecode::ownership::decode(&sem, Default::default()).unwrap();
    let target = if cfg!(windows) {
        lang_runtime::c_abi::Target::WindowsX86_64
    } else {
        lang_runtime::c_abi::Target::MacosAarch64
    };
    let artifact = lang_runtime::c_abi::Artifact::compile(
        p.flow()
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
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let id = artifact.digest();
    let c = include_str!("../src/native/sdk_startup/c-client.c.in")
        .replace("@DIGEST_BYTES@", &digest)
        .replace("@ENTRY_SYMBOL@", symbol)
        .replace("@ID@", &id.to_string());
    fs::write(package.join("client.c"), c).unwrap();
    let object = if cfg!(windows) {
        "client.obj"
    } else {
        "client.o"
    };
    let mut cc = Command::new(if cfg!(windows) { "cl" } else { "cc" });
    cc.current_dir(&package);
    if cfg!(windows) {
        cc.args(["/nologo", "/std:c11", "/MD", "/c", "client.c"])
            .arg(format!("/Fo{object}"));
    } else {
        cc.args(["-std=c11", "-Werror", "-c", "client.c", "-o", object]);
    }
    let out = cc.output().unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let host = r#"
mod bindings;
unsafe extern "C" { fn native_client()->i32; }
fn main(){
    let input=lang_codegen_clif::sdk::runtime::Input{target:bindings::TARGET,build_id:bindings::BUILD_ID,semantic:bindings::SEMANTIC,native:bindings::NATIVE};
    let capacities=lang_runtime::c_abi::context::config::Capacities::from_strides(64,16,64).unwrap();
    // SAFETY: exact original linked image. The synchronous C client creates,
    // calls and destroys every context on this owner thread before returning.
    let result=unsafe{lang_codegen_clif::sdk::runtime::with_c(input,&(),bindings::resolve,capacities,Default::default(),|_|native_client())}.unwrap();
    assert_eq!(result,0);
    // No C context is created in this body. Unwinding must retire the scoped
    // registration so the exact same image can be bound and called again.
    assert!(std::panic::catch_unwind(|| unsafe {
        lang_codegen_clif::sdk::runtime::with_c(input,&(),bindings::resolve,capacities,Default::default(),|_|panic!("host scope"))
    }).is_err());
    let result=unsafe{lang_codegen_clif::sdk::runtime::with_c(input,&(),bindings::resolve,capacities,Default::default(),|_|native_client())}.unwrap();
    assert_eq!(result,0);
}
"#;
    fs::write(package.join("host.rs"), host).unwrap();
    let exe = build_extra(&package, "c-application", &[object]);
    fs::remove_file(input).unwrap();
    let out = Command::new(exe).current_dir(&dir.0).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn source_native_cross_target_profiles_keep_original_entry_and_object() {
    use object::Object;
    let dir = Dir::new();
    for (index, source, profile) in [
        (
            "copy",
            "struct Pair { a:i64,b:i64 } fn add(p:Pair)->i64{return p.a+p.b;} fn main()->i64{return add(Pair{b:2,a:40});}",
            "copy",
        ),
        (
            "owned",
            "fn first()->i64{return 0;} fn main()->i64{let mut bytes:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));return core::bytes_len(bytes);}",
            "owned",
        ),
        (
            "c",
            "extern \"C\" fn unused(value:i64)->i64; fn main()->i64{return 7;}",
            "c",
        ),
    ] {
        let input = dir.0.join(format!("{index}.lang"));
        fs::write(&input, source).unwrap();
        for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let package = dir.0.join(format!("{index}-{target}"));
            let out = native(&input, &package, Some(target));
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let manifest = check(&package);
            assert_eq!(manifest["profile"], profile);
            assert_eq!(manifest["target"], target);
            if profile == "c" {
                assert!(manifest["entry"].is_null());
                assert!(!package.join("main.rs").exists());
            } else {
                assert_ne!(manifest["entry"], 0);
            }
            let object = manifest["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["role"] == "object")
                .unwrap();
            let raw = fs::read(package.join(object["path"].as_str().unwrap())).unwrap();
            let object = object::File::parse(raw.as_slice()).unwrap();
            assert_eq!(
                object.architecture(),
                if target.starts_with("aarch64") {
                    object::Architecture::Aarch64
                } else {
                    object::Architecture::X86_64
                }
            );
        }
    }
    assert_ne!(
        lang_codegen_clif::sdk::build_id(&"aarch64-apple-darwin".parse().unwrap()),
        lang_codegen_clif::sdk::build_id(&"x86_64-pc-windows-msvc".parse().unwrap())
    );
}

#[test]
fn source_native_generated_startup_matches_vm_for_managed_families_and_panic() {
    let combined = include_str!("../examples/conformance/owned_pipeline.lang");
    let combined_panic = combined.replace(
        "fn main() -> i64 { return scenario(false); }",
        "fn main() -> i64 { return scenario(true); }",
    );
    for (name, source) in [
        ("map", include_str!("../examples/scripts/map_owned.lang")),
        ("vec", include_str!("../examples/scripts/vec_owned.lang")),
        (
            "arena",
            include_str!("../examples/scripts/arena_owned.lang"),
        ),
        (
            "shared",
            include_str!("../examples/scripts/shared_owned.lang"),
        ),
        (
            "panic",
            include_str!("../examples/analysis/explicit_panic_owned.lang"),
        ),
        ("combined", combined),
        ("combined-panic", combined_panic.as_str()),
    ] {
        let dir = Dir::new();
        let input = dir.0.join(format!("{name}.lang"));
        fs::write(&input, source).unwrap();
        let oracle = Command::new(env!("CARGO_BIN_EXE_lang"))
            .arg("run")
            .arg(&input)
            .output()
            .unwrap();
        let package = dir.0.join("program");
        let out = native(&input, &package, None);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        check(&package);
        let exe = build(&package, "application");
        fs::remove_file(input).unwrap();
        let actual = Command::new(exe).output().unwrap();
        assert_eq!(
            actual.status.code(),
            oracle.status.code(),
            "{name}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(actual.stdout, oracle.stdout, "{name}");
        if name.ends_with("panic") {
            let error = String::from_utf8_lossy(&actual.stderr);
            assert!(error.contains("-42"), "{error}");
            assert!(error.contains("Execution(Panic)"), "{error}");
            assert!(error.contains("panic_code: Some(-42)"), "{error}");
            assert!(error.contains("stack: [(FunctionId"), "{error}");
            assert!(error.contains("cleanup_failures: 0"), "{error}");
        }
    }
}

#[test]
fn source_native_modules_retain_snapshots_and_sdk_mismatch_refuses_startup() {
    let dir = Dir::new();
    let input = dir.0.join("main.lang");
    let helper = dir.0.join("helper.lang");
    fs::write(
        &input,
        "mod helper;fn main()->i64{return helper::answer();}",
    )
    .unwrap();
    fs::write(&helper, "pub fn answer()->i64{return 42;}").unwrap();
    let package = dir.0.join("program");
    let out = Command::new(env!("CARGO_BIN_EXE_lang"))
        .arg("native")
        .arg("--source-root")
        .arg(&dir.0)
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
    let manifest = check(&package);
    let executable = build(&package, "matching");
    fs::remove_file(input).unwrap();
    fs::remove_file(helper).unwrap();
    assert_eq!(Command::new(executable).output().unwrap().stdout, b"42\n");
    let bindings = package.join("bindings.rs");
    let text = fs::read_to_string(&bindings).unwrap();
    fs::write(
        bindings,
        text.replace(manifest["build_id"].as_str().unwrap(), &"0".repeat(64)),
    )
    .unwrap();
    let executable = build(&package, "mismatched");
    let out = Command::new(executable).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("SDK build mismatch"));
}

#[test]
fn sdk_copy_preflight_refuses_before_resolution_and_keeps_the_borrowed_image() {
    let dir = Dir::new();
    let input = dir.0.join("input.lang");
    let package = dir.0.join("program");
    fs::write(
        &input,
        "fn first()->i64{return 7;} fn main()->i64{return 42;}",
    )
    .unwrap();
    let out = native(&input, &package, None);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::write(package.join("main.rs"),r#"
mod bindings;
use lang_codegen_clif::sdk::runtime::{self as sdk,Input,ErrorKind};
use lang_runtime::execution as e;
use std::{cell::Cell,rc::Rc,time::{Duration,Instant}};
struct Image(Rc<Cell<usize>>);
impl Drop for Image{fn drop(&mut self){self.0.set(self.0.get()+1);}}
fn main(){
 let drops=Rc::new(Cell::new(0));let image=Image(drops.clone());let resolutions=Cell::new(0);
 let mut resolve=|_:&Image,name:&str|{resolutions.set(resolutions.get()+1);bindings::resolve(&(),name)};
 let input=Input{target:bindings::TARGET,build_id:bindings::BUILD_ID,semantic:bindings::SEMANTIC,native:bindings::NATIVE};
 let wrong=Input{build_id:"wrong",..input};
 assert!(matches!(unsafe{sdk::execute_copy(wrong,&image,&mut resolve,Default::default(),Default::default())}.unwrap_err().kind,ErrorKind::BuildId{..}));
 let wrong=Input{target:if cfg!(windows){"aarch64-apple-darwin"}else{"x86_64-pc-windows-msvc"},..input};
 assert!(matches!(unsafe{sdk::execute_copy(wrong,&image,&mut resolve,Default::default(),Default::default())}.unwrap_err().kind,ErrorKind::Target{..}));
 let flag=e::InterruptionFlag::default();
 for control in [
   e::Control::with_limits(e::Limits{fuel:Some(0),..Default::default()}),
   e::Control::with_limits(e::Limits{host_calls:Some(0),..Default::default()}),
   e::Control::with_limits(e::Limits{frames:Some(1),..Default::default()}),
   e::Control::with_limits(e::Limits{live_cells:Some(1),..Default::default()}),
   e::Control::default().and_service_work(Some(0)),
   e::Control::default().and_interruption(&flag),
   e::Control::default().and_deadline(Instant::now()+Duration::from_secs(60)),
 ] {assert!(matches!(unsafe{sdk::execute_copy(input,&image,&mut resolve,control,Default::default())}.unwrap_err().kind,ErrorKind::UnsupportedCopyControls));}
 let malformed=Input{native:&[],..input};
 assert!(matches!(unsafe{sdk::execute_copy(malformed,&image,&mut resolve,Default::default(),Default::default())}.unwrap_err().kind,ErrorKind::Record(_)));
 let no_budget=sdk::Limits{record:lang_codegen_clif::native_record::Limits{bytes:0,..Default::default()},..Default::default()};
 assert!(matches!(unsafe{sdk::execute_copy(input,&image,&mut resolve,Default::default(),no_budget)}.unwrap_err().kind,ErrorKind::Record(_)));
 let mut allocator=lang_runtime::allocation::Allocator::new(lang_runtime::allocation::System,Default::default()).unwrap();
 let mut instance=lang_runtime::cleanup::Instance::default();let wrong=Input{build_id:"wrong",..input};
 assert!(unsafe{sdk::execute_owned(wrong,&image,&mut resolve,&mut instance,&mut allocator,Default::default(),Default::default())}.is_err());
 let capacities=lang_runtime::c_abi::context::config::Capacities::from_strides(64,16,64).unwrap();let entered=Cell::new(false);
 assert!(unsafe{sdk::with_c(wrong,&image,&mut resolve,capacities,Default::default(),|_|entered.set(true))}.is_err());
 assert!(!entered.get());assert_eq!(resolutions.get(),0);assert_eq!(drops.get(),0);
 assert_eq!(unsafe{sdk::execute_copy(input,&image,&mut resolve,Default::default(),Default::default())}.unwrap(),42);
 assert!(resolutions.get()>0);assert_eq!(drops.get(),0);drop(image);assert_eq!(drops.get(),1);
}
"#).unwrap();
    let executable = build(&package, "guarded");
    fs::remove_file(input).unwrap();
    let out = Command::new(executable).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn sdk_owned_uses_original_allocator_control_source_and_poisoned_instance() {
    let dir = Dir::new();
    let input = dir.0.join("input.lang");
    let package = dir.0.join("program");
    fs::write(&input, include_str!("../docs/experiments/sdk-owned.lang")).unwrap();
    let out = native(&input, &package, None);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::write(package.join("main.rs"),r#"
mod bindings;
use lang_codegen_clif::{sdk::runtime::{self as sdk,Input,ErrorKind},owned::runtime as rt};
use lang_runtime::{allocation as a,cleanup as c,execution as e};
use std::{cell::Cell,rc::Rc};
struct Track{system:a::System,calls:Rc<Cell<usize>>,panic:Rc<Cell<bool>>}
impl a::Provider for Track{
 type Region=a::Storage;
 fn allocate(&mut self,r:a::Request)->Option<Self::Region>{self.calls.set(self.calls.get()+1);assert!(!self.panic.get(),"SDK provider panic");a::Provider::allocate(&mut self.system,r)}
 fn deallocate(&mut self,r:Self::Region,q:a::Request){a::Provider::deallocate(&mut self.system,r,q)}
}
fn main(){
 let input=Input{target:bindings::TARGET,build_id:bindings::BUILD_ID,semantic:bindings::SEMANTIC,native:bindings::NATIVE};
 let calls=Rc::new(Cell::new(0));let panic=Rc::new(Cell::new(false));
 let mut allocator=a::Allocator::new(Track{system:a::System,calls:calls.clone(),panic:panic.clone()},Default::default()).unwrap();
 let identity=allocator.identity();let mut instance=c::Instance::default();
 let config=rt::Config{control:e::Control::with_limits(e::Limits{fuel:Some(0),host_calls:Some(0),frames:Some(4),live_cells:Some(1024)}),..Default::default()};
 let failure=unsafe{sdk::execute_owned(input,&(),bindings::resolve,&mut instance,&mut allocator,config,Default::default())}.unwrap_err();
 match &failure.kind{ErrorKind::OwnedEntry(e)=>assert_eq!(e.usage.fuel,0),other=>panic!("{other:?}")};
 assert!(std::ptr::eq(failure.input.semantic,bindings::SEMANTIC));assert!(calls.get()>0);assert_eq!(allocator.stats().live_allocations,0);assert!(!instance.is_poisoned());
 let p=lang_bytecode::ownership::decode(failure.input.semantic,Default::default()).unwrap();assert!(p.flow().source().activity().moved().typed().places().declarations().types().table().sources[0].text.contains("fn main"));drop(p);
 let outcome=unsafe{sdk::execute_owned(input,&(),bindings::resolve,&mut instance,&mut allocator,Default::default(),Default::default())}.unwrap();
 assert_eq!(outcome.value,42);assert!(calls.get()>0);assert_eq!(allocator.identity(),identity);assert_eq!(allocator.stats().live_allocations,0);assert!(!instance.is_poisoned());
 panic.set(true);let failure=unsafe{sdk::execute_owned(input,&(),bindings::resolve,&mut instance,&mut allocator,Default::default(),Default::default())}.unwrap_err();
 assert!(matches!(failure.kind,ErrorKind::OwnedEntry(_)));assert!(instance.is_poisoned());let before=calls.get();panic.set(false);
 let failure=unsafe{sdk::execute_owned(input,&(),bindings::resolve,&mut instance,&mut allocator,Default::default(),Default::default())}.unwrap_err();
 assert!(matches!(failure.kind,ErrorKind::OwnedEntry(rt::EntryError{kind:rt::ErrorKind::Poisoned,..})));assert_eq!(calls.get(),before);
}
"#).unwrap();
    let executable = build(&package, "owned-state");
    fs::remove_file(input).unwrap();
    let out = Command::new(executable).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
