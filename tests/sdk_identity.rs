//! Exercise the exact identity implementation with effective Cargo overrides.
use std::{collections::BTreeSet, fs, path::PathBuf, process::Command};

struct Probe(PathBuf);
impl Drop for Probe {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn compiled_identity_distinguishes_effective_cargo_profiles_and_package_overrides() {
    let probe =
        Probe(std::env::temp_dir().join(format!("lang sdk identity {}", std::process::id())));
    fs::create_dir_all(probe.0.join("identity/src")).unwrap();
    fs::create_dir(probe.0.join("src")).unwrap();
    fs::write(probe.0.join("Cargo.toml"), "[package]\nname='identity-probe'\nversion='0.0.0'\nedition='2024'\n[dependencies]\nidentity-unit={path='identity'}\n[workspace]\n").unwrap();
    fs::write(
        probe.0.join("identity/Cargo.toml"),
        "[package]\nname='identity-unit'\nversion='0.0.0'\nedition='2024'\n",
    )
    .unwrap();
    fs::write(
        probe.0.join("identity/src/implementation.rs"),
        include_str!("../crates/codegen-clif/src/sdk/identity.rs"),
    )
    .unwrap();
    fs::write(
        probe.0.join("identity/src/lib.rs"),
        "mod implementation; pub fn identity()->String {implementation::compiled_identity()}",
    )
    .unwrap();
    fs::write(
        probe.0.join("src/main.rs"),
        "fn main(){println!(\"{}\",identity_unit::identity());}",
    )
    .unwrap();
    let run = |scope: &str, debug: bool, overflow: bool, target: &str| {
        let output = Command::new(env!("CARGO"))
            .current_dir(&probe.0)
            .env("CARGO_TARGET_DIR", probe.0.join(target))
            .args([
                "run",
                "--quiet",
                "--offline",
                "--config",
                &format!("{scope}.debug-assertions={debug}"),
                "--config",
                &format!("{scope}.overflow-checks={overflow}"),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    for scope in ["profile.dev", "profile.dev.package.identity-unit"] {
        let mut identities = BTreeSet::new();
        for debug in [false, true] {
            for overflow in [false, true] {
                assert!(identities.insert(run(scope, debug, overflow, "target")));
            }
        }
        assert_eq!(
            run(scope, true, true, "target"),
            run(scope, true, true, "relocated")
        );
    }
}
