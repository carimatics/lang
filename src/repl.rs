//! Transactional source sessions. Only successfully analyzed items persist;
//! expression evaluation owns fresh VM counters and no mutable session values.
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use lang_bytecode::unit::Program;
use lang_diagnostics::{
    Diagnostic, Label, Severity, SourceError, SourceId, SourceMap, Span, codes,
};
use lang_mir::unit as m;
use lang_runtime::{HostFunctionRegistry, allocation as a};
use lang_semantics::{
    analysis::Program as SourceProgram,
    program_lowering::{self, LowerError},
};
use lang_syntax::{
    InputError, LexError, ParseError, SyntaxElement, TokenKind as K, add_source, lex, parse,
    parse_expression,
};
use lang_vm::unit::{self as vm, Control, HostBindings, Value};

use crate::frontend::{self, AnalysisError, CompileLimits};

mod owned;
pub use owned::{CleanupFailure, CleanupReport, OwnedRuntimeFailure};

#[derive(Clone, Copy, Debug)]
pub struct ReplLimits {
    pub compile: CompileLimits,
    pub display_bytes: usize,
}
impl Default for ReplLimits {
    fn default() -> Self {
        Self {
            compile: CompileLimits::default(),
            display_bytes: 65_536,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    Empty,
    ItemCommitted,
    Value(String),
    Reset,
    Quit,
}
#[derive(Debug)]
pub enum ReplError {
    Diagnostics,
    Input(InputError),
    Source(SourceError),
    Analysis(AnalysisError),
    Parse(ParseError),
    Lex(LexError),
    Lower(LowerError),
    Runtime(Box<RuntimeFailure>),
    OwnedRuntime(Box<OwnedRuntimeFailure>),
    Allocation,
    DisplayLimit,
}
impl Display for ReplError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diagnostics => f.write_str("submission diagnostics prevent execution"),
            Self::Input(e) => {
                if let Some(code) = e.code() {
                    write!(f, "{}: {e}", code.as_str())
                } else {
                    Display::fmt(e, f)
                }
            }
            Self::Source(e) => Display::fmt(e, f),
            Self::Analysis(e) => Display::fmt(e, f),
            Self::Parse(e) => Display::fmt(e, f),
            Self::Lex(e) => Display::fmt(e, f),
            Self::Lower(e) => Display::fmt(e, f),
            Self::Runtime(e) => write!(
                f,
                "{:?} (fuel {}, host calls {})",
                e.reason, e.usage.fuel, e.usage.host_calls
            ),
            Self::OwnedRuntime(e) => Display::fmt(e, f),
            Self::Allocation => f.write_str("REPL allocation failed"),
            Self::DisplayLimit => f.write_str("REPL display limit exceeded"),
        }
    }
}
impl Error for ReplError {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame {
    pub function: m::FunctionId,
    pub span: Option<Span>,
}
#[derive(Debug)]
pub struct RuntimeFailure {
    pub reason: vm::Reason,
    pub usage: vm::Usage,
    pub fault: Option<Span>,
    pub stack: Vec<Frame>,
    pub trace_complete: bool,
    program: Program,
    wrapper: m::FunctionId,
}
impl RuntimeFailure {
    pub fn function_name(&self, frame: Frame) -> &str {
        if frame.function == self.wrapper {
            "<expression>"
        } else {
            &self.program.verified().unit().functions[frame.function.0 as usize].name
        }
    }
}
#[derive(Debug)]
pub struct Report {
    pub sources: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
    pub result: Result<Event, ReplError>,
}
impl Report {
    fn failure(sources: SourceMap, mut diagnostics: Vec<Diagnostic>, error: ReplError) -> Self {
        let error = match error {
            ReplError::Parse(ParseError::Limit(d) | ParseError::Lex(LexError::Limit(d)))
            | ReplError::Lex(LexError::Limit(d)) => {
                diagnostics.push(*d);
                ReplError::Diagnostics
            }
            ReplError::Analysis(AnalysisError::Diagnostics) => ReplError::Diagnostics,
            error => error,
        };
        Self {
            sources,
            diagnostics,
            result: Err(error),
        }
    }
}
struct Item {
    text: String,
    number: usize,
}
pub struct Session {
    items: Vec<Item>,
    next: usize,
    source_root: Option<PathBuf>,
    limits: ReplLimits,
}
impl Default for Session {
    fn default() -> Self {
        Self::new(None, ReplLimits::default())
    }
}
impl Session {
    pub fn new(source_root: Option<PathBuf>, limits: ReplLimits) -> Self {
        Self {
            items: Vec::new(),
            next: 1,
            source_root,
            limits,
        }
    }
    pub fn definition_count(&self) -> usize {
        self.items.len()
    }
    pub const fn limits(&self) -> ReplLimits {
        self.limits
    }
    pub fn submit(
        &mut self,
        bytes: &[u8],
        hosts: &HostFunctionRegistry,
        bindings: &HostBindings,
        control: Control<'_>,
    ) -> Report {
        self.submit_internal::<a::System>(bytes, hosts, bindings, None, control)
    }
    /// Explicitly authorize ownership execution with this allocator's provider
    /// and cumulative quotas. Each expression has a fresh cleanup instance.
    /// Items commit after analysis and require no runtime allocator authority.
    pub fn submit_with_allocator<P: a::Provider>(
        &mut self,
        bytes: &[u8],
        hosts: &HostFunctionRegistry,
        bindings: &HostBindings,
        allocator: &mut a::Allocator<P>,
        control: Control<'_>,
    ) -> Report {
        self.submit_internal(bytes, hosts, bindings, Some(allocator), control)
    }
    fn submit_internal<P: a::Provider>(
        &mut self,
        bytes: &[u8],
        hosts: &HostFunctionRegistry,
        bindings: &HostBindings,
        allocator: Option<&mut a::Allocator<P>>,
        control: Control<'_>,
    ) -> Report {
        let mut sources = SourceMap::new();
        let command = std::str::from_utf8(bytes)
            .ok()
            .map(|text| text.trim_matches(|c: char| c.is_ascii_whitespace()));
        if matches!(command, Some(":reset" | ":quit")) {
            let event = if command == Some(":reset") {
                self.items.clear();
                self.next = 1;
                Event::Reset
            } else {
                Event::Quit
            };
            return Report {
                sources,
                diagnostics: Vec::new(),
                result: Ok(event),
            };
        }
        let number = self.next;
        self.next = match self.next.checked_add(1) {
            Some(n) => n,
            None => return Report::failure(sources, vec![], ReplError::Allocation),
        };
        let source = match add_source(
            &mut sources,
            format!("repl/{number}.lang"),
            bytes,
            self.limits.compile.modules.parse.lex,
        ) {
            Ok(source) => source,
            Err(e) => return Report::failure(sources, vec![], ReplError::Input(e)),
        };
        let lexed = match lex(&sources, source, self.limits.compile.modules.parse.lex) {
            Ok(lexed) => lexed,
            Err(e) => return Report::failure(sources, vec![], ReplError::Lex(e)),
        };
        if lexed
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            return Report::failure(sources, lexed.diagnostics, ReplError::Diagnostics);
        }
        let first = lexed
            .tokens
            .iter()
            .find(|t| !t.kind.is_trivia())
            .map(|t| t.kind);
        let Some(first) = first else {
            return Report {
                sources,
                diagnostics: lexed.diagnostics,
                result: Ok(Event::Empty),
            };
        };
        let item = matches!(
            first,
            K::Pub | K::Fn | K::Async | K::Struct | K::Enum | K::Mod | K::Use | K::Extern
        );
        let parsed = if item {
            parse(&sources, source, self.limits.compile.modules.parse)
        } else {
            parse_expression(&sources, source, self.limits.compile.modules.parse)
        };
        let parsed = match parsed {
            Ok(p) => p,
            Err(e) => return Report::failure(sources, vec![], ReplError::Parse(e)),
        };
        if parsed.has_errors() {
            return Report::failure(
                sources,
                parsed.diagnostics().to_vec(),
                ReplError::Diagnostics,
            );
        }
        if item {
            let nodes: Vec<_> = parsed
                .node(parsed.root())
                .unwrap()
                .children()
                .iter()
                .filter_map(|e| {
                    if let SyntaxElement::Node(id) = e {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect();
            if nodes.len() != 1 {
                let span = nodes.get(1).map_or_else(
                    || sources.span(source, bytes.len()..bytes.len()).unwrap(),
                    |id| parsed.node(*id).unwrap().span(),
                );
                return Report::failure(
                    sources,
                    vec![Diagnostic::new(
                        codes::EXPECTED_SYNTAX,
                        Severity::Error,
                        "submit exactly one item",
                        Label::new(span, "extra item"),
                    )],
                    ReplError::Diagnostics,
                );
            }
        }
        let text = sources.file(source).unwrap().text();
        let wrapper = if item {
            None
        } else {
            match wrapper_name(&self.items, text, self.limits.compile.modules.parse.work) {
                Some(name) => Some(name),
                None => return resource(sources, source, "REPL preparation work limit exceeded"),
            }
        };
        let header = wrapper
            .as_ref()
            .map(|name| format!("fn {name}() -> i64 {{ return "));
        let prefix = self.items.iter().try_fold(0usize, |n, item| {
            n.checked_add(item.text.len())?.checked_add(1)
        });
        let total = prefix
            .and_then(|n| n.checked_add(bytes.len()))
            .and_then(|n| n.checked_add(header.as_ref().map_or(1, |s| s.len() + 5)));
        let Some(total) =
            total.filter(|n| *n <= self.limits.compile.modules.parse.lex.source_bytes)
        else {
            return resource(sources, source, "REPL source limit exceeded");
        };
        let mut combined = String::new();
        if combined.try_reserve_exact(total).is_err() {
            return Report::failure(sources, vec![], ReplError::Allocation);
        }
        let mut origins = Origins {
            root: None,
            current: source,
            segments: Vec::new(),
        };
        if origins
            .segments
            .try_reserve_exact(self.items.len() + 1)
            .is_err()
        {
            return Report::failure(sources, vec![], ReplError::Allocation);
        }
        for previous in &self.items {
            let previous_source =
                match sources.add(format!("repl/{}.lang", previous.number), &previous.text) {
                    Ok(id) => id,
                    Err(e) => return Report::failure(sources, vec![], ReplError::Source(e)),
                };
            origins.append(&mut combined, &previous.text, previous_source);
            combined.push('\n');
        }
        if let Some(header) = header {
            combined.push_str(&header);
        }
        origins.append(
            &mut combined,
            std::str::from_utf8(bytes).expect("validated source"),
            source,
        );
        combined.push_str(if item { "\n" } else { "\n; }\n" });
        let analysis = frontend::analyze_virtual(
            sources,
            Path::new("repl.lang"),
            combined.as_bytes(),
            self.source_root.as_deref(),
            self.limits.compile,
            wrapper.as_deref(),
        );
        let sources = analysis.sources;
        origins.root = sources
            .iter()
            .filter(|(_, file)| file.name() == "repl.lang")
            .map(|(id, _)| id)
            .last();
        let diagnostics = analysis
            .diagnostics
            .into_iter()
            .map(|d| origins.diagnostic(d, &sources))
            .collect();
        let typed = match analysis.result {
            Ok(typed) => typed,
            Err(e) => return Report::failure(sources, diagnostics, ReplError::Analysis(e)),
        };
        let unit = match &typed {
            SourceProgram::Copy(typed) => {
                match program_lowering::lower_library(typed, &sources, self.limits.compile.mir) {
                    Ok(unit) => Some(unit),
                    Err(e) => return Report::failure(sources, diagnostics, ReplError::Lower(e)),
                }
            }
            // Ownership analysis already includes independent MIR validation.
            SourceProgram::Ownership(_) => None,
        };
        if item {
            if self.items.try_reserve(1).is_err() {
                return Report::failure(sources, diagnostics, ReplError::Allocation);
            }
            self.items.push(Item {
                text: std::str::from_utf8(bytes).unwrap().into(),
                number,
            });
            return Report {
                sources,
                diagnostics,
                result: Ok(Event::ItemCommitted),
            };
        }
        let name = wrapper.expect("expression wrapper");
        if let SourceProgram::Ownership(program) = typed {
            return owned::Evaluation {
                program,
                sources,
                diagnostics,
                origins,
                wrapper: name,
                display_bytes: self.limits.display_bytes,
            }
            .execute(allocator, control);
        }
        let unit = unit.expect("Copy library lowering");
        let function = m::FunctionId(
            unit.unit()
                .functions
                .iter()
                .position(|f| f.name == name)
                .expect("checked wrapper") as u32,
        );
        let program = Program::from_verified(unit);
        match vm::execute(&program, function, &[], hosts, bindings, control) {
            Ok(outcome) => match display(&program, &outcome.value, self.limits.display_bytes) {
                Ok(value) => Report {
                    sources,
                    diagnostics,
                    result: Ok(Event::Value(value)),
                },
                Err(e) => Report::failure(sources, diagnostics, e),
            },
            Err(error) => {
                let mapped =
                    |span| origins.mir_span(span, &program.verified().unit().sources, &sources);
                let mut stack = Vec::new();
                let trace_complete =
                    error.trace_complete && stack.try_reserve_exact(error.stack.len()).is_ok();
                if trace_complete {
                    for frame in &error.stack {
                        stack.push(Frame {
                            function: frame.function,
                            span: frame.span.map(mapped),
                        });
                    }
                }
                let error = RuntimeFailure {
                    reason: error.reason,
                    usage: error.usage,
                    fault: error.fault.map(mapped),
                    stack,
                    trace_complete,
                    program,
                    wrapper: function,
                };
                Report::failure(sources, diagnostics, ReplError::Runtime(Box::new(error)))
            }
        }
    }
}
fn resource(sources: SourceMap, source: SourceId, message: &str) -> Report {
    let end = sources.file(source).unwrap().text().len();
    let span = sources.span(source, end..end).unwrap();
    Report::failure(
        sources,
        vec![Diagnostic::new(
            codes::RESOURCE_LIMIT,
            Severity::Error,
            message,
            Label::new(span, message),
        )],
        ReplError::Diagnostics,
    )
}
fn wrapper_name(items: &[Item], current: &str, mut work: usize) -> Option<String> {
    let mut longest = 0usize;
    for text in items
        .iter()
        .map(|i| i.text.as_str())
        .chain(std::iter::once(current))
    {
        work = work.checked_sub(text.len())?;
        let mut run = 0;
        for byte in text.bytes() {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                run += 1;
                longest = longest.max(run);
            } else {
                run = 0;
            }
        }
    }
    // A name longer than every existing identifier cannot collide, including
    // identifiers explicitly called by the submitted expression. This needs
    // one linear byte scan and no symbol set or repeated candidate searches.
    let suffix = longest.checked_add(1)?;
    let length = "__repl_eval_".len().checked_add(suffix)?;
    work.checked_sub(length)?;
    let mut name = String::new();
    name.try_reserve_exact(length).ok()?;
    name.push_str("__repl_eval_");
    name.extend(std::iter::repeat_n('_', suffix));
    Some(name)
}
struct Segment {
    start: usize,
    end: usize,
    source: SourceId,
}
struct Origins {
    root: Option<SourceId>,
    current: SourceId,
    segments: Vec<Segment>,
}
impl Origins {
    fn mir_span(&self, span: m::Span, mir: &[m::Source], sources: &SourceMap) -> Span {
        let path = &mir[span.source.0 as usize].path;
        let id = sources
            .iter()
            .filter(|(_, file)| file.name() == path)
            .map(|(id, _)| id)
            .last()
            .expect("snapshot retained");
        self.span(
            sources
                .span(id, span.start as usize..span.end as usize)
                .expect("verified span"),
            sources,
        )
    }
    fn append(&mut self, text: &mut String, value: &str, source: SourceId) {
        let start = text.len();
        text.push_str(value);
        self.segments.push(Segment {
            start,
            end: text.len(),
            source,
        });
    }
    fn span(&self, span: Span, sources: &SourceMap) -> Span {
        if Some(span.source()) != self.root {
            return span;
        }
        let range = span.range();
        let segment = self
            .segments
            .iter()
            .find(|s| range.start >= s.start && range.start <= s.end)
            .or_else(|| {
                self.segments
                    .iter()
                    .find(|s| range.start < s.end && range.end > s.start)
            });
        if let Some(s) = segment {
            let start = range.start.clamp(s.start, s.end) - s.start;
            let end = range.end.clamp(s.start, s.end) - s.start;
            return sources
                .span(s.source, start..end)
                .expect("original UTF-8 boundaries");
        }
        let end = sources.file(self.current).unwrap().text().len();
        sources.span(self.current, end..end).unwrap()
    }
    fn diagnostic(&self, mut diagnostic: Diagnostic, sources: &SourceMap) -> Diagnostic {
        diagnostic.primary.span = self.span(diagnostic.primary.span, sources);
        for label in &mut diagnostic.secondary {
            label.span = self.span(label.span, sources);
        }
        for fix in &mut diagnostic.fixes {
            for edit in &mut fix.edits {
                edit.span = self.span(edit.span, sources);
            }
        }
        diagnostic
    }
}

enum Print<'a> {
    Value(m::Type, usize),
    Text(&'a str),
}
fn display(program: &Program, value: &Value, limit: usize) -> Result<String, ReplError> {
    let verified = program.verified();
    let u = verified.unit();
    let facts = verified.facts();
    let cells = value.cells();
    let mut pending = Vec::new();
    pending
        .try_reserve_exact(
            cells
                .len()
                .checked_mul(5)
                .and_then(|n| n.checked_add(1))
                .ok_or(ReplError::DisplayLimit)?,
        )
        .map_err(|_| ReplError::Allocation)?;
    pending.push(Print::Value(value.value_type(), 0));
    let mut output = String::new();
    let append = |output: &mut String, text: &str| -> Result<(), ReplError> {
        if output
            .len()
            .checked_add(text.len())
            .is_none_or(|n| n > limit)
        {
            return Err(ReplError::DisplayLimit);
        }
        output
            .try_reserve(text.len())
            .map_err(|_| ReplError::Allocation)?;
        output.push_str(text);
        Ok(())
    };
    while let Some(next) = pending.pop() {
        match next {
            Print::Text(text) => append(&mut output, text)?,
            Print::Value(m::Type::I64, start) => {
                append(&mut output, &(cells[start] as i64).to_string())?
            }
            Print::Value(m::Type::Bool, start) => append(
                &mut output,
                if cells[start] == 0 { "false" } else { "true" },
            )?,
            Print::Value(m::Type::Own(_), _) => return Err(ReplError::DisplayLimit),
            Print::Value(m::Type::Record(d), start) => {
                let definition = &u.definitions[d.0 as usize];
                let m::DefinitionKind::Record(fields) = &definition.kind else {
                    unreachable!()
                };
                append(&mut output, &definition.name)?;
                append(&mut output, if fields.is_empty() { " {}" } else { " { " })?;
                if !fields.is_empty() {
                    pending.push(Print::Text(" }"));
                }
                for (index, field) in fields.iter().enumerate().rev() {
                    pending.push(Print::Value(
                        field.value_type,
                        start + facts.layouts()[d.0 as usize].fields()[index],
                    ));
                    pending.push(Print::Text(": "));
                    pending.push(Print::Text(&field.name));
                    if index > 0 {
                        pending.push(Print::Text(", "));
                    }
                }
            }
            Print::Value(m::Type::Enum(d), start) => {
                let definition = &u.definitions[d.0 as usize];
                let m::DefinitionKind::Enum(cases) = &definition.kind else {
                    unreachable!()
                };
                let index = cells[start] as usize;
                let case = &cases[index];
                append(&mut output, &definition.name)?;
                append(&mut output, "::")?;
                append(&mut output, &case.name)?;
                if !case.payload.is_empty() {
                    append(&mut output, "(")?;
                    pending.push(Print::Text(")"));
                }
                for (field, t) in case.payload.iter().enumerate().rev() {
                    pending.push(Print::Value(
                        *t,
                        start + facts.layouts()[d.0 as usize].cases()[index][field],
                    ));
                    if field > 0 {
                        pending.push(Print::Text(", "));
                    }
                }
            }
        }
    }
    Ok(output)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Readiness {
    Complete,
    Continue,
}
#[derive(Default)]
pub struct SubmissionBuffer {
    bytes: Vec<u8>,
    scanned: usize,
    comment: Comment,
    delimiters: Vec<u8>,
    invalid: bool,
}
#[derive(Default)]
enum Comment {
    #[default]
    None,
    Line,
    Block(usize),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferLimit;
impl SubmissionBuffer {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    /// Scan only appended bytes. A delimiter/comment limit hands the complete
    /// submission to the bounded parser for its normal resource diagnostic.
    pub fn push(
        &mut self,
        bytes: &[u8],
        byte_limit: usize,
        depth: usize,
    ) -> Result<Readiness, BufferLimit> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > byte_limit)
        {
            return Err(BufferLimit);
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| BufferLimit)?;
        self.bytes.extend_from_slice(bytes);
        while self.scanned < self.bytes.len() && !self.invalid {
            let index = self.scanned;
            let byte = self.bytes[index];
            let next = self.bytes.get(index + 1).copied();
            if next.is_none() && matches!(byte, b'/' | b'*') {
                break;
            }
            match self.comment {
                Comment::Line => {
                    if byte == b'\n' {
                        self.comment = Comment::None;
                    }
                    self.scanned += 1;
                }
                Comment::Block(n) => {
                    if byte == b'/' && next == Some(b'*') {
                        if n >= depth.min(256) {
                            self.invalid = true;
                        } else {
                            self.comment = Comment::Block(n + 1);
                        }
                        self.scanned += 2;
                    } else if byte == b'*' && next == Some(b'/') {
                        self.comment = if n == 1 {
                            Comment::None
                        } else {
                            Comment::Block(n - 1)
                        };
                        self.scanned += 2;
                    } else {
                        self.scanned += 1;
                    }
                }
                Comment::None => {
                    if byte == b'/' && next == Some(b'/') {
                        self.comment = Comment::Line;
                        self.scanned += 2;
                    } else if byte == b'/' && next == Some(b'*') {
                        if depth == 0 {
                            self.invalid = true;
                        }
                        self.comment = Comment::Block(1);
                        self.scanned += 2;
                    } else {
                        if matches!(byte, b'(' | b'{') {
                            if self.delimiters.len() >= depth.min(256) {
                                self.invalid = true;
                            } else {
                                self.delimiters.push(byte);
                            }
                        } else if matches!(byte, b')' | b'}')
                            && self.delimiters.pop() != Some(if byte == b')' { b'(' } else { b'{' })
                        {
                            self.invalid = true;
                        }
                        self.scanned += 1;
                    }
                }
            }
        }
        Ok(
            if !self.invalid
                && (!self.delimiters.is_empty() || matches!(self.comment, Comment::Block(_)))
            {
                Readiness::Continue
            } else {
                Readiness::Complete
            },
        )
    }
}
