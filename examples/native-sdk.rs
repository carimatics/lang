//! Repository build tool, invoked with artifacts from the same Cargo build.
use std::{fs, io::Read, path::PathBuf};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let (mut artifacts, mut output) = (None, None);
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or("expected --artifacts FILE --output SDK")?;
        let slot = if option == "--artifacts" {
            &mut artifacts
        } else if option == "--output" {
            &mut output
        } else {
            return Err("unknown SDK builder option".into());
        };
        if slot.replace(PathBuf::from(value)).is_some() {
            return Err("duplicate SDK builder option".into());
        }
    }
    let artifacts = artifacts.ok_or("--artifacts FILE is required")?;
    let output = output.ok_or("--output SDK is required")?;
    let mut bytes = Vec::new();
    fs::File::open(artifacts)?
        .take(lang::native::sdk_package::CARGO_OUTPUT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    let manifest = lang::native::sdk_package::assemble(&bytes, &output, Default::default())?;
    println!(
        "SDK {}: {} components, build {}",
        output.display(),
        manifest.files().len(),
        manifest.build_id()
    );
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
