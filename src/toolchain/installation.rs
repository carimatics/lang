//! Explicit-root, authenticated host installation state (RFC 0042).
use super::*;
use std::{
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub mod distribution;
mod operations;
mod payload;
mod recovery;
mod source;
mod storage;
pub use operations::Location;
pub use source::Source;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub schema: u32,
    pub version: String,
    pub target: Target,
    pub compiler_build_id: String,
}
impl Identity {
    pub fn current() -> Result<Self, Error> {
        let target = host()?;
        Ok(Self {
            schema: 1,
            version: env!("CARGO_PKG_VERSION").into(),
            target,
            compiler_build_id: lang_codegen_clif::sdk::build_id(
                &target.text().parse().expect("fixed host triple"),
            ),
        })
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Init,
    Install,
    Update,
    List,
    Select,
    Uninstall,
    Recover,
}
#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub channel: Channel,
    pub version: String,
    pub compiler_build_id: String,
    pub generation: String,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub kind: &'static str,
    pub operation: Operation,
    pub root_sha256: String,
    pub repository: String,
    pub target: Target,
    pub active: Option<Summary>,
    pub installed: Vec<Summary>,
    pub floor_advanced: bool,
}
pub struct Request<'a> {
    pub source: &'a Path,
    pub channel: Channel,
    pub version: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Checkpoint {
    BeforeFloorCommit,
    AfterFloorCommit,
    AfterPayloadVerification,
    AfterGenerationPublication,
    BeforeActivationCommit,
    AfterUninstallMarker,
    AfterUninstallMove,
    AfterUninstallCommit,
}
#[derive(Debug)]
pub enum Failure {
    Admission(super::Error),
    Transport(Box<ureq::Error>),
    Storage {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    State(&'static str),
    Injected(Checkpoint),
    Child(&'static str),
}
#[derive(Debug)]
pub struct Error {
    pub failure: Failure,
    pub floor_advanced: bool,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.failure {
            Failure::Admission(e) => e.fmt(f),
            Failure::Transport(e) => write!(f, "HTTPS distribution request failed: {e}"),
            Failure::Storage {
                operation,
                path,
                source,
            } => write!(f, "{operation} at {}: {source}", path.display()),
            Failure::State(message) | Failure::Child(message) => f.write_str(message),
            Failure::Injected(point) => write!(f, "injected installation failure at {point:?}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<super::Error> for Error {
    fn from(error: super::Error) -> Self {
        Self {
            failure: Failure::Admission(error),
            floor_advanced: false,
        }
    }
}
fn state(message: &'static str) -> Error {
    Error {
        failure: Failure::State(message),
        floor_advanced: false,
    }
}
fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Error {
    Error {
        failure: Failure::Storage {
            operation,
            path: path.into(),
            source,
        },
        floor_advanced: false,
    }
}
fn now() -> Result<u64, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .map_err(|_| state("host clock precedes Unix epoch"))
}
fn host() -> Result<Target, Error> {
    match lang_codegen_clif::Triple::host().to_string().as_str() {
        "aarch64-apple-darwin" => Ok(Target::Macos),
        "x86_64-pc-windows-msvc" => Ok(Target::Windows),
        _ => Err(state("unsupported installation host")),
    }
}

/// Holds the installation's nonblocking exclusive lock until dropped.
pub struct Installation {
    store: storage::Store,
}
impl Installation {
    pub fn init(root: &Path, trust_root: &[u8]) -> Result<Self, Error> {
        storage::Store::init(root, trust_root, host()?, now()?).map(|store| Self { store })
    }
    pub fn open(root: &Path) -> Result<Self, Error> {
        storage::Store::open(root, host()?).map(|store| Self { store })
    }
    pub fn root(&self) -> &Path {
        &self.store.path
    }
    pub fn root_sha256(&self) -> &str {
        &self.store.state.root_sha256
    }
    pub fn repository(&self) -> &str {
        &self.store.state.repository
    }
    pub fn time_floor(&self) -> u64 {
        self.store.state.time_floor
    }
    pub fn report(&self, operation: Operation, floor_advanced: bool) -> Result<Report, Error> {
        let mut installed = Vec::new();
        let mut active = None;
        for entry in &self.store.state.installed {
            let admitted = self.store.authenticate(&entry.receipt, entry.channel)?;
            let release = admitted.release(Some(&entry.version))?;
            let summary = Summary {
                channel: entry.channel,
                version: entry.version.clone(),
                compiler_build_id: release.compiler_build_id.clone(),
                generation: entry.generation.clone(),
            };
            if self.store.state.active.as_ref() == Some(&entry.selection()) {
                active = Some(summary.clone());
            }
            installed.push(summary);
        }
        Ok(Report {
            schema: 1,
            kind: "toolchain_operation",
            operation,
            root_sha256: self.root_sha256().into(),
            repository: self.repository().into(),
            target: self.store.state.target,
            active,
            installed,
            floor_advanced,
        })
    }
    pub fn install(&mut self, request: Request<'_>) -> Result<Report, Error> {
        self.install_with(request, |_| false)
    }
    /// Trusted host test hook: returning true injects failure at a checkpoint.
    /// It receives no mutable admission/state and is not exposed as a CLI option.
    pub fn install_with(
        &mut self,
        request: Request<'_>,
        fail: impl FnMut(Checkpoint) -> bool,
    ) -> Result<Report, Error> {
        let source = Source::local(request.source)?;
        self.install_source_with(&source, request.channel, request.version, fail)
    }
    pub fn install_source(
        &mut self,
        source: &Source,
        channel: Channel,
        version: Option<&str>,
    ) -> Result<Report, Error> {
        self.install_source_with(source, channel, version, |_| false)
    }
    pub fn update(&mut self, source: &Source, channel: Channel) -> Result<Report, Error> {
        let mut report = self.install_source(source, channel, None)?;
        report.operation = Operation::Update;
        Ok(report)
    }
    fn install_source_with(
        &mut self,
        source: &Source,
        channel: Channel,
        version: Option<&str>,
        mut fail: impl FnMut(Checkpoint) -> bool,
    ) -> Result<Report, Error> {
        let checkpoint = |point, fail: &mut dyn FnMut(Checkpoint) -> bool| {
            if fail(point) {
                Err(Error {
                    failure: Failure::Injected(point),
                    floor_advanced: false,
                })
            } else {
                Ok(())
            }
        };
        self.store.mutation_ready()?;
        let current = now()?;
        if current < self.store.state.time_floor {
            return Err(state("clock moved behind installed admission floor"));
        }
        let root = TrustedRoot::decode(&self.store.root_bytes, current)?;
        let bytes = source.metadata(channel, self.store.state.target)?;
        let previous = self
            .store
            .state
            .channels
            .iter()
            .find(|f| f.channel == channel)
            .map(|f| self.store.authenticate(&f.receipt, f.channel))
            .transpose()?;
        let admitted = super::verify(
            &root,
            &bytes,
            super::Request {
                channel,
                target: self.store.state.target,
                now: current,
                previous: previous.as_ref().map(Admitted::admission),
            },
        )?;
        let release = admitted.release(version)?.clone();
        let generation = release.identity()?;
        let existing = self
            .store
            .state
            .installed
            .iter()
            .find(|e| e.channel == channel && e.version == release.version);
        if let Some(existing) = existing {
            if existing.generation != generation {
                return Err(state(
                    "installed channel/version is bound to another generation",
                ));
            }
        } else if self.store.state.installed.len() >= 64 {
            return Err(state("installed release capacity reached"));
        }
        let advanced = previous.as_ref().is_none_or(|p| {
            p.admission().sequence() < admitted.admission().sequence()
                || p.admission().time_floor() < current
        });
        checkpoint(Checkpoint::BeforeFloorCommit, &mut fail)?;
        let receipt = self.store.retain(&bytes, current)?;
        let mut next = self.store.state.clone();
        next.time_floor = current;
        if let Some(floor) = next.channels.iter_mut().find(|f| f.channel == channel) {
            floor.receipt = receipt.clone();
        } else {
            next.channels.push(storage::Floor {
                channel,
                receipt: receipt.clone(),
            });
        }
        self.store.commit(next)?;
        let result = (|| {
            checkpoint(Checkpoint::AfterFloorCommit, &mut fail)?;
            let relative = PathBuf::from(format!("generations/{generation}"));
            let destination = self.store.path.join(&relative);
            if self
                .store
                .dir
                .try_exists(&relative)
                .map_err(|e| io("inspect generation", &relative, e))?
            {
                let dir = self
                    .store
                    .dir
                    .open_dir(&relative)
                    .map_err(|e| io("open existing generation", &relative, e))?;
                payload::verify(&dir, &destination, &release, self.store.state.target)?;
                checkpoint(Checkpoint::AfterPayloadVerification, &mut fail)?;
            } else {
                self.store.generation_capacity()?;
                let stage = payload::Stage::create(&self.store)?;
                stage.transfer(source, &release)?;
                payload::verify(stage.dir(), stage.path(), &release, self.store.state.target)?;
                checkpoint(Checkpoint::AfterPayloadVerification, &mut fail)?;
                stage.publish(&destination)?;
            }
            checkpoint(Checkpoint::AfterGenerationPublication, &mut fail)?;
            let mut next = self.store.state.clone();
            if !next
                .installed
                .iter()
                .any(|e| e.channel == channel && e.version == release.version)
            {
                next.installed.push(storage::Installed {
                    channel,
                    version: release.version.clone(),
                    generation: generation.clone(),
                    receipt,
                });
            }
            next.active = Some(storage::Selection {
                channel,
                version: release.version,
                generation,
            });
            checkpoint(Checkpoint::BeforeActivationCommit, &mut fail)?;
            super::verify(
                &root,
                &bytes,
                super::Request {
                    channel,
                    target: self.store.state.target,
                    now: now()?,
                    previous: Some(admitted.admission()),
                },
            )?;
            self.store.commit(next)?;
            self.report(Operation::Install, advanced)
        })();
        result.map_err(|mut error: Error| {
            error.floor_advanced = advanced;
            error
        })
    }
}
