use lang::package::{lock::Lock, manifest::Manifest};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lang package commands {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn app(&self) {
        success(self.run(&["new", "app"]));
        fs::create_dir(self.0.join("app/deps")).unwrap();
        success(self.run(&["new", "app/deps/helper"]));
        self.write(
            "app/src/main.lang",
            "mod helper; fn main()->i64{return helper::answer();}\n",
        );
        self.write(
            "app/deps/helper/src/main.lang",
            "pub fn answer()->i64{return 42;} fn main()->i64{return -1;}\n",
        );
        let path = self.0.join("app/Lang.json");
        let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["dependencies"] = json!({"helper":{"path":"deps/helper","version":"0.1.0"}});
        fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    }
    fn lock(&self) -> Vec<u8> {
        fs::read(self.0.join("app/Lang.lock")).unwrap()
    }
    fn artifact(&self) -> PathBuf {
        self.0
            .join("app/target/lang")
            .join(lang_codegen_clif::sdk::digest(&self.lock()))
            .join("build/main.lbc")
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn diagnostic(output: &Output, code: &str) -> Value {
    assert_eq!(output.status.code(), Some(1), "{:?}", output);
    assert!(output.stdout.is_empty());
    let lines: Vec<_> = output
        .stderr
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(lines[0]).unwrap();
    assert_eq!(value["code"], code);
    assert_eq!(value["schema"], 1);
    assert_eq!(value["kind"], "diagnostic");
    value
}

#[test]
fn new_build_run_and_locked_offline_dependency_are_real_cli_operations() {
    let f = Files::new();
    f.app();
    let manifest = Manifest::read(&f.0.join("app/Lang.json")).unwrap();
    assert_eq!(manifest.package.unwrap().name, "app");
    assert_eq!(
        fs::read(f.0.join("app/tests/smoke.lang")).unwrap(),
        b"fn main() -> i64 {\n    return 0;\n}\n"
    );
    let built = success(f.run(&["build", "--manifest-path", "app/Lang.json"]));
    assert!(built.stdout.is_empty());
    assert!(built.stderr.is_empty());
    let lock = f.lock();
    let artifact = f.artifact();
    let bytes = fs::read(&artifact).unwrap();
    assert!(lang::artifact::decode(&bytes, Default::default()).is_ok());
    assert_eq!(
        fs::read(artifact.parent().unwrap().join("inputs.lock")).unwrap(),
        lock
    );
    let run = success(f.run(&[
        "run",
        "--manifest-path",
        "app/Lang.json",
        "--locked",
        "--offline",
        "--message-format",
        "json",
    ]));
    assert_eq!(run.stdout, b"42\n");
    assert!(run.stderr.is_empty());
    assert_eq!(f.lock(), lock);
    assert_eq!(fs::read(&artifact).unwrap(), bytes);
    let default = success(
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .current_dir(f.0.join("app"))
            .arg("run")
            .output()
            .unwrap(),
    );
    assert_eq!(default.stdout, b"42\n");
    assert!(default.stderr.is_empty());
    let nested = Command::new(env!("CARGO_BIN_EXE_lang"))
        .current_dir(f.0.join("app/src"))
        .args(["run", "--message-format", "json"])
        .output()
        .unwrap();
    diagnostic(&nested, "PKG0005");
    assert_eq!(f.lock(), lock);
    let external = success(f.run(&["run", "--artifact", artifact.to_str().unwrap()]));
    assert_eq!(external.stdout, b"42\n");
    let replay = success(f.run(&[
        "build",
        "--manifest-path",
        "app/Lang.json",
        "--locked",
        "--offline",
    ]));
    assert!(replay.stdout.is_empty());
    let target = if cfg!(windows) {
        "x86_64-pc-windows-msvc"
    } else {
        "aarch64-apple-darwin"
    };
    let graph = lang::package::graph::Graph::resolve(
        &f.0.join("app/Lang.json"),
        target,
        Default::default(),
    )
    .unwrap();
    Lock::capture(&graph).unwrap().verify(&lock).unwrap();
}

#[test]
fn stale_locked_input_refuses_before_execution_and_preserves_prior_outputs() {
    let f = Files::new();
    f.app();
    success(f.run(&["build", "--manifest-path", "app/Lang.json"]));
    let lock = f.lock();
    let artifact = f.artifact();
    let bytes = fs::read(&artifact).unwrap();
    f.write("app/deps/helper/src/main.lang", "invalid changed source\n");
    let output = f.run(&[
        "run",
        "--manifest-path",
        "app/Lang.json",
        "--locked",
        "--offline",
        "--message-format",
        "json",
    ]);
    diagnostic(&output, "PKG0007");
    assert_eq!(f.lock(), lock);
    assert_eq!(fs::read(artifact).unwrap(), bytes);
}

#[test]
fn runtime_failure_and_tampered_cache_preserve_the_previous_lock_and_artifact() {
    let f = Files::new();
    f.app();
    success(f.run(&["build", "--manifest-path", "app/Lang.json"]));
    let lock = f.lock();
    let artifact = f.artifact();
    let bytes = fs::read(&artifact).unwrap();
    f.write(
        "app/src/main.lang",
        "fn forever()->i64{return forever();} fn main()->i64{return forever();}\n",
    );
    diagnostic(
        &f.run(&[
            "run",
            "--manifest-path",
            "app/Lang.json",
            "--fuel",
            "30",
            "--message-format",
            "json",
        ]),
        "CLI0002",
    );
    assert_eq!(f.lock(), lock);
    assert_eq!(fs::read(&artifact).unwrap(), bytes);
    f.write(
        "app/src/main.lang",
        "mod helper; fn main()->i64{return helper::answer();}\n",
    );
    let mut corrupt = bytes.clone();
    corrupt[0] ^= 1;
    fs::write(&artifact, &corrupt).unwrap();
    diagnostic(
        &f.run(&[
            "run",
            "--manifest-path",
            "app/Lang.json",
            "--message-format",
            "json",
        ]),
        "PKG0007",
    );
    assert_eq!(f.lock(), lock);
    assert_eq!(fs::read(artifact).unwrap(), corrupt);
}

#[test]
fn new_refuses_existing_destinations_and_supports_explicit_names_with_spaced_paths() {
    let f = Files::new();
    fs::create_dir(f.0.join("existing")).unwrap();
    diagnostic(
        &f.run(&["new", "existing", "--message-format", "json"]),
        "PKG0005",
    );
    assert_eq!(fs::read_dir(f.0.join("existing")).unwrap().count(), 0);
    f.write("file", "original");
    diagnostic(
        &f.run(&["new", "file", "--message-format", "json"]),
        "PKG0005",
    );
    assert_eq!(fs::read(f.0.join("file")).unwrap(), b"original");
    success(f.run(&["new", "directory with spaces", "--name", "named_app"]));
    let manifest = Manifest::read(&f.0.join("directory with spaces/Lang.json")).unwrap();
    assert_eq!(manifest.package.unwrap().name, "named_app");
    let run = success(f.run(&["run", "--manifest-path", "directory with spaces/Lang.json"]));
    assert_eq!(run.stdout, b"0\n");
    assert!(!fs::read_dir(&f.0).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lang-stage-")
    }));
}

#[test]
fn package_usage_is_structured_and_explicit_file_run_remains_compatible() {
    let f = Files::new();
    f.write("-source.lang", "fn main()->i64{return 17;}\n");
    assert_eq!(success(f.run(&["run", "-source.lang"])).stdout, b"17\n");
    f.write("--manifest-path/entry.lang", "fn main()->i64{return 19;}\n");
    assert_eq!(
        success(f.run(&[
            "run",
            "--source-root",
            "--manifest-path",
            "./--manifest-path/entry.lang"
        ]))
        .stdout,
        b"19\n"
    );
    assert_eq!(
        success(f.run(&["-1", "2"])).stdout,
        b"bytecode: 1\nnative:   1\n"
    );
    for args in [
        vec!["build", "--message-format", "json", "--unknown"],
        vec![
            "run",
            "file.lang",
            "--manifest-path",
            "Lang.json",
            "--message-format",
            "json",
        ],
        vec!["build", "--locked", "--locked", "--message-format", "json"],
        vec!["new", "--message-format", "json"],
    ] {
        let out = f.run(&args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        let d: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(d["code"], "CLI0001");
    }
    let help = success(f.run(&["run", "--help"]));
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("lang run --artifact")
    );
    assert!(!f.0.join("target").exists());
}

#[test]
fn ownership_and_task_artifacts_use_the_existing_package_vm_profiles() {
    for (example, wanted, kind) in [
        ("owned_aggregate.lang", 9, "owned"),
        ("tasks.lang", 42, "tasks"),
    ] {
        let f = Files::new();
        success(f.run(&["new", "app"]));
        let source = fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples/scripts")
                .join(example),
        )
        .unwrap();
        f.write("app/src/main.lang", &source);
        success(f.run(&["build", "--manifest-path", "app/Lang.json"]));
        let program =
            lang::artifact::decode(&fs::read(f.artifact()).unwrap(), Default::default()).unwrap();
        assert!(matches!(
            (kind, program),
            ("owned", lang::artifact::Program::Ownership(_))
                | ("tasks", lang::artifact::Program::Tasks(_))
        ));
        let result = success(f.run(&[
            "run",
            "--manifest-path",
            "app/Lang.json",
            "--locked",
            "--offline",
            "--message-format",
            "json",
        ]));
        assert_eq!(result.stdout, format!("{wanted}\n").as_bytes());
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn source_json_diagnostics_keep_original_utf8_byte_offsets_without_rewriting_outputs() {
    let f = Files::new();
    f.app();
    success(f.run(&["build", "--manifest-path", "app/Lang.json"]));
    let lock = f.lock();
    let artifact = f.artifact();
    let bytes = fs::read(&artifact).unwrap();
    let source = "// 日本語\nfn main() -> i64 { return missing; }\n";
    f.write("app/src/main.lang", source);
    let output = f.run(&[
        "build",
        "--manifest-path",
        "app/Lang.json",
        "--message-format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let records: Vec<Value> = output
        .stderr
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).unwrap())
        .collect();
    let diagnostic = records
        .iter()
        .find(|d| d["code"] == "SRC0002")
        .expect("original source identity");
    assert_eq!(
        diagnostic["path"].as_str().unwrap().replace('\\', "/"),
        "src/main.lang"
    );
    let start = source.find("missing").unwrap();
    assert_eq!(diagnostic["span"], json!({"start":start,"end":start+7}));
    assert_eq!(f.lock(), lock);
    assert_eq!(fs::read(artifact).unwrap(), bytes);
}

#[test]
fn reserved_output_aliases_and_output_source_names_are_refused_without_writes() {
    let f = Files::new();
    f.app();
    fs::hard_link(f.0.join("app/Lang.json"), f.0.join("app/Lang.lock")).unwrap();
    let original = fs::read(f.0.join("app/Lang.json")).unwrap();
    diagnostic(
        &f.run(&[
            "build",
            "--manifest-path",
            "app/Lang.json",
            "--message-format",
            "json",
        ]),
        "PKG0003",
    );
    assert_eq!(fs::read(f.0.join("app/Lang.lock")).unwrap(), original);
    assert_eq!(fs::read(f.0.join("app/Lang.json")).unwrap(), original);
    assert!(!f.0.join("app/target").exists());
}

#[cfg(unix)]
#[test]
fn output_directory_links_cannot_redirect_publication_into_sources() {
    use std::os::unix::fs::symlink;
    let f = Files::new();
    f.app();
    let source = fs::read(f.0.join("app/src/main.lang")).unwrap();
    symlink("src", f.0.join("app/target")).unwrap();
    diagnostic(
        &f.run(&[
            "build",
            "--manifest-path",
            "app/Lang.json",
            "--message-format",
            "json",
        ]),
        "PKG0003",
    );
    assert_eq!(fs::read(f.0.join("app/src/main.lang")).unwrap(), source);
    assert!(!f.0.join("app/src/lang").exists());
    assert!(!f.0.join("app/Lang.lock").exists());
}

#[cfg(unix)]
#[test]
fn publication_uses_the_retained_directory_after_its_display_path_is_replaced() {
    let f = Files::new();
    f.app();
    let original = f.0.join("app");
    let moved = f.0.join("moved");
    let args = [
        "run".into(),
        "--manifest-path".into(),
        original.join("Lang.json").into_os_string(),
    ];
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = lang::package::command::run(&args, &mut stdout, &mut stderr, |program, _| {
        assert!(matches!(program, lang::artifact::Program::Copy(_)));
        fs::rename(&original, &moved).unwrap();
        fs::create_dir(&original).unwrap();
        fs::write(original.join("sentinel"), b"keep this directory").unwrap();
        Ok(42)
    })
    .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout, b"42\n");
    assert!(stderr.is_empty());
    assert_eq!(fs::read_dir(&original).unwrap().count(), 1);
    assert_eq!(
        fs::read(original.join("sentinel")).unwrap(),
        b"keep this directory"
    );
    let lock = fs::read(moved.join("Lang.lock")).unwrap();
    let generation = moved
        .join("target/lang")
        .join(lang_codegen_clif::sdk::digest(&lock))
        .join("build");
    assert_eq!(fs::read(generation.join("inputs.lock")).unwrap(), lock);
    assert!(
        lang::artifact::decode(
            &fs::read(generation.join("main.lbc")).unwrap(),
            Default::default()
        )
        .is_ok()
    );
}

#[test]
fn concurrent_builds_adopt_one_complete_generation_and_leave_no_private_stages() {
    let f = Files::new();
    f.app();
    let workers: Vec<_> = (0..4)
        .map(|_| {
            Command::new(env!("CARGO_BIN_EXE_lang"))
                .current_dir(&f.0)
                .args(["build", "--manifest-path", "app/Lang.json"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for worker in workers {
        let output = success(worker.wait_with_output().unwrap());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    let generation = f.artifact().parent().unwrap().to_path_buf();
    assert_eq!(fs::read(generation.join("inputs.lock")).unwrap(), f.lock());
    assert!(lang::artifact::decode(&fs::read(f.artifact()).unwrap(), Default::default()).is_ok());
    let key = generation.parent().unwrap();
    assert_eq!(fs::read_dir(key).unwrap().count(), 1);
    assert!(!fs::read_dir(f.0.join("app")).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".Lang.lock.stage-")
    }));
}

#[test]
fn an_incomplete_or_extended_generation_is_refused_without_replacement() {
    let f = Files::new();
    f.app();
    success(f.run(&["build", "--manifest-path", "app/Lang.json"]));
    let lock = f.lock();
    let artifact = f.artifact();
    let generation = artifact.parent().unwrap();
    fs::write(generation.join("unexpected"), b"keep me").unwrap();
    diagnostic(
        &f.run(&[
            "build",
            "--manifest-path",
            "app/Lang.json",
            "--message-format",
            "json",
        ]),
        "PKG0007",
    );
    assert_eq!(fs::read(generation.join("unexpected")).unwrap(), b"keep me");
    fs::remove_file(generation.join("unexpected")).unwrap();
    fs::remove_file(&artifact).unwrap();
    fs::remove_file(generation.join("inputs.lock")).unwrap();
    diagnostic(
        &f.run(&[
            "build",
            "--manifest-path",
            "app/Lang.json",
            "--message-format",
            "json",
        ]),
        "PKG0005",
    );
    assert_eq!(fs::read_dir(generation).unwrap().count(), 0);
    assert_eq!(f.lock(), lock);
}

#[cfg(windows)]
#[test]
fn a_busy_final_lock_preserves_the_previous_generation_and_cleans_stages() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Files::new();
    f.app();
    success(f.run(&["build", "--manifest-path", "app/Lang.json"]));
    let lock = f.lock();
    let artifact = f.artifact();
    let bytes = fs::read(&artifact).unwrap();
    f.write("app/src/main.lang", "fn main()->i64{return 99;}\n");
    let busy = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(f.0.join("app/Lang.lock"))
        .unwrap();
    diagnostic(
        &f.run(&[
            "run",
            "--manifest-path",
            "app/Lang.json",
            "--message-format",
            "json",
        ]),
        "PKG0005",
    );
    assert_eq!(f.lock(), lock);
    assert_eq!(fs::read(&artifact).unwrap(), bytes);
    for key in fs::read_dir(f.0.join("app/target/lang")).unwrap() {
        let entries: Vec<_> = fs::read_dir(key.unwrap().path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, ["build"]);
    }
    assert!(!fs::read_dir(f.0.join("app")).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".Lang.lock.stage-")
    }));
    drop(busy);
    assert_eq!(
        success(f.run(&["run", "--manifest-path", "app/Lang.json"])).stdout,
        b"99\n"
    );
}
