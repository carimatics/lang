#![forbid(unsafe_code)]

pub mod unit;

use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResourceTypeId(u32);

impl ResourceTypeId {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Type {
    I64,
    Bool,
    Own(ResourceTypeId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourcePosition {
    line: u32,
    column: u32,
}

impl SourcePosition {
    pub fn new(line: u32, column: u32) -> Result<Self, SourceMapError> {
        if line == 0 || column == 0 {
            return Err(SourceMapError::ZeroPosition);
        }
        Ok(Self { line, column })
    }

    pub const fn line(self) -> u32 {
        self.line
    }

    pub const fn column(self) -> u32 {
        self.column
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    start: SourcePosition,
    end: SourcePosition,
}

impl SourceSpan {
    pub fn new(start: SourcePosition, end: SourcePosition) -> Result<Self, SourceMapError> {
        if (end.line, end.column) < (start.line, start.column) {
            return Err(SourceMapError::ReversedSpan);
        }
        Ok(Self { start, end })
    }

    pub const fn start(self) -> SourcePosition {
        self.start
    }

    pub const fn end(self) -> SourcePosition {
        self.end
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockSourceMap {
    instruction_spans: Vec<SourceSpan>,
    terminator_span: SourceSpan,
}

impl BlockSourceMap {
    pub fn new(instruction_spans: Vec<SourceSpan>, terminator_span: SourceSpan) -> Self {
        Self {
            instruction_spans,
            terminator_span,
        }
    }

    pub fn instruction_spans(&self) -> &[SourceSpan] {
        &self.instruction_spans
    }

    pub const fn terminator_span(&self) -> SourceSpan {
        self.terminator_span
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionSourceMap {
    source_path: String,
    function_span: SourceSpan,
    blocks: Vec<BlockSourceMap>,
}

impl FunctionSourceMap {
    pub fn new(
        source_path: impl Into<String>,
        function_span: SourceSpan,
        blocks: Vec<BlockSourceMap>,
    ) -> Result<Self, SourceMapError> {
        let source_path = source_path.into();
        if source_path.is_empty() {
            return Err(SourceMapError::EmptySourcePath);
        }
        if source_path.as_bytes().contains(&0) {
            return Err(SourceMapError::NulSourcePath);
        }
        Ok(Self {
            source_path,
            function_span,
            blocks,
        })
    }

    pub fn source_path(&self) -> &str {
        &self.source_path
    }

    pub const fn function_span(&self) -> SourceSpan {
        self.function_span
    }

    pub fn blocks(&self) -> &[BlockSourceMap] {
        &self.blocks
    }
}

impl Display for Type {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I64 => formatter.write_str("i64"),
            Self::Bool => formatter.write_str("bool"),
            Self::Own(resource) => write!(formatter, "own<resource#{}>", resource.as_u32()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValueId(u32);

impl ValueId {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockId(u32);

impl BlockId {
    pub const ENTRY: Self = Self(0);

    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HostFunctionId(u32);

impl HostFunctionId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl Display for HostFunctionId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    parameters: Vec<Type>,
    result: Type,
}

impl Signature {
    pub fn new(parameters: Vec<Type>, result: Type) -> Self {
        Self { parameters, result }
    }

    pub fn parameters(&self) -> &[Type] {
        &self.parameters
    }

    pub const fn result(&self) -> Type {
        self.result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Instruction {
    I64Constant {
        result: ValueId,
        value: i64,
    },
    I64AddWrapping {
        result: ValueId,
        left: ValueId,
        right: ValueId,
    },
    I64LessThanSigned {
        result: ValueId,
        left: ValueId,
        right: ValueId,
    },
    CallHostI64 {
        result: ValueId,
        function: HostFunctionId,
        argument: ValueId,
    },
}

impl Instruction {
    pub const fn result(&self) -> ValueId {
        match *self {
            Self::I64Constant { result, .. }
            | Self::I64AddWrapping { result, .. }
            | Self::I64LessThanSigned { result, .. }
            | Self::CallHostI64 { result, .. } => result,
        }
    }

    pub const fn result_type(&self) -> Type {
        match self {
            Self::I64LessThanSigned { .. } => Type::Bool,
            Self::I64Constant { .. } | Self::I64AddWrapping { .. } | Self::CallHostI64 { .. } => {
                Type::I64
            }
        }
    }

    fn operands(&self) -> Vec<(ValueId, Type)> {
        match *self {
            Self::I64Constant { .. } => Vec::new(),
            Self::I64AddWrapping { left, right, .. }
            | Self::I64LessThanSigned { left, right, .. } => {
                vec![(left, Type::I64), (right, Type::I64)]
            }
            Self::CallHostI64 { argument, .. } => vec![(argument, Type::I64)],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockTarget {
    block: BlockId,
    arguments: Vec<ValueId>,
}

impl BlockTarget {
    pub fn new(block: BlockId, arguments: Vec<ValueId>) -> Self {
        Self { block, arguments }
    }

    pub const fn block(&self) -> BlockId {
        self.block
    }

    pub fn arguments(&self) -> &[ValueId] {
        &self.arguments
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Terminator {
    Return(ValueId),
    Jump(BlockTarget),
    Branch {
        condition: ValueId,
        then_target: BlockTarget,
        else_target: BlockTarget,
    },
}

impl Terminator {
    fn targets(&self) -> Vec<&BlockTarget> {
        match self {
            Self::Return(_) => Vec::new(),
            Self::Jump(target) => vec![target],
            Self::Branch {
                then_target,
                else_target,
                ..
            } => vec![then_target, else_target],
        }
    }

    fn successors(&self) -> Vec<BlockId> {
        self.targets().into_iter().map(BlockTarget::block).collect()
    }

    fn operands(&self, result_type: Type) -> Vec<(ValueId, Type)> {
        match self {
            Self::Return(value) => vec![(*value, result_type)],
            Self::Jump(_) => Vec::new(),
            Self::Branch { condition, .. } => vec![(*condition, Type::Bool)],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockParameter {
    value: ValueId,
    value_type: Type,
}

impl BlockParameter {
    pub const fn new(value: ValueId, value_type: Type) -> Self {
        Self { value, value_type }
    }

    pub const fn value(self) -> ValueId {
        self.value
    }

    pub const fn value_type(self) -> Type {
        self.value_type
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasicBlock {
    parameters: Vec<BlockParameter>,
    instructions: Vec<Instruction>,
    terminator: Terminator,
}

impl BasicBlock {
    pub fn new(instructions: Vec<Instruction>, terminator: Terminator) -> Self {
        Self::with_parameters(Vec::new(), instructions, terminator)
    }

    pub fn with_parameters(
        parameters: Vec<BlockParameter>,
        instructions: Vec<Instruction>,
        terminator: Terminator,
    ) -> Self {
        Self {
            parameters,
            instructions,
            terminator,
        }
    }

    pub fn parameters(&self) -> &[BlockParameter] {
        &self.parameters
    }

    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    pub const fn terminator(&self) -> &Terminator {
        &self.terminator
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    name: String,
    signature: Signature,
    blocks: Vec<BasicBlock>,
    source_map: Option<FunctionSourceMap>,
}

impl Function {
    pub fn new(
        name: impl Into<String>,
        signature: Signature,
        instructions: Vec<Instruction>,
        terminator: Terminator,
    ) -> Self {
        Self::with_blocks(
            name,
            signature,
            vec![BasicBlock::new(instructions, terminator)],
        )
    }

    pub fn with_blocks(
        name: impl Into<String>,
        signature: Signature,
        blocks: Vec<BasicBlock>,
    ) -> Self {
        Self {
            name: name.into(),
            signature,
            blocks,
            source_map: None,
        }
    }

    pub fn with_source_map(
        mut self,
        source_map: FunctionSourceMap,
    ) -> Result<Self, SourceMapError> {
        if source_map.blocks.len() != self.blocks.len() {
            return Err(SourceMapError::BlockCountMismatch {
                expected: self.blocks.len(),
                actual: source_map.blocks.len(),
            });
        }
        for (block, (source_block, mir_block)) in
            source_map.blocks.iter().zip(&self.blocks).enumerate()
        {
            if source_block.instruction_spans.len() != mir_block.instructions.len() {
                return Err(SourceMapError::InstructionCountMismatch {
                    block,
                    expected: mir_block.instructions.len(),
                    actual: source_block.instruction_spans.len(),
                });
            }
        }
        self.source_map = Some(source_map);
        Ok(self)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn blocks(&self) -> &[BasicBlock] {
        &self.blocks
    }

    pub const fn source_map(&self) -> Option<&FunctionSourceMap> {
        self.source_map.as_ref()
    }

    pub fn value_count(&self) -> usize {
        self.signature.parameters.len()
            + self
                .blocks
                .iter()
                .map(|block| block.parameters.len() + block.instructions.len())
                .sum::<usize>()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.name.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if self.name.as_bytes().contains(&0) {
            return Err(ValidationError::NulName);
        }
        if self.blocks.is_empty() {
            return Err(ValidationError::EmptyBody);
        }
        if !self.blocks[BlockId::ENTRY.index()].parameters().is_empty() {
            return Err(ValidationError::EntryBlockHasParameters);
        }
        let has_owned_resources = self
            .signature
            .parameters
            .iter()
            .chain(std::iter::once(&self.signature.result))
            .any(|value_type| matches!(value_type, Type::Own(_)))
            || self.blocks.iter().any(|block| {
                block
                    .parameters()
                    .iter()
                    .any(|parameter| matches!(parameter.value_type(), Type::Own(_)))
            });
        if has_owned_resources && self.blocks.len() != 1 {
            return Err(ValidationError::OwnedResourceControlFlowUnsupported);
        }

        let mut value_types = self.signature.parameters.clone();
        let mut definition_blocks = vec![None; value_types.len()];
        let mut uses = Vec::new();

        for (block_index, block) in self.blocks.iter().enumerate() {
            let block_id = BlockId::new(block_index as u32);
            for parameter in block.parameters() {
                let expected = ValueId::new(value_types.len() as u32);
                if parameter.value() != expected {
                    return Err(ValidationError::NonSequentialParameter {
                        expected,
                        actual: parameter.value(),
                    });
                }
                value_types.push(parameter.value_type());
                definition_blocks.push(Some(block_id));
            }
            for instruction in block.instructions() {
                for (operand, expected) in instruction.operands() {
                    validate_operand(operand, expected, &value_types)?;
                    uses.push((operand, block_id));
                }

                let expected = ValueId::new(value_types.len() as u32);
                if instruction.result() != expected {
                    return Err(ValidationError::NonSequentialResult {
                        expected,
                        actual: instruction.result(),
                    });
                }
                value_types.push(instruction.result_type());
                definition_blocks.push(Some(block_id));
            }

            for (operand, expected) in block.terminator().operands(self.signature.result) {
                validate_operand(operand, expected, &value_types)?;
                uses.push((operand, block_id));
            }
            for target in block.terminator().targets() {
                let target_block = target.block();
                if target_block.index() >= self.blocks.len() {
                    return Err(ValidationError::UndefinedBlock(target_block));
                }
                if target_block == BlockId::ENTRY {
                    return Err(ValidationError::EntryBlockHasPredecessor {
                        predecessor: block_id,
                    });
                }
                let parameters = self.blocks[target_block.index()].parameters();
                if target.arguments().len() != parameters.len() {
                    return Err(ValidationError::BlockArgumentCountMismatch {
                        predecessor: block_id,
                        target: target_block,
                        expected: parameters.len(),
                        actual: target.arguments().len(),
                    });
                }
                for (&argument, parameter) in target.arguments().iter().zip(parameters) {
                    validate_operand(argument, parameter.value_type(), &value_types)?;
                    uses.push((argument, block_id));
                }
            }
        }

        let reachable = reachable_blocks(&self.blocks);
        if let Some(block) = reachable.iter().position(|is_reachable| !is_reachable) {
            return Err(ValidationError::UnreachableBlock(BlockId::new(
                block as u32,
            )));
        }

        let dominators = compute_dominators(&self.blocks);
        for (index, value_type) in value_types.iter().enumerate() {
            if !matches!(value_type, Type::Own(_)) {
                continue;
            }
            let value = ValueId::new(index as u32);
            match uses.iter().filter(|(used, _)| *used == value).count() {
                0 => return Err(ValidationError::OwnedResourceNotConsumed(value)),
                1 => {}
                _ => return Err(ValidationError::OwnedResourceUsedMoreThanOnce(value)),
            }
        }
        for (value, use_block) in uses {
            if let Some(definition_block) = definition_blocks[value.index()]
                && !dominators[use_block.index()][definition_block.index()]
            {
                return Err(ValidationError::ValueDoesNotDominateUse {
                    value,
                    block: use_block,
                });
            }
        }

        Ok(())
    }
}

fn validate_operand(
    value: ValueId,
    expected: Type,
    value_types: &[Type],
) -> Result<(), ValidationError> {
    let Some(&actual) = value_types.get(value.index()) else {
        return Err(ValidationError::UndefinedValue(value));
    };
    if actual != expected {
        return Err(ValidationError::TypeMismatch {
            value,
            expected,
            actual,
        });
    }
    Ok(())
}

fn reachable_blocks(blocks: &[BasicBlock]) -> Vec<bool> {
    let mut reachable = vec![false; blocks.len()];
    let mut pending = vec![BlockId::ENTRY];
    while let Some(block) = pending.pop() {
        if reachable[block.index()] {
            continue;
        }
        reachable[block.index()] = true;
        pending.extend(blocks[block.index()].terminator().successors());
    }
    reachable
}

fn compute_dominators(blocks: &[BasicBlock]) -> Vec<Vec<bool>> {
    let block_count = blocks.len();
    let mut predecessors = vec![Vec::new(); block_count];
    for (block_index, block) in blocks.iter().enumerate() {
        for successor in block.terminator().successors() {
            predecessors[successor.index()].push(block_index);
        }
    }

    let mut dominators = vec![vec![true; block_count]; block_count];
    dominators[BlockId::ENTRY.index()].fill(false);
    dominators[BlockId::ENTRY.index()][BlockId::ENTRY.index()] = true;

    loop {
        let mut changed = false;
        for block in 1..block_count {
            let mut next = vec![true; block_count];
            for &predecessor in &predecessors[block] {
                for (candidate, predecessor_dominates) in
                    next.iter_mut().zip(&dominators[predecessor])
                {
                    *candidate &= predecessor_dominates;
                }
            }
            next[block] = true;
            if next != dominators[block] {
                dominators[block] = next;
                changed = true;
            }
        }
        if !changed {
            return dominators;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceMapError {
    ZeroPosition,
    ReversedSpan,
    EmptySourcePath,
    NulSourcePath,
    BlockCountMismatch {
        expected: usize,
        actual: usize,
    },
    InstructionCountMismatch {
        block: usize,
        expected: usize,
        actual: usize,
    },
}

impl Display for SourceMapError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroPosition => formatter.write_str("source line and column must be non-zero"),
            Self::ReversedSpan => formatter.write_str("source span end precedes its start"),
            Self::EmptySourcePath => formatter.write_str("source path must not be empty"),
            Self::NulSourcePath => formatter.write_str("source path must not contain NUL"),
            Self::BlockCountMismatch { expected, actual } => write!(
                formatter,
                "source map contains {actual} blocks, expected {expected}"
            ),
            Self::InstructionCountMismatch {
                block,
                expected,
                actual,
            } => write!(
                formatter,
                "source map block {block} contains {actual} instruction spans, expected {expected}"
            ),
        }
    }
}

impl Error for SourceMapError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationError {
    EmptyName,
    NulName,
    EmptyBody,
    EntryBlockHasParameters,
    UndefinedValue(ValueId),
    NonSequentialParameter {
        expected: ValueId,
        actual: ValueId,
    },
    NonSequentialResult {
        expected: ValueId,
        actual: ValueId,
    },
    TypeMismatch {
        value: ValueId,
        expected: Type,
        actual: Type,
    },
    UndefinedBlock(BlockId),
    EntryBlockHasPredecessor {
        predecessor: BlockId,
    },
    BlockArgumentCountMismatch {
        predecessor: BlockId,
        target: BlockId,
        expected: usize,
        actual: usize,
    },
    UnreachableBlock(BlockId),
    ValueDoesNotDominateUse {
        value: ValueId,
        block: BlockId,
    },
    OwnedResourceNotConsumed(ValueId),
    OwnedResourceUsedMoreThanOnce(ValueId),
    OwnedResourceControlFlowUnsupported,
}

impl Display for ValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("function name must not be empty"),
            Self::NulName => formatter.write_str("function name must not contain NUL"),
            Self::EmptyBody => formatter.write_str("function must contain an entry block"),
            Self::EntryBlockHasParameters => {
                formatter.write_str("entry block must not declare block parameters")
            }
            Self::UndefinedValue(value) => {
                write!(
                    formatter,
                    "value {} is used before it is defined",
                    value.index()
                )
            }
            Self::NonSequentialParameter { expected, actual } => write!(
                formatter,
                "expected block parameter value {}, found {}",
                expected.index(),
                actual.index()
            ),
            Self::NonSequentialResult { expected, actual } => write!(
                formatter,
                "expected result value {}, found {}",
                expected.index(),
                actual.index()
            ),
            Self::TypeMismatch {
                value,
                expected,
                actual,
            } => write!(
                formatter,
                "value {} has type {actual}, expected {expected}",
                value.index()
            ),
            Self::UndefinedBlock(block) => {
                write!(formatter, "block {} is not defined", block.index())
            }
            Self::EntryBlockHasPredecessor { predecessor } => write!(
                formatter,
                "block {} branches to the entry block",
                predecessor.index()
            ),
            Self::BlockArgumentCountMismatch {
                predecessor,
                target,
                expected,
                actual,
            } => write!(
                formatter,
                "edge from block {} to block {} passes {actual} arguments, expected {expected}",
                predecessor.index(),
                target.index()
            ),
            Self::UnreachableBlock(block) => {
                write!(formatter, "block {} is unreachable", block.index())
            }
            Self::ValueDoesNotDominateUse { value, block } => write!(
                formatter,
                "value {} does not dominate its use in block {}",
                value.index(),
                block.index()
            ),
            Self::OwnedResourceNotConsumed(value) => write!(
                formatter,
                "owned resource value {} is not consumed",
                value.index()
            ),
            Self::OwnedResourceUsedMoreThanOnce(value) => write!(
                formatter,
                "owned resource value {} is consumed more than once",
                value.index()
            ),
            Self::OwnedResourceControlFlowUnsupported => formatter
                .write_str("the owned-resource prototype currently supports only one basic block"),
        }
    }
}

impl Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::{
        BasicBlock, BlockId, BlockParameter, BlockSourceMap, BlockTarget, Function,
        FunctionSourceMap, Instruction, ResourceTypeId, Signature, SourceMapError, SourcePosition,
        SourceSpan, Terminator, Type, ValidationError, ValueId,
    };

    #[test]
    fn rejects_nul_in_function_names() {
        let function = Function::new(
            "name\0suffix",
            Signature::new(vec![Type::I64], Type::I64),
            vec![],
            Terminator::Return(ValueId::new(0)),
        );
        assert_eq!(function.validate(), Err(ValidationError::NulName));
    }

    #[test]
    fn attaches_structurally_valid_source_maps() {
        let function_span = span(1, 1, 3, 2);
        let return_span = span(2, 5, 2, 14);
        let source_map = FunctionSourceMap::new(
            "tests/identity.lang",
            function_span,
            vec![BlockSourceMap::new(vec![], return_span)],
        )
        .unwrap();
        let function = Function::new(
            "identity",
            Signature::new(vec![Type::I64], Type::I64),
            vec![],
            Terminator::Return(ValueId::new(0)),
        )
        .with_source_map(source_map)
        .unwrap();

        let source_map = function.source_map().unwrap();
        assert_eq!(source_map.source_path(), "tests/identity.lang");
        assert_eq!(source_map.function_span(), function_span);
        assert_eq!(source_map.blocks()[0].terminator_span(), return_span);
    }

    #[test]
    fn rejects_invalid_source_maps() {
        assert_eq!(SourcePosition::new(0, 1), Err(SourceMapError::ZeroPosition));
        assert_eq!(
            SourceSpan::new(position(2, 1), position(1, 1)),
            Err(SourceMapError::ReversedSpan)
        );
        assert_eq!(
            FunctionSourceMap::new("bad\0path", span(1, 1, 1, 2), vec![]),
            Err(SourceMapError::NulSourcePath)
        );

        let function = Function::new(
            "constant",
            Signature::new(vec![], Type::I64),
            vec![Instruction::I64Constant {
                result: ValueId::new(0),
                value: 42,
            }],
            Terminator::Return(ValueId::new(0)),
        );
        let source_map = FunctionSourceMap::new(
            "tests/constant.lang",
            span(1, 1, 3, 2),
            vec![BlockSourceMap::new(vec![], span(2, 5, 2, 14))],
        )
        .unwrap();

        assert_eq!(
            function.with_source_map(source_map),
            Err(SourceMapError::InstructionCountMismatch {
                block: 0,
                expected: 1,
                actual: 0,
            })
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

    #[test]
    fn rejects_use_before_definition() {
        let function = Function::new(
            "invalid",
            Signature::new(vec![Type::I64], Type::I64),
            vec![Instruction::I64AddWrapping {
                result: ValueId::new(1),
                left: ValueId::new(0),
                right: ValueId::new(2),
            }],
            Terminator::Return(ValueId::new(1)),
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::UndefinedValue(ValueId::new(2)))
        );
    }

    #[test]
    fn rejects_non_sequential_results() {
        let function = Function::new(
            "invalid",
            Signature::new(vec![], Type::I64),
            vec![Instruction::I64Constant {
                result: ValueId::new(1),
                value: 42,
            }],
            Terminator::Return(ValueId::new(1)),
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::NonSequentialResult {
                expected: ValueId::new(0),
                actual: ValueId::new(1),
            })
        );
    }

    #[test]
    fn validates_a_nominal_owned_resource_transfer() {
        let resource = ResourceTypeId::new(7);
        let function = Function::new(
            "transfer_resource",
            Signature::new(vec![Type::Own(resource)], Type::Own(resource)),
            vec![],
            Terminator::Return(ValueId::new(0)),
        );

        assert_eq!(function.validate(), Ok(()));
        assert_eq!(Type::Own(resource).to_string(), "own<resource#7>");
    }

    #[test]
    fn rejects_dropped_or_control_flow_owned_resources() {
        let resource = ResourceTypeId::new(7);
        let dropped = Function::new(
            "drop_resource",
            Signature::new(vec![Type::Own(resource), Type::I64], Type::I64),
            vec![],
            Terminator::Return(ValueId::new(1)),
        );
        assert_eq!(
            dropped.validate(),
            Err(ValidationError::OwnedResourceNotConsumed(ValueId::new(0)))
        );

        let control_flow = Function::with_blocks(
            "branch_resource",
            Signature::new(vec![Type::Own(resource), Type::Bool], Type::Own(resource)),
            vec![
                BasicBlock::new(
                    vec![],
                    Terminator::Branch {
                        condition: ValueId::new(1),
                        then_target: target(BlockId::new(1)),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(0))),
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(0))),
            ],
        );
        assert_eq!(
            control_flow.validate(),
            Err(ValidationError::OwnedResourceControlFlowUnsupported)
        );
    }

    #[test]
    fn validates_typed_control_flow() {
        assert_eq!(minimum_function().validate(), Ok(()));
    }

    #[test]
    fn rejects_non_boolean_branch_conditions() {
        let function = Function::with_blocks(
            "invalid_condition",
            Signature::new(vec![Type::I64, Type::I64], Type::I64),
            vec![
                BasicBlock::new(
                    vec![],
                    Terminator::Branch {
                        condition: ValueId::new(0),
                        then_target: target(BlockId::new(1)),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(0))),
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(1))),
            ],
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::TypeMismatch {
                value: ValueId::new(0),
                expected: Type::Bool,
                actual: Type::I64,
            })
        );
    }

    #[test]
    fn rejects_undefined_and_unreachable_blocks() {
        let undefined = Function::with_blocks(
            "undefined",
            Signature::new(vec![Type::I64], Type::I64),
            vec![BasicBlock::new(
                vec![],
                Terminator::Jump(target(BlockId::new(1))),
            )],
        );
        assert_eq!(
            undefined.validate(),
            Err(ValidationError::UndefinedBlock(BlockId::new(1)))
        );

        let unreachable = Function::with_blocks(
            "unreachable",
            Signature::new(vec![Type::I64], Type::I64),
            vec![
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(0))),
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(0))),
            ],
        );
        assert_eq!(
            unreachable.validate(),
            Err(ValidationError::UnreachableBlock(BlockId::new(1)))
        );
    }

    #[test]
    fn rejects_entry_block_predecessors() {
        let function = Function::with_blocks(
            "entry_back_edge",
            Signature::new(vec![], Type::I64),
            vec![
                BasicBlock::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
                BasicBlock::new(vec![], Terminator::Jump(target(BlockId::ENTRY))),
            ],
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::EntryBlockHasPredecessor {
                predecessor: BlockId::new(1),
            })
        );
    }

    #[test]
    fn rejects_values_that_do_not_dominate_their_use() {
        let function = Function::with_blocks(
            "invalid_merge",
            Signature::new(vec![Type::I64, Type::Bool], Type::I64),
            vec![
                BasicBlock::new(
                    vec![],
                    Terminator::Branch {
                        condition: ValueId::new(1),
                        then_target: target(BlockId::new(1)),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                BasicBlock::new(
                    vec![Instruction::I64Constant {
                        result: ValueId::new(2),
                        value: 42,
                    }],
                    Terminator::Jump(target(BlockId::new(3))),
                ),
                BasicBlock::new(vec![], Terminator::Jump(target(BlockId::new(3)))),
                BasicBlock::new(vec![], Terminator::Return(ValueId::new(2))),
            ],
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::ValueDoesNotDominateUse {
                value: ValueId::new(2),
                block: BlockId::new(3),
            })
        );
    }

    #[test]
    fn validates_a_back_edge_outside_the_entry_block() {
        let function = Function::with_blocks(
            "loop_forever",
            Signature::new(vec![], Type::I64),
            vec![
                BasicBlock::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
                BasicBlock::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
            ],
        );

        assert_eq!(function.validate(), Ok(()));
    }

    #[test]
    fn validates_loop_carried_block_parameters() {
        assert_eq!(countdown_function().validate(), Ok(()));
    }

    #[test]
    fn rejects_entry_block_parameters() {
        let function = Function::with_blocks(
            "entry_parameter",
            Signature::new(vec![], Type::I64),
            vec![BasicBlock::with_parameters(
                vec![BlockParameter::new(ValueId::new(0), Type::I64)],
                vec![],
                Terminator::Return(ValueId::new(0)),
            )],
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::EntryBlockHasParameters)
        );
    }

    #[test]
    fn rejects_non_sequential_block_parameters() {
        let function = Function::with_blocks(
            "non_sequential_parameter",
            Signature::new(vec![Type::I64], Type::I64),
            vec![
                BasicBlock::new(
                    vec![],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![ValueId::new(0)])),
                ),
                BasicBlock::with_parameters(
                    vec![BlockParameter::new(ValueId::new(2), Type::I64)],
                    vec![],
                    Terminator::Return(ValueId::new(2)),
                ),
            ],
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::NonSequentialParameter {
                expected: ValueId::new(1),
                actual: ValueId::new(2),
            })
        );
    }

    #[test]
    fn rejects_block_argument_count_mismatches() {
        let function = Function::with_blocks(
            "missing_argument",
            Signature::new(vec![Type::I64], Type::I64),
            vec![
                BasicBlock::new(
                    vec![],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![])),
                ),
                BasicBlock::with_parameters(
                    vec![BlockParameter::new(ValueId::new(1), Type::I64)],
                    vec![],
                    Terminator::Return(ValueId::new(1)),
                ),
            ],
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::BlockArgumentCountMismatch {
                predecessor: BlockId::ENTRY,
                target: BlockId::new(1),
                expected: 1,
                actual: 0,
            })
        );
    }

    #[test]
    fn rejects_block_argument_type_mismatches() {
        let function = Function::with_blocks(
            "wrong_argument_type",
            Signature::new(vec![Type::Bool], Type::I64),
            vec![
                BasicBlock::new(
                    vec![],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![ValueId::new(0)])),
                ),
                BasicBlock::with_parameters(
                    vec![BlockParameter::new(ValueId::new(1), Type::I64)],
                    vec![],
                    Terminator::Return(ValueId::new(1)),
                ),
            ],
        );

        assert_eq!(
            function.validate(),
            Err(ValidationError::TypeMismatch {
                value: ValueId::new(0),
                expected: Type::I64,
                actual: Type::Bool,
            })
        );
    }

    fn minimum_function() -> Function {
        Function::with_blocks(
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
        )
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

    fn target(block: BlockId) -> BlockTarget {
        BlockTarget::new(block, vec![])
    }
}
