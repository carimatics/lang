//! Backend-independent source snapshots, diagnostics, and atomic fix previews.
//!
//! Offsets are UTF-8 byte offsets into immutable snapshots. A new editor revision
//! must be added as a new source, so an old diagnostic cannot edit newer text.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter, Write};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SOURCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(u64);

/// A half-open range in one immutable source snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    source: SourceId,
    start: usize,
    end: usize,
}

impl Span {
    pub const fn source(self) -> SourceId {
        self.source
    }

    pub fn range(self) -> Range<usize> {
        self.start..self.end
    }
}

/// Human-facing positions count Unicode scalar values, starting at one.
/// `utf16_column` counts UTF-16 code units, starting at zero, for IDE adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    pub utf16_column: usize,
}

#[derive(Debug)]
pub struct SourceFile {
    name: String,
    text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn position(&self, offset: usize) -> Result<Position, SourceError> {
        if offset > self.text.len() {
            return Err(SourceError::OutOfBounds);
        }
        if !self.text.is_char_boundary(offset) {
            return Err(SourceError::NotCharBoundary);
        }
        let line = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let prefix = &self.text[self.line_starts[line]..offset];
        Ok(Position {
            line: line + 1,
            column: prefix.chars().count() + 1,
            utf16_column: prefix.encode_utf16().count(),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    files: BTreeMap<SourceId, std::sync::Arc<SourceFile>>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<SourceId, SourceError> {
        let name = name.into();
        if name.is_empty() || name.contains('\0') {
            return Err(SourceError::InvalidName);
        }
        let text = text.into();
        // New snapshots have globally unique IDs. Map clones/includes share
        // existing immutable identities. Relaxed ordering provides uniqueness;
        // this counter does not publish source data.
        let source = SourceId(
            NEXT_SOURCE
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .map_err(|_| SourceError::SourceIdsExhausted)?,
        );
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(offset, _)| offset + 1))
            .collect();
        self.files.insert(
            source,
            std::sync::Arc::new(SourceFile {
                name,
                text,
                line_starts,
            }),
        );
        Ok(source)
    }

    pub fn file(&self, source: SourceId) -> Result<&SourceFile, SourceError> {
        self.files
            .get(&source)
            .map(std::sync::Arc::as_ref)
            .ok_or(SourceError::UnknownSource)
    }

    /// Immutable snapshots in source-identity order; display names grant no IO
    /// authority and need not be unique across revisions.
    pub fn iter(&self) -> impl Iterator<Item = (SourceId, &SourceFile)> {
        self.files.iter().map(|(id, file)| (*id, file.as_ref()))
    }

    /// Share existing immutable snapshots and their identities. New `add`
    /// operations still allocate globally unique IDs, including on clones.
    pub fn include(&mut self, other: &Self) {
        self.files
            .extend(other.files.iter().map(|(id, file)| (*id, file.clone())));
    }

    pub fn span(&self, source: SourceId, range: Range<usize>) -> Result<Span, SourceError> {
        let file = self.file(source)?;
        if range.start > range.end {
            return Err(SourceError::ReversedRange);
        }
        if range.end > file.text.len() {
            return Err(SourceError::OutOfBounds);
        }
        if !file.text.is_char_boundary(range.start) || !file.text.is_char_boundary(range.end) {
            return Err(SourceError::NotCharBoundary);
        }
        Ok(Span {
            source,
            start: range.start,
            end: range.end,
        })
    }

    pub fn slice(&self, span: Span) -> Result<&str, SourceError> {
        self.span(span.source, span.range())?;
        Ok(&self.file(span.source)?.text[span.range()])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceError {
    InvalidName,
    SourceIdsExhausted,
    UnknownSource,
    ReversedRange,
    OutOfBounds,
    NotCharBoundary,
    InvalidCode,
    EmptyFix,
    OverlappingEdits,
}

impl Display for SourceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidName => "source name is empty or contains NUL",
            Self::SourceIdsExhausted => "source snapshot identifiers exhausted",
            Self::UnknownSource => "source snapshot is not in this map",
            Self::ReversedRange => "source range end precedes its start",
            Self::OutOfBounds => "source offset exceeds the snapshot length",
            Self::NotCharBoundary => "source offset splits a UTF-8 character",
            Self::InvalidCode => {
                "diagnostic code requires uppercase letters followed by four digits"
            }
            Self::EmptyFix => "fix must contain at least one edit",
            Self::OverlappingEdits => "fix contains overlapping or ambiguous edits",
        })
    }
}

impl Error for SourceError {}

/// Codes are machine-readable identities; wording may evolve independently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticCode(&'static str);

impl DiagnosticCode {
    pub fn new(code: &'static str) -> Result<Self, SourceError> {
        let prefix_len = code.len().checked_sub(4).ok_or(SourceError::InvalidCode)?;
        let (prefix, digits) = code.as_bytes().split_at(prefix_len);
        if prefix.is_empty()
            || !prefix.iter().all(u8::is_ascii_uppercase)
            || !digits.iter().all(u8::is_ascii_digit)
        {
            return Err(SourceError::InvalidCode);
        }
        Ok(Self(code))
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Accepted identities from RFCs 0002/0006/0007. Implementation and stabilization are separate.
pub mod codes {
    use super::DiagnosticCode;

    pub const INTEGER_OUT_OF_RANGE: DiagnosticCode = DiagnosticCode("SRC0001");
    pub const UNKNOWN_NAME: DiagnosticCode = DiagnosticCode("SRC0002");
    pub const DUPLICATE_NAME: DiagnosticCode = DiagnosticCode("SRC0003");
    pub const IMMUTABLE_ASSIGNMENT: DiagnosticCode = DiagnosticCode("SRC0004");
    pub const EXPECTED_I64: DiagnosticCode = DiagnosticCode("SRC0005");
    pub const EXPECTED_BOOL: DiagnosticCode = DiagnosticCode("SRC0006");
    pub const MISSING_RETURN: DiagnosticCode = DiagnosticCode("SRC0007");
    pub const INVALID_TOKEN: DiagnosticCode = DiagnosticCode("SRC0008");
    pub const UNTERMINATED_COMMENT: DiagnosticCode = DiagnosticCode("SRC0009");
    pub const INVALID_ENCODING: DiagnosticCode = DiagnosticCode("SRC0010");
    pub const RESOURCE_LIMIT: DiagnosticCode = DiagnosticCode("SRC0011");
    pub const EXPECTED_SYNTAX: DiagnosticCode = DiagnosticCode("SRC0012");
    pub const MODULE_ERROR: DiagnosticCode = DiagnosticCode("SRC0013");
    pub const TYPE_MISMATCH: DiagnosticCode = DiagnosticCode("SRC0014");
    pub const INVALID_SHAPE: DiagnosticCode = DiagnosticCode("SRC0015");
    pub const NON_EXHAUSTIVE_MATCH: DiagnosticCode = DiagnosticCode("SRC0016");
    pub const UNREACHABLE_ARM: DiagnosticCode = DiagnosticCode("SRC0017");
    pub const INVALID_ENTRY: DiagnosticCode = DiagnosticCode("SRC0018");
    pub const UNREACHABLE_STATEMENT: DiagnosticCode = DiagnosticCode("SRC0019");
    pub const INVALID_OWNER_USE: DiagnosticCode = DiagnosticCode("SRC0020");
    pub const INVALID_LOAN: DiagnosticCode = DiagnosticCode("SRC0021");
    pub const REGION_ESCAPE: DiagnosticCode = DiagnosticCode("SRC0022");
    pub const INVALID_OWNERSHIP_TYPE: DiagnosticCode = DiagnosticCode("SRC0023");
    pub const UNSUPPORTED_SPECIALIZATION: DiagnosticCode = DiagnosticCode("SRC0024");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

impl Label {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Applicability {
    MachineApplicable,
    MaybeIncorrect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEdit {
    pub span: Span,
    pub replacement: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fix {
    pub message: String,
    pub applicability: Applicability,
    pub edits: Vec<TextEdit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceChange {
    pub source: SourceId,
    pub text: String,
}

impl Fix {
    fn grouped_edits(
        &self,
        sources: &SourceMap,
    ) -> Result<BTreeMap<SourceId, Vec<&TextEdit>>, SourceError> {
        if self.edits.is_empty() {
            return Err(SourceError::EmptyFix);
        }
        let mut groups: BTreeMap<SourceId, Vec<&TextEdit>> = BTreeMap::new();
        for edit in &self.edits {
            sources.slice(edit.span)?;
            groups.entry(edit.span.source).or_default().push(edit);
        }
        for edits in groups.values_mut() {
            edits.sort_by_key(|edit| (edit.span.start, edit.span.end));
            if edits.windows(2).any(|pair| {
                pair[0].span.end > pair[1].span.start || pair[0].span.start == pair[1].span.start
            }) {
                return Err(SourceError::OverlappingEdits);
            }
        }
        Ok(groups)
    }

    /// Validate every file first, then return a preview. Never mutates sources
    /// or disk, and never silently applies a speculative fix.
    pub fn preview(&self, sources: &SourceMap) -> Result<Vec<SourceChange>, SourceError> {
        self.grouped_edits(sources)?
            .into_iter()
            .map(|(source, edits)| {
                let mut text = sources.file(source)?.text.clone();
                for edit in edits.into_iter().rev() {
                    text.replace_range(edit.span.range(), &edit.replacement);
                }
                Ok(SourceChange { source, text })
            })
            .collect()
    }
}

/// Exactly one primary label is required. Secondary labels may name other files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub primary: Label,
    pub secondary: Vec<Label>,
    pub notes: Vec<String>,
    pub fixes: Vec<Fix>,
}

impl Diagnostic {
    pub fn new(
        code: DiagnosticCode,
        severity: Severity,
        message: impl Into<String>,
        primary: Label,
    ) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            primary,
            secondary: Vec::new(),
            notes: Vec::new(),
            fixes: Vec::new(),
        }
    }

    pub fn validate(&self, sources: &SourceMap) -> Result<(), SourceError> {
        for label in std::iter::once(&self.primary).chain(&self.secondary) {
            sources.slice(label.span)?;
        }
        for fix in &self.fixes {
            fix.grouped_edits(sources)?;
        }
        Ok(())
    }

    /// Deterministic plain text. Escape controls in untrusted names/messages so
    /// they cannot inject terminal commands or additional diagnostic lines.
    pub fn render(&self, sources: &SourceMap) -> Result<String, SourceError> {
        self.validate(sources)?;
        let mut output = format!(
            "{}[{}]: {}\n",
            self.severity.as_str(),
            self.code.as_str(),
            escaped(&self.message)
        );
        for (kind, label) in std::iter::once(("primary", &self.primary))
            .chain(self.secondary.iter().map(|label| ("secondary", label)))
        {
            let file = sources.file(label.span.source)?;
            let start = file.position(label.span.start)?;
            let end = file.position(label.span.end)?;
            writeln!(
                output,
                "  --> {}:{}:{}..{}:{}: {kind}: {}",
                escaped(file.name()),
                start.line,
                start.column,
                end.line,
                end.column,
                escaped(&label.message)
            )
            .expect("writing a String cannot fail");
        }
        for note in &self.notes {
            writeln!(output, "  = note: {}", escaped(note)).expect("writing a String cannot fail");
        }
        for fix in &self.fixes {
            let applicability = match fix.applicability {
                Applicability::MachineApplicable => "machine-applicable",
                Applicability::MaybeIncorrect => "maybe-incorrect",
            };
            writeln!(
                output,
                "  = help ({applicability}): {}",
                escaped(&fix.message)
            )
            .expect("writing a String cannot fail");
        }
        Ok(output)
    }
}

fn escaped(text: &str) -> String {
    text.chars().flat_map(|character| {
        if character.is_control() || matches!(character, '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            character.escape_default().collect::<Vec<_>>()
        } else {
            vec![character]
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_positions_and_eof_use_immutable_snapshots() {
        let mut sources = SourceMap::new();
        let id = sources.add("unicode.lang", "a😀é\r\n次\n").unwrap();
        let file = sources.file(id).unwrap();
        assert_eq!(
            file.position(7).unwrap(),
            Position {
                line: 1,
                column: 4,
                utf16_column: 4
            }
        );
        assert_eq!(
            file.position(9).unwrap(),
            Position {
                line: 2,
                column: 1,
                utf16_column: 0
            }
        );
        assert_eq!(
            file.position(file.text().len()).unwrap(),
            Position {
                line: 3,
                column: 1,
                utf16_column: 0
            }
        );
        assert_eq!(file.position(2), Err(SourceError::NotCharBoundary));
        assert_eq!(sources.span(id, 0..2), Err(SourceError::NotCharBoundary));
        assert_eq!(sources.span(id, 0..100), Err(SourceError::OutOfBounds));
        let backwards = Range { start: 4, end: 1 };
        assert_eq!(sources.span(id, backwards), Err(SourceError::ReversedRange));
        let empty = sources.add("empty.lang", "").unwrap();
        assert!(sources.span(empty, 0..0).is_ok());
        let revision = sources.add("unicode.lang", "new revision").unwrap();
        assert_ne!(revision, id);
        assert_eq!(sources.file(id).unwrap().text(), "a😀é\r\n次\n");
        let other_map = SourceMap::new();
        assert_eq!(
            other_map.slice(sources.span(id, 0..1).unwrap()),
            Err(SourceError::UnknownSource)
        );
    }

    #[test]
    fn accepted_codes_and_code_validation() {
        let codes = [
            codes::INTEGER_OUT_OF_RANGE,
            codes::UNKNOWN_NAME,
            codes::DUPLICATE_NAME,
            codes::IMMUTABLE_ASSIGNMENT,
            codes::EXPECTED_I64,
            codes::EXPECTED_BOOL,
            codes::MISSING_RETURN,
        ];
        for (index, code) in codes.into_iter().enumerate() {
            assert_eq!(code.as_str(), format!("SRC{:04}", index + 1));
            assert_eq!(DiagnosticCode::new(code.as_str()), Ok(code));
        }
        for invalid in [
            "", "0001", "src0001", "SRC001", "SRC00001", "SRC00x1", "🦀0001",
        ] {
            assert_eq!(DiagnosticCode::new(invalid), Err(SourceError::InvalidCode));
        }
    }

    #[test]
    fn cross_file_labels_render_deterministically() {
        let mut sources = SourceMap::new();
        let first = sources.add("first.lang", "fn example(value: i64)").unwrap();
        let second = sources.add("second.lang", "value = 1;\n").unwrap();
        let mut diagnostic = Diagnostic::new(
            codes::IMMUTABLE_ASSIGNMENT,
            Severity::Error,
            "cannot assign to a parameter",
            Label::new(
                sources.span(second, 0..5).unwrap(),
                "immutable assignment target",
            ),
        );
        diagnostic.secondary.push(Label::new(
            sources.span(first, 11..16).unwrap(),
            "parameter declared here",
        ));
        diagnostic.notes.push("parameters are immutable".into());
        assert_eq!(
            diagnostic.render(&sources).unwrap(),
            "error[SRC0004]: cannot assign to a parameter\n  --> second.lang:1:1..1:6: primary: immutable assignment target\n  --> first.lang:1:12..1:17: secondary: parameter declared here\n  = note: parameters are immutable\n"
        );
        assert_eq!(diagnostic.primary.span.range(), 0..5);
        assert_eq!(
            diagnostic.render(&SourceMap::new()),
            Err(SourceError::UnknownSource)
        );
    }

    fn edit(
        sources: &SourceMap,
        source: SourceId,
        range: Range<usize>,
        replacement: &str,
    ) -> TextEdit {
        TextEdit {
            span: sources.span(source, range).unwrap(),
            replacement: replacement.into(),
        }
    }

    #[test]
    fn fixes_handle_insertions_deletions_unicode_and_multiple_files_atomically() {
        let mut sources = SourceMap::new();
        let first = sources.add("one.lang", "α = 1").unwrap();
        let second = sources.add("two.lang", "remove me").unwrap();
        let fix = Fix {
            message: "complete statements".into(),
            applicability: Applicability::MachineApplicable,
            edits: vec![
                edit(&sources, first, 6..6, ";"),
                edit(&sources, second, 0..7, ""),
                edit(&sources, first, 0..2, "beta"),
            ],
        };
        let changes = fix.preview(&sources).unwrap();
        assert_eq!(
            changes
                .iter()
                .find(|change| change.source == first)
                .unwrap()
                .text,
            "beta = 1;"
        );
        assert_eq!(
            changes
                .iter()
                .find(|change| change.source == second)
                .unwrap()
                .text,
            "me"
        );
        assert_eq!(sources.file(first).unwrap().text(), "α = 1");
        let mut missing = SourceMap::new();
        missing.add("one.lang", "α = 1").unwrap();
        assert_eq!(fix.preview(&missing), Err(SourceError::UnknownSource));
        for edits in [
            vec![],
            vec![
                edit(&sources, first, 0..2, "a"),
                edit(&sources, first, 0..0, "b"),
            ],
            vec![
                edit(&sources, first, 2..2, "a"),
                edit(&sources, first, 2..2, "b"),
            ],
            vec![
                edit(&sources, first, 0..4, "a"),
                edit(&sources, first, 2..5, "b"),
            ],
        ] {
            let expected = if edits.is_empty() {
                SourceError::EmptyFix
            } else {
                SourceError::OverlappingEdits
            };
            let invalid = Fix {
                edits,
                ..fix.clone()
            };
            assert_eq!(invalid.preview(&sources), Err(expected));
        }
    }

    #[test]
    fn rendering_escapes_untrusted_controls_and_marks_speculative_fixes() {
        let mut sources = SourceMap::new();
        let id = sources.add("bad\n\u{1b}[31m.lang", "").unwrap();
        let span = sources.span(id, 0..0).unwrap();
        let mut diagnostic = Diagnostic::new(
            codes::MISSING_RETURN,
            Severity::Error,
            "missing\nreturn\u{202e}",
            Label::new(span, "end of input"),
        );
        diagnostic.fixes.push(Fix {
            message: "insert a return".into(),
            applicability: Applicability::MaybeIncorrect,
            edits: vec![edit(&sources, id, 0..0, "return 0;")],
        });
        let rendered = diagnostic.render(&sources).unwrap();
        assert!(!rendered.contains('\u{1b}'));
        assert!(!rendered.contains('\u{202e}'));
        assert_eq!(rendered.lines().count(), 3);
        assert!(rendered.contains("help (maybe-incorrect)"));
        assert_eq!(sources.add("", ""), Err(SourceError::InvalidName));
        assert_eq!(sources.add("nul\0.lang", ""), Err(SourceError::InvalidName));
    }
}
