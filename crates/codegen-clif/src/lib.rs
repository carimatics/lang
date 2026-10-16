use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cranelift_codegen::ir::{AbiParam, BlockArg, InstBuilder, SourceLoc, condcodes::IntCC, types};
use cranelift_codegen::isa::{self, OwnedTargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use lang_mir::{
    Function, HostFunctionId, Instruction, SourceSpan, Terminator, Type, ValidationError,
};
use lang_runtime::HostFunctionRegistry;
pub use target_lexicon::Triple;

mod debug;

pub struct CompiledFunction {
    _module: JITModule,
    code: *const u8,
    code_size: usize,
    parameter_count: usize,
    source_map: Option<CompiledSourceMap>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectArtifact {
    bytes: Vec<u8>,
    function_symbol: String,
    code_size: usize,
    source_map: Option<CompiledSourceMap>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledSourceMap {
    source_path: String,
    function_span: SourceSpan,
    ranges: Vec<CompiledSourceRange>,
}

impl CompiledSourceMap {
    pub fn source_path(&self) -> &str {
        &self.source_path
    }

    pub const fn function_span(&self) -> SourceSpan {
        self.function_span
    }

    pub fn ranges(&self) -> &[CompiledSourceRange] {
        &self.ranges
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompiledSourceRange {
    start: u32,
    end: u32,
    span: SourceSpan,
}

impl CompiledSourceRange {
    pub const fn start(&self) -> u32 {
        self.start
    }

    pub const fn end(&self) -> u32 {
        self.end
    }

    pub const fn span(&self) -> SourceSpan {
        self.span
    }
}

impl ObjectArtifact {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn function_symbol(&self) -> &str {
        &self.function_symbol
    }

    pub const fn code_size(&self) -> usize {
        self.code_size
    }

    pub const fn source_map(&self) -> Option<&CompiledSourceMap> {
        self.source_map.as_ref()
    }
}

impl CompiledFunction {
    pub fn compile(function: &Function) -> Result<Self, CompileError> {
        Self::compile_with_hosts(function, &HostFunctionRegistry::new())
    }

    pub fn compile_with_hosts(
        function: &Function,
        host_functions: &HostFunctionRegistry,
    ) -> Result<Self, CompileError> {
        validate_function(function)?;

        let mut jit_builder =
            JITBuilder::new(default_libcall_names()).map_err(CompileError::backend)?;
        let host_symbols = collect_host_symbols(function);
        for (host_id, symbol) in &host_symbols {
            let callback = host_functions
                .resolve_i64(*host_id)
                .ok_or(CompileError::UnknownHostFunction(*host_id))?;
            jit_builder.symbol(symbol.clone(), callback as *const u8);
        }

        let mut module = JITModule::new(jit_builder);
        let defined = define_function(&mut module, function, Linkage::Local, &host_symbols)?;
        module
            .finalize_definitions()
            .map_err(CompileError::backend)?;
        let code = module.get_finalized_function(defined.function_id);

        Ok(Self {
            _module: module,
            code,
            code_size: defined.code_size,
            parameter_count: function.signature().parameters().len(),
            source_map: defined.source_map,
        })
    }

    pub const fn code_size(&self) -> usize {
        self.code_size
    }

    pub const fn source_map(&self) -> Option<&CompiledSourceMap> {
        self.source_map.as_ref()
    }

    pub fn invoke_i64_2(&self, left: i64, right: i64) -> Result<i64, InvokeError> {
        if self.parameter_count != 2 {
            return Err(InvokeError::Arity {
                expected: self.parameter_count,
                actual: 2,
            });
        }

        // SAFETY: compilation records the validated MIR arity, and this branch
        // accepts only the exact two-i64 signature used for this function.
        let function: unsafe extern "C" fn(i64, i64) -> i64 =
            unsafe { std::mem::transmute(self.code) };
        // SAFETY: JIT code remains alive with `self._module`, and the signature
        // checked above matches the generated function's native signature.
        Ok(unsafe { function(left, right) })
    }

    pub fn invoke_i64_1(&self, argument: i64) -> Result<i64, InvokeError> {
        if self.parameter_count != 1 {
            return Err(InvokeError::Arity {
                expected: self.parameter_count,
                actual: 1,
            });
        }

        // SAFETY: compilation records the validated MIR arity, and this branch
        // accepts only the exact one-i64 signature used for this function.
        let function: unsafe extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(self.code) };
        // SAFETY: JIT code remains alive with `self._module`, and the signature
        // checked above matches the generated function's native signature.
        Ok(unsafe { function(argument) })
    }
}

pub fn emit_host_object(function: &Function) -> Result<ObjectArtifact, CompileError> {
    validate_function(function)?;
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "true")
        .map_err(CompileError::backend)?;
    let isa_builder = cranelift_native::builder().map_err(CompileError::backend)?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(CompileError::backend)?;

    emit_object_with_isa(function, isa)
}

pub fn emit_object_for_target(
    function: &Function,
    target: Triple,
) -> Result<ObjectArtifact, CompileError> {
    validate_function(function)?;
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "true")
        .map_err(CompileError::backend)?;
    let isa_builder = isa::lookup(target).map_err(CompileError::backend)?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(CompileError::backend)?;

    emit_object_with_isa(function, isa)
}

fn emit_object_with_isa(
    function: &Function,
    isa: OwnedTargetIsa,
) -> Result<ObjectArtifact, CompileError> {
    let object_builder = ObjectBuilder::new(isa, b"lang".to_vec(), default_libcall_names())
        .map_err(CompileError::backend)?;
    let mut module = ObjectModule::new(object_builder);
    let host_symbols = collect_host_symbols(function);
    let defined = define_function(&mut module, function, Linkage::Export, &host_symbols)?;
    let mut product = module.finish();
    if let Some(source_map) = &defined.source_map {
        let function_symbol = product.function_symbol(defined.function_id);
        debug::add_native_debug(
            &mut product.object,
            function_symbol,
            function.name(),
            defined.code_size,
            source_map,
        )
        .map_err(CompileError::backend)?;
    }
    let bytes = product.emit().map_err(CompileError::backend)?;

    Ok(ObjectArtifact {
        bytes,
        function_symbol: function.name().to_owned(),
        code_size: defined.code_size,
        source_map: defined.source_map,
    })
}

struct DefinedFunction {
    function_id: cranelift_module::FuncId,
    code_size: usize,
    source_map: Option<CompiledSourceMap>,
}

fn define_function<M: Module>(
    module: &mut M,
    function: &Function,
    linkage: Linkage,
    host_symbols: &[(HostFunctionId, String)],
) -> Result<DefinedFunction, CompileError> {
    let mut context = module.make_context();
    context.func.signature.params.extend(
        function
            .signature()
            .parameters()
            .iter()
            .map(|_| AbiParam::new(types::I64)),
    );
    context
        .func
        .signature
        .returns
        .push(AbiParam::new(types::I64));

    let function_id = module
        .declare_function(function.name(), linkage, &context.func.signature)
        .map_err(CompileError::backend)?;
    let mut host_references = HashMap::new();
    for (host_id, symbol) in host_symbols {
        let mut signature = module.make_signature();
        signature.params.push(AbiParam::new(types::I64));
        signature.returns.push(AbiParam::new(types::I64));
        let host_function = module
            .declare_function(symbol, Linkage::Import, &signature)
            .map_err(CompileError::backend)?;
        let function_reference = module.declare_func_in_func(host_function, &mut context.func);
        host_references.insert(*host_id, function_reference);
    }

    let mut builder_context = FunctionBuilderContext::new();
    let frontend_config = module.target_config();
    let mut source_spans = Vec::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let blocks: Vec<_> = function
            .blocks()
            .iter()
            .map(|_| builder.create_block())
            .collect();
        let entry = blocks[0];
        builder.append_block_params_for_function_params(entry);
        for (block_index, block) in function.blocks().iter().enumerate().skip(1) {
            for parameter in block.parameters() {
                let parameter_type = match parameter.value_type() {
                    Type::I64 => types::I64,
                    Type::Bool => types::I8,
                };
                builder.append_block_param(blocks[block_index], parameter_type);
            }
        }
        let mut values = vec![None; function.value_count()];
        for (index, &value) in builder.block_params(entry).iter().enumerate() {
            values[index] = Some(value);
        }
        for (block_index, block) in function.blocks().iter().enumerate().skip(1) {
            for (parameter, &value) in block
                .parameters()
                .iter()
                .zip(builder.block_params(blocks[block_index]))
            {
                values[parameter.value().index()] = Some(value);
            }
        }

        for (block_index, block) in function.blocks().iter().enumerate() {
            builder.switch_to_block(blocks[block_index]);
            for (instruction_index, instruction) in block.instructions().iter().enumerate() {
                if let Some(source_map) = function.source_map() {
                    builder.set_srcloc(record_source_span(
                        &mut source_spans,
                        source_map.blocks()[block_index].instruction_spans()[instruction_index],
                    ));
                }
                let value = match *instruction {
                    Instruction::I64Constant { value, .. } => {
                        builder.ins().iconst(types::I64, value)
                    }
                    Instruction::I64AddWrapping { left, right, .. } => builder.ins().iadd(
                        values[left.index()].expect("validated operands have translated values"),
                        values[right.index()].expect("validated operands have translated values"),
                    ),
                    Instruction::I64LessThanSigned { left, right, .. } => builder.ins().icmp(
                        IntCC::SignedLessThan,
                        values[left.index()].expect("validated operands have translated values"),
                        values[right.index()].expect("validated operands have translated values"),
                    ),
                    Instruction::CallHostI64 {
                        function, argument, ..
                    } => {
                        let function_reference = host_references
                            .get(&function)
                            .expect("host reference was declared before lowering");
                        let call = builder.ins().call(
                            *function_reference,
                            &[values[argument.index()]
                                .expect("validated operands have translated values")],
                        );
                        builder.inst_results(call)[0]
                    }
                };
                values[instruction.result().index()] = Some(value);
            }

            if let Some(source_map) = function.source_map() {
                builder.set_srcloc(record_source_span(
                    &mut source_spans,
                    source_map.blocks()[block_index].terminator_span(),
                ));
            }
            match block.terminator() {
                Terminator::Return(return_value) => {
                    builder.ins().return_(&[values[return_value.index()]
                        .expect("validated return values have translated values")]);
                }
                Terminator::Jump(target) => {
                    let arguments = target
                        .arguments()
                        .iter()
                        .map(|argument| {
                            BlockArg::Value(
                                values[argument.index()]
                                    .expect("validated edge arguments have translated values"),
                            )
                        })
                        .collect::<Vec<_>>();
                    builder
                        .ins()
                        .jump(blocks[target.block().index()], &arguments);
                }
                Terminator::Branch {
                    condition,
                    then_target,
                    else_target,
                } => {
                    let then_arguments = then_target
                        .arguments()
                        .iter()
                        .map(|argument| {
                            BlockArg::Value(
                                values[argument.index()]
                                    .expect("validated edge arguments have translated values"),
                            )
                        })
                        .collect::<Vec<_>>();
                    let else_arguments = else_target
                        .arguments()
                        .iter()
                        .map(|argument| {
                            BlockArg::Value(
                                values[argument.index()]
                                    .expect("validated edge arguments have translated values"),
                            )
                        })
                        .collect::<Vec<_>>();
                    builder.ins().brif(
                        values[condition.index()]
                            .expect("validated branch conditions have translated values"),
                        blocks[then_target.block().index()],
                        &then_arguments,
                        blocks[else_target.block().index()],
                        &else_arguments,
                    );
                }
            }
        }
        builder.seal_all_blocks();
        builder.finalize(frontend_config);
    }

    module
        .define_function(function_id, &mut context)
        .map_err(CompileError::backend)?;
    let compiled_code = context
        .compiled_code()
        .expect("a successfully defined function has compiled code");
    let code_size = compiled_code.code_buffer().len();
    let source_map = function.source_map().map(|source_map| CompiledSourceMap {
        source_path: source_map.source_path().to_owned(),
        function_span: source_map.function_span(),
        ranges: compiled_code
            .buffer
            .get_srclocs_sorted()
            .iter()
            .filter_map(|range| {
                if range.loc.is_default() {
                    return None;
                }
                Some(CompiledSourceRange {
                    start: range.start,
                    end: range.end,
                    span: source_spans[range.loc.bits() as usize],
                })
            })
            .collect(),
    });
    module.clear_context(&mut context);

    Ok(DefinedFunction {
        function_id,
        code_size,
        source_map,
    })
}

fn record_source_span(source_spans: &mut Vec<SourceSpan>, span: SourceSpan) -> SourceLoc {
    let location = u32::try_from(source_spans.len())
        .expect("validated MIR value and block identifiers fit in u32");
    source_spans.push(span);
    SourceLoc::new(location)
}

fn validate_function(function: &Function) -> Result<(), CompileError> {
    function.validate().map_err(CompileError::InvalidMir)?;
    if function.signature().parameters().contains(&Type::Bool)
        || function.signature().result() != Type::I64
    {
        return Err(CompileError::UnsupportedSignature);
    }
    Ok(())
}

fn collect_host_symbols(function: &Function) -> Vec<(HostFunctionId, String)> {
    let mut host_symbols = Vec::new();
    for block in function.blocks() {
        for instruction in block.instructions() {
            let Instruction::CallHostI64 {
                function: host_id, ..
            } = instruction
            else {
                continue;
            };
            if host_symbols.iter().all(|(id, _)| id != host_id) {
                host_symbols.push((*host_id, host_symbol_name(*host_id)));
            }
        }
    }
    host_symbols
}

fn host_symbol_name(id: HostFunctionId) -> String {
    format!("lang_host_i64_{}", id.as_u32())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    InvalidMir(ValidationError),
    UnsupportedSignature,
    UnknownHostFunction(HostFunctionId),
    Backend(String),
}

impl CompileError {
    fn backend(error: impl Display) -> Self {
        Self::Backend(error.to_string())
    }
}

impl Display for CompileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMir(error) => write!(formatter, "invalid MIR: {error}"),
            Self::UnsupportedSignature => formatter.write_str("unsupported function signature"),
            Self::UnknownHostFunction(id) => {
                write!(formatter, "host function {id} is not registered")
            }
            Self::Backend(error) => write!(formatter, "Cranelift backend error: {error}"),
        }
    }
}

impl Error for CompileError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvokeError {
    Arity { expected: usize, actual: usize },
}

impl Display for InvokeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arity { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} arguments, received {actual}"
                )
            }
        }
    }
}

impl Error for InvokeError {}

#[cfg(test)]
mod tests {
    use lang_mir::{
        BasicBlock, BlockId, BlockParameter, BlockSourceMap, BlockTarget, Function,
        FunctionSourceMap, HostFunctionId, Instruction, Signature, SourcePosition, SourceSpan,
        Terminator, Type, ValueId,
    };

    use super::{CompileError, CompiledFunction, CompiledSourceMap, emit_host_object};

    fn target(block: BlockId) -> BlockTarget {
        BlockTarget::new(block, vec![])
    }

    #[test]
    fn preserves_mir_spans_as_native_code_ranges() {
        let function_span = span(1, 1, 3, 2);
        let return_span = span(2, 5, 2, 14);
        let source_map = FunctionSourceMap::new(
            "tests/identity.lang",
            function_span,
            vec![BlockSourceMap::new(vec![], return_span)],
        )
        .unwrap();
        let function = Function::new(
            "located_identity",
            Signature::new(vec![Type::I64], Type::I64),
            vec![],
            Terminator::Return(ValueId::new(0)),
        )
        .with_source_map(source_map)
        .unwrap();

        let compiled = CompiledFunction::compile(&function).unwrap();
        assert_eq!(compiled.invoke_i64_1(42), Ok(42));
        assert_source_map(compiled.source_map().unwrap(), function_span, return_span);

        let artifact = emit_host_object(&function).unwrap();
        assert_source_map(artifact.source_map().unwrap(), function_span, return_span);
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert_macho_dwarf_sections(&artifact);
    }

    fn assert_source_map(
        source_map: &CompiledSourceMap,
        function_span: SourceSpan,
        operation_span: SourceSpan,
    ) {
        assert_eq!(source_map.source_path(), "tests/identity.lang");
        assert_eq!(source_map.function_span(), function_span);
        assert!(!source_map.ranges().is_empty());
        assert!(
            source_map
                .ranges()
                .iter()
                .all(|range| range.start() < range.end())
        );
        assert!(
            source_map
                .ranges()
                .iter()
                .any(|range| range.span() == operation_span)
        );
    }

    fn position(line: u32, column: u32) -> SourcePosition {
        SourcePosition::new(line, column).unwrap()
    }

    fn span(start_line: u32, start_column: u32, end_line: u32, end_column: u32) -> SourceSpan {
        SourceSpan::new(
            position(start_line, start_column),
            position(end_line, end_column),
        )
        .unwrap()
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn assert_macho_dwarf_sections(artifact: &super::ObjectArtifact) {
        use object::{Object as _, ObjectSection as _};

        let object = object::File::parse(artifact.bytes()).unwrap();
        for name in ["__debug_abbrev", "__debug_info", "__debug_line"] {
            let section = object
                .section_by_name(name)
                .unwrap_or_else(|| panic!("missing {name}"));
            assert!(!section.data().unwrap().is_empty(), "empty {name}");
        }
    }

    #[test]
    fn reports_generated_code_size() {
        let function = Function::new(
            "identity",
            Signature::new(vec![Type::I64], Type::I64),
            vec![],
            Terminator::Return(ValueId::new(0)),
        );

        let compiled = CompiledFunction::compile(&function).unwrap();

        assert!(compiled.code_size() > 0);
        assert_eq!(compiled.invoke_i64_1(42), Ok(42));
    }

    #[test]
    fn compiles_typed_control_flow() {
        let function = Function::with_blocks(
            "minimum",
            Signature::new(vec![Type::I64, Type::I64], Type::I64),
            vec![
                BasicBlock::new(
                    vec![Instruction::I64LessThanSigned {
                        result: ValueId::new(2),
                        left: ValueId::new(0),
                        right: ValueId::new(1),
                    }],
                    Terminator::Branch {
                        condition: ValueId::new(2),
                        then_target: target(BlockId::new(1)),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(0))),
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(1))),
            ],
        );

        let compiled = CompiledFunction::compile(&function).unwrap();

        assert_eq!(compiled.invoke_i64_2(-7, 4), Ok(-7));
        assert_eq!(compiled.invoke_i64_2(9, 3), Ok(3));
        assert_eq!(compiled.invoke_i64_2(5, 5), Ok(5));

        let artifact = emit_host_object(&function).unwrap();
        assert!(artifact.code_size() > 0);
        assert!(!artifact.into_bytes().is_empty());
    }

    #[test]
    fn compiles_an_explicit_jump() {
        let function = Function::with_blocks(
            "jump_identity",
            Signature::new(vec![Type::I64], Type::I64),
            vec![
                BasicBlock::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(0))),
            ],
        );

        let compiled = CompiledFunction::compile(&function).unwrap();

        assert_eq!(compiled.invoke_i64_1(42), Ok(42));
    }

    #[test]
    fn compiles_loop_carried_block_parameters() {
        let function = countdown_function();
        let compiled = CompiledFunction::compile(&function).unwrap();

        for value in [i64::MIN, -7, -1, 0, 1, 2, 17, 100] {
            assert_eq!(compiled.invoke_i64_1(value), Ok(value.min(0)));
        }

        let artifact = emit_host_object(&function).unwrap();
        assert!(artifact.code_size() > 0);
        assert!(!artifact.into_bytes().is_empty());
    }

    fn countdown_function() -> Function {
        Function::with_blocks(
            "countdown",
            Signature::new(vec![Type::I64], Type::I64),
            vec![
                BasicBlock::new(
                    vec![
                        Instruction::I64Constant {
                            result: ValueId::new(1),
                            value: -1,
                        },
                        Instruction::I64Constant {
                            result: ValueId::new(2),
                            value: 1,
                        },
                    ],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![ValueId::new(0)])),
                ),
                BasicBlock::with_parameters(
                    vec![BlockParameter::new(ValueId::new(3), Type::I64)],
                    vec![Instruction::I64LessThanSigned {
                        result: ValueId::new(4),
                        left: ValueId::new(3),
                        right: ValueId::new(2),
                    }],
                    Terminator::Branch {
                        condition: ValueId::new(4),
                        then_target: BlockTarget::new(BlockId::new(3), vec![ValueId::new(3)]),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                BasicBlock::new(
                    vec![Instruction::I64AddWrapping {
                        result: ValueId::new(5),
                        left: ValueId::new(3),
                        right: ValueId::new(1),
                    }],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![ValueId::new(5)])),
                ),
                BasicBlock::with_parameters(
                    vec![BlockParameter::new(ValueId::new(6), Type::I64)],
                    vec![],
                    Terminator::Return(ValueId::new(6)),
                ),
            ],
        )
    }

    #[test]
    fn emits_a_host_object_with_an_exported_function() {
        let function = Function::new(
            "identity",
            Signature::new(vec![Type::I64], Type::I64),
            vec![],
            Terminator::Return(ValueId::new(0)),
        );

        let artifact = emit_host_object(&function).unwrap();

        assert_eq!(artifact.function_symbol(), "identity");
        assert!(artifact.code_size() > 0);
        assert!(artifact.bytes().len() > artifact.code_size());
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert_eq!(&artifact.bytes()[..4], &[0xcf, 0xfa, 0xed, 0xfe]);
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        assert_eq!(&artifact.bytes()[..2], &[0x64, 0x86]);
    }

    #[test]
    fn emits_host_calls_as_object_imports() {
        let host_id = HostFunctionId::new(9);
        let function = Function::new(
            "object_host_call",
            Signature::new(vec![Type::I64], Type::I64),
            vec![Instruction::CallHostI64 {
                result: ValueId::new(1),
                function: host_id,
                argument: ValueId::new(0),
            }],
            Terminator::Return(ValueId::new(1)),
        );

        let artifact = emit_host_object(&function).unwrap();

        assert!(artifact.code_size() > 0);
        assert!(!artifact.into_bytes().is_empty());
    }

    #[test]
    fn rejects_unregistered_host_functions() {
        let host_id = HostFunctionId::new(9);
        let function = Function::new(
            "missing_host",
            Signature::new(vec![Type::I64], Type::I64),
            vec![Instruction::CallHostI64 {
                result: ValueId::new(1),
                function: host_id,
                argument: ValueId::new(0),
            }],
            Terminator::Return(ValueId::new(1)),
        );

        assert_eq!(
            CompiledFunction::compile(&function).map(|_| ()),
            Err(CompileError::UnknownHostFunction(host_id))
        );
    }
}
