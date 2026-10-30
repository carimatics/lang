//! Trusted, explicit-input release production; signing secrets stay in memory.
use super::*;
use cap_std::fs::{Dir, OpenOptions};
use ed25519_dalek::{Signer, SigningKey};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_SIGNING_INPUT: usize = 4096;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SigningInput {
    schema: u32,
    signing_keys: Vec<String>,
}
/// Decode bounded stdin material without including secret bytes in any error.
pub fn signing_keys(bytes: &[u8]) -> Result<Vec<SigningKey>, Error> {
    if bytes.len() > MAX_SIGNING_INPUT {
        return Err(state("signing input byte limit exceeded"));
    }
    let input: SigningInput =
        serde_json::from_slice(bytes).map_err(|_| state("invalid signing input"))?;
    if input.schema != 1 || input.signing_keys.is_empty() || input.signing_keys.len() > 8 {
        return Err(state("invalid signing input schema or count"));
    }
    let mut seen = BTreeSet::new();
    let mut keys = Vec::new();
    for seed in input.signing_keys {
        let bytes = hex::<32>(&seed).map_err(|_| state("invalid signing key encoding"))?;
        let key = SigningKey::from_bytes(&bytes);
        if !seen.insert(key.verifying_key().to_bytes()) {
            return Err(state("duplicate signing key"));
        }
        keys.push(key);
    }
    Ok(keys)
}
pub struct Request<'a> {
    pub sdk: &'a Path,
    pub output: &'a Path,
    pub trust_root: &'a [u8],
    pub channel: Channel,
    pub version: &'a str,
    pub sequence: u64,
    pub issued: u64,
    pub expires: u64,
    pub signing_keys: &'a [SigningKey],
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub kind: &'static str,
    pub root_sha256: String,
    pub repository: String,
    pub channel: Channel,
    pub target: Target,
    pub sequence: u64,
    pub version: String,
    pub generation: String,
    pub compiler_build_id: String,
    pub files: usize,
    pub bytes: u64,
}
struct Input {
    name: String,
    file: File,
}
struct Stage {
    path: PathBuf,
    output: PathBuf,
    dir: Option<Dir>,
    published: bool,
}
impl Stage {
    fn create(output: &Path) -> Result<Self, Error> {
        let name = output
            .file_name()
            .ok_or_else(|| state("distribution output requires a directory name"))?;
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map_err(|e| io("resolve distribution parent", output, e))?;
        let output = parent.join(name);
        match fs::symlink_metadata(&output) {
            Ok(_) => return Err(state("distribution destination already exists")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(io("inspect distribution destination", output, e)),
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..16 {
            let path = parent.join(format!(
                ".lang-distribution-{}-{}",
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
                    let mut stage = Self {
                        path,
                        output,
                        dir: None,
                        published: false,
                    };
                    stage.dir = Some(
                        Dir::open_ambient_dir(&stage.path, cap_std::ambient_authority())
                            .map_err(|e| io("open distribution stage", &stage.path, e))?,
                    );
                    return Ok(stage);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io("reserve distribution stage", path, e)),
            }
        }
        Err(state("distribution staging collision limit exceeded"))
    }
    fn dir(&self) -> &Dir {
        self.dir.as_ref().expect("live distribution stage")
    }
    fn publish(mut self) -> Result<(), Error> {
        drop(self.dir.take());
        crate::native::publish::rename(&self.path, &self.output)
            .map_err(|e| io("publish signed distribution", &self.output, e))?;
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
/// Produce one verified bundle without generating or publishing signing material.
pub fn produce(request: Request<'_>) -> Result<Report, Error> {
    produce_inner(request).map_err(|mut error| {
        // A private validation install is never an existing user's admission floor.
        error.floor_advanced = false;
        error
    })
}
fn produce_inner(request: Request<'_>) -> Result<Report, Error> {
    let current = now()?;
    let root = TrustedRoot::decode(request.trust_root, current)?;
    if request.signing_keys.is_empty() || request.signing_keys.len() > 8 {
        return Err(state("invalid signing key count"));
    }
    let target = host()?;
    let sdk_dir = Dir::open_ambient_dir(request.sdk, cap_std::ambient_authority())
        .map_err(|e| io("open SDK for distribution", request.sdk, e))?;
    let sdk_bytes = storage::read(&sdk_dir, Path::new("sdk.json"), MAX_METADATA_BYTES, false)?;
    let sdk = crate::native::manifest::parse_sdk(&sdk_bytes, Default::default())
        .map_err(|_| state("invalid distribution SDK manifest"))?;
    if sdk.target().text() != target.text() {
        return Err(state("distribution SDK is not for this host"));
    }
    let compiler = if target == Target::Windows {
        "bin/lang.exe"
    } else {
        "bin/lang"
    };
    let mut inputs = Vec::new();
    for file in sdk.files() {
        let executable = file.path() == compiler;
        let signed = File {
            path: format!("sdk/{}", file.path()),
            bytes: file.bytes(),
            sha256: file.hash().to_string(),
            executable,
        };
        if executable {
            let mut root_compiler = signed.clone();
            root_compiler.path = compiler.into();
            inputs.push(Input {
                name: file.path().into(),
                file: root_compiler,
            });
        }
        inputs.push(Input {
            name: file.path().into(),
            file: signed,
        });
    }
    inputs.push(Input {
        name: "sdk.json".into(),
        file: File {
            path: "sdk/sdk.json".into(),
            bytes: sdk_bytes.len() as u64,
            sha256: digest(&sdk_bytes),
            executable: false,
        },
    });
    let release = Release {
        version: request.version.into(),
        compiler_build_id: sdk.build_id().to_string(),
        files: inputs.iter().map(|i| i.file.clone()).collect(),
    };
    let payload = Payload {
        schema: 1,
        repository: root.repository().into(),
        channel: request.channel,
        target,
        sequence: request.sequence,
        issued: request.issued,
        expires: request.expires,
        current_version: request.version.into(),
        releases: vec![release.clone()],
    };
    let payload =
        serde_json::to_string(&payload).map_err(|_| state("cannot encode distribution payload"))?;
    let mut message = SIGNING_DOMAIN.to_vec();
    message.extend_from_slice(payload.as_bytes());
    let mut signatures = Vec::new();
    for key in request.signing_keys {
        let signature: String = key
            .sign(&message)
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        signatures.push(serde_json::json!({"key_id":digest(&key.verifying_key().to_bytes()),"signature":signature}));
    }
    let envelope = serde_json::to_vec(
        &serde_json::json!({"schema":1,"payload":payload,"signatures":signatures}),
    )
    .map_err(|_| state("cannot encode distribution envelope"))?;
    let admitted = super::super::verify(
        &root,
        &envelope,
        super::super::Request {
            channel: request.channel,
            target,
            now: current,
            previous: None,
        },
    )?;
    let stage = Stage::create(request.output)?;
    let channel_dir = format!("channels/{}", request.channel.text());
    stage
        .dir()
        .create_dir_all(&channel_dir)
        .map_err(|e| io("create distribution channel", &channel_dir, e))?;
    stage
        .dir()
        .create_dir("objects")
        .map_err(|e| io("create distribution objects", "objects", e))?;
    for input in &inputs {
        let object = PathBuf::from(format!("objects/{}", input.file.sha256));
        let source = storage::open_file(&sdk_dir, Path::new(&input.name), false)?;
        if stage
            .dir()
            .try_exists(&object)
            .map_err(|e| io("inspect distribution object", &object, e))?
        {
            payload::hash_file(source, &input.file, None)?;
            payload::hash_file(
                storage::open_file(stage.dir(), &object, true)?,
                &input.file,
                None,
            )?;
        } else {
            let mut output = stage
                .dir()
                .open_with(&object, OpenOptions::new().write(true).create_new(true))
                .map_err(|e| io("create distribution object", &object, e))?
                .into_std();
            payload::hash_file(source, &input.file, Some(&mut output))?;
            output
                .sync_all()
                .map_err(|e| io("sync distribution object", &object, e))?;
        }
    }
    let metadata = format!("{channel_dir}/{}.json", target.text());
    storage::write_new(stage.dir(), Path::new(&metadata), &envelope)?;
    let checked = stage.path.join("checked-install");
    {
        let mut installation = Installation::init(&checked, request.trust_root)?;
        installation.install(super::Request {
            source: &stage.path,
            channel: request.channel,
            version: Some(request.version),
        })?;
    }
    stage
        .dir()
        .remove_dir_all("checked-install")
        .map_err(|e| io("remove distribution validation state", &checked, e))?;
    super::super::verify(
        &root,
        &envelope,
        super::super::Request {
            channel: request.channel,
            target,
            now: now()?,
            previous: Some(admitted.admission()),
        },
    )?;
    let report = Report {
        schema: 1,
        kind: "toolchain_distribution",
        root_sha256: root.identity().into(),
        repository: root.repository().into(),
        channel: request.channel,
        target,
        sequence: request.sequence,
        version: request.version.into(),
        generation: release.identity()?,
        compiler_build_id: release.compiler_build_id,
        files: release.files.len(),
        bytes: release.files.iter().map(|f| f.bytes).sum(),
    };
    stage.publish()?;
    Ok(report)
}
