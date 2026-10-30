use super::*;
use cap_std::fs::{Dir, OpenOptions};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub(super) struct Stage {
    path: PathBuf,
    dir: Option<Dir>,
    published: bool,
}
impl Stage {
    pub fn create(store: &storage::Store) -> Result<Self, Error> {
        store
            .dir
            .create_dir("staging/install")
            .map_err(|e| io("reserve installation staging", "staging/install", e))?;
        let mut stage = Self {
            path: store.path.join("staging/install"),
            dir: None,
            published: false,
        };
        stage.dir = Some(
            store
                .dir
                .open_dir("staging/install")
                .map_err(|e| io("open installation staging", &stage.path, e))?,
        );
        Ok(stage)
    }
    pub fn dir(&self) -> &Dir {
        self.dir.as_ref().expect("live staging capability")
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn transfer(&self, source: &Dir, release: &Release) -> Result<(), Error> {
        for file in &release.files {
            let object = PathBuf::from(format!("objects/{}", file.sha256));
            let input = storage::open_file(source, &object, false)?;
            if input
                .metadata()
                .map_err(|e| io("inspect payload length", &object, e))?
                .len()
                != file.bytes
            {
                return Err(state("payload declared length mismatch"));
            }
            let path = Path::new(&file.path);
            if let Some(parent) = path.parent() {
                self.dir()
                    .create_dir_all(parent)
                    .map_err(|e| io("create candidate directory", parent, e))?;
            }
            let mut output = self
                .dir()
                .open_with(path, OpenOptions::new().write(true).create_new(true))
                .map_err(|e| io("create candidate file", path, e))?
                .into_std();
            hash_file(input, file, Some(&mut output))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                output
                    .set_permissions(fs::Permissions::from_mode(if file.executable {
                        0o755
                    } else {
                        0o644
                    }))
                    .map_err(|e| io("set candidate permissions", path, e))?;
            }
            output
                .sync_all()
                .map_err(|e| io("sync candidate payload", path, e))?;
        }
        Ok(())
    }
    pub fn publish(mut self, destination: &Path) -> Result<(), Error> {
        drop(self.dir.take());
        crate::native::publish::rename(&self.path, destination)
            .map_err(|e| io("publish verified generation", destination, e))?;
        self.published = true;
        Ok(())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        drop(self.dir.take());
        if !self.published {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
fn hash_file(
    input: cap_std::fs::File,
    file: &File,
    mut output: Option<&mut fs::File>,
) -> Result<(), Error> {
    let mut input = input.take(file.bytes + 1);
    let (mut hash, mut count, mut buffer) = (Sha256::new(), 0_u64, [0_u8; 65536]);
    loop {
        let n = input
            .read(&mut buffer)
            .map_err(|e| io("read signed payload", &file.path, e))?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .ok_or_else(|| state("payload byte count overflow"))?;
        if count > file.bytes {
            return Err(state("payload exceeds signed length"));
        }
        hash.update(&buffer[..n]);
        if let Some(writer) = &mut output {
            writer
                .write_all(&buffer[..n])
                .map_err(|e| io("write candidate payload", &file.path, e))?;
        }
    }
    use std::fmt::Write as _;
    let mut digest = String::with_capacity(64);
    for byte in hash.finalize() {
        write!(&mut digest, "{byte:02x}").expect("String write");
    }
    if count != file.bytes || digest != file.sha256 {
        return Err(state("payload length or digest mismatch"));
    }
    Ok(())
}
fn inventory(
    dir: &Dir,
    expected: &BTreeSet<String>,
    prefix: &str,
    seen: &mut BTreeSet<String>,
) -> Result<(), Error> {
    for entry in dir
        .entries()
        .map_err(|e| io("enumerate generation", prefix, e))?
    {
        let entry = entry.map_err(|e| io("read generation entry", prefix, e))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| state("unsigned generation filename"))?;
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if !expected.contains(&path) || !seen.insert(path.clone()) {
            return Err(state("generation contains an unsigned entry"));
        }
        let kind = entry
            .file_type()
            .map_err(|e| io("inspect generation entry", &path, e))?;
        if kind.is_dir() {
            inventory(
                &dir.open_dir(&name)
                    .map_err(|e| io("open generation directory", &path, e))?,
                expected,
                &path,
                seen,
            )?;
        } else if !kind.is_file() {
            return Err(state("generation contains a nonregular or symlink entry"));
        }
    }
    Ok(())
}
fn platform(dir: &Dir, target: Target) -> Result<(), Error> {
    let mut file = storage::open_file(dir, Path::new(target.compiler()), true)?;
    let mut header = [0_u8; 64];
    file.read_exact(&mut header)
        .map_err(|e| io("read compiler platform header", target.compiler(), e))?;
    let valid = match target {
        Target::Macos => {
            header[..4] == [0xcf, 0xfa, 0xed, 0xfe]
                && u32::from_le_bytes(header[4..8].try_into().unwrap()) == 0x0100_000c
                && u32::from_le_bytes(header[12..16].try_into().unwrap()) == 2
        }
        Target::Windows => {
            if &header[..2] != b"MZ" {
                return Err(state("compiler is not a PE executable"));
            }
            let offset = u64::from(u32::from_le_bytes(header[60..64].try_into().unwrap()));
            let length = file
                .metadata()
                .map_err(|e| io("inspect PE compiler", target.compiler(), e))?
                .len();
            if offset < 64 || offset.checked_add(26).is_none_or(|end| end > length) {
                return Err(state("invalid PE compiler header offset"));
            }
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| io("seek PE header", target.compiler(), e))?;
            let mut pe = [0_u8; 26];
            file.read_exact(&mut pe)
                .map_err(|e| io("read PE header", target.compiler(), e))?;
            let flags = u16::from_le_bytes(pe[22..24].try_into().unwrap());
            &pe[..4] == b"PE\0\0"
                && u16::from_le_bytes(pe[4..6].try_into().unwrap()) == 0x8664
                && u16::from_le_bytes(pe[24..26].try_into().unwrap()) == 0x20b
                && flags & 2 != 0
                && flags & 0x2000 == 0
        }
    };
    if !valid {
        return Err(state(
            "compiler platform format does not match signed target",
        ));
    }
    Ok(())
}
fn capture(reader: impl Read + Send + 'static) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (send, receive) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = reader.take(65537).read_to_end(&mut bytes).map(|_| bytes);
        let _ = send.send(result);
    });
    receive
}
fn identity(path: &Path, target: Target) -> Result<Identity, Error> {
    let executable = path.join(target.compiler());
    let started = Instant::now();
    let mut child = Command::new(&executable)
        .args(["toolchain", "identity"])
        .current_dir(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| io("run verified compiler identity", &executable, e))?;
    let stdout = capture(child.stdout.take().expect("piped stdout"));
    let stderr = capture(child.stderr.take().expect("piped stderr"));
    let result = (|| {
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| io("poll compiler identity", &executable, e))?
            {
                break status;
            }
            if started.elapsed() >= Duration::from_secs(10) {
                return Err(state("compiler identity timed out"));
            }
            thread::sleep(Duration::from_millis(10));
        };
        let mut outputs = Vec::new();
        for receiver in [stdout, stderr] {
            let remaining = Duration::from_secs(10).saturating_sub(started.elapsed());
            let bytes = receiver
                .recv_timeout(remaining)
                .map_err(|_| state("compiler identity output timed out"))?
                .map_err(|e| io("read compiler identity output", &executable, e))?;
            if bytes.len() > 65536 {
                return Err(state("compiler identity output limit exceeded"));
            }
            outputs.push(bytes);
        }
        if !status.success() {
            return Err(state("verified compiler identity command failed"));
        }
        serde_json::from_slice(&outputs[0]).map_err(|_| state("invalid compiler identity record"))
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}
pub(super) fn verify(
    dir: &Dir,
    path: &Path,
    release: &Release,
    target: Target,
) -> Result<(), Error> {
    let mut expected = BTreeSet::new();
    for file in &release.files {
        expected.insert(file.path.clone());
        for (i, _) in file.path.match_indices('/') {
            expected.insert(file.path[..i].into());
        }
    }
    let mut seen = BTreeSet::new();
    inventory(dir, &expected, "", &mut seen)?;
    if seen != expected {
        return Err(state("generation is missing signed files or directories"));
    }
    for file in &release.files {
        let input = storage::open_file(dir, Path::new(&file.path), true)?;
        #[cfg(windows)]
        if input
            .metadata()
            .map_err(|e| io("inspect file permissions", &file.path, e))?
            .permissions()
            .readonly()
        {
            return Err(state("generation file permissions changed"));
        }
        #[cfg(unix)]
        {
            use cap_std::fs::PermissionsExt;
            let mode = input
                .metadata()
                .map_err(|e| io("inspect file permissions", &file.path, e))?
                .permissions()
                .mode()
                & 0o7777;
            if mode != if file.executable { 0o755 } else { 0o644 } {
                return Err(state("generation file permissions changed"));
            }
        }
        hash_file(input, file, None)?;
    }
    let sdk_bytes = storage::read(dir, Path::new("sdk/sdk.json"), MAX_METADATA_BYTES, true)?;
    let sdk = crate::native::manifest::parse_sdk(&sdk_bytes, Default::default())
        .map_err(|_| state("invalid signed SDK manifest"))?;
    if sdk.target().text() != target.text()
        || sdk.build_id().to_string() != release.compiler_build_id
    {
        return Err(state("signed SDK/compiler identity mismatch"));
    }
    let files: BTreeMap<_, _> = release
        .files
        .iter()
        .filter_map(|f| f.path.strip_prefix("sdk/").map(|name| (name, f)))
        .collect();
    if files.len() != sdk.files().len() + 1 {
        return Err(state("signed SDK file table does not reconcile"));
    }
    for file in sdk.files() {
        let signed = files
            .get(file.path())
            .ok_or_else(|| state("SDK references an unsigned file"))?;
        if signed.bytes != file.bytes() || signed.sha256 != file.hash().to_string() {
            return Err(state("SDK manifest disagrees with signed payload"));
        }
    }
    platform(dir, target)?;
    let observed = identity(path, target)?;
    if observed.schema != 1
        || !crate::package::manifest::valid_version(&observed.version)
        || observed.target != target
        || observed.compiler_build_id != release.compiler_build_id
    {
        return Err(state(
            "installed compiler identity disagrees with signed release",
        ));
    }
    Ok(())
}
