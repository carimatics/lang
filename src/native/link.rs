//! RFC 0020 trusted native linking from private verified SDK/program bytes.
//! This module does not use Cargo, a registry, source analysis or guest execution.
use super::{
    Stage,
    manifest::{self as m, Profile, Role},
    publish,
};
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};

#[derive(Debug)]
pub struct Request {
    /// Explicit installed compiler; None searches PATH. CLI maps RUSTC here.
    pub rustc: Option<OsString>,
    pub sdk: PathBuf,
    pub package: PathBuf,
    pub host: Option<PathBuf>,
    pub objects: Vec<PathBuf>,
    pub output: PathBuf,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub manifests: m::Limits,
    pub host_bytes: u64,
    pub object_bytes: u64,
    pub host_total: u64,
    pub objects: usize,
    pub diagnostics: usize,
    pub executable: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            manifests: Default::default(),
            host_bytes: 8 * 1024 * 1024,
            object_bytes: 64 * 1024 * 1024,
            host_total: 256 * 1024 * 1024,
            objects: 64,
            diagnostics: 1024 * 1024,
            executable: 2 * 1024 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum ToolStage {
    Version,
    Compile,
}
#[derive(Debug)]
pub enum Error {
    Native(super::Error),
    Manifest(m::Error),
    Target {
        sdk: String,
        host: String,
    },
    Toolchain {
        field: &'static str,
        expected: String,
        actual: String,
    },
    VersionFormat,
    Library {
        name: &'static str,
        matches: usize,
    },
    HostRequired,
    ObjectsRequireHost,
    DebugProfile,
    Limit {
        resource: &'static str,
        actual: u64,
        maximum: u64,
    },
    Input {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    Argument {
        index: usize,
    },
    ToolIo {
        stage: ToolStage,
        program: OsString,
        error: io::Error,
    },
    ToolFailed {
        stage: ToolStage,
        program: OsString,
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    ToolOutput {
        stage: ToolStage,
        maximum: usize,
    },
    MissingOutput,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HostRequired => f.write_str("native link: C library requires an explicit --host Rust root and any separately compiled C objects"),
            Self::ObjectsRequireHost => f.write_str("native link: --object requires an explicit --host"),
            Self::ToolIo {
                stage,
                program,
                error,
            } => write!(
                f,
                "native link {stage:?}: cannot run {program:?}: {error}; install the exact SDK Rust toolchain and Xcode/MSVC linker"
            ),
            Self::ToolFailed {
                stage,
                program,
                status,
                stdout,
                stderr,
            } => write!(
                f,
                "native link {stage:?}: {program:?} failed ({status})\n{}\n{}",
                String::from_utf8_lossy(stdout),
                String::from_utf8_lossy(stderr)
            ),
            _ => write!(f, "native link: {self:?}"),
        }
    }
}
impl std::error::Error for Error {}
fn io(stage: &'static str, path: &Path, error: io::Error) -> Error {
    Error::Native(super::io(stage, path, error))
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
fn copy(source: &Path, destination: &Path, maximum: u64) -> Result<u64, Error> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(source)
        .map_err(|e| io("host input open", source, e))?;
    let metadata = file
        .metadata()
        .map_err(|e| io("host input metadata", source, e))?;
    if !metadata.is_file() {
        return Err(io(
            "host input regular file",
            source,
            io::ErrorKind::InvalidInput.into(),
        ));
    }
    let size = metadata.len();
    bound("host input bytes", size, maximum)?;
    let mut input = file.take(size.saturating_add(1));
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| io("host snapshot create", destination, e))?;
    let (mut copied, mut bytes) = (0u64, [0u8; 65536]);
    loop {
        let n = input
            .read(&mut bytes)
            .map_err(|e| io("host input read", source, e))?;
        if n == 0 {
            break;
        }
        copied += n as u64;
        if copied > size {
            return Err(Error::Input {
                path: source.into(),
                expected: size,
                actual: copied,
            });
        }
        out.write_all(&bytes[..n])
            .map_err(|e| io("host snapshot write", destination, e))?;
    }
    if copied != size {
        return Err(Error::Input {
            path: source.into(),
            expected: size,
            actual: copied,
        });
    }
    out.sync_all()
        .map_err(|e| io("host snapshot sync", destination, e))?;
    Ok(size)
}
fn drain(mut input: impl Read, maximum: usize) -> io::Result<(Vec<u8>, bool)> {
    let (mut bytes, mut chunk, mut truncated) = (Vec::new(), [0u8; 8192], false);
    loop {
        let n = input.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let keep = n.min(maximum.saturating_sub(bytes.len()));
        bytes
            .try_reserve(keep)
            .map_err(|_| io::Error::other("tool diagnostic capacity"))?;
        bytes.extend_from_slice(&chunk[..keep]);
        truncated |= keep < n;
    }
    Ok((bytes, truncated))
}
fn run(command: &mut Command, stage: ToolStage, maximum: usize) -> Result<Vec<u8>, Error> {
    let program = command.get_program().to_owned();
    let failure = |error| Error::ToolIo {
        stage,
        program: program.clone(),
        error,
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(failure)?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let out = std::thread::spawn(move || drain(stdout, maximum));
    let err = std::thread::spawn(move || drain(stderr, maximum));
    let status = match child.wait() {
        Ok(status) => status,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(failure(error));
        }
    };
    let (stdout, out_cut) = out
        .join()
        .map_err(|_| failure(io::Error::other("stdout reader panicked")))?
        .map_err(failure)?;
    let (stderr, err_cut) = err
        .join()
        .map_err(|_| failure(io::Error::other("stderr reader panicked")))?
        .map_err(failure)?;
    if out_cut || err_cut {
        return Err(Error::ToolOutput { stage, maximum });
    }
    if !status.success() {
        return Err(Error::ToolFailed {
            stage,
            program,
            status,
            stdout,
            stderr,
        });
    }
    Ok(stdout)
}
fn version(program: &OsStr, cwd: &Path, sdk: &m::Sdk, maximum: usize) -> Result<(), Error> {
    let output = run(
        Command::new(program)
            .args(["--version", "--verbose"])
            .current_dir(cwd),
        ToolStage::Version,
        maximum.min(65536),
    )?;
    let text = std::str::from_utf8(&output).map_err(|_| Error::VersionFormat)?;
    for (field, prefix, expected) in [
        ("release", "release: ", sdk.rustc_release()),
        ("commit", "commit-hash: ", sdk.rustc_commit()),
        ("host", "host: ", sdk.target().text()),
    ] {
        let values: Vec<_> = text
            .lines()
            .filter_map(|l| l.strip_prefix(prefix))
            .collect();
        if values.len() != 1 {
            return Err(Error::VersionFormat);
        }
        if values[0] != expected {
            return Err(Error::Toolchain {
                field,
                expected: expected.into(),
                actual: values[0].chars().take(128).collect(),
            });
        }
    }
    Ok(())
}
fn library<'a>(sdk: &'a m::Sdk, name: &'static str) -> Result<&'a m::File, Error> {
    let files: Vec<_> = sdk
        .files()
        .iter()
        .filter(|f| {
            if name == "lang_codegen_clif" {
                return f.role() == Role::Runtime;
            }
            let leaf = Path::new(f.path())
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("");
            f.role() == Role::Dependency
                && (leaf.starts_with(&format!("lib{name}-")) || leaf == format!("lib{name}.rlib"))
                && leaf.ends_with(".rlib")
        })
        .collect();
    if files.len() != 1 {
        Err(Error::Library {
            name,
            matches: files.len(),
        })
    } else {
        Ok(files[0])
    }
}
fn compiler(selected: Option<&OsStr>) -> Result<OsString, Error> {
    let value = selected
        .map(OsStr::to_owned)
        .unwrap_or_else(|| OsString::from("rustc"));
    let path = Path::new(&value);
    let candidates = if path.is_absolute() || path.components().count() > 1 {
        vec![path.to_owned()]
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .flat_map(|dir| {
                let path = dir.join(&value);
                #[cfg(windows)]
                let paths = if path.extension().is_none() {
                    vec![path.clone(), path.with_extension("exe")]
                } else {
                    vec![path]
                };
                #[cfg(not(windows))]
                let paths = vec![path];
                paths
            })
            .collect()
    };
    for path in candidates {
        if path.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if fs::metadata(&path)
                    .map(|m| m.permissions().mode() & 0o111 == 0)
                    .unwrap_or(true)
                {
                    continue;
                }
            }
            // Keep the rustc shim's basename: canonicalizing it to rustup changes argv[0].
            return std::path::absolute(&path)
                .map(PathBuf::into_os_string)
                .map_err(|error| Error::ToolIo {
                    stage: ToolStage::Version,
                    program: value.clone(),
                    error,
                });
        }
    }
    Err(Error::ToolIo {
        stage: ToolStage::Version,
        program: value,
        error: io::ErrorKind::NotFound.into(),
    })
}

/// Link trusted native inputs only after bounded independent SDK/program checks.
/// The output must be fresh. Rust hosts are standalone roots beside bindings.rs;
/// their code and explicit native objects are trusted build inputs, not a sandbox.
pub fn link(request: &Request, limits: Limits) -> Result<(), Error> {
    link_recorded(request, limits).map(|_| ())
}

pub(crate) struct Inputs {
    pub sdk_manifest: Vec<u8>,
    pub rustc_release: String,
    pub rustc_commit: String,
}

/// Observe the same admitted SDK and selected rustc identity as a native build,
/// without compiling a startup, linking a program or publishing an output.
pub(crate) fn inspect_sdk(
    source: &Path,
    selected: Option<&OsStr>,
    limits: Limits,
) -> Result<Inputs, Error> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let output = std::env::temp_dir().join(format!(
        "lang-rebuild-input-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    let mut stage = Stage::new(&output).map_err(Error::Native)?;
    stage.0 = fs::canonicalize(&stage.0).map_err(|e| io("staging root", &stage.0, e))?;
    let sdk =
        m::SdkSnapshot::open_named(source, &stage.0, limits.manifests).map_err(Error::Manifest)?;
    let host = lang_codegen_clif::Triple::host().to_string();
    if sdk.manifest().target().text() != host {
        return Err(Error::Target {
            sdk: sdk.manifest().target().text().into(),
            host,
        });
    }
    version(
        &compiler(selected)?,
        sdk.root(),
        sdk.manifest(),
        limits.diagnostics,
    )?;
    Ok(Inputs {
        sdk_manifest: sdk.manifest_bytes().to_vec(),
        rustc_release: sdk.manifest().rustc_release().into(),
        rustc_commit: sdk.manifest().rustc_commit().into(),
    })
}

pub(crate) fn link_recorded(request: &Request, limits: Limits) -> Result<Inputs, Error> {
    match link_mode(request, limits, LinkMode::Publish)? {
        Linked::Published(inputs) => Ok(inputs),
        Linked::Debug(_) => unreachable!("selected publication mode"),
    }
}

/// Keeps verified code and its debug sidecars alive for one trusted debugger.
pub(crate) struct DebugImage {
    package: m::ProgramSnapshot,
    _sdk: m::SdkSnapshot,
    executable: PathBuf,
    _stage: Stage,
}
impl DebugImage {
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }
    pub(crate) fn semantic(&self) -> &[u8] {
        self.package.semantic()
    }
}
enum LinkMode {
    Publish,
    Debug,
}
enum Linked {
    Published(Inputs),
    Debug(Box<DebugImage>),
}

pub(crate) fn link_debug(request: &Request, limits: Limits) -> Result<DebugImage, Error> {
    match link_mode(request, limits, LinkMode::Debug)? {
        Linked::Debug(image) => Ok(*image),
        Linked::Published(_) => unreachable!("selected debug mode"),
    }
}
fn link_mode(request: &Request, limits: Limits, mode: LinkMode) -> Result<Linked, Error> {
    if request.host.is_none() && !request.objects.is_empty() {
        return Err(Error::ObjectsRequireHost);
    }
    bound(
        "host object count",
        request.objects.len() as u64,
        limits.objects as u64,
    )?;
    let mut stage = Stage::new(&request.output).map_err(Error::Native)?;
    stage.0 = fs::canonicalize(&stage.0).map_err(|e| io("staging root", &stage.0, e))?;
    let sdk = m::SdkSnapshot::open_named(&request.sdk, &stage.0, limits.manifests)
        .map_err(Error::Manifest)?;
    let inputs = Inputs {
        sdk_manifest: sdk.manifest_bytes().to_vec(),
        rustc_release: sdk.manifest().rustc_release().into(),
        rustc_commit: sdk.manifest().rustc_commit().into(),
    };
    let host = lang_codegen_clif::Triple::host().to_string();
    if sdk.manifest().target().text() != host {
        return Err(Error::Target {
            sdk: sdk.manifest().target().text().into(),
            host,
        });
    }
    let package =
        m::ProgramSnapshot::open_named(&request.package, &stage.0, &sdk, limits.manifests)
            .map_err(Error::Manifest)?;
    if matches!(mode, LinkMode::Debug)
        && (package.manifest().profile() != Profile::Copy
            || request.host.is_some()
            || !request.objects.is_empty())
    {
        return Err(Error::DebugProfile);
    }
    if matches!(
        package.manifest().profile(),
        Profile::C | Profile::Component
    ) && request.host.is_none()
    {
        return Err(Error::HostRequired);
    }
    let mut arguments: Vec<OsString> = vec![
        "--edition=2024".into(),
        "--crate-name=lang_native_startup".into(),
        "-Cpanic=unwind".into(),
    ];
    if matches!(mode, LinkMode::Debug) {
        arguments.extend(["-Cdebuginfo=2".into(), "-Csplit-debuginfo=packed".into()]);
    }
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    arguments.extend(["-C".into(), "link-arg=/Brepro".into()]);
    #[cfg(target_os = "macos")]
    if matches!(mode, LinkMode::Publish) {
        // Keep OSO debug maps while excluding this operation's private prefix.
        // Separate -Xlinker arguments preserve commas and spaces in the path.
        arguments.extend([
            "-C".into(),
            "link-arg=-Xlinker".into(),
            "-C".into(),
            "link-arg=-oso_prefix".into(),
            "-C".into(),
            "link-arg=-Xlinker".into(),
        ]);
        let mut prefix = OsString::from("link-arg=");
        prefix.push(&stage.0);
        prefix.push(std::path::MAIN_SEPARATOR_STR);
        arguments.extend(["-C".into(), prefix]);
    }
    let source = if let Some(host) = &request.host {
        let path = package.root().join("host.rs");
        copy(host, &path, limits.host_bytes.min(limits.host_total))?;
        path
    } else {
        package.root().join("main.rs")
    };
    // The verified source is in the compiler's snapshot working directory.
    // An absolute staging path would leak its random identity into file!()
    // and produce different executables from identical admitted inputs.
    arguments.push(source.file_name().expect("fixed snapshot source").into());
    let executable = stage.0.join(if cfg!(windows) {
        "application.exe"
    } else {
        "application"
    });
    arguments.extend([OsString::from("-o"), executable.clone().into_os_string()]);
    let mut directories = BTreeSet::new();
    for f in sdk
        .manifest()
        .files()
        .iter()
        .filter(|f| matches!(f.role(), Role::Runtime | Role::Dependency))
    {
        directories.insert(
            Path::new("../sdk")
                .join(f.path())
                .parent()
                .expect("SDK component parent")
                .to_owned(),
        );
    }
    for dir in directories {
        for kind in ["dependency=", "native="] {
            let mut arg = OsString::from(kind);
            arg.push(&dir);
            arguments.extend([OsString::from("-L"), arg]);
        }
    }
    for name in [
        "lang_codegen_clif",
        "lang_runtime",
        "lang_mir",
        "lang_bytecode",
    ] {
        let file = library(sdk.manifest(), name)?;
        let mut arg = OsString::from(format!("{name}="));
        arg.push(Path::new("../sdk").join(file.path()));
        arguments.extend([OsString::from("--extern"), arg]);
    }
    if package.manifest().profile() == Profile::Component {
        for name in ["lang", "lang_diagnostics", "lang_vm", "serde_json"] {
            let file = library(sdk.manifest(), name)?;
            let mut arg = OsString::from(format!("{name}="));
            arg.push(Path::new("../sdk").join(file.path()));
            arguments.extend([OsString::from("--extern"), arg]);
        }
    }
    let object = package
        .manifest()
        .file(Role::Object)
        .expect("verified program object");
    let mut arg = OsString::from("link-arg=");
    arg.push(object.path());
    arguments.extend([OsString::from("-C"), arg]);
    let mut total = if request.host.is_some() {
        fs::metadata(package.root().join("host.rs"))
            .map_err(|e| io("host snapshot size", &request.package, e))?
            .len()
    } else {
        0
    };
    for (index, object) in request.objects.iter().enumerate() {
        let path = stage.0.join(format!(
            "host-{index}.{}",
            if cfg!(windows) { "obj" } else { "o" }
        ));
        let maximum = limits
            .object_bytes
            .min(limits.host_total.saturating_sub(total));
        total = total
            .checked_add(copy(object, &path, maximum)?)
            .ok_or(Error::Limit {
                resource: "host input total",
                actual: u64::MAX,
                maximum: limits.host_total,
            })?;
        let mut arg = OsString::from("link-arg=");
        arg.push(Path::new("..").join(path.file_name().expect("fixed host object")));
        arguments.extend([OsString::from("-C"), arg]);
    }
    // rustc response files use one UTF-8 argument per line, without shell quoting.
    let response = stage.0.join("rustc.args");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&response)
        .map_err(|e| io("rustc response create", &response, e))?;
    for (index, arg) in arguments.iter().enumerate() {
        let arg = arg
            .to_str()
            .filter(|s| !s.contains(['\r', '\n']))
            .ok_or(Error::Argument { index })?;
        writeln!(file, "{arg}").map_err(|e| io("rustc response write", &response, e))?;
    }
    drop(file);
    let selected = if matches!(mode, LinkMode::Debug) && request.rustc.is_none() {
        let bytes = run(
            Command::new("rustup").args([
                "which",
                "--toolchain",
                sdk.manifest().rustc_release(),
                "rustc",
            ]),
            ToolStage::Version,
            limits.diagnostics,
        )?;
        let path = std::str::from_utf8(&bytes)
            .map_err(|_| Error::VersionFormat)?
            .trim();
        if path.is_empty() || path.contains(['\r', '\n']) {
            return Err(Error::VersionFormat);
        }
        Some(OsString::from(path))
    } else {
        request.rustc.clone()
    };
    let compiler = compiler(selected.as_deref())?;
    version(
        &compiler,
        package.root(),
        sdk.manifest(),
        limits.diagnostics,
    )?;
    let mut at = OsString::from("@");
    at.push(&response);
    run(
        Command::new(&compiler).arg(at).current_dir(package.root()),
        ToolStage::Compile,
        limits.diagnostics,
    )?;
    let metadata = fs::metadata(&executable).map_err(|_| Error::MissingOutput)?;
    if !metadata.is_file() {
        return Err(Error::MissingOutput);
    }
    bound("executable bytes", metadata.len(), limits.executable)?;
    if matches!(mode, LinkMode::Debug) {
        return Ok(Linked::Debug(Box::new(DebugImage {
            package,
            _sdk: sdk,
            executable,
            _stage: stage,
        })));
    }
    publish::rename(&executable, &request.output)
        .map_err(|e| io("exclusive executable publication", &request.output, e))?;
    Ok(Linked::Published(inputs))
}
