#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
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
fn library(name: &str) -> PathBuf {
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    fs::read_dir(deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("lib{name}-"))
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .max_by_key(|p| p.metadata().unwrap().modified().unwrap())
        .unwrap()
}
fn build(package: &Path, name: &str) -> PathBuf {
    build_extra(package, name, &[])
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
    let c = format!(
        r#"
#include "program.h"
#include <stdlib.h>
static uint64_t allocations, releases;
static void *allocate(void *user,uint64_t size,uint64_t align) {{
    (void)user;
    if (align > _Alignof(max_align_t)) return NULL;
    void *p=malloc((size_t)(size?size:1));
    if(p) ++allocations;
    return p;
}}
static void deallocate(void *user,void *p,uint64_t size,uint64_t align) {{
    (void)user; (void)size; (void)align; ++releases; free(p);
}}
int native_client(void) {{
    lang_allocator_descriptor allocator={{1,32,NULL,allocate,deallocate}};
    lang_limits_descriptor limits={{1,72,16777216,67108864,4096,64,65536,0,1000000,256,0}};
    lang_program_config config={{1,80,&allocator,&limits,NULL,0,{{{digest}}},0}};
    lang_context *context=NULL;
    if(lang_context_create_{id}(&config,&context)) return 1;
    int64_t output=-1;
    if({symbol}(context,41,&output) || output!=42) return 2;
    if(lang_context_destroy_{id}(context)) return 3;
    if(!allocations || allocations!=releases) return 4;
    return 0;
}}
"#
    );
    fs::write(package.join("client.c"), c).unwrap();
    let object = if cfg!(windows) {
        "client.obj"
    } else {
        "client.o"
    };
    let mut cc = Command::new(if cfg!(windows) { "cl" } else { "cc" });
    cc.current_dir(&package);
    if cfg!(windows) {
        cc.args(["/nologo", "/std:c11", "/c", "client.c"])
            .arg(format!("/Fo{object}"));
    } else {
        cc.args(["-std=c11", "-Werror", "-c", "client.c", "-o", object]);
    }
    let out = cc.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let host = r#"
mod bindings;
unsafe extern "C" { fn native_client()->i32; }
fn main(){
    let target=bindings::TARGET.parse().unwrap();
    assert_eq!(lang_codegen_clif::sdk::build_id(&target),bindings::BUILD_ID);
    let p=lang_bytecode::ownership::decode(bindings::SEMANTIC,Default::default()).unwrap();
    let flow=p.flow();let decl=flow.source().activity().moved().typed().places().declarations();
    let layouts=lang_runtime::layout::Layouts::compile(decl.types(),Default::default()).unwrap();
    let target=if cfg!(windows){lang_runtime::c_abi::Target::WindowsX86_64}else{lang_runtime::c_abi::Target::MacosAarch64};
    let artifact=lang_runtime::c_abi::Artifact::compile(decl,target,Default::default()).unwrap();
    let record=lang_codegen_clif::native_record::Record::decode(bindings::NATIVE,bindings::SEMANTIC,Default::default()).unwrap();
    let object=lang_codegen_clif::owned::RecordedObject::from_record(record).unwrap().prepare_c(flow,&layouts,&artifact,Default::default(),4096).unwrap();
    // SAFETY: generated bindings refer to the unchanged linked object; the
    // process retains every symbol, and native_client destroys all contexts.
    let code=unsafe{object.bind((),bindings::resolve,Default::default())}.unwrap();
    let program=lang_runtime::c_abi::context::Program::new(&artifact,&code,lang_runtime::c_abi::context::config::Capacities::from_strides(64,16,64).unwrap()).unwrap();
    let factory=lang_codegen_clif::c_abi::runtime::NativeFactory::new(&code,&artifact,Default::default()).unwrap();
    // SAFETY: synchronous owner thread; client returns with no context/call.
    let _exports=unsafe{lang_codegen_clif::c_abi::object::ObjectExports::new(&program,&code,&factory)}.unwrap();
    assert_eq!(unsafe{native_client()},0);
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
        if name == "panic" {
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
