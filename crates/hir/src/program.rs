//! Executable RFC 0006 Copy typing and type-only RFC 0007 body analysis.

mod bodies;
mod coverage;
pub use bodies::{BodyAnalysis, BodyTypes, analyze_bodies};

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_diagnostics::{
    Diagnostic, DiagnosticCode, Label, Severity, SourceError, SourceMap, Span, codes,
};

use crate::{
    core,
    layout::{LayoutError, LayoutLimits, ValueLayout, analyze_layouts},
    loans, moves, resolved as r,
    source::Name,
    types,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckLimits {
    pub layout: LayoutLimits,
    pub types: types::Limits,
    pub moves: moves::Limits,
    pub loans: loans::Limits,
    pub modules: usize,
    pub functions: usize,
    pub imports: usize,
    pub locals: usize,
    pub expressions: usize,
    pub patterns: usize,
    pub blocks: usize,
    pub depth: usize,
    pub diagnostics: usize,
    pub work: usize,
    pub exhaustiveness: usize,
    pub witness_bytes: usize,
    pub type_display_bytes: usize,
}
impl Default for CheckLimits {
    fn default() -> Self {
        Self {
            layout: LayoutLimits::default(),
            types: types::Limits::default(),
            moves: moves::Limits::default(),
            loans: loans::Limits::default(),
            modules: 256,
            functions: 4096,
            imports: 4096,
            locals: 1_000_000,
            expressions: 1_000_000,
            patterns: 1_000_000,
            blocks: 1_000_000,
            depth: 256,
            diagnostics: 4096,
            work: 10_000_000,
            exhaustiveness: 10_000_000,
            witness_bytes: 65_536,
            type_display_bytes: 65_536,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArenaKind {
    Expression,
    Pattern,
    Block,
    Local,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckInputErrorKind {
    Source(SourceError),
    MissingRoot,
    InvalidId { arena: ArenaKind, index: usize },
    InvalidModule(usize),
    InvalidDefinition(usize),
    InvalidConstructed(usize),
    InvalidFunction(usize),
    InvalidImport(usize),
    InvalidCase(usize),
    SpanOutsideOwner,
    NameTextMismatch,
    DuplicateQualifiedName,
    InvalidLocalMetadata,
    InvalidSignature,
    InvalidExpressionFunction,
    InvisibleLocal(usize),
    Cycle(ArenaKind),
    SharedNode(ArenaKind),
    Orphan(ArenaKind),
    IncompleteTyping,
    InvalidPlace,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckError {
    Input {
        kind: CheckInputErrorKind,
        span: Option<Span>,
    },
    Layout(LayoutError),
    Types(types::Error),
    Moves(moves::Error),
    Loans(loans::Error),
    OwnershipProfile {
        span: Span,
    },
    Limit(Box<Diagnostic>),
}
impl Display for CheckError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input { kind, .. } => write!(f, "invalid resolved program: {kind:?}"),
            Self::Layout(e) => Display::fmt(e, f),
            Self::Types(e) => Display::fmt(e, f),
            Self::Moves(e) => Display::fmt(e, f),
            Self::Loans(e) => Display::fmt(e, f),
            Self::OwnershipProfile { .. } => f.write_str(
                "ownership source bodies require the remaining provenance checks and ownership MIR",
            ),
            Self::Limit(d) => write!(f, "{}: {}", d.code.as_str(), d.message),
        }
    }
}
impl Error for CheckError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flow {
    FallsThrough,
    Returns,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionFacts {
    locals: Vec<r::Type>,
    expressions: Vec<r::Type>,
    patterns: Vec<r::Type>,
    blocks: Vec<Flow>,
    fields: BTreeMap<r::ExpressionId, usize>,
    records: BTreeMap<r::ExpressionId, Vec<usize>>,
    pattern_fields: BTreeMap<r::PatternId, Vec<r::PatternId>>,
    unreachable_arms: BTreeSet<r::PatternId>,
    calls: BTreeMap<r::ExpressionId, CallFacts>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallFacts {
    pub parameters: Vec<r::Type>,
    pub result_binding: core::ResultBinding,
}
impl FunctionFacts {
    pub fn locals(&self) -> &[r::Type] {
        &self.locals
    }
    pub fn expressions(&self) -> &[r::Type] {
        &self.expressions
    }
    pub fn patterns(&self) -> &[r::Type] {
        &self.patterns
    }
    pub fn blocks(&self) -> &[Flow] {
        &self.blocks
    }
    pub fn field_index(&self, id: r::ExpressionId) -> Option<usize> {
        self.fields.get(&id).copied()
    }
    /// Written field index to declaration field index; evaluation stays written.
    pub fn record_order(&self, id: r::ExpressionId) -> Option<&[usize]> {
        self.records.get(&id).map(Vec::as_slice)
    }
    pub fn pattern_fields(&self, id: r::PatternId) -> Option<&[r::PatternId]> {
        self.pattern_fields.get(&id).map(Vec::as_slice)
    }
    pub fn call(&self, id: r::ExpressionId) -> Option<&CallFacts> {
        self.calls.get(&id)
    }
    pub fn arm_is_unreachable(&self, pattern: r::PatternId) -> bool {
        self.unreachable_arms.contains(&pattern)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedProgram {
    source: r::Program,
    functions: Vec<FunctionFacts>,
    layouts: Vec<ValueLayout>,
}
impl TypedProgram {
    pub fn source(&self) -> &r::Program {
        &self.source
    }
    pub fn functions(&self) -> &[FunctionFacts] {
        &self.functions
    }
    pub fn layouts(&self) -> &[ValueLayout] {
        &self.layouts
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramAnalysis {
    pub program: Option<TypedProgram>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn check_program(
    source: r::Program,
    sources: &SourceMap,
    limits: CheckLimits,
) -> Result<ProgramAnalysis, CheckError> {
    check_program_internal(source, sources, limits, None)
}

/// Infer only a host-generated REPL expression wrapper's result. Ordinary
/// declarations keep explicit result types. The wrapper must be a private
/// root function with no locals/parameters and exactly one return expression;
/// no expression anywhere may call it, so provisional signatures cannot leak
/// into recursive calls or other function facts.
pub fn check_expression_function(
    source: r::Program,
    sources: &SourceMap,
    limits: CheckLimits,
    wrapper: r::FunctionId,
) -> Result<ProgramAnalysis, CheckError> {
    check_program_internal(source, sources, limits, Some(wrapper))
}

fn check_program_internal(
    mut source: r::Program,
    sources: &SourceMap,
    limits: CheckLimits,
    infer: Option<r::FunctionId>,
) -> Result<ProgramAnalysis, CheckError> {
    let mut context = Context {
        source: &source,
        sources,
        limits,
        work: 0,
        coverage_work: 0,
        diagnostics: Vec::new(),
        errors: 0,
        types: None,
    };
    let mut ownership_span = source.types.first().map(|first| first.span);
    if ownership_span.is_none() {
        // Validate/count foreign arenas before looking for new forms without
        // constructed annotations. The scan shares the Copy checker budget.
        context.metadata()?;
        'scan: for function in &source.functions {
            context.charge(function.span)?;
            for expression in &function.expressions {
                context.charge(function.span)?;
                if matches!(
                    expression.kind,
                    r::ExpressionKind::Borrow { .. }
                        | r::ExpressionKind::Dereference(_)
                        | r::ExpressionKind::CoreCall { .. }
                        | r::ExpressionKind::CoreCase { .. }
                        | r::ExpressionKind::CoreUnit
                ) {
                    ownership_span = Some(expression.span);
                    break 'scan;
                }
            }
            for pattern in &function.patterns {
                context.charge(function.span)?;
                if matches!(
                    pattern.kind,
                    r::PatternKind::CoreUnit | r::PatternKind::CoreCase { .. }
                ) {
                    ownership_span = Some(pattern.span);
                    break 'scan;
                }
            }
            for block in &function.blocks {
                context.charge(function.span)?;
                for statement in &block.statements {
                    context.charge(function.span)?;
                    if matches!(
                        statement.kind,
                        r::StatementKind::PlaceAssign { .. } | r::StatementKind::Arena { .. }
                    ) {
                        ownership_span = Some(statement.span);
                        break 'scan;
                    }
                }
            }
        }
    }
    if let Some(span) = ownership_span {
        let analysis = analyze_bodies(source, sources, limits)?;
        let Some(bodies) = analysis.bodies else {
            return Ok(ProgramAnalysis {
                program: None,
                diagnostics: analysis.diagnostics,
            });
        };
        let mut diagnostics = analysis.diagnostics;
        let moved = moves::analyze(bodies, sources, limits.moves).map_err(CheckError::Moves)?;
        diagnostics.extend(moved.diagnostics);
        let Some(checked_moves) = moved.bodies else {
            return Ok(ProgramAnalysis {
                program: None,
                diagnostics,
            });
        };
        let loaned =
            loans::analyze(checked_moves, sources, limits.loans).map_err(CheckError::Loans)?;
        if !loaned.diagnostics.is_empty() {
            diagnostics.extend(loaned.diagnostics);
            return Ok(ProgramAnalysis {
                program: None,
                diagnostics,
            });
        }
        return Err(CheckError::OwnershipProfile { span });
    }
    let expression = if let Some(id) = infer {
        let invalid = || input(CheckInputErrorKind::InvalidExpressionFunction, None);
        let function = source.functions.get(id.0).ok_or_else(invalid)?;
        if function.module != r::ModuleId(0)
            || function.public
            || !function.name.text.starts_with("__repl_eval_")
            || !function.parameters.is_empty()
            || !function.locals.is_empty()
            || !function.patterns.is_empty()
            || function.blocks.len() != 1
            || function.body != r::BlockId(0)
            || function.result.value_type != r::Type::I64
        {
            return Err(invalid());
        }
        let [
            r::Statement {
                kind: r::StatementKind::Return(expression),
                ..
            },
        ] = function.blocks[0].statements.as_slice()
        else {
            return Err(invalid());
        };
        for f in &source.functions {
            for e in &f.expressions {
                context.charge(e.span)?;
                if matches!(e.kind,r::ExpressionKind::Call {callee:r::Callable::Guest(target),..} if target==id)
                {
                    return Err(invalid());
                }
            }
        }
        Some(*expression)
    } else {
        None
    };
    let layout =
        analyze_layouts(&source.definitions, sources, limits.layout).map_err(CheckError::Layout)?;
    for diagnostic in layout.diagnostics {
        context.report(diagnostic)?;
    }
    let Some(layouts) = layout.layouts else {
        return Ok(ProgramAnalysis {
            program: None,
            diagnostics: context.diagnostics,
        });
    };
    let mut facts = Vec::new();
    for (index, function) in source.functions.iter().enumerate() {
        facts.push(
            FunctionChecker::new(
                &mut context,
                function,
                infer == Some(r::FunctionId(index)),
                r::FunctionId(index),
            )
            .check()?,
        );
    }
    let successful = context.errors == 0;
    let diagnostics = context.diagnostics;
    if !successful {
        return Ok(ProgramAnalysis {
            program: None,
            diagnostics,
        });
    }
    let functions = facts
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| input(CheckInputErrorKind::IncompleteTyping, None))?;
    if let (Some(wrapper), Some(expression)) = (infer, expression) {
        source.functions[wrapper.0].result.value_type =
            functions[wrapper.0].expressions[expression.0];
    }
    Ok(ProgramAnalysis {
        program: Some(TypedProgram {
            source,
            functions,
            layouts,
        }),
        diagnostics,
    })
}

fn input(kind: CheckInputErrorKind, span: Option<Span>) -> CheckError {
    CheckError::Input { kind, span }
}

struct Context<'a> {
    source: &'a r::Program,
    sources: &'a SourceMap,
    limits: CheckLimits,
    work: usize,
    coverage_work: usize,
    diagnostics: Vec<Diagnostic>,
    types: Option<bodies::TypeStore>,
    errors: usize,
}
impl<'a> Context<'a> {
    fn constructor(&self, ty: r::Type) -> Option<r::TypeConstructor> {
        self.types.as_ref().and_then(|types| types.constructor(ty))
    }
    fn loan(&self, ty: r::Type) -> Option<(r::LoanKind, r::Type)> {
        if let Some(r::TypeConstructor::Loan(kind, target)) = self.constructor(ty) {
            Some((kind, target))
        } else {
            None
        }
    }
    fn is_copy(&self, ty: r::Type, span: Span) -> Result<bool, CheckError> {
        self.types.as_ref().map_or(Ok(true), |types| {
            types.properties(ty, span).map(|p| p.is_copy)
        })
    }
    fn instantiate(
        &mut self,
        pattern: core::TypePattern,
        arguments: &[r::Type],
        span: Span,
    ) -> Result<r::Type, CheckError> {
        self.types
            .as_mut()
            .ok_or(CheckError::OwnershipProfile { span })?
            .instantiate(pattern, arguments, span, 0)
    }
    fn instantiate_with_owner(
        &mut self,
        pattern: core::TypePattern,
        arguments: &[r::Type],
        owner: Option<r::Type>,
        span: Span,
    ) -> Result<r::Type, CheckError> {
        self.types
            .as_mut()
            .ok_or(CheckError::OwnershipProfile { span })?
            .instantiate_with_owner(pattern, arguments, owner, span, 0)
    }
    fn intern(
        &mut self,
        constructor: r::TypeConstructor,
        span: Span,
    ) -> Result<r::Type, CheckError> {
        self.types
            .as_mut()
            .ok_or(CheckError::OwnershipProfile { span })?
            .intern(constructor, span)
    }
    fn core_payload(
        &mut self,
        case: core::Case,
        expected: Option<r::Type>,
        span: Span,
    ) -> Result<Option<Vec<r::Type>>, CheckError> {
        let family = expected.and_then(|ty| self.types.as_ref().and_then(|types| types.family(ty)));
        if let Some((family, arguments)) = family
            && family == case.family()
        {
            let mut payload = Vec::new();
            for pattern in case.payload() {
                self.charge(span)?;
                payload.push(self.instantiate(
                    *pattern,
                    &arguments[..family.type_arguments()],
                    span,
                )?);
            }
            Ok(Some(payload))
        } else {
            self.error(
                codes::TYPE_MISMATCH,
                span,
                "core case requires an unambiguous expected type of its own family",
                None,
            )?;
            Ok(None)
        }
    }
    fn limit(&self, span: Span, message: &str) -> CheckError {
        CheckError::Limit(Box::new(Diagnostic::new(
            codes::RESOURCE_LIMIT,
            Severity::Error,
            message,
            Label::new(span, message),
        )))
    }
    fn span(&self, span: Span) -> Result<(), CheckError> {
        self.sources
            .slice(span)
            .map(|_| ())
            .map_err(|e| input(CheckInputErrorKind::Source(e), Some(span)))
    }
    fn within(&self, owner: Span, span: Span) -> Result<(), CheckError> {
        self.span(span)?;
        if owner.source() != span.source()
            || span.range().start < owner.range().start
            || span.range().end > owner.range().end
        {
            return Err(input(CheckInputErrorKind::SpanOutsideOwner, Some(span)));
        }
        Ok(())
    }
    fn name(&mut self, owner: Span, name: &Name) -> Result<(), CheckError> {
        self.within(owner, name.span)?;
        self.charge(name.span)?;
        if self.sources.slice(name.span).unwrap() != name.text {
            return Err(input(
                CheckInputErrorKind::NameTextMismatch,
                Some(name.span),
            ));
        }
        Ok(())
    }
    fn charge(&mut self, span: Span) -> Result<(), CheckError> {
        if self.work >= self.limits.work {
            return Err(self.limit(span, "program type-check work limit exceeded"));
        }
        self.work += 1;
        Ok(())
    }
    fn enter(&mut self, span: Span, depth: usize) -> Result<(), CheckError> {
        self.charge(span)?;
        if depth >= self.limits.depth.min(256) {
            return Err(self.limit(span, "program type-check depth limit exceeded"));
        }
        Ok(())
    }
    fn report(&mut self, diagnostic: Diagnostic) -> Result<(), CheckError> {
        if self.diagnostics.len() >= self.limits.diagnostics {
            return Err(self.limit(diagnostic.primary.span, "program diagnostic limit exceeded"));
        }
        if diagnostic.severity == Severity::Error {
            self.errors += 1;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }
    fn error(
        &mut self,
        code: DiagnosticCode,
        span: Span,
        message: &str,
        original: Option<Span>,
    ) -> Result<(), CheckError> {
        let mut diagnostic =
            Diagnostic::new(code, Severity::Error, message, Label::new(span, message));
        if let Some(span) = original {
            diagnostic.secondary.push(Label::new(span, "declared here"));
        }
        self.report(diagnostic)
    }
    fn type_text(&mut self, ty: r::Type, span: Span) -> Result<String, CheckError> {
        let mut text = String::new();
        self.write_type(ty, span, 0, &mut text)?;
        Ok(text)
    }
    fn type_fragment(
        &mut self,
        text: &mut String,
        fragment: &str,
        span: Span,
    ) -> Result<(), CheckError> {
        if fragment.len() > self.limits.type_display_bytes.saturating_sub(text.len()) {
            return Err(self.limit(span, "type diagnostic byte limit exceeded"));
        }
        for _ in 0..fragment.len().div_ceil(1024) {
            self.charge(span)?;
        }
        text.try_reserve(fragment.len())
            .map_err(|_| CheckError::Types(types::Error::AllocationFailed { span }))?;
        text.push_str(fragment);
        Ok(())
    }
    fn write_type(
        &mut self,
        ty: r::Type,
        span: Span,
        depth: usize,
        text: &mut String,
    ) -> Result<(), CheckError> {
        self.charge(span)?;
        if depth > self.limits.types.depth.min(64) {
            return Err(self.limit(span, "type diagnostic depth limit exceeded"));
        }
        match ty {
            r::Type::I64 => self.type_fragment(text, "i64", span),
            r::Type::Bool => self.type_fragment(text, "bool", span),
            r::Type::Nominal(id) => {
                let name = self
                    .source
                    .definitions
                    .get(id.0)
                    .ok_or_else(|| input(CheckInputErrorKind::InvalidDefinition(id.0), Some(span)))?
                    .qualified_name
                    .as_str();
                self.type_fragment(text, name, span)
            }
            r::Type::Constructed(id) => {
                let constructor = self.constructor(ty).ok_or_else(|| {
                    input(CheckInputErrorKind::InvalidConstructed(id.0), Some(span))
                })?;
                let (prefix, suffix) = match constructor {
                    r::TypeConstructor::ArenaOwner(_) => ("arena-owner", ""),
                    r::TypeConstructor::Builtin(builtin) => {
                        self.type_fragment(text, "core::", span)?;
                        return self.type_fragment(text, builtin.name(), span);
                    }
                    r::TypeConstructor::Option(_) => ("core::Option<", ">"),
                    r::TypeConstructor::Result(..) => ("core::Result<", ">"),
                    r::TypeConstructor::Vec(_) => ("core::Vec<", ">"),
                    r::TypeConstructor::Map(..) => ("core::Map<", ">"),
                    r::TypeConstructor::Shared(_) => ("shared ", ""),
                    r::TypeConstructor::Weak(_) => ("weak ", ""),
                    r::TypeConstructor::Arena(_) => ("arena ", ""),
                    r::TypeConstructor::Loan(r::LoanKind::Shared, _) => ("&", ""),
                    r::TypeConstructor::Loan(r::LoanKind::Exclusive, _) => ("&mut ", ""),
                };
                self.type_fragment(text, prefix, span)?;
                for (index, argument) in constructor.arguments().enumerate() {
                    if index != 0 {
                        self.type_fragment(text, ", ", span)?;
                    }
                    self.write_type(argument, span, depth + 1, text)?;
                }
                self.type_fragment(text, suffix, span)
            }
        }
    }
    fn expect(
        &mut self,
        actual: Option<r::Type>,
        expected: r::Type,
        code: DiagnosticCode,
        span: Span,
        original: Option<Span>,
    ) -> Result<bool, CheckError> {
        if let Some(actual) = actual
            && actual != expected
        {
            let expected_text = self.type_text(expected, span)?;
            let actual_text = self.type_text(actual, span)?;
            self.error(
                code,
                span,
                &format!("expected `{expected_text}`, found `{actual_text}`"),
                original,
            )?;
        }
        Ok(actual == Some(expected))
    }
    fn declaration(
        &mut self,
        module: r::ModuleId,
        owner: Span,
        name: &Name,
        qualified: &str,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), CheckError> {
        self.span(owner)?;
        let parent = self
            .source
            .modules
            .get(module.0)
            .ok_or_else(|| input(CheckInputErrorKind::InvalidModule(module.0), Some(owner)))?;
        self.within(parent.span, owner)?;
        self.name(owner, name)?;
        let expected = if parent.qualified_name.is_empty() {
            name.text.clone()
        } else {
            format!("{}::{}", parent.qualified_name, name.text)
        };
        if expected != qualified {
            return Err(input(
                CheckInputErrorKind::InvalidSignature,
                Some(name.span),
            ));
        }
        if !seen.insert(qualified.into()) {
            return Err(input(
                CheckInputErrorKind::DuplicateQualifiedName,
                Some(name.span),
            ));
        }
        Ok(())
    }
    fn type_use(&mut self, owner: Span, ty: r::TypeUse, public: bool) -> Result<(), CheckError> {
        self.within(owner, ty.span)?;
        self.charge(ty.span)?;
        match ty.value_type {
            r::Type::Constructed(id) => {
                if self.types.is_none() || id.0 >= self.source.types.len() {
                    return Err(input(
                        CheckInputErrorKind::InvalidConstructed(id.0),
                        Some(ty.span),
                    ));
                }
            }
            r::Type::Nominal(id) => {
                let definition = self.source.definitions.get(id.0).ok_or_else(|| {
                    input(CheckInputErrorKind::InvalidDefinition(id.0), Some(ty.span))
                })?;
                if public && !definition.public {
                    self.error(
                        codes::MODULE_ERROR,
                        ty.span,
                        "public signature contains a private type",
                        Some(definition.name.span),
                    )?;
                }
            }
            _ if self.limits.layout.cells == 0 || self.limits.layout.depth == 0 => {
                return Err(self.limit(ty.span, "scalar type exceeds value layout limit"));
            }
            _ => {}
        }
        Ok(())
    }
    fn metadata(&mut self) -> Result<(), CheckError> {
        if self.source.modules.is_empty() {
            return Err(input(CheckInputErrorKind::MissingRoot, None));
        }
        let root = self.source.modules[0].span;
        self.span(root)?;
        if self.source.modules.len() > self.limits.modules
            || self.source.functions.len() > self.limits.functions
            || self.source.imports.len() > self.limits.imports
            || self.source.definitions.len() > self.limits.layout.definitions
        {
            return Err(self.limit(root, "program declaration count limit exceeded"));
        }
        let mut seen = BTreeSet::new();
        for module in &self.source.modules {
            self.span(module.span)?;
            self.charge(module.span)?;
            if !seen.insert(module.qualified_name.clone()) {
                return Err(input(
                    CheckInputErrorKind::DuplicateQualifiedName,
                    Some(module.span),
                ));
            }
        }
        // Root's empty name is a namespace, not an ordinary declaration.
        let mut locals = 0_usize;
        let mut expressions = 0_usize;
        let mut patterns = 0_usize;
        let mut blocks = 0_usize;
        for definition in &self.source.definitions {
            self.declaration(
                definition.module,
                definition.span,
                &definition.name,
                &definition.qualified_name,
                &mut seen,
            )?;
            match &definition.kind {
                r::DefinitionKind::Record(fields) => {
                    if fields.len() > self.limits.layout.fields {
                        return Err(self.limit(definition.span, "nominal field limit exceeded"));
                    }
                    for field in fields {
                        self.type_use(definition.span, field.value_type, definition.public)?;
                    }
                }
                r::DefinitionKind::Enum(cases) => {
                    if cases.len() > self.limits.layout.fields {
                        return Err(self.limit(definition.span, "nominal case limit exceeded"));
                    }
                    for case in cases {
                        if case.payload.len() > self.limits.layout.fields {
                            return Err(
                                self.limit(definition.span, "nominal payload limit exceeded")
                            );
                        }
                        for ty in &case.payload {
                            self.type_use(definition.span, *ty, definition.public)?;
                        }
                    }
                }
            }
        }
        for host in &self.source.imports {
            self.declaration(
                host.module,
                host.span,
                &host.name,
                &host.qualified_name,
                &mut seen,
            )?;
            self.name(host.span, &host.parameter)?;
        }
        for function in &self.source.functions {
            self.declaration(
                function.module,
                function.span,
                &function.name,
                &function.qualified_name,
                &mut seen,
            )?;
            self.within(function.span, function.signature_span)?;
            self.type_use(function.span, function.result, function.public)?;
            if function.parameters.len() > self.limits.layout.fields {
                return Err(
                    self.limit(function.signature_span, "function parameter limit exceeded")
                );
            }
            for (total, count, maximum) in [
                (&mut locals, function.locals.len(), self.limits.locals),
                (
                    &mut expressions,
                    function.expressions.len(),
                    self.limits.expressions,
                ),
                (&mut patterns, function.patterns.len(), self.limits.patterns),
                (&mut blocks, function.blocks.len(), self.limits.blocks),
            ] {
                *total = total
                    .checked_add(count)
                    .ok_or_else(|| self.limit(function.span, "program arena count overflow"))?;
                if *total > maximum {
                    return Err(self.limit(function.span, "program arena count limit exceeded"));
                }
            }
            let mut names = BTreeSet::new();
            for local in &function.locals {
                self.name(function.span, &local.name)?;
                if !names.insert(&local.name.text) {
                    return Err(input(
                        CheckInputErrorKind::InvalidLocalMetadata,
                        Some(local.name.span),
                    ));
                }
                match (local.kind, local.annotation) {
                    (r::BindingKind::Pattern | r::BindingKind::ArenaOwner, None) => {}
                    (r::BindingKind::Parameter | r::BindingKind::Mutable, Some(ty)) => self
                        .type_use(
                            function.span,
                            ty,
                            local.kind == r::BindingKind::Parameter && function.public,
                        )?,
                    _ => {
                        return Err(input(
                            CheckInputErrorKind::InvalidLocalMetadata,
                            Some(local.name.span),
                        ));
                    }
                }
            }
            let mut parameters = BTreeSet::new();
            for local in &function.parameters {
                self.charge(function.signature_span)?;
                let item = function.locals.get(local.0).ok_or_else(|| {
                    input(
                        CheckInputErrorKind::InvalidId {
                            arena: ArenaKind::Local,
                            index: local.0,
                        },
                        Some(function.signature_span),
                    )
                })?;
                if item.kind != r::BindingKind::Parameter || !parameters.insert(*local) {
                    return Err(input(
                        CheckInputErrorKind::InvalidSignature,
                        Some(function.signature_span),
                    ));
                }
            }
            if function
                .locals
                .iter()
                .filter(|l| l.kind == r::BindingKind::Parameter)
                .count()
                != parameters.len()
            {
                return Err(input(
                    CheckInputErrorKind::InvalidSignature,
                    Some(function.signature_span),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Visit {
    Unseen,
    Active,
    Done,
}

struct FunctionChecker<'a, 'b> {
    context: &'b mut Context<'a>,
    function: &'a r::Function,
    function_id: r::FunctionId,
    infer_result: bool,
    locals: Vec<Option<r::Type>>,
    expressions: Vec<Option<r::Type>>,
    patterns: Vec<Option<r::Type>>,
    blocks: Vec<Option<Flow>>,
    expression_visits: Vec<Visit>,
    pattern_visits: Vec<Visit>,
    block_visits: Vec<Visit>,
    visible: Vec<bool>,
    declared: Vec<bool>,
    scopes: Vec<Vec<r::LocalId>>,
    fields: BTreeMap<r::ExpressionId, usize>,
    records: BTreeMap<r::ExpressionId, Vec<usize>>,
    pattern_fields: BTreeMap<r::PatternId, Vec<r::PatternId>>,
    unreachable_arms: BTreeSet<r::PatternId>,
    calls: BTreeMap<r::ExpressionId, CallFacts>,
    borrow_match: bool,
}
impl<'a, 'b> FunctionChecker<'a, 'b> {
    fn new(
        context: &'b mut Context<'a>,
        function: &'a r::Function,
        infer_result: bool,
        function_id: r::FunctionId,
    ) -> Self {
        Self {
            context,
            function,
            function_id,
            infer_result,
            locals: function
                .locals
                .iter()
                .map(|l| l.annotation.map(|t| t.value_type))
                .collect(),
            expressions: vec![None; function.expressions.len()],
            patterns: vec![None; function.patterns.len()],
            blocks: vec![None; function.blocks.len()],
            expression_visits: vec![Visit::Unseen; function.expressions.len()],
            pattern_visits: vec![Visit::Unseen; function.patterns.len()],
            block_visits: vec![Visit::Unseen; function.blocks.len()],
            visible: vec![false; function.locals.len()],
            declared: vec![false; function.locals.len()],
            scopes: vec![Vec::new()],
            fields: BTreeMap::new(),
            records: BTreeMap::new(),
            pattern_fields: BTreeMap::new(),
            unreachable_arms: BTreeSet::new(),
            calls: BTreeMap::new(),
            borrow_match: false,
        }
    }
    fn start(
        visits: &mut [Visit],
        index: usize,
        arena: ArenaKind,
        span: Span,
    ) -> Result<(), CheckError> {
        let state = visits
            .get_mut(index)
            .ok_or_else(|| input(CheckInputErrorKind::InvalidId { arena, index }, Some(span)))?;
        match state {
            Visit::Active => return Err(input(CheckInputErrorKind::Cycle(arena), Some(span))),
            Visit::Done => return Err(input(CheckInputErrorKind::SharedNode(arena), Some(span))),
            Visit::Unseen => *state = Visit::Active,
        }
        Ok(())
    }
    fn check(mut self) -> Result<Option<FunctionFacts>, CheckError> {
        let before = self.context.errors;
        for local in &self.function.parameters {
            self.declare(
                *local,
                r::BindingKind::Parameter,
                self.locals[local.0],
                self.function.signature_span,
            )?;
        }
        let flow = self.block(self.function.body, 0)?;
        if flow == Flow::FallsThrough {
            let block = &self.function.blocks[self.function.body.0];
            self.context.error(
                codes::MISSING_RETURN,
                block.closing_brace,
                "function can finish without returning a value",
                Some(self.function.signature_span),
            )?;
        }
        for (visits, arena) in [
            (&self.expression_visits, ArenaKind::Expression),
            (&self.pattern_visits, ArenaKind::Pattern),
            (&self.block_visits, ArenaKind::Block),
        ] {
            if visits.iter().any(|v| *v != Visit::Done) {
                return Err(input(
                    CheckInputErrorKind::Orphan(arena),
                    Some(self.function.span),
                ));
            }
        }
        if self.declared.iter().any(|d| !d) {
            return Err(input(
                CheckInputErrorKind::Orphan(ArenaKind::Local),
                Some(self.function.span),
            ));
        }
        if self.context.errors != before {
            return Ok(None);
        }
        let incomplete = || {
            input(
                CheckInputErrorKind::IncompleteTyping,
                Some(self.function.span),
            )
        };
        Ok(Some(FunctionFacts {
            locals: self
                .locals
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(incomplete)?,
            expressions: self
                .expressions
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(incomplete)?,
            patterns: self
                .patterns
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(incomplete)?,
            blocks: self
                .blocks
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(incomplete)?,
            fields: self.fields,
            records: self.records,
            pattern_fields: self.pattern_fields,
            unreachable_arms: self.unreachable_arms,
            calls: self.calls,
        }))
    }
    fn declare(
        &mut self,
        id: r::LocalId,
        kind: r::BindingKind,
        ty: Option<r::Type>,
        span: Span,
    ) -> Result<(), CheckError> {
        self.context.charge(span)?;
        let local = self.function.locals.get(id.0).ok_or_else(|| {
            input(
                CheckInputErrorKind::InvalidId {
                    arena: ArenaKind::Local,
                    index: id.0,
                },
                Some(span),
            )
        })?;
        if local.kind != kind || self.declared[id.0] {
            return Err(input(CheckInputErrorKind::InvalidLocalMetadata, Some(span)));
        }
        self.declared[id.0] = true;
        self.visible[id.0] = true;
        self.locals[id.0] = ty;
        self.scopes.last_mut().unwrap().push(id);
        Ok(())
    }
    fn local(&mut self, id: r::LocalId, span: Span) -> Result<Option<r::Type>, CheckError> {
        self.context.charge(span)?;
        let visible = self.visible.get(id.0).ok_or_else(|| {
            input(
                CheckInputErrorKind::InvalidId {
                    arena: ArenaKind::Local,
                    index: id.0,
                },
                Some(span),
            )
        })?;
        if !visible {
            return Err(input(CheckInputErrorKind::InvisibleLocal(id.0), Some(span)));
        }
        Ok(self.locals[id.0])
    }
    fn leave_scope(&mut self) {
        for local in self.scopes.pop().unwrap() {
            self.visible[local.0] = false;
        }
    }
    fn block(&mut self, id: r::BlockId, depth: usize) -> Result<Flow, CheckError> {
        Self::start(
            &mut self.block_visits,
            id.0,
            ArenaKind::Block,
            self.function.span,
        )?;
        let block = &self.function.blocks[id.0];
        self.context.within(self.function.span, block.span)?;
        self.context.within(block.span, block.closing_brace)?;
        self.context.enter(block.span, depth)?;
        self.scopes.push(Vec::new());
        let mut flow = Flow::FallsThrough;
        for statement in &block.statements {
            self.context.within(block.span, statement.span)?;
            if flow == Flow::Returns {
                self.context.report(Diagnostic::new(
                    codes::UNREACHABLE_STATEMENT,
                    Severity::Warning,
                    "unreachable statement",
                    Label::new(statement.span, "this statement cannot be reached"),
                ))?;
            }
            let next = self.statement(statement, depth + 1)?;
            if flow == Flow::FallsThrough {
                flow = next;
            }
        }
        self.leave_scope();
        self.block_visits[id.0] = Visit::Done;
        self.blocks[id.0] = Some(flow);
        Ok(flow)
    }
    fn statement(&mut self, statement: &r::Statement, depth: usize) -> Result<Flow, CheckError> {
        self.context.enter(statement.span, depth)?;
        match &statement.kind {
            r::StatementKind::Arena {
                owner,
                allocator,
                body,
            } if self.context.types.is_some() => {
                let expected = self.context.instantiate(
                    core::TypePattern::Loan(
                        r::LoanKind::Shared,
                        &core::TypePattern::Builtin(r::BuiltinType::Allocator),
                    ),
                    &[],
                    statement.span,
                )?;
                let actual = self.expression_expected(*allocator, Some(expected), depth + 1)?;
                self.argument(actual, expected, *allocator, None)?;
                let local = self.function.locals.get(owner.0).ok_or_else(|| {
                    input(
                        CheckInputErrorKind::InvalidId {
                            arena: ArenaKind::Local,
                            index: owner.0,
                        },
                        Some(statement.span),
                    )
                })?;
                let owner_type = self.context.intern(
                    r::TypeConstructor::ArenaOwner(r::ArenaOwnerId {
                        function: self.function_id,
                        local: *owner,
                    }),
                    local.name.span,
                )?;
                self.scopes.push(Vec::new());
                self.declare(
                    *owner,
                    r::BindingKind::ArenaOwner,
                    Some(owner_type),
                    statement.span,
                )?;
                let flow = self.block(*body, depth + 1)?;
                self.leave_scope();
                return Ok(flow);
            }
            r::StatementKind::PlaceAssign { place, value } if self.context.types.is_some() => {
                self.place(*place, statement.span)?;
                let expected = self.expression(r::ExpressionId(place.0), depth + 1)?;
                let actual = self.expression_expected(*value, expected, depth + 1)?;
                if let Some(expected) = expected {
                    self.context.expect(
                        actual,
                        expected,
                        codes::TYPE_MISMATCH,
                        statement.span,
                        None,
                    )?;
                }
            }
            r::StatementKind::PlaceAssign { .. } | r::StatementKind::Arena { .. } => {
                return Err(CheckError::OwnershipProfile {
                    span: statement.span,
                });
            }
            r::StatementKind::LetMut { local, initializer } => {
                let expected = self
                    .function
                    .locals
                    .get(local.0)
                    .and_then(|l| l.annotation)
                    .ok_or_else(|| {
                        input(
                            CheckInputErrorKind::InvalidLocalMetadata,
                            Some(statement.span),
                        )
                    })?;
                let actual =
                    self.expression_expected(*initializer, Some(expected.value_type), depth + 1)?;
                self.context.expect(
                    actual,
                    expected.value_type,
                    primitive_context(expected.value_type),
                    self.function.expressions[initializer.0].span,
                    Some(expected.span),
                )?;
                self.declare(
                    *local,
                    r::BindingKind::Mutable,
                    Some(expected.value_type),
                    statement.span,
                )?;
            }
            r::StatementKind::Assign { local, value } => {
                let expected = self.local(*local, statement.span)?;
                let actual = self.expression_expected(*value, expected, depth + 1)?;
                let declaration = &self.function.locals[local.0];
                if declaration.kind != r::BindingKind::Mutable {
                    self.context.error(
                        codes::IMMUTABLE_ASSIGNMENT,
                        statement.span,
                        "binding is immutable",
                        Some(declaration.name.span),
                    )?;
                }
                if let Some(expected) = expected {
                    self.context.expect(
                        actual,
                        expected,
                        primitive_context(expected),
                        self.function.expressions[value.0].span,
                        Some(declaration.name.span),
                    )?;
                }
            }
            r::StatementKind::Return(value) => {
                let expected = (!self.infer_result).then_some(self.function.result.value_type);
                let ty = self.expression_expected(*value, expected, depth + 1)?;
                if !self.infer_result {
                    self.context.expect(
                        ty,
                        self.function.result.value_type,
                        primitive_context(self.function.result.value_type),
                        self.function.expressions[value.0].span,
                        Some(self.function.result.span),
                    )?;
                }
                return Ok(Flow::Returns);
            }
            r::StatementKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let ty = self.expression(*condition, depth + 1)?;
                self.context.expect(
                    ty,
                    r::Type::Bool,
                    codes::EXPECTED_BOOL,
                    self.function.expressions[condition.0].span,
                    None,
                )?;
                let then_flow = self.block(*then_block, depth + 1)?;
                let else_flow = if let Some(block) = else_block {
                    self.block(*block, depth + 1)?
                } else {
                    Flow::FallsThrough
                };
                if then_flow == Flow::Returns && else_flow == Flow::Returns {
                    return Ok(Flow::Returns);
                }
            }
            r::StatementKind::While { condition, body } => {
                let ty = self.expression(*condition, depth + 1)?;
                self.context.expect(
                    ty,
                    r::Type::Bool,
                    codes::EXPECTED_BOOL,
                    self.function.expressions[condition.0].span,
                    None,
                )?;
                self.block(*body, depth + 1)?;
            }
            r::StatementKind::Match { scrutinee, arms } => {
                let ty = self.expression(*scrutinee, depth + 1)?;
                let previous_borrow_match = self.borrow_match;
                let ty = if let Some(ty) = ty {
                    self.borrow_match = self.context.loan(ty).is_some()
                        || !self.context.is_copy(ty, statement.span)?;
                    Some(self.context.loan(ty).map_or(ty, |(_, target)| target))
                } else {
                    None
                };
                let mut valid = ty.is_some();
                for arm in arms {
                    self.context.within(statement.span, arm.span)?;
                    self.context.enter(arm.span, depth + 1)?;
                    self.scopes.push(Vec::new());
                    valid &= self.pattern(arm.pattern, ty, depth + 2)?;
                    self.block(arm.body, depth + 2)?;
                    self.leave_scope();
                }
                if valid {
                    let exhaustive = coverage::check_match(
                        self.context,
                        self.function,
                        &self.pattern_fields,
                        &mut self.unreachable_arms,
                        ty.unwrap(),
                        arms,
                        statement.span,
                    )?;
                    let all_return = arms
                        .iter()
                        .filter(|arm| !self.unreachable_arms.contains(&arm.pattern))
                        .all(|arm| self.blocks[arm.body.0] == Some(Flow::Returns));
                    if exhaustive && all_return {
                        self.borrow_match = previous_borrow_match;
                        return Ok(Flow::Returns);
                    }
                }
                self.borrow_match = previous_borrow_match;
            }
        }
        Ok(Flow::FallsThrough)
    }
    fn definition(&self, id: r::DefinitionId, span: Span) -> Result<&'a r::Definition, CheckError> {
        self.context
            .source
            .definitions
            .get(id.0)
            .ok_or_else(|| input(CheckInputErrorKind::InvalidDefinition(id.0), Some(span)))
    }
    fn field_index(
        &mut self,
        fields: &[r::Field],
        name: &Name,
    ) -> Result<Option<usize>, CheckError> {
        for (index, field) in fields.iter().enumerate() {
            self.context.charge(name.span)?;
            if field.name.text == name.text {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
    fn expression(
        &mut self,
        id: r::ExpressionId,
        depth: usize,
    ) -> Result<Option<r::Type>, CheckError> {
        self.expression_expected(id, None, depth)
    }
    fn expression_expected(
        &mut self,
        id: r::ExpressionId,
        expected: Option<r::Type>,
        depth: usize,
    ) -> Result<Option<r::Type>, CheckError> {
        Self::start(
            &mut self.expression_visits,
            id.0,
            ArenaKind::Expression,
            self.function.span,
        )?;
        let expression = &self.function.expressions[id.0];
        self.context.within(self.function.span, expression.span)?;
        self.context.enter(expression.span, depth)?;
        let span = expression.span;
        if self.context.types.is_none()
            && matches!(
                expression.kind,
                r::ExpressionKind::Borrow { .. }
                    | r::ExpressionKind::Dereference(_)
                    | r::ExpressionKind::CoreCall { .. }
                    | r::ExpressionKind::CoreUnit
                    | r::ExpressionKind::CoreCase { .. }
            )
        {
            return Err(CheckError::OwnershipProfile { span });
        }
        let ty = match &expression.kind {
            r::ExpressionKind::CoreUnit => Some(
                self.context
                    .intern(r::TypeConstructor::Builtin(r::BuiltinType::Unit), span)?,
            ),
            r::ExpressionKind::CoreCase {
                case,
                payload,
                explicit_payload,
                case_span,
            } => {
                self.context.within(span, *case_span)?;
                let ty = if case.family().type_arguments() == 0 {
                    Some(self.context.intern(
                        r::TypeConstructor::Builtin(
                            r::BuiltinType::from_name(case.family().name()).unwrap(),
                        ),
                        span,
                    )?)
                } else {
                    expected
                };
                let expected_payload = self.context.core_payload(*case, ty, span)?;
                if payload.len() != case.payload().len()
                    || *explicit_payload == case.payload().is_empty()
                {
                    self.context.error(
                        codes::INVALID_SHAPE,
                        span,
                        "core case payload shape does not match its case",
                        None,
                    )?;
                }
                if payload.len() > self.context.limits.layout.fields {
                    return Err(self.context.limit(span, "core payload limit exceeded"));
                }
                for (position, expression) in payload.iter().enumerate() {
                    let expected = expected_payload
                        .as_ref()
                        .and_then(|p| p.get(position))
                        .copied();
                    let actual = self.expression_expected(*expression, expected, depth + 1)?;
                    if let Some(expected) = expected {
                        self.context.expect(
                            actual,
                            expected,
                            codes::TYPE_MISMATCH,
                            self.function.expressions[expression.0].span,
                            Some(*case_span),
                        )?;
                    }
                }
                expected_payload.and(ty)
            }
            r::ExpressionKind::CoreCall { .. } => self.core_call(id, expression, depth)?,
            r::ExpressionKind::Borrow { kind, place } => {
                self.place(*place, span)?;
                let target = self.expression(r::ExpressionId(place.0), depth + 1)?;
                if let Some(target) = target {
                    Some(
                        self.context
                            .intern(r::TypeConstructor::Loan(*kind, target), span)?,
                    )
                } else {
                    None
                }
            }
            r::ExpressionKind::Dereference(value) => {
                let target = self.expression(*value, depth + 1)?;
                if let Some((_, target)) = target.and_then(|ty| self.context.loan(ty)) {
                    Some(target)
                } else {
                    if target.is_some() {
                        self.context.error(
                            codes::INVALID_LOAN,
                            span,
                            "dereference requires an ordinary loan",
                            None,
                        )?;
                    }
                    None
                }
            }
            r::ExpressionKind::Integer(_) => Some(r::Type::I64),
            r::ExpressionKind::Boolean(_) => Some(r::Type::Bool),
            r::ExpressionKind::Local(local) => self.local(*local, span)?,
            r::ExpressionKind::Add(left, right) | r::ExpressionKind::LessThan(left, right) => {
                for value in [left, right] {
                    let ty = self.expression(*value, depth + 1)?;
                    self.context.expect(
                        ty,
                        r::Type::I64,
                        codes::EXPECTED_I64,
                        self.function.expressions[value.0].span,
                        None,
                    )?;
                }
                Some(if matches!(expression.kind, r::ExpressionKind::Add(..)) {
                    r::Type::I64
                } else {
                    r::Type::Bool
                })
            }
            r::ExpressionKind::Call { callee, arguments } => {
                self.call(id, *callee, arguments, span, depth)?
            }
            r::ExpressionKind::Case { definition, index } => {
                let declared = self.definition(*definition, span)?;
                let r::DefinitionKind::Enum(cases) = &declared.kind else {
                    return Err(input(CheckInputErrorKind::InvalidCase(*index), Some(span)));
                };
                let case = cases
                    .get(*index)
                    .ok_or_else(|| input(CheckInputErrorKind::InvalidCase(*index), Some(span)))?;
                if !case.payload.is_empty() {
                    self.context.error(
                        codes::INVALID_SHAPE,
                        span,
                        "enum payload requires call parentheses and arguments",
                        Some(case.name.span),
                    )?;
                }
                Some(r::Type::Nominal(*definition))
            }
            r::ExpressionKind::Record { definition, fields } => {
                if fields.len() > self.context.limits.layout.fields {
                    return Err(self.context.limit(span, "record field limit exceeded"));
                }
                let declared = self.definition(*definition, span)?;
                let declared_fields = if let r::DefinitionKind::Record(fields) = &declared.kind {
                    Some(fields.as_slice())
                } else {
                    self.context.error(
                        codes::INVALID_SHAPE,
                        span,
                        "record construction requires a record type",
                        Some(declared.name.span),
                    )?;
                    None
                };
                let mut seen = BTreeMap::new();
                let mut order = Vec::new();
                for field in fields {
                    self.context.name(span, &field.name)?;
                    let index = if let Some(declared_fields) = declared_fields {
                        self.field_index(declared_fields, &field.name)?
                    } else {
                        None
                    };
                    let expected =
                        declared_fields.and_then(|f| index.map(|i| f[i].value_type.value_type));
                    let ty = self.expression_expected(field.value, expected, depth + 1)?;
                    if let Some(declared_fields) = declared_fields {
                        if let Some(index) = index {
                            if let Some(original) = seen.insert(index, field.name.span) {
                                self.context.error(
                                    codes::INVALID_SHAPE,
                                    field.name.span,
                                    "duplicate record field",
                                    Some(original),
                                )?;
                            }
                            let expected = &declared_fields[index];
                            self.context.expect(
                                ty,
                                expected.value_type.value_type,
                                codes::TYPE_MISMATCH,
                                self.function.expressions[field.value.0].span,
                                Some(expected.name.span),
                            )?;
                            order.push(index);
                        } else {
                            self.context.error(
                                codes::INVALID_SHAPE,
                                field.name.span,
                                "unknown record field",
                                Some(declared.name.span),
                            )?;
                        }
                    }
                }
                if let Some(declared_fields) = declared_fields {
                    for (index, field) in declared_fields.iter().enumerate() {
                        self.context.charge(span)?;
                        if !seen.contains_key(&index) {
                            self.context.error(
                                codes::INVALID_SHAPE,
                                span,
                                &format!("missing record field `{}`", field.name.text),
                                Some(field.name.span),
                            )?;
                        }
                    }
                }
                self.records.insert(id, order);
                Some(r::Type::Nominal(*definition))
            }
            r::ExpressionKind::Field { receiver, name } => {
                self.context.name(span, name)?;
                let receiver = self.expression(*receiver, depth + 1)?;
                let receiver =
                    receiver.map(|ty| self.context.loan(ty).map_or(ty, |(_, target)| target));
                if let Some(r::Type::Nominal(definition)) = receiver {
                    let declared = self.definition(definition, span)?;
                    if let r::DefinitionKind::Record(fields) = &declared.kind {
                        if let Some(index) = self.field_index(fields, name)? {
                            self.fields.insert(id, index);
                            Some(fields[index].value_type.value_type)
                        } else {
                            self.context.error(
                                codes::INVALID_SHAPE,
                                name.span,
                                "unknown record field",
                                Some(declared.name.span),
                            )?;
                            None
                        }
                    } else {
                        self.context.error(
                            codes::TYPE_MISMATCH,
                            span,
                            "field access requires a record value",
                            Some(declared.name.span),
                        )?;
                        None
                    }
                } else {
                    if receiver.is_some() {
                        self.context.error(
                            codes::TYPE_MISMATCH,
                            span,
                            "field access requires a record value",
                            None,
                        )?;
                    }
                    None
                }
            }
        };
        self.expression_visits[id.0] = Visit::Done;
        self.expressions[id.0] = ty;
        Ok(ty)
    }
    fn call(
        &mut self,
        id: r::ExpressionId,
        callee: r::Callable,
        arguments: &[r::ExpressionId],
        span: Span,
        depth: usize,
    ) -> Result<Option<r::Type>, CheckError> {
        if arguments.len() > self.context.limits.layout.fields {
            return Err(self.context.limit(span, "call argument limit exceeded"));
        }
        let (expected, result, original): (Vec<r::Type>, r::Type, Span) =
            match callee {
                r::Callable::Guest(id) => {
                    let function = self.context.source.functions.get(id.0).ok_or_else(|| {
                        input(CheckInputErrorKind::InvalidFunction(id.0), Some(span))
                    })?;
                    for _ in &function.parameters {
                        self.context.charge(span)?;
                    }
                    (
                        function
                            .parameters
                            .iter()
                            .map(|p| function.locals[p.0].annotation.unwrap().value_type)
                            .collect(),
                        function.result.value_type,
                        function.signature_span,
                    )
                }
                r::Callable::Host(id) => {
                    let host = self.context.source.imports.get(id.0).ok_or_else(|| {
                        input(CheckInputErrorKind::InvalidImport(id.0), Some(span))
                    })?;
                    self.context.charge(span)?;
                    (vec![r::Type::I64], r::Type::I64, host.span)
                }
                r::Callable::Case { definition, index } => {
                    let declared = self.definition(definition, span)?;
                    let r::DefinitionKind::Enum(cases) = &declared.kind else {
                        return Err(input(CheckInputErrorKind::InvalidCase(index), Some(span)));
                    };
                    let case = cases.get(index).ok_or_else(|| {
                        input(CheckInputErrorKind::InvalidCase(index), Some(span))
                    })?;
                    if case.payload.is_empty() {
                        self.context.error(
                            codes::INVALID_SHAPE,
                            span,
                            "zero-payload enum case must omit parentheses",
                            Some(case.name.span),
                        )?;
                    }
                    for _ in &case.payload {
                        self.context.charge(span)?;
                    }
                    (
                        case.payload.iter().map(|ty| ty.value_type).collect(),
                        r::Type::Nominal(definition),
                        case.name.span,
                    )
                }
            };
        let mut actual = Vec::new();
        for (position, argument) in arguments.iter().enumerate() {
            actual.push(self.expression_expected(
                *argument,
                expected.get(position).copied(),
                depth + 1,
            )?);
        }
        if actual.len() != expected.len() {
            self.context.error(
                codes::TYPE_MISMATCH,
                span,
                "call argument count does not match its declaration",
                Some(original),
            )?;
        }
        for (index, (actual, expected)) in
            actual.into_iter().zip(expected.iter().copied()).enumerate()
        {
            self.context.charge(span)?;
            self.argument(actual, expected, arguments[index], Some(original))?;
        }
        self.calls.insert(
            id,
            CallFacts {
                parameters: expected,
                result_binding: core::ResultBinding::Value,
            },
        );
        Ok(Some(result))
    }
    fn is_place(&mut self, id: r::PlaceId, span: Span) -> Result<bool, CheckError> {
        let mut current = id.0;
        for depth in 0..=self.context.limits.depth.min(256) {
            self.context.enter(span, depth)?;
            let expression = self.function.expressions.get(current).ok_or_else(|| {
                input(
                    CheckInputErrorKind::InvalidId {
                        arena: ArenaKind::Expression,
                        index: current,
                    },
                    Some(span),
                )
            })?;
            match expression.kind {
                r::ExpressionKind::Local(_) | r::ExpressionKind::Dereference(_) => return Ok(true),
                r::ExpressionKind::Field { receiver, .. } => current = receiver.0,
                _ => return Ok(false),
            }
        }
        Err(self.context.limit(span, "place depth limit exceeded"))
    }
    fn place(&mut self, id: r::PlaceId, span: Span) -> Result<(), CheckError> {
        if self.is_place(id, span)? {
            Ok(())
        } else {
            Err(input(CheckInputErrorKind::InvalidPlace, Some(span)))
        }
    }
    fn argument(
        &mut self,
        actual: Option<r::Type>,
        expected: r::Type,
        expression: r::ExpressionId,
        related: Option<Span>,
    ) -> Result<(), CheckError> {
        let span = self.function.expressions[expression.0].span;
        if let (Some(actual), Some((kind, target))) = (actual, self.context.loan(expected)) {
            if let Some((actual_kind, actual_target)) = self.context.loan(actual)
                && target == actual_target
            {
                if kind != actual_kind && kind == r::LoanKind::Exclusive {
                    self.context.error(
                        codes::INVALID_LOAN,
                        span,
                        "exclusive parameter cannot use a shared loan",
                        related,
                    )?;
                }
                return Ok(());
            }
            if actual == target {
                if self.is_place(r::PlaceId(expression.0), span)? {
                    return Ok(());
                }
                self.context.error(
                    codes::INVALID_LOAN,
                    span,
                    "borrowed parameter requires a compatible place or loan",
                    related,
                )?;
                return Ok(());
            }
        }
        self.context
            .expect(actual, expected, codes::TYPE_MISMATCH, span, related)?;
        Ok(())
    }
    fn core_call(
        &mut self,
        id: r::ExpressionId,
        expression: &'a r::Expression,
        depth: usize,
    ) -> Result<Option<r::Type>, CheckError> {
        let r::ExpressionKind::CoreCall {
            operation,
            type_arguments,
            arguments,
            callee_span,
        } = &expression.kind
        else {
            unreachable!()
        };
        let span = expression.span;
        self.context.within(span, *callee_span)?;
        let signature = operation.signature();
        if arguments.len() > self.context.limits.layout.fields
            || type_arguments.len() > self.context.limits.layout.fields
        {
            return Err(self
                .context
                .limit(span, "core call argument limit exceeded"));
        }
        let arity_valid = type_arguments.len() == signature.type_arguments.len();
        if !arity_valid {
            self.context.error(
                codes::UNSUPPORTED_SPECIALIZATION,
                span,
                "incorrect number of core type arguments",
                Some(*callee_span),
            )?;
        }
        if arguments.len() != signature.parameters.len() {
            self.context.error(
                codes::TYPE_MISMATCH,
                span,
                "incorrect number of core value arguments",
                Some(*callee_span),
            )?;
        }
        let mut types = Vec::new();
        for (index, ty) in type_arguments.iter().enumerate() {
            self.context.type_use(span, *ty, false)?;
            types.push(ty.value_type);
            if let Some(rule) = signature.type_arguments.get(index) {
                let properties = self
                    .context
                    .types
                    .as_ref()
                    .unwrap()
                    .properties(ty.value_type, ty.span)?;
                if properties.contains_loan || properties.contains_allocator {
                    self.context.error(codes::INVALID_OWNERSHIP_TYPE, ty.span, "core type argument must be storable without ordinary loans or allocator authority", None)?;
                }
                if *rule == core::TypeArgumentRule::SharedPayload && properties.contains_arena {
                    self.context.error(
                        codes::REGION_ESCAPE,
                        ty.span,
                        "shared payload type cannot contain arena brands",
                        None,
                    )?;
                }
                if *rule == core::TypeArgumentRule::MapKey
                    && ty.value_type != r::Type::I64
                    && self.context.constructor(ty.value_type)
                        != Some(r::TypeConstructor::Builtin(r::BuiltinType::Text))
                {
                    self.context.error(
                        codes::UNSUPPORTED_SPECIALIZATION,
                        ty.span,
                        "Map keys must be exactly i64 or core::Text",
                        None,
                    )?;
                }
            }
        }
        let arena_operation = matches!(
            operation,
            core::Operation::ArenaNew | core::Operation::ArenaRead | core::Operation::ArenaReplace
        );
        let first = if arena_operation {
            arguments
                .first()
                .map(|argument| self.expression(*argument, depth + 1))
                .transpose()?
        } else {
            None
        };
        let owner = first
            .flatten()
            .map(|ty| self.context.loan(ty).map_or(ty, |(_, target)| target))
            .filter(|ty| {
                matches!(
                    self.context.constructor(*ty),
                    Some(r::TypeConstructor::ArenaOwner(_))
                )
            });
        if arena_operation && owner.is_none() {
            let owner_span = arguments
                .first()
                .map_or(span, |id| self.function.expressions[id.0].span);
            self.context.error(
                codes::INVALID_OWNERSHIP_TYPE,
                owner_span,
                "arena operation requires a lexical arena owner or its loan",
                Some(*callee_span),
            )?;
            for argument in arguments.iter().skip(usize::from(first.is_some())) {
                self.expression(*argument, depth + 1)?;
            }
            return Ok(None);
        }
        let mut parameters = Vec::new();
        if arity_valid {
            for pattern in signature.parameters {
                parameters.push(
                    self.context
                        .instantiate_with_owner(*pattern, &types, owner, span)?,
                );
            }
        }
        for (index, argument) in arguments.iter().enumerate() {
            let expected = parameters.get(index).copied();
            let actual = if index == 0
                && let Some(actual) = first
            {
                actual
            } else {
                self.expression_expected(*argument, expected, depth + 1)?
            };
            if let Some(expected) = expected {
                self.argument(actual, expected, *argument, Some(*callee_span))?;
            }
        }
        if !arity_valid {
            return Ok(None);
        }
        let result = self.context.instantiate(signature.result, &types, span)?;
        self.calls.insert(
            id,
            CallFacts {
                parameters,
                result_binding: signature.result_binding,
            },
        );
        Ok(Some(result))
    }
    fn pattern(
        &mut self,
        id: r::PatternId,
        expected: Option<r::Type>,
        depth: usize,
    ) -> Result<bool, CheckError> {
        Self::start(
            &mut self.pattern_visits,
            id.0,
            ArenaKind::Pattern,
            self.function.span,
        )?;
        let pattern = &self.function.patterns[id.0];
        let span = pattern.span;
        self.context.within(self.function.span, span)?;
        self.context.enter(span, depth)?;
        let binding_expected = expected;
        let expected = expected.map(|ty| self.context.loan(ty).map_or(ty, |(_, target)| target));
        let mut valid = expected.is_some();
        match &pattern.kind {
            r::PatternKind::CoreUnit => {
                let unit = self
                    .context
                    .intern(r::TypeConstructor::Builtin(r::BuiltinType::Unit), span)?;
                valid &= self.pattern_type(unit, expected, span, None)?;
            }
            r::PatternKind::CoreCase {
                case,
                payload,
                explicit_payload,
                case_span,
            } => {
                if self.context.types.is_none() {
                    return Err(CheckError::OwnershipProfile { span });
                }
                self.context.within(span, *case_span)?;
                let types = self.context.core_payload(*case, expected, span)?;
                valid &= types.is_some();
                if payload.len() > self.context.limits.layout.fields {
                    return Err(self
                        .context
                        .limit(span, "core pattern payload limit exceeded"));
                }
                if payload.len() != case.payload().len()
                    || *explicit_payload == case.payload().is_empty()
                {
                    self.context.error(
                        codes::INVALID_SHAPE,
                        span,
                        "core pattern payload shape does not match its case",
                        Some(*case_span),
                    )?;
                    valid = false;
                }
                for (position, pattern) in payload.iter().enumerate() {
                    valid &= self.pattern(
                        *pattern,
                        types.as_ref().and_then(|p| p.get(position)).copied(),
                        depth + 1,
                    )?;
                }
            }
            r::PatternKind::Wildcard => {}
            r::PatternKind::Binding(local) => {
                let expected = binding_expected;
                let binding = if let Some(ty) = expected
                    && self.borrow_match
                    && self.context.loan(ty).is_none()
                    && !self.context.is_copy(ty, span)?
                {
                    Some(
                        self.context
                            .intern(r::TypeConstructor::Loan(r::LoanKind::Shared, ty), span)?,
                    )
                } else {
                    expected
                };
                self.declare(*local, r::BindingKind::Pattern, binding, span)?
            }
            r::PatternKind::Integer(_) => {
                valid &= self.pattern_type(r::Type::I64, expected, span, None)?
            }
            r::PatternKind::Boolean(_) => {
                valid &= self.pattern_type(r::Type::Bool, expected, span, None)?
            }
            r::PatternKind::Record { definition, fields } => {
                if fields.len() > self.context.limits.layout.fields {
                    return Err(self
                        .context
                        .limit(span, "record-pattern field limit exceeded"));
                }
                let declared = self.definition(*definition, span)?;
                valid &= self.pattern_type(
                    r::Type::Nominal(*definition),
                    expected,
                    span,
                    Some(declared.name.span),
                )?;
                let declared_fields = if let r::DefinitionKind::Record(fields) = &declared.kind {
                    Some(fields.as_slice())
                } else {
                    self.context.error(
                        codes::INVALID_SHAPE,
                        span,
                        "record pattern requires a record type",
                        Some(declared.name.span),
                    )?;
                    valid = false;
                    None
                };
                let mut order = declared_fields.map_or_else(Vec::new, |f| vec![None; f.len()]);
                for field in fields {
                    self.context.name(span, &field.name)?;
                    let index = if let Some(fields) = declared_fields {
                        self.field_index(fields, &field.name)?
                    } else {
                        None
                    };
                    let child_type =
                        index.map(|index| declared_fields.unwrap()[index].value_type.value_type);
                    if let Some(index) = index {
                        if order[index].replace(field.pattern).is_some() {
                            self.context.error(
                                codes::INVALID_SHAPE,
                                field.name.span,
                                "duplicate record-pattern field",
                                Some(declared_fields.unwrap()[index].name.span),
                            )?;
                            valid = false;
                        }
                    } else {
                        self.context.error(
                            codes::INVALID_SHAPE,
                            field.name.span,
                            "unknown record-pattern field",
                            Some(declared.name.span),
                        )?;
                        valid = false;
                    }
                    valid &= self.pattern(field.pattern, child_type, depth + 1)?;
                }
                if let Some(fields) = declared_fields {
                    for (index, field) in fields.iter().enumerate() {
                        self.context.charge(span)?;
                        if order[index].is_none() {
                            self.context.error(
                                codes::INVALID_SHAPE,
                                span,
                                &format!("missing pattern field `{}`", field.name.text),
                                Some(field.name.span),
                            )?;
                            valid = false;
                        }
                    }
                }
                if valid {
                    self.pattern_fields
                        .insert(id, order.into_iter().map(Option::unwrap).collect());
                }
            }
            r::PatternKind::Case {
                definition,
                index,
                payload,
                explicit_payload,
            } => {
                if payload.len() > self.context.limits.layout.fields {
                    return Err(self
                        .context
                        .limit(span, "enum-pattern payload limit exceeded"));
                }
                let declared = self.definition(*definition, span)?;
                valid &= self.pattern_type(
                    r::Type::Nominal(*definition),
                    expected,
                    span,
                    Some(declared.name.span),
                )?;
                let r::DefinitionKind::Enum(cases) = &declared.kind else {
                    return Err(input(CheckInputErrorKind::InvalidCase(*index), Some(span)));
                };
                let case = cases
                    .get(*index)
                    .ok_or_else(|| input(CheckInputErrorKind::InvalidCase(*index), Some(span)))?;
                if *explicit_payload == case.payload.is_empty()
                    || payload.len() != case.payload.len()
                {
                    self.context.error(
                        codes::INVALID_SHAPE,
                        span,
                        "enum pattern payload shape does not match its case",
                        Some(case.name.span),
                    )?;
                    valid = false;
                }
                for (position, pattern) in payload.iter().enumerate() {
                    let ty = case.payload.get(position).map(|ty| ty.value_type);
                    valid &= self.pattern(*pattern, ty, depth + 1)?;
                }
            }
        }
        self.pattern_visits[id.0] = Visit::Done;
        self.patterns[id.0] = binding_expected;
        Ok(valid)
    }
    fn pattern_type(
        &mut self,
        actual: r::Type,
        expected: Option<r::Type>,
        span: Span,
        related: Option<Span>,
    ) -> Result<bool, CheckError> {
        if let Some(expected) = expected {
            self.context
                .expect(Some(actual), expected, codes::TYPE_MISMATCH, span, related)
        } else {
            Ok(false)
        }
    }
}

fn primitive_context(expected: r::Type) -> DiagnosticCode {
    if expected == r::Type::I64 {
        codes::EXPECTED_I64
    } else {
        codes::TYPE_MISMATCH
    }
}
