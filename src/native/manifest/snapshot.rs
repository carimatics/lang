use super::*;
use cap_std::fs::{Dir, OpenOptions};
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

fn io(path: impl Into<PathBuf>, e: std::io::Error) -> Error {
    error(path, ErrorKind::Io(e))
}
fn open(dir: &Dir, path: &str) -> Result<cap_std::fs::File, Error> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = dir.open_with(path, &options).map_err(|e| io(path, e))?;
    if !file.metadata().map_err(|e| io(path, e))?.is_file() {
        return Err(error(
            path,
            ErrorKind::Invalid {
                field: "regular file",
            },
        ));
    }
    Ok(file)
}
fn read(dir: &Dir, name: &str, maximum: usize) -> Result<Vec<u8>, Error> {
    let file = open(dir, name)?;
    bound(
        "file bytes",
        file.metadata().map_err(|e| io(name, e))?.len(),
        maximum as u64,
    )?;
    let mut file = file.take((maximum as u64).saturating_add(1));
    let (mut bytes, mut chunk) = (Vec::new(), [0u8; 16384]);
    loop {
        let n = file.read(&mut chunk).map_err(|e| io(name, e))?;
        if n == 0 {
            break;
        }
        let length = bytes
            .len()
            .checked_add(n)
            .ok_or_else(|| error(name, ErrorKind::Capacity))?;
        bound("file bytes", length as u64, maximum as u64).map_err(|e| at(e, name))?;
        bytes
            .try_reserve(n)
            .map_err(|_| error(name, ErrorKind::Capacity))?;
        bytes.extend_from_slice(&chunk[..n]);
    }
    Ok(bytes)
}
struct Snapshot {
    path: PathBuf,
    dir: Option<Dir>,
}
impl Snapshot {
    fn new(parent: &Path) -> Result<Self, Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let path = parent.join(format!(
                ".lang-snapshot-{}-{}",
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
                Ok(()) => {
                    let mut result = Self { path, dir: None };
                    result.dir = Some(
                        Dir::open_ambient_dir(&result.path, cap_std::ambient_authority())
                            .map_err(|e| io(&result.path, e))?,
                    );
                    return Ok(result);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(io(&path, e)),
            }
        }
        Err(error(parent, ErrorKind::Capacity))
    }
    fn dir(&self) -> &Dir {
        self.dir.as_ref().expect("live private snapshot")
    }
    fn create(&self, name: &str) -> Result<cap_std::fs::File, Error> {
        if let Some(parent) = Path::new(name)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            self.dir()
                .create_dir_all(parent)
                .map_err(|e| io(self.path.join(parent), e))?;
        }
        self.dir()
            .open_with(name, OpenOptions::new().write(true).create_new(true))
            .map_err(|e| io(self.path.join(name), e))
    }
    fn write(&self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        let mut file = self.create(name)?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| io(self.path.join(name), e))
    }
    fn copy(&self, source: &Dir, files: &[File]) -> Result<(), Error> {
        let mut chunk = [0u8; 65536];
        for f in files {
            let input = open(source, f.path())?;
            let length = input.metadata().map_err(|e| io(f.path(), e))?.len();
            if length != f.bytes() {
                return Err(at(
                    mismatch("component size", f.bytes().to_string(), length.to_string()),
                    f.path(),
                ));
            }
            let mut input = input.take(f.bytes().saturating_add(1));
            let mut output = self.create(f.path())?;
            let (mut count, mut hash) = (0u64, Sha256::new());
            loop {
                let n = input.read(&mut chunk).map_err(|e| io(f.path(), e))?;
                if n == 0 {
                    break;
                }
                count = count
                    .checked_add(n as u64)
                    .ok_or_else(|| error(f.path(), ErrorKind::Capacity))?;
                bound("declared component bytes", count, f.bytes()).map_err(|e| at(e, f.path()))?;
                hash.update(&chunk[..n]);
                output
                    .write_all(&chunk[..n])
                    .map_err(|e| io(self.path.join(f.path()), e))?;
            }
            if count != f.bytes() {
                return Err(at(
                    mismatch("component size", f.bytes().to_string(), count.to_string()),
                    f.path(),
                ));
            }
            let actual = Hex(hash.finalize().into());
            if &actual != f.hash() {
                return Err(at(
                    mismatch(
                        "component SHA-256",
                        f.hash().to_string(),
                        actual.to_string(),
                    ),
                    f.path(),
                ));
            }
            output
                .sync_all()
                .map_err(|e| io(self.path.join(f.path()), e))?;
        }
        Ok(())
    }
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        drop(self.dir.take());
        let _ = fs::remove_dir_all(&self.path);
    }
}
fn root(path: &Path) -> Result<Dir, Error> {
    Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(|e| io(path, e))
}
fn context<T>(root: &Path, result: Result<T, Error>) -> Result<T, Error> {
    result.map_err(|mut e| {
        e.path = root.join(&e.path).into_boxed_path();
        e
    })
}

/// Owns verified private copies, not authority to execute native inputs.
/// The caller must retain this owner and keep its private files unchanged
/// while trusted build tools consume them. Dropping it removes its directory.
pub struct SdkSnapshot {
    snapshot: Snapshot,
    manifest: Sdk,
}
impl SdkSnapshot {
    pub fn open(source: &Path, staging_parent: &Path, limits: Limits) -> Result<Self, Error> {
        context(source, Self::load(source, staging_parent, limits))
    }
    fn load(source: &Path, parent: &Path, limits: Limits) -> Result<Self, Error> {
        let source = root(source)?;
        let bytes = read(&source, "sdk.json", limits.sdk_manifest)?;
        let manifest = parse_sdk(&bytes, limits).map_err(|e| at(e, "sdk.json"))?;
        use lang_codegen_clif::sdk;
        for (field, expected, actual) in [
            (
                "build ID",
                sdk::build_id(&manifest.target().triple()),
                manifest.build_id().to_string(),
            ),
            (
                "rustc release",
                sdk::RUSTC_RELEASE.to_owned(),
                manifest.rustc_release().to_owned(),
            ),
            (
                "rustc commit",
                sdk::RUSTC_COMMIT.to_owned(),
                manifest.rustc_commit().to_owned(),
            ),
            (
                "build profile",
                sdk::BUILD_PROFILE.to_owned(),
                manifest.build_profile().to_owned(),
            ),
            (
                "feature policy",
                sdk::FEATURE_POLICY.to_owned(),
                manifest.feature_policy().to_owned(),
            ),
        ] {
            if expected != actual {
                return Err(at(mismatch(field, expected, actual), "sdk.json"));
            }
        }
        let snapshot = Snapshot::new(parent)?;
        snapshot.copy(&source, manifest.files())?;
        snapshot.write("sdk.json", &bytes)?;
        Ok(Self { snapshot, manifest })
    }
    pub fn root(&self) -> &Path {
        &self.snapshot.path
    }
    pub fn manifest(&self) -> &Sdk {
        &self.manifest
    }
}
/// Checked complete program data plus original private native build inputs.
/// Native code/host glue remain trusted inputs, as specified by RFC 0020.
pub struct ProgramSnapshot {
    snapshot: Snapshot,
    manifest: Program,
    semantic: Vec<u8>,
    native: Vec<u8>,
}
impl ProgramSnapshot {
    pub fn open(
        source: &Path,
        staging_parent: &Path,
        sdk: &SdkSnapshot,
        limits: Limits,
    ) -> Result<Self, Error> {
        context(source, Self::load(source, staging_parent, sdk, limits))
    }
    fn load(
        source: &Path,
        parent: &Path,
        sdk: &SdkSnapshot,
        limits: Limits,
    ) -> Result<Self, Error> {
        let source = root(source)?;
        let bytes = read(&source, "package.json", limits.program_manifest)?;
        let manifest = parse_program(&bytes, limits).map_err(|e| at(e, "package.json"))?;
        if manifest.target() != sdk.manifest.target() {
            return Err(mismatch(
                "target",
                sdk.manifest.target().text(),
                manifest.target().text(),
            ));
        }
        if manifest.build_id() != sdk.manifest.build_id() {
            return Err(mismatch(
                "build ID",
                sdk.manifest.build_id().to_string(),
                manifest.build_id().to_string(),
            ));
        }
        let snapshot = Snapshot::new(parent)?;
        snapshot.copy(&source, manifest.files())?;
        snapshot.write("package.json", &bytes)?;
        let semantic = read(snapshot.dir(), "program.lbc", 64 * 1024 * 1024)?;
        let native = read(snapshot.dir(), "program.native", 64 * 1024 * 1024)?;
        validate(&snapshot, &manifest, &semantic, &native)?;
        Ok(Self {
            snapshot,
            manifest,
            semantic,
            native,
        })
    }
    pub fn root(&self) -> &Path {
        &self.snapshot.path
    }
    pub fn manifest(&self) -> &Program {
        &self.manifest
    }
    pub fn semantic(&self) -> &[u8] {
        &self.semantic
    }
    pub fn native_record(&self) -> &[u8] {
        &self.native
    }
}
fn validate(
    snapshot: &Snapshot,
    manifest: &Program,
    semantic: &[u8],
    native: &[u8],
) -> Result<(), Error> {
    use lang_codegen_clif::{
        native_record::{Kind, Record},
        owned,
    };
    use lang_mir::unit as m;
    let record = Record::decode(native, semantic, Default::default())
        .map_err(|e| error("program.native", ErrorKind::Record(e)))?;
    if record.target() != &manifest.target().triple() {
        return Err(mismatch(
            "record target",
            manifest.target().text(),
            record.target().to_string(),
        ));
    }
    let mut file = open(snapshot.dir(), manifest.file(Role::Object).unwrap().path())?;
    let mut remaining = record.object();
    let mut chunk = [0u8; 65536];
    loop {
        let n = file.read(&mut chunk).map_err(|e| io("object", e))?;
        if n == 0 {
            break;
        }
        if !remaining.starts_with(&chunk[..n]) {
            return Err(invalid("original object/record bytes"));
        }
        remaining = &remaining[n..];
    }
    if !remaining.is_empty() {
        return Err(invalid("original object length"));
    }
    if manifest.profile() == Profile::Copy {
        if record.kind() != Kind::Copy {
            return Err(invalid("Copy record kind"));
        }
        let p = lang_bytecode::unit::decode(semantic, Default::default())
            .map_err(|e| error("program.lbc", ErrorKind::CopyCodec(e)))?;
        let unit = p.verified().unit();
        if unit.entry.map(|e| e.0) != manifest.entry() {
            return Err(invalid("semantic entry"));
        }
        let entry = &unit
            .functions
            .get(manifest.entry().unwrap() as usize)
            .ok_or_else(|| invalid("entry index"))?
            .signature;
        if !entry.parameters.is_empty() || entry.result != m::Type::I64 {
            return Err(invalid("ordinary i64 entry"));
        }
        record
            .prepare_copy(p.verified(), &[], Default::default())
            .map_err(|e| error("program.native", ErrorKind::CopyPlan(e)))?;
    } else {
        let p = lang_bytecode::ownership::decode(semantic, Default::default())
            .map_err(|e| error("program.lbc", ErrorKind::OwnedCodec(e)))?;
        if p.entry().map(|e| e.0) != manifest.entry() {
            return Err(invalid("semantic entry"));
        }
        let flow = p.flow();
        let decl = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations();
        let c = decl
            .imports()
            .iter()
            .any(|i| i.abi == m::ownership::declarations::ImportAbi::C)
            || decl
                .functions()
                .iter()
                .any(|f| f.abi == m::ownership::declarations::FunctionAbi::CExport);
        if c != (manifest.profile() == Profile::C) {
            return Err(invalid("whole-unit ABI profile"));
        }
        if let Some(entry) = p.entry() {
            let f = decl
                .functions()
                .get(entry.0 as usize)
                .ok_or_else(|| invalid("entry index"))?;
            if !f.parameters.is_empty() || f.result.ty != m::ownership::Type::I64 {
                return Err(invalid("ordinary i64 entry"));
            }
        }
        let layouts = lang_runtime::layout::Layouts::compile(decl.types(), Default::default())
            .map_err(|e| error("program.lbc", ErrorKind::Layout(e)))?;
        if c {
            if !matches!(record.kind(), Kind::CObject(_)) {
                return Err(invalid("C object record kind"));
            }
            let target = match manifest.target() {
                Target::MacosAarch64 => lang_runtime::c_abi::Target::MacosAarch64,
                Target::WindowsX86_64 => lang_runtime::c_abi::Target::WindowsX86_64,
            };
            let artifact = lang_runtime::c_abi::Artifact::compile(decl, target, Default::default())
                .map_err(|e| error("program.lbc", ErrorKind::CMetadata(e)))?;
            owned::RecordedObject::from_record(record)
                .map_err(|e| error("program.native", ErrorKind::Record(e)))?
                .prepare_c(flow, &layouts, &artifact, Default::default(), 4096)
                .map_err(|e| error("program.native", ErrorKind::CPlan(e)))?;
        } else {
            if record.kind() != Kind::OwnershipUnit {
                return Err(invalid("whole-unit ownership record kind"));
            }
            owned::RecordedObject::from_record(record)
                .map_err(|e| error("program.native", ErrorKind::Record(e)))?
                .prepare(flow, &layouts, Default::default(), 4096)
                .map_err(|e| error("program.native", ErrorKind::OwnedPlan(e)))?;
        }
    }
    Ok(())
}
