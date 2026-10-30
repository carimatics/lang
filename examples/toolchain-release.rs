//! Trusted release-author tool. Private signing input is accepted only on stdin.
use lang::toolchain::{Channel, MAX_ROOT_BYTES, distribution};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const USAGE: &str = "toolchain-release --sdk DIR --output NEW_DIR --trust-root FILE --channel stable|beta|nightly --version X.Y.Z --sequence N --issued UNIX_SECONDS --expires UNIX_SECONDS\nSigning material: bounded schema-1 JSON on stdin only; see RFC 0042.\n";
fn root_bytes(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use cap_std::fs::{Dir, OpenOptions};
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().ok_or("trust root requires a file path")?;
    let dir = Dir::open_ambient_dir(parent, cap_std::ambient_authority())?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = dir.open_with(name, &options)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_ROOT_BYTES as u64 {
        return Err("invalid trust root input type or size".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_ROOT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_ROOT_BYTES {
        return Err("trust root byte limit exceeded".into());
    }
    Ok(bytes)
}
fn run(args: Vec<OsString>) -> Result<distribution::Report, Box<dyn std::error::Error>> {
    let mut values = BTreeMap::new();
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let flag = flag.to_str().ok_or("invalid release option")?;
        if !matches!(
            flag,
            "--sdk"
                | "--output"
                | "--trust-root"
                | "--channel"
                | "--version"
                | "--sequence"
                | "--issued"
                | "--expires"
        ) {
            return Err("unknown release option".into());
        }
        let value = args
            .next()
            .filter(|v| !v.is_empty())
            .ok_or("release option requires a value")?;
        if values.insert(flag, value).is_some() {
            return Err("duplicate release option".into());
        }
    }
    let required = |key| {
        values
            .get(key)
            .copied()
            .ok_or("missing required release option")
    };
    let text = |key| required(key)?.to_str().ok_or("invalid release option text");
    let sdk = PathBuf::from(required("--sdk")?);
    let output = PathBuf::from(required("--output")?);
    let root = root_bytes(Path::new(required("--trust-root")?))?;
    let channel = match text("--channel")? {
        "stable" => Channel::Stable,
        "beta" => Channel::Beta,
        "nightly" => Channel::Nightly,
        _ => return Err("invalid release channel".into()),
    };
    let version = text("--version")?;
    let sequence = text("--sequence")?.parse()?;
    let issued = text("--issued")?.parse()?;
    let expires = text("--expires")?.parse()?;
    let mut input = Vec::new();
    io::stdin()
        .lock()
        .take(distribution::MAX_SIGNING_INPUT as u64 + 1)
        .read_to_end(&mut input)?;
    let keys = distribution::signing_keys(&input)?;
    Ok(distribution::produce(distribution::Request {
        sdk: &sdk,
        output: &output,
        trust_root: &root,
        channel,
        version,
        sequence,
        issued,
        expires,
        signing_keys: &keys,
    })?)
}
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        print!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    let result = run(args).and_then(|report| {
        let mut output = io::stdout().lock();
        serde_json::to_writer(&mut output, &report)?;
        writeln!(output)?;
        Ok(())
    });
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
