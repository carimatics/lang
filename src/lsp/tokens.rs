//! Full semantic tokens from one admitted package epoch, with lexical fallback.
use super::{Fault, Server, database_error, field, internal, string, uri};
use crate::package::editor::SymbolKind;
use lang_syntax::{TokenKind as K, lex};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub(super) const LEGEND: [&str; 12] = [
    "namespace",
    "type",
    "enumMember",
    "function",
    "parameter",
    "variable",
    "property",
    "keyword",
    "comment",
    "string",
    "number",
    "operator",
];

impl Server {
    pub(super) fn semantic_tokens(&mut self, params: &Value) -> Result<Value, Fault> {
        let name = string(field(params, "textDocument")?, "uri")?;
        let path = uri::path(name, self.limits.uri_bytes)
            .map_err(|_| super::invalid("document must use a local file URI"))?;
        let path = self
            .db
            .as_ref()
            .unwrap()
            .document_path(&path)
            .map_err(database_error)?;
        self.ensure_package();
        let view = self
            .package_view
            .as_ref()
            .unwrap()
            .as_ref()
            .map_err(internal)?;
        let (id, file) = view.source(&path).map_err(internal)?;
        if file.text().len() > self.limits.mapping.source_bytes {
            return Err(internal("semantic token source byte limit exceeded"));
        }
        let mut work = view.limits.work;
        let mut identities = BTreeMap::new();
        for occurrence in view.snapshot.occurrences() {
            charge(&mut work, 1)?;
            if occurrence.location.path != Path::new(file.name()) {
                continue;
            }
            let symbol = &view.snapshot.symbols()[occurrence.symbol.index()];
            let kind = match symbol.kind {
                SymbolKind::Module | SymbolKind::Import => 0,
                SymbolKind::Record | SymbolKind::Enum => 1,
                SymbolKind::EnumCase => 2,
                SymbolKind::Function | SymbolKind::Host => 3,
                SymbolKind::Parameter => 4,
                SymbolKind::Variable => 5,
                SymbolKind::Field => 6,
            };
            let span = occurrence.location.span;
            if identities
                .insert((span.start, span.end), kind)
                .is_some_and(|old| old != kind)
            {
                return Err(internal("ambiguous semantic token identity"));
            }
        }
        let tokens = lex(
            &view.snapshot.analysis().sources,
            id,
            view.limits.compiler.modules.parse.lex,
        )
        .map_err(internal)?
        .tokens;
        let mut encoded = Encoded::new(self.limits.mapping.output_bytes)?;
        let (mut line, mut character, mut after_cr) = (0u32, 0u32, false);
        for token in tokens {
            charge(&mut work, 1)?;
            let range = token.span.range();
            let kind = if token.kind == K::Identifier {
                identities.get(&(range.start, range.end)).copied()
            } else {
                lexical(token.kind)
            };
            let mut start = character;
            for c in file.text()[range].chars() {
                charge(&mut work, c.len_utf8())?;
                if c == '\r' || c == '\n' {
                    if let Some(kind) = kind {
                        encoded.push(line, start, character - start, kind)?;
                    }
                    if c == '\r' || !after_cr {
                        line = line
                            .checked_add(1)
                            .filter(|n| *n <= i32::MAX as u32)
                            .ok_or_else(|| internal("semantic token coordinate overflow"))?;
                    }
                    character = 0;
                    start = 0;
                    after_cr = c == '\r';
                } else {
                    character = character
                        .checked_add(c.len_utf16() as u32)
                        .filter(|n| *n <= i32::MAX as u32)
                        .ok_or_else(|| internal("semantic token coordinate overflow"))?;
                    after_cr = false;
                }
            }
            if let Some(kind) = kind {
                encoded.push(line, start, character - start, kind)?;
            }
        }
        Ok(json!({"resultId":view.snapshot.epoch().to_string(),"data":encoded.data}))
    }
}

fn lexical(kind: K) -> Option<u32> {
    Some(match kind {
        K::Whitespace
        | K::Invalid
        | K::Identifier
        | K::Wildcard
        | K::LeftParen
        | K::RightParen
        | K::LeftBrace
        | K::RightBrace
        | K::Colon
        | K::Semicolon
        | K::Comma
        | K::Dot => return None,
        K::I64 | K::Bool => 1,
        K::LineComment | K::BlockComment => 8,
        K::CAbi | K::ComponentAbi => 9,
        K::Integer(_) | K::IntegerOutOfRange => 10,
        K::Equal
        | K::Plus
        | K::LessThan
        | K::GreaterThan
        | K::Ampersand
        | K::Star
        | K::Arrow
        | K::ColonColon
        | K::FatArrow => 11,
        _ => 7,
    })
}
fn charge(work: &mut usize, amount: usize) -> Result<(), Fault> {
    *work = work
        .checked_sub(amount)
        .ok_or_else(|| internal("semantic token work limit exceeded"))?;
    Ok(())
}
struct Encoded {
    data: Vec<u32>,
    last: (u32, u32, u32),
    bytes: usize,
    limit: usize,
}
impl Encoded {
    fn new(limit: usize) -> Result<Self, Fault> {
        // The fixed envelope includes a decimal u64 epoch and JSON punctuation.
        if limit < 48 {
            return Err(internal("semantic token response byte limit exceeded"));
        }
        Ok(Self {
            data: Vec::new(),
            last: (0, 0, 0),
            bytes: 48,
            limit,
        })
    }
    fn push(&mut self, line: u32, start: u32, length: u32, kind: u32) -> Result<(), Fault> {
        if length == 0 {
            return Ok(());
        }
        if line < self.last.0 || (line == self.last.0 && start < self.last.2) {
            return Err(internal("overlapping semantic tokens"));
        }
        let delta_line = line - self.last.0;
        let delta_start = if delta_line == 0 {
            start - self.last.1
        } else {
            start
        };
        let values = [delta_line, delta_start, length, kind, 0];
        for value in values {
            self.bytes = self
                .bytes
                .checked_add(if value == 0 {
                    2
                } else {
                    value.ilog10() as usize + 2
                })
                .filter(|n| *n <= self.limit)
                .ok_or_else(|| internal("semantic token response byte limit exceeded"))?;
        }
        self.data.extend(values);
        self.last = (
            line,
            start,
            start
                .checked_add(length)
                .ok_or_else(|| internal("semantic token coordinate overflow"))?,
        );
        Ok(())
    }
}
