use super::*;
use cap_std::fs::{Dir, OpenOptions};
use std::collections::BTreeMap;
use std::io::{Read, Write};

pub(super) const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub(super) const MAX_ENTRIES: usize = 262_144;
const DIRECTORIES: [&str; 4] = ["objects", "states", "audits", "staging"];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Space {
    Object,
    Audit,
    State,
    Head,
}
impl Space {
    fn directory(self) -> &'static str {
        match self {
            Self::Object => "objects",
            Self::Audit => "audits",
            Self::State => "states",
            Self::Head => "",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Audit => "audit",
            Self::State => "state",
            Self::Head => "head",
        }
    }
    fn limit(self) -> usize {
        match self {
            Self::Object => archive::MAX_ARCHIVE_BYTES,
            Self::Audit => records::MAX_AUDIT,
            Self::State => records::MAX_STATE,
            Self::Head => records::MAX_HEAD,
        }
    }
    fn path(self, hash: &str) -> String {
        format!("{}/{hash}", self.directory())
    }
    fn staged(self, hash: &str) -> String {
        format!("staging/{}-{hash}", self.prefix())
    }
}
pub(super) struct Inventory {
    pub bytes: u64,
    pub entries: usize,
    pub immutable: BTreeMap<(Space, String), u64>,
    pub bindings: BTreeMap<String, String>,
    pub staged: Vec<String>,
}
impl Inventory {
    pub fn charge(&mut self, bytes: u64, entries: usize) -> Result<(), Error> {
        self.bytes = self.bytes.checked_add(bytes).ok_or_else(capacity)?;
        self.entries = self.entries.checked_add(entries).ok_or_else(capacity)?;
        if self.bytes > MAX_BYTES || self.entries > MAX_ENTRIES {
            return Err(capacity());
        }
        Ok(())
    }
    pub fn reserve(
        &self,
        candidates: &[(Space, &str, &[u8])],
        head_bytes: usize,
    ) -> Result<(), Error> {
        let (mut bytes, mut entries) = (self.bytes, self.entries);
        for (space, hash, candidate) in candidates {
            if !self.immutable.contains_key(&(*space, (*hash).into())) {
                bytes = bytes
                    .checked_add(candidate.len() as u64)
                    .ok_or_else(capacity)?;
                entries = entries.checked_add(1).ok_or_else(capacity)?;
            }
        }
        bytes = bytes.checked_add(head_bytes as u64).ok_or_else(capacity)?;
        entries = entries.checked_add(1).ok_or_else(capacity)?;
        if bytes > MAX_BYTES || entries > MAX_ENTRIES {
            return Err(capacity());
        }
        Ok(())
    }
}

pub(super) struct Storage {
    pub dir: Dir,
    _lock: Lock,
}
struct Lock(std::fs::File);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
fn component(name: &str) -> Result<(), Error> {
    // A host-selected child name, never a path or stored-data-derived value.
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(error(
            ErrorKind::Malformed,
            "invalid registry directory component",
        ));
    }
    Ok(())
}
fn directory(parent: &Dir, name: &str) -> Result<Dir, Error> {
    use cap_fs_ext::DirExt;
    let metadata = parent.symlink_metadata(name).map_err(|_| io())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(corrupt());
    }
    parent.open_dir_nofollow(name).map_err(|_| io())
}
fn open_file(dir: &Dir, path: &str, write: bool) -> Result<cap_std::fs::File, Error> {
    let metadata = dir.symlink_metadata(path).map_err(|_| io())?;
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
    let file = dir.open_with(path, &options).map_err(|_| io())?;
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
impl Storage {
    pub fn create(parent: &Dir, child: &str) -> Result<Self, Error> {
        component(child)?;
        let builder = cap_std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use cap_std::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        parent.create_dir_with(child, &builder).map_err(|_| io())?;
        let dir = directory(parent, child)?;
        for name in DIRECTORIES {
            dir.create_dir(name).map_err(|_| io())?;
        }
        write_new(&dir, "LOCK", b"")?;
        Self::locked(dir)
    }
    pub fn open(parent: &Dir, child: &str) -> Result<Self, Error> {
        component(child)?;
        Self::locked(directory(parent, child)?)
    }
    fn locked(dir: Dir) -> Result<Self, Error> {
        let file = open_file(&dir, "LOCK", true)?.into_std();
        if file.metadata().map_err(|_| io())?.len() != 0 {
            return Err(corrupt());
        }
        file.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => error(ErrorKind::Busy, "registry is busy"),
            std::fs::TryLockError::Error(_) => io(),
        })?;
        Ok(Self {
            dir,
            _lock: Lock(file),
        })
    }
    pub fn read(&self, path: &str, limit: usize) -> Result<Vec<u8>, Error> {
        let file = open_file(&self.dir, path, false)?;
        if file.metadata().map_err(|_| io())?.len() > limit as u64 {
            return Err(capacity());
        }
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| io())?;
        if bytes.len() > limit {
            return Err(capacity());
        }
        Ok(bytes)
    }
    pub fn immutable(&self, space: Space, hash: &str) -> Result<Vec<u8>, Error> {
        records::hex::<32>(hash)?;
        let bytes = self.read(&space.path(hash), space.limit())?;
        if digest(&bytes) != hash {
            return Err(corrupt());
        }
        Ok(bytes)
    }
    pub fn head(&self) -> Result<String, Error> {
        let h: records::Head =
            records::decode(&self.read("HEAD", records::MAX_HEAD)?, records::MAX_HEAD)?;
        if h.schema != 1 {
            return Err(corrupt());
        }
        records::hex::<32>(&h.state_sha256)?;
        Ok(h.state_sha256)
    }
    pub fn inventory(&self, identity: &ServiceIdentity) -> Result<Inventory, Error> {
        let mut inventory = Inventory {
            bytes: 0,
            entries: 0,
            immutable: BTreeMap::new(),
            bindings: BTreeMap::new(),
            staged: Vec::new(),
        };
        let mut seen = std::collections::BTreeSet::new();
        for entry in self.dir.entries().map_err(|_| io())? {
            inventory.charge(0, 1)?;
            let entry = entry.map_err(|_| io())?;
            let name = entry.file_name().into_string().map_err(|_| corrupt())?;
            if !seen.insert(name.clone()) {
                return Err(corrupt());
            }
            match name.as_str() {
                "LOCK" | "HEAD" => {
                    let limit = if name == "LOCK" { 0 } else { records::MAX_HEAD };
                    inventory.charge(self.read(&name, limit)?.len() as u64, 0)?;
                }
                "objects" | "states" | "audits" | "staging" => {
                    directory(&self.dir, &name)?;
                }
                _ => return Err(corrupt()),
            }
        }
        if seen.len() != 6 {
            return Err(corrupt());
        }
        for namespace in DIRECTORIES {
            let child = directory(&self.dir, namespace)?;
            for entry in child.entries().map_err(|_| io())? {
                inventory.charge(0, 1)?;
                let filename = entry
                    .map_err(|_| io())?
                    .file_name()
                    .into_string()
                    .map_err(|_| corrupt())?;
                let (space, hash) = match namespace {
                    "objects" => (Space::Object, filename.as_str()),
                    "states" => (Space::State, filename.as_str()),
                    "audits" => (Space::Audit, filename.as_str()),
                    "staging" => {
                        let (prefix, hash) = filename.split_once('-').ok_or_else(corrupt)?;
                        let space = match prefix {
                            "object" => Space::Object,
                            "audit" => Space::Audit,
                            "state" => Space::State,
                            "head" => Space::Head,
                            _ => return Err(corrupt()),
                        };
                        (space, hash)
                    }
                    _ => return Err(corrupt()),
                };
                records::hex::<32>(hash)?;
                let path = format!("{namespace}/{filename}");
                let bytes = self.read(&path, space.limit())?;
                inventory.charge(bytes.len() as u64, 0)?;
                if namespace == "staging" {
                    inventory.staged.push(path);
                    continue;
                }
                if digest(&bytes) != hash {
                    return Err(corrupt());
                }
                match space {
                    Space::Object => {
                        let archive = AdmittedArchive::admit(&bytes).map_err(|_| corrupt())?;
                        let binding = records::Release::from_archive(&archive)
                            .binding(archive.descriptor().package().name())?;
                        inventory.bindings.insert(hash.into(), binding);
                    }
                    Space::State => {
                        let s: records::State = records::decode(&bytes, space.limit())?;
                        s.validate(identity)?;
                    }
                    Space::Audit => {
                        let a: records::Audit = records::decode(&bytes, space.limit())?;
                        a.validate()?;
                    }
                    Space::Head => return Err(corrupt()),
                }
                if inventory
                    .immutable
                    .insert((space, hash.into()), bytes.len() as u64)
                    .is_some()
                {
                    return Err(corrupt());
                }
            }
        }
        Ok(inventory)
    }
    pub fn cleanup(
        &self,
        inventory: &mut Inventory,
        reachable: &std::collections::BTreeSet<(Space, String)>,
    ) -> Result<(), Error> {
        // Called only after the entire inventory and committed chain succeeded.
        for path in inventory.staged.drain(..) {
            self.dir.remove_file(&path).map_err(|_| io())?;
        }
        let unused: Vec<_> = inventory
            .immutable
            .keys()
            .filter(|key| !reachable.contains(*key))
            .cloned()
            .collect();
        for (space, hash) in unused {
            self.dir.remove_file(space.path(&hash)).map_err(|_| io())?;
            inventory.immutable.remove(&(space, hash));
        }
        // Retain the pre-cleanup accounting: it is a conservative peak bound and
        // avoids a second enumeration. The next operation may reclaim capacity.
        Ok(())
    }
    pub fn install(&self, space: Space, hash: &str, bytes: &[u8]) -> Result<(), Error> {
        records::hex::<32>(hash)?;
        if bytes.len() > space.limit() || digest(bytes) != hash {
            return Err(corrupt());
        }
        let path = space.path(hash);
        match self.dir.symlink_metadata(&path) {
            Ok(_) => {
                if self.immutable(space, hash)? != bytes {
                    return Err(corrupt());
                }
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(io()),
        }
        let stage = space.staged(hash);
        write_new(&self.dir, &stage, bytes)?;
        checkpoint("before-immutable-move");
        self.dir
            .rename(&stage, &self.dir, &path)
            .map_err(|_| io())?;
        checkpoint("after-immutable-move");
        Ok(())
    }
    pub fn replace_head(
        &self,
        hash: &str,
        bytes: &[u8],
        authorize: impl FnOnce() -> Result<(), Error>,
    ) -> Result<(), Error> {
        let stage = Space::Head.staged(hash);
        write_new(&self.dir, &stage, bytes)?;
        checkpoint("before-head-replace");
        authorize()?;
        #[cfg(test)]
        super::tests::replacement_fault(false, &self.dir)?;
        self.dir
            .rename(&stage, &self.dir, "HEAD")
            .map_err(|_| io())?;
        #[cfg(test)]
        super::tests::replacement_fault(true, &self.dir)?;
        checkpoint("after-head-replace");
        Ok(())
    }
}
fn write_new(dir: &Dir, path: &str, bytes: &[u8]) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = dir.open_with(path, &options).map_err(|_| io())?;
    checkpoint("after-create");
    let split = bytes.len() / 2;
    file.write_all(&bytes[..split]).map_err(|_| io())?;
    checkpoint("after-partial-write");
    file.write_all(&bytes[split..]).map_err(|_| io())?;
    checkpoint("after-write");
    file.sync_all().map_err(|_| io())?;
    checkpoint("after-sync");
    Ok(())
}
