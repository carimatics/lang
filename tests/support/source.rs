use lang_diagnostics::{Severity, SourceMap};
use lang_hir::program::{CheckLimits, check_program};
use lang_mir::unit as m;
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_modules},
    program_lowering,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Files(pub PathBuf);
impl Files {
    pub fn new(text: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "lang source conformance {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let files = Self(directory);
        fs::write(files.0.join("source.lang"), text).unwrap();
        files
    }
    pub fn unit(&self) -> m::VerifiedUnit {
        let mut sources = SourceMap::new();
        let loaded = load_modules(
            &mut sources,
            &self.0.join("source.lang"),
            &self.0,
            ModuleLimits::default(),
        )
        .unwrap();
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        let bound =
            bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
        assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
        let checked =
            check_program(bound.program.unwrap(), &sources, CheckLimits::default()).unwrap();
        assert!(
            checked
                .diagnostics
                .iter()
                .all(|d| d.severity == Severity::Warning),
            "{:?}",
            checked.diagnostics
        );
        program_lowering::lower(&checked.program.unwrap(), &sources, m::Limits::default()).unwrap()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
