use super::*;
use crate::{
    native::link,
    package::{command, graph::Graph, lock::Lock},
};
use cap_std::fs::Dir;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

struct Options {
    package: command::Request,
    previous_lock: Option<PathBuf>,
    previous_native: Option<PathBuf>,
    sdk: Option<PathBuf>,
}
fn parse(args: &[OsString]) -> Result<Options, Diagnostic> {
    let mut filtered = vec![OsString::from("build")];
    let mut values = args.iter().skip(1);
    let mut seen = BTreeSet::new();
    let (mut previous_lock, mut previous_native, mut sdk) = (None, None, None);
    while let Some(flag) = values.next() {
        let flag = flag
            .to_str()
            .ok_or_else(|| super::super::usage("invalid rebuild option"))?;
        if !seen.insert(flag) {
            return Err(super::super::usage("repeated rebuild option"));
        }
        let value = values
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| super::super::usage("rebuild option requires a value"))?;
        match flag {
            "--previous-lock" => previous_lock = Some(PathBuf::from(value)),
            "--previous-native" => previous_native = Some(PathBuf::from(value)),
            "--native-sdk" => sdk = Some(PathBuf::from(value)),
            "--manifest-path" | "--package" | "--target" | "--message-format" => {
                filtered.push(OsString::from(flag));
                filtered.push(value.clone());
            }
            _ => return Err(super::super::usage("unsupported rebuild option")),
        }
    }
    if sdk.is_some() != previous_native.is_some() || sdk.is_some() && previous_lock.is_some() {
        return Err(super::super::usage(
            "native rebuild comparison requires both --native-sdk and --previous-native, without --previous-lock",
        ));
    }
    let command::Command::Package(package) = command::parse(&filtered)? else {
        return Err(super::super::usage("invalid rebuild request"));
    };
    Ok(Options {
        package,
        previous_lock,
        previous_native,
        sdk,
    })
}
fn directory(path: &Path) -> Result<Dir, Diagnostic> {
    Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(|e| error(e).at_path(path))
}
fn explicit_lock(path: &Path) -> Result<Vec<u8>, Diagnostic> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| error("previous lock requires a file path"))?;
    explain::read_record(
        &directory(parent)?,
        parent,
        Path::new(name),
        crate::package::lock::MAX_BYTES,
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    target: String,
    compiler_build_id: String,
    lock_sha256: String,
    sdk_manifest_sha256: String,
    rustc_release: String,
    rustc_commit: String,
    package_manifest_sha256: String,
    executable_sha256: String,
}
fn prior_native(root: &Path) -> Result<Snapshot, Diagnostic> {
    let dir = directory(root)?;
    let lock = explain::read_record(
        &dir,
        root,
        Path::new("inputs.lock"),
        crate::package::lock::MAX_BYTES,
    )?;
    let bytes = explain::read_record(&dir, root, Path::new("build.json"), 64 * 1024)?;
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .map_err(|_| error("invalid previous native build receipt"))?;
    let sdk_bytes = explain::read_record(&dir, root, Path::new("sdk-input.json"), 1024 * 1024)?;
    let input = explain::inspect(&lock)?;
    if receipt.schema != 1
        || receipt.target != input.target
        || receipt.compiler_build_id != input.compiler_build_id
        || receipt.lock_sha256 != sdk::digest(&lock)
        || receipt.sdk_manifest_sha256 != sdk::digest(&sdk_bytes)
        || !super::super::digest(&receipt.package_manifest_sha256)
        || !super::super::digest(&receipt.executable_sha256)
    {
        return Err(error(
            "previous native build receipt does not match its admitted inputs",
        ));
    }
    Snapshot::capture(
        &lock,
        Some(Native {
            sdk_manifest_sha256: receipt.sdk_manifest_sha256,
            sdk_manifest: String::from_utf8(sdk_bytes)
                .map_err(|_| error("SDK input is not UTF-8"))?,
            rustc_release: receipt.rustc_release,
            rustc_commit: receipt.rustc_commit,
        }),
    )
}
fn report(options: &Options) -> Result<Report, Diagnostic> {
    let graph = Graph::resolve_selected(
        &options.package.manifest,
        &options.package.target,
        Default::default(),
        options.package.package.as_deref(),
    )?;
    let current = Lock::capture(&graph)?;
    if let (Some(sdk_path), Some(previous)) = (&options.sdk, &options.previous_native) {
        if options.package.target != lang_codegen_clif::Triple::host().to_string() {
            return Err(error("native rebuild comparison requires the host target"));
        }
        let before = prior_native(previous)?;
        let inputs = link::inspect_sdk(
            sdk_path,
            std::env::var_os("RUSTC").as_deref(),
            Default::default(),
        )
        .map_err(error)?;
        let native = Native {
            sdk_manifest_sha256: sdk::digest(&inputs.sdk_manifest),
            sdk_manifest: String::from_utf8(inputs.sdk_manifest)
                .map_err(|_| error("SDK input is not UTF-8"))?,
            rustc_release: inputs.rustc_release,
            rustc_commit: inputs.rustc_commit,
        };
        return Report::compare(
            Some(before),
            Snapshot::capture(current.as_bytes(), Some(native))?,
        );
    }
    let previous = if let Some(path) = &options.previous_lock {
        Some(explicit_lock(path)?)
    } else {
        let root = graph.authority_root();
        let dir = directory(root)?;
        match dir.symlink_metadata(crate::package::lock::NAME) {
            Ok(_) => Some(explain::read_record(
                &dir,
                root,
                Path::new(crate::package::lock::NAME),
                crate::package::lock::MAX_BYTES,
            )?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(error(e).at_path(root)),
        }
    };
    let previous = previous
        .map(|bytes| Snapshot::capture(&bytes, None))
        .transpose()?;
    Report::compare(previous, Snapshot::capture(current.as_bytes(), None)?)
}
pub fn run(args: &[OsString], stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<u8> {
    if args.iter().any(|s| s == "--help" || s == "-h") {
        stdout.write_all(super::super::USAGE.as_bytes())?;
        return Ok(0);
    }
    let options = match parse(args) {
        Ok(r) => r,
        Err(d) => {
            command::emit(stderr, command::requested_format(args), &d)?;
            return Ok(2);
        }
    };
    match report(&options).and_then(|r| r.encode()) {
        Ok(bytes) => {
            stdout.write_all(&bytes)?;
            writeln!(stdout)?;
            Ok(0)
        }
        Err(d) => {
            command::emit(stderr, options.package.format, &d)?;
            Ok(1)
        }
    }
}
