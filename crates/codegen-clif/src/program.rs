//! Whole-unit native lowering for the RFC 0006 copy-value profile.
//!
//! The cell/pointer calling convention is private to this backend. It is not a
//! source-language C ABI or a serialized layout. Native execution is trusted,
//! synchronous execution; VM fuel, frame and interruption limits do not apply.

use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

use cranelift_codegen::ir::{
    AbiParam, InstBuilder, MemFlagsData, SourceLoc, StackSlot, StackSlotData, StackSlotKind,
    Value as IrValue, condcodes::IntCC, types,
};
use cranelift_codegen::isa::{self, OwnedTargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use lang_mir::{HostFunctionId, unit as m};
use lang_runtime::HostFunctionRegistry;

use crate::{OwnedJitModule, Triple, host_symbol_name};

mod recorded;
pub use recorded::{BindError, LinkedProgram, PreparedObject};

// Explicit compiler capacity, independent of MIR's semantic value-cell limit.
// Large frames require a different lowering strategy, not unchecked offsets.
const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Verification(m::Error),
    UnsupportedOwnedType,
    ImportArity,
    UnknownHostFunction(HostFunctionId),
    NativeCapacity,
    Backend(String),
    SourceDebug(crate::SourceDebugError),
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "native program compilation failed: {self:?}")
    }
}
impl StdError for Error {}
impl From<m::Error> for Error {
    fn from(e: m::Error) -> Self {
        Self::Verification(e)
    }
}
fn backend(e: impl Display) -> Error {
    Error::Backend(e.to_string())
}

/// Flattened values use one i64 cell per scalar, a zero record header, and an
/// enum case index followed by payload cells and zero padding. All inputs are
/// checked again against the invoked program, including nominal identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Value {
    pub value_type: m::Type,
    pub cells: Vec<i64>,
}
impl Value {
    pub fn i64(value: i64) -> Self {
        Self {
            value_type: m::Type::I64,
            cells: vec![value],
        }
    }
    pub fn boolean(value: bool) -> Self {
        Self {
            value_type: m::Type::Bool,
            cells: vec![i64::from(value)],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvokeError {
    InvalidFunction(m::FunctionId),
    InvalidArguments,
    InvalidVariantProjection,
    InvalidNativeResult,
    AllocationFailed,
}
impl Display for InvokeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "native program invocation failed: {self:?}")
    }
}
impl StdError for InvokeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRange {
    pub start: u32,
    pub end: u32,
    pub span: m::Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionCode {
    pub function: m::FunctionId,
    pub symbol: String,
    pub code_size: usize,
    pub source_ranges: Vec<SourceRange>,
}

pub struct CompiledProgram<'a> {
    // Dropped only after all calls borrowing self finish. Code pointers never
    // escape; OwnedJitModule also releases unpublished code on errors.
    _module: OwnedJitModule,
    verified: &'a m::VerifiedUnit,
    pointers: Vec<*const u8>,
    functions: Vec<FunctionCode>,
}
impl<'a> CompiledProgram<'a> {
    pub fn compile(
        verified: &'a m::VerifiedUnit,
        imports: &[HostFunctionId],
        hosts: &HostFunctionRegistry,
        limits: m::Limits,
    ) -> Result<Self, Error> {
        let mut budget = preflight(verified, imports, limits)?;
        let mut builder = JITBuilder::new(default_libcall_names()).map_err(backend)?;
        for id in imports {
            let host = hosts
                .resolve_i64(*id)
                .ok_or(Error::UnknownHostFunction(*id))?;
            builder.symbol(host_symbol_name(*id), host as *const u8);
        }
        let mut owner = OwnedJitModule(Some(JITModule::new(builder)));
        let module = owner.0.as_mut().expect("new module");
        let (ids, functions) = define(module, verified, imports, &mut budget)?;
        module.finalize_definitions().map_err(backend)?;
        let mut pointers = budget.vector(ids.len())?;
        for id in &ids {
            pointers.push(module.get_finalized_function(*id));
        }
        Ok(Self {
            _module: owner,
            verified,
            pointers,
            functions,
        })
    }
    pub fn functions(&self) -> &[FunctionCode] {
        &self.functions
    }
    pub fn invoke(
        &self,
        function: m::FunctionId,
        arguments: &[Value],
    ) -> Result<Value, InvokeError> {
        invoke(self.verified, &self.pointers, function, arguments)
    }
}

fn invoke(
    verified: &m::VerifiedUnit,
    pointers: &[*const u8],
    function: m::FunctionId,
    arguments: &[Value],
) -> Result<Value, InvokeError> {
    let f = verified
        .unit()
        .functions
        .get(function.0 as usize)
        .ok_or(InvokeError::InvalidFunction(function))?;
    if arguments.len() != f.signature.parameters.len() {
        return Err(InvokeError::InvalidArguments);
    }
    let count = parameter_cells(verified, &f.signature);
    let mut inputs = Vec::new();
    inputs
        .try_reserve_exact(count)
        .map_err(|_| InvokeError::AllocationFailed)?;
    for (argument, ty) in arguments.iter().zip(&f.signature.parameters) {
        if argument.value_type != *ty || !valid_cells(verified, *ty, &argument.cells) {
            return Err(InvokeError::InvalidArguments);
        }
        inputs.extend_from_slice(&argument.cells);
    }
    let mut cells = Vec::new();
    let count = cells_for(verified, f.signature.result);
    cells
        .try_reserve_exact(count)
        .map_err(|_| InvokeError::AllocationFailed)?;
    cells.resize(count, 0);
    // SAFETY: Both compiled/bound owners retain matching extern-C (input *const i64, output
    // *mut i64) -> i32 for every private symbol. Verified signatures and
    // canonical values establish exact buffer extents; generated loads and
    // stores use checked, statically bounded offsets. No pointer escapes or
    // is retained by guest code. Invocation borrows the executable owner.
    let status = unsafe {
        let call: unsafe extern "C" fn(*const i64, *mut i64) -> i32 =
            std::mem::transmute(pointers[function.0 as usize]);
        call(inputs.as_ptr(), cells.as_mut_ptr())
    };
    match status {
        0 if valid_cells(verified, f.signature.result, &cells) => Ok(Value {
            value_type: f.signature.result,
            cells,
        }),
        1 => Err(InvokeError::InvalidVariantProjection),
        _ => Err(InvokeError::InvalidNativeResult),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectArtifact {
    pub bytes: Vec<u8>,
    pub functions: Vec<FunctionCode>,
}
impl ObjectArtifact {
    /// Transport these compiler bytes/metadata under RFC 0018. The caller must
    /// supply their actual target and pair semantics from the same program.
    /// A validated record supplies data and never authorizes native execution.
    pub fn into_record(self, target: Triple) -> crate::native_record::Record {
        crate::native_record::Record::new(
            target,
            crate::native_record::Kind::Copy,
            self.bytes,
            self.functions,
        )
    }
}
pub fn emit_host_object(
    verified: &m::VerifiedUnit,
    imports: &[HostFunctionId],
    limits: m::Limits,
) -> Result<ObjectArtifact, Error> {
    let budget = preflight(verified, imports, limits)?;
    let builder = cranelift_native::builder().map_err(backend)?;
    emit(
        verified,
        imports,
        budget,
        builder.finish(flags()?).map_err(backend)?,
        None,
    )
}
pub fn emit_object_for_target(
    verified: &m::VerifiedUnit,
    imports: &[HostFunctionId],
    limits: m::Limits,
    target: Triple,
) -> Result<ObjectArtifact, Error> {
    let budget = preflight(verified, imports, limits)?;
    let builder = isa::lookup(target).map_err(backend)?;
    emit(
        verified,
        imports,
        budget,
        builder.finish(flags()?).map_err(backend)?,
        None,
    )
}
/// Emit source-mapped Mach-O debug metadata for an actual whole source unit.
pub fn emit_object_for_target_with_debug(
    verified: &m::VerifiedUnit,
    imports: &[HostFunctionId],
    limits: m::Limits,
    target: Triple,
) -> Result<ObjectArtifact, Error> {
    let budget = preflight(verified, imports, limits)?;
    let builder = isa::lookup(target).map_err(backend)?;
    emit(
        verified,
        imports,
        budget,
        builder.finish(flags()?).map_err(backend)?,
        Some(&verified.unit().sources),
    )
}
fn flags() -> Result<settings::Flags, Error> {
    let mut builder = settings::builder();
    builder.set("is_pic", "true").map_err(backend)?;
    Ok(settings::Flags::new(builder))
}
fn emit(
    verified: &m::VerifiedUnit,
    imports: &[HostFunctionId],
    mut budget: m::Budget,
    isa: OwnedTargetIsa,
    sources: Option<&[m::Source]>,
) -> Result<ObjectArtifact, Error> {
    let builder =
        ObjectBuilder::new(isa, b"lang-unit".to_vec(), default_libcall_names()).map_err(backend)?;
    let mut module = ObjectModule::new(builder);
    let (_, functions) = define(&mut module, verified, imports, &mut budget)?;
    let mut product = module.finish();
    if let Some(sources) = sources {
        crate::source_debug::macho(
            &mut product.object,
            functions.iter(),
            |id| {
                verified
                    .unit()
                    .functions
                    .get(id.0 as usize)
                    .and_then(|function| function.span.map(|span| (function.name.as_str(), span)))
            },
            sources,
            &mut budget,
        )
        .map_err(Error::SourceDebug)?;
    }
    let bytes = product.emit().map_err(backend)?;
    Ok(ObjectArtifact { bytes, functions })
}

fn cells_for(v: &m::VerifiedUnit, ty: m::Type) -> usize {
    v.facts().type_cells(ty).expect("verified type")
}
fn parameter_cells(v: &m::VerifiedUnit, signature: &m::Signature) -> usize {
    signature
        .parameters
        .iter()
        .map(|ty| cells_for(v, *ty))
        .sum()
}
fn valid_cells(v: &m::VerifiedUnit, ty: m::Type, cells: &[i64]) -> bool {
    if !ty.is_copy() || v.facts().type_cells(ty) != Some(cells.len()) {
        return false;
    }
    match ty {
        m::Type::I64 => true,
        m::Type::Bool => matches!(cells[0], 0 | 1),
        m::Type::Own(_) => false,
        m::Type::Record(id) | m::Type::Enum(id) => {
            let definition = &v.unit().definitions[id.0 as usize];
            let mut offset = 1;
            let mut check_field = |field| {
                let end = offset + cells_for(v, field);
                if !valid_cells(v, field, &cells[offset..end]) {
                    return false;
                }
                offset = end;
                true
            };
            let valid = match &definition.kind {
                m::DefinitionKind::Record(fields) if cells[0] == 0 => {
                    fields.iter().all(|field| check_field(field.value_type))
                }
                m::DefinitionKind::Enum(cases) => {
                    match usize::try_from(cells[0]).ok().and_then(|i| cases.get(i)) {
                        Some(case) => case.payload.iter().copied().all(check_field),
                        None => false,
                    }
                }
                _ => false,
            };
            valid && cells[offset..].iter().all(|cell| *cell == 0)
        }
    }
}
fn preflight(
    v: &m::VerifiedUnit,
    imports: &[HostFunctionId],
    limits: m::Limits,
) -> Result<m::Budget, Error> {
    let mut budget = m::Budget::new(limits);
    v.revalidate(&mut budget)?;
    // Keep host-side canonical-value validation within a fixed recursion
    // ceiling even when callers verified MIR with unusually generous limits.
    if v.facts().layouts().iter().any(|layout| layout.depth() > 64) {
        return Err(Error::NativeCapacity);
    }
    if imports.len() != v.unit().imports.len() {
        return Err(Error::ImportArity);
    }
    for f in &v.unit().functions {
        if f.registers.iter().any(|ty| !ty.is_copy()) {
            return Err(Error::UnsupportedOwnedType);
        }
    }
    for (f, facts) in v.unit().functions.iter().zip(v.facts().functions()) {
        let outgoing = f
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter_map(|i| match &i.op {
                m::Op::Call { function, .. } => Some(parameter_cells(
                    v,
                    &v.unit().functions[function.0 as usize].signature,
                )),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            .max(1);
        let count = facts
            .cells()
            .checked_add(outgoing)
            .ok_or(Error::NativeCapacity)?;
        if count.checked_mul(8).is_none_or(|n| n > MAX_FRAME_BYTES) {
            return Err(Error::NativeCapacity);
        }
        // Every emitted cell copy is charged during lowering as well.
        budget.charge(count)?;
    }
    Ok(budget)
}

fn define<M: Module>(
    module: &mut M,
    v: &m::VerifiedUnit,
    imports: &[HostFunctionId],
    budget: &mut m::Budget,
) -> Result<(Vec<FuncId>, Vec<FunctionCode>), Error> {
    let pointer = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature
        .params
        .extend([AbiParam::new(pointer), AbiParam::new(pointer)]);
    signature.returns.push(AbiParam::new(types::I32));
    let mut ids = budget.vector(v.unit().functions.len())?;
    for index in 0..v.unit().functions.len() {
        ids.push(
            module
                .declare_function(&format!("lang_unit_f{index}"), Linkage::Export, &signature)
                .map_err(backend)?,
        );
    }
    let mut host_ids = budget.vector(imports.len())?;
    let mut host_signature = module.make_signature();
    host_signature.params.push(AbiParam::new(types::I64));
    host_signature.returns.push(AbiParam::new(types::I64));
    for host in imports {
        host_ids.push(
            module
                .declare_function(&host_symbol_name(*host), Linkage::Import, &host_signature)
                .map_err(backend)?,
        );
    }
    let mut functions = budget.vector(ids.len())?;
    for (index, id) in ids.iter().copied().enumerate() {
        let f = &v.unit().functions[index];
        let mut context = module.make_context();
        context.func.signature = signature.clone();
        let mut calls = budget.vector(ids.len())?;
        for id in &ids {
            calls.push(module.declare_func_in_func(*id, &mut context.func));
        }
        let mut hosts = budget.vector(host_ids.len())?;
        for id in &host_ids {
            hosts.push(module.declare_func_in_func(*id, &mut context.func));
        }
        let config = module.target_config();
        let mut builder_context = FunctionBuilderContext::new();
        let mut spans = Vec::new();
        {
            let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
            let mut blocks = budget.vector(f.blocks.len())?;
            for _ in &f.blocks {
                blocks.push(builder.create_block());
            }
            builder.append_block_params_for_function_params(blocks[0]);
            builder.switch_to_block(blocks[0]);
            let input = builder.block_params(blocks[0])[0];
            let output = builder.block_params(blocks[0])[1];
            let mut offsets = budget.vector(f.registers.len())?;
            let mut cells = 0;
            for ty in &f.registers {
                offsets.push(cells);
                cells += cells_for(v, *ty);
            }
            let frame = slot(&mut builder, cells.max(1));
            let max_args = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter_map(|i| match &i.op {
                    m::Op::Call { function, .. } => Some(parameter_cells(
                        v,
                        &v.unit().functions[function.0 as usize].signature,
                    )),
                    _ => None,
                })
                .max()
                .unwrap_or(0)
                .max(1);
            let outgoing = slot(&mut builder, max_args);
            let mut lower = Lower {
                builder,
                v,
                f,
                frame,
                outgoing,
                offsets,
                budget,
                spans: &mut spans,
                pointer,
            };
            let mut offset = 0;
            for (r, ty) in f.signature.parameters.iter().enumerate() {
                for cell in 0..cells_for(v, *ty) {
                    lower.budget.charge(1)?;
                    let value = lower.builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        input,
                        byte_offset(offset),
                    );
                    lower.store(r as u32, cell, value);
                    offset += 1;
                }
            }
            for (bi, block) in f.blocks.iter().enumerate() {
                if bi != 0 {
                    lower.builder.switch_to_block(blocks[bi]);
                }
                for instruction in &block.instructions {
                    lower.location(instruction.span)?;
                    lower.instruction(&instruction.op, &calls, &hosts)?;
                }
                lower.location(block.terminator.span)?;
                match &block.terminator.kind {
                    m::Term::Return(r) => {
                        for cell in 0..cells_for(v, f.signature.result) {
                            lower.budget.charge(1)?;
                            let value = lower.load(r.0, cell);
                            lower.builder.ins().store(
                                MemFlagsData::new(),
                                value,
                                output,
                                byte_offset(cell),
                            );
                        }
                        let status = lower.builder.ins().iconst(types::I32, 0);
                        lower.builder.ins().return_(&[status]);
                    }
                    m::Term::Jump(t) => lower.edge(t, &blocks)?,
                    m::Term::Branch {
                        condition,
                        then_target,
                        else_target,
                    } => {
                        let condition = lower.load(condition.0, 0);
                        let yes = lower.builder.create_block();
                        let no = lower.builder.create_block();
                        lower.builder.ins().brif(condition, yes, &[], no, &[]);
                        lower.builder.switch_to_block(yes);
                        lower.edge(then_target, &blocks)?;
                        lower.builder.switch_to_block(no);
                        lower.edge(else_target, &blocks)?;
                    }
                }
            }
            lower.builder.seal_all_blocks();
            lower.builder.finalize(config);
        }
        module.define_function(id, &mut context).map_err(backend)?;
        let code = context.compiled_code().expect("defined code");
        let mut ranges = budget.vector(code.buffer.get_srclocs_sorted().len())?;
        for range in code.buffer.get_srclocs_sorted() {
            if !range.loc.is_default() {
                ranges.push(SourceRange {
                    start: range.start,
                    end: range.end,
                    span: spans[range.loc.bits() as usize],
                });
            }
        }
        functions.push(FunctionCode {
            function: m::FunctionId(index as u32),
            symbol: format!("lang_unit_f{index}"),
            code_size: code.code_buffer().len(),
            source_ranges: ranges,
        });
        module.clear_context(&mut context);
    }
    Ok((ids, functions))
}
fn byte_offset(cells: usize) -> i32 {
    i32::try_from(cells * 8).expect("preflight bounded frame/signature")
}
fn slot(builder: &mut FunctionBuilder<'_>, cells: usize) -> StackSlot {
    builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        (cells * 8) as u32,
        3,
    ))
}
struct Lower<'a, 'b> {
    builder: FunctionBuilder<'a>,
    v: &'b m::VerifiedUnit,
    f: &'b m::Function,
    frame: StackSlot,
    outgoing: StackSlot,
    offsets: Vec<usize>,
    budget: &'b mut m::Budget,
    spans: &'b mut Vec<m::Span>,
    pointer: cranelift_codegen::ir::Type,
}
impl Lower<'_, '_> {
    fn location(&mut self, span: Option<m::Span>) -> Result<(), Error> {
        let loc = if let Some(span) = span {
            self.budget.allocate::<m::Span>(1)?;
            let loc =
                SourceLoc::new(u32::try_from(self.spans.len()).map_err(|_| Error::NativeCapacity)?);
            self.spans.push(span);
            loc
        } else {
            SourceLoc::default()
        };
        self.builder.set_srcloc(loc);
        self.budget.charge(1)?;
        Ok(())
    }
    fn load(&mut self, register: u32, cell: usize) -> IrValue {
        self.builder.ins().stack_load(
            self.pointer,
            types::I64,
            self.frame,
            byte_offset(self.offsets[register as usize] + cell),
        )
    }
    fn store(&mut self, register: u32, cell: usize, value: IrValue) {
        self.builder.ins().stack_store(
            self.pointer,
            value,
            self.frame,
            byte_offset(self.offsets[register as usize] + cell),
        );
    }
    fn count(&self, r: m::Register) -> usize {
        cells_for(self.v, self.f.registers[r.0 as usize])
    }
    fn copy(
        &mut self,
        destination: m::Register,
        start: usize,
        source: m::Register,
        source_start: usize,
        count: usize,
    ) -> Result<(), Error> {
        self.budget.charge(count)?;
        for cell in 0..count {
            let value = self.load(source.0, source_start + cell);
            self.store(destination.0, start + cell, value);
        }
        Ok(())
    }
    fn check_status(&mut self, status: IrValue) {
        let fail = self.builder.create_block();
        let next = self.builder.create_block();
        self.builder.ins().brif(status, fail, &[], next, &[]);
        self.builder.switch_to_block(fail);
        self.builder.ins().return_(&[status]);
        self.builder.switch_to_block(next);
    }
    fn field_offset(&self, ty: m::Type, case: Option<u32>, field: u32) -> usize {
        let id = match ty {
            m::Type::Record(id) | m::Type::Enum(id) => id,
            _ => unreachable!("verified aggregate"),
        };
        let fields = if let Some(case) = case {
            &self.v.facts().layouts()[id.0 as usize].cases()[case as usize]
        } else {
            self.v.facts().layouts()[id.0 as usize].fields()
        };
        fields[field as usize]
    }
    fn instruction(
        &mut self,
        op: &m::Op,
        calls: &[cranelift_codegen::ir::FuncRef],
        hosts: &[cranelift_codegen::ir::FuncRef],
    ) -> Result<(), Error> {
        let destination = op.destination();
        let scalar = match op {
            m::Op::I64Constant { value, .. } => Some(self.builder.ins().iconst(types::I64, *value)),
            m::Op::BoolConstant { value, .. } => {
                Some(self.builder.ins().iconst(types::I64, i64::from(*value)))
            }
            m::Op::Add { left, right, .. }
            | m::Op::LessThan { left, right, .. }
            | m::Op::EqualI64 { left, right, .. } => {
                let left = self.load(left.0, 0);
                let right = self.load(right.0, 0);
                Some(match op {
                    m::Op::Add { .. } => self.builder.ins().iadd(left, right),
                    _ => {
                        let cc = if matches!(op, m::Op::LessThan { .. }) {
                            IntCC::SignedLessThan
                        } else {
                            IntCC::Equal
                        };
                        let comparison = self.builder.ins().icmp(cc, left, right);
                        self.builder.ins().uextend(types::I64, comparison)
                    }
                })
            }
            m::Op::HostCall {
                import, argument, ..
            } => {
                let value = self.load(argument.0, 0);
                let call = self.builder.ins().call(hosts[import.0 as usize], &[value]);
                Some(self.builder.inst_results(call)[0])
            }
            m::Op::Call {
                function,
                arguments,
                ..
            } => {
                let mut offset = 0;
                for argument in arguments {
                    let count = self.count(*argument);
                    self.budget.charge(count)?;
                    for cell in 0..count {
                        let value = self.load(argument.0, cell);
                        self.builder.ins().stack_store(
                            self.pointer,
                            value,
                            self.outgoing,
                            byte_offset(offset),
                        );
                        offset += 1;
                    }
                }
                let input = self
                    .builder
                    .ins()
                    .stack_addr(self.pointer, self.outgoing, 0);
                let output = self.builder.ins().stack_addr(
                    self.pointer,
                    self.frame,
                    byte_offset(self.offsets[destination.0 as usize]),
                );
                let call = self
                    .builder
                    .ins()
                    .call(calls[function.0 as usize], &[input, output]);
                let status = self.builder.inst_results(call)[0];
                self.check_status(status);
                None
            }
            m::Op::Record { fields, .. }
            | m::Op::Enum {
                payload: fields, ..
            } => {
                self.budget.charge(self.count(destination))?;
                let tag = if let m::Op::Enum { case, .. } = op {
                    i64::from(*case)
                } else {
                    0
                };
                let zero = self.builder.ins().iconst(types::I64, 0);
                for cell in 1..self.count(destination) {
                    self.store(destination.0, cell, zero);
                }
                let tag = self.builder.ins().iconst(types::I64, tag);
                self.store(destination.0, 0, tag);
                let mut offset = 1;
                for field in fields {
                    let count = self.count(*field);
                    self.copy(destination, offset, *field, 0, count)?;
                    offset += count;
                }
                None
            }
            m::Op::Field {
                receiver, field, ..
            }
            | m::Op::EnumField {
                receiver, field, ..
            } => {
                let case = if let m::Op::EnumField { case, .. } = op {
                    Some(*case)
                } else {
                    None
                };
                if let Some(case) = case {
                    let tag = self.load(receiver.0, 0);
                    let wrong =
                        self.builder
                            .ins()
                            .icmp_imm_s(IntCC::NotEqual, tag, i64::from(case));
                    let status = self.builder.ins().uextend(types::I32, wrong);
                    self.check_status(status);
                }
                let offset = self.field_offset(self.f.registers[receiver.0 as usize], case, *field);
                self.copy(destination, 0, *receiver, offset, self.count(destination))?;
                None
            }
            m::Op::IsCase { receiver, case, .. } => {
                let tag = self.load(receiver.0, 0);
                let comparison = self
                    .builder
                    .ins()
                    .icmp_imm_s(IntCC::Equal, tag, i64::from(*case));
                Some(self.builder.ins().uextend(types::I64, comparison))
            }
        };
        if let Some(value) = scalar {
            self.store(destination.0, 0, value);
        }
        Ok(())
    }
    fn edge(
        &mut self,
        target: &m::Target,
        blocks: &[cranelift_codegen::ir::Block],
    ) -> Result<(), Error> {
        let parameters = &self.f.blocks[target.block.0 as usize].parameters;
        let count: usize = target.arguments.iter().map(|r| self.count(*r)).sum();
        let mut copies = self.budget.vector(count)?;
        self.budget.charge(count)?;
        // Read every edge operand before writing any destination: loop edges
        // may permute aggregate parameters, including self-edge swaps.
        for (argument, parameter) in target.arguments.iter().zip(parameters) {
            for cell in 0..self.count(*argument) {
                let value = self.load(argument.0, cell);
                copies.push((parameter.0, cell, value));
            }
        }
        for (parameter, cell, value) in copies {
            self.store(parameter, cell, value);
        }
        self.builder
            .ins()
            .jump(blocks[target.block.0 as usize], &[]);
        Ok(())
    }
}
