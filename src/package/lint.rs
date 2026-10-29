//! RFC 0037 non-executing package lint over frozen frontend inputs.
use super::{ByteSpan, Diagnostic, Severity, graph::Graph, source};
use crate::frontend::{Analysis, AnalysisError, CompileLimits};
use lang_diagnostics::{SourceMap, Span};
use lang_hir::resolved::{BindingKind, ExpressionKind};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_DIAGNOSTICS: usize = 65_536;
type Key = (String, usize, usize, String, u8, String);
#[derive(Debug)]
pub struct Report {
    diagnostics: Vec<Diagnostic>,
    failed: bool,
}
impl Report {
    pub fn capture(graph: &Graph, limits: CompileLimits) -> Result<Self, Diagnostic> {
        let mut collector = Collector::default();
        collector.analysis(source::analyze(graph, limits)?)?;
        let root = graph.package(graph.root()).expect("resolved root");
        for index in 0..root.manifest().tests.len() {
            collector.analysis(source::analyze_test(graph, index, limits)?)?;
        }
        Ok(Self {
            diagnostics: collector.values.into_values().collect(),
            failed: collector.failed,
        })
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn failed(&self, deny_warnings: bool) -> bool {
        self.failed
            || self.diagnostics.iter().any(|d| {
                d.severity == Severity::Error || (deny_warnings && d.severity == Severity::Warning)
            })
    }
}
#[derive(Default)]
struct Collector {
    values: BTreeMap<Key, Diagnostic>,
    failed: bool,
}
impl Collector {
    fn push(&mut self, d: Diagnostic) -> Result<(), Diagnostic> {
        let span = d.span.unwrap_or(ByteSpan { start: 0, end: 0 });
        let severity = match d.severity {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Note => 2,
        };
        let key = (
            d.path.clone().unwrap_or_default(),
            span.start,
            span.end,
            d.code.to_string(),
            severity,
            d.message.clone(),
        );
        if !self.values.contains_key(&key) && self.values.len() == MAX_DIAGNOSTICS {
            return Err(Diagnostic::error(
                "PKG0004",
                "aggregate lint diagnostic limit exceeded",
            ));
        }
        self.values.insert(key, d);
        Ok(())
    }
    fn analysis(&mut self, analysis: Analysis) -> Result<(), Diagnostic> {
        for diagnostic in analysis.diagnostics {
            let mut d = at(
                &analysis.sources,
                diagnostic.primary.span,
                diagnostic.code.as_str(),
                diagnostic.message,
            )?;
            d.severity = match diagnostic.severity {
                lang_diagnostics::Severity::Error => Severity::Error,
                lang_diagnostics::Severity::Warning => Severity::Warning,
                lang_diagnostics::Severity::Note => Severity::Note,
            };
            self.push(d)?;
        }
        let program = match analysis.result {
            Ok(program) => program,
            Err(AnalysisError::Diagnostics) => {
                self.failed = true;
                return Ok(());
            }
            Err(e) => {
                self.failed = true;
                self.push(Diagnostic::error("CLI0002", e.to_string()))?;
                return Ok(());
            }
        };
        for function in &program.source().functions {
            let used: BTreeSet<_> = function
                .expressions
                .iter()
                .filter_map(|e| {
                    if let ExpressionKind::Local(id) = e.kind {
                        Some(id.0)
                    } else {
                        None
                    }
                })
                .collect();
            for (index, local) in function.locals.iter().enumerate() {
                if local.kind != BindingKind::ArenaOwner
                    && !local.name.text.starts_with('_')
                    && !used.contains(&index)
                {
                    let mut d = at(
                        &analysis.sources,
                        local.name.span,
                        "LNT0001",
                        format!(
                            "binding {} is never read; prefix its name with _ if intentional",
                            local.name.text
                        ),
                    )?;
                    d.severity = Severity::Warning;
                    self.push(d)?;
                }
            }
        }
        Ok(())
    }
}
fn at(
    sources: &SourceMap,
    span: Span,
    code: &'static str,
    message: impl Into<String>,
) -> Result<Diagnostic, Diagnostic> {
    let file = sources
        .file(span.source())
        .map_err(|e| Diagnostic::error("CLI0002", e.to_string()))?;
    let mut d = Diagnostic::error(code, message);
    d.path = Some(file.name().to_string());
    d.span = Some(ByteSpan {
        start: span.range().start,
        end: span.range().end,
    });
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collector_accepts_exact_limit_deduplicates_and_refuses_one_more() {
        let mut c = Collector::default();
        for index in 0..MAX_DIAGNOSTICS {
            c.push(Diagnostic::error("LNT0001", index.to_string()))
                .unwrap();
        }
        c.push(Diagnostic::error("LNT0001", "0")).unwrap();
        assert_eq!(c.values.len(), MAX_DIAGNOSTICS);
        assert_eq!(
            c.push(Diagnostic::error("LNT0001", "overflow"))
                .unwrap_err()
                .code,
            "PKG0004"
        );
        assert_eq!(c.values.len(), MAX_DIAGNOSTICS);
    }
}
