//! RFC 0038 bounded, detached static API documentation.
use super::Diagnostic;
use lang_diagnostics::{SourceMap, Span};
use lang_hir::resolved::{self as r, DefinitionKind, Type, TypeConstructor};
use lang_semantics::analysis::Program;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: super::output::MAX_OUTPUT,
            work: 4_000_000,
        }
    }
}
fn limit() -> Diagnostic {
    Diagnostic::error("PKG0004", "documentation rendering limit exceeded")
}
fn source_error(e: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("CLI0002", e.to_string())
}
struct Html {
    text: String,
    limits: Limits,
    work: usize,
}
impl Html {
    fn step(&mut self) -> Result<(), Diagnostic> {
        self.work = self
            .work
            .checked_add(1)
            .filter(|n| *n <= self.limits.work)
            .ok_or_else(limit)?;
        Ok(())
    }
    fn raw(&mut self, text: &str) -> Result<(), Diagnostic> {
        self.text
            .len()
            .checked_add(text.len())
            .filter(|n| *n <= self.limits.bytes)
            .ok_or_else(limit)?;
        self.text.push_str(text);
        Ok(())
    }
    fn escaped(&mut self, text: &str) -> Result<(), Diagnostic> {
        for c in text.chars() {
            self.step()?;
            match c {
                '&' => self.raw("&amp;")?,
                '<' => self.raw("&lt;")?,
                '>' => self.raw("&gt;")?,
                '"' => self.raw("&quot;")?,
                '\'' => self.raw("&#39;")?,
                _ => self.raw(c.encode_utf8(&mut [0; 4]))?,
            }
        }
        Ok(())
    }
}
struct Item<'a> {
    name: &'a str,
    anchor: String,
    span: Span,
    types: Vec<Type>,
}
/// Render only a successfully checked program; all source slices are validated.
pub fn render(
    program: &Program,
    sources: &SourceMap,
    limits: Limits,
) -> Result<String, Diagnostic> {
    let program = program.source();
    let mut h = Html {
        text: String::new(),
        limits,
        work: 0,
    };
    let mut items = Vec::new();
    for (index, definition) in program.definitions.iter().enumerate() {
        h.step()?;
        if !definition.public {
            continue;
        }
        let types = match &definition.kind {
            DefinitionKind::Record(fields) => {
                fields.iter().map(|f| f.value_type.value_type).collect()
            }
            DefinitionKind::Enum(cases) => cases
                .iter()
                .flat_map(|c| c.payload.iter().map(|t| t.value_type))
                .collect(),
        };
        items.push(Item {
            name: &definition.qualified_name,
            anchor: format!("t-{index}"),
            span: definition.span,
            types,
        });
    }
    for (index, function) in program.functions.iter().enumerate() {
        h.step()?;
        if !function.public {
            continue;
        }
        let mut types = vec![function.result.value_type];
        types.extend(
            function
                .parameters
                .iter()
                .filter_map(|p| function.locals[p.0].annotation.map(|t| t.value_type)),
        );
        items.push(Item {
            name: &function.qualified_name,
            anchor: format!("f-{index}"),
            span: function.signature_span,
            types,
        });
    }
    items.sort_by(|a, b| (a.name, &a.anchor).cmp(&(b.name, &b.anchor)));
    let mut files: Vec<_> = sources.iter().collect();
    files.sort_by(|a, b| a.1.name().cmp(b.1.name()));
    let source_indices: BTreeMap<_, _> = files
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    h.raw("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\"><title>Package API reference</title><style>body{font:16px system-ui;max-width:72rem;margin:2rem auto;padding:0 1rem;line-height:1.6}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#f5f5f5;padding:1rem}section{margin:3rem 0}a{color:#154ec1}nav ul{columns:2}code{font-family:monospace}</style></head><body><h1>Package API reference</h1><nav aria-label=\"Public API\"><ul>")?;
    for item in &items {
        h.step()?;
        h.raw(&format!("<li><a href=\"#{}\">", item.anchor))?;
        h.escaped(item.name)?;
        h.raw("</a></li>")?;
    }
    h.raw("</ul></nav><main>")?;
    for item in &items {
        h.step()?;
        h.raw(&format!("<section id=\"{}\"><h2>", item.anchor))?;
        h.escaped(item.name)?;
        h.raw("</h2>")?;
        let file = sources.file(item.span.source()).map_err(source_error)?;
        let signature = sources.slice(item.span).map_err(source_error)?;
        let prefix = &file.text()[..item.span.range().start];
        let before = &prefix[..prefix.rfind('\n').map_or(0, |i| i + 1)];
        let comments: Vec<_> = before
            .lines()
            .rev()
            .map(str::trim_start)
            .take_while(|l| l.starts_with("///"))
            .collect();
        if !comments.is_empty() {
            h.raw("<pre class=\"documentation\">")?;
            for line in comments.iter().rev() {
                h.escaped(line[3..].strip_prefix(' ').unwrap_or(&line[3..]))?;
                h.raw("\n")?;
            }
            h.raw("</pre>")?;
        }
        h.raw("<pre class=\"signature\">")?;
        h.escaped(signature)?;
        h.raw("</pre><p>")?;
        let source_index = source_indices[&item.span.source()];
        h.raw(&format!(
            "<a href=\"#s-{source_index}\" data-start=\"{}\" data-end=\"{}\">Source: ",
            item.span.range().start,
            item.span.range().end
        ))?;
        h.escaped(file.name())?;
        h.raw(&format!(
            " [{}..{}]</a></p>",
            item.span.range().start,
            item.span.range().end
        ))?;
        let references = type_references(&mut h, program, &item.types)?;
        if !references.is_empty() {
            h.raw("<p>Referenced types:</p><ul>")?;
            for index in references {
                h.raw(&format!("<li><a href=\"#t-{index}\">"))?;
                h.escaped(&program.definitions[index].qualified_name)?;
                h.raw("</a></li>")?;
            }
            h.raw("</ul>")?;
        }
        h.raw("</section>")?;
    }
    h.raw(
        "</main><h2>Original sources</h2><p>These panels include private implementation text.</p>",
    )?;
    for (index, (_, file)) in files.iter().enumerate() {
        h.step()?;
        h.raw(&format!("<section id=\"s-{index}\"><h3>"))?;
        h.escaped(file.name())?;
        h.raw("</h3><pre>")?;
        h.escaped(file.text())?;
        h.raw("</pre></section>")?;
    }
    h.raw("</body></html>\n")?;
    Ok(h.text)
}
fn type_references(
    h: &mut Html,
    program: &r::Program,
    roots: &[Type],
) -> Result<BTreeSet<usize>, Diagnostic> {
    let mut pending = roots.to_vec();
    let mut seen = BTreeSet::new();
    let mut definitions = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        h.step()?;
        match ty {
            Type::Nominal(id) => {
                if program.definitions[id.0].public {
                    definitions.insert(id.0);
                }
            }
            Type::Constructed(id) if seen.insert(id.0) => {
                let constructor = program.types[id.0].constructor;
                if let TypeConstructor::ComponentResource(id) = constructor {
                    pending.push(Type::Nominal(id));
                }
                pending.extend(constructor.arguments());
            }
            _ => {}
        }
    }
    Ok(definitions)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaped_output_checks_exact_byte_and_work_limits_before_appending() {
        let mut h = Html {
            text: String::new(),
            limits: Limits { bytes: 9, work: 2 },
            work: 0,
        };
        h.escaped("<&").unwrap();
        assert_eq!(h.text, "&lt;&amp;");
        assert_eq!(h.escaped("x").unwrap_err().code, "PKG0004");
        assert_eq!(h.text, "&lt;&amp;");
        let mut h = Html {
            text: String::new(),
            limits: Limits { bytes: 8, work: 2 },
            work: 0,
        };
        assert_eq!(h.escaped("<&").unwrap_err().code, "PKG0004");
        assert_eq!(h.text, "&lt;");
    }
}
