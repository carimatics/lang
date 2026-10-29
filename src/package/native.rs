//! Frozen package-native composition; guest code is never executed here.
use super::{Diagnostic, command::Request, lock::Lock, output};
use crate::native::{self, link};
use serde::Serialize;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn tool(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("CLI0002", error.to_string())
}
struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Result<Self, Diagnostic> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let path = std::env::temp_dir().join(format!(
                "lang-package-native-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(tool(e)),
            }
        }
        Err(Diagnostic::error(
            "PKG0004",
            "native temporary directory capacity exceeded",
        ))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[derive(Serialize)]
struct Receipt<'a> {
    schema: u32,
    target: &'a str,
    compiler_build_id: String,
    lock_sha256: String,
    sdk_manifest_sha256: String,
    rustc_release: &'a str,
    rustc_commit: &'a str,
    package_manifest_sha256: String,
    executable_sha256: String,
}
pub(super) struct Built {
    pub generation: String,
    pub relative_path: String,
    package: native::Package,
    sdk_manifest: Vec<u8>,
    receipt: Vec<u8>,
    executable: Vec<u8>,
    executable_name: &'static str,
}
impl Built {
    pub fn files(&self) -> Vec<output::File<'_>> {
        self.package
            .files()
            .iter()
            .map(|f| output::File {
                path: f.path,
                bytes: &f.bytes,
                executable: false,
            })
            .chain([
                output::File {
                    path: "sdk-input.json",
                    bytes: &self.sdk_manifest,
                    executable: false,
                },
                output::File {
                    path: "build.json",
                    bytes: &self.receipt,
                    executable: false,
                },
                output::File {
                    path: self.executable_name,
                    bytes: &self.executable,
                    executable: true,
                },
            ])
            .collect()
    }
}
pub(super) fn build(
    package: native::Package,
    request: &Request,
    lock: &Lock,
) -> Result<Built, Diagnostic> {
    let mut total = lock.as_bytes().len();
    for f in package.files() {
        total = total
            .checked_add(f.bytes.len())
            .filter(|n| *n <= output::MAX_OUTPUT)
            .ok_or_else(|| {
                Diagnostic::error("PKG0004", "native package output byte limit exceeded")
            })?;
    }
    let stage = Temporary::new()?;
    let program = stage.0.join("program");
    package.publish(&program).map_err(tool)?;
    let executable_name = if cfg!(windows) {
        "application.exe"
    } else {
        "application"
    };
    let executable_path = stage.0.join(executable_name);
    let inputs = link::link_recorded(
        &link::Request {
            rustc: std::env::var_os("RUSTC"),
            sdk: request.native_sdk.clone().expect("native SDK request"),
            package: program,
            host: None,
            objects: Vec::new(),
            output: executable_path.clone(),
        },
        link::Limits {
            executable: output::MAX_OUTPUT.saturating_sub(total) as u64,
            ..Default::default()
        },
    )
    .map_err(tool)?;
    let executable = fs::read(executable_path).map_err(tool)?;
    let digest = lang_codegen_clif::sdk::digest;
    let sdk_hash = digest(&inputs.sdk_manifest);
    let target = request.target.parse().map_err(tool)?;
    let package_manifest = package
        .files()
        .iter()
        .find(|f| f.path == "package.json")
        .expect("native manifest");
    let mut receipt = serde_json::to_vec_pretty(&Receipt {
        schema: 1,
        target: &request.target,
        compiler_build_id: lang_codegen_clif::sdk::build_id(&target),
        lock_sha256: digest(lock.as_bytes()),
        sdk_manifest_sha256: sdk_hash.clone(),
        rustc_release: &inputs.rustc_release,
        rustc_commit: &inputs.rustc_commit,
        package_manifest_sha256: digest(&package_manifest.bytes),
        executable_sha256: digest(&executable),
    })
    .map_err(tool)?;
    receipt.push(b'\n');
    for bytes in [&inputs.sdk_manifest, &receipt, &executable] {
        total = total
            .checked_add(bytes.len())
            .filter(|n| *n <= output::MAX_OUTPUT)
            .ok_or_else(|| {
                Diagnostic::error("PKG0004", "native package output byte limit exceeded")
            })?;
    }
    let generation = format!("native-{sdk_hash}");
    Ok(Built {
        relative_path: format!(
            "target/lang/{}/{generation}/{executable_name}",
            digest(lock.as_bytes())
        ),
        generation,
        package,
        sdk_manifest: inputs.sdk_manifest,
        receipt,
        executable,
        executable_name,
    })
}
