//! Diagnostic data conversion for the LSP UTF-16 position profile.
//!
//! This is an adapter, not a language server or a wire codec. A future server
//! must supply document URIs, negotiate UTF-16, respect related-information
//! capabilities, and guard publication with the shared query graph.
//! Source: LSP 3.18 position/range/diagnostic/text-document definitions,
//! verified 2026-10-17; see docs/architecture.md for source dates and links.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_diagnostics::{
    Diagnostic as SourceDiagnostic, Severity as SourceSeverity, SourceError, SourceId, SourceMap,
    Span,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    line: u32,
    character: u32,
}

impl Position {
    pub const fn line(self) -> u32 {
        self.line
    }
    pub const fn character(self) -> u32 {
        self.character
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Information,
}

impl Severity {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Error => 1,
            Self::Warning => 2,
            Self::Information => 3,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelatedInformation {
    pub uri: String,
    pub range: Range,
    pub message: String,
}

/// Fields correspond to the LSP diagnostic data model. The primary document URI
/// belongs to the enclosing publication; related locations carry their own URI.
/// Fixes remain in the source diagnostic for a later code-action adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub code: String,
    pub source: String,
    pub message: String,
    pub related_information: Vec<RelatedInformation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappedDiagnostic {
    pub uri: String,
    pub diagnostic: Diagnostic,
}

/// Host-selected adapter bounds, not a new source-language or LSP wire profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub source_bytes: usize,
    pub labels: usize,
    pub notes: usize,
    pub output_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    SourceBytes,
    Labels,
    Notes,
    OutputBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MappingError {
    Source(SourceError),
    MissingUri(SourceId),
    CoordinateOverflow,
    LimitExceeded(LimitKind),
}

impl Display for MappingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "LSP diagnostic mapping failed: {self:?}")
    }
}

impl Error for MappingError {}

fn coordinate(value: usize) -> Result<u32, MappingError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value <= i32::MAX as u32)
        .ok_or(MappingError::CoordinateOverflow)
}

struct Line {
    start: usize,
    end: usize,
}

struct Index<'a> {
    text: &'a str,
    lines: Vec<Line>,
}

impl<'a> Index<'a> {
    fn new(text: &'a str) -> Result<Self, MappingError> {
        let mut lines = Vec::new();
        let mut start = 0;
        let mut offset = 0;
        let bytes = text.as_bytes();
        while offset < bytes.len() {
            let newline = match bytes[offset] {
                b'\r' if bytes.get(offset + 1) == Some(&b'\n') => 2,
                b'\r' | b'\n' => 1,
                _ => {
                    offset += 1;
                    continue;
                }
            };
            coordinate(lines.len())?;
            lines.push(Line { start, end: offset });
            offset += newline;
            start = offset;
        }
        coordinate(lines.len())?;
        lines.push(Line {
            start,
            end: bytes.len(),
        });
        Ok(Self { text, lines })
    }

    fn position(&self, offset: usize) -> Result<Position, MappingError> {
        if offset > self.text.len() {
            return Err(MappingError::Source(SourceError::OutOfBounds));
        }
        if !self.text.is_char_boundary(offset) {
            return Err(MappingError::Source(SourceError::NotCharBoundary));
        }
        let line = self.lines.partition_point(|line| line.start <= offset) - 1;
        let bounds = &self.lines[line];
        // A byte boundary inside CRLF has no distinct LSP position. Canonicalize
        // it to the preceding content's line end instead of counting the CR.
        let offset = offset.min(bounds.end);
        Ok(Position {
            line: coordinate(line)?,
            character: coordinate(self.text[bounds.start..offset].encode_utf16().count())?,
        })
    }

    fn range(&self, span: Span) -> Result<Range, MappingError> {
        let range = span.range();
        Ok(Range {
            start: self.position(range.start)?,
            end: self.position(range.end)?,
        })
    }
}

struct OutputBudget(usize);

impl OutputBudget {
    fn charge(&mut self, bytes: usize) -> Result<(), MappingError> {
        self.0 = self
            .0
            .checked_sub(bytes)
            .ok_or(MappingError::LimitExceeded(LimitKind::OutputBytes))?;
        Ok(())
    }

    fn copy(&mut self, text: &str) -> Result<String, MappingError> {
        self.charge(text.len())?;
        Ok(text.into())
    }

    fn append(&mut self, output: &mut String, text: &str) -> Result<(), MappingError> {
        self.charge(text.len())?;
        output.push_str(text);
        Ok(())
    }
}

/// Convert valid source labels using host-supplied document URIs. URIs are
/// opaque, already validated protocol inputs; no filesystem paths are inferred
/// or read. Related information is retained for capable clients; capability
/// filtering, publication versions, JSON encoding, and code actions come later.
pub fn map_diagnostic(
    diagnostic: &SourceDiagnostic,
    sources: &SourceMap,
    uris: &BTreeMap<SourceId, String>,
    limits: Limits,
) -> Result<MappedDiagnostic, MappingError> {
    if diagnostic.notes.len() > limits.notes {
        return Err(MappingError::LimitExceeded(LimitKind::Notes));
    }
    let label_count = diagnostic
        .secondary
        .len()
        .checked_add(1)
        .filter(|count| *count <= limits.labels)
        .ok_or(MappingError::LimitExceeded(LimitKind::Labels))?;
    let mut source_ids = BTreeSet::new();
    let mut bytes = 0_usize;
    for label in std::iter::once(&diagnostic.primary).chain(&diagnostic.secondary) {
        sources.slice(label.span).map_err(MappingError::Source)?;
        if !uris.contains_key(&label.span.source()) {
            return Err(MappingError::MissingUri(label.span.source()));
        }
        if source_ids.insert(label.span.source()) {
            bytes = bytes
                .checked_add(
                    sources
                        .file(label.span.source())
                        .map_err(MappingError::Source)?
                        .text()
                        .len(),
                )
                .filter(|bytes| *bytes <= limits.source_bytes)
                .ok_or(MappingError::LimitExceeded(LimitKind::SourceBytes))?;
        }
    }
    let indices = source_ids
        .into_iter()
        .map(|id| {
            Ok((
                id,
                Index::new(sources.file(id).map_err(MappingError::Source)?.text())?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, MappingError>>()?;
    let mut budget = OutputBudget(limits.output_bytes);
    let uri = budget.copy(&uris[&diagnostic.primary.span.source()])?;
    let code = budget.copy(diagnostic.code.as_str())?;
    let source = budget.copy("lang")?;
    let mut message = budget.copy(&diagnostic.message)?;
    if !diagnostic.primary.message.is_empty() && diagnostic.primary.message != diagnostic.message {
        budget.append(&mut message, "\n")?;
        budget.append(&mut message, &diagnostic.primary.message)?;
    }
    for note in &diagnostic.notes {
        // The independent note-count bound also covers empty notes.
        if note.is_empty() {
            continue;
        }
        budget.append(&mut message, "\nnote: ")?;
        budget.append(&mut message, note)?;
    }
    let mut related_information = Vec::with_capacity(label_count - 1);
    for label in &diagnostic.secondary {
        related_information.push(RelatedInformation {
            uri: budget.copy(&uris[&label.span.source()])?,
            range: indices[&label.span.source()].range(label.span)?,
            message: budget.copy(&label.message)?,
        });
    }
    let severity = match diagnostic.severity {
        SourceSeverity::Error => Severity::Error,
        SourceSeverity::Warning => Severity::Warning,
        SourceSeverity::Note => Severity::Information,
    };
    Ok(MappedDiagnostic {
        uri,
        diagnostic: Diagnostic {
            range: indices[&diagnostic.primary.span.source()].range(diagnostic.primary.span)?,
            severity,
            code,
            source,
            message,
            related_information,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_diagnostics::{Label, codes};

    const LIMITS: Limits = Limits {
        source_bytes: 100_000,
        labels: 100,
        notes: 100,
        output_bytes: 100_000,
    };

    fn position(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn utf16_positions_cover_all_protocol_line_endings_and_crlf_boundaries() {
        let text = "a😀\r\né\r次\nz";
        let index = Index::new(text).unwrap();
        for (offset, expected) in [
            (0, position(0, 0)),
            (1, position(0, 1)),
            (5, position(0, 3)),
            (6, position(0, 3)),
            (7, position(1, 0)),
            (9, position(1, 1)),
            (10, position(2, 0)),
            (13, position(2, 1)),
            (14, position(3, 0)),
            (15, position(3, 1)),
        ] {
            assert_eq!(index.position(offset), Ok(expected), "byte offset {offset}");
        }
        assert_eq!(
            index.position(2),
            Err(MappingError::Source(SourceError::NotCharBoundary))
        );
        assert_eq!(
            index.position(100),
            Err(MappingError::Source(SourceError::OutOfBounds))
        );
        assert_eq!(Index::new("").unwrap().position(0).unwrap(), position(0, 0));
        assert_eq!(
            Index::new("x\r\n").unwrap().position(3).unwrap(),
            position(1, 0)
        );
        assert_eq!(coordinate(i32::MAX as usize).unwrap(), i32::MAX as u32);
        assert_eq!(
            coordinate(i32::MAX as usize + 1),
            Err(MappingError::CoordinateOverflow)
        );
    }

    #[test]
    fn maps_cross_file_labels_codes_notes_and_severity_without_path_inference() {
        let mut sources = SourceMap::new();
        let first = sources.add("not-a-uri.lang", "😀value\r\n").unwrap();
        let second = sources.add("C:\\untrusted\\name.lang", "x\rvalue").unwrap();
        let mut diagnostic = SourceDiagnostic::new(
            codes::IMMUTABLE_ASSIGNMENT,
            SourceSeverity::Error,
            "cannot assign",
            Label::new(sources.span(first, 4..9).unwrap(), "immutable parameter"),
        );
        diagnostic.secondary.push(Label::new(
            sources.span(second, 2..7).unwrap(),
            "declared here",
        ));
        diagnostic.notes.push("use a mutable local".into());
        let uris = BTreeMap::from([
            (first, "untitled:buffer-1".into()),
            (second, "file:///C:/project/other.lang".into()),
        ]);
        let mapped = map_diagnostic(&diagnostic, &sources, &uris, LIMITS).unwrap();
        assert_eq!(mapped.uri, "untitled:buffer-1");
        assert_eq!(
            mapped.diagnostic.range,
            Range {
                start: position(0, 2),
                end: position(0, 7)
            }
        );
        assert_eq!(mapped.diagnostic.code, "SRC0004");
        assert_eq!(mapped.diagnostic.source, "lang");
        assert_eq!(
            mapped.diagnostic.message,
            "cannot assign\nimmutable parameter\nnote: use a mutable local"
        );
        assert_eq!(
            mapped.diagnostic.related_information[0],
            RelatedInformation {
                uri: "file:///C:/project/other.lang".into(),
                range: Range {
                    start: position(1, 0),
                    end: position(1, 5)
                },
                message: "declared here".into()
            }
        );
        for (severity, expected) in [
            (SourceSeverity::Error, 1),
            (SourceSeverity::Warning, 2),
            (SourceSeverity::Note, 3),
        ] {
            diagnostic.severity = severity;
            assert_eq!(
                map_diagnostic(&diagnostic, &sources, &uris, LIMITS)
                    .unwrap()
                    .diagnostic
                    .severity
                    .as_u8(),
                expected
            );
        }
    }

    #[test]
    fn eof_labels_are_zero_width_and_unknown_snapshots_and_uris_are_errors() {
        let mut sources = SourceMap::new();
        let id = sources.add("empty.lang", "").unwrap();
        let diagnostic = SourceDiagnostic::new(
            codes::MISSING_RETURN,
            SourceSeverity::Error,
            "missing return",
            Label::new(sources.span(id, 0..0).unwrap(), "missing return"),
        );
        let uris = BTreeMap::from([(id, "untitled:empty".into())]);
        let mapped = map_diagnostic(&diagnostic, &sources, &uris, LIMITS).unwrap();
        assert_eq!(
            mapped.diagnostic.range,
            Range {
                start: position(0, 0),
                end: position(0, 0)
            }
        );
        assert_eq!(mapped.diagnostic.message, "missing return");
        assert_eq!(
            map_diagnostic(&diagnostic, &sources, &BTreeMap::new(), LIMITS),
            Err(MappingError::MissingUri(id))
        );
        assert_eq!(
            map_diagnostic(&diagnostic, &SourceMap::new(), &uris, LIMITS),
            Err(MappingError::Source(SourceError::UnknownSource))
        );
    }

    #[test]
    fn bounds_cover_unique_source_bytes_all_labels_output_and_empty_notes() {
        let mut sources = SourceMap::new();
        let id = sources.add("input.lang", "value").unwrap();
        let span = sources.span(id, 0..5).unwrap();
        let mut diagnostic = SourceDiagnostic::new(
            codes::UNKNOWN_NAME,
            SourceSeverity::Error,
            "unknown name",
            Label::new(span, "unknown name"),
        );
        diagnostic.secondary.push(Label::new(span, "same source"));
        let uris = BTreeMap::from([(id, "untitled:input".into())]);
        // Repeated labels from one source charge its source text only once.
        assert!(
            map_diagnostic(
                &diagnostic,
                &sources,
                &uris,
                Limits {
                    source_bytes: 5,
                    ..LIMITS
                }
            )
            .is_ok()
        );
        for (limits, kind) in [
            (
                Limits {
                    source_bytes: 4,
                    ..LIMITS
                },
                LimitKind::SourceBytes,
            ),
            (
                Limits {
                    labels: 1,
                    ..LIMITS
                },
                LimitKind::Labels,
            ),
            (
                Limits {
                    output_bytes: 1,
                    ..LIMITS
                },
                LimitKind::OutputBytes,
            ),
        ] {
            assert_eq!(
                map_diagnostic(&diagnostic, &sources, &uris, limits),
                Err(MappingError::LimitExceeded(kind))
            );
        }
        diagnostic.notes = vec![String::new(); 100];
        assert_eq!(
            map_diagnostic(
                &diagnostic,
                &sources,
                &uris,
                Limits {
                    notes: 99,
                    ..LIMITS
                }
            ),
            Err(MappingError::LimitExceeded(LimitKind::Notes))
        );
    }
}
