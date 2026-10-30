//! RFC 0044 migration-only scalar profile; no filesystem or execution authority.
mod proof;
mod reader;

use super::{ByteSpan, Diagnostic};
use crate::frontend;
use lang_diagnostics::{SourceMap, Span};
use lang_hir::program::check_program;
use lang_syntax::{LexLimits, Token, TokenKind as K, add_source, lex};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub source_bytes: usize,
    pub tokens: usize,
    pub comment_depth: usize,
    pub depth: usize,
    pub nodes: usize,
    pub functions: usize,
    /// Reserved separately for each existing parser/module/binding/type pass.
    pub phase_work: usize,
    pub work: usize,
    pub analysis_bytes: usize,
    pub rewritten_bytes: usize,
    pub report_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            source_bytes: 1024 * 1024,
            tokens: 65_536,
            comment_depth: 64,
            depth: 128,
            nodes: 65_536,
            functions: 256,
            phase_work: 1_000_000,
            work: 16_000_000,
            analysis_bytes: 8 * 1024 * 1024,
            rewritten_bytes: 4 * 1024 * 1024,
            report_bytes: 8 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Edit {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub replacement: String,
    pub recipe: &'static str,
}
#[derive(Debug, Serialize)]
pub struct Preview {
    schema: u32,
    operation: &'static str,
    source_profile: &'static str,
    target_edition: &'static str,
    roots: Vec<PathBuf>,
    files: Vec<PathBuf>,
    names: BTreeMap<String, String>,
    edits: Vec<Edit>,
    changed: usize,
    applied: usize,
    applicability: &'static str,
    outcome: &'static str,
    #[serde(skip)]
    original: Vec<u8>,
    #[serde(skip)]
    replacement: String,
}
fn invalid(message: &str, span: Option<Span>) -> Diagnostic {
    let mut d = Diagnostic::error("MIG0001", message);
    d.span = span.map(|s| ByteSpan {
        start: s.range().start,
        end: s.range().end,
    });
    d
}
fn capacity() -> Diagnostic {
    Diagnostic::error(
        "MIG0002",
        "aggregate historical migration resource limit exceeded",
    )
}
struct Work {
    remaining: usize,
    analysis: usize,
}
impl Work {
    fn take(&mut self, amount: usize) -> Result<(), Diagnostic> {
        self.remaining = self.remaining.checked_sub(amount).ok_or_else(capacity)?;
        Ok(())
    }
    fn analyze(&mut self, amount: usize) -> Result<(), Diagnostic> {
        self.analysis = self.analysis.checked_sub(amount).ok_or_else(capacity)?;
        Ok(())
    }
}
impl Preview {
    pub fn capture(path: &Path, bytes: &[u8], limits: Limits) -> Result<Self, Diagnostic> {
        Self::checked(path, bytes, limits).map_err(|d| d.at_path(path))
    }
    fn checked(path: &Path, bytes: &[u8], limits: Limits) -> Result<Self, Diagnostic> {
        // A caller may lower the recursion bound, but cannot remove the stack
        // safety ceiling by setting an arbitrarily large embedding limit.
        let limits = Limits {
            depth: limits.depth.min(128),
            ..limits
        };
        let path_bytes = path.as_os_str().as_encoded_bytes().len();
        if bytes.len() > limits.source_bytes || path_bytes > 4096 {
            return Err(capacity());
        }
        let lex_limits = LexLimits {
            source_bytes: limits.source_bytes,
            tokens: limits.tokens,
            comment_depth: limits.comment_depth,
            diagnostics: 16,
        };
        let mut work = Work {
            remaining: limits.work,
            analysis: limits.analysis_bytes,
        };
        work.take(bytes.len())?;
        work.analyze(bytes.len())?;
        let mut sources = SourceMap::new();
        let source =
            add_source(&mut sources, path.to_string_lossy(), bytes, lex_limits).map_err(|_| {
                invalid(
                    "historical source must be bounded UTF-8 without a BOM",
                    None,
                )
            })?;
        let text = std::str::from_utf8(bytes).expect("validated UTF-8");
        let original_tokens = lex(&sources, source, lex_limits).map_err(|_| capacity())?;
        if let Some(d) = original_tokens.diagnostics.first() {
            return Err(invalid(
                "invalid historical token or comment",
                Some(d.primary.span),
            ));
        }
        let program = reader::read(
            &sources,
            source,
            text,
            &original_tokens.tokens,
            limits,
            &mut work,
        )?;
        frontend::check_script_entry(&program, &sources)
            .map_err(|_| invalid("historical entry must be fn main() -> i64", None))?;
        let mut compile = frontend::CompileLimits::default();
        compile.modules.parse.lex = LexLimits {
            source_bytes: limits.rewritten_bytes,
            ..lex_limits
        };
        compile.modules.parse.nodes = limits.nodes;
        compile.modules.parse.depth = limits.depth;
        compile.modules.parse.list_elements = limits.nodes;
        compile.modules.parse.work = limits.phase_work;
        compile.modules.total_source_bytes = limits.rewritten_bytes;
        compile.modules.modules = 1;
        compile.modules.functions = limits.functions;
        compile.modules.definitions = 0;
        compile.modules.imports = 0;
        compile.modules.work = limits.phase_work;
        compile.binding.work = limits.phase_work;
        compile.binding.locals = limits.nodes;
        compile.binding.expressions = limits.nodes;
        compile.binding.blocks = limits.nodes;
        compile.binding.depth = limits.depth;
        compile.typing.work = limits.phase_work;
        compile.typing.functions = limits.functions;
        compile.typing.locals = limits.nodes;
        compile.typing.expressions = limits.nodes;
        compile.typing.blocks = limits.nodes;
        compile.typing.depth = limits.depth;
        // check_program has an independently bounded task-profile classifier
        // before its scalar checker. Reserve both passes, even for Copy input.
        work.take(limits.phase_work.checked_mul(2).ok_or_else(capacity)?)?;
        work.analyze(bytes.len())?;
        let checked = check_program(program, &sources, compile.typing).map_err(|_| capacity())?;
        let old = checked.program.ok_or_else(|| {
            invalid(
                "historical scalar type or return-flow validation failed",
                checked.diagnostics.first().map(|d| d.primary.span),
            )
        })?;
        let names = fresh_names(text, &original_tokens.tokens, &mut work)?;
        let mut edits = Vec::new();
        let mut raw_report = limits.report_bytes;
        let mut length = bytes.len();
        for token in &original_tokens.tokens {
            if reader::historical(*token, text) != K::Identifier {
                continue;
            }
            let Some(replacement) = names.get(&text[token.span.range()]) else {
                continue;
            };
            let cost = path_bytes
                .checked_add(replacement.len())
                .ok_or_else(capacity)?;
            raw_report = raw_report.checked_sub(cost).ok_or_else(capacity)?;
            work.take(cost)?;
            let range = token.span.range();
            length = length
                .checked_sub(range.len())
                .and_then(|n| n.checked_add(replacement.len()))
                .filter(|n| *n <= limits.rewritten_bytes)
                .ok_or_else(capacity)?;
            edits.push(Edit {
                path: path.to_path_buf(),
                span: ByteSpan {
                    start: range.start,
                    end: range.end,
                },
                replacement: replacement.clone(),
                recipe: "rfc0006-reserved-identifier",
            });
        }
        if length > limits.rewritten_bytes {
            return Err(capacity());
        }
        work.take(length)?;
        let mut replacement = String::with_capacity(length);
        let mut cursor = 0;
        for edit in &edits {
            replacement.push_str(&text[cursor..edit.span.start]);
            replacement.push_str(&edit.replacement);
            cursor = edit.span.end;
        }
        replacement.push_str(&text[cursor..]);
        // Token identity and mapped byte boundaries prove that only the reported
        // complete identifier tokens changed, including all trivia boundaries.
        work.take(replacement.len())?;
        work.analyze(replacement.len())?;
        let mut next_sources = SourceMap::new();
        let next_source = add_source(
            &mut next_sources,
            path.to_string_lossy(),
            replacement.as_bytes(),
            compile.modules.parse.lex,
        )
        .map_err(|_| capacity())?;
        let next_tokens =
            lex(&next_sources, next_source, compile.modules.parse.lex).map_err(|_| capacity())?;
        verify_tokens(
            text,
            &original_tokens.tokens,
            &replacement,
            &next_tokens.tokens,
            &names,
            &mut work,
        )?;
        if !next_tokens.diagnostics.is_empty() {
            return Err(invalid("rewritten token validation failed", None));
        }
        for allowance in [
            compile.modules.parse.work,
            compile.modules.work,
            compile.binding.work,
            // The semantic facade and HIR checker each classify task syntax
            // independently, before the Copy checker uses its own allowance.
            compile.typing.work,
            compile.typing.work,
            compile.typing.work,
        ] {
            work.take(allowance)?;
        }
        work.analyze(replacement.len())?;
        let analysis = frontend::analyze_virtual(
            SourceMap::new(),
            path,
            replacement.as_bytes(),
            None,
            compile,
            None,
        );
        if analysis
            .diagnostics
            .iter()
            .any(|d| d.code == lang_diagnostics::codes::RESOURCE_LIMIT)
        {
            return Err(capacity());
        }
        let Ok(lang_semantics::analysis::Program::Copy(current)) = analysis.result else {
            return Err(invalid(
                "rewritten scalar source failed current analysis",
                None,
            ));
        };
        let before = proof::project(&old, &names, limits.depth, &mut work)?;
        let after = proof::project(&current, &BTreeMap::new(), limits.depth, &mut work)?;
        if before != after {
            return Err(invalid(
                "resolved scalar meaning changed during migration",
                None,
            ));
        }
        let result = Self {
            schema: 1,
            operation: "migrate",
            source_profile: "rfc0006-scalar",
            target_edition: "2026",
            roots: vec![path.to_path_buf()],
            files: vec![path.to_path_buf()],
            names,
            changed: usize::from(!edits.is_empty()),
            applied: 0,
            applicability: "checked",
            outcome: "preview",
            edits,
            original: bytes.to_vec(),
            replacement,
        };
        serde_json::to_writer(ReportLimit(limits.report_bytes), &result).map_err(|e| {
            if e.is_io() {
                capacity()
            } else {
                invalid("migration report path cannot be encoded", None)
            }
        })?;
        Ok(result)
    }
    pub fn original(&self) -> &[u8] {
        &self.original
    }
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }
}
fn fresh_names(
    text: &str,
    tokens: &[Token],
    work: &mut Work,
) -> Result<BTreeMap<String, String>, Diagnostic> {
    let mut occupied = BTreeSet::new();
    let mut conflicts = BTreeSet::new();
    work.take(text.len())?;
    for token in tokens {
        work.take(1)?;
        if reader::historical(*token, text) == K::Identifier {
            let name = &text[token.span.range()];
            occupied.insert(name.to_string());
            if token.kind != K::Identifier || name == "core" {
                conflicts.insert(name.to_string());
            }
        }
    }
    let mut result = BTreeMap::new();
    for name in conflicts {
        let mut suffix = 1usize;
        loop {
            work.take(name.len().checked_add(32).ok_or_else(capacity)?)?;
            let fresh = format!("_migrated_{name}_{suffix}");
            if occupied.insert(fresh.clone()) {
                result.insert(name, fresh);
                break;
            }
            suffix = suffix.checked_add(1).ok_or_else(capacity)?;
        }
    }
    Ok(result)
}
fn verify_tokens(
    old: &str,
    original: &[Token],
    new: &str,
    rewritten: &[Token],
    names: &BTreeMap<String, String>,
    work: &mut Work,
) -> Result<(), Diagnostic> {
    if original.len() != rewritten.len() {
        return Err(invalid("migration changed token structure", None));
    }
    let mut offset = 0usize;
    for (a, b) in original.iter().zip(rewritten) {
        work.take(1)?;
        let text = &old[a.span.range()];
        let expected = if reader::historical(*a, old) == K::Identifier {
            names.get(text).map_or(text, String::as_str)
        } else {
            text
        };
        work.take(expected.len())?;
        let end = offset.checked_add(expected.len()).ok_or_else(capacity)?;
        if b.span.range() != (offset..end)
            || &new[b.span.range()] != expected
            || reader::historical(*a, old) != reader::historical(*b, new)
        {
            return Err(invalid(
                "migration token or original-offset proof failed",
                Some(a.span),
            ));
        }
        offset = end;
    }
    if offset != new.len() {
        return Err(invalid("migration token coverage is incomplete", None));
    }
    Ok(())
}
struct ReportLimit(usize);
impl std::io::Write for ReportLimit {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("migration report byte limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
