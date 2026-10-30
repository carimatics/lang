//! Checked identifier replacements; proposals never write source files.
mod proof;
#[cfg(test)]
mod tests;
use super::{
    ByteSpan, Diagnostic, Location, Overlay, Snapshot, Symbol, SymbolKind, capacity, invalid,
    spend, spend_bytes, workspace::View,
};
use lang_diagnostics::{SourceMap, Span};
use lang_syntax::{TokenKind, add_source, lex};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) struct Plan {
    pub replacement: String,
    pub files: BTreeMap<PathBuf, Vec<ByteSpan>>,
    offsets: BTreeMap<PathBuf, Vec<(usize, usize)>>,
}
pub(crate) fn format_document(
    view: &View,
    path: &Path,
    epoch: u64,
) -> Result<Option<Plan>, Diagnostic> {
    if epoch != view.snapshot.epoch() {
        return Err(Diagnostic::error("PKG0007", "formatting epoch is stale"));
    }
    let (id, file) = view.source(path)?;
    let graph = view
        .snapshot
        .modules()
        .ok_or_else(|| invalid("checked formatting requires a module graph"))?;
    let tree = graph
        .modules()
        .iter()
        .find(|m| m.source() == id)
        .ok_or_else(|| invalid("missing formatting syntax"))?
        .tree();
    let result = lang_syntax::format::format(
        tree,
        &view.snapshot.analysis().sources,
        lang_syntax::format::Limits {
            work: view.limits.work,
            output_bytes: view.limits.compiler.modules.parse.lex.source_bytes,
        },
    )
    .map_err(|_| capacity())?;
    if result.status == lang_syntax::format::Status::PreservedInvalid || result.text == file.text()
    {
        return Ok(None);
    }
    let display = PathBuf::from(file.name());
    let canonical = view
        .snapshot
        .aliases
        .iter()
        .find(|(_, value)| *value == &display)
        .map(|(key, _)| key.clone())
        .unwrap_or_else(|| display.clone());
    let mut overlays = view.overlays.clone();
    let version = overlays
        .iter()
        .find(|o| view.snapshot.display_path(&o.path) == display)
        .map(|o| o.version)
        .unwrap_or(0);
    overlays.retain(|o| view.snapshot.display_path(&o.path) != display);
    overlays.push(Overlay {
        path: canonical,
        version,
        bytes: Arc::from(result.text.as_bytes()),
    });
    let next = view
        .snapshot
        .recapture(&view.graph, epoch, &overlays, view.limits)?;
    let mut work = view.limits.work;
    proof::verify_format(view, &next, &mut work)?;
    Ok(Some(Plan {
        replacement: result.text,
        files: BTreeMap::from([(
            display,
            vec![ByteSpan {
                start: 0,
                end: file.text().len(),
            }],
        )]),
        offsets: BTreeMap::new(),
    }))
}
pub(crate) fn target<'a>(
    view: &'a View,
    path: &Path,
    offset: usize,
    epoch: u64,
) -> Result<&'a Symbol, Diagnostic> {
    let symbol = view
        .snapshot
        .definition(view.relative(path)?, offset, epoch)?
        .ok_or_else(|| invalid("position has no proven rename target"))?;
    if matches!(
        symbol.kind,
        SymbolKind::Module | SymbolKind::Import | SymbolKind::Host
    ) {
        return Err(invalid(
            "module, import alias and external renames are unsupported",
        ));
    }
    let checked = view
        .snapshot
        .analysis()
        .result
        .as_ref()
        .map_err(|_| invalid("rename requires valid source"))?;
    for function in &checked.source().functions {
        if function.abi != lang_hir::resolved::FunctionAbi::Guest
            && location_matches(view, &symbol.location, function.name.span)
        {
            return Err(invalid("external ABI declarations cannot be renamed"));
        }
    }
    Ok(symbol)
}
fn location_matches(view: &View, location: &Location, span: Span) -> bool {
    view.snapshot
        .analysis()
        .sources
        .file(span.source())
        .is_ok_and(|f| Path::new(f.name()) == location.path)
        && span.range() == (location.span.start..location.span.end)
}
pub(crate) fn rename(
    view: &View,
    path: &Path,
    offset: usize,
    name: &str,
    epoch: u64,
) -> Result<Plan, Diagnostic> {
    let selected = target(view, path, offset, epoch)?;
    let mut work = view.limits.work;
    spend_bytes(&mut work, name.len())?;
    let mut names = SourceMap::new();
    let id = add_source(
        &mut names,
        "<rename>",
        name.as_bytes(),
        view.limits.compiler.modules.parse.lex,
    )
    .map_err(|_| invalid("invalid rename identifier"))?;
    let tokens = lex(&names, id, view.limits.compiler.modules.parse.lex)
        .map_err(|_| invalid("invalid rename identifier"))?
        .tokens;
    if tokens.len() != 1
        || tokens[0].kind != TokenKind::Identifier
        || tokens[0].span.range() != (0..name.len())
    {
        return Err(invalid("new name must be a non-keyword identifier"));
    }
    let mut plan = Plan {
        replacement: name.to_string(),
        files: BTreeMap::new(),
        offsets: BTreeMap::new(),
    };
    for occurrence in view.snapshot.occurrences() {
        spend(&mut work)?;
        if occurrence.symbol == selected.id {
            spend_bytes(
                &mut work,
                occurrence
                    .location
                    .path
                    .as_os_str()
                    .as_encoded_bytes()
                    .len(),
            )?;
            plan.files
                .entry(occurrence.location.path.clone())
                .or_default()
                .push(occurrence.location.span);
        }
    }
    // An import also creates a lexical alias; that is a separate refactoring contract.
    for symbol in view.snapshot.symbols() {
        spend(&mut work)?;
        if symbol.kind == SymbolKind::Import && plan.contains(&symbol.location) {
            return Err(invalid("rename through an import alias is unsupported"));
        }
    }
    let mut overlays = view.overlays.clone();
    for (display, edits) in &mut plan.files {
        edits.sort_by_key(|s| (s.start, s.end));
        edits.dedup();
        let (mut old_end, mut new_end) = (0usize, 0usize);
        let mut offsets = Vec::new();
        for edit in edits.iter() {
            spend(&mut work)?;
            new_end = new_end
                .checked_add(
                    edit.start
                        .checked_sub(old_end)
                        .ok_or_else(|| invalid("overlapping rename edits"))?,
                )
                .and_then(|n| n.checked_add(name.len()))
                .ok_or_else(capacity)?;
            old_end = edit.end;
            offsets.push((old_end, new_end));
        }
        plan.offsets.insert(display.clone(), offsets);
        if edits.windows(2).any(|p| p[0].end > p[1].start) {
            return Err(invalid("overlapping rename edits"));
        }
        let (_, source) = view.source(&view.authority.join(display))?;
        let text = source.text();
        let mut size = text.len();
        for edit in edits.iter() {
            if text.get(edit.start..edit.end) != Some(selected.name.as_str()) {
                return Err(invalid("rename token does not match its binding"));
            }
            size = size
                .checked_sub(edit.end - edit.start)
                .and_then(|n| n.checked_add(name.len()))
                .ok_or_else(capacity)?;
        }
        if size > view.limits.compiler.modules.parse.lex.source_bytes {
            return Err(capacity());
        }
        spend_bytes(&mut work, size)?;
        let mut updated = String::with_capacity(size);
        let mut end = 0;
        for edit in edits.iter() {
            updated.push_str(&text[end..edit.start]);
            updated.push_str(name);
            end = edit.end;
        }
        updated.push_str(&text[end..]);
        let canonical = view
            .snapshot
            .aliases
            .iter()
            .find(|(_, value)| *value == display)
            .map(|(key, _)| key.clone())
            .unwrap_or_else(|| display.clone());
        let version = overlays
            .iter()
            .find(|o| view.snapshot.display_path(&o.path) == display)
            .map(|o| o.version)
            .unwrap_or(0);
        overlays.retain(|o| view.snapshot.display_path(&o.path) != display);
        overlays.push(Overlay {
            path: canonical,
            version,
            bytes: Arc::from(updated.as_bytes()),
        });
    }
    let next = view
        .snapshot
        .recapture(&view.graph, epoch, &overlays, view.limits)?;
    proof::verify(view, &next, &plan, &mut work)?;
    Ok(plan)
}
impl Plan {
    fn contains(&self, location: &Location) -> bool {
        self.files.get(&location.path).is_some_and(|edits| {
            edits
                .binary_search_by_key(&(location.span.start, location.span.end), |e| {
                    (e.start, e.end)
                })
                .is_ok()
        })
    }
    fn boundary(&self, path: &Path, offset: usize) -> Result<usize, Diagnostic> {
        if let Some(edits) = self.offsets.get(path) {
            let count = edits.partition_point(|(end, _)| *end <= offset);
            if let Some((old, new)) = count.checked_sub(1).map(|i| edits[i]) {
                return offset
                    .checked_sub(old)
                    .and_then(|n| n.checked_add(new))
                    .ok_or_else(capacity);
            }
        }
        Ok(offset)
    }
}
