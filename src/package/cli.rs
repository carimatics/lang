//! Package inspection commands; dependency IO is deliberately absent here.
use super::{Diagnostic, manifest::Manifest};
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;

const USAGE: &str = "usage: lang package check-manifest [--message-format human|json] FILE";

pub fn run(arguments: &[OsString], stderr: &mut impl Write) -> io::Result<u8> {
    // Recognize JSON even when another argument is invalid, so usage failures
    // remain machine readable. Parsing still refuses duplicate format options.
    let json = arguments
        .windows(2)
        .any(|pair| pair[0] == "--message-format" && pair[1] == "json");
    let parsed = parse(arguments);
    let (status, diagnostic) = match parsed {
        Err(message) => (2, Some(Diagnostic::error("CLI0001", message))),
        Ok(file) => match Manifest::read(&file) {
            Ok(_) => (0, None),
            Err(error) => (1, Some(error)),
        },
    };
    if let Some(diagnostic) = diagnostic {
        if json {
            serde_json::to_writer(&mut *stderr, &diagnostic)?;
            writeln!(stderr)?;
        } else {
            writeln!(stderr, "error: {diagnostic}")?;
            if status == 2 {
                writeln!(stderr, "{USAGE}")?;
            }
        }
    }
    Ok(status)
}

fn parse(arguments: &[OsString]) -> Result<PathBuf, String> {
    if arguments.first().is_none_or(|a| a != "check-manifest") {
        return Err("expected package check-manifest".into());
    }
    let mut file = None;
    let mut format_seen = false;
    let mut args = arguments.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--message-format" {
            if format_seen {
                return Err("duplicate --message-format".into());
            }
            format_seen = true;
            if args.next().is_none_or(|v| v != "human" && v != "json") {
                return Err("--message-format requires human or json".into());
            }
        } else if arg.to_str().is_some_and(|v| v.starts_with('-')) {
            return Err(format!("unknown package option: {}", arg.to_string_lossy()));
        } else if file.replace(PathBuf::from(arg)).is_some() {
            return Err("only one manifest path is allowed".into());
        }
    }
    file.ok_or_else(|| "missing manifest path".into())
}
