//! Public native-link behavior, pre-tool refusal, bounded host inputs and atomic
//! output. Tiny trusted SDK fixtures test failure order; real SDKs test execution.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
use lang::native::{
    link::{self as l, Error, Limits, Request, ToolStage},
    manifest as m,
};
use lang_codegen_clif::sdk;
use serde_json::{Value, json};
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
            "lang native link-{}-{}",
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
fn ok(out: Output) -> Output {
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lang"))
}
fn no_stages(dir: &Dir) {
    assert!(!fs::read_dir(&dir.0).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lang-")
    }));
}
fn program(compiler: &Path, dir: &Dir, name: &str, text: &str) -> PathBuf {
    let source = dir.0.join(format!("{name}.lang"));
    let package = dir.0.join(name);
    fs::write(&source, text).unwrap();
    ok(Command::new(compiler)
        .current_dir(&dir.0)
        .arg("native")
        .arg("--output")
        .arg(&package)
        .arg(&source)
        .output()
        .unwrap());
    fs::remove_file(source).unwrap();
    package
}
fn component(role: &str, path: &str, bytes: &[u8]) -> Value {
    json!({"role":role,"path":path,"bytes":bytes.len(),"sha256":sdk::digest(bytes)})
}
fn fixture(dir: &Dir) -> Request {
    let root = dir.0.join("sdk");
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    let mut files = Vec::new();
    for (role, path) in [
        ("compiler", "bin/lang"),
        ("runtime", "lib/liblang_codegen_clif-fixture.rlib"),
        ("dependency", "lib/liblang_runtime-fixture.rlib"),
        ("dependency", "lib/liblang_mir-fixture.rlib"),
        ("dependency", "lib/liblang_bytecode-fixture.rlib"),
    ] {
        let bytes = b"trusted IO fixture";
        fs::write(root.join(path), bytes).unwrap();
        files.push(component(role, path, bytes));
    }
    let target = lang_codegen_clif::Triple::host();
    let manifest = json!({"format":"lang-native-sdk","version":1,"build_id":sdk::build_id(&target),"target":target.to_string(),"rustc":{"release":sdk::RUSTC_RELEASE,"commit":sdk::RUSTC_COMMIT},"build_profile":sdk::BUILD_PROFILE,"feature_policy":sdk::FEATURE_POLICY,"files":files});
    fs::write(
        root.join("sdk.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let package = program(
        Path::new(env!("CARGO_BIN_EXE_lang")),
        dir,
        "program",
        "fn main()->i64{return 42;}",
    );
    Request {
        rustc: None,
        sdk: root,
        package,
        host: None,
        objects: Vec::new(),
        output: dir.0.join("application"),
    }
}
fn fake_compiler(dir: &Dir) -> PathBuf {
    let version = format!(
        "rustc fixture\nrelease: {}\ncommit-hash: {}\nhost: {}\n",
        sdk::RUSTC_RELEASE,
        sdk::RUSTC_COMMIT,
        lang_codegen_clif::Triple::host()
    );
    let source=r#"
use std::{fs,io::Write};
fn main(){
 let exe=std::env::current_exe().unwrap();let mode=exe.file_stem().unwrap().to_string_lossy();
 let mut marker=fs::OpenOptions::new().create(true).append(true).open(exe.parent().unwrap().join("tool.marker")).unwrap();
 let args:Vec<_>=std::env::args().skip(1).collect();let version=__VERSION__;
 if args[0]=="--version"{
  writeln!(marker,"version").unwrap();
  if mode=="noisy-version"{print!("{}","x".repeat(131072));return;}
  if mode=="wrong-version"{print!("{}",version.replace("release: ","release: wrong-"));return;}
  if mode=="wrong-commit"{print!("{}",version.replace("commit-hash: ","commit-hash: wrong-"));return;}
  if mode=="wrong-host"{print!("{}",version.replace("host: ","host: wrong-"));return;}
  if mode=="malformed"{println!("missing version fields");return;}
  print!("{version}");return;
 }
 writeln!(marker,"compile").unwrap();
 if mode=="compile-fail"{eprintln!("fixture linker failure");std::process::exit(17);}
 if mode=="noisy-compile"{eprint!("{}","x".repeat(2097152));return;}
 if mode=="missing-output"{return;}
 let text=fs::read_to_string(args[0].strip_prefix('@').unwrap()).unwrap();let lines:Vec<_>=text.lines().collect();
 let index=lines.iter().position(|a|*a=="-o").unwrap();fs::write(lines[index+1],b"linked").unwrap();
}
"#.replace("__VERSION__",&format!("{version:?}"));
    let input = dir.0.join("fake.rs");
    fs::write(&input, source).unwrap();
    let compiler = dir.0.join(if cfg!(windows) {
        "success.exe"
    } else {
        "success"
    });
    ok(Command::new("rustc")
        .arg("--edition=2024")
        .arg(input)
        .arg("-o")
        .arg(&compiler)
        .output()
        .unwrap());
    compiler
}
fn mode(dir: &Dir, compiler: &Path, name: &str) -> PathBuf {
    let path = dir
        .0
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    fs::copy(compiler, &path).unwrap();
    let _ = fs::remove_file(dir.0.join("tool.marker"));
    path
}

#[test]
fn cli_usage_errors_are_distinct_and_object_requires_explicit_host() {
    for args in [
        vec!["link-native"],
        vec!["link-native", "--sdk"],
        vec!["link-native", "--unknown"],
        vec![
            "link-native",
            "--sdk",
            "a",
            "--sdk",
            "b",
            "--package",
            "p",
            "--output",
            "x",
        ],
        vec![
            "link-native",
            "--sdk",
            "a",
            "--package",
            "p",
            "--output",
            "x",
            "--object",
            "host.o",
        ],
    ] {
        let out = cli().args(args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let out = ok(cli().args(["link-native", "--help"]).output().unwrap());
    assert!(String::from_utf8_lossy(&out.stdout).contains("link-native"));
}

#[test]
fn preflight_errors_do_not_start_compiler_or_replace_previous_outputs() {
    let dir = Dir::new();
    let mut request = fixture(&dir);
    let compiler = fake_compiler(&dir);
    request.rustc = Some(compiler.into_os_string());
    fs::write(&request.output, b"previous").unwrap();
    assert!(l::link(&request, Limits::default()).is_err());
    assert_eq!(fs::read(&request.output).unwrap(), b"previous");
    fs::remove_file(&request.output).unwrap();
    let manifest = request.sdk.join("sdk.json");
    let original = fs::read(&manifest).unwrap();
    let mut bad: Value = serde_json::from_slice(&original).unwrap();
    bad["build_id"] = "00".repeat(32).into();
    fs::write(&manifest, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(matches!(
        l::link(&request, Limits::default()),
        Err(Error::Manifest(_))
    ));
    fs::write(&manifest, original).unwrap();
    let object = request.package.join(if cfg!(windows) {
        "program.obj"
    } else {
        "program.o"
    });
    let original = fs::read(&object).unwrap();
    fs::write(&object, b"changed object").unwrap();
    assert!(matches!(
        l::link(&request, Limits::default()),
        Err(Error::Manifest(_))
    ));
    fs::write(object, original).unwrap();
    let host = dir.0.join("host.rs");
    fs::write(&host, b"host").unwrap();
    request.host = Some(host);
    assert!(matches!(
        l::link(
            &request,
            Limits {
                host_bytes: 3,
                ..Default::default()
            }
        ),
        Err(Error::Limit { .. })
    ));
    request.objects.push(dir.0.join("not-read.o"));
    assert!(matches!(
        l::link(
            &request,
            Limits {
                objects: 0,
                ..Default::default()
            }
        ),
        Err(Error::Limit { .. })
    ));
    assert!(!dir.0.join("tool.marker").exists());
    assert!(!request.output.exists());
    no_stages(&dir);
}

#[test]
fn exact_rustc_identity_diagnostic_budgets_and_failed_link_roll_back() {
    let dir = Dir::new();
    let mut request = fixture(&dir);
    let compiler = fake_compiler(&dir);
    for name in [
        "wrong-version",
        "wrong-commit",
        "wrong-host",
        "malformed",
        "noisy-version",
        "compile-fail",
        "noisy-compile",
        "missing-output",
    ] {
        request.rustc = Some(mode(&dir, &compiler, name).into_os_string());
        let error = l::link(&request, Limits::default()).unwrap_err();
        match name {
            "wrong-version" | "wrong-commit" | "wrong-host" => {
                assert!(matches!(error, Error::Toolchain { .. }))
            }
            "malformed" => assert!(matches!(error, Error::VersionFormat)),
            "noisy-version" | "noisy-compile" => assert!(matches!(error, Error::ToolOutput { .. })),
            "compile-fail" => assert!(matches!(
                error,
                Error::ToolFailed {
                    stage: ToolStage::Compile,
                    ..
                }
            )),
            _ => assert!(matches!(error, Error::MissingOutput)),
        }
        let calls = fs::read_to_string(dir.0.join("tool.marker")).unwrap();
        assert_eq!(
            calls.contains("compile"),
            matches!(name, "compile-fail" | "noisy-compile" | "missing-output")
        );
        assert!(!request.output.exists());
        no_stages(&dir);
    }
    request.rustc = Some(dir.0.join("missing-rustc").into_os_string());
    assert!(matches!(
        l::link(&request, Limits::default()),
        Err(Error::ToolIo {
            stage: ToolStage::Version,
            ..
        })
    ));
    no_stages(&dir);
}

#[test]
fn host_and_executable_limits_accept_exact_bytes_and_keep_input_snapshots() {
    let dir = Dir::new();
    let mut request = fixture(&dir);
    request.rustc = Some(fake_compiler(&dir).into_os_string());
    let host = dir.0.join("original host.rs");
    let object = dir.0.join("original host.o");
    fs::write(&host, b"abc").unwrap();
    fs::write(&object, b"xyz").unwrap();
    request.host = Some(host.clone());
    request.objects.push(object.clone());
    let exact = Limits {
        host_bytes: 3,
        object_bytes: 3,
        host_total: 6,
        executable: 6,
        ..Default::default()
    };
    l::link(&request, exact).unwrap();
    assert_eq!(fs::read(&request.output).unwrap(), b"linked");
    fs::remove_file(&request.output).unwrap();
    for limits in [
        Limits {
            object_bytes: 2,
            ..exact
        },
        Limits {
            host_total: 5,
            ..exact
        },
        Limits {
            executable: 5,
            ..exact
        },
    ] {
        assert!(matches!(
            l::link(&request, limits),
            Err(Error::Limit { .. })
        ));
        assert!(!request.output.exists());
        no_stages(&dir);
    }
    assert_eq!(fs::read(host).unwrap(), b"abc");
    assert_eq!(fs::read(object).unwrap(), b"xyz");
}

#[test]
fn concurrent_native_link_publishes_one_complete_executable() {
    let dir = Dir::new();
    let mut request = fixture(&dir);
    request.rustc = Some(fake_compiler(&dir).into_os_string());
    let barrier = std::sync::Barrier::new(2);
    let outcomes = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            l::link(&request, Limits::default())
        });
        let b = scope.spawn(|| {
            barrier.wait();
            l::link(&request, Limits::default())
        });
        [a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(fs::read(&request.output).unwrap(), b"linked");
    no_stages(&dir);
}

fn c_object(sdk_root: &Path, package: &Path) -> PathBuf {
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
    ok(cc.output().unwrap());
    object
}

#[test]
fn public_link_native_runs_all_profiles_without_source_or_installed_sdk_at_startup() {
    let dir = Dir::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let built = ok(Command::new(env!("CARGO"))
        .current_dir(&root)
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
        .unwrap());
    let sdk_root = dir.0.join("sdk");
    let manifest =
        lang::native::sdk_package::assemble(&built.stdout, &sdk_root, Default::default()).unwrap();
    let compiler = sdk_root.join(
        manifest
            .files()
            .iter()
            .find(|f| f.role() == m::Role::Compiler)
            .unwrap()
            .path(),
    );
    let c = fs::read_to_string(sdk_root.join("startup/c-guest.lang")).unwrap();
    let mut executables = Vec::new();
    for (name, text, expected) in [
        (
            "copy",
            "fn first()->i64{return 7;} fn main()->i64{return 42;}",
            "42",
        ),
        (
            "owned",
            include_str!("../docs/experiments/sdk-owned.lang"),
            "42",
        ),
        ("c", c.as_str(), ""),
        ("task", include_str!("../examples/scripts/tasks.lang"), "42"),
        (
            "task-owned",
            include_str!("fixtures/task_owned_buffers.lang"),
            "71",
        ),
    ] {
        let package = program(&compiler, &dir, name, text);
        if name == "task" {
            let manifest_path = sdk_root.join("sdk.json");
            let original = fs::read(&manifest_path).unwrap();
            let mut legacy: serde_json::Value = serde_json::from_slice(&original).unwrap();
            assert_eq!(legacy["version"], 2);
            legacy["version"] = serde_json::json!(1);
            fs::write(&manifest_path, serde_json::to_vec(&legacy).unwrap()).unwrap();
            let denied_output = dir.0.join("task-with-legacy-sdk");
            let denied = Command::new(&compiler)
                .current_dir(&dir.0)
                .arg("link-native")
                .arg("--sdk")
                .arg(&sdk_root)
                .arg("--package")
                .arg(&package)
                .arg("--output")
                .arg(&denied_output)
                .env("RUSTC", dir.0.join("must-not-start"))
                .output()
                .unwrap();
            assert_eq!(denied.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&denied.stderr).contains("task SDK version"));
            assert!(!denied_output.exists());
            fs::write(&manifest_path, original).unwrap();
        }
        let executable = dir.0.join(format!(
            "{name}-application{}",
            std::env::consts::EXE_SUFFIX
        ));
        let mut link = Command::new(&compiler);
        link.current_dir(&dir.0)
            .arg("link-native")
            .arg("--sdk")
            .arg(&sdk_root)
            .arg("--package")
            .arg(&package)
            .arg("--output")
            .arg(&executable)
            .env("CARGO", dir.0.join("no-cargo"))
            .env("CARGO_HOME", dir.0.join("empty-cargo-home"));
        if name == "c" {
            let denied_output = dir.0.join("c-without-host");
            let denied = Command::new(&compiler)
                .current_dir(&dir.0)
                .arg("link-native")
                .arg("--sdk")
                .arg(&sdk_root)
                .arg("--package")
                .arg(&package)
                .arg("--output")
                .arg(&denied_output)
                .env("RUSTC", dir.0.join("must-not-start"))
                .output()
                .unwrap();
            assert_eq!(denied.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&denied.stderr).contains("explicit --host"));
            assert!(!denied_output.exists());
            let object = c_object(&sdk_root, &package);
            link.arg("--host")
                .arg(sdk_root.join("startup/c-host.rs"))
                .arg("--object")
                .arg(object);
        }
        ok(link.output().unwrap());
        fs::rename(&package, dir.0.join(format!("{name}-package-away"))).unwrap();
        executables.push((name, expected, executable));
    }
    fs::rename(&sdk_root, dir.0.join("sdk-away")).unwrap();
    for (_name, expected, path) in executables {
        let output = ok(Command::new(path).current_dir(&dir.0).output().unwrap());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
    }
    no_stages(&dir);
}
