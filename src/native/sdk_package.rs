//! Host SDK assembly from the exact successful Cargo build, never a deps scan.
//! Cargo output and native files are trusted build inputs, not authentication.
use super::{Stage, manifest, publish};
use lang_codegen_clif::{Triple, sdk};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const CARGO_OUTPUT_LIMIT: usize = 32 * 1024 * 1024;
#[derive(Debug)]
pub enum Error {
    Json {
        line: usize,
        error: serde_json::Error,
    },
    Build(BuildIssue),
    MissingArchive {
        name: String,
    },
    Collision {
        destination: String,
        first: PathBuf,
        second: PathBuf,
    },
    Limit {
        resource: &'static str,
        actual: u64,
        maximum: u64,
    },
    Native(super::Error),
    Manifest(manifest::Error),
}
#[derive(Debug)]
pub enum BuildIssue {
    PanicStrategy,
    AbsoluteArtifactPath,
    ArtifactFilename,
    ResourceCollision,
    FinishRecord,
    ArtifactTarget,
    ArtifactKind,
    DuplicateCompiler,
    CompilerExecutable,
    DuplicateRuntime,
    ArtifactFilenames,
    IncompleteBuild,
    RegularFile,
    TotalOverflow,
    InputChangedSize,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if matches!(self, Self::Build(BuildIssue::PanicStrategy)) {
            return f.write_str("native SDK assembly requires panic=unwind for host-panic containment/poison; rebuild without panic=abort");
        }
        write!(f, "native SDK assembly: {self:?}")
    }
}
impl std::error::Error for Error {}
fn io(stage: &'static str, path: &Path, e: std::io::Error) -> Error {
    Error::Native(super::io(stage, path, e))
}
fn bound(resource: &'static str, actual: u64, maximum: u64) -> Result<(), Error> {
    if actual > maximum {
        Err(Error::Limit {
            resource,
            actual,
            maximum,
        })
    } else {
        Ok(())
    }
}
enum Source {
    File(PathBuf),
    Bytes(&'static [u8]),
}
struct Asset {
    role: &'static str,
    source: Source,
}
struct Inventory {
    assets: BTreeMap<String, Asset>,
}
impl Inventory {
    fn file(&mut self, role: &'static str, directory: &str, source: &Path) -> Result<(), Error> {
        if !source.is_absolute() {
            return Err(Error::Build(BuildIssue::AbsoluteArtifactPath));
        }
        let name = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(Error::Build(BuildIssue::ArtifactFilename))?;
        let destination = format!("{directory}/{name}");
        if let Some(old) = self.assets.get(&destination) {
            if let Source::File(first) = &old.source {
                if first == source && old.role == role {
                    return Ok(());
                }
                return Err(Error::Collision {
                    destination,
                    first: first.clone(),
                    second: source.to_owned(),
                });
            }
            return Err(Error::Build(BuildIssue::ResourceCollision));
        }
        self.assets.insert(
            destination,
            Asset {
                role,
                source: Source::File(source.to_owned()),
            },
        );
        Ok(())
    }
    fn bytes(&mut self, role: &'static str, name: &str, bytes: &'static [u8]) {
        self.assets.insert(
            name.into(),
            Asset {
                role,
                source: Source::Bytes(bytes),
            },
        );
    }
    fn cargo(bytes: &[u8]) -> Result<Self, Error> {
        bound(
            "Cargo output bytes",
            bytes.len() as u64,
            CARGO_OUTPUT_LIMIT as u64,
        )?;
        let mut result = Self {
            assets: BTreeMap::new(),
        };
        let (mut compiler, mut runtime, mut finished) = (false, false, false);
        let mut native_paths = BTreeSet::new();
        let mut static_libraries = BTreeSet::new();
        for (index, line) in bytes.split(|b| *b == b'\n').enumerate() {
            let line = line.trim_ascii_start();
            if !line.starts_with(b"{") {
                continue;
            }
            let v: Value = serde_json::from_slice(line).map_err(|error| Error::Json {
                line: index + 1,
                error,
            })?;
            match v["reason"].as_str() {
                Some("build-finished") => {
                    if finished || v["success"] != true {
                        return Err(Error::Build(BuildIssue::FinishRecord));
                    }
                    finished = true;
                }
                Some("build-script-executed") => {
                    for path in v["linked_paths"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        let path = match path.split_once('=') {
                            Some(("native" | "all", path)) => path,
                            Some(("dependency" | "crate" | "framework", _)) => continue,
                            _ => path,
                        };
                        native_paths.insert(PathBuf::from(path));
                    }
                    for lib in v["linked_libs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        if let Some((kind, name)) = lib.split_once('=')
                            && kind.starts_with("static")
                        {
                            static_libraries.insert(name.to_owned());
                        }
                    }
                }
                Some("compiler-artifact") => {
                    if v["profile"]["test"] == true {
                        continue;
                    }
                    let name = v["target"]["name"]
                        .as_str()
                        .ok_or(Error::Build(BuildIssue::ArtifactTarget))?;
                    let kinds = v["target"]["kind"]
                        .as_array()
                        .ok_or(Error::Build(BuildIssue::ArtifactKind))?;
                    let kind = |s: &str| kinds.iter().any(|v| v == s);
                    if name == "lang" && kind("bin") {
                        if compiler {
                            return Err(Error::Build(BuildIssue::DuplicateCompiler));
                        }
                        let exe = v["executable"]
                            .as_str()
                            .ok_or(Error::Build(BuildIssue::CompilerExecutable))?;
                        result.file("compiler", "bin", Path::new(exe))?;
                        compiler = true;
                    }
                    for file in v["filenames"]
                        .as_array()
                        .ok_or(Error::Build(BuildIssue::ArtifactFilenames))?
                        .iter()
                        .filter_map(Value::as_str)
                    {
                        let path = Path::new(file);
                        if path.extension().is_some_and(|s| s == "rlib") {
                            let role = if name == "lang_codegen_clif" {
                                if runtime {
                                    return Err(Error::Build(BuildIssue::DuplicateRuntime));
                                }
                                runtime = true;
                                "runtime"
                            } else {
                                "dependency"
                            };
                            result.file(role, "lib", path)?;
                        } else if kind("proc-macro")
                            && path
                                .extension()
                                .is_some_and(|s| s == "dll" || s == "dylib" || s == "so")
                        {
                            result.file("dependency", "lib", path)?;
                        }
                    }
                }
                _ => {}
            }
        }
        if !compiler || !runtime || !finished {
            return Err(Error::Build(BuildIssue::IncompleteBuild));
        }
        for directory in native_paths {
            let entries = fs::read_dir(&directory)
                .map_err(|e| io("native dependency directory", &directory, e))?;
            for entry in entries {
                let path = entry
                    .map_err(|e| io("native dependency entry", &directory, e))?
                    .path();
                if path
                    .extension()
                    .is_some_and(|s| s == "lib" || s == "a" || s == "dll" || s == "dylib")
                {
                    result.file("dependency", "native", &path)?;
                }
            }
        }
        for name in static_libraries {
            if !result.assets.contains_key(&format!("native/{name}.lib"))
                && !result.assets.contains_key(&format!("native/lib{name}.a"))
            {
                return Err(Error::MissingArchive { name });
            }
        }
        result.bytes(
            "header",
            "include/lang/runtime.h",
            concat!(
                "#include <stdint.h>\n#include <stddef.h>\n",
                include_str!("../../crates/runtime/src/c_abi/support.h")
            )
            .as_bytes(),
        );
        result.bytes(
            "startup",
            "startup/copy.rs",
            include_bytes!("sdk_startup/copy.rs"),
        );
        result.bytes(
            "startup",
            "startup/owned.rs",
            include_bytes!("sdk_startup/owned.rs"),
        );
        result.bytes(
            "startup",
            "startup/c-host.rs",
            include_bytes!("sdk_startup/c-host.rs"),
        );
        result.bytes(
            "startup",
            "startup/c-client.c.in",
            include_bytes!("sdk_startup/c-client.c.in"),
        );
        result.bytes(
            "startup",
            "startup/c-guest.lang",
            include_bytes!("sdk_startup/c-guest.lang"),
        );
        result.bytes(
            "documentation",
            "docs/native-sdk.md",
            include_bytes!("../../docs/native-sdk.md"),
        );
        result.bytes(
            "startup",
            "startup/component-producer.rs",
            include_bytes!("sdk_startup/component-producer.rs"),
        );
        result.bytes(
            "startup",
            "startup/component-host.rs",
            include_bytes!("sdk_startup/component-host.rs"),
        );
        result.bytes(
            "documentation",
            "docs/ja/native-sdk.md",
            include_bytes!("../../docs/ja/native-sdk.md"),
        );
        bound("SDK files", result.assets.len() as u64, 4096)?;
        Ok(result)
    }
}

/// Assemble a fresh host SDK from the same successful Cargo invocation that
/// produced this tool and its compiler/runtime. Keep those trusted build outputs
/// unchanged until return. No Cargo/registry paths are retained in sdk.json.
pub fn assemble(
    cargo_output: &[u8],
    output: &Path,
    limits: manifest::Limits,
) -> Result<manifest::Sdk, Error> {
    if !cfg!(panic = "unwind") {
        return Err(Error::Build(BuildIssue::PanicStrategy));
    }
    let target = Triple::host();
    super::target(&target.to_string()).map_err(Error::Native)?;
    let inventory = Inventory::cargo(cargo_output)?;
    let stage = Stage::new(output).map_err(Error::Native)?;
    let mut files = Vec::new();
    let mut total = 0u64;
    for (destination, asset) in inventory.assets {
        let path = stage.0.join(&destination);
        fs::create_dir_all(path.parent().expect("SDK subdirectory"))
            .map_err(|e| io("SDK subdirectory", &path, e))?;
        let (mut input, size, permissions): (Box<dyn Read>, _, _) = match &asset.source {
            Source::File(source) => {
                let file = fs::File::open(source).map_err(|e| io("SDK input open", source, e))?;
                let metadata = file
                    .metadata()
                    .map_err(|e| io("SDK input metadata", source, e))?;
                if !metadata.is_file() {
                    return Err(Error::Build(BuildIssue::RegularFile));
                }
                (Box::new(file), metadata.len(), Some(metadata.permissions()))
            }
            Source::Bytes(bytes) => (Box::new(*bytes), bytes.len() as u64, None),
        };
        bound("SDK component bytes", size, limits.sdk_file)?;
        total = total
            .checked_add(size)
            .ok_or(Error::Build(BuildIssue::TotalOverflow))?;
        bound("SDK total bytes", total, limits.sdk_total)?;
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| io("SDK component create", &path, e))?;
        let (mut hash, mut copied, mut chunk) = (Sha256::new(), 0u64, [0u8; 65536]);
        input = Box::new(input.take(size.saturating_add(1)));
        loop {
            let n = input
                .read(&mut chunk)
                .map_err(|e| io("SDK component read", &path, e))?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            if copied > size {
                return Err(Error::Build(BuildIssue::InputChangedSize));
            }
            out.write_all(&chunk[..n])
                .map_err(|e| io("SDK component write", &path, e))?;
            hash.update(&chunk[..n]);
        }
        if copied != size {
            return Err(Error::Build(BuildIssue::InputChangedSize));
        }
        out.sync_all()
            .map_err(|e| io("SDK component sync", &path, e))?;
        if asset.role == "compiler" {
            fs::set_permissions(&path, permissions.expect("compiler is a file"))
                .map_err(|e| io("SDK compiler permissions", &path, e))?;
        }
        let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        files.push(json!({"role":asset.role,"path":destination,"bytes":size,"sha256":digest}));
    }
    let bytes = serde_json::to_vec_pretty(&json!({"format":"lang-native-sdk","version":3,"build_id":sdk::build_id(&target),"target":target.to_string(),"rustc":{"release":sdk::RUSTC_RELEASE,"commit":sdk::RUSTC_COMMIT},"build_profile":sdk::BUILD_PROFILE,"feature_policy":sdk::FEATURE_POLICY,"files":files})).map_err(|error| Error::Json { line: 0, error })?;
    bound(
        "SDK total bytes",
        total
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Build(BuildIssue::TotalOverflow))?,
        limits.sdk_total,
    )?;
    let manifest = manifest::parse_sdk(&bytes, limits).map_err(Error::Manifest)?;
    let path = stage.0.join("sdk.json");
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| io("SDK manifest create", &path, e))?;
    out.write_all(&bytes)
        .and_then(|_| out.sync_all())
        .map_err(|e| io("SDK manifest write", &path, e))?;
    drop(out);
    // Reuse the production verifier against the complete written file table.
    // Its private snapshot also verifies the compiled target/toolchain identity.
    let verified =
        manifest::SdkSnapshot::open(&stage.0, stage.0.parent().expect("staging parent"), limits)
            .map_err(Error::Manifest)?;
    drop(verified);
    publish::rename(&stage.0, output).map_err(|e| io("SDK exclusive publication", output, e))?;
    Ok(manifest)
}
