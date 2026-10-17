//! Shared host-facing source analysis; snapshots survive diagnostics/failure.
use lang_bytecode::unit::Program;
use lang_diagnostics::{Diagnostic, Label, Severity, SourceMap, codes};
use lang_hir::{
    layout::LayoutError,
    program::{CheckError, CheckLimits, TypedProgram, check_expression_function, check_program},
    resolved as r,
};
use lang_mir::unit as m;
use lang_semantics::{
    binding::{BindingError, BindingLimits, bind_program},
    modules::{ModuleAnalysis, ModuleLimits, ModuleLoadError, load_modules, load_virtual},
    program_lowering::{self, LowerError},
};
use lang_syntax::{LexError, ParseError};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default)]
pub struct CompileLimits {
    pub modules: ModuleLimits,
    pub binding: BindingLimits,
    pub typing: CheckLimits,
    pub mir: m::Limits,
}
#[derive(Debug)]
pub enum AnalysisError {
    Diagnostics,
    Module(ModuleLoadError),
    Binding(BindingError),
    Typing(CheckError),
    MissingWrapper,
}
impl Display for AnalysisError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diagnostics => f.write_str("source diagnostics prevent compilation"),
            Self::Module(ModuleLoadError::Input { path, error }) => {
                if let Some(code) = error.code() {
                    write!(f, "{}: {:?}: {error}", code.as_str(), path)
                } else {
                    write!(f, "{path:?}: {error}")
                }
            }
            Self::Module(e) => Display::fmt(e, f),
            Self::Binding(e) => Display::fmt(e, f),
            Self::Typing(e) => Display::fmt(e, f),
            Self::MissingWrapper => f.write_str("missing generated expression wrapper"),
        }
    }
}
impl Error for AnalysisError {}
#[derive(Debug)]
pub struct Analysis {
    pub sources: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
    pub result: Result<TypedProgram, AnalysisError>,
}
impl Analysis {
    fn failure(sources: SourceMap, mut diagnostics: Vec<Diagnostic>, error: AnalysisError) -> Self {
        let error = match error {
            AnalysisError::Module(
                ModuleLoadError::Limit(d)
                | ModuleLoadError::Parse(ParseError::Limit(d) | ParseError::Lex(LexError::Limit(d))),
            )
            | AnalysisError::Binding(BindingError::Limit(d))
            | AnalysisError::Typing(
                CheckError::Limit(d)
                | CheckError::Layout(LayoutError::Limit(d))
                | CheckError::Types(lang_hir::types::Error::Limit(d)),
            ) => {
                diagnostics.push(*d);
                AnalysisError::Diagnostics
            }
            error => error,
        };
        Self {
            sources,
            diagnostics,
            result: Err(error),
        }
    }
}
pub fn analyze_file(file: &Path, root: &Path, limits: CompileLimits) -> Analysis {
    let mut sources = SourceMap::new();
    let modules = load_modules(&mut sources, file, root, limits.modules);
    finish(sources, modules, limits, None)
}
pub fn analyze_virtual(
    mut sources: SourceMap,
    display: &Path,
    bytes: &[u8],
    root: Option<&Path>,
    limits: CompileLimits,
    wrapper: Option<&str>,
) -> Analysis {
    let modules = load_virtual(&mut sources, display, bytes, root, limits.modules);
    finish(sources, modules, limits, wrapper)
}
fn finish(
    sources: SourceMap,
    modules: Result<ModuleAnalysis, ModuleLoadError>,
    limits: CompileLimits,
    wrapper: Option<&str>,
) -> Analysis {
    let modules = match modules {
        Ok(m) => m,
        Err(e) => return Analysis::failure(sources, vec![], AnalysisError::Module(e)),
    };
    let mut diagnostics = modules.diagnostics;
    let Some(graph) = modules.graph else {
        return Analysis::failure(sources, diagnostics, AnalysisError::Diagnostics);
    };
    let bound = match bind_program(&graph, &sources, limits.binding) {
        Ok(b) => b,
        Err(e) => return Analysis::failure(sources, diagnostics, AnalysisError::Binding(e)),
    };
    diagnostics.extend(bound.diagnostics);
    let Some(program) = bound.program else {
        return Analysis::failure(sources, diagnostics, AnalysisError::Diagnostics);
    };
    let checked = if let Some(wrapper) = wrapper {
        let Some(index) = program
            .functions
            .iter()
            .position(|f| f.qualified_name == wrapper)
        else {
            return Analysis::failure(sources, diagnostics, AnalysisError::MissingWrapper);
        };
        check_expression_function(program, &sources, limits.typing, r::FunctionId(index))
    } else {
        check_program(program, &sources, limits.typing)
    };
    let checked = match checked {
        Ok(c) => c,
        Err(e) => return Analysis::failure(sources, diagnostics, AnalysisError::Typing(e)),
    };
    diagnostics.extend(checked.diagnostics);
    Analysis {
        sources,
        diagnostics,
        result: checked.program.ok_or(AnalysisError::Diagnostics),
    }
}
#[derive(Debug)]
pub enum ScriptError {
    Entry(Box<Diagnostic>),
    Lower(LowerError),
}
impl Display for ScriptError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Entry(d) => write!(f, "{}: {}", d.code.as_str(), d.message),
            Self::Lower(e) => Display::fmt(e, f),
        }
    }
}
impl Error for ScriptError {}
pub fn compile_script(
    typed: &TypedProgram,
    sources: &SourceMap,
    limits: CompileLimits,
) -> Result<Program, ScriptError> {
    let main = typed
        .source()
        .functions
        .iter()
        .find(|f| f.qualified_name == "main");
    if !main.is_some_and(|f| f.parameters.is_empty() && f.result.value_type == r::Type::I64) {
        let span = if let Some(main) = main {
            main.signature_span
        } else {
            let root = typed.source().modules[0].span;
            sources
                .span(root.source(), root.range().end..root.range().end)
                .expect("checked root span")
        };
        return Err(ScriptError::Entry(Box::new(Diagnostic::new(
            codes::INVALID_ENTRY,
            Severity::Error,
            "script entry must be fn main() -> i64",
            Label::new(span, "missing or invalid script entry"),
        ))));
    }
    program_lowering::lower(typed, sources, limits.mir)
        .map(Program::from_verified)
        .map_err(ScriptError::Lower)
}
