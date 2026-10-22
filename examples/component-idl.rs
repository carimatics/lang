//! Check an explicit IDL file; checking declares no runtime authority.
use lang_mir::unit::SourceId;
use lang_runtime::component::idl;
use std::io::Read;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: cargo run --example component-idl -- <file.idl.json>");
        return ExitCode::from(2);
    };
    let limits = idl::Limits::default();
    let mut bytes = Vec::new();
    let read = std::fs::File::open(&path).and_then(|file| {
        file.take(limits.source_bytes as u64 + 1)
            .read_to_end(&mut bytes)
    });
    match read {
        Ok(_) => {}
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::FAILURE;
        }
    }
    match idl::check(SourceId(0), &bytes, limits) {
        Ok(checked) => {
            let d = checked.document();
            println!(
                "checked {}:{}@{}: {} interfaces, {} imports, {} exports",
                d.package.value.namespace(),
                d.package.value.package(),
                d.version.value,
                d.interfaces.len(),
                d.imports.len(),
                d.exports.len()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "{path}:{}..{}: {}: {:?}",
                error.span.start,
                error.span.end,
                error.code.as_str(),
                error.reason
            );
            if let Some(span) = error.related {
                eprintln!("related: {path}:{}..{}", span.start, span.end);
            }
            ExitCode::FAILURE
        }
    }
}
