use super::*;
use cap_std::fs::{Dir, OpenOptions};
use std::{
    fs,
    io::{Read, Write},
};

pub(super) const MAX_STATE: usize = 1024 * 1024;
const MAX_ENTRIES: usize = 128;
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema: u32,
    pub metadata_sha256: String,
    pub admitted_at: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
    pub channel: Channel,
    pub version: String,
    pub generation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Installed {
    pub channel: Channel,
    pub version: String,
    pub generation: String,
    pub receipt: Receipt,
}
impl Installed {
    pub fn selection(&self) -> Selection {
        Selection {
            channel: self.channel,
            version: self.version.clone(),
            generation: self.generation.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Floor {
    pub channel: Channel,
    pub receipt: Receipt,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub schema: u32,
    pub root_sha256: String,
    pub repository: String,
    pub target: Target,
    pub time_floor: u64,
    pub channels: Vec<Floor>,
    pub installed: Vec<Installed>,
    #[serde(deserialize_with = "required_option")]
    pub active: Option<Selection>,
}
fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
pub(super) struct Store {
    pub path: PathBuf,
    pub dir: Dir,
    pub root_bytes: Vec<u8>,
    pub state: State,
    _lock: fs::File,
}
pub(super) fn open_file(dir: &Dir, path: &Path, owned: bool) -> Result<cap_std::fs::File, Error> {
    open_mode(dir, path, owned, false)
}
fn open_mode(
    dir: &Dir,
    path: &Path,
    owned: bool,
    writable: bool,
) -> Result<cap_std::fs::File, Error> {
    if owned
        && dir
            .symlink_metadata(path)
            .map_err(|e| io("inspect owned input", path, e))?
            .file_type()
            .is_symlink()
    {
        return Err(state("owned installation input is a symlink"));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(writable);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | if owned { libc::O_NOFOLLOW } else { 0 });
    }
    let file = dir
        .open_with(path, &options)
        .map_err(|e| io("open regular input", path, e))?;
    if !file
        .metadata()
        .map_err(|e| io("inspect input", path, e))?
        .is_file()
    {
        return Err(state("installation input is not a regular file"));
    }
    Ok(file)
}
pub(super) fn read(dir: &Dir, path: &Path, limit: usize, owned: bool) -> Result<Vec<u8>, Error> {
    let file = open_file(dir, path, owned)?;
    if file
        .metadata()
        .map_err(|e| io("size input", path, e))?
        .len()
        > limit as u64
    {
        return Err(state("installation input byte limit exceeded"));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io("read input", path, e))?;
    if bytes.len() > limit {
        return Err(state("installation input byte limit exceeded"));
    }
    Ok(bytes)
}
pub(super) fn write_new(dir: &Dir, path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut file = dir
        .open_with(path, OpenOptions::new().write(true).create_new(true))
        .map_err(|e| io("create owned file", path, e))?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    if let Err(e) = result {
        let _ = dir.remove_file(path);
        return Err(io("write owned file", path, e));
    }
    Ok(())
}
fn write_state(dir: &Dir, value: &State) -> Result<(), Error> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| state("cannot encode installation state"))?;
    if bytes.len() > MAX_STATE {
        return Err(state("installation state byte limit exceeded"));
    }
    let temporary = Path::new("state.next");
    write_new(dir, temporary, &bytes)?;
    if let Err(e) = dir.rename(temporary, dir, "state.json") {
        let _ = dir.remove_file(temporary);
        return Err(io("replace installation state", "state.json", e));
    }
    Ok(())
}
fn inventory(dir: &Dir, name: &str) -> Result<(), Error> {
    for (index, entry) in dir
        .read_dir(name)
        .map_err(|e| io("enumerate installation", name, e))?
        .enumerate()
    {
        if index >= MAX_ENTRIES {
            return Err(state(
                "installation directory entry limit exceeded; recover before retry",
            ));
        }
        let entry = entry.map_err(|e| io("enumerate installation entry", name, e))?;
        let filename = entry
            .file_name()
            .into_string()
            .map_err(|_| state("unknown installation entry name"))?;
        let kind = entry
            .file_type()
            .map_err(|e| io("inspect installation entry", name, e))?;
        let hash = if name == "metadata" {
            if !kind.is_file() {
                return Err(state("retained metadata is not a regular file"));
            }
            filename
                .strip_suffix(".json")
                .ok_or_else(|| state("unknown retained metadata name"))?
        } else {
            if !kind.is_dir() {
                return Err(state("generation is not an owned directory"));
            }
            &filename
        };
        hex::<32>(hash)?;
    }
    Ok(())
}
impl Store {
    pub fn mutation_ready(&self) -> Result<(), Error> {
        if recovery::pending(self)?.is_pending() {
            return Err(state("interrupted installation requires recovery"));
        }
        Ok(())
    }
    pub fn retain(&self, bytes: &[u8], current: u64) -> Result<Receipt, Error> {
        let hash = digest(bytes);
        let name = PathBuf::from(format!("metadata/{hash}.json"));
        if self
            .dir
            .try_exists(&name)
            .map_err(|e| io("inspect retained metadata", &name, e))?
        {
            if read(&self.dir, &name, MAX_METADATA_BYTES, true)? != bytes {
                return Err(state("existing metadata bytes do not match"));
            }
        } else {
            let count = self
                .dir
                .read_dir("metadata")
                .map_err(|e| io("count metadata", "metadata", e))?
                .take(MAX_ENTRIES + 1)
                .count();
            if count >= MAX_ENTRIES {
                return Err(state("metadata capacity reached; recover before retry"));
            }
            write_new(&self.dir, &name, bytes)?;
        }
        Ok(Receipt {
            schema: 1,
            metadata_sha256: hash,
            admitted_at: current,
        })
    }
    pub fn commit(&mut self, next: State) -> Result<(), Error> {
        write_state(&self.dir, &next)?;
        self.state = next;
        Ok(())
    }
    pub fn generation_capacity(&self) -> Result<(), Error> {
        let count = self
            .dir
            .read_dir("generations")
            .map_err(|e| io("count generations", "generations", e))?
            .take(MAX_ENTRIES + 1)
            .count();
        if count >= MAX_ENTRIES {
            return Err(state("generation capacity reached; recover before retry"));
        }
        Ok(())
    }
    pub fn init(path: &Path, bytes: &[u8], target: Target, current: u64) -> Result<Self, Error> {
        let root = TrustedRoot::decode(bytes, current)?;
        let value = State {
            schema: 1,
            root_sha256: root.identity().into(),
            repository: root.repository().into(),
            target,
            time_floor: 0,
            channels: Vec::new(),
            installed: Vec::new(),
            active: None,
        };
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        builder
            .create(path)
            .map_err(|e| io("create isolated root", path, e))?;
        let result = (|| {
            let dir = Dir::open_ambient_dir(path, cap_std::ambient_authority())
                .map_err(|e| io("open isolated root", path, e))?;
            for name in ["metadata", "generations", "staging", "trash"] {
                dir.create_dir(name)
                    .map_err(|e| io("create installation directory", name, e))?;
            }
            write_new(&dir, Path::new("root.json"), bytes)?;
            write_new(&dir, Path::new("lock"), b"")?;
            write_state(&dir, &value)?;
            drop(dir);
            Self::open(path, target)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(path);
        }
        result
    }
    pub fn open(path: &Path, target: Target) -> Result<Self, Error> {
        let path = path
            .canonicalize()
            .map_err(|e| io("resolve installation root", path, e))?;
        let dir = Dir::open_ambient_dir(&path, cap_std::ambient_authority())
            .map_err(|e| io("open installation root", &path, e))?;
        let lock = open_mode(&dir, Path::new("lock"), true, true)?.into_std();
        lock.try_lock()
            .map_err(|e| io("lock installation", path.join("lock"), io::Error::other(e)))?;
        for name in ["metadata", "generations", "staging", "trash"] {
            let metadata = dir
                .symlink_metadata(name)
                .map_err(|e| io("inspect installation directory", name, e))?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(state("installation directory is not an owned directory"));
            }
        }
        let root_bytes = read(&dir, Path::new("root.json"), MAX_ROOT_BYTES, true)?;
        inventory(&dir, "metadata")?;
        inventory(&dir, "generations")?;
        let raw = read(&dir, Path::new("state.json"), MAX_STATE, true)?;
        let value: State =
            serde_json::from_slice(&raw).map_err(|_| state("invalid installation state"))?;
        let store = Self {
            path,
            dir,
            root_bytes,
            state: value,
            _lock: lock,
        };
        store.validate(target)?;
        recovery::pending(&store)?;
        Ok(store)
    }
    pub fn authenticate(&self, receipt: &Receipt, channel: Channel) -> Result<Admitted, Error> {
        if receipt.schema != 1
            || receipt.admitted_at > self.state.time_floor
            || receipt.admitted_at > MAX_TIME
        {
            return Err(state("invalid installation receipt time or schema"));
        }
        hex::<32>(&receipt.metadata_sha256)?;
        let root = TrustedRoot::decode(&self.root_bytes, receipt.admitted_at)?;
        let name = format!("metadata/{}.json", receipt.metadata_sha256);
        let bytes = read(&self.dir, Path::new(&name), MAX_METADATA_BYTES, true)?;
        if digest(&bytes) != receipt.metadata_sha256 {
            return Err(state("retained metadata digest mismatch"));
        }
        Ok(verify(
            &root,
            &bytes,
            super::super::Request {
                channel,
                target: self.state.target,
                now: receipt.admitted_at,
                previous: None,
            },
        )?)
    }
    fn validate(&self, target: Target) -> Result<(), Error> {
        let value = &self.state;
        let root = TrustedRoot::decode(&self.root_bytes, 0)?;
        if value.schema != 1
            || value.root_sha256 != root.identity()
            || value.repository != root.repository()
            || value.target != target
            || value.time_floor > MAX_TIME
            || value.channels.len() > 3
            || value.installed.len() > 64
        {
            return Err(state("installation state identity or count mismatch"));
        }
        let mut floors = BTreeMap::new();
        for floor in &value.channels {
            let admitted = self.authenticate(&floor.receipt, floor.channel)?;
            if floors.insert(floor.channel, admitted).is_some() {
                return Err(state("duplicate installed channel floor"));
            }
        }
        let mut installed = BTreeSet::new();
        for entry in &value.installed {
            if !installed.insert((entry.channel, &entry.version)) {
                return Err(state("duplicate installed channel version"));
            }
            let admitted = self.authenticate(&entry.receipt, entry.channel)?;
            let release = admitted.release(Some(&entry.version))?;
            if entry.generation != release.identity()? {
                return Err(state("installed generation identity mismatch"));
            }
            let floor = floors
                .get(&entry.channel)
                .ok_or_else(|| state("installed release lacks a channel floor"))?;
            let (older, newer) = (admitted.admission(), floor.admission());
            if older.sequence() > newer.sequence()
                || (older.sequence() == newer.sequence()
                    && older.payload_digest() != newer.payload_digest())
            {
                return Err(state(
                    "installed receipt exceeds or changes the channel floor",
                ));
            }
        }
        if value.active.as_ref().is_some_and(|active| {
            !value
                .installed
                .iter()
                .any(|entry| &entry.selection() == active)
        }) {
            return Err(state("active installation is absent from installed state"));
        }
        Ok(())
    }
}
