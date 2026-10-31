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
    pub(super) entry_limit: usize,
    pub immutable: BTreeMap<(Space, String), u64>,
    pub bindings: BTreeMap<String, String>,
    pub staged: Vec<String>,
}
impl Inventory {
    pub fn charge(&mut self, bytes: u64, entries: usize) -> Result<(), Error> {
        self.bytes = self.bytes.checked_add(bytes).ok_or_else(capacity)?;
        self.entries = self.entries.checked_add(entries).ok_or_else(capacity)?;
        if self.bytes > MAX_BYTES || self.entries > self.entry_limit {
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
        if bytes > MAX_BYTES || entries > self.entry_limit {
            return Err(capacity());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) struct Envelope {
    pub state: usize,
    pub object: usize,
    pub entries: usize,
}

pub(super) struct Probe {
    pub state: usize,
    pub object: usize,
    pub entries: usize,
    pub head: String,
}

pub(super) struct Storage {
    pub dir: Dir,
    _lock: Lock,
    pub envelope: Option<Envelope>,
    pub control: Option<super::workspace::RequestControl>,
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
            envelope: None,
            control: None,
        })
    }
    pub fn check_request(&self) -> Result<(), Error> {
        if let Some(control) = &self.control {
            control.check()?;
        }
        Ok(())
    }
    pub fn state_limit(&self) -> usize {
        self.envelope.map_or(records::MAX_STATE, |e| e.state)
    }
    fn space_limit(&self, space: Space) -> usize {
        match (space, self.envelope) {
            (Space::State, Some(e)) => e.state,
            (Space::Object, Some(e)) => e.object,
            _ => space.limit(),
        }
    }
    // The caller already owns the store lock and a fixed probe reservation.
    // Inspect one directory entry at a time; never decode records or clean up.
    pub fn probe(&self, clock: &impl Clock) -> Result<Probe, Error> {
        let mut probe = Probe {
            state: 0,
            object: 0,
            entries: 0,
            head: String::new(),
        };
        let mut bytes = 0u64;
        let mut seen = 0u8;
        for entry in self.dir.entries().map_err(|_| io())? {
            clock.now()?;
            probe.entries = probe.entries.checked_add(1).ok_or_else(capacity)?;
            if probe.entries > MAX_ENTRIES {
                return Err(capacity());
            }
            let name = entry
                .map_err(|_| io())?
                .file_name()
                .into_string()
                .map_err(|_| corrupt())?;
            let bit = match name.as_str() {
                "LOCK" => {
                    if open_file(&self.dir, &name, false)?
                        .metadata()
                        .map_err(|_| io())?
                        .len()
                        != 0
                    {
                        return Err(corrupt());
                    }
                    1
                }
                "HEAD" => {
                    let len = open_file(&self.dir, &name, false)?
                        .metadata()
                        .map_err(|_| io())?
                        .len();
                    if len > records::MAX_HEAD as u64 {
                        return Err(capacity());
                    }
                    bytes = bytes.checked_add(len).ok_or_else(capacity)?;
                    2
                }
                "objects" => {
                    directory(&self.dir, &name)?;
                    4
                }
                "states" => {
                    directory(&self.dir, &name)?;
                    8
                }
                "audits" => {
                    directory(&self.dir, &name)?;
                    16
                }
                "staging" => {
                    directory(&self.dir, &name)?;
                    32
                }
                _ => return Err(corrupt()),
            };
            if seen & bit != 0 {
                return Err(corrupt());
            }
            seen |= bit;
        }
        if seen != 63 {
            return Err(corrupt());
        }
        for namespace in DIRECTORIES {
            let child = directory(&self.dir, namespace)?;
            for entry in child.entries().map_err(|_| io())? {
                clock.now()?;
                probe.entries = probe.entries.checked_add(1).ok_or_else(capacity)?;
                if probe.entries > MAX_ENTRIES {
                    return Err(capacity());
                }
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
                            "state" => Space::State,
                            "audit" => Space::Audit,
                            "head" => Space::Head,
                            _ => return Err(corrupt()),
                        };
                        (space, hash)
                    }
                    _ => return Err(corrupt()),
                };
                records::hex::<32>(hash)?;
                let len = open_file(&child, &filename, false)?
                    .metadata()
                    .map_err(|_| io())?
                    .len();
                if len > space.limit() as u64 {
                    return Err(capacity());
                }
                bytes = bytes.checked_add(len).ok_or_else(capacity)?;
                if bytes > MAX_BYTES {
                    return Err(capacity());
                }
                let len = usize::try_from(len).map_err(|_| capacity())?;
                match space {
                    Space::State => probe.state = probe.state.max(len),
                    Space::Object => probe.object = probe.object.max(len),
                    _ => (),
                }
            }
        }
        clock.now()?;
        probe.head = self.head()?;
        Ok(probe)
    }
    pub fn read(&self, path: &str, limit: usize) -> Result<Vec<u8>, Error> {
        self.check_request()?;
        let limit = if path.starts_with("states/") || path.starts_with("staging/state-") {
            limit.min(self.state_limit())
        } else if path.starts_with("objects/") || path.starts_with("staging/object-") {
            limit.min(self.space_limit(Space::Object))
        } else {
            limit
        };
        let file = open_file(&self.dir, path, false)?;
        if file.metadata().map_err(|_| io())?.len() > limit as u64 {
            return Err(capacity());
        }
        let mut bytes = Vec::new();
        let mut reader = file.take(limit as u64 + 1);
        let mut chunk = [0; 64 * 1024];
        loop {
            self.check_request()?;
            let count = match reader.read(&mut chunk) {
                Ok(count) => count,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(io()),
            };
            if count == 0 {
                break;
            }
            bytes.try_reserve(count).map_err(|_| capacity())?;
            bytes.extend_from_slice(&chunk[..count]);
        }
        self.check_request()?;
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

    pub fn captured_object(&self, hash: &str, length: usize) -> Result<Vec<u8>, Error> {
        self.check_request()?;
        records::hex::<32>(hash)?;
        if length > archive::MAX_ARCHIVE_BYTES {
            return Err(capacity());
        }
        let mut file = open_file(&self.dir, &Space::Object.path(hash), false)?;
        if file.metadata().map_err(|_| io())?.len() != length as u64 {
            return Err(corrupt());
        }
        // The caller reserves the exact complete body before this allocation.
        // Do not use read_to_end's geometric buffer growth for served objects.
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).map_err(|_| capacity())?;
        bytes.resize(length, 0);
        for chunk in bytes.chunks_mut(64 * 1024) {
            self.check_request()?;
            file.read_exact(chunk).map_err(|_| io())?;
        }
        if file.read(&mut [0]).map_err(|_| io())? != 0 || digest(&bytes) != hash {
            return Err(corrupt());
        }
        self.check_request()?;
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
        self.check_request()?;
        let mut inventory = Inventory {
            bytes: 0,
            entries: 0,
            entry_limit: self.envelope.map_or(MAX_ENTRIES, |e| e.entries),
            immutable: BTreeMap::new(),
            bindings: BTreeMap::new(),
            staged: Vec::new(),
        };
        let mut seen = std::collections::BTreeSet::new();
        for entry in self.dir.entries().map_err(|_| io())? {
            self.check_request()?;
            inventory.charge(0, 1)?;
            let entry = entry.map_err(|_| io())?;
            let name = entry.file_name().into_string().map_err(|_| corrupt())?;
            if !seen.insert(name.clone()) {
                return Err(corrupt());
            }
            match name.as_str() {
                "LOCK" => {
                    // Windows byte-range locks also reject reads through another
                    // handle, including an EOF probe of this empty file. Recheck
                    // the no-follow regular-file/link invariant and exact length
                    // without reading through a second handle while we own it.
                    let file = open_file(&self.dir, &name, false)?;
                    if file.metadata().map_err(|_| io())?.len() != 0 {
                        return Err(corrupt());
                    }
                }
                "HEAD" => {
                    inventory.charge(self.read(&name, records::MAX_HEAD)?.len() as u64, 0)?;
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
                self.check_request()?;
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
            self.check_request()?;
            self.dir.remove_file(&path).map_err(|_| io())?;
        }
        let unused: Vec<_> = inventory
            .immutable
            .keys()
            .filter(|key| !reachable.contains(*key))
            .cloned()
            .collect();
        for (space, hash) in unused {
            self.check_request()?;
            self.dir.remove_file(space.path(&hash)).map_err(|_| io())?;
            inventory.immutable.remove(&(space, hash));
        }
        // Retain the pre-cleanup accounting: it is a conservative peak bound and
        // avoids a second enumeration. The next operation may reclaim capacity.
        Ok(())
    }
    pub fn install(&self, space: Space, hash: &str, bytes: &[u8]) -> Result<(), Error> {
        self.check_request()?;
        records::hex::<32>(hash)?;
        if bytes.len() > self.space_limit(space) {
            return Err(capacity());
        }
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
        write_new_guarded(&self.dir, &stage, bytes, || self.check_request())?;
        checkpoint("before-immutable-move");
        self.check_request()?;
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
        write_new_guarded(&self.dir, &stage, bytes, || self.check_request())?;
        checkpoint("before-head-replace");
        authorize()?;
        self.check_request()?;
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
    write_new_guarded(dir, path, bytes, || Ok(()))
}
fn write_new_guarded(
    dir: &Dir,
    path: &str,
    bytes: &[u8],
    check: impl Fn() -> Result<(), Error>,
) -> Result<(), Error> {
    check()?;
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
    for chunk in bytes[..split].chunks(64 * 1024) {
        check()?;
        file.write_all(chunk).map_err(|_| io())?;
    }
    checkpoint("after-partial-write");
    for chunk in bytes[split..].chunks(64 * 1024) {
        check()?;
        file.write_all(chunk).map_err(|_| io())?;
    }
    checkpoint("after-write");
    file.sync_all().map_err(|_| io())?;
    checkpoint("after-sync");
    check()?;
    Ok(())
}
