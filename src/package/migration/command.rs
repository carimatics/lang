//! Explicit standalone historical source migration; no ambient package discovery.
use super::{Limits, Plan, Preview, invalid};
use crate::package::{
    Diagnostic,
    command::{Format, emit, requested_format},
};
use cap_std::{ambient_authority, fs::Dir};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const USAGE: &str = "       lang migrate --from-profile rfc0006-scalar --to-edition 2026 --source-root DIR [--apply] [--message-format human|json] FILE\n";

struct Request {
    root: PathBuf,
    file: PathBuf,
    apply: bool,
}
fn usage(message: &str) -> Diagnostic {
    Diagnostic::error("CLI0001", message)
}
fn parse(args: &[OsString]) -> Result<Request, Diagnostic> {
    let (mut root, mut file, mut profile, mut edition) = (None, None, None, None);
    let mut apply = false;
    let mut seen = BTreeSet::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if let Some(flag) = arg.to_str().filter(|s| s.starts_with('-')) {
            if !seen.insert(flag) {
                return Err(usage("duplicate migration option"));
            }
            if flag == "--apply" {
                apply = true;
                continue;
            }
            if !matches!(
                flag,
                "--from-profile" | "--to-edition" | "--source-root" | "--message-format"
            ) {
                return Err(usage("unknown migration option"));
            }
            let value = args
                .next()
                .filter(|v| !v.is_empty() && !v.to_string_lossy().starts_with("--"))
                .ok_or_else(|| usage("missing migration option value"))?;
            match flag {
                "--from-profile" => profile = Some(value),
                "--to-edition" => edition = Some(value),
                "--source-root" => root = Some(PathBuf::from(value)),
                "--message-format" if value == "human" || value == "json" => {}
                "--message-format" => return Err(usage("message format must be human or json")),
                _ => unreachable!(),
            }
        } else if file.replace(PathBuf::from(arg)).is_some() {
            return Err(usage("migration requires exactly one source file"));
        }
    }
    if !profile.is_some_and(|v| v == "rfc0006-scalar") {
        return Err(usage("migration requires --from-profile rfc0006-scalar"));
    }
    if !edition.is_some_and(|v| v == "2026") {
        return Err(usage("migration requires --to-edition 2026"));
    }
    Ok(Request {
        root: root.ok_or_else(|| usage("migration requires --source-root DIR"))?,
        file: file.ok_or_else(|| usage("migration requires a source file"))?,
        apply,
    })
}
fn filesystem(path: &Path, error: io::Error) -> Diagnostic {
    Diagnostic::error("PKG0005", error.to_string()).at_path(path)
}
fn prepare(request: &Request) -> Result<(Dir, Plan), Diagnostic> {
    // Match ordinary source commands: FILE is cwd-relative, not root-relative.
    // Preserve its spelling until the shared alias checks. Canonicalizing FILE
    // here would hide a symlink by replacing it with the target's name.
    let root = std::path::absolute(&request.root).map_err(|e| filesystem(&request.root, e))?;
    let file = std::path::absolute(&request.file).map_err(|e| filesystem(&request.file, e))?;
    let relative = file.strip_prefix(&root).map_err(|_| {
        invalid("migration source must be inside --source-root", None).at_path(&request.file)
    })?;
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err(invalid(
            "migration source cannot traverse outside its canonical relative path",
            None,
        )
        .at_path(&request.file));
    }
    let authority = Dir::open_ambient_dir(&root, ambient_authority())
        .map_err(|e| filesystem(&request.root, e))?;
    let plan = Plan::capture(&authority, relative, Limits::default())?;
    Ok((authority, plan))
}
fn report(stdout: &mut impl Write, format: Format, preview: &Preview) -> io::Result<()> {
    if format == Format::Json {
        serde_json::to_writer(&mut *stdout, preview)?;
        return writeln!(stdout);
    }
    writeln!(
        stdout,
        "migrate {} -> edition {}: {} changed, {} applied ({}, {})",
        preview.source_profile,
        preview.target_edition,
        preview.changed,
        preview.applied,
        preview.outcome,
        preview.applicability
    )?;
    for root in &preview.roots {
        writeln!(stdout, "root {root:?}")?;
    }
    for file in &preview.files {
        writeln!(stdout, "file {file:?}")?;
    }
    for edit in &preview.edits {
        writeln!(
            stdout,
            "{:?}:{}..{} => {} ({})",
            edit.path, edit.span.start, edit.span.end, edit.replacement, edit.recipe
        )?;
    }
    Ok(())
}
pub fn run(args: &[OsString], stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<u8> {
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        write!(stdout, "{USAGE}")?;
        return Ok(0);
    }
    let format = requested_format(args);
    let request = match parse(args) {
        Ok(request) => request,
        Err(error) => {
            emit(stderr, format, &error)?;
            return Ok(2);
        }
    };
    let (authority, plan) = match prepare(&request) {
        Ok(value) => value,
        Err(error) => {
            emit(stderr, format, &error)?;
            return Ok(1);
        }
    };
    if request.apply {
        match plan.apply(&authority) {
            Ok(preview) => report(stdout, format, &preview)?,
            Err(error) => {
                emit(stderr, format, &error)?;
                return Ok(1);
            }
        }
    } else {
        report(stdout, format, plan.preview())?;
    }
    Ok(0)
}
