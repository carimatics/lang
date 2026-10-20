use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn files(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("production source directory") {
        let entry = entry.expect("source entry");
        let ty = entry.file_type().expect("source type");
        assert!(
            !ty.is_symlink(),
            "SDK production inputs must not be symlinks"
        );
        if ty.is_dir() {
            files(&entry.path(), out);
        } else if ty.is_file() {
            out.push(entry.path());
        }
    }
}
fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.parent().unwrap().parent().unwrap();
    println!("cargo:rerun-if-changed={}", root.join("src").display());
    println!("cargo:rerun-if-changed={}", root.join("crates").display());
    let mut inputs = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        root.join("rust-toolchain.toml"),
    ];
    files(&root.join("src"), &mut inputs);
    for entry in fs::read_dir(root.join("crates")).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_dir() {
            continue;
        }
        inputs.push(path.join("Cargo.toml"));
        if path.join("build.rs").exists() {
            inputs.push(path.join("build.rs"));
        }
        if path.join("src").is_dir() {
            files(&path.join("src"), &mut inputs);
        }
    }
    inputs.sort();
    let rustc = Command::new(env::var_os("RUSTC").unwrap())
        .arg("--version")
        .arg("--verbose")
        .output()
        .unwrap();
    assert!(rustc.status.success(), "query SDK rustc");
    let rustc = String::from_utf8(rustc.stdout).unwrap();
    let field = |prefix: &str| rustc.lines().find_map(|s| s.strip_prefix(prefix)).unwrap();
    let release = field("release: ");
    let commit = field("commit-hash: ");
    let profile = env::var("PROFILE").unwrap();
    let flags = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let opt_level = env::var("OPT_LEVEL").unwrap();
    let debug = env::var("DEBUG").unwrap();
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    let mut hash = Sha256::new();
    hash.update(b"LANGSDK1\0");
    let mut total = 0usize;
    for path in inputs {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let bytes = fs::read(&path).expect("SDK production input");
        total = total.checked_add(bytes.len()).unwrap();
        assert!(total <= 64 * 1024 * 1024, "SDK production input bound");
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    for value in [
        release,
        commit,
        &profile,
        &flags,
        &opt_level,
        &debug,
        "workspace-fixed",
        "portable1-4/native1/c1",
    ] {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    let seed = hash
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    println!("cargo:rustc-env=LANG_SDK_SEED={seed}");
    println!("cargo:rustc-env=LANG_SDK_RUSTC_RELEASE={release}");
    println!("cargo:rustc-env=LANG_SDK_RUSTC_COMMIT={commit}");
    println!("cargo:rustc-env=LANG_SDK_PROFILE={profile}");
}
