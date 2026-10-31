use super::*;
use cap_std::fs::{Dir, OpenOptions};
use std::io::{Read, Write};

pub(super) const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub(super) const MAX_ENTRIES: usize = 262_144;
const DIRECTORIES: [&str; 3] = ["objects", "generations", "staging"];

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Object,
    Generation,
}
impl Kind {
    fn directory(self) -> &'static str {
        match self {
            Self::Object => "objects",
            Self::Generation => "generations",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Object => "object-",
            Self::Generation => "generation-",
        }
    }
    pub fn limit(self) -> usize {
        match self {
            Self::Object => crate::registry::archive::MAX_ARCHIVE_BYTES,
            Self::Generation => crate::package::lock::MAX_BYTES,
        }
    }
}

#[derive(Default)]
pub(super) struct Inventory {
    pub bytes: u64,
    pub entries: usize,
    pub objects: BTreeMap<String, ObjectSummary>,
    pub generations: BTreeSet<String>,
    pub bindings: BTreeMap<Locator, String>,
    stages: Vec<String>,
    referenced: BTreeSet<String>,
}
impl Inventory {
    fn charge(&mut self, bytes: u64, entries: usize) -> Result<(), Error> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(capacity)?;
        self.entries = self
            .entries
            .checked_add(entries)
            .filter(|n| *n <= MAX_ENTRIES)
            .ok_or_else(capacity)?;
        Ok(())
    }
    pub fn reserve(&self, bytes: u64, entries: usize) -> Result<(), Error> {
        // A staged file is renamed, not copied: reserving every final new file
        // also covers every intermediate staging byte/entry, including equality.
        self.bytes
            .checked_add(bytes)
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(capacity)?;
        self.entries
            .checked_add(entries)
            .filter(|n| *n <= MAX_ENTRIES)
            .ok_or_else(capacity)?;
        Ok(())
    }
    pub fn bind(&mut self, reference: &Reference) -> Result<(), Error> {
        match self.bindings.get(&reference.locator) {
            Some(hash) if hash != &reference.archive_sha256 => return Err(conflict()),
            Some(_) => return Ok(()),
            None => (),
        }
        if self.bindings.len() == MAX_ENTRIES {
            return Err(capacity());
        }
        self.bindings
            .insert(reference.locator.clone(), reference.archive_sha256.clone());
        Ok(())
    }
}

pub(super) struct Storage {
    dir: Dir,
    _lock: Lock,
}
struct Lock(std::fs::File);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn component(name: &str) -> Result<(), Error> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(error(
            ErrorKind::Malformed,
            "invalid cache directory component",
        ));
    }
    Ok(())
}
fn directory(parent: &Dir, name: &str) -> Result<Dir, Error> {
    use cap_fs_ext::DirExt;
    let metadata = parent.symlink_metadata(name).map_err(|_| corrupt())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(corrupt());
    }
    parent.open_dir_nofollow(name).map_err(|_| corrupt())
}
fn open_file(dir: &Dir, name: &str, write: bool) -> Result<cap_std::fs::File, Error> {
    let metadata = dir.symlink_metadata(name).map_err(|_| corrupt())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(corrupt());
    }
    let mut options = OpenOptions::new();
    options.read(true).write(write);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = dir.open_with(name, &options).map_err(|_| io())?;
    let metadata = file.metadata().map_err(|_| io())?;
    #[cfg(unix)]
    let links = {
        use cap_std::fs::MetadataExt;
        metadata.nlink()
    };
    #[cfg(windows)]
    let links = {
        use cap_fs_ext::MetadataExt;
        metadata.nlink()
    };
    #[cfg(not(any(unix, windows)))]
    let links = 0;
    if !metadata.is_file() || links != 1 {
        return Err(corrupt());
    }
    Ok(file)
}
fn read(dir: &Dir, name: &str, limit: usize) -> Result<Vec<u8>, Error> {
    let file = open_file(dir, name, false)?;
    let length = file.metadata().map_err(|_| io())?.len();
    if length > limit as u64 {
        return Err(capacity());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| io())?;
    if bytes.len() > limit {
        return Err(capacity());
    }
    if bytes.len() as u64 != length {
        return Err(corrupt());
    }
    Ok(bytes)
}
fn write_new(dir: &Dir, name: &str, bytes: &[u8]) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = dir.open_with(name, &options).map_err(|_| io())?;
    checkpoint("create");
    let split = bytes.len() / 2;
    file.write_all(&bytes[..split]).map_err(|_| io())?;
    checkpoint("partial-write");
    file.write_all(&bytes[split..]).map_err(|_| io())?;
    checkpoint("write");
    file.sync_all().map_err(|_| io())?;
    checkpoint("sync");
    Ok(())
}

impl Storage {
    pub fn require_external_output(&self, output: &Dir) -> Result<(), Error> {
        use crate::package::source::identity;
        let path = std::path::Path::new("");
        let output =
            identity(&output.dir_metadata().map_err(|_| io())?, path).map_err(|_| corrupt())?;
        let mut directories = vec![self.dir.try_clone().map_err(|_| io())?];
        for name in DIRECTORIES {
            directories.push(directory(&self.dir, name)?);
        }
        for dir in directories {
            let actual =
                identity(&dir.dir_metadata().map_err(|_| io())?, path).map_err(|_| corrupt())?;
            if actual == output {
                return Err(error(
                    ErrorKind::Conflict,
                    "root output aliases cache storage",
                ));
            }
        }
        Ok(())
    }
    pub fn create(parent: &Dir, child: &str) -> Result<Self, Error> {
        component(child)?;
        let builder = cap_std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use cap_std::fs::DirBuilderExt;
            let mut b = builder;
            b.mode(0o700);
            b
        };
        parent.create_dir_with(child, &builder).map_err(|_| io())?;
        checkpoint("cache-root-created");
        let dir = directory(parent, child)?;
        for name in DIRECTORIES {
            dir.create_dir(name).map_err(|_| io())?;
            checkpoint("cache-directory-created");
        }
        write_new(&dir, "LOCK", b"")?;
        write_new(&dir, "FORMAT", FORMAT)?;
        Self::locked(dir, true)
    }
    pub fn open(parent: &Dir, child: &str, write: bool) -> Result<Self, Error> {
        component(child)?;
        Self::locked(directory(parent, child)?, write)
    }
    fn locked(dir: Dir, write: bool) -> Result<Self, Error> {
        let file = open_file(&dir, "LOCK", write)?.into_std();
        if file.metadata().map_err(|_| io())?.len() != 0 {
            return Err(corrupt());
        }
        let result = if write {
            file.try_lock()
        } else {
            file.try_lock_shared()
        };
        result.map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => error(ErrorKind::Busy, "registry cache is busy"),
            std::fs::TryLockError::Error(_) => io(),
        })?;
        Ok(Self {
            dir,
            _lock: Lock(file),
        })
    }
    pub fn read(&self, kind: Kind, hash: &str) -> Result<Vec<u8>, Error> {
        if !valid_hash(hash) {
            return Err(corrupt());
        }
        let bytes = read(&directory(&self.dir, kind.directory())?, hash, kind.limit())?;
        if digest(&bytes) != hash {
            return Err(integrity());
        }
        Ok(bytes)
    }
    pub fn inventory(&self) -> Result<Inventory, Error> {
        let mut view = Inventory::default();
        let mut names = BTreeSet::new();
        for entry in self.dir.entries().map_err(|_| io())? {
            view.charge(0, 1)?;
            let name = entry
                .map_err(|_| io())?
                .file_name()
                .into_string()
                .map_err(|_| corrupt())?;
            if !names.insert(name.clone()) {
                return Err(corrupt());
            }
            match name.as_str() {
                "LOCK" => {
                    if open_file(&self.dir, &name, false)?
                        .metadata()
                        .map_err(|_| io())?
                        .len()
                        != 0
                    {
                        return Err(corrupt());
                    }
                }
                "FORMAT" => {
                    let bytes = read(&self.dir, &name, FORMAT.len())?;
                    if bytes != FORMAT {
                        return Err(error(
                            ErrorKind::Malformed,
                            "unsupported registry cache profile",
                        ));
                    }
                    view.charge(bytes.len() as u64, 0)?;
                }
                name if DIRECTORIES.contains(&name) => {
                    directory(&self.dir, name)?;
                }
                _ => return Err(corrupt()),
            }
        }
        if names.len() != 5 {
            return Err(corrupt());
        }
        for kind in [Kind::Object, Kind::Generation] {
            let dir = directory(&self.dir, kind.directory())?;
            for entry in dir.entries().map_err(|_| io())? {
                view.charge(0, 1)?;
                let name = entry
                    .map_err(|_| io())?
                    .file_name()
                    .into_string()
                    .map_err(|_| corrupt())?;
                if !valid_hash(&name) {
                    return Err(corrupt());
                }
                let bytes = read(&dir, &name, kind.limit())?;
                view.charge(bytes.len() as u64, 0)?;
                if digest(&bytes) != name {
                    return Err(integrity());
                }
                match kind {
                    Kind::Object => {
                        let archive = AdmittedArchive::admit(&bytes).map_err(|_| integrity())?;
                        let summary = ObjectSummary::capture(&archive).map_err(|_| integrity())?;
                        if view.objects.insert(name, summary).is_some() {
                            return Err(corrupt());
                        }
                    }
                    Kind::Generation => {
                        let references = references(&bytes).map_err(|_| corrupt())?;
                        check_generation_bytes(&references)?;
                        for reference in references {
                            let object = view
                                .objects
                                .get(&reference.archive_sha256)
                                .ok_or_else(corrupt)?;
                            if !reference.matches(object) {
                                return Err(integrity());
                            }
                            view.bind(&reference)?;
                            view.referenced.insert(reference.archive_sha256);
                        }
                        if !view.generations.insert(name) {
                            return Err(corrupt());
                        }
                    }
                }
            }
        }
        let dir = directory(&self.dir, "staging")?;
        for entry in dir.entries().map_err(|_| io())? {
            view.charge(0, 1)?;
            let name = entry
                .map_err(|_| io())?
                .file_name()
                .into_string()
                .map_err(|_| corrupt())?;
            let kind = if let Some(hash) = name.strip_prefix(Kind::Object.prefix()) {
                if !valid_hash(hash) {
                    return Err(corrupt());
                }
                Kind::Object
            } else if let Some(hash) = name.strip_prefix(Kind::Generation.prefix()) {
                if !valid_hash(hash) {
                    return Err(corrupt());
                }
                Kind::Generation
            } else {
                return Err(corrupt());
            };
            let length = open_file(&dir, &name, false)?
                .metadata()
                .map_err(|_| io())?
                .len();
            if length > kind.limit() as u64 {
                return Err(capacity());
            }
            view.charge(length, 0)?;
            view.stages.push(name);
        }
        Ok(view)
    }
    pub fn cleanup(&self, view: &Inventory) -> Result<(), Error> {
        let stages = directory(&self.dir, "staging")?;
        for name in &view.stages {
            open_file(&stages, name, false)?;
            stages.remove_file(name).map_err(|_| io())?;
        }
        let objects = directory(&self.dir, "objects")?;
        for name in view
            .objects
            .keys()
            .filter(|n| !view.referenced.contains(*n))
        {
            // Recheck exact bytes at the deletion boundary as well.
            self.read(Kind::Object, name)?;
            objects.remove_file(name).map_err(|_| io())?;
        }
        Ok(())
    }
    pub fn install(&self, kind: Kind, hash: &str, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > kind.limit() || !valid_hash(hash) || digest(bytes) != hash {
            return Err(integrity());
        }
        let destination = directory(&self.dir, kind.directory())?;
        match destination.symlink_metadata(hash) {
            Ok(_) => {
                if self.read(kind, hash)? != bytes {
                    return Err(integrity());
                }
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(io()),
        }
        let stages = directory(&self.dir, "staging")?;
        let name = format!("{}{hash}", kind.prefix());
        write_new(&stages, &name, bytes)?;
        checkpoint("before-immutable-install");
        stages.rename(&name, &destination, hash).map_err(|_| io())?;
        checkpoint("after-immutable-install");
        Ok(())
    }
}
