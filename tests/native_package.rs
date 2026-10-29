//! Native package reproducibility through the existing SDK and linker.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
use lang::{
    frontend,
    native::{self, link, sdk_package},
    package::{graph::Graph, lock::Lock, source},
};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("lang package native {}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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
fn refused(output: Output, code: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let diagnostic: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(diagnostic["code"], code);
}

#[test]
fn relocated_locked_sibling_program_produces_identical_native_executables() {
    let files = Files::new();
    let built = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "--offline",
            "--all-features",
            "--bin",
            "lang",
            "--example",
            "native-sdk",
            "--message-format=json",
        ])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let sdk = files.0.join("sdk");
    sdk_package::assemble(&built.stdout, &sdk, Default::default()).unwrap();
    files.write(
        "workspace/Lang.json",
        r#"{"schema":1,"workspace":{"members":["app","helper"]}}"#,
    );
    files.write("workspace/app/Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"../helper","version":"0.1.0"}}}"#);
    files.write("workspace/helper/Lang.json", r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
    files.write(
        "workspace/app/src/main.lang",
        "mod helper; fn main()->i64{return helper::answer();}",
    );
    files.write(
        "workspace/helper/src/main.lang",
        "pub fn answer()->i64{return 42;}",
    );
    let target = lang_codegen_clif::Triple::host();
    let mut expected_lock: Option<Vec<u8>> = None;
    let mut digests = Vec::new();
    let mut cli_digests = Vec::new();
    let mut cli_relative = String::new();
    for (index, root) in ["workspace", "relocated"].into_iter().enumerate() {
        if index == 1 {
            fs::rename(files.0.join("workspace"), files.0.join("relocated")).unwrap();
        }
        let graph = Graph::resolve_selected(
            &files.0.join(root).join("Lang.json"),
            &target.to_string(),
            Default::default(),
            Some("app"),
        )
        .unwrap();
        let lock = Lock::capture(&graph).unwrap();
        if let Some(expected) = &expected_lock {
            lock.verify(expected).unwrap();
        } else {
            expected_lock = Some(lock.as_bytes().to_vec());
        }
        let analysis = source::analyze(&graph, Default::default()).unwrap();
        assert!(analysis.result.is_ok(), "{:?}", analysis.diagnostics);
        let package = native::compile(
            analysis.result.unwrap(),
            &analysis.sources,
            frontend::CompileLimits::default(),
            target.clone(),
        )
        .unwrap();
        let package_root = files.0.join(format!("package-{index}"));
        if index == 1 {
            for file in package.files() {
                let previous = fs::read(files.0.join("package-0").join(file.path)).unwrap();
                assert_eq!(
                    lang_codegen_clif::sdk::digest(&file.bytes),
                    lang_codegen_clif::sdk::digest(&previous),
                    "native package input {} changed after relocation",
                    file.path
                );
            }
        }
        package.publish(&package_root).unwrap();
        let executable = files.0.join(format!(
            "application-{index}{}",
            std::env::consts::EXE_SUFFIX
        ));
        link::link(
            &link::Request {
                rustc: None,
                sdk: sdk.clone(),
                package: package_root,
                host: None,
                objects: Vec::new(),
                output: executable.clone(),
            },
            Default::default(),
        )
        .unwrap();
        let output = Command::new(&executable).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"42\n");
        digests.push(lang_codegen_clif::sdk::digest(
            &fs::read(&executable).unwrap(),
        ));
        if index == 1 {
            #[cfg(windows)]
            {
                let stamp = |bytes: &[u8]| {
                    let offset = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
                    u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap())
                };
                let first = fs::read(files.0.join("application-0.exe")).unwrap();
                let second = fs::read(&executable).unwrap();
                eprintln!(
                    "native PE timestamps: {:#x} / {:#x}",
                    stamp(&first),
                    stamp(&second)
                );
            }
            assert_eq!(
                digests[0], digests[1],
                "direct native link differs after relocation"
            );
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_lang"));
        command
            .current_dir(files.0.join(root))
            .args(["build", "--package", "app", "--native", "--sdk"])
            .arg(&sdk);
        if index == 1 {
            command.args(["--locked", "--offline", "--message-format", "json"]);
        }
        let output = success(command.output().unwrap());
        let relative = if index == 0 {
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        } else {
            let artifact: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(artifact["schema"], 1);
            assert_eq!(artifact["kind"], "artifact");
            artifact["path"].as_str().unwrap().to_owned()
        };
        if index == 0 {
            cli_relative.clone_from(&relative);
        } else {
            assert_eq!(relative, cli_relative);
        }
        let executable = files.0.join(root).join(&relative);
        let generation = executable.parent().unwrap();
        let bytes = fs::read(&executable).unwrap();
        let executable_hash = lang_codegen_clif::sdk::digest(&bytes);
        cli_digests.push(executable_hash.clone());
        let receipt: Value =
            serde_json::from_slice(&fs::read(generation.join("build.json")).unwrap()).unwrap();
        assert_eq!(receipt["schema"], 1);
        assert_eq!(
            receipt["rustc_release"],
            lang_codegen_clif::sdk::RUSTC_RELEASE
        );
        assert_eq!(
            receipt["rustc_commit"],
            lang_codegen_clif::sdk::RUSTC_COMMIT
        );
        assert_eq!(
            receipt["compiler_build_id"],
            lang_codegen_clif::sdk::build_id(&target)
        );
        assert_eq!(
            receipt["package_manifest_sha256"],
            lang_codegen_clif::sdk::digest(&fs::read(generation.join("package.json")).unwrap())
        );
        assert_eq!(receipt["target"], target.to_string());
        assert_eq!(receipt["executable_sha256"], executable_hash);
        assert_eq!(
            receipt["lock_sha256"],
            lang_codegen_clif::sdk::digest(lock.as_bytes())
        );
        assert_eq!(
            receipt["sdk_manifest_sha256"],
            lang_codegen_clif::sdk::digest(&fs::read(sdk.join("sdk.json")).unwrap())
        );
        assert_eq!(
            fs::read(generation.join("sdk-input.json")).unwrap(),
            fs::read(sdk.join("sdk.json")).unwrap()
        );
        assert_eq!(
            fs::read(generation.join("inputs.lock")).unwrap(),
            lock.as_bytes()
        );
        lock.verify(&fs::read(files.0.join(root).join("Lang.lock")).unwrap())
            .unwrap();
        assert_eq!(
            success(Command::new(&executable).output().unwrap()).stdout,
            b"42\n"
        );
    }
    assert_eq!(
        digests[0], digests[1],
        "native executable bytes differ after relocation"
    );
    assert_eq!(cli_digests[0], cli_digests[1]);
    let root = files.0.join("relocated");
    let executable = root.join(&cli_relative);
    let old_lock = fs::read(root.join("Lang.lock")).unwrap();
    let old_executable = fs::read(&executable).unwrap();
    let run = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lang"));
        command
            .current_dir(&root)
            .args(["build", "--package", "app", "--native", "--sdk"])
            .arg(&sdk)
            .args(["--locked", "--offline", "--message-format", "json"]);
        command
    };
    files.write(
        "relocated/helper/src/main.lang",
        "pub fn answer()->i64{return 43;}",
    );
    refused(run().output().unwrap(), "PKG0007");
    files.write(
        "relocated/helper/src/main.lang",
        "pub fn answer()->i64{return 42;}",
    );
    let sdk_bytes = fs::read(sdk.join("sdk.json")).unwrap();
    let mut invalid_sdk: Value = serde_json::from_slice(&sdk_bytes).unwrap();
    invalid_sdk["build_id"] = Value::String("0".repeat(64));
    fs::write(
        sdk.join("sdk.json"),
        serde_json::to_vec(&invalid_sdk).unwrap(),
    )
    .unwrap();
    refused(run().output().unwrap(), "CLI0002");
    fs::write(sdk.join("sdk.json"), sdk_bytes).unwrap();
    refused(
        run()
            .env("RUSTC", files.0.join("missing-rustc"))
            .output()
            .unwrap(),
        "CLI0002",
    );
    assert_eq!(fs::read(root.join("Lang.lock")).unwrap(), old_lock);
    assert_eq!(fs::read(&executable).unwrap(), old_executable);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o600)).unwrap();
        refused(run().output().unwrap(), "PKG0007");
        assert_eq!(fs::read(&executable).unwrap(), old_executable);
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let installed = files
        .0
        .join(format!("installed{}", std::env::consts::EXE_SUFFIX));
    fs::copy(&executable, &installed).unwrap();
    for path in [
        &root,
        &sdk,
        &files.0.join("package-0"),
        &files.0.join("package-1"),
    ] {
        fs::remove_dir_all(path).unwrap();
    }
    assert_eq!(
        success(Command::new(&installed).output().unwrap()).stdout,
        b"42\n"
    );
}

#[test]
fn native_build_options_require_an_explicit_sdk_and_the_host_target() {
    use lang::package::command::{self, Command as Parsed};
    let args = |values: &[&str]| {
        values
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>()
    };
    for values in [
        vec!["build", "--native"],
        vec!["build", "--sdk", "sdk"],
        vec!["build", "--native", "--sdk"],
        vec!["build", "--native", "--native", "--sdk", "sdk"],
        vec!["run", "--native", "--sdk", "sdk"],
        vec!["test", "--native", "--sdk", "sdk"],
        vec!["new", "--native", "--sdk", "sdk", "app"],
    ] {
        let Err(error) = command::parse(&args(&values)) else {
            panic!("invalid native options accepted");
        };
        assert_eq!(error.code, "CLI0001");
    }
    let Parsed::Package(request) =
        command::parse(&args(&["build", "--native", "--sdk", "sdk"])).unwrap()
    else {
        panic!("native request");
    };
    assert_eq!(request.native_sdk, Some(PathBuf::from("sdk")));
    let other = if lang_codegen_clif::Triple::host().to_string() == "aarch64-apple-darwin" {
        "x86_64-pc-windows-msvc"
    } else {
        "aarch64-apple-darwin"
    };
    let Err(error) = command::parse(&args(&[
        "build", "--native", "--sdk", "sdk", "--target", other,
    ])) else {
        panic!("cross-host native link accepted");
    };
    assert_eq!(error.code, "PKG0003");
}
