use super::*;
use std::{
    ffi::OsString,
    process::{Child, Command},
};

#[derive(Debug, Serialize)]
pub struct Location {
    pub schema: u32,
    pub kind: &'static str,
    pub root_sha256: String,
    pub repository: String,
    pub target: Target,
    pub active: Summary,
    pub compiler: PathBuf,
    pub sdk: PathBuf,
}
impl Installation {
    pub fn list(&self) -> Result<Report, Error> {
        self.report(Operation::List, false)
    }
    fn verified(&self, selected: &storage::Selection) -> Result<Location, Error> {
        let entry = self
            .store
            .state
            .installed
            .iter()
            .find(|e| e.selection() == *selected)
            .ok_or_else(|| state("selected toolchain is not installed"))?;
        let admitted = self.store.authenticate(&entry.receipt, entry.channel)?;
        let release = admitted.release(Some(&entry.version))?;
        let relative = format!("generations/{}", entry.generation);
        let path = self.store.path.join(&relative);
        let dir = self
            .store
            .dir
            .open_dir(&relative)
            .map_err(|e| io("open selected generation", &relative, e))?;
        payload::verify(&dir, &path, release, self.store.state.target)?;
        let executable = match self.store.state.target {
            Target::Macos => "bin/lang",
            Target::Windows => "bin/lang.exe",
        };
        Ok(Location {
            schema: 1,
            kind: "toolchain_location",
            root_sha256: self.root_sha256().into(),
            repository: self.repository().into(),
            target: self.store.state.target,
            active: Summary {
                channel: entry.channel,
                version: entry.version.clone(),
                compiler_build_id: release.compiler_build_id.clone(),
                generation: entry.generation.clone(),
            },
            compiler: path.join(executable),
            sdk: path.join("sdk"),
        })
    }
    pub fn which(&self) -> Result<Location, Error> {
        let active = self
            .store
            .state
            .active
            .as_ref()
            .ok_or_else(|| state("no active toolchain"))?;
        self.verified(active)
    }
    pub fn select(&mut self, channel: Channel, version: &str) -> Result<Report, Error> {
        self.store.mutation_ready()?;
        let selected = self
            .store
            .state
            .installed
            .iter()
            .find(|e| e.channel == channel && e.version == version)
            .ok_or_else(|| state("requested toolchain is not installed"))?
            .selection();
        self.verified(&selected)?;
        let mut next = self.store.state.clone();
        next.active = Some(selected);
        self.store.commit(next)?;
        self.report(Operation::Select, false)
    }
    /// Spawn with inherited standard streams, releasing the installation lock before returning.
    pub fn spawn(self, args: &[OsString]) -> Result<Child, Error> {
        let location = self.which()?;
        let child = Command::new(&location.compiler)
            .args(args)
            .spawn()
            .map_err(|e| io("spawn installed compiler", &location.compiler, e))?;
        drop(self);
        Ok(child)
    }
    pub fn uninstall(&mut self, channel: Channel, version: &str) -> Result<Report, Error> {
        self.uninstall_with(channel, version, |_| false)
    }
    /// Trusted failure hook; no CLI verification bypass is provided.
    pub fn uninstall_with(
        &mut self,
        channel: Channel,
        version: &str,
        mut fail: impl FnMut(Checkpoint) -> bool,
    ) -> Result<Report, Error> {
        self.store.mutation_ready()?;
        let index = self
            .store
            .state
            .installed
            .iter()
            .position(|e| e.channel == channel && e.version == version)
            .ok_or_else(|| state("requested toolchain is not installed"))?;
        let entry = &self.store.state.installed[index];
        if self.store.state.active.as_ref() == Some(&entry.selection()) {
            return Err(state("cannot uninstall the active toolchain"));
        }
        let generation = entry.generation.clone();
        let mut next = self.store.state.clone();
        next.installed.remove(index);
        if next.installed.iter().any(|e| e.generation == generation) {
            self.store.commit(next)?;
        } else {
            recovery::uninstall(&mut self.store, next, &generation, &mut fail)?;
        }
        self.report(Operation::Uninstall, false)
    }
    pub fn recover(&mut self) -> Result<Report, Error> {
        recovery::recover(&self.store)?;
        self.report(Operation::Recover, false)
    }
}
