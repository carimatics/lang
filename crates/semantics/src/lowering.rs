//! Accepted RFC 0002 typed HIR to backend-neutral, validated SSA MIR.
//!
//! This operates on successful structured-input HIR. It neither parses text
//! nor implements any proposed RFC 0006 syntax, call, aggregate, or wire rule.

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_diagnostics::{SourceError, SourceMap, Span};
use lang_hir as hir;
use lang_mir as mir;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoweringLimits {
    pub work: usize,
    pub values: usize,
    pub blocks: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Work,
    Values,
    Blocks,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LowerError {
    Source(SourceError),
    SourceMap(mir::SourceMapError),
    Validation(mir::ValidationError),
    LimitExceeded(LimitKind),
    PositionOverflow,
    ValueIdOverflow,
    BlockIdOverflow,
    ForeignSourceSpan,
    UninitializedLocal(usize),
    MissingReturn,
    MissingTerminator(usize),
    DuplicateTerminator(usize),
}

impl Display for LowerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "HIR to MIR lowering failed: {self:?}")
    }
}

impl Error for LowerError {}

type Environment = Vec<Option<mir::ValueId>>;

struct Exit {
    block: mir::BlockId,
    environment: Environment,
}

#[derive(Default)]
struct Block {
    parameters: Vec<mir::BlockParameter>,
    instructions: Vec<mir::Instruction>,
    instruction_spans: Vec<Span>,
    terminator: Option<(mir::Terminator, Span)>,
}

struct Lowerer<'a> {
    function: &'a hir::Function,
    sources: &'a SourceMap,
    limits: LoweringLimits,
    work: usize,
    values: usize,
    blocks: Vec<Block>,
}

/// Construct MIR under caller-supplied internal budgets, attach source maps,
/// and run the existing MIR validator before returning. The prototype MIR
/// validator's own CPU/allocation budgeting remains a separate hardening gate.
pub fn lower(
    function: &hir::Function,
    sources: &SourceMap,
    limits: LoweringLimits,
) -> Result<mir::Function, LowerError> {
    let mut lowerer = Lowerer {
        function,
        sources,
        limits,
        work: 0,
        values: 0,
        blocks: Vec::new(),
    };
    lowerer.charge(function.locals().len())?;
    let mut environment = vec![None; function.locals().len()];
    for parameter in function.parameters() {
        environment[parameter.index()] = Some(lowerer.value()?);
    }
    let entry = lowerer.block()?;
    if lowerer
        .lower_block(
            function.body(),
            Exit {
                block: entry,
                environment,
            },
        )?
        .is_some()
    {
        return Err(LowerError::MissingReturn);
    }
    lowerer.finish()
}

impl Lowerer<'_> {
    fn charge(&mut self, amount: usize) -> Result<(), LowerError> {
        self.work = self
            .work
            .checked_add(amount)
            .filter(|work| *work <= self.limits.work)
            .ok_or(LowerError::LimitExceeded(LimitKind::Work))?;
        Ok(())
    }

    fn value(&mut self) -> Result<mir::ValueId, LowerError> {
        self.charge(1)?;
        if self.values >= self.limits.values {
            return Err(LowerError::LimitExceeded(LimitKind::Values));
        }
        let value =
            mir::ValueId::new(u32::try_from(self.values).map_err(|_| LowerError::ValueIdOverflow)?);
        self.values += 1;
        Ok(value)
    }

    fn block(&mut self) -> Result<mir::BlockId, LowerError> {
        self.charge(1)?;
        if self.blocks.len() >= self.limits.blocks {
            return Err(LowerError::LimitExceeded(LimitKind::Blocks));
        }
        let id = mir::BlockId::new(
            u32::try_from(self.blocks.len()).map_err(|_| LowerError::BlockIdOverflow)?,
        );
        self.blocks.push(Block::default());
        Ok(id)
    }

    fn copy_environment(&mut self, environment: &Environment) -> Result<Environment, LowerError> {
        self.charge(environment.len())?;
        Ok(environment.clone())
    }

    fn seal(
        &mut self,
        block: mir::BlockId,
        terminator: mir::Terminator,
        span: Span,
    ) -> Result<(), LowerError> {
        self.charge(1)?;
        let block_index = block.index();
        if self.blocks[block_index].terminator.is_some() {
            return Err(LowerError::DuplicateTerminator(block_index));
        }
        self.blocks[block_index].terminator = Some((terminator, span));
        Ok(())
    }

    fn expression(
        &mut self,
        id: hir::ExpressionId,
        exit: &Exit,
    ) -> Result<mir::ValueId, LowerError> {
        self.charge(1)?;
        let expression = &self.function.expressions()[id.index()];
        let instruction = match expression.kind {
            hir::ExpressionKind::Local(local) => {
                return exit.environment[local.index()]
                    .ok_or(LowerError::UninitializedLocal(local.index()));
            }
            hir::ExpressionKind::Integer(value) => mir::Instruction::I64Constant {
                result: self.value()?,
                value,
            },
            hir::ExpressionKind::Add(left, right) | hir::ExpressionKind::LessThan(left, right) => {
                let left = self.expression(left, exit)?;
                let right = self.expression(right, exit)?;
                let result = self.value()?;
                match expression.kind {
                    hir::ExpressionKind::Add(_, _) => mir::Instruction::I64AddWrapping {
                        result,
                        left,
                        right,
                    },
                    _ => mir::Instruction::I64LessThanSigned {
                        result,
                        left,
                        right,
                    },
                }
            }
        };
        let result = instruction.result();
        self.blocks[exit.block.index()]
            .instructions
            .push(instruction);
        self.blocks[exit.block.index()]
            .instruction_spans
            .push(expression.span);
        Ok(result)
    }

    fn lower_block(&mut self, id: hir::BlockId, exit: Exit) -> Result<Option<Exit>, LowerError> {
        self.charge(1)?;
        let function = self.function;
        let block = &function.blocks()[id.index()];
        let mut current = Some(exit);
        for statement in &block.statements {
            let Some(mut exit) = current else {
                break;
            };
            self.charge(1)?;
            current = match statement.kind {
                hir::StatementKind::LetMut { local, initializer } => {
                    let value = self.expression(initializer, &exit)?;
                    exit.environment[local.index()] = Some(value);
                    Some(exit)
                }
                hir::StatementKind::Assign { local, value } => {
                    let value = self.expression(value, &exit)?;
                    exit.environment[local.index()] = Some(value);
                    Some(exit)
                }
                hir::StatementKind::Return(value) => {
                    let value = self.expression(value, &exit)?;
                    self.seal(exit.block, mir::Terminator::Return(value), statement.span)?;
                    None
                }
                hir::StatementKind::If {
                    condition,
                    then_block,
                    else_block,
                } => self.conditional(condition, then_block, else_block, exit)?,
                hir::StatementKind::While { condition, body } => {
                    Some(self.while_loop(condition, body, exit, statement.span)?)
                }
            };
        }
        Ok(current)
    }

    fn conditional(
        &mut self,
        condition: hir::ExpressionId,
        then_hir: hir::BlockId,
        else_hir: Option<hir::BlockId>,
        exit: Exit,
    ) -> Result<Option<Exit>, LowerError> {
        let value = self.expression(condition, &exit)?;
        let then_block = self.block()?;
        let else_block = self.block()?;
        let span = self.function.expressions()[condition.index()].span;
        self.seal(
            exit.block,
            mir::Terminator::Branch {
                condition: value,
                then_target: mir::BlockTarget::new(then_block, vec![]),
                else_target: mir::BlockTarget::new(else_block, vec![]),
            },
            span,
        )?;
        let then_environment = self.copy_environment(&exit.environment)?;
        let else_environment = self.copy_environment(&exit.environment)?;
        let then_exit = self.lower_block(
            then_hir,
            Exit {
                block: then_block,
                environment: then_environment,
            },
        )?;
        let else_exit = match else_hir {
            Some(block) => self.lower_block(
                block,
                Exit {
                    block: else_block,
                    environment: else_environment,
                },
            )?,
            None => Some(Exit {
                block: else_block,
                environment: else_environment,
            }),
        };
        match (then_exit, else_exit) {
            (None, None) => Ok(None),
            (Some(mut continuing), None) | (None, Some(mut continuing)) => {
                // Block-local declarations never escape the branch's scope.
                for (index, original) in exit.environment.iter().enumerate() {
                    self.charge(1)?;
                    if original.is_none() {
                        continuing.environment[index] = None;
                    }
                }
                Ok(Some(continuing))
            }
            (Some(left), Some(right)) => {
                let join = self.block()?;
                let mut environment = exit.environment;
                let mut left_arguments = Vec::new();
                let mut right_arguments = Vec::new();
                for (index, value) in environment.iter_mut().enumerate() {
                    self.charge(1)?;
                    if value.is_none() {
                        continue;
                    }
                    let left_value =
                        left.environment[index].ok_or(LowerError::UninitializedLocal(index))?;
                    let right_value =
                        right.environment[index].ok_or(LowerError::UninitializedLocal(index))?;
                    if left_value == right_value {
                        *value = Some(left_value);
                        continue;
                    }
                    let parameter = self.value()?;
                    self.blocks[join.index()]
                        .parameters
                        .push(mir::BlockParameter::new(parameter, mir::Type::I64));
                    left_arguments.push(left_value);
                    right_arguments.push(right_value);
                    *value = Some(parameter);
                }
                let left_span = self.function.blocks()[then_hir.index()].closing_brace;
                let right_span =
                    else_hir.map_or(span, |id| self.function.blocks()[id.index()].closing_brace);
                self.seal(
                    left.block,
                    mir::Terminator::Jump(mir::BlockTarget::new(join, left_arguments)),
                    left_span,
                )?;
                self.seal(
                    right.block,
                    mir::Terminator::Jump(mir::BlockTarget::new(join, right_arguments)),
                    right_span,
                )?;
                Ok(Some(Exit {
                    block: join,
                    environment,
                }))
            }
        }
    }

    fn while_loop(
        &mut self,
        condition: hir::ExpressionId,
        body_hir: hir::BlockId,
        exit: Exit,
        span: Span,
    ) -> Result<Exit, LowerError> {
        let header = self.block()?;
        let mut header_environment = self.copy_environment(&exit.environment)?;
        let mut carried = Vec::new();
        let mut arguments = Vec::new();
        for (index, value) in header_environment.iter_mut().enumerate() {
            self.charge(1)?;
            if self.function.locals()[index].kind != hir::LocalKind::Mutable {
                continue;
            }
            let Some(initial) = *value else {
                continue;
            };
            let parameter = self.value()?;
            self.blocks[header.index()]
                .parameters
                .push(mir::BlockParameter::new(parameter, mir::Type::I64));
            carried.push(index);
            arguments.push(initial);
            *value = Some(parameter);
        }
        self.seal(
            exit.block,
            mir::Terminator::Jump(mir::BlockTarget::new(header, arguments)),
            span,
        )?;
        let header_exit = Exit {
            block: header,
            environment: header_environment,
        };
        let condition_value = self.expression(condition, &header_exit)?;
        let body = self.block()?;
        let after = self.block()?;
        self.seal(
            header,
            mir::Terminator::Branch {
                condition: condition_value,
                then_target: mir::BlockTarget::new(body, vec![]),
                else_target: mir::BlockTarget::new(after, vec![]),
            },
            self.function.expressions()[condition.index()].span,
        )?;
        let body_environment = self.copy_environment(&header_exit.environment)?;
        if let Some(body_exit) = self.lower_block(
            body_hir,
            Exit {
                block: body,
                environment: body_environment,
            },
        )? {
            self.charge(carried.len())?;
            let arguments = carried
                .into_iter()
                .map(|index| {
                    body_exit.environment[index].ok_or(LowerError::UninitializedLocal(index))
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.seal(
                body_exit.block,
                mir::Terminator::Jump(mir::BlockTarget::new(header, arguments)),
                self.function.blocks()[body_hir.index()].closing_brace,
            )?;
        }
        Ok(Exit {
            block: after,
            environment: header_exit.environment,
        })
    }

    fn source_span(&mut self, span: Span) -> Result<mir::SourceSpan, LowerError> {
        if span.source() != self.function.span().source() {
            return Err(LowerError::ForeignSourceSpan);
        }
        self.sources.slice(span).map_err(LowerError::Source)?;
        let range = span.range();
        // Position lookup scans a line prefix. Charging the entire byte prefix
        // conservatively bounds that work without exposing source internals.
        self.charge(range.start.saturating_add(1))?;
        self.charge(range.end.saturating_add(1))?;
        let file = self
            .sources
            .file(span.source())
            .map_err(LowerError::Source)?;
        let convert = |offset| -> Result<mir::SourcePosition, LowerError> {
            let position = file.position(offset).map_err(LowerError::Source)?;
            mir::SourcePosition::new(
                u32::try_from(position.line).map_err(|_| LowerError::PositionOverflow)?,
                u32::try_from(position.column).map_err(|_| LowerError::PositionOverflow)?,
            )
            .map_err(LowerError::SourceMap)
        };
        mir::SourceSpan::new(convert(range.start)?, convert(range.end)?)
            .map_err(LowerError::SourceMap)
    }

    fn finish(mut self) -> Result<mir::Function, LowerError> {
        // MIR requires IDs to be sequential in final block/definition order.
        // Construction uses symbolic IDs, since nested CFGs allocate values
        // in a different order. Remap every definition/use in one final pass.
        self.charge(self.values)?;
        let mut values = vec![mir::ValueId::new(0); self.values];
        for (index, value) in values
            .iter_mut()
            .enumerate()
            .take(self.function.parameters().len())
        {
            *value =
                mir::ValueId::new(u32::try_from(index).map_err(|_| LowerError::ValueIdOverflow)?);
        }
        let mut next = self.function.parameters().len();
        for block in &self.blocks {
            for old in block
                .parameters
                .iter()
                .copied()
                .map(mir::BlockParameter::value)
                .chain(block.instructions.iter().map(mir::Instruction::result))
            {
                values[old.index()] = mir::ValueId::new(
                    u32::try_from(next).map_err(|_| LowerError::ValueIdOverflow)?,
                );
                next += 1;
            }
        }
        let remap = |value: mir::ValueId| values[value.index()];
        let target = |target: mir::BlockTarget| {
            mir::BlockTarget::new(
                target.block(),
                target.arguments().iter().copied().map(remap).collect(),
            )
        };
        let mut blocks = Vec::new();
        let mut source_blocks = Vec::new();
        for (index, block) in std::mem::take(&mut self.blocks).into_iter().enumerate() {
            self.charge(block.parameters.len() + block.instructions.len() + 1)?;
            let parameters = block
                .parameters
                .into_iter()
                .map(|parameter| {
                    mir::BlockParameter::new(remap(parameter.value()), parameter.value_type())
                })
                .collect();
            let instructions = block
                .instructions
                .into_iter()
                .map(|instruction| match instruction {
                    mir::Instruction::I64Constant { result, value } => {
                        mir::Instruction::I64Constant {
                            result: remap(result),
                            value,
                        }
                    }
                    mir::Instruction::I64AddWrapping {
                        result,
                        left,
                        right,
                    } => mir::Instruction::I64AddWrapping {
                        result: remap(result),
                        left: remap(left),
                        right: remap(right),
                    },
                    mir::Instruction::I64LessThanSigned {
                        result,
                        left,
                        right,
                    } => mir::Instruction::I64LessThanSigned {
                        result: remap(result),
                        left: remap(left),
                        right: remap(right),
                    },
                    mir::Instruction::CallHostI64 {
                        result,
                        function,
                        argument,
                    } => mir::Instruction::CallHostI64 {
                        result: remap(result),
                        function,
                        argument: remap(argument),
                    },
                })
                .collect();
            let (terminator, span) = block
                .terminator
                .ok_or(LowerError::MissingTerminator(index))?;
            let terminator = match terminator {
                mir::Terminator::Return(value) => mir::Terminator::Return(remap(value)),
                mir::Terminator::Jump(destination) => mir::Terminator::Jump(target(destination)),
                mir::Terminator::Branch {
                    condition,
                    then_target,
                    else_target,
                } => mir::Terminator::Branch {
                    condition: remap(condition),
                    then_target: target(then_target),
                    else_target: target(else_target),
                },
            };
            let instruction_spans = block
                .instruction_spans
                .into_iter()
                .map(|span| self.source_span(span))
                .collect::<Result<Vec<_>, _>>()?;
            let terminator_span = self.source_span(span)?;
            source_blocks.push(mir::BlockSourceMap::new(instruction_spans, terminator_span));
            blocks.push(mir::BasicBlock::with_parameters(
                parameters,
                instructions,
                terminator,
            ));
        }
        let span = self.source_span(self.function.span())?;
        let path = self
            .sources
            .file(self.function.span().source())
            .map_err(LowerError::Source)?
            .name();
        let source_map = mir::FunctionSourceMap::new(path, span, source_blocks)
            .map_err(LowerError::SourceMap)?;
        let function = mir::Function::with_blocks(
            self.function.name(),
            mir::Signature::new(
                vec![mir::Type::I64; self.function.parameters().len()],
                mir::Type::I64,
            ),
            blocks,
        )
        .with_source_map(source_map)
        .map_err(LowerError::SourceMap)?;
        function.validate().map_err(LowerError::Validation)?;
        Ok(function)
    }
}
