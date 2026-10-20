//! Source locations come from the immutable, serialized MIR snapshots. MIR
//! source indices are deliberately distinct from diagnostic/editor identities.
use crate::debug::{NativeDebugFunction, NativeDebugRange};
use crate::program::FunctionCode;
use lang_mir::{SourceMapError, SourcePosition, SourceSpan, unit as m};
use object::write::Object;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceFault {
    UnknownSource,
    OutOfBounds,
    NotCharBoundary,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Source {
        span: m::Span,
        error: SourceFault,
    },
    Position {
        span: m::Span,
        error: SourceMapError,
    },
    Function(m::FunctionId),
    Symbol {
        function: m::FunctionId,
        symbol: String,
    },
    Range {
        function: m::FunctionId,
        start: u32,
        end: u32,
        size: usize,
    },
    Capacity,
    Resource(m::Error),
    Format,
    Backend(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native source debug metadata failed: {self:?}")
    }
}
impl std::error::Error for Error {}

struct Positions<'a> {
    source: &'a m::Source,
    lines: Vec<u32>,
    // End of each non-ASCII scalar and cumulative excess UTF-8 bytes.
    unicode: Vec<(u32, u32)>,
}
impl<'a> Positions<'a> {
    fn new(source: &'a m::Source, budget: &mut m::Budget) -> Result<Self, Error> {
        u32::try_from(source.text.len()).map_err(|_| Error::Capacity)?;
        budget
            .charge(source.text.len().checked_mul(3).ok_or(Error::Capacity)?)
            .map_err(Error::Resource)?;
        let line_count = source.text.bytes().filter(|&c| c == b'\n').count() + 1;
        let unicode_count = source.text.chars().filter(|c| !c.is_ascii()).count();
        let mut lines = budget.vector(line_count).map_err(Error::Resource)?;
        let mut unicode = budget.vector(unicode_count).map_err(Error::Resource)?;
        lines.push(0);
        let mut excess = 0;
        for (offset, c) in source.text.char_indices() {
            let end = u32::try_from(offset + c.len_utf8()).map_err(|_| Error::Capacity)?;
            if c == '\n' {
                lines.push(end);
            }
            if !c.is_ascii() {
                excess += c.len_utf8() as u32 - 1;
                unicode.push((end, excess));
            }
        }
        Ok(Self {
            source,
            lines,
            unicode,
        })
    }
    fn excess(&self, offset: u32) -> u32 {
        let count = self.unicode.partition_point(|&(end, _)| end <= offset);
        if count == 0 {
            0
        } else {
            self.unicode[count - 1].1
        }
    }
    fn position(&self, offset: u32, span: m::Span) -> Result<SourcePosition, Error> {
        if offset as usize > self.source.text.len() {
            return Err(Error::Source {
                span,
                error: SourceFault::OutOfBounds,
            });
        }
        if !self.source.text.is_char_boundary(offset as usize) {
            return Err(Error::Source {
                span,
                error: SourceFault::NotCharBoundary,
            });
        }
        let line = self.lines.partition_point(|&start| start <= offset) - 1;
        let start = self.lines[line];
        let column = (offset - start - (self.excess(offset) - self.excess(start)))
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        SourcePosition::new(
            u32::try_from(line + 1).map_err(|_| Error::Capacity)?,
            column,
        )
        .map_err(|error| Error::Position { span, error })
    }
    fn span(&self, span: m::Span) -> Result<SourceSpan, Error> {
        SourceSpan::new(
            self.position(span.start, span)?,
            self.position(span.end, span)?,
        )
        .map_err(|error| Error::Position { span, error })
    }
}

pub(crate) fn macho<'a>(
    object: &mut Object<'_>,
    functions: impl ExactSizeIterator<Item = &'a FunctionCode>,
    lookup: impl Fn(m::FunctionId) -> Option<(&'a str, m::Span)>,
    sources: &'a [m::Source],
    budget: &mut m::Budget,
) -> Result<(), Error> {
    if object.format() != object::BinaryFormat::MachO {
        return Err(Error::Format);
    }
    let mut positions = budget.vector(sources.len()).map_err(Error::Resource)?;
    for source in sources {
        positions.push(Positions::new(source, budget)?);
    }
    let get = |span: m::Span| {
        positions.get(span.source.0 as usize).ok_or(Error::Source {
            span,
            error: SourceFault::UnknownSource,
        })
    };
    let mut debug = budget.vector(functions.len()).map_err(Error::Resource)?;
    for code in functions {
        budget.charge(1).map_err(Error::Resource)?;
        if code.source_ranges.is_empty() {
            continue;
        }
        let (name, declaration) = lookup(code.function).ok_or(Error::Function(code.function))?;
        let file = get(declaration)?;
        // Conservatively reserve writer DIE/line instructions, relocations and
        // copied strings before constructing third-party writer tables.
        let bound = name
            .len()
            .checked_add(code.symbol.len())
            .and_then(|n| n.checked_add(file.source.path.len()))
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add(2048))
            .ok_or(Error::Capacity)?;
        budget.bytes(bound).map_err(Error::Resource)?;
        let symbol = object
            .symbol_id(code.symbol.as_bytes())
            .ok_or_else(|| Error::Symbol {
                function: code.function,
                symbol: code.symbol.clone(),
            })?;
        let mut ranges = budget
            .vector(code.source_ranges.len())
            .map_err(Error::Resource)?;
        for range in &code.source_ranges {
            budget.charge(1).map_err(Error::Resource)?;
            if range.start > range.end || range.end as usize > code.code_size {
                return Err(Error::Range {
                    function: code.function,
                    start: range.start,
                    end: range.end,
                    size: code.code_size,
                });
            }
            let source = get(range.span)?;
            let bound = source
                .source
                .path
                .len()
                .checked_mul(4)
                .and_then(|n| n.checked_add(1024))
                .ok_or(Error::Capacity)?;
            budget.bytes(bound).map_err(Error::Resource)?;
            ranges.push(NativeDebugRange {
                start: range.start,
                end: range.end,
                path: &source.source.path,
                span: source.span(range.span)?,
            });
        }
        debug.push(NativeDebugFunction {
            symbol,
            name,
            linkage_name: &code.symbol,
            path: &file.source.path,
            span: file.span(declaration)?,
            code_size: code.code_size,
            ranges,
        });
    }
    crate::debug::add_macho_units(object, &debug).map_err(Error::Backend)
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::write::{Symbol, SymbolSection};
    use object::{
        Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope,
    };

    fn source(path: &str, text: &str) -> m::Source {
        m::Source {
            path: path.into(),
            text: text.into(),
        }
    }
    fn span(source: u32, start: u32, end: u32) -> m::Span {
        m::Span {
            source: m::SourceId(source),
            start,
            end,
        }
    }
    fn object() -> Object<'static> {
        let mut object = Object::new(
            BinaryFormat::MachO,
            Architecture::Aarch64,
            Endianness::Little,
        );
        let text = object.add_section(b"__TEXT".to_vec(), b"__text".to_vec(), SectionKind::Text);
        object.append_section_data(text, &[0; 16], 4);
        object.add_symbol(Symbol {
            name: b"guest".to_vec(),
            value: 0,
            size: 16,
            kind: SymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: SymbolSection::Section(text),
            flags: SymbolFlags::None,
        });
        object
    }
    fn code(source_span: m::Span) -> FunctionCode {
        FunctionCode {
            function: m::FunctionId(0),
            symbol: "guest".into(),
            code_size: 16,
            source_ranges: vec![crate::program::SourceRange {
                start: 0,
                end: 16,
                span: source_span,
            }],
        }
    }

    #[test]
    fn utf8_scalar_columns_crlf_and_end_of_file_are_exact() {
        let source = source("unicode.lang", "é😀x\r\n文z");
        let positions = Positions::new(&source, &mut m::Budget::new(Default::default())).unwrap();
        for (offset, line, column) in [
            (0, 1, 1),
            (2, 1, 2),
            (6, 1, 3),
            (9, 2, 1),
            (12, 2, 2),
            (13, 2, 3),
        ] {
            let p = positions.position(offset, span(0, offset, offset)).unwrap();
            assert_eq!((p.line(), p.column()), (line, column));
        }
        assert!(matches!(
            positions.position(1, span(0, 1, 1)),
            Err(Error::Source {
                error: SourceFault::NotCharBoundary,
                ..
            })
        ));
        assert!(matches!(
            positions.position(14, span(0, 14, 14)),
            Err(Error::Source {
                error: SourceFault::OutOfBounds,
                ..
            })
        ));
    }

    #[test]
    fn unknown_snapshot_native_range_and_budget_are_rejected_before_debug_output() {
        let sources = [source("test.lang", "x")];
        let code = code(span(0, 0, 1));
        let mut image = object();
        assert!(matches!(
            macho(
                &mut image,
                std::iter::once(&code),
                |_| Some(("guest", span(9, 0, 1))),
                &sources,
                &mut m::Budget::new(Default::default())
            ),
            Err(Error::Source {
                error: SourceFault::UnknownSource,
                ..
            })
        ));
        let mut bad = code.clone();
        bad.source_ranges[0].end = 17;
        assert!(matches!(
            macho(
                &mut image,
                std::iter::once(&bad),
                |_| Some(("guest", span(0, 0, 1))),
                &sources,
                &mut m::Budget::new(Default::default())
            ),
            Err(Error::Range {
                end: 17,
                size: 16,
                ..
            })
        ));
        let limits = m::Limits {
            allocations: 0,
            ..Default::default()
        };
        assert!(matches!(
            macho(
                &mut image,
                std::iter::once(&code),
                |_| Some(("guest", span(0, 0, 1))),
                &sources,
                &mut m::Budget::new(limits)
            ),
            Err(Error::Resource(_))
        ));
        // Earlier failures must not attach partially written DWARF sections.
        let bytes = image.write().unwrap();
        use object::Object as _;
        let parsed = object::File::parse(bytes.as_slice()).unwrap();
        assert!(parsed.section_by_name("__debug_info").is_none());
    }

    #[test]
    fn one_function_can_retain_ranges_from_multiple_source_snapshots() {
        let sources = [
            source("root.lang", "a"),
            source("lib/helper.lang", "// é\n文"),
        ];
        let mut code = code(span(0, 0, 1));
        code.source_ranges[0].end = 8;
        code.source_ranges.push(crate::program::SourceRange {
            start: 8,
            end: 16,
            span: span(1, 6, 9),
        });
        let mut image = object();
        macho(
            &mut image,
            std::iter::once(&code),
            |_| Some(("guest", span(0, 0, 1))),
            &sources,
            &mut m::Budget::new(Default::default()),
        )
        .unwrap();
        let bytes = image.write().unwrap();
        use object::{Object as _, ObjectSection as _};
        let parsed = object::File::parse(bytes.as_slice()).unwrap();
        let lines = parsed
            .section_by_name("__debug_line")
            .unwrap()
            .data()
            .unwrap();
        assert!(lines.windows(b"root.lang".len()).any(|s| s == b"root.lang"));
        assert!(
            lines
                .windows(b"lib/helper.lang".len())
                .any(|s| s == b"lib/helper.lang")
        );
    }
}
