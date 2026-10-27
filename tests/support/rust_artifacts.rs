//! In-tree manual-rustc fixtures must use one exact Cargo graph. Modification
//! times can mix abort/unwind, feature or profile variants from the same cache.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::Command,
    sync::OnceLock,
};

struct Artifacts {
    libraries: BTreeMap<String, Vec<PathBuf>>,
    native_paths: BTreeSet<PathBuf>,
}

fn artifacts() -> &'static Artifacts {
    static ARTIFACTS: OnceLock<Artifacts> = OnceLock::new();
    ARTIFACTS.get_or_init(|| {
        let mut cargo = Command::new(env!("CARGO"));
        cargo.current_dir(env!("CARGO_MANIFEST_DIR")).args([
            "build",
            "--locked",
            "--offline",
            "--lib",
            "--message-format=json",
        ]);
        // This query needs rlibs only. Rebuilding the CLI here can transiently
        // unlink CARGO_BIN_EXE_lang while parallel native tests execute it.
        if lang_codegen_clif::sdk::BUILD_PROFILE == "release" {
            cargo.arg("--release");
        }
        let output = cargo.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut libraries: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        let mut native_paths = BTreeSet::new();
        for line in output.stdout.split(|b| *b == b'\n') {
            if !line.starts_with(b"{") {
                continue;
            }
            let v: serde_json::Value = serde_json::from_slice(line).unwrap();
            if v["reason"] == "build-script-executed" {
                for path in v["linked_paths"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str)
                {
                    let path = match path.split_once('=') {
                        Some(("native" | "all", path)) => path,
                        Some(("dependency" | "crate" | "framework", _)) => continue,
                        _ => path,
                    };
                    native_paths.insert(PathBuf::from(path));
                }
                continue;
            }
            if v["reason"] != "compiler-artifact" || v["profile"]["test"] == true {
                continue;
            }
            for file in v["filenames"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(serde_json::Value::as_str)
            {
                let path = PathBuf::from(file);
                if path.extension().is_some_and(|s| s == "rlib") {
                    let paths = libraries
                        .entry(v["target"]["name"].as_str().unwrap().to_owned())
                        .or_default();
                    if !paths.contains(&path) {
                        paths.push(path);
                    }
                }
            }
        }
        Artifacts {
            libraries,
            native_paths,
        }
    })
}

pub fn library(name: &str) -> PathBuf {
    let paths = artifacts()
        .libraries
        .get(name)
        .unwrap_or_else(|| panic!("missing Cargo library {name}"));
    assert_eq!(paths.len(), 1, "ambiguous Cargo library {name}: {paths:?}");
    paths[0].clone()
}

/// Preserve build-script native dependencies from the same graph as the rlibs.
pub fn native_search_paths(command: &mut Command) {
    for path in &artifacts().native_paths {
        command.arg("-L").arg(format!("native={}", path.display()));
    }
}
