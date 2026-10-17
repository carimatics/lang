//! Bounded nominal layout validation for the copy-only RFC 0006 source types.
//! This pass allocates no guest values and does not establish body typing.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_diagnostics::{Diagnostic, Label, Severity, SourceError, SourceMap, Span, codes};

use crate::{resolved as r, source::Name};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutLimits {
    pub definitions: usize,
    pub fields: usize,
    pub depth: usize,
    pub cells: usize,
    pub diagnostics: usize,
    pub work: usize,
}
impl Default for LayoutLimits {
    fn default() -> Self {
        Self {
            definitions: 4096,
            fields: 256,
            depth: 64,
            cells: 4096,
            diagnostics: 4096,
            work: 10_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueLayout {
    cells: usize,
    depth: usize,
}
impl ValueLayout {
    pub const fn cells(self) -> usize {
        self.cells
    }
    /// Scalars have depth one; an empty record also has depth one.
    pub const fn depth(self) -> usize {
        self.depth
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayoutInputErrorKind {
    Source(SourceError),
    NameTextMismatch,
    InvalidDefinition(usize),
    DuplicateQualifiedName,
    SpanOutsideDefinition,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayoutError {
    Input {
        kind: LayoutInputErrorKind,
        span: Span,
    },
    Limit(Box<Diagnostic>),
}
impl Display for LayoutError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input { kind, .. } => write!(f, "invalid nominal layout input: {kind:?}"),
            Self::Limit(d) => write!(f, "{}: {}", d.code.as_str(), d.message),
        }
    }
}
impl Error for LayoutError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutAnalysis {
    pub layouts: Option<Vec<ValueLayout>>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Validates all IDs/spans and shapes, then walks dependencies iteratively.
/// Reused nominal types are computed once; every reference/field visit is
/// charged, including shape validation and layout accumulation revisits.
pub fn analyze_layouts(
    definitions: &[r::Definition],
    sources: &SourceMap,
    limits: LayoutLimits,
) -> Result<LayoutAnalysis, LayoutError> {
    let mut analyzer = Analyzer {
        definitions,
        sources,
        limits,
        work: 0,
        diagnostics: Vec::new(),
    };
    if definitions.len() > limits.definitions {
        let span = definitions[limits.definitions].span;
        analyzer.span(span)?;
        return Err(analyzer.limit(span, "nominal definition limit exceeded"));
    }
    analyzer.validate()?;
    let mut layouts = vec![None; definitions.len()];
    let mut state = vec![Visit::Unseen; definitions.len()];
    for root in 0..definitions.len() {
        if state[root] != Visit::Unseen {
            continue;
        }
        state[root] = Visit::Active;
        let mut pending = vec![Frame {
            id: root,
            dependencies: type_uses(&definitions[root]),
        }];
        while let Some(frame) = pending.last_mut() {
            if let Some(reference) = frame.dependencies.next() {
                analyzer.charge(reference.span)?;
                if let r::Type::Nominal(target) = reference.value_type {
                    match state[target.0] {
                        Visit::Unseen => {
                            state[target.0] = Visit::Active;
                            pending.push(Frame {
                                id: target.0,
                                dependencies: type_uses(&definitions[target.0]),
                            });
                        }
                        Visit::Active => analyzer.report(
                            reference.span,
                            "recursive nominal type cycle",
                            Some(definitions[target.0].name.span),
                        )?,
                        Visit::Done => {}
                    }
                }
            } else {
                let id = frame.id;
                layouts[id] = analyzer.compute(&definitions[id], &layouts)?;
                state[id] = Visit::Done;
                pending.pop();
            }
        }
    }
    let layouts = if analyzer.diagnostics.is_empty() {
        Some(
            layouts
                .into_iter()
                .map(|l| l.expect("acyclic types have layouts"))
                .collect(),
        )
    } else {
        None
    };
    Ok(LayoutAnalysis {
        layouts,
        diagnostics: analyzer.diagnostics,
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Visit {
    Unseen,
    Active,
    Done,
}
struct Frame<'a> {
    id: usize,
    dependencies: Box<dyn Iterator<Item = &'a r::TypeUse> + 'a>,
}

fn type_uses(definition: &r::Definition) -> Box<dyn Iterator<Item = &r::TypeUse> + '_> {
    match &definition.kind {
        r::DefinitionKind::Record(fields) => Box::new(fields.iter().map(|f| &f.value_type)),
        r::DefinitionKind::Enum(cases) => Box::new(cases.iter().flat_map(|c| &c.payload)),
    }
}

struct Analyzer<'a> {
    definitions: &'a [r::Definition],
    sources: &'a SourceMap,
    limits: LayoutLimits,
    work: usize,
    diagnostics: Vec<Diagnostic>,
}
impl Analyzer<'_> {
    fn contained(&self, outer: Span, inner: Span) -> Result<(), LayoutError> {
        self.span(inner)?;
        if inner.source() != outer.source()
            || inner.range().start < outer.range().start
            || inner.range().end > outer.range().end
        {
            return Err(LayoutError::Input {
                kind: LayoutInputErrorKind::SpanOutsideDefinition,
                span: inner,
            });
        }
        Ok(())
    }
    fn limit(&self, span: Span, message: &str) -> LayoutError {
        LayoutError::Limit(Box::new(Diagnostic::new(
            codes::RESOURCE_LIMIT,
            Severity::Error,
            message,
            Label::new(span, message),
        )))
    }
    fn span(&self, span: Span) -> Result<(), LayoutError> {
        self.sources
            .slice(span)
            .map(|_| ())
            .map_err(|error| LayoutError::Input {
                kind: LayoutInputErrorKind::Source(error),
                span,
            })
    }
    fn name(&mut self, name: &Name) -> Result<(), LayoutError> {
        self.span(name.span)?;
        self.charge(name.span)?;
        if self.sources.slice(name.span).unwrap() != name.text {
            return Err(LayoutError::Input {
                kind: LayoutInputErrorKind::NameTextMismatch,
                span: name.span,
            });
        }
        Ok(())
    }
    fn charge(&mut self, span: Span) -> Result<(), LayoutError> {
        if self.work >= self.limits.work {
            return Err(self.limit(span, "nominal layout work limit exceeded"));
        }
        self.work += 1;
        Ok(())
    }
    fn report(
        &mut self,
        span: Span,
        message: &str,
        previous: Option<Span>,
    ) -> Result<(), LayoutError> {
        if self.diagnostics.len() >= self.limits.diagnostics {
            return Err(self.limit(span, "nominal layout diagnostic limit exceeded"));
        }
        let mut diagnostic = Diagnostic::new(
            codes::INVALID_SHAPE,
            Severity::Error,
            message,
            Label::new(span, message),
        );
        if let Some(previous) = previous {
            diagnostic
                .secondary
                .push(Label::new(previous, "declared here"));
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }
    fn unique_names<'a>(
        &mut self,
        names: impl Iterator<Item = &'a Name>,
    ) -> Result<(), LayoutError> {
        let mut seen = BTreeMap::new();
        for name in names {
            self.name(name)?;
            if let Some(original) = seen.insert(&name.text, name.span) {
                self.report(name.span, "duplicate nominal member name", Some(original))?;
            }
        }
        Ok(())
    }
    fn count(&self, count: usize, span: Span) -> Result<(), LayoutError> {
        if count > self.limits.fields {
            return Err(self.limit(span, "nominal field/case/payload limit exceeded"));
        }
        Ok(())
    }
    fn validate(&mut self) -> Result<(), LayoutError> {
        let mut qualified = BTreeMap::new();
        for definition in self.definitions {
            self.span(definition.span)?;
            self.contained(definition.span, definition.name.span)?;
            self.charge(definition.span)?;
            self.name(&definition.name)?;
            if qualified.insert(&definition.qualified_name, ()).is_some() {
                return Err(LayoutError::Input {
                    kind: LayoutInputErrorKind::DuplicateQualifiedName,
                    span: definition.name.span,
                });
            }
            match &definition.kind {
                r::DefinitionKind::Record(fields) => {
                    self.count(fields.len(), definition.span)?;
                    for field in fields {
                        self.contained(definition.span, field.name.span)?;
                    }
                    self.unique_names(fields.iter().map(|f| &f.name))?;
                }
                r::DefinitionKind::Enum(cases) => {
                    self.count(cases.len(), definition.span)?;
                    for case in cases {
                        self.contained(definition.span, case.name.span)?;
                    }
                    self.unique_names(cases.iter().map(|c| &c.name))?;
                    if cases.is_empty() {
                        self.report(definition.span, "enum must have at least one case", None)?;
                    }
                    for case in cases {
                        self.count(case.payload.len(), case.name.span)?;
                    }
                }
            }
            for reference in type_uses(definition) {
                self.contained(definition.span, reference.span)?;
                self.charge(reference.span)?;
                if let r::Type::Nominal(id) = reference.value_type
                    && id.0 >= self.definitions.len()
                {
                    return Err(LayoutError::Input {
                        kind: LayoutInputErrorKind::InvalidDefinition(id.0),
                        span: reference.span,
                    });
                }
            }
        }
        Ok(())
    }
    fn referenced(
        &self,
        reference: r::TypeUse,
        layouts: &[Option<ValueLayout>],
    ) -> Option<ValueLayout> {
        match reference.value_type {
            r::Type::I64 | r::Type::Bool => Some(ValueLayout { cells: 1, depth: 1 }),
            r::Type::Nominal(id) => layouts[id.0],
        }
    }
    fn sum<'a>(
        &mut self,
        fields: impl Iterator<Item = &'a r::TypeUse>,
        layouts: &[Option<ValueLayout>],
        span: Span,
    ) -> Result<Option<ValueLayout>, LayoutError> {
        let mut cells = 0_usize;
        let mut depth = 0;
        let mut valid = true;
        for reference in fields {
            self.charge(reference.span)?;
            if let Some(child) = self.referenced(*reference, layouts) {
                cells = cells
                    .checked_add(child.cells)
                    .ok_or_else(|| self.limit(reference.span, "nominal cell count overflow"))?;
                depth = depth.max(child.depth);
            } else {
                valid = false;
            }
        }
        let cells = cells
            .checked_add(1)
            .ok_or_else(|| self.limit(span, "nominal cell count overflow"))?;
        let depth = depth
            .checked_add(1)
            .ok_or_else(|| self.limit(span, "nominal depth overflow"))?;
        if valid {
            if cells > self.limits.cells {
                return Err(self.limit(span, "nominal value cell limit exceeded"));
            }
            if depth > self.limits.depth {
                return Err(self.limit(span, "nominal type depth limit exceeded"));
            }
        }
        Ok(valid.then_some(ValueLayout { cells, depth }))
    }
    fn compute(
        &mut self,
        definition: &r::Definition,
        layouts: &[Option<ValueLayout>],
    ) -> Result<Option<ValueLayout>, LayoutError> {
        self.charge(definition.span)?;
        match &definition.kind {
            r::DefinitionKind::Record(fields) => self.sum(
                fields.iter().map(|f| &f.value_type),
                layouts,
                definition.span,
            ),
            r::DefinitionKind::Enum(cases) => {
                let mut maximum = ValueLayout { cells: 1, depth: 1 };
                let mut valid = !cases.is_empty();
                for case in cases {
                    self.charge(case.name.span)?;
                    if let Some(layout) = self.sum(case.payload.iter(), layouts, case.name.span)? {
                        maximum.cells = maximum.cells.max(layout.cells);
                        maximum.depth = maximum.depth.max(layout.depth);
                    } else {
                        valid = false;
                    }
                }
                Ok(valid.then_some(maximum))
            }
        }
    }
}
