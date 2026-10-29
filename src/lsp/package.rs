use super::{
    Fault, Server, Value, database_error, diagnostic, field, host_diagnostic, internal, invalid,
    json, string, uri,
};
pub(super) use crate::package::editor::workspace::{Config, View};
use crate::package::editor::{self, workspace::OpenDocument};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

pub(super) fn configuration(root: Option<&Path>, params: &Value) -> Result<Option<Config>, Fault> {
    let options = params.get("initializationOptions").filter(|v| !v.is_null());
    if options.is_some_and(|o| !o.is_object()) {
        return Err(invalid("initializationOptions must be an object"));
    }
    let option = |name: &str| -> Result<Option<String>, Fault> {
        options
            .and_then(|o| o.get(name))
            .map(|v| {
                v.as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        invalid("package initialization option must be a nonempty string")
                    })
            })
            .transpose()
    };
    let explicit = option("manifestPath")?;
    let selected = option("package")?;
    let target = option("target")?;
    let Some(root) = root else {
        if explicit.is_some() || selected.is_some() || target.is_some() {
            return Err(invalid(
                "package selection requires an explicit workspace root",
            ));
        }
        return Ok(None);
    };
    let relative = PathBuf::from(explicit.as_deref().unwrap_or("Lang.json"));
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(invalid("manifestPath must be relative to the workspace"));
    }
    let manifest = root.join(relative);
    if explicit.is_none()
        && selected.is_none()
        && target.is_none()
        && !manifest.try_exists().map_err(internal)?
    {
        return Ok(None);
    }
    Ok(Some(Config {
        root: root.to_path_buf(),
        manifest,
        selected,
        target: target.unwrap_or_else(|| lang_codegen_clif::Triple::host().to_string()),
    }))
}
impl Server {
    pub(super) fn completion(&mut self, params: &Value) -> Result<Value, Fault> {
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
        let coordinate = |key: &str| -> Result<u32, Fault> {
            field(position, key)?
                .as_u64()
                .filter(|n| *n <= i32::MAX as u64)
                .map(|n| n as u32)
                .ok_or_else(|| invalid("invalid UTF-16 position"))
        };
        let line = coordinate("line")?;
        let character = coordinate("character")?;
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
        let list = editor::completion::complete(view, &path, offset, self.package_epoch)
            .map_err(internal)?;
        let sources = &view.snapshot.analysis().sources;
        let span = sources
            .span(id, list.replace.span.start..list.replace.span.end)
            .map_err(internal)?;
        let edit_range = super::range(
            lang_lsp::map_range(span, sources, self.limits.mapping.source_bytes)
                .map_err(internal)?,
        );
        let version = view
            .snapshot
            .versions()
            .get(view.relative(&path).map_err(internal)?)
            .copied();
        let mut items = Vec::new();
        let mut bytes = 40usize;
        if bytes > self.limits.mapping.output_bytes {
            return Err(internal("completion response byte limit exceeded"));
        }
        for candidate in list.candidates {
            use editor::SymbolKind as K;
            let kind = match candidate.kind {
                K::Function | K::Host => 3,
                K::Record => 22,
                K::Enum => 13,
                K::EnumCase => 20,
                K::Module | K::Import => 9,
                K::Parameter | K::Variable => 6,
                K::Field => 5,
            };
            let value = json!({"label":candidate.name,"kind":kind,"detail":candidate.detail,"textEdit":{"range":edit_range,"newText":candidate.name},"data":{"epoch":self.package_epoch,"version":version}});
            bytes = bytes
                .checked_add(serde_json::to_vec(&value).map_err(internal)?.len() + 1)
                .filter(|n| *n <= self.limits.mapping.output_bytes)
                .ok_or_else(|| internal("completion response byte limit exceeded"))?;
            items.push(value);
        }
        Ok(json!({"isIncomplete":false,"items":items}))
    }
    pub(super) fn package_publications(&mut self) -> Vec<Value> {
        self.ensure_package();
        let mut groups: BTreeMap<String, Vec<Value>> = self
            .documents
            .keys()
            .chain(self.package_published.iter())
            .map(|u| (u.clone(), Vec::new()))
            .collect();
        let mut used = BTreeMap::<String, usize>::new();
        let mut limited = BTreeSet::new();
        {
            let mut push = |uri: String, value: Value| {
                if limited.contains(&uri) {
                    return;
                }
                let previous = used.get(&uri).copied().unwrap_or(2);
                let next = serde_json::to_vec(&value)
                    .ok()
                    .and_then(|v| previous.checked_add(v.len() + 1))
                    .filter(|n| *n <= self.limits.mapping.output_bytes);
                if let Some(next) = next {
                    used.insert(uri.clone(), next);
                    groups.entry(uri).or_default().push(value);
                } else {
                    groups.insert(
                        uri.clone(),
                        vec![host_diagnostic(
                            "package diagnostic response byte limit exceeded",
                        )],
                    );
                    limited.insert(uri);
                }
            };
            match self.package_view.as_ref().unwrap() {
                Ok(view) => {
                    let analysis = view.snapshot.analysis();
                    let uris = source_uris(view, &self.documents);
                    for source in &analysis.diagnostics {
                        match lang_lsp::map_diagnostic(
                            source,
                            &analysis.sources,
                            &uris,
                            self.limits.mapping,
                        ) {
                            Ok(mut mapped) => {
                                if !self.related {
                                    mapped.diagnostic.related_information.clear();
                                }
                                push(mapped.uri, diagnostic(mapped.diagnostic));
                            }
                            Err(e) => {
                                if let Some(owner) = self.documents.keys().next() {
                                    push(owner.clone(), host_diagnostic(&e.to_string()));
                                }
                            }
                        }
                    }
                    if let Err(error) = &analysis.result
                        && !matches!(error, crate::frontend::AnalysisError::Diagnostics)
                        && let Some(owner) = self.documents.keys().next()
                    {
                        let mut value = host_diagnostic(&error.to_string());
                        value["code"] = json!("CLI0002");
                        push(owner.clone(), value);
                    }
                }
                Err(error) => {
                    if let Some(owner) = self.documents.keys().next() {
                        let mut value = host_diagnostic(&error.to_string());
                        value["code"] = json!(error.code);
                        push(owner.clone(), value);
                    }
                }
            }
        }
        self.package_published = groups
            .iter()
            .filter(|(_, diagnostics)| !diagnostics.is_empty())
            .map(|(uri, _)| uri.clone())
            .collect();
        groups
            .into_iter()
            .map(|(uri, mut diagnostics)| {
                if serde_json::to_vec(&diagnostics)
                    .map_or(true, |b| b.len() > self.limits.mapping.output_bytes)
                {
                    diagnostics = vec![host_diagnostic(
                        "package diagnostic response byte limit exceeded",
                    )];
                }
                self.publication(
                    &uri,
                    self.documents.get(&uri).map(|d| d.version),
                    diagnostics,
                )
            })
            .collect()
    }
    fn ensure_package(&mut self) {
        if self.package_view.is_some() {
            return;
        }
        let db = self.limits.database;
        let limits = editor::Limits {
            compiler: crate::frontend::CompileLimits {
                modules: db.modules,
                binding: db.binding,
                typing: db.typing,
                ownership: db.ownership,
                ..Default::default()
            },
            overlays: db.files,
            overlay_bytes: db.input_bytes,
            ..Default::default()
        };
        let documents: Vec<_> = self
            .documents
            .values()
            .map(|d| OpenDocument {
                path: d.path.clone(),
                version: d.version,
                bytes: d.text.clone(),
            })
            .collect();
        self.package_view = Some(
            self.package
                .as_ref()
                .expect("package configuration")
                .capture(self.package_epoch, &documents, limits),
        );
    }
    pub(super) fn navigation(&mut self, params: &Value, references: bool) -> Result<Value, Fault> {
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
        let coordinate = |key: &str| -> Result<u32, Fault> {
            field(position, key)?
                .as_u64()
                .filter(|n| *n <= i32::MAX as u64)
                .map(|n| n as u32)
                .ok_or_else(|| invalid("invalid UTF-16 position"))
        };
        let line = coordinate("line")?;
        let character = coordinate("character")?;
        let declaration = if references {
            field(field(params, "context")?, "includeDeclaration")?
                .as_bool()
                .ok_or_else(|| invalid("includeDeclaration must be boolean"))?
        } else {
            false
        };
        self.ensure_package();
        let view = self
            .package_view
            .as_ref()
            .unwrap()
            .as_ref()
            .map_err(internal)?;
        let (_, file) = view.source(&path).map_err(internal)?;
        let offset = lang_lsp::text_offset(
            file.text(),
            line,
            character,
            self.limits.mapping.source_bytes,
        )
        .map_err(|_| invalid("position has no valid source boundary"))?;
        let target = view
            .snapshot
            .definition(
                view.relative(&path).map_err(internal)?,
                offset,
                self.package_epoch,
            )
            .map_err(internal)?;
        let Some(target) = target else {
            return Ok(if references { json!([]) } else { Value::Null });
        };
        let uris = source_uris(view, &self.documents);
        if !references {
            return location(view, &target.location, self.limits, &uris);
        }
        let mut bytes = 2usize;
        let mut result = Vec::new();
        for occurrence in view
            .snapshot
            .occurrences()
            .iter()
            .filter(|o| o.symbol == target.id && (declaration || !o.declaration))
        {
            let value = location(view, &occurrence.location, self.limits, &uris)?;
            bytes = bytes
                .checked_add(serde_json::to_vec(&value).map_err(internal)?.len() + 1)
                .filter(|n| *n <= self.limits.mapping.output_bytes)
                .ok_or_else(|| internal("reference response byte limit exceeded"))?;
            result.push(value);
        }
        Ok(Value::Array(result))
    }
}
fn location(
    view: &View,
    location: &editor::Location,
    limits: super::Limits,
    uris: &BTreeMap<lang_diagnostics::SourceId, String>,
) -> Result<Value, Fault> {
    let absolute = view.authority.join(&location.path);
    let (id, _) = view.source(&absolute).map_err(internal)?;
    let name = uris
        .get(&id)
        .ok_or_else(|| internal("missing source URI"))?;
    let sources = &view.snapshot.analysis().sources;
    let span = sources
        .span(id, location.span.start..location.span.end)
        .map_err(internal)?;
    let range =
        lang_lsp::map_range(span, sources, limits.mapping.source_bytes).map_err(internal)?;
    let value = json!({"uri":name,"range":super::range(range)});
    if serde_json::to_vec(&value).map_err(internal)?.len() > limits.mapping.output_bytes {
        return Err(internal("location response byte limit exceeded"));
    }
    Ok(value)
}
fn source_uris(
    view: &View,
    documents: &BTreeMap<String, super::Document>,
) -> BTreeMap<lang_diagnostics::SourceId, String> {
    let open: BTreeMap<_, _> = documents
        .iter()
        .filter_map(|(uri, document)| {
            view.source(&document.path)
                .ok()
                .map(|(id, _)| (id, uri.clone()))
        })
        .collect();
    view.snapshot
        .analysis()
        .sources
        .iter()
        .filter_map(|(id, file)| {
            open.get(&id)
                .cloned()
                .or_else(|| uri::file(&view.authority.join(file.name())).ok())
                .map(|uri| (id, uri))
        })
        .collect()
}
