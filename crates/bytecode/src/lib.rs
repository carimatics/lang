#![forbid(unsafe_code)]

pub mod ownership;
pub mod tasks;
pub mod unit;

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_mir::{
    Function, FunctionSourceMap, HostFunctionId, Instruction as MirInstruction,
    Terminator as MirTerminator, Type, ValidationError,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockId(usize);

impl BlockId {
    pub const ENTRY: Self = Self(0);

    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Instruction {
    I64Constant {
        destination: usize,
        value: i64,
    },
    I64AddWrapping {
        destination: usize,
        left: usize,
        right: usize,
    },
    I64LessThanSigned {
        destination: usize,
        left: usize,
        right: usize,
    },
    CallHostI64 {
        destination: usize,
        function: HostFunctionId,
        argument: usize,
    },
}

impl Instruction {
    const fn destination(self) -> usize {
        match self {
            Self::I64Constant { destination, .. }
            | Self::I64AddWrapping { destination, .. }
            | Self::I64LessThanSigned { destination, .. }
            | Self::CallHostI64 { destination, .. } => destination,
        }
    }

    const fn result_type(self) -> Type {
        match self {
            Self::I64LessThanSigned { .. } => Type::Bool,
            Self::I64Constant { .. } | Self::I64AddWrapping { .. } | Self::CallHostI64 { .. } => {
                Type::I64
            }
        }
    }

    fn sources(self) -> Vec<(usize, Type)> {
        match self {
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
    arguments: Vec<usize>,
}

impl BlockTarget {
    pub fn new(block: BlockId, arguments: Vec<usize>) -> Self {
        Self { block, arguments }
    }

    pub const fn block(&self) -> BlockId {
        self.block
    }

    pub fn arguments(&self) -> &[usize] {
        &self.arguments
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Terminator {
    Return(usize),
    Jump(BlockTarget),
    Branch {
        condition: usize,
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

    fn sources(&self, result_type: Type) -> Vec<(usize, Type)> {
        match self {
            Self::Return(register) => vec![(*register, result_type)],
            Self::Jump(_) => Vec::new(),
            Self::Branch { condition, .. } => vec![(*condition, Type::Bool)],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockParameter {
    register: usize,
    register_type: Type,
}

impl BlockParameter {
    pub const fn new(register: usize, register_type: Type) -> Self {
        Self {
            register,
            register_type,
        }
    }

    pub const fn register(self) -> usize {
        self.register
    }

    pub const fn register_type(self) -> Type {
        self.register_type
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    parameters: Vec<BlockParameter>,
    instructions: Vec<Instruction>,
    terminator: Terminator,
}

impl Block {
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
pub struct Program {
    parameter_types: Vec<Type>,
    result_type: Type,
    register_count: usize,
    blocks: Vec<Block>,
    source_map: Option<FunctionSourceMap>,
}

impl Program {
    pub fn new(
        parameter_count: usize,
        register_count: usize,
        instructions: Vec<Instruction>,
        return_register: usize,
    ) -> Result<Self, VerificationError> {
        Self::with_terminator(
            parameter_count,
            register_count,
            instructions,
            Terminator::Return(return_register),
        )
    }

    pub fn with_terminator(
        parameter_count: usize,
        register_count: usize,
        instructions: Vec<Instruction>,
        terminator: Terminator,
    ) -> Result<Self, VerificationError> {
        Self::with_blocks(
            parameter_count,
            register_count,
            vec![Block::new(instructions, terminator)],
        )
    }

    pub fn with_blocks(
        parameter_count: usize,
        register_count: usize,
        blocks: Vec<Block>,
    ) -> Result<Self, VerificationError> {
        Self::with_typed_blocks(
            vec![Type::I64; parameter_count],
            Type::I64,
            register_count,
            blocks,
        )
    }

    pub fn with_typed_blocks(
        parameter_types: Vec<Type>,
        result_type: Type,
        register_count: usize,
        blocks: Vec<Block>,
    ) -> Result<Self, VerificationError> {
        let program = Self {
            parameter_types,
            result_type,
            register_count,
            blocks,
            source_map: None,
        };
        program.verify()?;
        Ok(program)
    }

    pub const fn parameter_count(&self) -> usize {
        self.parameter_types.len()
    }

    pub fn parameter_types(&self) -> &[Type] {
        &self.parameter_types
    }

    pub const fn result_type(&self) -> Type {
        self.result_type
    }

    pub const fn register_count(&self) -> usize {
        self.register_count
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Source locations retained by the one-to-one MIR instruction/block lowering.
    pub const fn source_map(&self) -> Option<&FunctionSourceMap> {
        self.source_map.as_ref()
    }

    pub fn verify(&self) -> Result<(), VerificationError> {
        if self.parameter_types.len() > self.register_count {
            return Err(VerificationError::InvalidParameterCount);
        }
        if self.blocks.is_empty() {
            return Err(VerificationError::EmptyProgram);
        }
        if !self.blocks[BlockId::ENTRY.index()].parameters().is_empty() {
            return Err(VerificationError::EntryBlockHasParameters);
        }
        let has_owned_resources = self
            .parameter_types
            .iter()
            .chain(std::iter::once(&self.result_type))
            .any(|value_type| matches!(value_type, Type::Own(_)))
            || self.blocks.iter().any(|block| {
                block
                    .parameters()
                    .iter()
                    .any(|parameter| matches!(parameter.register_type(), Type::Own(_)))
            });
        if has_owned_resources && self.blocks.len() != 1 {
            return Err(VerificationError::OwnedResourceControlFlowUnsupported);
        }

        let mut register_types = vec![None; self.register_count];
        for (register, value_type) in register_types.iter_mut().zip(&self.parameter_types) {
            *register = Some(*value_type);
        }
        let mut definition_blocks = vec![None; self.register_count];
        let mut uses = Vec::new();

        for (block_index, block) in self.blocks.iter().enumerate() {
            let block_id = BlockId::new(block_index);
            for parameter in block.parameters() {
                let register = parameter.register();
                verify_destination(register, &register_types)?;
                register_types[register] = Some(parameter.register_type());
                definition_blocks[register] = Some(block_id);
            }
            for &instruction in block.instructions() {
                for (source, expected) in instruction.sources() {
                    verify_source(source, expected, &register_types)?;
                    uses.push((source, block_id));
                }
                let destination = instruction.destination();
                verify_destination(destination, &register_types)?;
                register_types[destination] = Some(instruction.result_type());
                definition_blocks[destination] = Some(block_id);
            }

            for (source, expected) in block.terminator().sources(self.result_type) {
                verify_source(source, expected, &register_types)?;
                uses.push((source, block_id));
            }
            for target in block.terminator().targets() {
                let target_block = target.block();
                if target_block.index() >= self.blocks.len() {
                    return Err(VerificationError::BlockOutOfBounds(target_block));
                }
                if target_block == BlockId::ENTRY {
                    return Err(VerificationError::EntryBlockHasPredecessor {
                        predecessor: block_id,
                    });
                }
                let parameters = self.blocks[target_block.index()].parameters();
                if target.arguments().len() != parameters.len() {
                    return Err(VerificationError::BlockArgumentCountMismatch {
                        predecessor: block_id,
                        target: target_block,
                        expected: parameters.len(),
                        actual: target.arguments().len(),
                    });
                }
                for (&argument, parameter) in target.arguments().iter().zip(parameters) {
                    verify_source(argument, parameter.register_type(), &register_types)?;
                    uses.push((argument, block_id));
                }
            }
        }

        let reachable = reachable_blocks(&self.blocks);
        if let Some(block) = reachable.iter().position(|is_reachable| !is_reachable) {
            return Err(VerificationError::UnreachableBlock(BlockId::new(block)));
        }

        for (register, value_type) in register_types.iter().enumerate() {
            if !matches!(value_type, Some(Type::Own(_))) {
                continue;
            }
            match uses.iter().filter(|(used, _)| *used == register).count() {
                0 => return Err(VerificationError::OwnedResourceNotConsumed(register)),
                1 => {}
                _ => return Err(VerificationError::OwnedResourceUsedMoreThanOnce(register)),
            }
        }

        let dominators = compute_dominators(&self.blocks);
        for (register, use_block) in uses {
            if let Some(definition_block) = definition_blocks[register]
                && !dominators[use_block.index()][definition_block.index()]
            {
                return Err(VerificationError::RegisterDoesNotDominateUse {
                    register,
                    block: use_block,
                });
            }
        }

        Ok(())
    }
}

fn verify_source(
    register: usize,
    expected: Type,
    register_types: &[Option<Type>],
) -> Result<(), VerificationError> {
    match register_types.get(register) {
        Some(Some(actual)) if *actual == expected => Ok(()),
        Some(Some(actual)) => Err(VerificationError::TypeMismatch {
            register,
            expected,
            actual: *actual,
        }),
        Some(None) => Err(VerificationError::UninitializedRegister(register)),
        None => Err(VerificationError::RegisterOutOfBounds(register)),
    }
}

fn verify_destination(
    register: usize,
    register_types: &[Option<Type>],
) -> Result<(), VerificationError> {
    match register_types.get(register) {
        Some(None) => Ok(()),
        Some(Some(_)) => Err(VerificationError::RegisterAlreadyInitialized(register)),
        None => Err(VerificationError::RegisterOutOfBounds(register)),
    }
}

fn reachable_blocks(blocks: &[Block]) -> Vec<bool> {
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

fn compute_dominators(blocks: &[Block]) -> Vec<Vec<bool>> {
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

pub fn lower(function: &Function) -> Result<Program, LowerError> {
    function.validate().map_err(LowerError::InvalidMir)?;
    if function.signature().parameters().contains(&Type::Bool)
        || function.signature().result() == Type::Bool
    {
        return Err(LowerError::UnsupportedSignature);
    }

    let blocks = function
        .blocks()
        .iter()
        .map(|block| {
            let parameters = block
                .parameters()
                .iter()
                .map(|parameter| {
                    BlockParameter::new(parameter.value().index(), parameter.value_type())
                })
                .collect();
            let instructions = block
                .instructions()
                .iter()
                .map(|instruction| match *instruction {
                    MirInstruction::I64Constant { result, value } => Instruction::I64Constant {
                        destination: result.index(),
                        value,
                    },
                    MirInstruction::I64AddWrapping {
                        result,
                        left,
                        right,
                    } => Instruction::I64AddWrapping {
                        destination: result.index(),
                        left: left.index(),
                        right: right.index(),
                    },
                    MirInstruction::I64LessThanSigned {
                        result,
                        left,
                        right,
                    } => Instruction::I64LessThanSigned {
                        destination: result.index(),
                        left: left.index(),
                        right: right.index(),
                    },
                    MirInstruction::CallHostI64 {
                        result,
                        function,
                        argument,
                    } => Instruction::CallHostI64 {
                        destination: result.index(),
                        function,
                        argument: argument.index(),
                    },
                })
                .collect();
            let terminator = match block.terminator() {
                MirTerminator::Return(return_value) => Terminator::Return(return_value.index()),
                MirTerminator::Jump(target) => Terminator::Jump(BlockTarget::new(
                    BlockId::new(target.block().index()),
                    target
                        .arguments()
                        .iter()
                        .map(|argument| argument.index())
                        .collect(),
                )),
                MirTerminator::Branch {
                    condition,
                    then_target,
                    else_target,
                } => Terminator::Branch {
                    condition: condition.index(),
                    then_target: BlockTarget::new(
                        BlockId::new(then_target.block().index()),
                        then_target
                            .arguments()
                            .iter()
                            .map(|argument| argument.index())
                            .collect(),
                    ),
                    else_target: BlockTarget::new(
                        BlockId::new(else_target.block().index()),
                        else_target
                            .arguments()
                            .iter()
                            .map(|argument| argument.index())
                            .collect(),
                    ),
                },
            };
            Block::with_parameters(parameters, instructions, terminator)
        })
        .collect();

    let mut program = Program::with_typed_blocks(
        function.signature().parameters().to_vec(),
        function.signature().result(),
        function.value_count(),
        blocks,
    )
    .map_err(LowerError::InvalidBytecode)?;
    program.source_map = function.source_map().cloned();
    Ok(program)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationError {
    InvalidParameterCount,
    EmptyProgram,
    EntryBlockHasParameters,
    RegisterOutOfBounds(usize),
    UninitializedRegister(usize),
    RegisterAlreadyInitialized(usize),
    TypeMismatch {
        register: usize,
        expected: Type,
        actual: Type,
    },
    BlockOutOfBounds(BlockId),
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
    RegisterDoesNotDominateUse {
        register: usize,
        block: BlockId,
    },
    OwnedResourceNotConsumed(usize),
    OwnedResourceUsedMoreThanOnce(usize),
    OwnedResourceControlFlowUnsupported,
}

impl Display for VerificationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidParameterCount => {
                formatter.write_str("parameter count exceeds register count")
            }
            Self::EmptyProgram => formatter.write_str("program must contain an entry block"),
            Self::EntryBlockHasParameters => {
                formatter.write_str("entry block must not declare block parameters")
            }
            Self::RegisterOutOfBounds(register) => {
                write!(formatter, "register {register} is out of bounds")
            }
            Self::UninitializedRegister(register) => {
                write!(formatter, "register {register} is not initialized")
            }
            Self::RegisterAlreadyInitialized(register) => {
                write!(
                    formatter,
                    "register {register} is initialized more than once"
                )
            }
            Self::TypeMismatch {
                register,
                expected,
                actual,
            } => write!(
                formatter,
                "register {register} has type {actual}, expected {expected}"
            ),
            Self::BlockOutOfBounds(block) => {
                write!(formatter, "block {} is out of bounds", block.index())
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
            Self::RegisterDoesNotDominateUse { register, block } => write!(
                formatter,
                "register {register} does not dominate its use in block {}",
                block.index()
            ),
            Self::OwnedResourceNotConsumed(register) => {
                write!(
                    formatter,
                    "owned resource register {register} is not consumed"
                )
            }
            Self::OwnedResourceUsedMoreThanOnce(register) => write!(
                formatter,
                "owned resource register {register} is consumed more than once"
            ),
            Self::OwnedResourceControlFlowUnsupported => formatter
                .write_str("the owned-resource prototype currently supports only one basic block"),
        }
    }
}

impl Error for VerificationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LowerError {
    InvalidMir(ValidationError),
    InvalidBytecode(VerificationError),
    UnsupportedSignature,
}

impl Display for LowerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMir(error) => write!(formatter, "invalid MIR: {error}"),
            Self::InvalidBytecode(error) => write!(formatter, "invalid bytecode: {error}"),
            Self::UnsupportedSignature => formatter.write_str(
                "bytecode prototype supports only i64 or owned-resource parameters and results",
            ),
        }
    }
}

impl Error for LowerError {}

#[cfg(test)]
mod tests {
    use lang_mir::{
        BlockSourceMap, Function, FunctionSourceMap, HostFunctionId, ResourceTypeId, Signature,
        SourcePosition, SourceSpan, Terminator as MirTerminator, Type, ValueId,
    };

    #[test]
    fn lowering_preserves_instruction_and_terminator_spans() {
        let span = |line| {
            SourceSpan::new(
                SourcePosition::new(line, 1).unwrap(),
                SourcePosition::new(line, 9).unwrap(),
            )
            .unwrap()
        };
        let function = Function::with_blocks(
            "located",
            Signature::new(vec![], Type::I64),
            vec![
                lang_mir::BasicBlock::new(
                    vec![lang_mir::Instruction::I64Constant {
                        result: ValueId::new(0),
                        value: 42,
                    }],
                    MirTerminator::Jump(lang_mir::BlockTarget::new(
                        lang_mir::BlockId::new(1),
                        vec![],
                    )),
                ),
                lang_mir::BasicBlock::new(vec![], MirTerminator::Return(ValueId::new(0))),
            ],
        );
        assert!(lower(&function).unwrap().source_map().is_none());
        let source_map = FunctionSourceMap::new(
            "fixture.lang",
            span(1),
            vec![
                BlockSourceMap::new(vec![span(2)], span(3)),
                BlockSourceMap::new(vec![], span(4)),
            ],
        )
        .unwrap();
        let program = lower(&function.with_source_map(source_map.clone()).unwrap()).unwrap();
        assert_eq!(program.source_map(), Some(&source_map));
        assert_eq!(program.verify(), Ok(()));
    }

    use super::{
        Block, BlockId, BlockParameter, BlockTarget, Instruction, Program, Terminator,
        VerificationError, lower,
    };

    #[test]
    fn lowers_a_nominal_owned_resource_signature() {
        let resource = ResourceTypeId::new(7);
        let function = Function::new(
            "transfer_resource",
            Signature::new(vec![Type::Own(resource)], Type::Own(resource)),
            vec![],
            MirTerminator::Return(ValueId::new(0)),
        );

        let program = lower(&function).expect("lowers the owned resource identity");

        assert_eq!(program.parameter_types(), &[Type::Own(resource)]);
        assert_eq!(program.result_type(), Type::Own(resource));
        assert_eq!(program.verify(), Ok(()));
    }

    #[test]
    fn rejects_a_mismatched_owned_resource_result() {
        let source = ResourceTypeId::new(7);
        let destination = ResourceTypeId::new(8);

        assert_eq!(
            Program::with_typed_blocks(
                vec![Type::Own(source)],
                Type::Own(destination),
                1,
                vec![Block::new(vec![], Terminator::Return(0))],
            ),
            Err(VerificationError::TypeMismatch {
                register: 0,
                expected: Type::Own(destination),
                actual: Type::Own(source),
            })
        );
    }

    #[test]
    fn rejects_reads_from_uninitialized_registers() {
        let result = Program::new(
            1,
            3,
            vec![Instruction::I64AddWrapping {
                destination: 2,
                left: 0,
                right: 1,
            }],
            2,
        );

        assert_eq!(result, Err(VerificationError::UninitializedRegister(1)));
    }

    #[test]
    fn rejects_uninitialized_host_call_arguments() {
        let result = Program::new(
            0,
            2,
            vec![Instruction::CallHostI64 {
                destination: 1,
                function: HostFunctionId::new(0),
                argument: 0,
            }],
            1,
        );

        assert_eq!(result, Err(VerificationError::UninitializedRegister(0)));
    }

    #[test]
    fn verifies_typed_control_flow() {
        let result = minimum_program();

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_i64_branch_conditions() {
        let result = Program::with_blocks(
            2,
            2,
            vec![
                Block::new(
                    vec![],
                    Terminator::Branch {
                        condition: 0,
                        then_target: target(BlockId::new(1)),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                Block::new(vec![], Terminator::Return(0)),
                Block::new(vec![], Terminator::Return(1)),
            ],
        );

        assert_eq!(
            result,
            Err(VerificationError::TypeMismatch {
                register: 0,
                expected: Type::Bool,
                actual: Type::I64,
            })
        );
    }

    #[test]
    fn rejects_registers_that_do_not_dominate_their_use() {
        let result = Program::with_blocks(
            1,
            3,
            vec![
                Block::new(
                    vec![Instruction::I64LessThanSigned {
                        destination: 1,
                        left: 0,
                        right: 0,
                    }],
                    Terminator::Branch {
                        condition: 1,
                        then_target: target(BlockId::new(1)),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                Block::new(
                    vec![Instruction::I64Constant {
                        destination: 2,
                        value: 42,
                    }],
                    Terminator::Jump(target(BlockId::new(3))),
                ),
                Block::new(vec![], Terminator::Jump(target(BlockId::new(3)))),
                Block::new(vec![], Terminator::Return(2)),
            ],
        );

        assert_eq!(
            result,
            Err(VerificationError::RegisterDoesNotDominateUse {
                register: 2,
                block: BlockId::new(3),
            })
        );
    }

    #[test]
    fn rejects_invalid_block_graphs() {
        let out_of_bounds = Program::with_blocks(
            0,
            0,
            vec![Block::new(
                vec![],
                Terminator::Jump(target(BlockId::new(1))),
            )],
        );
        assert_eq!(
            out_of_bounds,
            Err(VerificationError::BlockOutOfBounds(BlockId::new(1)))
        );

        let unreachable = Program::with_blocks(
            1,
            1,
            vec![
                Block::new(vec![], Terminator::Return(0)),
                Block::new(vec![], Terminator::Return(0)),
            ],
        );
        assert_eq!(
            unreachable,
            Err(VerificationError::UnreachableBlock(BlockId::new(1)))
        );

        let entry_predecessor = Program::with_blocks(
            0,
            0,
            vec![
                Block::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
                Block::new(vec![], Terminator::Jump(target(BlockId::ENTRY))),
            ],
        );
        assert_eq!(
            entry_predecessor,
            Err(VerificationError::EntryBlockHasPredecessor {
                predecessor: BlockId::new(1),
            })
        );
    }

    #[test]
    fn verifies_a_back_edge_outside_the_entry_block() {
        let result = Program::with_blocks(
            0,
            0,
            vec![
                Block::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
                Block::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
            ],
        );

        assert!(result.is_ok());
    }

    #[test]
    fn verifies_loop_carried_block_parameters() {
        let result = countdown_program();

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_entry_block_parameters() {
        let result = Program::with_blocks(
            0,
            1,
            vec![Block::with_parameters(
                vec![BlockParameter::new(0, Type::I64)],
                vec![],
                Terminator::Return(0),
            )],
        );

        assert_eq!(result, Err(VerificationError::EntryBlockHasParameters));
    }

    #[test]
    fn rejects_block_argument_count_mismatches() {
        let result = Program::with_blocks(
            1,
            2,
            vec![
                Block::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
                Block::with_parameters(
                    vec![BlockParameter::new(1, Type::I64)],
                    vec![],
                    Terminator::Return(1),
                ),
            ],
        );

        assert_eq!(
            result,
            Err(VerificationError::BlockArgumentCountMismatch {
                predecessor: BlockId::ENTRY,
                target: BlockId::new(1),
                expected: 1,
                actual: 0,
            })
        );
    }

    #[test]
    fn rejects_block_argument_type_mismatches() {
        let result = Program::with_blocks(
            1,
            3,
            vec![
                Block::new(
                    vec![Instruction::I64LessThanSigned {
                        destination: 1,
                        left: 0,
                        right: 0,
                    }],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![1])),
                ),
                Block::with_parameters(
                    vec![BlockParameter::new(2, Type::I64)],
                    vec![],
                    Terminator::Return(2),
                ),
            ],
        );

        assert_eq!(
            result,
            Err(VerificationError::TypeMismatch {
                register: 1,
                expected: Type::I64,
                actual: Type::Bool,
            })
        );
    }

    fn minimum_program() -> Result<Program, VerificationError> {
        Program::with_blocks(
            2,
            3,
            vec![
                Block::new(
                    vec![Instruction::I64LessThanSigned {
                        destination: 2,
                        left: 0,
                        right: 1,
                    }],
                    Terminator::Branch {
                        condition: 2,
                        then_target: target(BlockId::new(1)),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                Block::new(vec![], Terminator::Return(0)),
                Block::new(vec![], Terminator::Return(1)),
            ],
        )
    }

    fn countdown_program() -> Result<Program, VerificationError> {
        Program::with_blocks(
            1,
            7,
            vec![
                Block::new(
                    vec![
                        Instruction::I64Constant {
                            destination: 1,
                            value: -1,
                        },
                        Instruction::I64Constant {
                            destination: 2,
                            value: 1,
                        },
                    ],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![0])),
                ),
                Block::with_parameters(
                    vec![BlockParameter::new(3, Type::I64)],
                    vec![Instruction::I64LessThanSigned {
                        destination: 4,
                        left: 3,
                        right: 2,
                    }],
                    Terminator::Branch {
                        condition: 4,
                        then_target: BlockTarget::new(BlockId::new(3), vec![3]),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                Block::new(
                    vec![Instruction::I64AddWrapping {
                        destination: 5,
                        left: 3,
                        right: 1,
                    }],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![5])),
                ),
                Block::with_parameters(
                    vec![BlockParameter::new(6, Type::I64)],
                    vec![],
                    Terminator::Return(6),
                ),
            ],
        )
    }

    fn target(block: BlockId) -> BlockTarget {
        BlockTarget::new(block, vec![])
    }
}
