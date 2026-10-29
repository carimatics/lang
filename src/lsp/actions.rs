//! Literal, versioned actions using the normal formatter and lint/binding rules.
use super::{
    Fault, Server, database_error, edits::bounded, field, internal, invalid, range, string, uri,
};
use crate::package::{editor::edits, lint};
use serde_json::{Value, json};

impl Server {
    pub(super) fn code_actions(&mut self, params: &Value) -> Result<Value, Fault> {
        if !self.action_literals {
            return Err(internal("code actions require literal support"));
        }
        if !self.edit_document_changes {
            return Err(internal("code actions require versioned documentChanges"));
        }
        let name = string(field(params, "textDocument")?, "uri")?;
        let path = uri::path(name, self.limits.uri_bytes)
            .map_err(|_| invalid("document must use a local file URI"))?;
        let path = self
            .db
            .as_ref()
            .unwrap()
            .document_path(&path)
            .map_err(database_error)?;
        let context = field(params, "context")?;
        if !field(context, "diagnostics")?.is_array() {
            return Err(invalid("code action diagnostics must be an array"));
        }
        let only = context
            .get("only")
            .map(|value| {
                let kinds = value
                    .as_array()
                    .ok_or_else(|| invalid("code action only must be an array"))?;
                if kinds.iter().any(|kind| !kind.is_string()) {
                    return Err(invalid("code action kind must be a string"));
                }
                Ok(kinds.as_slice())
            })
            .transpose()?;
        self.ensure_package();
        let wants = |kind: &str| {
            let supported = match kind {
                "quickfix" => self.action_quickfix,
                "source.format" => self.action_format,
                _ => false,
            };
            supported
                && only.as_ref().is_none_or(|kinds| {
                    kinds
                        .iter()
                        .any(|k| kind_matches(kind, k.as_str().unwrap()))
                })
        };
        let view = self
            .package_view
            .as_ref()
            .unwrap()
            .as_ref()
            .map_err(internal)?;
        let (id, file) = view.source(&path).map_err(internal)?;
        let mut work = view.limits.work;
        if let Some(kinds) = only {
            spend(&mut work, kinds.len().saturating_mul(3))?;
        }
        spend(&mut work, file.text().len().saturating_mul(4))?;
        let requested = field(params, "range")?;
        let point = |key: &str| -> Result<usize, Fault> {
            let p = field(requested, key)?;
            let coordinate = |key: &str| {
                field(p, key)?
                    .as_u64()
                    .filter(|n| *n <= i32::MAX as u64)
                    .map(|n| n as u32)
                    .ok_or_else(|| invalid("invalid UTF-16 position"))
            };
            lang_lsp::text_offset(
                file.text(),
                coordinate("line")?,
                coordinate("character")?,
                self.limits.mapping.source_bytes,
            )
            .map_err(|_| invalid("range has no valid source boundary"))
        };
        let (start, end) = (point("start")?, point("end")?);
        if start > end {
            return Err(invalid("code action range is reversed"));
        }
        self.verify_edit_inputs()?;
        let Ok(program) = &view.snapshot.analysis().result else {
            return Ok(json!([]));
        };
        let mut actions = Vec::new();
        if wants("source.format") {
            match edits::format_document(view,&path,self.package_epoch) {
                Ok(Some(plan))=>actions.push(json!({"title":"Format document","kind":"source.format","edit":self.proposed_edit(plan)?})),
                Ok(None)=>{},
                Err(error) if error.code=="PKG0004"=>return Err(internal(error)),
                Err(_)=>{},
            }
        }
        if wants("quickfix") {
            for function in &program.source().functions {
                spend(
                    &mut work,
                    function
                        .expressions
                        .len()
                        .saturating_add(function.locals.len()),
                )?;
            }
            let selected = lint::unread_bindings(program.source())
                .filter(|name| {
                    let span = name.span.range();
                    name.span.source() == id
                        && if start == end {
                            span.start <= start && start <= span.end
                        } else {
                            span.start < end && start < span.end
                        }
                })
                .take(2)
                .collect::<Vec<_>>();
            if let [binding] = selected.as_slice() {
                spend(
                    &mut work,
                    binding.text.len().saturating_mul(3).saturating_add(1),
                )?;
                let replacement = format!("_{}", binding.text);
                match edits::rename(
                    view,
                    &path,
                    binding.span.range().start,
                    &replacement,
                    self.package_epoch,
                ) {
                    Ok(plan) => {
                        let diagnostic =
                            lint::unread_diagnostic(&view.snapshot.analysis().sources, binding)
                                .map_err(internal)?;
                        let location = range(
                            lang_lsp::map_range(
                                binding.span,
                                &view.snapshot.analysis().sources,
                                self.limits.mapping.source_bytes,
                            )
                            .map_err(internal)?,
                        );
                        actions.push(json!({"title":format!("Prefix {} with _",binding.text),"kind":"quickfix","isPreferred":true,"diagnostics":[{"range":location,"severity":2,"code":diagnostic.code,"source":"lang","message":diagnostic.message}],"edit":self.proposed_edit(plan)?}));
                    }
                    Err(error) if error.code == "PKG0004" => return Err(internal(error)),
                    Err(_) => {}
                }
            }
        }
        self.verify_edit_inputs()?;
        bounded(json!(actions), self.limits.mapping.output_bytes)
    }
}
pub(super) fn kind_matches(kind: &str, base: &str) -> bool {
    base.is_empty()
        || kind == base
        || kind
            .strip_prefix(base)
            .is_some_and(|suffix| suffix.starts_with('.'))
}
fn spend(work: &mut usize, amount: usize) -> Result<(), Fault> {
    *work = work
        .checked_sub(amount)
        .ok_or_else(|| internal("code action work limit exceeded"))?;
    Ok(())
}
