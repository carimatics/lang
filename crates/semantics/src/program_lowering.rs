//! Typed source HIR to the shared RFC 0006 compilation-unit MIR.
//!
//! Construction and final verification share a cumulative allocation/work
//! budget. Source snapshots are copied, never reread from display paths.

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_diagnostics::{SourceError, SourceId, SourceMap, Span};
use lang_hir::{
    program::{FunctionFacts, TypedProgram},
    resolved as r,
};
use lang_mir::unit as m;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LowerError {
    Source(SourceError),
    Mir(m::Error),
    MissingSource,
    MissingReturn,
    MissingTerminator,
    DuplicateTerminator,
    UninitializedLocal(usize),
    Depth,
    Capacity,
    UnsupportedConstructedType(r::ConstructedTypeId),
    UnsupportedOwnershipBody { span: Span },
}
impl From<m::Error> for LowerError {
    fn from(e: m::Error) -> Self {
        Self::Mir(e)
    }
}
impl From<SourceError> for LowerError {
    fn from(e: SourceError) -> Self {
        Self::Source(e)
    }
}
impl Display for LowerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "source MIR lowering failed: {self:?}")
    }
}
impl Error for LowerError {}
type Result<T> = std::result::Result<T, LowerError>;

fn id(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| LowerError::Capacity)
}
fn cap(n: usize, max: usize, kind: m::LimitKind) -> Result<()> {
    if n > max {
        Err(m::Error::new(m::ErrorKind::Limit {
            kind,
            limit: max,
            requested: n,
        })
        .into())
    } else {
        Ok(())
    }
}
fn push<T>(budget: &mut m::Budget, values: &mut Vec<T>, value: T) -> Result<()> {
    if values.len() == values.capacity() {
        // Charge every requested slot, including buffers replaced by growth.
        let next = values
            .capacity()
            .max(4)
            .checked_mul(2)
            .ok_or(LowerError::Capacity)?;
        budget.allocate::<T>(next)?;
        values
            .try_reserve_exact(next - values.len())
            .map_err(|_| LowerError::Capacity)?;
    }
    values.push(value);
    Ok(())
}
fn text(budget: &mut m::Budget, value: &str) -> Result<String> {
    budget.charge(value.len().div_ceil(1024))?;
    budget.bytes(value.len())?;
    let mut result = String::new();
    result
        .try_reserve_exact(value.len())
        .map_err(|_| LowerError::Capacity)?;
    result.push_str(value);
    Ok(result)
}
fn ty(program: &r::Program, value: r::Type) -> Result<m::Type> {
    Ok(match value {
        r::Type::Constructed(id) => return Err(LowerError::UnsupportedConstructedType(id)),
        r::Type::I64 => m::Type::I64,
        r::Type::Bool => m::Type::Bool,
        r::Type::Nominal(d) => match program.definitions[d.0].kind {
            r::DefinitionKind::Record(_) => m::Type::Record(m::DefinitionId(id(d.0)?)),
            r::DefinitionKind::Enum(_) => m::Type::Enum(m::DefinitionId(id(d.0)?)),
        },
    })
}

struct Context<'a> {
    input: &'a r::Program,
    sources: &'a SourceMap,
    source_ids: Vec<(SourceId, m::SourceId)>,
    budget: m::Budget,
    registers: usize,
    blocks: usize,
    instructions: usize,
}
impl Context<'_> {
    fn span(&mut self, span: Span) -> Result<Option<m::Span>> {
        self.budget.charge(self.source_ids.len() + 1)?;
        self.sources.slice(span)?;
        let source = self
            .source_ids
            .iter()
            .find(|(source, _)| *source == span.source())
            .ok_or(LowerError::MissingSource)?
            .1;
        Ok(Some(m::Span {
            source,
            start: id(span.range().start)?,
            end: id(span.range().end)?,
        }))
    }
}

/// Lower only an immutable, successfully checked program. A valid root `main`
/// becomes the optional script entry; library units need no entry. An invalid
/// root main signature is rejected by the unit verifier before publication.
pub fn lower(
    program: &TypedProgram,
    sources: &SourceMap,
    limits: m::Limits,
) -> Result<m::VerifiedUnit> {
    lower_unit(program, sources, limits, true)
}

/// Library/REPL analysis does not select a script entry, even if an ordinary
/// function is named main. Explicit embedding invocation still checks its type.
pub fn lower_library(
    program: &TypedProgram,
    sources: &SourceMap,
    limits: m::Limits,
) -> Result<m::VerifiedUnit> {
    lower_unit(program, sources, limits, false)
}

fn lower_unit(
    program: &TypedProgram,
    sources: &SourceMap,
    limits: m::Limits,
    select_entry: bool,
) -> Result<m::VerifiedUnit> {
    let input = program.source();
    if let Some(span) = input
        .imports
        .iter()
        .find(|i| i.abi != r::ImportAbi::Host)
        .map(|i| i.span)
        .or_else(|| {
            input
                .functions
                .iter()
                .find(|f| f.abi == r::FunctionAbi::CExport)
                .map(|f| f.signature_span)
        })
    {
        return Err(LowerError::UnsupportedOwnershipBody { span });
    }
    let mut c = Context {
        input,
        sources,
        source_ids: Vec::new(),
        budget: m::Budget::new(limits),
        registers: 0,
        blocks: 0,
        instructions: 0,
    };
    cap(
        input.definitions.len(),
        limits.definitions,
        m::LimitKind::Definitions,
    )?;
    cap(input.imports.len(), limits.imports, m::LimitKind::Imports)?;
    cap(
        input.functions.len(),
        limits.functions,
        m::LimitKind::Functions,
    )?;
    let mut unit = m::Unit {
        sources: Vec::new(),
        definitions: c.budget.vector(input.definitions.len())?,
        imports: c.budget.vector(input.imports.len())?,
        functions: c.budget.vector(input.functions.len())?,
        entry: None,
    };
    let mut source_bytes = 0usize;
    for module in &input.modules {
        c.budget.charge(c.source_ids.len() + 1)?;
        let source = module.span.source();
        if c.source_ids.iter().any(|(s, _)| *s == source) {
            continue;
        }
        cap(
            unit.sources.len() + 1,
            limits.sources,
            m::LimitKind::Sources,
        )?;
        let file = sources.file(source)?;
        cap(
            file.text().len(),
            limits.source_bytes,
            m::LimitKind::SourceBytes,
        )?;
        source_bytes = source_bytes
            .checked_add(file.text().len())
            .ok_or(LowerError::Capacity)?;
        cap(
            source_bytes,
            limits.total_source_bytes,
            m::LimitKind::TotalSourceBytes,
        )?;
        let mapped = m::SourceId(id(unit.sources.len())?);
        let item = m::Source {
            path: text(&mut c.budget, file.name())?,
            text: text(&mut c.budget, file.text())?,
        };
        push(&mut c.budget, &mut unit.sources, item)?;
        push(&mut c.budget, &mut c.source_ids, (source, mapped))?;
    }
    for definition in &input.definitions {
        c.budget.charge(1)?;
        let kind = match &definition.kind {
            r::DefinitionKind::Record(fields) => {
                cap(fields.len(), limits.fields, m::LimitKind::Fields)?;
                let mut out = c.budget.vector(fields.len())?;
                for field in fields {
                    c.budget.charge(1)?;
                    out.push(m::Field {
                        name: text(&mut c.budget, &field.name.text)?,
                        value_type: ty(input, field.value_type.value_type)?,
                    });
                }
                m::DefinitionKind::Record(out)
            }
            r::DefinitionKind::Enum(cases) => {
                cap(cases.len(), limits.fields, m::LimitKind::Fields)?;
                let mut out = c.budget.vector(cases.len())?;
                for case in cases {
                    cap(case.payload.len(), limits.fields, m::LimitKind::Fields)?;
                    c.budget.charge(case.payload.len() + 1)?;
                    let mut payload = c.budget.vector(case.payload.len())?;
                    for t in &case.payload {
                        payload.push(ty(input, t.value_type)?);
                    }
                    out.push(m::Case {
                        name: text(&mut c.budget, &case.name.text)?,
                        payload,
                    });
                }
                m::DefinitionKind::Enum(out)
            }
        };
        unit.definitions.push(m::Definition {
            name: text(&mut c.budget, &definition.qualified_name)?,
            kind,
        });
    }
    for import in &input.imports {
        c.budget.charge(1)?;
        let mut parameters = c.budget.vector(1)?;
        parameters.push(m::Type::I64);
        unit.imports.push(m::Import {
            name: text(&mut c.budget, &import.qualified_name)?,
            signature: m::Signature {
                parameters,
                result: m::Type::I64,
            },
        });
    }
    for (i, function) in input.functions.iter().enumerate() {
        if select_entry && function.qualified_name == "main" {
            unit.entry = Some(m::FunctionId(id(i)?));
        }
        unit.functions
            .push(Lowerer::run(&mut c, function, &program.functions()[i])?);
    }
    Ok(unit.verify_with_budget(&mut c.budget)?)
}

type Environment = Vec<Option<m::Register>>;
struct Exit {
    block: m::BlockId,
    environment: Environment,
}
#[derive(Default)]
struct Block {
    parameters: Vec<m::Register>,
    instructions: Vec<m::Instruction>,
    terminator: Option<m::Terminator>,
}
struct Lowerer<'a, 'b> {
    context: &'b mut Context<'a>,
    function: &'a r::Function,
    facts: &'a FunctionFacts,
    registers: Vec<m::Type>,
    blocks: Vec<Block>,
}
impl<'a> Lowerer<'a, '_> {
    fn run(
        c: &mut Context<'a>,
        function: &'a r::Function,
        facts: &'a FunctionFacts,
    ) -> Result<m::Function> {
        let mut l = Lowerer {
            context: c,
            function,
            facts,
            registers: Vec::new(),
            blocks: Vec::new(),
        };
        l.context.budget.charge(function.locals.len())?;
        let mut environment = l.context.budget.vector(function.locals.len())?;
        environment.resize(function.locals.len(), None);
        let mut parameters = l.context.budget.vector(function.parameters.len())?;
        for local in &function.parameters {
            let t = ty(l.context.input, facts.locals()[local.0])?;
            environment[local.0] = Some(l.register(t)?);
            parameters.push(t);
        }
        let block = l.block()?;
        if l.body(function.body, Exit { block, environment }, 0)?
            .is_some()
        {
            return Err(LowerError::MissingReturn);
        }
        let mut blocks = l.context.budget.vector(l.blocks.len())?;
        for b in l.blocks {
            blocks.push(m::Block {
                parameters: b.parameters,
                instructions: b.instructions,
                terminator: b.terminator.ok_or(LowerError::MissingTerminator)?,
            });
        }
        Ok(m::Function {
            name: text(&mut l.context.budget, &function.qualified_name)?,
            signature: m::Signature {
                parameters,
                result: ty(l.context.input, function.result.value_type)?,
            },
            registers: l.registers,
            span: l.context.span(function.span)?,
            blocks,
        })
    }
    fn depth(&mut self, n: usize) -> Result<()> {
        self.context.budget.charge(1)?;
        if n > 256 {
            return Err(LowerError::Depth);
        }
        Ok(())
    }
    fn register(&mut self, t: m::Type) -> Result<m::Register> {
        self.context.budget.charge(1)?;
        cap(
            self.context.registers + 1,
            self.context.budget.limits().registers,
            m::LimitKind::Registers,
        )?;
        let register = m::Register(id(self.registers.len())?);
        push(&mut self.context.budget, &mut self.registers, t)?;
        self.context.registers += 1;
        Ok(register)
    }
    fn block(&mut self) -> Result<m::BlockId> {
        self.context.budget.charge(1)?;
        cap(
            self.context.blocks + 1,
            self.context.budget.limits().blocks,
            m::LimitKind::Blocks,
        )?;
        let block = m::BlockId(id(self.blocks.len())?);
        push(&mut self.context.budget, &mut self.blocks, Block::default())?;
        self.context.blocks += 1;
        Ok(block)
    }
    fn environment(&mut self, e: &Environment) -> Result<Environment> {
        self.context.budget.charge(e.len())?;
        let mut copy = self.context.budget.vector(e.len())?;
        copy.extend_from_slice(e);
        Ok(copy)
    }
    fn seal(&mut self, block: m::BlockId, kind: m::Term, span: Span) -> Result<()> {
        self.context.budget.charge(1)?;
        let span = self.context.span(span)?;
        let b = &mut self.blocks[block.0 as usize];
        if b.terminator.is_some() {
            return Err(LowerError::DuplicateTerminator);
        }
        b.terminator = Some(m::Terminator { kind, span });
        Ok(())
    }
    fn emit(
        &mut self,
        block: m::BlockId,
        t: m::Type,
        span: Span,
        op: impl FnOnce(m::Register) -> m::Op,
    ) -> Result<m::Register> {
        cap(
            self.context.instructions + 1,
            self.context.budget.limits().instructions,
            m::LimitKind::Instructions,
        )?;
        let destination = self.register(t)?;
        let span = self.context.span(span)?;
        push(
            &mut self.context.budget,
            &mut self.blocks[block.0 as usize].instructions,
            m::Instruction {
                op: op(destination),
                span,
            },
        )?;
        self.context.instructions += 1;
        Ok(destination)
    }
    fn expression(
        &mut self,
        expression: r::ExpressionId,
        exit: &Exit,
        depth: usize,
    ) -> Result<m::Register> {
        self.depth(depth)?;
        let source = &self.function.expressions[expression.0];
        let span = source.span;
        let t = ty(self.context.input, self.facts.expressions()[expression.0])?;
        let op = match &source.kind {
            r::ExpressionKind::Borrow { .. }
            | r::ExpressionKind::Spawn(_)
            | r::ExpressionKind::Await(_)
            | r::ExpressionKind::Dereference(_)
            | r::ExpressionKind::CoreCall { .. }
            | r::ExpressionKind::CoreUnit
            | r::ExpressionKind::CoreCase { .. } => {
                return Err(LowerError::UnsupportedOwnershipBody { span: source.span });
            }
            r::ExpressionKind::Local(local) => {
                return exit.environment[local.0].ok_or(LowerError::UninitializedLocal(local.0));
            }
            r::ExpressionKind::Integer(value) => m::Op::I64Constant {
                destination: m::Register(0),
                value: *value,
            },
            r::ExpressionKind::Boolean(value) => m::Op::BoolConstant {
                destination: m::Register(0),
                value: *value,
            },
            r::ExpressionKind::Add(a, b) | r::ExpressionKind::LessThan(a, b) => {
                let left = self.expression(*a, exit, depth + 1)?;
                let right = self.expression(*b, exit, depth + 1)?;
                if matches!(source.kind, r::ExpressionKind::Add(..)) {
                    m::Op::Add {
                        destination: m::Register(0),
                        left,
                        right,
                    }
                } else {
                    m::Op::LessThan {
                        destination: m::Register(0),
                        left,
                        right,
                    }
                }
            }
            r::ExpressionKind::Call { callee, arguments } => {
                let mut out = self.context.budget.vector(arguments.len())?;
                for a in arguments {
                    out.push(self.expression(*a, exit, depth + 1)?);
                }
                match callee {
                    r::Callable::Guest(f) => m::Op::Call {
                        destination: m::Register(0),
                        function: m::FunctionId(id(f.0)?),
                        arguments: out,
                    },
                    r::Callable::Host(i) => m::Op::HostCall {
                        destination: m::Register(0),
                        import: m::ImportId(id(i.0)?),
                        argument: out[0],
                    },
                    r::Callable::Case { definition, index } => m::Op::Enum {
                        destination: m::Register(0),
                        definition: m::DefinitionId(id(definition.0)?),
                        case: id(*index)?,
                        payload: out,
                    },
                }
            }
            r::ExpressionKind::Case { definition, index } => m::Op::Enum {
                destination: m::Register(0),
                definition: m::DefinitionId(id(definition.0)?),
                case: id(*index)?,
                payload: Vec::new(),
            },
            r::ExpressionKind::Record { definition, fields } => {
                let mut out = self.context.budget.vector(fields.len())?;
                out.resize(fields.len(), m::Register(0));
                for (written, field) in fields.iter().enumerate() {
                    let v = self.expression(field.value, exit, depth + 1)?;
                    out[self.facts.record_order(expression).expect("checked record")[written]] = v;
                }
                m::Op::Record {
                    destination: m::Register(0),
                    definition: m::DefinitionId(id(definition.0)?),
                    fields: out,
                }
            }
            r::ExpressionKind::Field { receiver, .. } => {
                let receiver = self.expression(*receiver, exit, depth + 1)?;
                m::Op::Field {
                    destination: m::Register(0),
                    receiver,
                    field: id(self.facts.field_index(expression).expect("checked field"))?,
                }
            }
        };
        self.emit(exit.block, t, span, |destination| {
            // Only the destination is symbolic; operand IDs are already final.
            match op {
                m::Op::I64Constant { value, .. } => m::Op::I64Constant { destination, value },
                m::Op::BoolConstant { value, .. } => m::Op::BoolConstant { destination, value },
                m::Op::Add { left, right, .. } => m::Op::Add {
                    destination,
                    left,
                    right,
                },
                m::Op::LessThan { left, right, .. } => m::Op::LessThan {
                    destination,
                    left,
                    right,
                },
                m::Op::Call {
                    function,
                    arguments,
                    ..
                } => m::Op::Call {
                    destination,
                    function,
                    arguments,
                },
                m::Op::HostCall {
                    import, argument, ..
                } => m::Op::HostCall {
                    destination,
                    import,
                    argument,
                },
                m::Op::Enum {
                    definition,
                    case,
                    payload,
                    ..
                } => m::Op::Enum {
                    destination,
                    definition,
                    case,
                    payload,
                },
                m::Op::Record {
                    definition, fields, ..
                } => m::Op::Record {
                    destination,
                    definition,
                    fields,
                },
                m::Op::Field {
                    receiver, field, ..
                } => m::Op::Field {
                    destination,
                    receiver,
                    field,
                },
                _ => unreachable!(),
            }
        })
    }
    fn body(&mut self, block: r::BlockId, exit: Exit, depth: usize) -> Result<Option<Exit>> {
        self.depth(depth)?;
        let mut current = Some(exit);
        for statement in &self.function.blocks[block.0].statements {
            let Some(mut exit) = current else {
                break;
            };
            self.context.budget.charge(1)?;
            current = match &statement.kind {
                r::StatementKind::PlaceAssign { .. }
                | r::StatementKind::TaskScope { .. }
                | r::StatementKind::Yield
                | r::StatementKind::Cancel(_)
                | r::StatementKind::CancelCheck
                | r::StatementKind::Arena { .. }
                | r::StatementKind::Unsafe { .. } => {
                    return Err(LowerError::UnsupportedOwnershipBody {
                        span: statement.span,
                    });
                }
                r::StatementKind::LetMut { local, initializer }
                | r::StatementKind::Assign {
                    local,
                    value: initializer,
                } => {
                    exit.environment[local.0] = Some(self.expression(*initializer, &exit, 0)?);
                    Some(exit)
                }
                r::StatementKind::Return(value) => {
                    let value = self.expression(*value, &exit, 0)?;
                    self.seal(exit.block, m::Term::Return(value), statement.span)?;
                    None
                }
                r::StatementKind::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let condition = self.expression(*condition, &exit, 0)?;
                    let then = self.block()?;
                    let otherwise = self.block()?;
                    self.seal(
                        exit.block,
                        m::Term::Branch {
                            condition,
                            then_target: target(then),
                            else_target: target(otherwise),
                        },
                        statement.span,
                    )?;
                    let then_env = self.environment(&exit.environment)?;
                    let else_env = self.environment(&exit.environment)?;
                    let a = self.body(
                        *then_block,
                        Exit {
                            block: then,
                            environment: then_env,
                        },
                        depth + 1,
                    )?;
                    let b = if let Some(block) = else_block {
                        self.body(
                            *block,
                            Exit {
                                block: otherwise,
                                environment: else_env,
                            },
                            depth + 1,
                        )?
                    } else {
                        Some(Exit {
                            block: otherwise,
                            environment: else_env,
                        })
                    };
                    let mut continuing = self.context.budget.vector(2)?;
                    continuing.extend(a);
                    continuing.extend(b);
                    self.join(continuing, exit.environment, statement.span)?
                }
                r::StatementKind::While { condition, body } => {
                    Some(self.while_loop(*condition, *body, exit, statement.span, depth + 1)?)
                }
                r::StatementKind::Match { scrutinee, arms } => {
                    let value = self.expression(*scrutinee, &exit, 0)?;
                    let mut reachable = self.context.budget.vector(arms.len())?;
                    for arm in arms {
                        self.context.budget.charge(1)?;
                        if !self.facts.arm_is_unreachable(arm.pattern) {
                            reachable.push(arm);
                        }
                    }
                    let mut continuing = self.context.budget.vector(reachable.len())?;
                    let mut current = exit.block;
                    for (index, arm) in reachable.iter().enumerate() {
                        let mut selected = Exit {
                            block: current,
                            environment: self.environment(&exit.environment)?,
                        };
                        let last = index + 1 == reachable.len();
                        let failure = if last { None } else { Some(self.block()?) };
                        self.pattern(arm.pattern, value, &mut selected, failure, 0)?;
                        if let Some(failure) = failure {
                            // Keep the selected body and continuation as explicit
                            // blocks. Coverage-pruned arms are omitted above.
                            let yes = self.emit(
                                selected.block,
                                m::Type::Bool,
                                arm.span,
                                |destination| m::Op::BoolConstant {
                                    destination,
                                    value: true,
                                },
                            )?;
                            let body = self.block()?;
                            self.seal(
                                selected.block,
                                m::Term::Branch {
                                    condition: yes,
                                    then_target: target(body),
                                    else_target: target(failure),
                                },
                                arm.span,
                            )?;
                            selected.block = body;
                            current = failure;
                        }
                        if let Some(e) = self.body(arm.body, selected, depth + 1)? {
                            continuing.push(e);
                        }
                    }
                    self.join(continuing, exit.environment, statement.span)?
                }
            };
        }
        Ok(current)
    }
    fn join(
        &mut self,
        mut exits: Vec<Exit>,
        mut baseline: Environment,
        span: Span,
    ) -> Result<Option<Exit>> {
        if exits.is_empty() {
            return Ok(None);
        }
        if exits.len() == 1 {
            let mut e = exits.pop().unwrap();
            for (i, v) in baseline.iter().enumerate() {
                self.context.budget.charge(1)?;
                if v.is_none() {
                    e.environment[i] = None;
                }
            }
            return Ok(Some(e));
        }
        let block = self.block()?;
        let mut changed = self.context.budget.vector(baseline.len())?;
        for (i, value) in baseline.iter_mut().enumerate() {
            self.context.budget.charge(exits.len())?;
            if value.is_none() {
                continue;
            }
            let first = exits[0].environment[i].ok_or(LowerError::UninitializedLocal(i))?;
            if exits.iter().all(|e| e.environment[i] == Some(first)) {
                *value = Some(first);
            } else {
                let parameter = self.register(ty(self.context.input, self.facts.locals()[i])?)?;
                push(
                    &mut self.context.budget,
                    &mut self.blocks[block.0 as usize].parameters,
                    parameter,
                )?;
                changed.push(i);
                *value = Some(parameter);
            }
        }
        for e in exits {
            let mut arguments = self.context.budget.vector(changed.len())?;
            for i in &changed {
                arguments.push(e.environment[*i].ok_or(LowerError::UninitializedLocal(*i))?);
            }
            self.seal(e.block, m::Term::Jump(m::Target { block, arguments }), span)?;
        }
        Ok(Some(Exit {
            block,
            environment: baseline,
        }))
    }
    fn while_loop(
        &mut self,
        condition: r::ExpressionId,
        body: r::BlockId,
        exit: Exit,
        span: Span,
        depth: usize,
    ) -> Result<Exit> {
        self.depth(depth)?;
        let header = self.block()?;
        let mut environment = self.environment(&exit.environment)?;
        let mut carried = self.context.budget.vector(environment.len())?;
        let mut arguments = self.context.budget.vector(environment.len())?;
        for (i, value) in environment.iter_mut().enumerate() {
            self.context.budget.charge(1)?;
            if self.function.locals[i].kind != r::BindingKind::Mutable {
                continue;
            }
            let Some(initial) = *value else {
                continue;
            };
            let parameter = self.register(ty(self.context.input, self.facts.locals()[i])?)?;
            push(
                &mut self.context.budget,
                &mut self.blocks[header.0 as usize].parameters,
                parameter,
            )?;
            carried.push(i);
            arguments.push(initial);
            *value = Some(parameter);
        }
        self.seal(
            exit.block,
            m::Term::Jump(m::Target {
                block: header,
                arguments,
            }),
            span,
        )?;
        let header_exit = Exit {
            block: header,
            environment,
        };
        let condition = self.expression(condition, &header_exit, 0)?;
        let body_block = self.block()?;
        let after = self.block()?;
        self.seal(
            header,
            m::Term::Branch {
                condition,
                then_target: target(body_block),
                else_target: target(after),
            },
            span,
        )?;
        let body_env = self.environment(&header_exit.environment)?;
        if let Some(e) = self.body(
            body,
            Exit {
                block: body_block,
                environment: body_env,
            },
            depth + 1,
        )? {
            let mut arguments = self.context.budget.vector(carried.len())?;
            for i in carried {
                arguments.push(e.environment[i].ok_or(LowerError::UninitializedLocal(i))?);
            }
            self.seal(
                e.block,
                m::Term::Jump(m::Target {
                    block: header,
                    arguments,
                }),
                self.function.blocks[body.0].closing_brace,
            )?;
        }
        Ok(Exit {
            block: after,
            environment: header_exit.environment,
        })
    }
    fn guard(
        &mut self,
        e: &mut Exit,
        condition: m::Register,
        failure: m::BlockId,
        span: Span,
    ) -> Result<()> {
        let pass = self.block()?;
        self.seal(
            e.block,
            m::Term::Branch {
                condition,
                then_target: target(pass),
                else_target: target(failure),
            },
            span,
        )?;
        e.block = pass;
        Ok(())
    }
    fn pattern(
        &mut self,
        pattern: r::PatternId,
        value: m::Register,
        e: &mut Exit,
        failure: Option<m::BlockId>,
        depth: usize,
    ) -> Result<()> {
        self.depth(depth)?;
        let pattern_source = &self.function.patterns[pattern.0];
        let span = pattern_source.span;
        match &pattern_source.kind {
            r::PatternKind::CoreUnit | r::PatternKind::CoreCase { .. } => {
                return Err(LowerError::UnsupportedOwnershipBody {
                    span: pattern_source.span,
                });
            }
            r::PatternKind::Wildcard => {}
            r::PatternKind::Binding(local) => e.environment[local.0] = Some(value),
            r::PatternKind::Integer(integer) => {
                if let Some(failure) = failure {
                    let constant = self.emit(e.block, m::Type::I64, span, |destination| {
                        m::Op::I64Constant {
                            destination,
                            value: *integer,
                        }
                    })?;
                    let test = self.emit(e.block, m::Type::Bool, span, |destination| {
                        m::Op::EqualI64 {
                            destination,
                            left: value,
                            right: constant,
                        }
                    })?;
                    self.guard(e, test, failure, span)?;
                }
            }
            r::PatternKind::Boolean(boolean) => {
                if let Some(failure) = failure {
                    let pass = self.block()?;
                    let (then_target, else_target) = if *boolean {
                        (target(pass), target(failure))
                    } else {
                        (target(failure), target(pass))
                    };
                    self.seal(
                        e.block,
                        m::Term::Branch {
                            condition: value,
                            then_target,
                            else_target,
                        },
                        span,
                    )?;
                    e.block = pass;
                }
            }
            r::PatternKind::Record { .. } => {
                let fields = self.facts.pattern_fields(pattern).expect("checked pattern");
                for (index, p) in fields.iter().enumerate() {
                    let t = ty(self.context.input, self.facts.patterns()[p.0])?;
                    let field = id(index)?;
                    let child = self.emit(e.block, t, span, |destination| m::Op::Field {
                        destination,
                        receiver: value,
                        field,
                    })?;
                    self.pattern(*p, child, e, failure, depth + 1)?;
                }
            }
            r::PatternKind::Case { index, payload, .. } => {
                let case = id(*index)?;
                if let Some(failure) = failure {
                    let test =
                        self.emit(e.block, m::Type::Bool, span, |destination| m::Op::IsCase {
                            destination,
                            receiver: value,
                            case,
                        })?;
                    self.guard(e, test, failure, span)?;
                }
                for (index, p) in payload.iter().enumerate() {
                    let t = ty(self.context.input, self.facts.patterns()[p.0])?;
                    let field = id(index)?;
                    let child = self.emit(e.block, t, span, |destination| m::Op::EnumField {
                        destination,
                        receiver: value,
                        case,
                        field,
                    })?;
                    self.pattern(*p, child, e, failure, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
fn target(block: m::BlockId) -> m::Target {
    m::Target {
        block,
        arguments: Vec::new(),
    }
}
