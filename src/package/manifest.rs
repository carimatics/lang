//! Strict, bounded schema 1 parsing. Filesystem authority is a resolver concern.
use super::Diagnostic;
use serde::{Deserialize, Deserializer, de};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const MAX_BYTES: usize = 64 * 1024;
pub const MAX_ENTRIES: usize = 128;
pub const TARGETS: [&str; 2] = ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"];

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    #[serde(default, deserialize_with = "present")]
    pub package: Option<Package>,
    #[serde(default, deserialize_with = "present")]
    pub workspace: Option<Workspace>,
    #[serde(default, deserialize_with = "dependencies")]
    pub dependencies: BTreeMap<String, Dependency>,
    #[serde(default)]
    pub tests: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub entry: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub members: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub path: String,
    pub version: String,
    #[serde(default, deserialize_with = "present")]
    pub targets: Option<Vec<String>>,
}

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

// Derived structs reject repeated fields; maps need their own duplicate check.
fn dependencies<'de, D>(deserializer: D) -> Result<BTreeMap<String, Dependency>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, Dependency>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a dependency object with unique aliases")
        }
        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, Dependency>()? {
                if entries.insert(key, value).is_some() {
                    return Err(de::Error::custom("duplicate dependency alias"));
                }
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_map(Visitor)
}

fn invalid(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("PKG0003", message)
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

pub fn valid_version(version: &str) -> bool {
    let pieces: Vec<_> = version.split('.').collect();
    pieces.len() == 3
        && pieces.iter().all(|p| {
            !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p.len() == 1 || !p.starts_with('0'))
                && p.parse::<u32>().is_ok()
        })
}

fn valid_path(path: &str, parents: bool) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && (parents || p != ".."))
}

fn paths(paths: &[String], name: &str) -> Result<(), Diagnostic> {
    if paths.len() > MAX_ENTRIES {
        return Err(Diagnostic::error(
            "PKG0004",
            format!("{name} entry limit exceeded"),
        ));
    }
    let mut seen = BTreeSet::new();
    for path in paths {
        if !valid_path(path, false) || !seen.insert(path) {
            return Err(invalid(format!("invalid or duplicate {name} path: {path}")));
        }
    }
    Ok(())
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, Diagnostic> {
        if bytes.len() > MAX_BYTES {
            return Err(Diagnostic::error("PKG0004", "manifest exceeds 64 KiB"));
        }
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|e| Diagnostic::error("PKG0001", format!("invalid manifest JSON: {e}")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Reads at most the admitted byte limit plus one sentinel; no dependency IO.
    pub fn read(path: &Path) -> Result<Self, Diagnostic> {
        let read = || {
            let mut bytes = Vec::new();
            File::open(path)
                .and_then(|f| f.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes))
                .map_err(|e| Diagnostic::error("PKG0005", format!("cannot read manifest: {e}")))?;
            Self::parse(&bytes)
        };
        read().map_err(|e| e.at_path(path))
    }

    fn validate(&self) -> Result<(), Diagnostic> {
        if self.schema != 1 {
            return Err(Diagnostic::error(
                "PKG0002",
                "unsupported manifest schema; expected 1",
            ));
        }
        if self.package.is_none() && self.workspace.is_none() {
            return Err(invalid("manifest requires package or workspace"));
        }
        if let Some(package) = &self.package {
            if !valid_name(&package.name) || !valid_version(&package.version) {
                return Err(invalid("invalid package name or exact version"));
            }
            if package.edition != "2026" {
                return Err(Diagnostic::error(
                    "PKG0002",
                    "unsupported package edition; expected 2026",
                ));
            }
            if !valid_path(&package.entry, false) || !package.entry.ends_with(".lang") {
                return Err(invalid(
                    "package entry must be a relative .lang source path",
                ));
            }
        } else if !self.dependencies.is_empty() || !self.tests.is_empty() {
            return Err(invalid("dependencies and tests require a package"));
        }
        if let Some(workspace) = &self.workspace {
            if workspace.members.is_empty() {
                return Err(invalid("workspace members must not be empty"));
            }
            paths(&workspace.members, "workspace member")?;
        }
        paths(&self.tests, "test")?;
        if self.dependencies.len() > MAX_ENTRIES {
            return Err(Diagnostic::error(
                "PKG0004",
                "dependency entry limit exceeded",
            ));
        }
        if self.tests.iter().any(|p| !p.ends_with(".lang")) {
            return Err(invalid("tests must be .lang source paths"));
        }
        for (alias, dependency) in &self.dependencies {
            if !valid_name(alias) || !valid_version(&dependency.version) {
                return Err(invalid("invalid dependency alias or exact version"));
            }
            if !valid_path(&dependency.path, true) {
                return Err(invalid(format!("invalid dependency path for {alias}")));
            }
            if let Some(targets) = &dependency.targets {
                let unique: BTreeSet<_> = targets.iter().collect();
                if targets.is_empty()
                    || unique.len() != targets.len()
                    || targets.iter().any(|t| !TARGETS.contains(&t.as_str()))
                {
                    return Err(invalid(format!("invalid target condition for {alias}")));
                }
            }
        }
        Ok(())
    }
}
