//! Compatibility bridge from verified compilation units to the initial native
//! scalar/CFG and straight-line owned subset. Unsupported extended operations
//! are rejected before any backend code allocation/publication.

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_mir::{self as legacy, unit as m};
use lang_runtime::HostFunctionRegistry;

use crate::{CompileError, CompiledFunction, ObjectArtifact};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    InvalidFunction(u32),
    UnsupportedType(m::Type),
    UnsupportedOperation,
    ImportArity,
    UnknownHostFunction(legacy::HostFunctionId),
    SourceMetadata,
    SourceMap(legacy::SourceMapError),
    Budget(m::Error),
    Compile(CompileError),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterError {
    pub kind: ErrorKind,
    pub function: m::FunctionId,
    pub block: Option<m::BlockId>,
    pub instruction: Option<u32>,
}
impl Display for AdapterError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "native unit adapter failed: {:?}", self.kind)
    }
}
impl Error for AdapterError {}

/// Import IDs are supplied in unit-table order. Native symbols remain the
/// existing typed registry ID symbols, not serialized callback addresses.
pub fn compile(
    verified: &m::VerifiedUnit,
    function: m::FunctionId,
    imports: &[legacy::HostFunctionId],
    hosts: &HostFunctionRegistry,
    limits: m::Limits,
) -> Result<CompiledFunction, AdapterError> {
    let scalar = scalar_function(verified, function, imports, limits)?;
    for host in imports {
        if hosts.resolve_i64(*host).is_none() {
            return Err(AdapterError {
                kind: ErrorKind::UnknownHostFunction(*host),
                function,
                block: None,
                instruction: None,
            });
        }
    }
    CompiledFunction::compile_with_hosts(&scalar, hosts).map_err(|e| AdapterError {
        kind: ErrorKind::Compile(e),
        function,
        block: None,
        instruction: None,
    })
}
pub fn emit_host_object(
    verified: &m::VerifiedUnit,
    function: m::FunctionId,
    imports: &[legacy::HostFunctionId],
    limits: m::Limits,
) -> Result<ObjectArtifact, AdapterError> {
    let scalar = scalar_function(verified, function, imports, limits)?;
    crate::emit_host_object(&scalar).map_err(|e| AdapterError {
        kind: ErrorKind::Compile(e),
        function,
        block: None,
        instruction: None,
    })
}

/// Adapt a single verified function, renumbering every register definition/use
/// into the legacy backend's sequential definition order. The returned MIR may
/// also be used for differential checks of the original scalar VM. Native
/// compilation/legacy validation retain their existing trusted-input boundary.
pub fn scalar_function(
    verified: &m::VerifiedUnit,
    function: m::FunctionId,
    imports: &[legacy::HostFunctionId],
    limits: m::Limits,
) -> Result<legacy::Function, AdapterError> {
    let fail = |kind| AdapterError {
        kind,
        function,
        block: None,
        instruction: None,
    };
    let mut budget = m::Budget::new(limits);
    verified
        .revalidate(&mut budget)
        .map_err(|e| fail(ErrorKind::Budget(e)))?;
    let u = verified.unit();
    let f = u
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| fail(ErrorKind::InvalidFunction(function.0)))?;
    if imports.len() != u.imports.len() {
        return Err(fail(ErrorKind::ImportArity));
    }
    let mut adapter = Adapter {
        verified,
        function,
        budget,
    };
    let result = adapter.ty(f.signature.result).map_err(fail)?;
    // The original native profile permits only i64/own at the ABI boundary.
    if matches!(result, legacy::Type::Bool) {
        return Err(fail(ErrorKind::UnsupportedType(f.signature.result)));
    }
    let mut parameters = adapter.vector(f.signature.parameters.len()).map_err(fail)?;
    for t in &f.signature.parameters {
        let converted = adapter.ty(*t).map_err(fail)?;
        if converted == legacy::Type::Bool {
            return Err(fail(ErrorKind::UnsupportedType(*t)));
        }
        parameters.push(converted);
    }
    for t in &f.registers {
        adapter.ty(*t).map_err(fail)?;
    }
    let mut map = adapter.vector(f.registers.len()).map_err(fail)?;
    map.resize(f.registers.len(), legacy::ValueId::new(0));
    let mut next = 0u32;
    for register in map.iter_mut().take(f.signature.parameters.len()) {
        *register = legacy::ValueId::new(next);
        next += 1;
    }
    for block in &f.blocks {
        for r in block
            .parameters
            .iter()
            .copied()
            .chain(block.instructions.iter().map(|i| i.op.destination()))
        {
            adapter.charge(1).map_err(fail)?;
            map[r.0 as usize] = legacy::ValueId::new(next);
            next = next.checked_add(1).ok_or_else(|| {
                fail(ErrorKind::Budget(m::Error::new(
                    m::ErrorKind::ArithmeticOverflow,
                )))
            })?;
        }
    }
    let value = |r: m::Register| map[r.0 as usize];
    let mut blocks = adapter.vector(f.blocks.len()).map_err(fail)?;
    let mut source_blocks = adapter.vector(f.blocks.len()).map_err(fail)?;
    for (bi, b) in f.blocks.iter().enumerate() {
        let located = |kind, instruction| AdapterError {
            kind,
            function,
            block: Some(m::BlockId(bi as u32)),
            instruction,
        };
        let mut parameters = adapter
            .vector(b.parameters.len())
            .map_err(|e| located(e, None))?;
        for r in &b.parameters {
            parameters.push(legacy::BlockParameter::new(
                value(*r),
                adapter
                    .ty(f.registers[r.0 as usize])
                    .map_err(|e| located(e, None))?,
            ));
        }
        let mut instructions = adapter
            .vector(b.instructions.len())
            .map_err(|e| located(e, None))?;
        let mut instruction_spans = adapter
            .vector(b.instructions.len())
            .map_err(|e| located(e, None))?;
        for (ii, i) in b.instructions.iter().enumerate() {
            adapter.charge(1).map_err(|e| located(e, Some(ii as u32)))?;
            let result = value(i.op.destination());
            instructions.push(match &i.op {
                m::Op::I64Constant { value, .. } => legacy::Instruction::I64Constant {
                    result,
                    value: *value,
                },
                m::Op::Add { left, right, .. } => legacy::Instruction::I64AddWrapping {
                    result,
                    left: value(*left),
                    right: value(*right),
                },
                m::Op::LessThan { left, right, .. } => legacy::Instruction::I64LessThanSigned {
                    result,
                    left: value(*left),
                    right: value(*right),
                },
                m::Op::HostCall {
                    import, argument, ..
                } => legacy::Instruction::CallHostI64 {
                    result,
                    function: imports[import.0 as usize],
                    argument: value(*argument),
                },
                _ => return Err(located(ErrorKind::UnsupportedOperation, Some(ii as u32))),
            });
            if f.span.is_some() {
                instruction_spans.push(
                    adapter
                        .span(i.span)
                        .map_err(|e| located(e, Some(ii as u32)))?,
                );
            } else if i.span.is_some() {
                return Err(located(ErrorKind::SourceMetadata, Some(ii as u32)));
            }
        }
        let terminator = match &b.terminator.kind {
            m::Term::Return(r) => legacy::Terminator::Return(value(*r)),
            m::Term::Jump(t) => {
                legacy::Terminator::Jump(adapter.target(t, &map).map_err(|e| located(e, None))?)
            }
            m::Term::Branch {
                condition,
                then_target,
                else_target,
            } => legacy::Terminator::Branch {
                condition: value(*condition),
                then_target: adapter
                    .target(then_target, &map)
                    .map_err(|e| located(e, None))?,
                else_target: adapter
                    .target(else_target, &map)
                    .map_err(|e| located(e, None))?,
            },
        };
        if f.span.is_some() {
            source_blocks.push(legacy::BlockSourceMap::new(
                instruction_spans,
                adapter
                    .span(b.terminator.span)
                    .map_err(|e| located(e, None))?,
            ));
        } else if b.terminator.span.is_some() {
            return Err(located(ErrorKind::SourceMetadata, None));
        }
        blocks.push(legacy::BasicBlock::with_parameters(
            parameters,
            instructions,
            terminator,
        ));
    }
    let name = adapter.text(&f.name).map_err(fail)?;
    let mut out =
        legacy::Function::with_blocks(name, legacy::Signature::new(parameters, result), blocks);
    if let Some(span) = f.span {
        let path = adapter
            .text(&u.sources[span.source.0 as usize].path)
            .map_err(fail)?;
        let span = adapter.span(Some(span)).map_err(fail)?;
        let source_map = legacy::FunctionSourceMap::new(path, span, source_blocks)
            .map_err(|e| fail(ErrorKind::SourceMap(e)))?;
        out = out
            .with_source_map(source_map)
            .map_err(|e| fail(ErrorKind::SourceMap(e)))?;
    }
    Ok(out)
}

struct Adapter<'a> {
    verified: &'a m::VerifiedUnit,
    function: m::FunctionId,
    budget: m::Budget,
}
impl Adapter<'_> {
    fn charge(&mut self, n: usize) -> Result<(), ErrorKind> {
        self.budget.charge(n).map_err(ErrorKind::Budget)
    }
    fn vector<T>(&mut self, n: usize) -> Result<Vec<T>, ErrorKind> {
        self.charge(n)?;
        self.budget.vector(n).map_err(ErrorKind::Budget)
    }
    fn text(&mut self, s: &str) -> Result<String, ErrorKind> {
        self.charge(s.len())?;
        self.budget.bytes(s.len()).map_err(ErrorKind::Budget)?;
        let mut text = String::new();
        text.try_reserve_exact(s.len())
            .map_err(|_| ErrorKind::Budget(m::Error::new(m::ErrorKind::AllocationFailed)))?;
        text.push_str(s);
        Ok(text)
    }
    fn ty(&mut self, t: m::Type) -> Result<legacy::Type, ErrorKind> {
        self.charge(1)?;
        match t {
            m::Type::I64 => Ok(legacy::Type::I64),
            m::Type::Bool => Ok(legacy::Type::Bool),
            m::Type::Own(r) => Ok(legacy::Type::Own(r)),
            _ => Err(ErrorKind::UnsupportedType(t)),
        }
    }
    fn target(
        &mut self,
        t: &m::Target,
        map: &[legacy::ValueId],
    ) -> Result<legacy::BlockTarget, ErrorKind> {
        let mut arguments = self.vector(t.arguments.len())?;
        for r in &t.arguments {
            arguments.push(map[r.0 as usize]);
        }
        Ok(legacy::BlockTarget::new(
            legacy::BlockId::new(t.block.0),
            arguments,
        ))
    }
    fn span(&mut self, span: Option<m::Span>) -> Result<legacy::SourceSpan, ErrorKind> {
        let span = span.ok_or(ErrorKind::SourceMetadata)?;
        let owner = self.verified.unit().functions[self.function.0 as usize]
            .span
            .ok_or(ErrorKind::SourceMetadata)?;
        if owner.source != span.source {
            return Err(ErrorKind::SourceMetadata);
        }
        // SourceFacts position lookup scans the line prefix for Unicode scalar
        // columns. Charging the byte prefixes conservatively bounds both scans.
        self.charge(span.start as usize + 1)?;
        self.charge(span.end as usize + 1)?;
        let source = &self.verified.unit().sources[span.source.0 as usize];
        let facts = &self.verified.facts().sources()[span.source.0 as usize];
        let (line, column) = facts
            .position(source, span.start)
            .ok_or(ErrorKind::SourceMetadata)?;
        let start = legacy::SourcePosition::new(line, column).map_err(ErrorKind::SourceMap)?;
        let (line, column) = facts
            .position(source, span.end)
            .ok_or(ErrorKind::SourceMetadata)?;
        let end = legacy::SourcePosition::new(line, column).map_err(ErrorKind::SourceMap)?;
        legacy::SourceSpan::new(start, end).map_err(ErrorKind::SourceMap)
    }
}
