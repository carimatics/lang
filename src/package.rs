//! Host-only package metadata; parsing never follows dependency paths.
pub mod archive;
pub mod cli;
pub mod command;
pub mod compatibility;
pub mod doc;
pub mod editor;
pub mod format;
pub mod graph;
pub mod lint;
pub mod lock;
pub mod manifest;
mod native;
mod output;
mod scaffold;
pub mod source;

use serde::Serialize;
use std::fmt::{Display, Formatter};

/// RFC 0034 schema 1; byte spans always address the original named input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub schema: u32,
    pub kind: Kind,
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub path: Option<String>,
    pub span: Option<ByteSpan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Diagnostic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            schema: 1,
            kind: Kind::Diagnostic,
            code,
            severity: Severity::Error,
            message: message.into(),
            path: None,
            span: None,
        }
    }

    pub fn at_path(mut self, path: &std::path::Path) -> Self {
        self.path = Some(path.to_string_lossy().into_owned());
        self
    }
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(path) = &self.path {
            write!(f, " ({path})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}
