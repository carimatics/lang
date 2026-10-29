//! Bounded advisory completion using normal parser/binder facts.
use super::workspace::View;
use super::{
    Diagnostic, Location, Overlay, Snapshot, SymbolKind, capacity, invalid, nominal, spend,
    spend_bytes,
};
use lang_diagnostics::SourceId;
use lang_hir::resolved::{self as r, BindingKind, DefinitionKind, ExpressionKind, StatementKind};
use lang_semantics::{
    binding::bind_program,
    modules::{ItemKind, ModuleId, ResolvedName},
};
use lang_syntax::{SyntaxElement, SyntaxKind, TokenKind, lex};
use std::{collections::BTreeMap, path::Path, sync::Arc};
const MARKER: &str = "__lang_completion_slot";
const MAX_CANDIDATES: usize = 256;
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub name: String,
    pub kind: SymbolKind,
    pub detail: String,
}
pub(crate) struct ResultList {
    pub replace: Location,
    pub candidates: Vec<Candidate>,
}
struct Site<'a> {
    source: SourceId,
    start: usize,
    marker_len: usize,
    prefix: &'a str,
}
pub(crate) fn complete(
    view: &View,
    path: &Path,
    offset: usize,
    epoch: u64,
) -> Result<ResultList, Diagnostic> {
    if epoch != view.snapshot.epoch() {
        return Err(Diagnostic::error("PKG0007", "completion epoch is stale"));
    }
    let (source, file) = view.source(path)?;
    let text = file.text();
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err(invalid("invalid completion source boundary"));
    }
    let tokens = lex(
        &view.snapshot.analysis().sources,
        source,
        view.limits.compiler.modules.parse.lex,
    )
    .map_err(|_| capacity())?
    .tokens;
    let word = tokens.iter().find(|t| {
        t.kind == TokenKind::Identifier
            && t.span.range().start <= offset
            && offset <= t.span.range().end
    });
    let (start, end) = word
        .map(|t| (t.span.range().start, t.span.range().end))
        .unwrap_or((offset, offset));
    let replace = Location {
        path: view
            .snapshot
            .display_path(view.relative(path)?)
            .to_path_buf(),
        span: super::ByteSpan { start, end },
    };
    if tokens.iter().any(|t| {
        t.span.range().start <= offset
            && offset < t.span.range().end
            && matches!(
                t.kind,
                TokenKind::LineComment
                    | TokenKind::BlockComment
                    | TokenKind::CAbi
                    | TokenKind::ComponentAbi
            )
    }) {
        return Ok(ResultList {
            replace,
            candidates: Vec::new(),
        });
    }
    let prefix = &text[start..offset];
    let previous = tokens
        .iter()
        .rfind(|t| !t.kind.is_trivia() && t.span.range().end <= start)
        .map(|t| t.kind);
    let member = previous == Some(TokenKind::Dot);
    let marker = if member || previous == Some(TokenKind::ColonColon) {
        MARKER
    } else {
        "0"
    };
    let mut probe = make_probe(view, path, text, start, end, marker)?;
    if probe.modules().is_none() && marker == "0" {
        probe = make_probe(view, path, text, start, end, MARKER)?;
    }
    let Some(graph) = probe.modules() else {
        return Ok(ResultList {
            replace,
            candidates: Vec::new(),
        });
    };
    let sources = &probe.analysis().sources;
    let display = view.snapshot.display_path(view.relative(path)?);
    let (id, _) = sources
        .iter()
        .find(|(_, f)| Path::new(f.name()) == display)
        .ok_or_else(|| invalid("completion probe source is unavailable"))?;
    let mut work = view.limits.work;
    let bound =
        bind_program(graph, sources, view.limits.compiler.binding).map_err(|_| capacity())?;
    let mut candidates = BTreeMap::<String, Candidate>::new();
    let site = Site {
        source: id,
        start,
        marker_len: marker.len(),
        prefix,
    };
    if member {
        if let Some(program) = bound.program.as_ref() {
            field_candidates(program, sources, &site, &mut work, &mut candidates)?;
        }
    } else {
        let module = origin(graph, id, start)?;
        let mut path_node = None;
        let mut type_position = false;
        for m in graph.modules().iter().filter(|m| m.source() == id) {
            let tree = m.tree();
            let mut pending = vec![tree.root()];
            while let Some(node) = pending.pop() {
                spend(&mut work)?;
                let n = tree.node(node).expect("validated syntax");
                if n.kind() == SyntaxKind::Type && n.span().range().contains(&start) {
                    type_position = true;
                }
                pending.extend(n.children().iter().filter_map(|e| {
                    if let SyntaxElement::Node(id) = e {
                        Some(*id)
                    } else {
                        None
                    }
                }));
                if n.kind() == SyntaxKind::Path
                    && m.owns_path(node)
                    && n.span().range().start <= start
                    && start < n.span().range().end
                {
                    path_node = Some((m.id(), node));
                }
            }
        }
        let values = if let Some((module, node)) = path_node {
            graph.complete_path(module, node, prefix, &mut work, MAX_CANDIDATES)
        } else {
            let site = sources
                .span(id, start..start + 1)
                .map_err(|_| invalid("invalid completion probe site"))?;
            graph.complete_name(module, site, prefix, &mut work, MAX_CANDIDATES)
        }
        .map_err(|_| capacity())?;
        for value in values {
            let (kind, detail) = match value.target {
                ResolvedName::Item(id) => {
                    let item = graph.item(id).expect("resolved completion");
                    (
                        match item.kind() {
                            ItemKind::Function | ItemKind::Host => SymbolKind::Function,
                            ItemKind::Record => SymbolKind::Record,
                            ItemKind::Enum { .. } => SymbolKind::Enum,
                            ItemKind::Module { .. } => SymbolKind::Module,
                            ItemKind::Import { .. } => SymbolKind::Import,
                        },
                        item.qualified_name().to_string(),
                    )
                }
                ResolvedName::Case { definition, .. } => (
                    SymbolKind::EnumCase,
                    graph
                        .item(definition)
                        .expect("resolved case")
                        .qualified_name()
                        .to_string(),
                ),
            };
            if type_position
                && !matches!(
                    kind,
                    SymbolKind::Record | SymbolKind::Enum | SymbolKind::Module | SymbolKind::Import
                )
            {
                continue;
            }
            spend_bytes(
                &mut work,
                value
                    .name
                    .len()
                    .saturating_mul(2)
                    .saturating_add(detail.len()),
            )?;
            candidates.insert(
                value.name.clone(),
                Candidate {
                    name: value.name,
                    kind,
                    detail,
                },
            );
        }
        if !type_position
            && previous != Some(TokenKind::ColonColon)
            && let Some(program) = bound.program.as_ref()
        {
            local_candidates(program, sources, &site, &mut work, &mut candidates)?;
        }
    }
    if candidates.len() > MAX_CANDIDATES {
        return Err(capacity());
    }
    Ok(ResultList {
        replace,
        candidates: candidates.into_values().collect(),
    })
}
fn make_probe(
    view: &View,
    path: &Path,
    text: &str,
    start: usize,
    end: usize,
    marker: &str,
) -> Result<Snapshot, Diagnostic> {
    let size = text
        .len()
        .checked_sub(end - start)
        .and_then(|n| n.checked_add(marker.len()))
        .ok_or_else(capacity)?;
    if size > view.limits.compiler.modules.parse.lex.source_bytes {
        return Err(capacity());
    }
    let mut text = text.to_string();
    text.replace_range(start..end, marker);
    let relative = view.relative(path)?.to_path_buf();
    let mut overlays = view.overlays.clone();
    overlays.retain(|o| o.path != relative);
    overlays.push(Overlay {
        path: relative,
        version: *view
            .snapshot
            .versions()
            .get(view.relative(path)?)
            .unwrap_or(&0),
        bytes: Arc::from(text.as_bytes()),
    });
    Snapshot::capture(&view.graph, view.snapshot.epoch(), &overlays, view.limits)
}
fn origin(
    graph: &lang_semantics::modules::ModuleGraph,
    source: SourceId,
    offset: usize,
) -> Result<ModuleId, Diagnostic> {
    let mut origin = graph
        .modules()
        .iter()
        .find(|m| m.source() == source)
        .map(|m| m.id())
        .ok_or_else(|| invalid("completion module is unavailable"))?;
    let mut width = usize::MAX;
    for item in graph.items() {
        let module = graph.module(item.module()).expect("validated module");
        if module.source() != source {
            continue;
        }
        let node = module.tree().node(item.syntax()).expect("validated item");
        let range = node.span().range();
        if range.start <= offset && offset <= range.end && range.len() < width {
            origin = match item.kind() {
                ItemKind::Module {
                    module: Some(child),
                } => *child,
                _ => item.module(),
            };
            width = range.len();
        }
    }
    Ok(origin)
}
fn field_candidates(
    program: &r::Program,
    sources: &lang_diagnostics::SourceMap,
    site: &Site<'_>,
    work: &mut usize,
    out: &mut BTreeMap<String, Candidate>,
) -> Result<(), Diagnostic> {
    for function in &program.functions {
        for expression in &function.expressions {
            spend(work)?;
            let ExpressionKind::Field { receiver, name } = &expression.kind else {
                continue;
            };
            if name.span.source() != site.source
                || name.span.range() != (site.start..site.start + site.marker_len)
            {
                continue;
            }
            let Some(ty) = declared_type(program, function, *receiver, work)? else {
                continue;
            };
            let Some(id) = nominal(program, ty, work)? else {
                continue;
            };
            let definition = &program.definitions[id.0];
            if !definition.public && definition.module != function.module {
                continue;
            }
            let DefinitionKind::Record(fields) = &definition.kind else {
                continue;
            };
            for field in fields {
                spend(work)?;
                if field.name.text.starts_with(site.prefix) {
                    if out.contains_key(&field.name.text) {
                        out.clear();
                        return Ok(());
                    }
                    if out.len() == MAX_CANDIDATES {
                        return Err(capacity());
                    }
                    let detail = sources.slice(field.value_type.span).unwrap_or("field");
                    spend_bytes(
                        work,
                        field
                            .name
                            .text
                            .len()
                            .saturating_mul(2)
                            .saturating_add(detail.len()),
                    )?;
                    out.insert(
                        field.name.text.clone(),
                        Candidate {
                            name: field.name.text.clone(),
                            kind: SymbolKind::Field,
                            detail: detail.to_string(),
                        },
                    );
                }
            }
        }
    }
    Ok(())
}
fn declared_type(
    program: &r::Program,
    function: &r::Function,
    expression: r::ExpressionId,
    work: &mut usize,
) -> Result<Option<r::Type>, Diagnostic> {
    spend(work)?;
    Ok(match &function.expressions[expression.0].kind {
        ExpressionKind::Local(id) => function.locals[id.0].annotation.map(|t| t.value_type),
        ExpressionKind::Dereference(inner) => match declared_type(program, function, *inner, work)?
        {
            Some(r::Type::Constructed(id)) => match program.types[id.0].constructor {
                r::TypeConstructor::Loan(_, inner) => Some(inner),
                _ => None,
            },
            _ => None,
        },
        ExpressionKind::Call { callee, .. } => match callee {
            r::Callable::Guest(id) => Some(program.functions[id.0].result.value_type),
            r::Callable::Host(id) => Some(program.imports[id.0].result.value_type),
            r::Callable::Case { definition, .. } => Some(r::Type::Nominal(*definition)),
        },
        _ => None,
    })
}
fn local_candidates(
    program: &r::Program,
    sources: &lang_diagnostics::SourceMap,
    site: &Site<'_>,
    work: &mut usize,
    out: &mut BTreeMap<String, Candidate>,
) -> Result<(), Diagnostic> {
    let offset = site.start;
    let mut visible = BTreeMap::<String, (usize, Candidate)>::new();
    for function in &program.functions {
        if function.span.source() != site.source || !function.span.range().contains(&offset) {
            continue;
        }
        for (index, local) in function.locals.iter().enumerate() {
            spend(work)?;
            if !local.name.text.starts_with(site.prefix) || local.kind == BindingKind::ArenaOwner {
                continue;
            }
            let mut scope = None;
            if local.kind == BindingKind::Parameter {
                scope = Some(function.span.range());
            }
            for block in &function.blocks {
                spend(work)?;
                if !block.span.range().contains(&offset) {
                    continue;
                }
                for statement in &block.statements {
                    spend(work)?;
                    if let StatementKind::LetMut { local: id, .. } = statement.kind
                        && id.0 == index
                        && statement.span.range().end <= offset
                    {
                        scope = Some(block.span.range());
                    }
                }
            }
            if local.kind == BindingKind::Pattern {
                for block in &function.blocks {
                    for statement in &block.statements {
                        spend(work)?;
                        if let StatementKind::Match { arms, .. } = &statement.kind {
                            for arm in arms {
                                spend(work)?;
                                if function.patterns[arm.pattern.0]
                                    .span
                                    .range()
                                    .contains(&local.name.span.range().start)
                                    && function.blocks[arm.body.0].span.range().contains(&offset)
                                {
                                    scope = Some(function.blocks[arm.body.0].span.range());
                                }
                            }
                        }
                    }
                }
            }
            let Some(scope) = scope else { continue };
            let detail = local
                .annotation
                .and_then(|t| sources.slice(t.span).ok())
                .unwrap_or("binding");
            spend_bytes(
                work,
                local
                    .name
                    .text
                    .len()
                    .saturating_mul(3)
                    .saturating_add(detail.len()),
            )?;
            let candidate = Candidate {
                name: local.name.text.clone(),
                kind: if local.kind == BindingKind::Parameter {
                    SymbolKind::Parameter
                } else {
                    SymbolKind::Variable
                },
                detail: detail.to_string(),
            };
            if visible
                .get(&local.name.text)
                .is_none_or(|(old, _)| scope.len() < *old)
            {
                if !visible.contains_key(&local.name.text) && visible.len() == MAX_CANDIDATES {
                    return Err(capacity());
                }
                visible.insert(local.name.text.clone(), (scope.len(), candidate));
            }
        }
    }
    for (name, (_, candidate)) in visible {
        out.insert(name, candidate);
    }
    Ok(())
}
