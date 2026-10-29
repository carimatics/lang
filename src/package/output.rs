//! Capability-relative package outputs. The root lock is the commit point.
use super::{
    Diagnostic,
    command::Request,
    graph::{self, Graph, Limits},
    lock::{Lock, NAME},
    source,
};
use cap_std::fs::{Dir, DirBuilder, OpenOptions};
use std::{
    collections::BTreeSet,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) const MAX_OUTPUT: usize = 64 * 1024 * 1024;
pub(super) struct File<'a> {
    pub path: &'a str,
    pub bytes: &'a [u8],
    pub executable: bool,
}
pub(super) enum Generation<'a> {
    Build,
    Test,
    Native(&'a str),
}
impl Generation<'_> {
    fn name(&self) -> &str {
        match self {
            Self::Build => "build",
            Self::Test => "test",
            Self::Native(name) => name,
        }
    }
}
pub(super) struct Output {
    directory: Dir,
    inputs: BTreeSet<source::Identity>,
}
pub(super) struct Prepared {
    pub graph: Graph,
    pub lock: Lock,
    pub output: Output,
}
fn fs_error(path: &Path, e: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("PKG0005", e.to_string()).at_path(path)
}
fn invalid(path: &Path, message: &str) -> Diagnostic {
    Diagnostic::error("PKG0003", message).at_path(path)
}
fn mismatch(path: &Path, message: &str) -> Diagnostic {
    Diagnostic::error("PKG0007", message).at_path(path)
}
fn equivalent(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_str()
            .zip(b.to_str())
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
    } else {
        a == b
    }
}
fn reserved(path: &Path) -> bool {
    let Some(Component::Normal(first)) = path.components().next() else {
        return false;
    };
    equivalent(Path::new(first), Path::new("target")) || equivalent(path, Path::new(NAME))
}
fn input_id(metadata: &cap_std::fs::Metadata, path: &Path) -> Result<source::Identity, Diagnostic> {
    source::identity(metadata, path).map_err(|e| fs_error(path, e))
}
impl Prepared {
    pub fn open(request: &Request) -> Result<Self, Diagnostic> {
        let absolute = if request.manifest.is_absolute() {
            request.manifest.clone()
        } else {
            std::env::current_dir()
                .map_err(|e| fs_error(&request.manifest, e))?
                .join(&request.manifest)
        };
        let name = absolute
            .file_name()
            .ok_or_else(|| invalid(&absolute, "missing manifest filename"))?;
        let parent = absolute
            .parent()
            .ok_or_else(|| invalid(&absolute, "missing manifest directory"))?;
        let display = std::fs::canonicalize(parent.join(".")).map_err(|e| fs_error(parent, e))?;
        let directory = Dir::open_ambient_dir(&display, cap_std::ambient_authority())
            .map_err(|e| fs_error(&display, e))?;
        // This clone, not a later reopen of the display path, ties source and
        // output to exactly the same directory under concurrent path changes.
        let graph = Graph::resolve_selected_in(
            directory.try_clone().map_err(|e| fs_error(&display, e))?,
            display,
            Path::new(name),
            &request.target,
            Limits::default(),
            request.package.as_deref(),
        )?;
        let lock = Lock::capture(&graph)?;
        let inventory = graph.inputs()?;
        let mut inputs: BTreeSet<_> = inventory.files.values().map(|f| f.identity).collect();
        for (path, metadata) in graph.manifest_inputs() {
            if reserved(path) {
                return Err(invalid(path, "manifest occupies a reserved package output"));
            }
            inputs.insert(input_id(metadata, path)?);
        }
        for path in inventory.files.keys() {
            if reserved(path) {
                return Err(invalid(path, "source occupies a reserved package output"));
            }
        }
        let output = Output { directory, inputs };
        output.admit_slot(Path::new(NAME))?;
        if request.locked {
            lock.check(&output.directory)?;
        }
        Ok(Self {
            graph,
            lock,
            output,
        })
    }
}
impl Output {
    fn admit_slot(&self, path: &Path) -> Result<(), Diagnostic> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = match self.directory.open_with(path, &options) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(fs_error(path, e)),
        };
        let metadata = file.metadata().map_err(|e| fs_error(path, e))?;
        if !metadata.is_file() {
            return Err(invalid(
                path,
                "reserved output must be a regular file when present",
            ));
        }
        if self.inputs.contains(&input_id(&metadata, path)?) {
            return Err(invalid(path, "reserved output aliases an admitted input"));
        }
        Ok(())
    }
    pub fn publish(
        &self,
        lock: &Lock,
        generation: Generation<'_>,
        files: &[File<'_>],
        locked: bool,
    ) -> Result<(), Diagnostic> {
        let name = generation.name();
        let mut names = BTreeSet::new();
        let mut bytes = lock.as_bytes().len();
        if files.len() > super::manifest::MAX_ENTRIES {
            return Err(Diagnostic::error(
                "PKG0004",
                "package output file limit exceeded",
            ));
        }
        for file in files {
            let path = Path::new(file.path);
            if path.components().next().is_none()
                || !path.components().all(|c| matches!(c, Component::Normal(_)))
                || file.path.contains(['\\', ':', '\0'])
                || !names.insert(file.path)
                || file.path == "inputs.lock"
            {
                return Err(invalid(path, "invalid or duplicate generated output path"));
            }
            bytes = bytes
                .checked_add(file.bytes.len())
                .filter(|n| *n <= MAX_OUTPUT)
                .ok_or_else(|| {
                    Diagnostic::error("PKG0004", "package output byte limit exceeded")
                })?;
        }
        if bytes > MAX_OUTPUT {
            return Err(Diagnostic::error(
                "PKG0004",
                "package output byte limit exceeded",
            ));
        }
        let key = lang_codegen_clif::sdk::digest(lock.as_bytes());
        let mut parent = self
            .directory
            .try_clone()
            .map_err(|e| fs_error(Path::new("target"), e))?;
        for name in ["target", "lang", &key] {
            match parent.create_dir(name) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(fs_error(Path::new(name), e)),
            }
            let canonical = parent
                .canonicalize(Path::new(name).join("."))
                .map_err(|e| fs_error(Path::new(name), e))?;
            if !equivalent(&canonical, Path::new(name)) {
                return Err(invalid(
                    Path::new(name),
                    "package output directories must not be aliases",
                ));
            }
            parent = parent
                .open_dir(name)
                .map_err(|e| fs_error(Path::new(name), e))?;
        }
        let expected: Vec<_> = std::iter::once(File {
            path: "inputs.lock",
            bytes: lock.as_bytes(),
            executable: false,
        })
        .chain(files.iter().map(|f| File {
            path: f.path,
            bytes: f.bytes,
            executable: f.executable,
        }))
        .collect();
        if !matches_existing(&parent, name, &expected, &self.inputs)? {
            let mut stage = Stage::new(&parent)?;
            for file in &expected {
                stage.write_mode(file.path, file.bytes, file.executable)?;
            }
            stage.close();
            match parent.rename(&stage.name, &parent, name) {
                Ok(()) => {}
                Err(e) => {
                    if !matches_existing(&parent, name, &expected, &self.inputs)? {
                        return Err(fs_error(Path::new(name), e));
                    }
                }
            }
            // Every committed generation contains inputs.lock and is nonempty.
            // A capability-relative rename cannot replace a concurrent complete
            // generation; in that case compare/reuse its exact verified bytes.
        }
        if !locked {
            self.admit_slot(Path::new(NAME))?;
            lock.publish(&self.directory)?;
        }
        Ok(())
    }
}
fn matches_existing(
    parent: &Dir,
    name: &str,
    files: &[File<'_>],
    inputs: &BTreeSet<source::Identity>,
) -> Result<bool, Diagnostic> {
    match parent.symlink_metadata(name) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(fs_error(Path::new(name), e)),
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(invalid(
                Path::new(name),
                "cached generation must be a real directory",
            ));
        }
        Ok(_) => {}
    }
    let canonical = parent
        .canonicalize(Path::new(name).join("."))
        .map_err(|e| fs_error(Path::new(name), e))?;
    if !equivalent(&canonical, Path::new(name)) {
        return Err(invalid(
            Path::new(name),
            "cached generation must not be an alias",
        ));
    }
    let dir = parent
        .open_dir(name)
        .map_err(|e| fs_error(Path::new(name), e))?;
    let mut expected = BTreeSet::new();
    let mut directories = BTreeSet::new();
    directories.insert(PathBuf::new());
    for file in files {
        let path = Path::new(file.path);
        expected.insert(path.to_path_buf());
        for parent in path.ancestors().skip(1) {
            directories.insert(parent.to_path_buf());
        }
        let input = graph::read(&dir, Path::new(name), path, file.bytes.len())?;
        #[cfg(unix)]
        {
            use cap_std::fs::PermissionsExt;
            if file.executable && input.metadata.permissions().mode() & 0o111 == 0 {
                return Err(mismatch(path, "cached native output is not executable"));
            }
        }
        if input.bytes != file.bytes {
            return Err(mismatch(
                path,
                "cached output differs from verified package inputs",
            ));
        }
        if inputs.contains(&input_id(&input.metadata, path)?) {
            return Err(invalid(path, "cached output aliases an admitted input"));
        }
    }
    let mut pending = vec![PathBuf::new()];
    let mut visited = 0;
    while let Some(path) = pending.pop() {
        let current = dir
            .open_dir(path.join("."))
            .map_err(|e| fs_error(&path, e))?;
        for entry in current.entries().map_err(|e| fs_error(&path, e))? {
            visited += 1;
            if visited > 2 * (super::manifest::MAX_ENTRIES + 1) {
                return Err(Diagnostic::error(
                    "PKG0004",
                    "cached output entry limit exceeded",
                ));
            }
            let entry = entry.map_err(|e| fs_error(&path, e))?;
            let path = path.join(entry.file_name());
            let ty = entry.file_type().map_err(|e| fs_error(&path, e))?;
            if ty.is_symlink() {
                return Err(invalid(&path, "cached output must not contain links"));
            }
            if ty.is_dir() && directories.contains(&path) {
                pending.push(path);
            } else if !ty.is_file() || !expected.contains(&path) {
                return Err(mismatch(
                    &path,
                    "cached generation contains an unexpected output",
                ));
            }
        }
    }
    Ok(true)
}
pub(super) struct Stage {
    parent: Dir,
    name: String,
    directory: Option<Dir>,
}
impl Stage {
    pub fn new(parent: &Dir) -> Result<Self, Diagnostic> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let retained = parent
            .try_clone()
            .map_err(|e| fs_error(Path::new("output stage"), e))?;
        for _ in 0..128 {
            let name = format!(
                ".lang-stage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let builder = DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use cap_std::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            match parent.create_dir_with(&name, &builder) {
                Ok(()) => {
                    let mut stage = Self {
                        parent: retained,
                        name,
                        directory: None,
                    };
                    stage.directory = Some(
                        parent
                            .open_dir(&stage.name)
                            .map_err(|e| fs_error(Path::new(&stage.name), e))?,
                    );
                    return Ok(stage);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(fs_error(Path::new(&name), e)),
            }
        }
        Err(Diagnostic::error(
            "PKG0004",
            "private output stage capacity exceeded",
        ))
    }
    pub fn directory(&self) -> &Dir {
        self.directory.as_ref().expect("live stage")
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), Diagnostic> {
        self.write_mode(name, bytes, false)
    }
    fn write_mode(&self, name: &str, bytes: &[u8], executable: bool) -> Result<(), Diagnostic> {
        #[cfg(not(unix))]
        let _ = executable;
        let path = Path::new(name);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            self.directory()
                .create_dir_all(parent)
                .map_err(|e| fs_error(parent, e))?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(if executable { 0o700 } else { 0o600 });
        }
        let mut file = self
            .directory()
            .open_with(path, &options)
            .map_err(|e| fs_error(path, e))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| fs_error(path, e))
    }
    pub fn close(&mut self) {
        drop(self.directory.take());
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        self.close();
        let _ = self.parent.remove_dir_all(&self.name);
    }
}
