//! Explain admitted input changes without promising a cache hit or build success.
mod command;
use crate::{
    native::manifest as native,
    package::{Diagnostic, lock::explain},
};
pub use command::run;
pub use explain::Category;
use lang_codegen_clif::sdk;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{self, Write},
};

fn error(message: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("PRF0004", message.to_string())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Rebuild,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Package,
    Native,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Native {
    pub sdk_manifest_sha256: String,
    pub sdk_manifest: String,
    pub rustc_release: String,
    pub rustc_commit: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub lock_sha256: String,
    pub lock: String,
    pub native: Option<Native>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub category: Category,
    /// Structured key components avoid ambiguity between nested package paths.
    pub key: Vec<String>,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema: u32,
    pub kind: Kind,
    pub scope: Scope,
    pub before: Option<Snapshot>,
    pub current: Snapshot,
    pub inputs_changed: bool,
    pub changes: Vec<Change>,
}

fn insert(
    facts: &mut explain::Facts,
    category: Category,
    key: &[&str],
    value: String,
) -> Result<(), Diagnostic> {
    if facts.len() == explain::MAX_FACTS
        || value.len() > 8192
        || value.contains('\0')
        || facts
            .insert((category, key.iter().map(|s| (*s).into()).collect()), value)
            .is_some()
    {
        return Err(error("duplicate or oversized rebuild input fact"));
    }
    Ok(())
}
impl Snapshot {
    pub fn capture(lock: &[u8], native: Option<Native>) -> Result<Self, Diagnostic> {
        if lock.len() > crate::package::lock::MAX_BYTES {
            return Err(error("rebuild lock input limit exceeded"));
        }
        let result = Self {
            lock_sha256: sdk::digest(lock),
            lock: std::str::from_utf8(lock)
                .map_err(|_| error("input lock is not UTF-8"))?
                .into(),
            native,
        };
        result.facts()?;
        Ok(result)
    }
    fn facts(&self) -> Result<explain::Facts, Diagnostic> {
        if self.lock.len() > crate::package::lock::MAX_BYTES
            || self.lock_sha256 != sdk::digest(self.lock.as_bytes())
        {
            return Err(error("invalid rebuild lock identity or byte limit"));
        }
        let input = explain::inspect(self.lock.as_bytes())?;
        let mut facts = input.facts;
        if let Some(record) = &self.native {
            if record.sdk_manifest.len() > 1024 * 1024
                || record.sdk_manifest_sha256 != sdk::digest(record.sdk_manifest.as_bytes())
            {
                return Err(error("invalid rebuild SDK identity or byte limit"));
            }
            let sdk = native::parse_sdk(record.sdk_manifest.as_bytes(), Default::default())
                .map_err(error)?;
            if sdk.target().text() != input.target
                || sdk.build_id().to_string() != input.compiler_build_id
                || sdk.rustc_release() != record.rustc_release
                || sdk.rustc_commit() != record.rustc_commit
            {
                return Err(error("rebuild native identities do not reconcile"));
            }
            for (key, value) in [
                ("manifest_sha256", record.sdk_manifest_sha256.clone()),
                ("schema", sdk.version().to_string()),
                ("target", sdk.target().text().into()),
                ("build_id", sdk.build_id().to_string()),
                ("build_profile", sdk.build_profile().into()),
                ("feature_policy", sdk.feature_policy().into()),
            ] {
                insert(&mut facts, Category::Sdk, &[key], value)?;
            }
            insert(
                &mut facts,
                Category::Rustc,
                &["release"],
                record.rustc_release.clone(),
            )?;
            insert(
                &mut facts,
                Category::Rustc,
                &["commit"],
                record.rustc_commit.clone(),
            )?;
            for file in sdk.files() {
                let role = match file.role() {
                    native::Role::Compiler => "compiler",
                    native::Role::Runtime => "runtime",
                    native::Role::Dependency => "dependency",
                    native::Role::Header => "header",
                    native::Role::Startup => "startup",
                    native::Role::Documentation => "documentation",
                    _ => return Err(error("invalid SDK input role")),
                };
                for (field, value) in [
                    ("role", role.into()),
                    ("bytes", file.bytes().to_string()),
                    ("sha256", file.hash().to_string()),
                ] {
                    insert(
                        &mut facts,
                        Category::Sdk,
                        &["file", file.path(), field],
                        value,
                    )?;
                }
            }
        }
        Ok(facts)
    }
}
fn differences(before: &explain::Facts, current: &explain::Facts) -> Vec<Change> {
    let keys: BTreeSet<_> = before.keys().chain(current.keys()).collect();
    keys.into_iter()
        .filter_map(|key| {
            let previous = before.get(key);
            let next = current.get(key);
            (previous != next).then(|| Change {
                category: key.0,
                key: key.1.clone(),
                before: previous.cloned(),
                after: next.cloned(),
            })
        })
        .collect()
}
impl Report {
    pub fn compare(before: Option<Snapshot>, current: Snapshot) -> Result<Self, Diagnostic> {
        let previous = before
            .as_ref()
            .map(Snapshot::facts)
            .transpose()?
            .unwrap_or_default();
        let next = current.facts()?;
        let changes = differences(&previous, &next);
        let report = Self {
            schema: 1,
            kind: Kind::Rebuild,
            scope: if current.native.is_some() {
                Scope::Native
            } else {
                Scope::Package
            },
            before,
            current,
            inputs_changed: !changes.is_empty(),
            changes,
        };
        report.validate()?;
        Ok(report)
    }
    pub fn validate(&self) -> Result<(), Diagnostic> {
        let native = self.scope == Scope::Native;
        if self.schema != 1
            || self.current.native.is_some() != native
            || self
                .before
                .as_ref()
                .is_some_and(|s| s.native.is_some() != native)
            || native && self.before.is_none()
            || self.changes.len() > 2 * explain::MAX_FACTS
        {
            return Err(error("unsupported rebuild report identity or scope"));
        }
        let previous = self
            .before
            .as_ref()
            .map(Snapshot::facts)
            .transpose()?
            .unwrap_or_default();
        let next = self.current.facts()?;
        if self.changes != differences(&previous, &next)
            || self.inputs_changed == self.changes.is_empty()
        {
            return Err(error(
                "rebuild changes do not reconcile with input snapshots",
            ));
        }
        Ok(())
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Diagnostic> {
        if bytes.len() > super::MAX_REPORT_BYTES {
            return Err(error("rebuild report input limit exceeded"));
        }
        let report: Self =
            serde_json::from_slice(bytes).map_err(|_| error("invalid rebuild report JSON"))?;
        report.validate()?;
        Ok(report)
    }
    pub fn encode(&self) -> Result<Vec<u8>, Diagnostic> {
        self.validate()?;
        struct Buffer(Vec<u8>);
        impl Write for Buffer {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.0.len().saturating_add(bytes.len()) > super::MAX_REPORT_BYTES {
                    return Err(io::Error::other("rebuild report output limit"));
                }
                self.0
                    .try_reserve_exact(bytes.len())
                    .map_err(io::Error::other)?;
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut buffer = Buffer(Vec::new());
        serde_json::to_writer(&mut buffer, self)
            .map_err(|_| error("cannot encode bounded rebuild report"))?;
        Ok(buffer.0)
    }
}
