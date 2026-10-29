//! Exclusive new-project publication, using an explicit host output request.
use super::{Diagnostic, manifest, output::Stage};
use cap_std::fs::Dir;
use std::path::Path;

fn io(path: &Path, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("PKG0005", error.to_string()).at_path(path)
}
pub(super) fn create(requested: &Path, name: Option<&str>) -> Result<(), Diagnostic> {
    let absolute = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| io(requested, e))?
            .join(requested)
    };
    match std::fs::symlink_metadata(&absolute) {
        Ok(_) => return Err(io(requested, "new package destination already exists")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(requested, e)),
    }
    let leaf = absolute
        .file_name()
        .ok_or_else(|| io(requested, "missing destination directory name"))?;
    let name = name
        .or_else(|| leaf.to_str())
        .filter(|n| manifest::valid_name(n))
        .ok_or_else(|| {
            Diagnostic::error(
                "PKG0003",
                "directory basename is not a package name; supply --name",
            )
            .at_path(requested)
        })?;
    let parent = absolute
        .parent()
        .ok_or_else(|| io(requested, "missing parent directory"))?;
    // The caller explicitly grants this output pathname. No source Graph or
    // diagnostic display name is used to obtain scaffold write authority.
    let anchor = std::fs::canonicalize(parent.join(".")).map_err(|e| io(parent, e))?;
    let parent =
        Dir::open_ambient_dir(&anchor, cap_std::ambient_authority()).map_err(|e| io(&anchor, e))?;
    let metadata = serde_json::json!({"schema":1,"package":{"name":name,"version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{},"tests":["tests/smoke.lang"]});
    let mut bytes = serde_json::to_vec_pretty(&metadata).map_err(|e| io(requested, e))?;
    bytes.push(b'\n');
    manifest::Manifest::parse(&bytes)?;
    let mut stage = Stage::new(&parent)?;
    stage.write("Lang.json", &bytes)?;
    stage.write("src/main.lang", b"fn main() -> i64 {\n    return 0;\n}\n")?;
    stage.write(
        "tests/smoke.lang",
        b"fn main() -> i64 {\n    return 0;\n}\n",
    )?;
    stage.close();
    crate::native::publish::rename(&anchor.join(stage.name()), &anchor.join(leaf))
        .map_err(|e| io(requested, e))
}
