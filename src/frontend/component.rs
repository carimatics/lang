//! Name-based compiler data translated to an independently checked MIR unit.
use super::{AnalysisError, CompileLimits};
use lang_diagnostics::{Diagnostic, SourceMap};
use lang_mir::unit as m;
use lang_runtime::component::{bindings as b, idl, negotiation::ResourceTypeKey};
use lang_semantics::analysis::Program;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resource {
    pub tag: String,
    pub resource: ResourceTypeKey,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    pub source: String,
    pub member: b::MemberKey,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Export {
    pub source: String,
    pub member: b::MemberKey,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Input {
    pub version: u32,
    pub resources: Vec<Resource>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
}
#[derive(Debug)]
pub enum Error {
    Frontend(AnalysisError),
    Bindings(b::Error),
    Name { section: b::Section, index: usize },
    SourceProfile,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "component source compilation failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<b::Error> for Error {
    fn from(error: b::Error) -> Self {
        Self::Bindings(error)
    }
}
impl From<m::Error> for Error {
    fn from(error: m::Error) -> Self {
        Self::Bindings(error.into())
    }
}
#[derive(Debug)]
pub struct Analysis {
    pub sources: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
    /// Original SourceMap IDs for code-local MIR source indices on failure.
    pub code_sources: Vec<lang_diagnostics::SourceId>,
    pub result: Result<b::Unit, Error>,
}
pub struct Request<'a> {
    pub sources: SourceMap,
    pub display: &'a Path,
    pub bytes: &'a [u8],
    pub root: Option<&'a Path>,
    pub compile: CompileLimits,
    pub document: idl::CheckedDocument,
    pub bindings: Input,
    pub limits: b::Limits,
}
pub fn analyze(request: Request<'_>) -> Analysis {
    let Request {
        sources,
        display,
        bytes,
        root,
        compile,
        document,
        bindings,
        limits,
    } = request;
    let count = bindings
        .resources
        .len()
        .checked_add(bindings.imports.len())
        .and_then(|n| n.checked_add(bindings.exports.len()));
    let invalid = if bindings.version != 1 {
        Some(b::ErrorKind::Version(bindings.version))
    } else if count.is_none_or(|n| n > limits.bindings) {
        Some(b::ErrorKind::BindingLimit)
    } else {
        None
    };
    if let Some(kind) = invalid {
        return Analysis {
            sources,
            diagnostics: Vec::new(),
            code_sources: Vec::new(),
            result: Err(Error::Bindings(b::Error {
                kind,
                source: None,
                contract: None,
                binding: None,
            })),
        };
    }
    let out = super::component_source(sources, display, bytes, root, compile);
    let sources = out.sources;
    let diagnostics = out.diagnostics;
    let mut code_sources = Vec::new();
    let result = (|| {
        let program = out.result.map_err(Error::Frontend)?;
        let flow = match &program {
            Program::Ownership(program) => program.failure_flow(),
            Program::Tasks(program) => program.verified().plan(),
            Program::Copy(_) => return Err(Error::SourceProfile),
        };
        let previous = document.usage().budget;
        let add = |a: usize, b: usize| {
            a.checked_add(b)
                .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))
        };
        let base = m::Usage {
            work: add(previous.work, flow.work_used())?,
            allocation_bytes: add(previous.allocation_bytes, flow.requested_bytes())?,
            elements: add(previous.elements, flow.elements_used())?,
        };
        let mut budget = m::Budget::with_usage(limits.budget, base)?;
        let mut original_sources = budget.vector(program.source().modules.len())?;
        for module in &program.source().modules {
            budget.charge(original_sources.len().saturating_add(1))?;
            let id = module.span.source();
            if !original_sources.contains(&id) {
                original_sources.push(id);
            }
        }
        let code = match program {
            Program::Ownership(program) => b::Code::Owned((*program).into_failure_flow()),
            Program::Tasks(program) => b::Code::Tasks((*program).into_verified()),
            Program::Copy(_) => return Err(Error::SourceProfile),
        };
        let declarations = code.declarations();
        code_sources = budget.vector(declarations.types().table().sources.len())?;
        for snapshot in &declarations.types().table().sources {
            let mut original = None;
            for id in &original_sources {
                let file = sources.file(*id).map_err(|_| Error::SourceProfile)?;
                budget.charge(
                    snapshot
                        .path
                        .len()
                        .saturating_add(file.name().len())
                        .saturating_add(1),
                )?;
                if file.name() == snapshot.path {
                    budget.charge(
                        snapshot
                            .text
                            .len()
                            .saturating_add(file.text().len())
                            .saturating_add(1),
                    )?;
                    if file.text() == snapshot.text {
                        original = Some(*id);
                        break;
                    }
                }
            }
            code_sources.push(original.ok_or(Error::SourceProfile)?);
        }
        let mappings = resolve(bindings, declarations, &mut budget, limits)?;
        let used = m::Usage {
            work: budget.work_used() - base.work,
            allocation_bytes: budget.allocation_bytes() - base.allocation_bytes,
            elements: budget.elements_used() - base.elements,
        };
        Ok(b::Unit::with_usage(document, code, mappings, limits, used)?)
    })();
    Analysis {
        sources,
        diagnostics,
        code_sources,
        result,
    }
}
fn resolve(
    input: Input,
    declarations: &lang_mir::unit::ownership::declarations::VerifiedDeclarations,
    budget: &mut m::Budget,
    limits: b::Limits,
) -> Result<b::Map, Error> {
    if input.version != 1 {
        return Err(b::Error {
            kind: b::ErrorKind::Version(input.version),
            source: None,
            contract: None,
            binding: None,
        }
        .into());
    }
    let total = input
        .resources
        .len()
        .checked_add(input.imports.len())
        .and_then(|n| n.checked_add(input.exports.len()));
    if total.is_none_or(|n| n > limits.bindings) {
        return Err(b::Error {
            kind: b::ErrorKind::BindingLimit,
            source: None,
            contract: None,
            binding: None,
        }
        .into());
    }
    let mut resources = budget.vector(input.resources.len())?;
    let mut imports = budget.vector(input.imports.len())?;
    let mut exports = budget.vector(input.exports.len())?;
    for (index, entry) in input.resources.into_iter().enumerate() {
        let mut found = None;
        for (id, definition) in declarations.types().table().definitions.iter().enumerate() {
            budget.charge(
                entry
                    .tag
                    .len()
                    .saturating_add(definition.name.len())
                    .saturating_add(1),
            )?;
            if entry.tag == definition.name {
                found = Some(id);
                break;
            }
        }
        let tag = found
            .and_then(|id| u32::try_from(id).ok())
            .ok_or(Error::Name {
                section: b::Section::Resources,
                index,
            })?;
        resources.push(b::ResourceBinding {
            tag: m::DefinitionId(tag),
            resource: entry.resource,
        });
    }
    for (index, entry) in input.imports.into_iter().enumerate() {
        let mut found = None;
        for (id, import) in declarations.imports().iter().enumerate() {
            budget.charge(
                entry
                    .source
                    .len()
                    .saturating_add(import.name.len())
                    .saturating_add(1),
            )?;
            if entry.source == import.name {
                found = Some(id);
                break;
            }
        }
        let source = found
            .and_then(|id| u32::try_from(id).ok())
            .ok_or(Error::Name {
                section: b::Section::Imports,
                index,
            })?;
        imports.push(b::ImportBinding {
            source: m::ImportId(source),
            member: entry.member,
        });
    }
    for (index, entry) in input.exports.into_iter().enumerate() {
        let mut found = None;
        for (id, function) in declarations.functions().iter().enumerate() {
            budget.charge(
                entry
                    .source
                    .len()
                    .saturating_add(function.name.len())
                    .saturating_add(1),
            )?;
            if entry.source == function.name {
                found = Some(id);
                break;
            }
        }
        let source = found
            .and_then(|id| u32::try_from(id).ok())
            .ok_or(Error::Name {
                section: b::Section::Exports,
                index,
            })?;
        exports.push(b::ExportBinding {
            source: m::FunctionId(source),
            member: entry.member,
        });
    }
    Ok(b::Map {
        version: input.version,
        resources,
        imports,
        exports,
    })
}
