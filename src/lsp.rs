//! Small stdio LSP 3.18 profile using the shared source query database.
//! Official source/update dates are recorded in docs/architecture.md.
mod package;
mod tokens;
pub mod uri;
pub mod wire;
use lang_diagnostics::{Diagnostic as SourceDiagnostic, SourceId, Span};
use lang_lsp::{Diagnostic, Range};
use lang_semantics::{
    database::{AnalysisFailure, AnalysisSnapshot, Database, ModuleSnapshot},
    modules::{ItemKind, ModuleLoadError},
};
use lang_syntax::format;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
pub use wire::TransportError;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub wire: wire::Limits,
    pub database: lang_semantics::database::Limits,
    pub uri_bytes: usize,
    pub id_bytes: usize,
    pub mapping: lang_lsp::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            wire: wire::Limits::default(),
            database: lang_semantics::database::Limits::default(),
            uri_bytes: 8192,
            id_bytes: 1024,
            mapping: lang_lsp::Limits {
                source_bytes: 16 * 1024 * 1024,
                labels: 64,
                notes: 64,
                output_bytes: 65_536,
            },
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Cold,
    Awaiting,
    Ready,
    Shutdown,
    Exited(u8),
}
struct Document {
    path: PathBuf,
    version: i32,
    text: Arc<[u8]>,
}
struct Fault {
    code: i32,
    message: String,
}
fn invalid(message: &str) -> Fault {
    Fault {
        code: -32602,
        message: message.into(),
    }
}
fn internal(error: impl std::fmt::Display) -> Fault {
    Fault {
        code: -32603,
        message: error.to_string(),
    }
}
fn database_error(error: lang_semantics::database::DbError) -> Fault {
    match error {
        lang_semantics::database::DbError::OutsideRoot(_) => {
            internal("OutsideRoot: document is outside the workspace")
        }
        error => internal(error),
    }
}
fn response_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn response(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","method":method,"params":params})
}
fn log(message: &str) -> Value {
    notification("window/logMessage", json!({"type":1,"message":message}))
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, Fault> {
    value
        .get(name)
        .ok_or_else(|| invalid("missing required parameter"))
}
fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, Fault> {
    field(value, name)?
        .as_str()
        .ok_or_else(|| invalid("expected string parameter"))
}
fn version(value: &Value) -> Result<i32, Fault> {
    field(value, "version")?
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| invalid("invalid document version"))
}
fn range(range: Range) -> Value {
    json!({"start":{"line":range.start.line(),"character":range.start.character()},"end":{"line":range.end.line(),"character":range.end.character()}})
}
fn point() -> Value {
    json!({"start":{"line":0,"character":0},"end":{"line":0,"character":0}})
}
fn diagnostic(d: Diagnostic) -> Value {
    let mut value = json!({"range":range(d.range),"severity":d.severity.as_u8(),"code":d.code,"source":d.source,"message":d.message});
    if !d.related_information.is_empty() {
        value["relatedInformation"]=Value::Array(d.related_information.into_iter().map(|r|json!({"location":{"uri":r.uri,"range":range(r.range)},"message":r.message})).collect());
    }
    value
}
pub struct Server {
    limits: Limits,
    phase: Phase,
    db: Option<Database>,
    documents: BTreeMap<String, Document>,
    related: bool,
    versions: bool,
    watch: bool,
    token_refresh: bool,
    package: Option<package::Config>,
    package_view: Option<Result<package::View, crate::package::Diagnostic>>,
    package_epoch: u64,
    package_published: BTreeSet<String>,
}
impl Server {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            phase: Phase::Cold,
            db: None,
            documents: BTreeMap::new(),
            related: false,
            versions: false,
            watch: false,
            token_refresh: false,
            package: None,
            package_view: None,
            package_epoch: 0,
            package_published: BTreeSet::new(),
        }
    }
    pub fn exit_status(&self) -> Option<u8> {
        if let Phase::Exited(code) = self.phase {
            Some(code)
        } else {
            None
        }
    }
    pub fn stats(&self) -> Option<lang_semantics::database::Stats> {
        self.db.as_ref().map(Database::stats)
    }
    pub fn handle(&mut self, value: Value) -> Vec<Value> {
        if !value.is_object() || value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return vec![response_error(Value::Null, -32600, "invalid request")];
        }
        let id = value.get("id").cloned();
        if id.as_ref().is_some_and(|id| match id {
            Value::String(s) => s.len() > self.limits.id_bytes,
            Value::Number(n) => n.as_i64().and_then(|n| i32::try_from(n).ok()).is_none(),
            _ => true,
        }) {
            return vec![response_error(Value::Null, -32600, "invalid request id")];
        }
        let Some(method) = value.get("method").and_then(Value::as_str) else {
            if id.is_some() && (value.get("result").is_some() ^ value.get("error").is_some()) {
                return vec![];
            }
            return vec![response_error(Value::Null, -32600, "invalid request")];
        };
        if method.len() > 256 {
            return vec![response_error(
                id.unwrap_or(Value::Null),
                -32600,
                "method limit exceeded",
            )];
        }
        let params = value.get("params").unwrap_or(&Value::Null);
        if method == "exit" {
            if let Some(id) = id {
                return vec![response_error(id, -32600, "exit is a notification")];
            }
            self.phase = Phase::Exited(if self.phase == Phase::Shutdown { 0 } else { 1 });
            return vec![];
        }
        if matches!(self.phase, Phase::Shutdown | Phase::Exited(_)) {
            return id
                .map(|id| vec![response_error(id, -32600, "server has shut down")])
                .unwrap_or_default();
        }
        if method == "initialize" {
            let Some(id) = id else {
                return vec![];
            };
            if self.phase != Phase::Cold {
                return vec![response_error(
                    id,
                    -32600,
                    "initialize may only be sent once",
                )];
            }
            return vec![match self.initialize(params) {
                Ok(result) => response(id, result),
                Err(error) => response_error(id, error.code, &error.message),
            }];
        }
        if method == "initialized" && self.phase == Phase::Awaiting && id.is_none() {
            self.phase = Phase::Ready;
            return if self.watch && self.db.as_ref().unwrap().source_root().is_some() {
                vec![
                    json!({"jsonrpc":"2.0","id":"lang-watch","method":"client/registerCapability","params":{"registrations":[{"id":"lang-watch","method":"workspace/didChangeWatchedFiles","registerOptions":{"watchers":[{"globPattern":"**/*.lang","kind":7}]}}]}}),
                ]
            } else {
                vec![]
            };
        }
        if method == "shutdown" && self.phase != Phase::Cold {
            let Some(id) = id else {
                return vec![];
            };
            self.phase = Phase::Shutdown;
            return vec![response(id, Value::Null)];
        }
        if self.phase != Phase::Ready {
            return id
                .map(|id| vec![response_error(id, -32002, "server not initialized")])
                .unwrap_or_default();
        }
        if let Some(id) = id {
            if self.package.is_some() && method == "textDocument/semanticTokens/full" {
                return vec![match self.semantic_tokens(params) {
                    Ok(result) => response(id, result),
                    Err(e) => response_error(id, e.code, &e.message),
                }];
            }
            if self.package.is_some() && method == "textDocument/completion" {
                return vec![match self.completion(params) {
                    Ok(result) => response(id, result),
                    Err(e) => response_error(id, e.code, &e.message),
                }];
            }
            if self.package.is_some()
                && matches!(
                    method,
                    "textDocument/definition" | "textDocument/references"
                )
            {
                return vec![
                    match self.navigation(params, method == "textDocument/references") {
                        Ok(result) => response(id, result),
                        Err(e) => response_error(id, e.code, &e.message),
                    },
                ];
            }
            if method == "textDocument/formatting" {
                return vec![match self.formatting(params) {
                    Ok(result) => response(id, result),
                    Err(e) => response_error(id, e.code, &e.message),
                }];
            }
            return vec![response_error(
                id,
                if matches!(
                    method,
                    "textDocument/didOpen"
                        | "textDocument/didChange"
                        | "textDocument/didClose"
                        | "initialized"
                ) {
                    -32600
                } else {
                    -32601
                },
                "unsupported request",
            )];
        }
        match self.notify(method, params) {
            Ok(messages) => messages,
            Err(error) => vec![log(&error.message)],
        }
    }
    fn initialize(&mut self, params: &Value) -> Result<Value, Fault> {
        if !params.is_object() || !params.get("capabilities").is_some_and(Value::is_object) {
            return Err(invalid("invalid initialize parameters"));
        }
        let root = if let Some(uri) = params.get("rootUri").filter(|v| !v.is_null()) {
            Some(uri.as_str().ok_or_else(|| invalid("invalid root URI"))?)
        } else if let Some(folders) = params.get("workspaceFolders").filter(|v| !v.is_null()) {
            let folders = folders
                .as_array()
                .ok_or_else(|| invalid("invalid workspace folders"))?;
            if folders.len() > 1 {
                return Err(invalid("prototype supports one workspace root"));
            }
            folders.first().map(|f| string(f, "uri")).transpose()?
        } else {
            None
        };
        let root = root
            .map(|root| {
                uri::path(root, self.limits.uri_bytes)
                    .map_err(|_| invalid("workspace root must be a local file URI"))
            })
            .transpose()?;
        let db = Database::new(root.as_deref(), self.limits.database)
            .map_err(|_| invalid("invalid workspace source root"))?;
        self.package = package::configuration(db.source_root(), params)?;
        self.related = params
            .pointer("/capabilities/textDocument/publishDiagnostics/relatedInformation")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.versions = params
            .pointer("/capabilities/textDocument/publishDiagnostics/versionSupport")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.watch = params
            .pointer("/capabilities/workspace/didChangeWatchedFiles/dynamicRegistration")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.token_refresh = params
            .pointer("/capabilities/workspace/semanticTokens/refreshSupport")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.db = Some(db);
        self.phase = Phase::Awaiting;
        let mut result = json!({"capabilities":{"positionEncoding":"utf-16","textDocumentSync":{"openClose":true,"change":1},"documentFormattingProvider":true},"serverInfo":{"name":"lang prototype","version":env!("CARGO_PKG_VERSION")}});
        if self.package.is_some() {
            result["capabilities"]["definitionProvider"] = json!(true);
            result["capabilities"]["referencesProvider"] = json!(true);
            result["capabilities"]["completionProvider"] =
                json!({"triggerCharacters":[".",":"],"resolveProvider":false});
            result["capabilities"]["semanticTokensProvider"] = json!({"legend":{"tokenTypes":tokens::LEGEND,"tokenModifiers":[]},"full":true,"range":false});
        }
        Ok(result)
    }
    fn notify(&mut self, method: &str, params: &Value) -> Result<Vec<Value>, Fault> {
        match method {
            "textDocument/didOpen" => {
                let text_document = field(params, "textDocument")?;
                let name = string(text_document, "uri")?;
                if self.documents.contains_key(name) {
                    return Err(invalid("document is already open"));
                }
                let _language = string(text_document, "languageId")?;
                let version = version(text_document)?;
                let text = string(text_document, "text")?;
                if text.len() > self.limits.database.modules.parse.lex.source_bytes {
                    return Err(invalid("document source byte limit exceeded"));
                }
                let path = uri::path(name, self.limits.uri_bytes)
                    .map_err(|_| invalid("document must use a local file URI"))?;
                if path.file_name().is_none() {
                    return Err(invalid("document URI must name a file"));
                }
                let bytes: Arc<[u8]> = Arc::from(text.as_bytes());
                let path = self
                    .db
                    .as_mut()
                    .unwrap()
                    .open(&path, bytes.clone())
                    .map_err(database_error)?;
                self.documents.insert(
                    name.into(),
                    Document {
                        path,
                        version,
                        text: bytes,
                    },
                );
                Ok(self.reanalyze())
            }
            "textDocument/didChange" => {
                let document = field(params, "textDocument")?;
                let name = string(document, "uri")?;
                let version = version(document)?;
                let old = self
                    .documents
                    .get(name)
                    .ok_or_else(|| invalid("document is not open"))?;
                if version <= old.version {
                    return Ok(vec![]);
                }
                let changes = field(params, "contentChanges")?
                    .as_array()
                    .filter(|v| !v.is_empty() && v.len() <= 256)
                    .ok_or_else(|| invalid("invalid content changes"))?;
                let mut text = None;
                for change in changes {
                    if change.get("range").is_some() || change.get("rangeLength").is_some() {
                        return Err(invalid("prototype uses full document synchronization"));
                    }
                    let replacement = string(change, "text")?;
                    if replacement.len() > self.limits.database.modules.parse.lex.source_bytes {
                        return Err(invalid("document source byte limit exceeded"));
                    }
                    text = Some(replacement);
                }
                let bytes: Arc<[u8]> = Arc::from(text.unwrap().as_bytes());
                let path = old.path.clone();
                self.db
                    .as_mut()
                    .unwrap()
                    .change(&path, bytes.clone())
                    .map_err(database_error)?;
                let document = self.documents.get_mut(name).unwrap();
                document.version = version;
                document.text = bytes;
                Ok(self.reanalyze())
            }
            "textDocument/didClose" => {
                let name = string(field(params, "textDocument")?, "uri")?;
                let document = self
                    .documents
                    .get(name)
                    .ok_or_else(|| invalid("document is not open"))?;
                let version = document.version;
                self.db
                    .as_mut()
                    .unwrap()
                    .close(&document.path)
                    .map_err(database_error)?;
                self.documents.remove(name);
                let mut messages = vec![self.publication(name, Some(version), vec![])];
                messages.extend(self.reanalyze());
                Ok(messages)
            }
            "workspace/didChangeWatchedFiles" => {
                if self.db.as_ref().unwrap().source_root().is_none() {
                    return Err(invalid("file watching requires an explicit source root"));
                }
                let changes = field(params, "changes")?
                    .as_array()
                    .filter(|v| v.len() <= 256)
                    .ok_or_else(|| invalid("invalid watched files"))?;
                let mut paths = Vec::new();
                for change in changes {
                    if !field(change, "type")?
                        .as_i64()
                        .is_some_and(|n| (1..=3).contains(&n))
                    {
                        return Err(invalid("invalid watched-file event"));
                    }
                    let path = uri::path(string(change, "uri")?, self.limits.uri_bytes)
                        .map_err(|_| invalid("invalid watched URI"))?;
                    paths.push(
                        self.db
                            .as_ref()
                            .unwrap()
                            .document_path(&path)
                            .map_err(database_error)?,
                    );
                }
                for path in paths {
                    self.db
                        .as_mut()
                        .unwrap()
                        .refresh(&path)
                        .map_err(database_error)?;
                }
                Ok(self.reanalyze())
            }
            _ => Ok(vec![]),
        }
    }
    fn publication(&self, uri: &str, version: Option<i32>, diagnostics: Vec<Value>) -> Value {
        let mut params = json!({"uri":uri,"diagnostics":diagnostics});
        if self.versions
            && let Some(version) = version
        {
            params["version"] = json!(version);
        }
        notification("textDocument/publishDiagnostics", params)
    }
    fn reanalyze(&mut self) -> Vec<Value> {
        if self.package.is_some() {
            let Some(epoch) = self.package_epoch.checked_add(1) else {
                return vec![log("package editor epoch exhausted")];
            };
            self.package_epoch = epoch;
            self.package_view = None;
            let mut messages = self.package_publications();
            if self.token_refresh {
                messages.push(json!({"jsonrpc":"2.0","id":format!("lang-tokens-{epoch}"),"method":"workspace/semanticTokens/refresh"}));
            }
            return messages;
        }
        let paths: Vec<_> = self.documents.values().map(|d| d.path.clone()).collect();
        let mut roots = Vec::new();
        let mut failures = Vec::new();
        for path in paths {
            match self.db.as_mut().unwrap().modules(&path) {
                Ok(module) => roots.push((path, module)),
                Err(error) => failures.push((path, error.to_string())),
            }
        }
        roots
            .sort_by(|(ap, a), (bp, b)| b.paths.len().cmp(&a.paths.len()).then_with(|| ap.cmp(bp)));
        let lookup: BTreeMap<_, _> = self
            .documents
            .iter()
            .enumerate()
            .map(|(index, (uri, document))| (document.path.clone(), (uri.clone(), index)))
            .collect();
        let mut groups: BTreeMap<String, Vec<Value>> = self
            .documents
            .keys()
            .map(|uri| (uri.clone(), vec![]))
            .collect();
        let mut covered = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for (path, module) in roots {
            if covered.contains(&path) {
                continue;
            }
            covered.extend(module.paths.values().cloned());
            let owner = &lookup[&path].0;
            match self.db.as_mut().unwrap().analyze(&path) {
                Ok(analysis) => {
                    for d in &analysis.diagnostics {
                        let target = analysis
                            .paths
                            .get(&d.primary.span.source())
                            .and_then(|p| lookup.get(p))
                            .unwrap_or(&lookup[&path]);
                        let bytes = d.primary.span.range();
                        if !seen.insert((
                            target.1,
                            d.primary.span.source(),
                            bytes.start,
                            bytes.end,
                            d.code.as_str(),
                        )) {
                            continue;
                        }
                        match self.mapped(d, &analysis, &module, &path, &lookup) {
                            Ok(diagnostic) => groups.get_mut(&target.0).unwrap().push(diagnostic),
                            Err(e) => groups
                                .get_mut(owner)
                                .unwrap()
                                .push(host_diagnostic(&e.message)),
                        }
                    }
                    if let Some(error) = &analysis.error {
                        let (target, diagnostic) =
                            self.analysis_failure(error, &path, owner, &lookup);
                        groups.get_mut(&target).unwrap().push(diagnostic);
                    }
                }
                Err(error) => groups
                    .get_mut(owner)
                    .unwrap()
                    .push(host_diagnostic(&error.to_string())),
            }
        }
        for (path, error) in failures {
            if !covered.contains(&path) {
                groups
                    .get_mut(&lookup[&path].0)
                    .unwrap()
                    .push(host_diagnostic(&error));
            }
        }
        groups
            .into_iter()
            .map(|(uri, diagnostics)| {
                self.publication(&uri, Some(self.documents[&uri].version), diagnostics)
            })
            .collect()
    }
    fn mapped(
        &self,
        source: &SourceDiagnostic,
        analysis: &AnalysisSnapshot,
        module: &ModuleSnapshot,
        owner: &Path,
        lookup: &BTreeMap<PathBuf, (String, usize)>,
    ) -> Result<Value, Fault> {
        let uris = analysis
            .paths
            .iter()
            .map(|(id, path)| {
                Ok((
                    *id,
                    lookup
                        .get(path)
                        .map(|(uri, _)| Ok(uri.clone()))
                        .unwrap_or_else(|| {
                            uri::file(path).map_err(|_| invalid("cannot encode source URI"))
                        })?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, Fault>>()?;
        let mut source = source.clone();
        if !self.related {
            source.secondary.clear();
        }
        let mut mapped =
            lang_lsp::map_diagnostic(&source, &analysis.sources, &uris, self.limits.mapping)
                .map_err(internal)?;
        let primary = analysis
            .paths
            .get(&source.primary.span.source())
            .ok_or_else(|| internal("missing source path"))?;
        if !lookup.contains_key(primary) {
            let original_uri = mapped.uri.clone();
            let original_range = mapped.diagnostic.range;
            let span = dependency_span(module, source.primary.span.source())
                .or_else(|| {
                    analysis
                        .paths
                        .iter()
                        .find(|(_, path)| path.as_path() == owner)
                        .and_then(|(id, _)| analysis.sources.span(*id, 0..0).ok())
                })
                .ok_or_else(|| internal("missing root snapshot"))?;
            mapped.diagnostic.range =
                lang_lsp::map_range(span, &analysis.sources, self.limits.mapping.source_bytes)
                    .map_err(internal)?;
            let name = analysis
                .sources
                .file(source.primary.span.source())
                .map_err(internal)?
                .name();
            mapped.diagnostic.message = format!("{name}: {}", mapped.diagnostic.message);
            if self.related {
                mapped.diagnostic.related_information.insert(
                    0,
                    lang_lsp::RelatedInformation {
                        uri: original_uri,
                        range: original_range,
                        message: "dependency diagnostic".into(),
                    },
                );
            }
        }
        Ok(diagnostic(mapped.diagnostic))
    }
    fn analysis_failure(
        &self,
        error: &AnalysisFailure,
        owner: &Path,
        owner_uri: &str,
        lookup: &BTreeMap<PathBuf, (String, usize)>,
    ) -> (String, Value) {
        if let AnalysisFailure::Module(error) = error
            && let ModuleLoadError::Input { path, error } = error.as_ref()
        {
            let target = lookup.get(path).map_or(owner_uri, |(uri, _)| uri.as_str());
            let mut location = point();
            if (target != owner_uri || path == owner)
                && let Some(bytes) = self.db.as_ref().unwrap().raw_input(path)
                && let Ok(text) = std::str::from_utf8(bytes)
                && let (Ok(start), Ok(end)) = (
                    lang_lsp::text_position(
                        text,
                        error.byte_range.start,
                        self.limits.mapping.source_bytes,
                    ),
                    lang_lsp::text_position(
                        text,
                        error.byte_range.end,
                        self.limits.mapping.source_bytes,
                    ),
                )
            {
                location = json!({"start":{"line":start.line(),"character":start.character()},"end":{"line":end.line(),"character":end.character()}});
            }
            return (
                target.into(),
                json!({"range":location,"severity":1,"source":"lang","code":error.code().map(|c|c.as_str()).unwrap_or("input"),"message":format!("{}: {error}",path.file_name().unwrap_or_default().to_string_lossy())}),
            );
        }
        (
            owner_uri.into(),
            host_diagnostic(&format!("analysis unavailable: {error:?}")),
        )
    }
    fn formatting(&mut self, params: &Value) -> Result<Value, Fault> {
        let name = string(field(params, "textDocument")?, "uri")?;
        let document = self
            .documents
            .get(name)
            .ok_or_else(|| invalid("document is not open"))?;
        let options = field(params, "options")?;
        if !options
            .get("tabSize")
            .and_then(Value::as_u64)
            .is_some_and(|n| n > 0 && n <= i32::MAX as u64)
            || options
                .get("insertSpaces")
                .and_then(Value::as_bool)
                .is_none()
        {
            return Err(invalid("invalid formatting options"));
        }
        let parsed = self
            .db
            .as_mut()
            .unwrap()
            .parsed(&document.path)
            .map_err(database_error)?;
        let tree = parsed
            .result
            .as_ref()
            .map_err(|_| invalid("source cannot be formatted"))?;
        let formatted =
            format::format(tree, &parsed.sources, format::Limits::default()).map_err(internal)?;
        if formatted.status == format::Status::PreservedInvalid
            || formatted.text.as_bytes() == document.text.as_ref()
        {
            return Ok(json!([]));
        }
        let source = tree.source();
        let span = parsed
            .sources
            .span(source, 0..document.text.len())
            .map_err(internal)?;
        Ok(
            json!([{"range":range(lang_lsp::map_range(span,&parsed.sources,self.limits.mapping.source_bytes).map_err(internal)?),"newText":formatted.text}]),
        )
    }
}
fn dependency_span(module: &ModuleSnapshot, source: SourceId) -> Option<Span> {
    let graph = module.graph.as_ref()?;
    let mut id = graph.modules().iter().find(|m| m.source() == source)?.id();
    while let Some(parent) = graph.module(id)?.parent() {
        if parent == graph.root() {
            break;
        }
        id = parent;
    }
    graph.items().iter().find_map(|item| match item.kind() {
        ItemKind::Module {
            module: Some(child),
        } if *child == id && item.module() == graph.root() => Some(item.name().span),
        _ => None,
    })
}
fn host_diagnostic(message: &str) -> Value {
    json!({"range":point(),"severity":1,"code":"query","source":"lang","message":message.chars().take(1024).collect::<String>()})
}
pub fn serve(
    input: &mut impl BufRead,
    output: &mut impl Write,
    limits: Limits,
) -> Result<u8, TransportError> {
    let mut server = Server::new(limits);
    loop {
        let bytes = match wire::read(input, limits.wire) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                return Ok(if server.phase == Phase::Shutdown {
                    0
                } else {
                    server.exit_status().unwrap_or(1)
                });
            }
            Err(TransportError::UnsupportedEncoding) => {
                wire::write(
                    output,
                    &response_error(Value::Null, -32700, "unsupported JSON encoding"),
                    limits.wire,
                )?;
                continue;
            }
            Err(error) => return Err(error),
        };
        let messages = match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => server.handle(value),
            Err(_) => vec![response_error(
                Value::Null,
                -32700,
                "invalid or excessively nested JSON",
            )],
        };
        for message in messages {
            match wire::write(output, &message, limits.wire) {
                Ok(()) => {}
                Err(TransportError::MessageLimit) => {
                    let fallback = if let Some(id) = message.get("id") {
                        response_error(id.clone(), -32603, "response byte limit exceeded")
                    } else {
                        log("diagnostic publication byte limit exceeded")
                    };
                    wire::write(output, &fallback, limits.wire)?;
                }
                Err(error) => return Err(error),
            }
        }
        if let Some(status) = server.exit_status() {
            return Ok(status);
        }
    }
}
