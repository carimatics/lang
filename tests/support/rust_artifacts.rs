//! In-tree manual-rustc fixtures must use one exact Cargo graph. Modification
//! times can mix abort/unwind, feature or profile variants from the same cache.
use std::{collections::BTreeMap, path::PathBuf, process::Command, sync::OnceLock};

pub fn library(name: &str) -> PathBuf {
    static LIBRARIES: OnceLock<BTreeMap<String, Vec<PathBuf>>> = OnceLock::new();
    let libraries = LIBRARIES.get_or_init(|| {
        let mut cargo = Command::new(env!("CARGO"));
        cargo.current_dir(env!("CARGO_MANIFEST_DIR")).args([
            "build",
            "--locked",
            "--offline",
            "--bin",
            "lang",
            "--message-format=json",
        ]);
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
        for line in output.stdout.split(|b| *b == b'\n') {
            if !line.starts_with(b"{") {
                continue;
            }
            let v: serde_json::Value = serde_json::from_slice(line).unwrap();
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
        libraries
    });
    let paths = libraries
        .get(name)
        .unwrap_or_else(|| panic!("missing Cargo library {name}"));
    assert_eq!(paths.len(), 1, "ambiguous Cargo library {name}: {paths:?}");
    paths[0].clone()
}
