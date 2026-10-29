//! Versioned client edit proposals guarded by complete input and binding proofs.
use super::{Fault, Server, database_error, field, internal, invalid, range, string, uri};
use crate::package::editor::{edits, workspace::OpenDocument};
use serde_json::{Value, json};
impl Server {
    pub(super) fn checked_rename(&mut self, params: &Value, prepare: bool) -> Result<Value, Fault> {
        let name = string(field(params, "textDocument")?, "uri")?;
        let path = uri::path(name, self.limits.uri_bytes)
            .map_err(|_| invalid("document must use a local file URI"))?;
        let path = self
            .db
            .as_ref()
            .unwrap()
            .document_path(&path)
            .map_err(database_error)?;
        let position = field(params, "position")?;
        let coordinate = |key: &str| {
            field(position, key)?
                .as_u64()
                .filter(|n| *n <= i32::MAX as u64)
                .map(|n| n as u32)
                .ok_or_else(|| invalid("invalid UTF-16 position"))
        };
        let (line, character) = (coordinate("line")?, coordinate("character")?);
        self.ensure_package();
        let view = self
            .package_view
            .as_ref()
            .unwrap()
            .as_ref()
            .map_err(internal)?;
        let (id, file) = view.source(&path).map_err(internal)?;
        let offset = lang_lsp::text_offset(
            file.text(),
            line,
            character,
            self.limits.mapping.source_bytes,
        )
        .map_err(|_| invalid("position has no valid source boundary"))?;
        let selected = edits::target(view, &path, offset, self.package_epoch).map_err(internal)?;
        let mut mapping_work = view.limits.work;
        if prepare {
            let display = view
                .snapshot
                .display_path(view.relative(&path).map_err(internal)?);
            let occurrence = view
                .snapshot
                .occurrences()
                .iter()
                .find(|o| {
                    o.symbol == selected.id
                        && o.location.path == display
                        && o.location.span.start <= offset
                        && offset < o.location.span.end
                })
                .ok_or_else(|| internal("missing prepare rename occurrence"))?;
            let span = view
                .snapshot
                .analysis()
                .sources
                .span(
                    id,
                    occurrence.location.span.start..occurrence.location.span.end,
                )
                .map_err(internal)?;
            charge_mapping(&mut mapping_work, file.text().len())?;
            let result = json!({"range":range(lang_lsp::map_range(span, &view.snapshot.analysis().sources, self.limits.mapping.source_bytes).map_err(internal)?),"placeholder":selected.name});
            return bounded(result, self.limits.mapping.output_bytes);
        }
        if !self.edit_document_changes {
            return Err(internal(
                "rename requires workspaceEdit.documentChanges for versioned edits",
            ));
        }
        self.verify_edit_inputs()?;
        let new_name = string(params, "newName")?;
        let plan =
            edits::rename(view, &path, offset, new_name, self.package_epoch).map_err(internal)?;
        self.proposed_edit(plan)
    }
    pub(super) fn proposed_edit(&self, plan: edits::Plan) -> Result<Value, Fault> {
        if !self.edit_document_changes {
            return Err(internal("edits require workspaceEdit.documentChanges"));
        }
        let view = self
            .package_view
            .as_ref()
            .unwrap()
            .as_ref()
            .map_err(internal)?;
        let mut mapping_work = view.limits.work;
        if plan.files.len() > 1 && !self.edit_transactional {
            return Err(internal(
                "multi-file rename requires transactional or textOnlyTransactional client edits",
            ));
        }
        let mut changes = Vec::new();
        let mut bytes = 24usize;
        for (display, edits) in plan.files {
            let (source_id, source) = view
                .source(&view.authority.join(&display))
                .map_err(internal)?;
            let (uri, document) = self
                .documents
                .iter()
                .find(|(_, d)| view.source(&d.path).is_ok_and(|(id, _)| id == source_id))
                .ok_or_else(|| {
                    internal("every edited file must be open with a concrete document version")
                })?;
            let mut text_edits = Vec::new();
            for edit in edits {
                charge_mapping(&mut mapping_work, source.text().len())?;
                let span = view
                    .snapshot
                    .analysis()
                    .sources
                    .span(source_id, edit.start..edit.end)
                    .map_err(internal)?;
                let value = json!({"range":range(lang_lsp::map_range(span, &view.snapshot.analysis().sources, self.limits.mapping.source_bytes).map_err(internal)?),"newText":plan.replacement});
                bytes = bytes
                    .checked_add(serde_json::to_vec(&value).map_err(internal)?.len() + 1)
                    .filter(|n| *n <= self.limits.mapping.output_bytes)
                    .ok_or_else(|| internal("rename response byte limit exceeded"))?;
                text_edits.push(value);
            }
            let value =
                json!({"textDocument":{"uri":uri,"version":document.version},"edits":text_edits});
            changes.push(value);
        }
        let result = bounded(
            json!({"documentChanges":changes}),
            self.limits.mapping.output_bytes,
        )?;
        self.verify_edit_inputs()?;
        Ok(result)
    }
    pub(super) fn verify_edit_inputs(&self) -> Result<(), Fault> {
        let view = self
            .package_view
            .as_ref()
            .unwrap()
            .as_ref()
            .map_err(internal)?;
        let documents = self
            .documents
            .values()
            .map(|d| OpenDocument {
                path: d.path.clone(),
                version: d.version,
                bytes: d.text.clone(),
            })
            .collect::<Vec<_>>();
        let fresh = self
            .package
            .as_ref()
            .unwrap()
            .capture(self.package_epoch, &documents, view.limits)
            .map_err(internal)?;
        if !view.same_inputs(&fresh) {
            return Err(internal(
                "package inputs changed; refresh before requesting edits",
            ));
        }
        Ok(())
    }
}
fn charge_mapping(work: &mut usize, source_bytes: usize) -> Result<(), Fault> {
    // One line-index scan and at most two full-line UTF-16 prefix scans.
    *work = source_bytes
        .checked_mul(3)
        .and_then(|cost| work.checked_sub(cost))
        .ok_or_else(|| internal("rename UTF-16 mapping work limit exceeded"))?;
    Ok(())
}
pub(super) fn bounded(value: Value, limit: usize) -> Result<Value, Fault> {
    if serde_json::to_vec(&value).map_err(internal)?.len() > limit {
        return Err(internal("rename response byte limit exceeded"));
    }
    Ok(value)
}
