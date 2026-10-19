//! Converts successful RFC 0006 syntax/module input to name-resolved source HIR.
//! This phase checks names and visibility; it does not claim expression typing,
//! nominal layout validation, exhaustiveness, or executable code generation.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_diagnostics::{
    Diagnostic, DiagnosticCode, Label, Severity, SourceError, SourceMap, Span, codes,
};
use lang_hir::{core, resolved as r, source::Name};
use lang_syntax::{NodeId, SyntaxElement, SyntaxKind as S, SyntaxTree, TokenKind as K};

use crate::modules::{
    Item, ItemId, ItemKind, ModuleGraph, ModuleId, ResolutionError, ResolvedName, child_nodes,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindingLimits {
    pub work: usize,
    pub locals: usize,
    pub expressions: usize,
    pub patterns: usize,
    pub blocks: usize,
    pub diagnostics: usize,
    pub depth: usize,
    pub types: usize,
}
impl Default for BindingLimits {
    fn default() -> Self {
        Self {
            work: 10_000_000,
            locals: 1_000_000,
            expressions: 1_000_000,
            patterns: 1_000_000,
            blocks: 1_000_000,
            diagnostics: 4096,
            depth: 256,
            types: 65_536,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BindingError {
    Source(SourceError),
    Limit(Box<Diagnostic>),
}
impl Display for BindingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(e) => Display::fmt(e, f),
            Self::Limit(d) => write!(f, "{}: {}", d.code.as_str(), d.message),
        }
    }
}
impl Error for BindingError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingAnalysis {
    pub program: Option<r::Program>,
    pub diagnostics: Vec<Diagnostic>,
}

enum Abort {
    Recover,
    Fatal(BindingError),
}
impl From<BindingError> for Abort {
    fn from(error: BindingError) -> Self {
        Self::Fatal(error)
    }
}
type BindResult<T> = Result<T, Abort>;
#[derive(Clone, Copy, Eq, PartialEq)]
enum CoreName {
    Operation(core::Operation),
    Case(core::Case),
    Unit,
}

/// A failed name pass yields diagnostics without a resolved program. The
/// original immutable snapshots must remain available throughout analysis.
pub fn bind_program(
    graph: &ModuleGraph,
    sources: &SourceMap,
    limits: BindingLimits,
) -> Result<BindingAnalysis, BindingError> {
    for module in graph.modules() {
        sources
            .file(module.source())
            .map_err(BindingError::Source)?;
    }
    let mut context = Context {
        graph,
        sources,
        limits,
        diagnostics: Vec::new(),
        work: 0,
        locals: 0,
        expressions: 0,
        patterns: 0,
        blocks: 0,
        definitions: BTreeMap::new(),
        functions: BTreeMap::new(),
        imports: BTreeMap::new(),
        significant: Vec::new(),
        types: Vec::new(),
        type_ids: BTreeMap::new(),
    };
    // Keep CST ranges lossless, but exclude leading trivia from semantic
    // ranges. Index once so long comment prefixes are not rescanned for every
    // parent in a left-associative expression.
    for module in graph.modules() {
        let mut starts = Vec::new();
        for token in module.tree().tokens() {
            context.charge(token.span, 0).map_err(|abort| match abort {
                Abort::Fatal(error) => error,
                Abort::Recover => unreachable!(),
            })?;
            if !token.kind.is_trivia() {
                starts.push(token.span.range().start);
            }
        }
        context.significant.push(starts);
        // Charge every parsed CST node, including unreachable source, before
        // binding the finite core signatures and ordinary declarations.
        for node in module.tree().nodes() {
            context
                .charge(node.span(), 0)
                .map_err(|abort| match abort {
                    Abort::Fatal(error) => error,
                    Abort::Recover => unreachable!(),
                })?;
        }
    }
    let mut definition_items = Vec::new();
    let mut function_items = Vec::new();
    let mut import_items = Vec::new();
    for item in graph.items() {
        match item.kind() {
            ItemKind::Record | ItemKind::Enum { .. } => {
                context
                    .definitions
                    .insert(item.id(), r::DefinitionId(definition_items.len()));
                definition_items.push(item);
            }
            ItemKind::Function => {
                context
                    .functions
                    .insert(item.id(), r::FunctionId(function_items.len()));
                function_items.push(item);
            }
            ItemKind::Host => {
                context
                    .imports
                    .insert(item.id(), r::ImportId(import_items.len()));
                import_items.push(item);
            }
            _ => {}
        }
    }
    let mut definitions = Vec::new();
    let mut imports = Vec::new();
    let mut functions = Vec::new();
    for item in definition_items {
        match context.definition(item) {
            Ok(value) => definitions.push(value),
            Err(Abort::Recover) => {}
            Err(Abort::Fatal(error)) => return Err(error),
        }
    }
    for item in import_items {
        match context.host(item) {
            Ok(value) => imports.push(value),
            Err(Abort::Recover) => {}
            Err(Abort::Fatal(error)) => return Err(error),
        }
    }
    for item in function_items {
        match FunctionBinder::bind(&mut context, item) {
            Ok(value) => functions.push(value),
            Err(Abort::Recover) => {}
            Err(Abort::Fatal(error)) => return Err(error),
        }
    }
    let modules = graph
        .modules()
        .iter()
        .map(|module| r::Module {
            qualified_name: module.hierarchy().join("::"),
            span: module.tree().node(module.tree().root()).unwrap().span(),
        })
        .collect();
    let program = context.diagnostics.is_empty().then_some(r::Program {
        modules,
        definitions,
        types: context.types,
        imports,
        functions,
    });
    Ok(BindingAnalysis {
        program,
        diagnostics: context.diagnostics,
    })
}

struct Context<'a> {
    graph: &'a ModuleGraph,
    sources: &'a SourceMap,
    significant: Vec<Vec<usize>>,
    limits: BindingLimits,
    diagnostics: Vec<Diagnostic>,
    work: usize,
    locals: usize,
    expressions: usize,
    patterns: usize,
    blocks: usize,
    definitions: BTreeMap<ItemId, r::DefinitionId>,
    functions: BTreeMap<ItemId, r::FunctionId>,
    imports: BTreeMap<ItemId, r::ImportId>,
    types: Vec<r::ConstructedType>,
    type_ids: BTreeMap<r::TypeConstructor, r::ConstructedTypeId>,
}

#[derive(Clone, Copy)]
enum Arena {
    Local,
    Expression,
    Pattern,
    Block,
}

impl<'a> Context<'a> {
    fn tree(&self, module: ModuleId) -> &'a SyntaxTree {
        self.graph.module(module).unwrap().tree()
    }
    fn semantic_span(&mut self, module: ModuleId, syntax: NodeId) -> BindResult<Span> {
        let span = self.tree(module).node(syntax).unwrap().span();
        let steps = self.significant[module.index()]
            .len()
            .checked_ilog2()
            .unwrap_or(0)
            + 1;
        for _ in 0..steps {
            self.charge(span, 0)?;
        }
        let starts = &self.significant[module.index()];
        let index = starts.partition_point(|start| *start < span.range().start);
        let start = starts
            .get(index)
            .copied()
            .filter(|start| *start < span.range().end)
            .unwrap_or(span.range().start);
        self.sources
            .span(span.source(), start..span.range().end)
            .map_err(|error| Abort::Fatal(BindingError::Source(error)))
    }
    fn limit(&self, span: Span, message: &str) -> Abort {
        Abort::Fatal(BindingError::Limit(Box::new(Diagnostic::new(
            codes::RESOURCE_LIMIT,
            Severity::Error,
            message,
            Label::new(span, message),
        ))))
    }
    fn charge(&mut self, span: Span, depth: usize) -> BindResult<()> {
        if self.work >= self.limits.work {
            return Err(self.limit(span, "source binding work limit exceeded"));
        }
        if depth >= self.limits.depth.min(256) {
            return Err(self.limit(span, "source binding depth limit exceeded"));
        }
        self.work += 1;
        Ok(())
    }
    fn reserve(&mut self, arena: Arena, span: Span) -> BindResult<()> {
        let (count, limit) = match arena {
            Arena::Local => (&mut self.locals, self.limits.locals),
            Arena::Expression => (&mut self.expressions, self.limits.expressions),
            Arena::Pattern => (&mut self.patterns, self.limits.patterns),
            Arena::Block => (&mut self.blocks, self.limits.blocks),
        };
        if *count >= limit {
            return Err(self.limit(span, "resolved source arena limit exceeded"));
        }
        *count += 1;
        Ok(())
    }
    fn report(&mut self, diagnostic: Diagnostic) -> BindResult<()> {
        if self.diagnostics.len() >= self.limits.diagnostics {
            return Err(self.limit(
                diagnostic.primary.span,
                "source binding diagnostic limit exceeded",
            ));
        }
        self.diagnostics.push(diagnostic);
        Err(Abort::Recover)
    }
    fn fail<T>(
        &mut self,
        code: DiagnosticCode,
        span: Span,
        message: &str,
        related: Option<Span>,
    ) -> BindResult<T> {
        let mut diagnostic =
            Diagnostic::new(code, Severity::Error, message, Label::new(span, message));
        if let Some(span) = related {
            diagnostic.secondary.push(Label::new(span, "declared here"));
        }
        self.report(diagnostic)?;
        unreachable!()
    }
    fn resolve(&mut self, module: ModuleId, path: NodeId) -> BindResult<ResolvedName> {
        let span = self.tree(module).node(path).unwrap().span();
        self.charge(span, 0)?;
        let mut remaining = self.limits.work.saturating_sub(self.work);
        let result = self.graph.resolve_charged(module, path, &mut remaining);
        self.work = self.limits.work - remaining;
        match result {
            Ok(name) => Ok(name),
            Err(ResolutionError::Diagnostic(d)) => {
                self.report(*d)?;
                unreachable!()
            }
            Err(ResolutionError::Limit(d)) => Err(Abort::Fatal(BindingError::Limit(d))),
            Err(ResolutionError::InvalidInput) => {
                unreachable!("path comes from the original parsed module")
            }
        }
    }
    fn ty(&mut self, module: ModuleId, syntax: NodeId, public: bool) -> BindResult<r::TypeUse> {
        self.ty_at(module, syntax, public, 0)
    }
    fn intern(&mut self, constructor: r::TypeConstructor, span: Span) -> BindResult<r::TypeUse> {
        self.charge(span, 0)?;
        let id = if let Some(id) = self.type_ids.get(&constructor) {
            *id
        } else {
            if self.types.len() >= self.limits.types {
                return Err(self.limit(span, "constructed type instance limit exceeded"));
            }
            self.types
                .try_reserve(1)
                .map_err(|_| self.limit(span, "constructed type allocation failed"))?;
            let id = r::ConstructedTypeId(self.types.len());
            self.types.push(r::ConstructedType { constructor, span });
            self.type_ids.insert(constructor, id);
            id
        };
        Ok(r::TypeUse {
            value_type: r::Type::Constructed(id),
            span,
        })
    }
    fn ty_at(
        &mut self,
        module: ModuleId,
        syntax: NodeId,
        public: bool,
        depth: usize,
    ) -> BindResult<r::TypeUse> {
        let tree = self.tree(module);
        let span = self.semantic_span(module, syntax)?;
        self.charge(span, depth)?;
        if let Some(modifier) = child_nodes(tree, syntax).find(|id| {
            matches!(
                tree.node(*id).unwrap().kind(),
                S::LoanType | S::SharedType | S::WeakType | S::ArenaType
            )
        }) {
            let node = tree.node(modifier).unwrap();
            let inner = child_nodes(tree, modifier)
                .next()
                .expect("parsed modifier has a type");
            let kind = node.kind();
            let mutable = direct_tokens(tree, modifier).any(|t| t.kind == K::Mut);
            let inner = self.ty_at(module, inner, public, depth + 1)?.value_type;
            let constructor = match kind {
                S::LoanType => r::TypeConstructor::Loan(
                    if mutable {
                        r::LoanKind::Exclusive
                    } else {
                        r::LoanKind::Shared
                    },
                    inner,
                ),
                S::SharedType => r::TypeConstructor::Shared(inner),
                S::WeakType => r::TypeConstructor::Weak(inner),
                S::ArenaType => r::TypeConstructor::Arena(inner),
                _ => unreachable!(),
            };
            return self.intern(constructor, span);
        }
        for token in direct_tokens(tree, syntax) {
            match token.kind {
                K::I64 => {
                    return Ok(r::TypeUse {
                        value_type: r::Type::I64,
                        span,
                    });
                }
                K::Bool => {
                    return Ok(r::TypeUse {
                        value_type: r::Type::Bool,
                        span,
                    });
                }
                _ => {}
            }
        }
        let path = first_path(tree, syntax);
        let arguments =
            child_nodes(tree, syntax).find(|id| tree.node(*id).unwrap().kind() == S::TypeArguments);
        let names: Vec<_> = direct_tokens(tree, path)
            .filter(|t| t.kind == K::Identifier)
            .map(|t| self.sources.slice(t.span).expect("validated source token"))
            .collect();
        if names.first() == Some(&"core") {
            let name = if let [_, name] = names.as_slice() {
                *name
            } else {
                return self.fail(
                    codes::UNSUPPORTED_SPECIALIZATION,
                    span,
                    "unknown core type path",
                    None,
                );
            };
            let arity = match name {
                "Option" | "Vec" => 1,
                "Result" | "Map" => 2,
                _ if r::BuiltinType::from_name(name).is_some() => 0,
                _ => {
                    return self.fail(
                        codes::UNSUPPORTED_SPECIALIZATION,
                        span,
                        "unknown core type",
                        None,
                    );
                }
            };
            let mut parts = [None, None];
            let mut count = 0usize;
            if let Some(arguments) = arguments {
                for node in child_nodes(tree, arguments) {
                    self.charge(tree.node(node).unwrap().span(), depth)?;
                    if count >= 2 {
                        return self.fail(
                            codes::UNSUPPORTED_SPECIALIZATION,
                            span,
                            "invalid built-in type arity",
                            None,
                        );
                    }
                    parts[count] = Some(node);
                    count += 1;
                }
            }
            if count != arity || (arity == 0 && arguments.is_some()) {
                return self.fail(
                    codes::UNSUPPORTED_SPECIALIZATION,
                    span,
                    "invalid built-in type arity",
                    None,
                );
            }
            let mut values = [r::Type::I64; 2];
            for i in 0..arity {
                values[i] = self
                    .ty_at(module, parts[i].unwrap(), public, depth + 1)?
                    .value_type;
            }
            let constructor = match name {
                "Option" => r::TypeConstructor::Option(values[0]),
                "Result" => r::TypeConstructor::Result(values[0], values[1]),
                "Vec" => r::TypeConstructor::Vec(values[0]),
                "Map" => r::TypeConstructor::Map(values[0], values[1]),
                _ => r::TypeConstructor::Builtin(r::BuiltinType::from_name(name).unwrap()),
            };
            return self.intern(constructor, span);
        }
        if arguments.is_some() {
            return self.fail(
                codes::UNSUPPORTED_SPECIALIZATION,
                span,
                "only built-in core types take type arguments",
                None,
            );
        }
        let target = self.resolve(module, path)?;
        if let ResolvedName::Item(id) = target
            && let Some(definition) = self.definitions.get(&id).copied()
        {
            let item = self.graph.item(id).unwrap();
            if public && !item.is_public() {
                return self.fail(
                    codes::MODULE_ERROR,
                    span,
                    "public signature contains a private type",
                    Some(item.name().span),
                );
            }
            return Ok(r::TypeUse {
                value_type: r::Type::Nominal(definition),
                span,
            });
        }
        self.fail(
            codes::TYPE_MISMATCH,
            span,
            "expected a nominal type declaration",
            None,
        )
    }
    fn nominal(&mut self, module: ModuleId, path: NodeId) -> BindResult<r::DefinitionId> {
        if let ResolvedName::Item(id) = self.resolve(module, path)?
            && let Some(id) = self.definitions.get(&id)
        {
            return Ok(*id);
        }
        self.fail(
            codes::INVALID_SHAPE,
            self.tree(module).node(path).unwrap().span(),
            "record syntax requires a nominal type",
            None,
        )
    }
    fn core_name(&mut self, module: ModuleId, path: NodeId) -> BindResult<Option<CoreName>> {
        let tree = self.tree(module);
        let span = self.semantic_span(module, path)?;
        self.charge(span, 0)?;
        let mut names = direct_tokens(tree, path).filter(|t| t.kind == K::Identifier);
        let first = names.next().expect("parsed path has a name");
        if self.sources.slice(first.span).unwrap() != "core" {
            return Ok(None);
        }
        let second = names
            .next()
            .map(|token| self.sources.slice(token.span).unwrap());
        let third = names
            .next()
            .map(|token| self.sources.slice(token.span).unwrap());
        let name = match (second, third, names.next()) {
            (Some("Unit"), None, None) => Some(CoreName::Unit),
            (Some(name), None, None) => core::Operation::from_name(name).map(CoreName::Operation),
            (Some(family), Some(name), None) => {
                core::Case::from_path(family, name).map(CoreName::Case)
            }
            _ => None,
        };
        if name.is_none() {
            return self.fail(
                codes::UNSUPPORTED_SPECIALIZATION,
                span,
                "unknown core value or operation",
                None,
            );
        }
        Ok(name)
    }
    fn case_shape(
        &mut self,
        case: core::Case,
        explicit: bool,
        count: usize,
        span: Span,
    ) -> BindResult<()> {
        self.charge(span, 0)?;
        if count != case.payload().len() || explicit == case.payload().is_empty() {
            return self.fail(
                codes::INVALID_SHAPE,
                span,
                "core case requires its exact payload shape; zero-payload cases omit parentheses",
                None,
            );
        }
        Ok(())
    }
    fn case(&mut self, module: ModuleId, path: NodeId) -> BindResult<(r::DefinitionId, usize)> {
        if let ResolvedName::Case { definition, index } = self.resolve(module, path)? {
            return Ok((self.definitions[&definition], index));
        }
        self.fail(
            codes::INVALID_SHAPE,
            self.tree(module).node(path).unwrap().span(),
            "expected an enum case",
            None,
        )
    }
    fn definition(&mut self, item: &Item) -> BindResult<r::Definition> {
        let tree = self.tree(item.module());
        let syntax = item.syntax();
        self.charge(tree.node(syntax).unwrap().span(), 0)?;
        let mut fields = Vec::new();
        let mut cases = Vec::new();
        for child in child_nodes(tree, syntax) {
            match tree.node(child).unwrap().kind() {
                S::RecordField => {
                    let ty = child_nodes(tree, child)
                        .find(|id| tree.node(*id).unwrap().kind() == S::Type)
                        .unwrap();
                    fields.push(r::Field {
                        name: direct_name(self.sources, tree, child),
                        value_type: self.ty(item.module(), ty, item.is_public())?,
                    });
                }
                S::EnumCase => {
                    let mut payload = Vec::new();
                    for list in child_nodes(tree, child) {
                        for ty in child_nodes(tree, list) {
                            payload.push(self.ty(item.module(), ty, item.is_public())?);
                        }
                    }
                    cases.push(r::Case {
                        name: direct_name(self.sources, tree, child),
                        payload,
                    });
                }
                _ => {}
            }
        }
        let kind = if matches!(item.kind(), ItemKind::Record) {
            r::DefinitionKind::Record(fields)
        } else {
            r::DefinitionKind::Enum(cases)
        };
        Ok(r::Definition {
            module: r::ModuleId(item.module().index()),
            name: item_name(item),
            qualified_name: item.qualified_name().into(),
            public: item.is_public(),
            kind,
            span: self.semantic_span(item.module(), syntax)?,
        })
    }
    fn host(&mut self, item: &Item) -> BindResult<r::HostImport> {
        let tree = self.tree(item.module());
        self.charge(tree.node(item.syntax()).unwrap().span(), 0)?;
        let parameters = child_nodes(tree, item.syntax())
            .find(|id| tree.node(*id).unwrap().kind() == S::Parameters)
            .unwrap();
        let mut signature = Vec::new();
        for parameter in child_nodes(tree, parameters) {
            self.charge(tree.node(parameter).unwrap().span(), 0)?;
            let ty = child_nodes(tree, parameter).next().unwrap();
            signature.push(r::ImportParameter {
                name: direct_name(self.sources, tree, parameter),
                ty: self.ty(item.module(), ty, false)?,
            });
        }
        let result = child_nodes(tree, item.syntax())
            .find(|id| tree.node(*id).unwrap().kind() == S::Type)
            .unwrap();
        Ok(r::HostImport {
            abi: if tree.node(item.syntax()).unwrap().kind() == S::ForeignImport {
                r::ImportAbi::C
            } else {
                r::ImportAbi::Host
            },
            module: r::ModuleId(item.module().index()),
            name: item_name(item),
            qualified_name: item.qualified_name().into(),
            parameters: signature,
            result: self.ty(item.module(), result, false)?,
            span: self.semantic_span(item.module(), item.syntax())?,
        })
    }
}

struct FunctionBinder<'a, 'b> {
    context: &'b mut Context<'a>,
    module: ModuleId,
    function: r::Function,
    scopes: Vec<BTreeMap<String, r::LocalId>>,
    declared: BTreeMap<String, Span>,
}

impl<'a, 'b> FunctionBinder<'a, 'b> {
    fn bind(context: &'b mut Context<'a>, item: &Item) -> BindResult<r::Function> {
        let tree = context.tree(item.module());
        let span = context.semantic_span(item.module(), item.syntax())?;
        context.charge(span, 0)?;
        let children: Vec<_> = child_nodes(tree, item.syntax()).collect();
        let parameters = children
            .iter()
            .copied()
            .find(|id| tree.node(*id).unwrap().kind() == S::Parameters)
            .unwrap();
        let result = children
            .iter()
            .copied()
            .find(|id| tree.node(*id).unwrap().kind() == S::Type)
            .unwrap();
        let body = children
            .iter()
            .copied()
            .find(|id| tree.node(*id).unwrap().kind() == S::Block)
            .unwrap();
        let result = context.ty(item.module(), result, item.is_public())?;
        let signature_span = context
            .sources
            .span(
                span.source(),
                span.range().start..context.semantic_span(item.module(), body)?.range().start,
            )
            .map_err(|e| Abort::Fatal(BindingError::Source(e)))?;
        let function = r::Function {
            abi: if tree.node(item.syntax()).unwrap().kind() == S::ForeignExport {
                r::FunctionAbi::CExport
            } else {
                r::FunctionAbi::Guest
            },
            module: r::ModuleId(item.module().index()),
            name: item_name(item),
            qualified_name: item.qualified_name().into(),
            public: item.is_public(),
            parameters: Vec::new(),
            result,
            locals: Vec::new(),
            expressions: Vec::new(),
            patterns: Vec::new(),
            blocks: Vec::new(),
            body: r::BlockId(0),
            span,
            signature_span,
        };
        let mut binder = Self {
            context,
            module: item.module(),
            function,
            scopes: vec![BTreeMap::new()],
            declared: BTreeMap::new(),
        };
        for parameter in child_nodes(tree, parameters) {
            let name = direct_name(binder.context.sources, tree, parameter);
            let ty = child_nodes(tree, parameter).next().unwrap();
            let ty = binder.context.ty(item.module(), ty, item.is_public())?;
            let local = binder.declare(name, r::BindingKind::Parameter, Some(ty))?;
            binder.function.parameters.push(local);
        }
        binder.function.body = binder.block(body, 0)?;
        Ok(binder.function)
    }
    fn tree(&self) -> &'a SyntaxTree {
        self.context.tree(self.module)
    }
    fn local(&self, name: &str) -> Option<r::LocalId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
    fn declare(
        &mut self,
        name: Name,
        kind: r::BindingKind,
        annotation: Option<r::TypeUse>,
    ) -> BindResult<r::LocalId> {
        self.context.charge(name.span, 0)?;
        if name.text == "core" {
            return self.context.fail(
                codes::UNSUPPORTED_SPECIALIZATION,
                name.span,
                "core is a reserved root namespace; rename this binding",
                None,
            );
        }
        if let Some(original) = self.declared.get(&name.text).copied() {
            return self.context.fail(
                codes::DUPLICATE_NAME,
                name.span,
                "name is already used in this function",
                Some(original),
            );
        }
        self.context.reserve(Arena::Local, name.span)?;
        let id = r::LocalId(self.function.locals.len());
        self.declared.insert(name.text.clone(), name.span);
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.text.clone(), id);
        self.function.locals.push(r::Local {
            name,
            kind,
            annotation,
        });
        Ok(id)
    }
    fn local_path(&self, path: NodeId) -> Option<r::LocalId> {
        let tree = self.tree();
        let mut names = direct_tokens(tree, path).filter(|t| !t.kind.is_trivia());
        let first = names.next()?;
        if first.kind != K::Identifier {
            return None;
        }
        self.local(self.context.sources.slice(first.span).unwrap())
    }
    fn value_name(&mut self, path: NodeId) -> BindResult<ResolvedName> {
        let tree = self.tree();
        let tokens: Vec<_> = direct_tokens(tree, path)
            .filter(|t| !t.kind.is_trivia())
            .collect();
        if tokens.len() == 1 && tokens[0].kind == K::Identifier {
            let text = self.context.sources.slice(tokens[0].span).unwrap();
            if self
                .context
                .graph
                .module(self.module)
                .unwrap()
                .find(text)
                .is_none()
            {
                return self.context.fail(
                    codes::UNKNOWN_NAME,
                    tokens[0].span,
                    "unknown local or module value",
                    None,
                );
            }
        }
        self.context.resolve(self.module, path)
    }
    fn reject_local_path(&mut self, path: NodeId) -> BindResult<()> {
        if let Some(local) = self.local_path(path) {
            return self.context.fail(
                codes::TYPE_MISMATCH,
                self.tree().node(path).unwrap().span(),
                "a local value is not a callable or namespace",
                Some(self.function.locals[local.0].name.span),
            );
        }
        Ok(())
    }
    fn block(&mut self, syntax: NodeId, depth: usize) -> BindResult<r::BlockId> {
        let tree = self.tree();
        let span = self.context.semantic_span(self.module, syntax)?;
        self.context.charge(span, depth)?;
        self.context.reserve(Arena::Block, span)?;
        self.scopes.push(BTreeMap::new());
        let mut statements = Vec::new();
        for node in child_nodes(tree, syntax) {
            statements.push(self.statement(node, depth + 1)?);
        }
        self.scopes.pop();
        let closing_brace = direct_tokens(tree, syntax)
            .find(|t| t.kind == K::RightBrace)
            .unwrap()
            .span;
        let id = r::BlockId(self.function.blocks.len());
        self.function.blocks.push(r::Block {
            statements,
            span,
            closing_brace,
        });
        Ok(id)
    }
    fn statement(&mut self, syntax: NodeId, depth: usize) -> BindResult<r::Statement> {
        let tree = self.tree();
        let node = tree.node(syntax).unwrap();
        let span = self.context.semantic_span(self.module, syntax)?;
        self.context.charge(span, depth)?;
        let children: Vec<_> = child_nodes(tree, syntax).collect();
        let kind = match node.kind() {
            S::UnsafeBlock => r::StatementKind::Unsafe {
                body: self.block(children[0], depth + 1)?,
            },
            S::Let => {
                let annotation = self.context.ty(self.module, children[0], false)?;
                let initializer = self.expression(children[1], depth + 1)?;
                let local = self.declare(
                    direct_name(self.context.sources, tree, syntax),
                    r::BindingKind::Mutable,
                    Some(annotation),
                )?;
                r::StatementKind::LetMut { local, initializer }
            }
            S::Assign => {
                let name = direct_name(self.context.sources, tree, syntax);
                let Some(local) = self.local(&name.text) else {
                    return self.context.fail(
                        codes::UNKNOWN_NAME,
                        name.span,
                        "unknown assignment target",
                        None,
                    );
                };
                if self.function.locals[local.0].kind != r::BindingKind::Mutable {
                    return self.context.fail(
                        codes::IMMUTABLE_ASSIGNMENT,
                        name.span,
                        "binding is immutable",
                        Some(self.function.locals[local.0].name.span),
                    );
                }
                r::StatementKind::Assign {
                    local,
                    value: self.expression(children[0], depth + 1)?,
                }
            }
            S::PlaceAssign => {
                let place = self.place(children[0], depth + 1)?;
                let value = self.expression(children[1], depth + 1)?;
                r::StatementKind::PlaceAssign { place, value }
            }
            S::Arena => {
                // The owner is not visible in its own allocator expression.
                let allocator = self.expression(children[0], depth + 1)?;
                self.scopes.push(BTreeMap::new());
                let owner = self.declare(
                    direct_name(self.context.sources, tree, syntax),
                    r::BindingKind::ArenaOwner,
                    None,
                )?;
                let body = self.block(children[1], depth + 1)?;
                self.scopes.pop();
                r::StatementKind::Arena {
                    owner,
                    allocator,
                    body,
                }
            }
            S::Return => r::StatementKind::Return(self.expression(children[0], depth + 1)?),
            S::If => {
                let condition = self.expression(children[0], depth + 1)?;
                let then_block = self.block(children[1], depth + 1)?;
                let else_block = if children.len() > 2 {
                    Some(self.block(children[2], depth + 1)?)
                } else {
                    None
                };
                r::StatementKind::If {
                    condition,
                    then_block,
                    else_block,
                }
            }
            S::While => {
                let condition = self.expression(children[0], depth + 1)?;
                let body = self.block(children[1], depth + 1)?;
                r::StatementKind::While { condition, body }
            }
            S::Match => {
                let scrutinee = self.expression(children[0], depth + 1)?;
                let mut arms = Vec::new();
                for arm in children.iter().skip(1) {
                    self.context
                        .charge(tree.node(*arm).unwrap().span(), depth + 1)?;
                    let parts: Vec<_> = child_nodes(tree, *arm).collect();
                    self.scopes.push(BTreeMap::new());
                    let pattern = self.pattern(parts[0], depth + 2)?;
                    let body = self.block(parts[1], depth + 2)?;
                    self.scopes.pop();
                    arms.push(r::Arm {
                        pattern,
                        body,
                        span: self.context.semantic_span(self.module, *arm)?,
                    });
                }
                r::StatementKind::Match { scrutinee, arms }
            }
            _ => unreachable!("successful parser statement"),
        };
        Ok(r::Statement { kind, span })
    }
    fn expression(&mut self, syntax: NodeId, depth: usize) -> BindResult<r::ExpressionId> {
        let tree = self.tree();
        let node = tree.node(syntax).unwrap();
        let span = self.context.semantic_span(self.module, syntax)?;
        self.context.charge(span, depth)?;
        let children: Vec<_> = child_nodes(tree, syntax).collect();
        let kind = match node.kind() {
            S::Borrow => {
                let kind = if direct_tokens(tree, syntax).any(|t| t.kind == K::Mut) {
                    r::LoanKind::Exclusive
                } else {
                    r::LoanKind::Shared
                };
                r::ExpressionKind::Borrow {
                    kind,
                    place: self.place(children[0], depth + 1)?,
                }
            }
            S::Dereference => {
                r::ExpressionKind::Dereference(self.expression(children[0], depth + 1)?)
            }
            S::Literal => match direct_tokens(tree, syntax)
                .find(|t| !t.kind.is_trivia())
                .unwrap()
                .kind
            {
                K::Integer(value) => r::ExpressionKind::Integer(value),
                K::True => r::ExpressionKind::Boolean(true),
                K::False => r::ExpressionKind::Boolean(false),
                _ => unreachable!("valid literal"),
            },
            S::Parenthesized => {
                let id = self.expression(children[0], depth + 1)?;
                self.function.expressions[id.0].span = span;
                return Ok(id);
            }
            S::Name => {
                let path = children[0];
                let count = direct_tokens(tree, path)
                    .filter(|t| !t.kind.is_trivia())
                    .count();
                if let Some(name) = self.context.core_name(self.module, path)? {
                    if let CoreName::Case(case) = name {
                        self.core_case_value(case, path, None, span, depth + 1)?
                    } else {
                        return self.context.fail(codes::INVALID_SHAPE, span, "core operation requires call syntax; Unit requires empty record braces", None);
                    }
                } else if let Some(local) = self.local_path(path) {
                    if count == 1 {
                        r::ExpressionKind::Local(local)
                    } else {
                        self.reject_local_path(path)?;
                        unreachable!()
                    }
                } else {
                    match self.value_name(path)? {
                        ResolvedName::Case { definition, index } => r::ExpressionKind::Case {
                            definition: self.context.definitions[&definition],
                            index,
                        },
                        ResolvedName::Item(id) => {
                            return self.context.fail(
                                codes::TYPE_MISMATCH,
                                span,
                                "module declaration is not a value",
                                Some(self.context.graph.item(id).unwrap().name().span),
                            );
                        }
                    }
                }
            }
            S::Add | S::LessThan => {
                let left = self.expression(children[0], depth + 1)?;
                let right = self.expression(children[1], depth + 1)?;
                if node.kind() == S::Add {
                    r::ExpressionKind::Add(left, right)
                } else {
                    r::ExpressionKind::LessThan(left, right)
                }
            }
            S::Field => r::ExpressionKind::Field {
                receiver: self.expression(children[0], depth + 1)?,
                name: direct_name(self.context.sources, tree, syntax),
            },
            S::Call | S::SpecializedCall => {
                let path = first_path(tree, children[0]);
                self.reject_local_path(path)?;
                if let Some(name) = self.context.core_name(self.module, path)? {
                    match name {
                        CoreName::Operation(operation) => {
                            self.core_call(operation, path, syntax, depth + 1)?
                        }
                        CoreName::Case(case) => {
                            if node.kind() == S::SpecializedCall {
                                return self.context.fail(
                                    codes::UNSUPPORTED_SPECIALIZATION,
                                    span,
                                    "core case type arguments come from expected context",
                                    None,
                                );
                            }
                            self.core_case_value(case, path, Some(children[1]), span, depth + 1)?
                        }
                        CoreName::Unit => {
                            return self.context.fail(
                                codes::INVALID_SHAPE,
                                span,
                                "Unit requires empty record braces",
                                None,
                            );
                        }
                    }
                } else {
                    if node.kind() == S::SpecializedCall {
                        return self.context.fail(
                            codes::UNSUPPORTED_SPECIALIZATION,
                            span,
                            "only listed core operations take explicit type arguments",
                            None,
                        );
                    }
                    let callee = match self.value_name(path)? {
                        ResolvedName::Case { definition, index } => r::Callable::Case {
                            definition: self.context.definitions[&definition],
                            index,
                        },
                        ResolvedName::Item(id) => {
                            if let Some(id) = self.context.functions.get(&id) {
                                r::Callable::Guest(*id)
                            } else if let Some(id) = self.context.imports.get(&id) {
                                r::Callable::Host(*id)
                            } else {
                                return self.context.fail(
                                    codes::TYPE_MISMATCH,
                                    span,
                                    "declaration is not callable",
                                    Some(self.context.graph.item(id).unwrap().name().span),
                                );
                            }
                        }
                    };
                    let mut arguments = Vec::new();
                    for argument in child_nodes(tree, children[1]) {
                        arguments.push(self.expression(argument, depth + 1)?);
                    }
                    r::ExpressionKind::Call { callee, arguments }
                }
            }
            S::Construction => {
                let path = first_path(tree, children[0]);
                self.reject_local_path(path)?;
                if let Some(name) = self.context.core_name(self.module, path)? {
                    if name != CoreName::Unit || children.len() != 1 {
                        return self.context.fail(
                            codes::INVALID_SHAPE,
                            span,
                            "only core::Unit has an empty core record construction",
                            None,
                        );
                    }
                    r::ExpressionKind::CoreUnit
                } else {
                    let definition = self.context.nominal(self.module, path)?;
                    let mut fields = Vec::new();
                    for field in children.iter().skip(1) {
                        let value = child_nodes(tree, *field).next().unwrap();
                        fields.push(r::FieldValue {
                            name: direct_name(self.context.sources, tree, *field),
                            value: self.expression(value, depth + 1)?,
                        });
                    }
                    r::ExpressionKind::Record { definition, fields }
                }
            }
            _ => unreachable!("successful parser expression"),
        };
        self.context.reserve(Arena::Expression, span)?;
        let id = r::ExpressionId(self.function.expressions.len());
        self.function.expressions.push(r::Expression { kind, span });
        Ok(id)
    }
    fn core_case_value(
        &mut self,
        case: core::Case,
        path: NodeId,
        arguments: Option<NodeId>,
        span: Span,
        depth: usize,
    ) -> BindResult<r::ExpressionKind> {
        let tree = self.tree();
        self.context.charge(span, depth)?;
        let count = arguments.map_or(0, |list| child_nodes(tree, list).count());
        self.context
            .case_shape(case, arguments.is_some(), count, span)?;
        let mut payload = Vec::new();
        if let Some(list) = arguments {
            for node in child_nodes(tree, list) {
                payload.push(self.expression(node, depth + 1)?);
            }
        }
        Ok(r::ExpressionKind::CoreCase {
            case,
            payload,
            explicit_payload: arguments.is_some(),
            case_span: self.context.semantic_span(self.module, path)?,
        })
    }
    fn core_call(
        &mut self,
        operation: core::Operation,
        path: NodeId,
        syntax: NodeId,
        depth: usize,
    ) -> BindResult<r::ExpressionKind> {
        let tree = self.tree();
        let span = self.context.semantic_span(self.module, syntax)?;
        self.context.charge(span, depth)?;
        let signature = operation.signature();
        let types =
            child_nodes(tree, syntax).find(|id| tree.node(*id).unwrap().kind() == S::TypeArguments);
        let arguments = child_nodes(tree, syntax)
            .find(|id| tree.node(*id).unwrap().kind() == S::Arguments)
            .expect("parsed call has arguments");
        let count = types.map_or(0, |types| child_nodes(tree, types).count());
        if count != signature.type_arguments.len() || (count == 0 && types.is_some()) {
            return self.context.fail(
                codes::UNSUPPORTED_SPECIALIZATION,
                span,
                "incorrect number of explicit core type arguments",
                None,
            );
        }
        if child_nodes(tree, arguments).count() != signature.parameters.len() {
            return self.context.fail(
                codes::TYPE_MISMATCH,
                span,
                "incorrect number of core value arguments",
                None,
            );
        }
        let mut type_arguments = Vec::new();
        if let Some(types) = types {
            for ty in child_nodes(tree, types) {
                type_arguments.push(self.context.ty(self.module, ty, false)?);
            }
        }
        let mut values = Vec::new();
        for argument in child_nodes(tree, arguments) {
            values.push(self.expression(argument, depth + 1)?);
        }
        Ok(r::ExpressionKind::CoreCall {
            operation,
            type_arguments,
            arguments: values,
            callee_span: self.context.semantic_span(self.module, path)?,
        })
    }
    fn place(&mut self, syntax: NodeId, depth: usize) -> BindResult<r::PlaceId> {
        let tree = self.tree();
        let node = tree.node(syntax).unwrap();
        let span = self.context.semantic_span(self.module, syntax)?;
        self.context.charge(span, depth)?;
        let kind = match node.kind() {
            S::Parenthesized => {
                let place = self.place(child_nodes(tree, syntax).next().unwrap(), depth + 1)?;
                self.function.expressions[place.0].span = span;
                return Ok(place);
            }
            S::Name => {
                let path = first_path(tree, syntax);
                let count = direct_tokens(tree, path)
                    .filter(|t| !t.kind.is_trivia())
                    .count();
                let Some(local) = self.local_path(path).filter(|_| count == 1) else {
                    let name = path_name(self.context.sources, tree, path);
                    let code = if count == 1
                        && self
                            .context
                            .graph
                            .module(self.module)
                            .unwrap()
                            .find(&name.text)
                            .is_none()
                    {
                        codes::UNKNOWN_NAME
                    } else {
                        codes::INVALID_LOAN
                    };
                    return self.context.fail(
                        code,
                        span,
                        "place must name a visible local owner or loan",
                        None,
                    );
                };
                r::ExpressionKind::Local(local)
            }
            S::Field => {
                let receiver = self.place(child_nodes(tree, syntax).next().unwrap(), depth + 1)?;
                r::ExpressionKind::Field {
                    receiver: r::ExpressionId(receiver.0),
                    name: direct_name(self.context.sources, tree, syntax),
                }
            }
            S::Dereference => r::ExpressionKind::Dereference(
                self.expression(child_nodes(tree, syntax).next().unwrap(), depth + 1)?,
            ),
            _ => {
                return self.context.fail(
                    codes::INVALID_LOAN,
                    span,
                    "borrow and assignment require a place, not a temporary value",
                    None,
                );
            }
        };
        self.context.reserve(Arena::Expression, span)?;
        let id = r::PlaceId(self.function.expressions.len());
        self.function.expressions.push(r::Expression { kind, span });
        Ok(id)
    }
    fn pattern(&mut self, syntax: NodeId, depth: usize) -> BindResult<r::PatternId> {
        let tree = self.tree();
        let node = tree.node(syntax).unwrap();
        let span = self.context.semantic_span(self.module, syntax)?;
        self.context.charge(span, depth)?;
        let children: Vec<_> = child_nodes(tree, syntax).collect();
        let kind = match node.kind() {
            S::WildcardPattern => r::PatternKind::Wildcard,
            S::BindingPattern => {
                let name = path_name(self.context.sources, tree, children[0]);
                r::PatternKind::Binding(self.declare(name, r::BindingKind::Pattern, None)?)
            }
            S::LiteralPattern => match direct_tokens(tree, syntax)
                .find(|t| !t.kind.is_trivia())
                .unwrap()
                .kind
            {
                K::Integer(value) => r::PatternKind::Integer(value),
                K::True => r::PatternKind::Boolean(true),
                K::False => r::PatternKind::Boolean(false),
                _ => unreachable!("valid literal pattern"),
            },
            S::RecordPattern => {
                let path = first_path(tree, children[0]);
                if let Some(name) = self.context.core_name(self.module, path)? {
                    if name != CoreName::Unit || children.len() != 1 {
                        return self.context.fail(
                            codes::INVALID_SHAPE,
                            span,
                            "only core::Unit has an empty core record pattern",
                            None,
                        );
                    }
                    r::PatternKind::CoreUnit
                } else {
                    let definition = self
                        .context
                        .nominal(self.module, first_path(tree, children[0]))?;
                    let mut fields = Vec::new();
                    for field in children.iter().skip(1) {
                        let pattern = child_nodes(tree, *field).next().unwrap();
                        fields.push(r::PatternField {
                            name: direct_name(self.context.sources, tree, *field),
                            pattern: self.pattern(pattern, depth + 1)?,
                        });
                    }
                    r::PatternKind::Record { definition, fields }
                }
            }
            S::CasePattern => {
                let path = first_path(tree, children[0]);
                if let Some(name) = self.context.core_name(self.module, path)? {
                    let CoreName::Case(case) = name else {
                        return self.context.fail(
                            codes::INVALID_SHAPE,
                            span,
                            "pattern requires a core case or Unit record",
                            None,
                        );
                    };
                    let explicit_payload = children.len() > 1;
                    let count = if explicit_payload {
                        child_nodes(tree, children[1]).count()
                    } else {
                        0
                    };
                    self.context
                        .case_shape(case, explicit_payload, count, span)?;
                    let mut payload = Vec::new();
                    if explicit_payload {
                        for node in child_nodes(tree, children[1]) {
                            payload.push(self.pattern(node, depth + 1)?);
                        }
                    }
                    r::PatternKind::CoreCase {
                        case,
                        payload,
                        explicit_payload,
                        case_span: self.context.semantic_span(self.module, path)?,
                    }
                } else {
                    let (definition, index) = self
                        .context
                        .case(self.module, first_path(tree, children[0]))?;
                    let explicit_payload = children.len() > 1;
                    let mut payload = Vec::new();
                    if explicit_payload {
                        for pattern in child_nodes(tree, children[1]) {
                            payload.push(self.pattern(pattern, depth + 1)?);
                        }
                    }
                    r::PatternKind::Case {
                        definition,
                        index,
                        payload,
                        explicit_payload,
                    }
                }
            }
            _ => unreachable!("successful parser pattern"),
        };
        self.context.reserve(Arena::Pattern, span)?;
        let id = r::PatternId(self.function.patterns.len());
        self.function.patterns.push(r::Pattern { kind, span });
        Ok(id)
    }
}

fn item_name(item: &Item) -> Name {
    Name {
        text: item.name().text.clone(),
        span: item.name().span,
    }
}
fn direct_tokens(tree: &SyntaxTree, node: NodeId) -> impl Iterator<Item = lang_syntax::Token> + '_ {
    tree.node(node)
        .unwrap()
        .children()
        .iter()
        .filter_map(|element| match element {
            SyntaxElement::Token(index) => Some(tree.tokens()[*index]),
            _ => None,
        })
}
fn direct_name(sources: &SourceMap, tree: &SyntaxTree, node: NodeId) -> Name {
    let token = direct_tokens(tree, node)
        .find(|token| token.kind == K::Identifier)
        .expect("named parser node");
    Name {
        text: sources.slice(token.span).unwrap().into(),
        span: token.span,
    }
}
fn path_name(sources: &SourceMap, tree: &SyntaxTree, node: NodeId) -> Name {
    direct_name(sources, tree, node)
}
fn first_path(tree: &SyntaxTree, node: NodeId) -> NodeId {
    if tree.node(node).unwrap().kind() == S::Path {
        node
    } else {
        first_path(tree, child_nodes(tree, node).next().expect("path wrapper"))
    }
}
