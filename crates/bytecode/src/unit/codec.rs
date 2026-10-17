use std::cmp::Ordering;
use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_mir::{ResourceTypeId, unit as m};

use super::Program;

const MAGIC: &[u8; 8] = b"LANGBC\0\0";
const HEADER: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodecLimits {
    pub bytes: usize,
    pub validation: m::Limits,
}
impl Default for CodecLimits {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024,
            validation: m::Limits::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Table {
    Sources,
    Definitions,
    Imports,
    Functions,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tag {
    Definition,
    Type,
    Instruction,
    Terminator,
    Span,
    Entry,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodecErrorKind {
    TooLarge { limit: usize, requested: usize },
    Truncated,
    BadMagic,
    Version(u16),
    Flags(u16),
    LengthMismatch,
    TrailingBytes,
    InvalidUtf8,
    InvalidBoolean(u8),
    InvalidTag { tag: Tag, value: u8 },
    NonCanonicalOrder(Table),
    IntegerOverflow,
    Budget(m::Error),
    Validation(m::Error),
    AllocationFailed,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodecError {
    pub offset: usize,
    pub kind: CodecErrorKind,
}
impl Display for CodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "bytecode error at byte {}: {:?}", self.offset, self.kind)
    }
}
impl Error for CodecError {}
fn error(offset: usize, kind: CodecErrorKind) -> CodecError {
    CodecError { offset, kind }
}

struct Reader<'a> {
    input: &'a [u8],
    offset: usize,
    limits: CodecLimits,
    budget: m::Budget,
}
impl<'a> Reader<'a> {
    fn charge(&mut self, count: usize) -> Result<(), CodecError> {
        self.budget
            .charge(count)
            .map_err(|e| error(self.offset, CodecErrorKind::Budget(e)))
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], CodecError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| error(self.offset, CodecErrorKind::IntegerOverflow))?;
        let bytes = self
            .input
            .get(self.offset..end)
            .ok_or_else(|| error(self.offset, CodecErrorKind::Truncated))?;
        self.offset = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, CodecError> {
        self.charge(1)?;
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, CodecError> {
        self.charge(1)?;
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("checked size"),
        ))
    }
    fn i64(&mut self) -> Result<i64, CodecError> {
        self.charge(1)?;
        Ok(i64::from_le_bytes(
            self.take(8)?.try_into().expect("checked size"),
        ))
    }
    fn count(
        &mut self,
        maximum: usize,
        minimum_bytes: usize,
        kind: m::LimitKind,
    ) -> Result<usize, CodecError> {
        let count = self.u32()? as usize;
        self.cap(count, maximum, kind)?;
        let minimum = count
            .checked_mul(minimum_bytes)
            .ok_or_else(|| error(self.offset, CodecErrorKind::IntegerOverflow))?;
        if minimum > self.input.len() - self.offset {
            return Err(error(self.offset, CodecErrorKind::Truncated));
        }
        Ok(count)
    }
    fn cap(&self, count: usize, limit: usize, kind: m::LimitKind) -> Result<(), CodecError> {
        if count > limit {
            Err(error(
                self.offset,
                CodecErrorKind::Budget(m::Error::new(m::ErrorKind::Limit {
                    kind,
                    limit,
                    requested: count,
                })),
            ))
        } else {
            Ok(())
        }
    }
    fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, CodecError> {
        self.budget
            .vector(count)
            .map_err(|e| error(self.offset, CodecErrorKind::Budget(e)))
    }
    fn string(&mut self, maximum: usize, kind: m::LimitKind) -> Result<String, CodecError> {
        let length = self.u32()? as usize;
        self.cap(length, maximum, kind)?;
        self.raw_string(length)
    }
    fn source_text(&mut self, total: &mut usize) -> Result<String, CodecError> {
        let length = self.u32()? as usize;
        self.cap(
            length,
            self.limits.validation.source_bytes,
            m::LimitKind::SourceBytes,
        )?;
        let next = total
            .checked_add(length)
            .ok_or_else(|| error(self.offset, CodecErrorKind::IntegerOverflow))?;
        self.cap(
            next,
            self.limits.validation.total_source_bytes,
            m::LimitKind::TotalSourceBytes,
        )?;
        let text = self.raw_string(length)?;
        *total = next;
        Ok(text)
    }
    fn raw_string(&mut self, length: usize) -> Result<String, CodecError> {
        let start = self.offset;
        let bytes = self.take(length)?;
        self.charge(length / 1024 + 1)?;
        let text = std::str::from_utf8(bytes)
            .map_err(|e| error(start + e.valid_up_to(), CodecErrorKind::InvalidUtf8))?;
        self.budget
            .bytes(length)
            .map_err(|e| error(start, CodecErrorKind::Budget(e)))?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(length)
            .map_err(|_| error(start, CodecErrorKind::AllocationFailed))?;
        owned.push_str(text);
        Ok(owned)
    }
    fn name(&mut self) -> Result<String, CodecError> {
        self.string(self.limits.bytes, m::LimitKind::FormatCapacity)
    }
    fn ty(&mut self) -> Result<m::Type, CodecError> {
        let offset = self.offset;
        Ok(match self.byte()? {
            0 => m::Type::I64,
            1 => m::Type::Bool,
            2 => m::Type::Own(ResourceTypeId::new(self.u32()?)),
            3 => m::Type::Record(m::DefinitionId(self.u32()?)),
            4 => m::Type::Enum(m::DefinitionId(self.u32()?)),
            value => {
                return Err(error(
                    offset,
                    CodecErrorKind::InvalidTag {
                        tag: Tag::Type,
                        value,
                    },
                ));
            }
        })
    }
    fn types(&mut self, maximum: usize, kind: m::LimitKind) -> Result<Vec<m::Type>, CodecError> {
        let count = self.count(maximum, 1, kind)?;
        let mut types = self.vector(count)?;
        for _ in 0..count {
            types.push(self.ty()?);
        }
        Ok(types)
    }
    fn signature(&mut self) -> Result<m::Signature, CodecError> {
        Ok(m::Signature {
            parameters: self.types(self.limits.validation.fields, m::LimitKind::Fields)?,
            result: self.ty()?,
        })
    }
    fn span(&mut self) -> Result<Option<m::Span>, CodecError> {
        let offset = self.offset;
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(m::Span {
                source: m::SourceId(self.u32()?),
                start: self.u32()?,
                end: self.u32()?,
            })),
            value => Err(error(
                offset,
                CodecErrorKind::InvalidTag {
                    tag: Tag::Span,
                    value,
                },
            )),
        }
    }
    fn registers(&mut self, maximum: usize) -> Result<Vec<m::Register>, CodecError> {
        let count = self.count(maximum, 4, m::LimitKind::Registers)?;
        let mut result = self.vector(count)?;
        for _ in 0..count {
            result.push(m::Register(self.u32()?));
        }
        Ok(result)
    }
    fn target(&mut self) -> Result<m::Target, CodecError> {
        Ok(m::Target {
            block: m::BlockId(self.u32()?),
            arguments: self.registers(self.limits.validation.registers)?,
        })
    }
    fn instruction(&mut self) -> Result<m::Instruction, CodecError> {
        let offset = self.offset;
        let tag = self.byte()?;
        if tag > 11 {
            return Err(error(
                offset,
                CodecErrorKind::InvalidTag {
                    tag: Tag::Instruction,
                    value: tag,
                },
            ));
        }
        let destination = m::Register(self.u32()?);
        let op = match tag {
            0 => m::Op::I64Constant {
                destination,
                value: self.i64()?,
            },
            1 => m::Op::Add {
                destination,
                left: m::Register(self.u32()?),
                right: m::Register(self.u32()?),
            },
            2 => m::Op::LessThan {
                destination,
                left: m::Register(self.u32()?),
                right: m::Register(self.u32()?),
            },
            3 => m::Op::HostCall {
                destination,
                import: m::ImportId(self.u32()?),
                argument: m::Register(self.u32()?),
            },
            4 => {
                let at = self.offset;
                let value = match self.byte()? {
                    0 => false,
                    1 => true,
                    value => return Err(error(at, CodecErrorKind::InvalidBoolean(value))),
                };
                m::Op::BoolConstant { destination, value }
            }
            5 => m::Op::Call {
                destination,
                function: m::FunctionId(self.u32()?),
                arguments: self.registers(self.limits.validation.fields)?,
            },
            6 => m::Op::Record {
                destination,
                definition: m::DefinitionId(self.u32()?),
                fields: self.registers(self.limits.validation.fields)?,
            },
            7 => m::Op::Field {
                destination,
                receiver: m::Register(self.u32()?),
                field: self.u32()?,
            },
            8 => m::Op::Enum {
                destination,
                definition: m::DefinitionId(self.u32()?),
                case: self.u32()?,
                payload: self.registers(self.limits.validation.fields)?,
            },
            9 => m::Op::IsCase {
                destination,
                receiver: m::Register(self.u32()?),
                case: self.u32()?,
            },
            10 => m::Op::EnumField {
                destination,
                receiver: m::Register(self.u32()?),
                case: self.u32()?,
                field: self.u32()?,
            },
            11 => m::Op::EqualI64 {
                destination,
                left: m::Register(self.u32()?),
                right: m::Register(self.u32()?),
            },
            _ => unreachable!(),
        };
        Ok(m::Instruction {
            op,
            span: self.span()?,
        })
    }
    fn terminator(&mut self) -> Result<m::Terminator, CodecError> {
        let offset = self.offset;
        let kind = match self.byte()? {
            0 => m::Term::Return(m::Register(self.u32()?)),
            1 => m::Term::Jump(self.target()?),
            2 => m::Term::Branch {
                condition: m::Register(self.u32()?),
                then_target: self.target()?,
                else_target: self.target()?,
            },
            value => {
                return Err(error(
                    offset,
                    CodecErrorKind::InvalidTag {
                        tag: Tag::Terminator,
                        value,
                    },
                ));
            }
        };
        Ok(m::Terminator {
            kind,
            span: self.span()?,
        })
    }
    fn canonical(
        &mut self,
        previous: Option<&str>,
        name: &str,
        table: Table,
    ) -> Result<(), CodecError> {
        if let Some(previous) = previous
            && compare(previous, name, &mut self.budget)
                .map_err(|e| error(self.offset, CodecErrorKind::Budget(e)))?
                != Ordering::Less
        {
            return Err(error(self.offset, CodecErrorKind::NonCanonicalOrder(table)));
        }
        Ok(())
    }
    fn unit(&mut self) -> Result<m::Unit, CodecError> {
        let limits = self.limits.validation;
        let count = self.count(limits.sources, 8, m::LimitKind::Sources)?;
        let mut sources: Vec<m::Source> = self.vector(count)?;
        let mut total = 0_usize;
        for _ in 0..count {
            let path = self.name()?;
            self.canonical(
                sources.last().map(|s| s.path.as_str()),
                &path,
                Table::Sources,
            )?;
            let text = self.source_text(&mut total)?;
            sources.push(m::Source { path, text });
        }
        let count = self.count(limits.definitions, 9, m::LimitKind::Definitions)?;
        let mut definitions: Vec<m::Definition> = self.vector(count)?;
        for _ in 0..count {
            let name = self.name()?;
            self.canonical(
                definitions.last().map(|d| d.name.as_str()),
                &name,
                Table::Definitions,
            )?;
            let offset = self.offset;
            let kind = match self.byte()? {
                0 => {
                    let count = self.count(limits.fields, 5, m::LimitKind::Fields)?;
                    let mut fields = self.vector(count)?;
                    for _ in 0..count {
                        fields.push(m::Field {
                            name: self.name()?,
                            value_type: self.ty()?,
                        });
                    }
                    m::DefinitionKind::Record(fields)
                }
                1 => {
                    let count = self.count(limits.fields, 8, m::LimitKind::Fields)?;
                    let mut cases = self.vector(count)?;
                    for _ in 0..count {
                        cases.push(m::Case {
                            name: self.name()?,
                            payload: self.types(limits.fields, m::LimitKind::Fields)?,
                        });
                    }
                    m::DefinitionKind::Enum(cases)
                }
                value => {
                    return Err(error(
                        offset,
                        CodecErrorKind::InvalidTag {
                            tag: Tag::Definition,
                            value,
                        },
                    ));
                }
            };
            definitions.push(m::Definition { name, kind });
        }
        let count = self.count(limits.imports, 9, m::LimitKind::Imports)?;
        let mut imports: Vec<m::Import> = self.vector(count)?;
        for _ in 0..count {
            let name = self.name()?;
            self.canonical(
                imports.last().map(|i| i.name.as_str()),
                &name,
                Table::Imports,
            )?;
            imports.push(m::Import {
                name,
                signature: self.signature()?,
            });
        }
        let count = self.count(limits.functions, 18, m::LimitKind::Functions)?;
        let mut functions: Vec<m::Function> = self.vector(count)?;
        let mut registers = 0_usize;
        let mut blocks = 0_usize;
        let mut instructions = 0_usize;
        for _ in 0..count {
            let name = self.name()?;
            self.canonical(
                functions.last().map(|f| f.name.as_str()),
                &name,
                Table::Functions,
            )?;
            let signature = self.signature()?;
            let register_types = self.types(
                limits.registers.saturating_sub(registers),
                m::LimitKind::Registers,
            )?;
            registers += register_types.len();
            let span = self.span()?;
            let count = self.count(
                limits.blocks.saturating_sub(blocks),
                14,
                m::LimitKind::Blocks,
            )?;
            blocks += count;
            let mut body = self.vector(count)?;
            for _ in 0..count {
                let parameters = self.registers(limits.registers)?;
                let count = self.count(
                    limits.instructions.saturating_sub(instructions),
                    7,
                    m::LimitKind::Instructions,
                )?;
                instructions += count;
                let mut ops = self.vector(count)?;
                for _ in 0..count {
                    ops.push(self.instruction()?);
                }
                body.push(m::Block {
                    parameters,
                    instructions: ops,
                    terminator: self.terminator()?,
                });
            }
            functions.push(m::Function {
                name,
                signature,
                registers: register_types,
                span,
                blocks: body,
            });
        }
        let offset = self.offset;
        let entry = match self.byte()? {
            0 => None,
            1 => Some(m::FunctionId(self.u32()?)),
            value => {
                return Err(error(
                    offset,
                    CodecErrorKind::InvalidTag {
                        tag: Tag::Entry,
                        value,
                    },
                ));
            }
        };
        Ok(m::Unit {
            sources,
            definitions,
            imports,
            functions,
            entry,
        })
    }
}

pub fn decode(bytes: &[u8], limits: CodecLimits) -> Result<Program, CodecError> {
    if bytes.len() > limits.bytes {
        return Err(error(
            0,
            CodecErrorKind::TooLarge {
                limit: limits.bytes,
                requested: bytes.len(),
            },
        ));
    }
    if bytes.len() < HEADER {
        return Err(error(bytes.len(), CodecErrorKind::Truncated));
    }
    if &bytes[..8] != MAGIC {
        return Err(error(0, CodecErrorKind::BadMagic));
    }
    let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    if version != 1 {
        return Err(error(8, CodecErrorKind::Version(version)));
    }
    let flags = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
    if flags != 0 {
        return Err(error(10, CodecErrorKind::Flags(flags)));
    }
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    if length != bytes.len() - HEADER {
        return Err(error(12, CodecErrorKind::LengthMismatch));
    }
    let mut reader = Reader {
        input: bytes,
        offset: HEADER,
        limits,
        budget: m::Budget::new(limits.validation),
    };
    let unit = reader.unit()?;
    if reader.offset != bytes.len() {
        return Err(error(reader.offset, CodecErrorKind::TrailingBytes));
    }
    let verified = unit
        .verify_with_budget(&mut reader.budget)
        .map_err(|e| error(reader.offset, CodecErrorKind::Validation(e)))?;
    Ok(Program::from_verified(verified))
}

fn compare(left: &str, right: &str, budget: &mut m::Budget) -> Result<Ordering, m::Error> {
    for (a, b) in left.bytes().zip(right.bytes()) {
        budget.charge(1)?;
        if a != b {
            return Ok(a.cmp(&b));
        }
    }
    budget.charge(1)?;
    Ok(left.len().cmp(&right.len()))
}
struct Order {
    indices: Vec<usize>,
    inverse: Vec<u32>,
}
fn sift(
    indices: &mut [usize],
    names: &[&str],
    mut root: usize,
    end: usize,
    budget: &mut m::Budget,
) -> Result<(), m::Error> {
    while root * 2 + 1 < end {
        let mut child = root * 2 + 1;
        if child + 1 < end
            && compare(names[indices[child]], names[indices[child + 1]], budget)? == Ordering::Less
        {
            child += 1;
        }
        if compare(names[indices[root]], names[indices[child]], budget)? != Ordering::Less {
            break;
        }
        indices.swap(root, child);
        root = child;
    }
    Ok(())
}
fn order<'a>(
    names: impl Iterator<Item = &'a str>,
    count: usize,
    budget: &mut m::Budget,
) -> Result<Order, m::Error> {
    let mut table = budget.vector(count)?;
    let mut indices = budget.vector(count)?;
    let mut inverse = budget.vector(count)?;
    for (index, name) in names.enumerate() {
        budget.charge(1)?;
        table.push(name);
        indices.push(index);
        inverse.push(0);
    }
    for root in (0..count / 2).rev() {
        sift(&mut indices, &table, root, count, budget)?;
    }
    for end in (1..count).rev() {
        indices.swap(0, end);
        sift(&mut indices, &table, 0, end, budget)?;
    }
    for (new, old) in indices.iter().enumerate() {
        budget.charge(1)?;
        inverse[*old] = new as u32;
    }
    Ok(Order { indices, inverse })
}
struct Orders {
    sources: Order,
    definitions: Order,
    imports: Order,
    functions: Order,
}

struct Writer<'a> {
    output: Option<Vec<u8>>,
    length: usize,
    maximum: usize,
    budget: &'a mut m::Budget,
}
impl Writer<'_> {
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), CodecError> {
        self.budget
            .charge(1)
            .map_err(|e| error(self.length, CodecErrorKind::Budget(e)))?;
        let end = self
            .length
            .checked_add(bytes.len())
            .ok_or_else(|| error(self.length, CodecErrorKind::IntegerOverflow))?;
        if end > self.maximum {
            return Err(error(
                self.length,
                CodecErrorKind::TooLarge {
                    limit: self.maximum,
                    requested: end,
                },
            ));
        }
        if let Some(output) = &mut self.output {
            output.extend_from_slice(bytes);
        }
        self.length = end;
        Ok(())
    }
    fn byte(&mut self, value: u8) -> Result<(), CodecError> {
        self.bytes(&[value])
    }
    fn u32(&mut self, value: u32) -> Result<(), CodecError> {
        self.bytes(&value.to_le_bytes())
    }
    fn count(&mut self, count: usize) -> Result<(), CodecError> {
        self.u32(
            u32::try_from(count)
                .map_err(|_| error(self.length, CodecErrorKind::IntegerOverflow))?,
        )
    }
    fn string(&mut self, value: &str) -> Result<(), CodecError> {
        self.count(value.len())?;
        self.bytes(value.as_bytes())
    }
    fn ty(&mut self, ty: m::Type, orders: &Orders) -> Result<(), CodecError> {
        match ty {
            m::Type::I64 => self.byte(0),
            m::Type::Bool => self.byte(1),
            m::Type::Own(id) => {
                self.byte(2)?;
                self.u32(id.as_u32())
            }
            m::Type::Record(id) => {
                self.byte(3)?;
                self.u32(orders.definitions.inverse[id.0 as usize])
            }
            m::Type::Enum(id) => {
                self.byte(4)?;
                self.u32(orders.definitions.inverse[id.0 as usize])
            }
        }
    }
    fn types(&mut self, types: &[m::Type], orders: &Orders) -> Result<(), CodecError> {
        self.count(types.len())?;
        for ty in types {
            self.ty(*ty, orders)?;
        }
        Ok(())
    }
    fn signature(&mut self, signature: &m::Signature, orders: &Orders) -> Result<(), CodecError> {
        self.types(&signature.parameters, orders)?;
        self.ty(signature.result, orders)
    }
    fn span(&mut self, span: Option<m::Span>, orders: &Orders) -> Result<(), CodecError> {
        if let Some(span) = span {
            self.byte(1)?;
            self.u32(orders.sources.inverse[span.source.0 as usize])?;
            self.u32(span.start)?;
            self.u32(span.end)
        } else {
            self.byte(0)
        }
    }
    fn registers(&mut self, registers: &[m::Register]) -> Result<(), CodecError> {
        self.count(registers.len())?;
        for register in registers {
            self.u32(register.0)?;
        }
        Ok(())
    }
    fn target(&mut self, target: &m::Target) -> Result<(), CodecError> {
        self.u32(target.block.0)?;
        self.registers(&target.arguments)
    }
    fn instruction(
        &mut self,
        instruction: &m::Instruction,
        orders: &Orders,
    ) -> Result<(), CodecError> {
        let op = &instruction.op;
        let tag = match op {
            m::Op::I64Constant { .. } => 0,
            m::Op::Add { .. } => 1,
            m::Op::LessThan { .. } => 2,
            m::Op::HostCall { .. } => 3,
            m::Op::BoolConstant { .. } => 4,
            m::Op::Call { .. } => 5,
            m::Op::Record { .. } => 6,
            m::Op::Field { .. } => 7,
            m::Op::Enum { .. } => 8,
            m::Op::IsCase { .. } => 9,
            m::Op::EnumField { .. } => 10,
            m::Op::EqualI64 { .. } => 11,
        };
        self.byte(tag)?;
        self.u32(op.destination().0)?;
        match op {
            m::Op::I64Constant { value, .. } => self.bytes(&value.to_le_bytes())?,
            m::Op::BoolConstant { value, .. } => self.byte(u8::from(*value))?,
            m::Op::Add { left, right, .. }
            | m::Op::LessThan { left, right, .. }
            | m::Op::EqualI64 { left, right, .. } => {
                self.u32(left.0)?;
                self.u32(right.0)?;
            }
            m::Op::HostCall {
                import, argument, ..
            } => {
                self.u32(orders.imports.inverse[import.0 as usize])?;
                self.u32(argument.0)?;
            }
            m::Op::Call {
                function,
                arguments,
                ..
            } => {
                self.u32(orders.functions.inverse[function.0 as usize])?;
                self.registers(arguments)?;
            }
            m::Op::Record {
                definition, fields, ..
            } => {
                self.u32(orders.definitions.inverse[definition.0 as usize])?;
                self.registers(fields)?;
            }
            m::Op::Field {
                receiver, field, ..
            } => {
                self.u32(receiver.0)?;
                self.u32(*field)?;
            }
            m::Op::Enum {
                definition,
                case,
                payload,
                ..
            } => {
                self.u32(orders.definitions.inverse[definition.0 as usize])?;
                self.u32(*case)?;
                self.registers(payload)?;
            }
            m::Op::IsCase { receiver, case, .. } => {
                self.u32(receiver.0)?;
                self.u32(*case)?;
            }
            m::Op::EnumField {
                receiver,
                case,
                field,
                ..
            } => {
                self.u32(receiver.0)?;
                self.u32(*case)?;
                self.u32(*field)?;
            }
        }
        self.span(instruction.span, orders)
    }
    fn terminator(
        &mut self,
        terminator: &m::Terminator,
        orders: &Orders,
    ) -> Result<(), CodecError> {
        match &terminator.kind {
            m::Term::Return(value) => {
                self.byte(0)?;
                self.u32(value.0)?;
            }
            m::Term::Jump(target) => {
                self.byte(1)?;
                self.target(target)?;
            }
            m::Term::Branch {
                condition,
                then_target,
                else_target,
            } => {
                self.byte(2)?;
                self.u32(condition.0)?;
                self.target(then_target)?;
                self.target(else_target)?;
            }
        }
        self.span(terminator.span, orders)
    }
    fn unit(&mut self, unit: &m::Unit, orders: &Orders, payload: u32) -> Result<(), CodecError> {
        self.bytes(MAGIC)?;
        self.bytes(&1_u16.to_le_bytes())?;
        self.bytes(&0_u16.to_le_bytes())?;
        self.u32(payload)?;
        self.count(unit.sources.len())?;
        for index in &orders.sources.indices {
            let source = &unit.sources[*index];
            self.string(&source.path)?;
            self.string(&source.text)?;
        }
        self.count(unit.definitions.len())?;
        for index in &orders.definitions.indices {
            let definition = &unit.definitions[*index];
            self.string(&definition.name)?;
            match &definition.kind {
                m::DefinitionKind::Record(fields) => {
                    self.byte(0)?;
                    self.count(fields.len())?;
                    for field in fields {
                        self.string(&field.name)?;
                        self.ty(field.value_type, orders)?;
                    }
                }
                m::DefinitionKind::Enum(cases) => {
                    self.byte(1)?;
                    self.count(cases.len())?;
                    for case in cases {
                        self.string(&case.name)?;
                        self.types(&case.payload, orders)?;
                    }
                }
            }
        }
        self.count(unit.imports.len())?;
        for index in &orders.imports.indices {
            let import = &unit.imports[*index];
            self.string(&import.name)?;
            self.signature(&import.signature, orders)?;
        }
        self.count(unit.functions.len())?;
        for index in &orders.functions.indices {
            let function = &unit.functions[*index];
            self.string(&function.name)?;
            self.signature(&function.signature, orders)?;
            self.types(&function.registers, orders)?;
            self.span(function.span, orders)?;
            self.count(function.blocks.len())?;
            for block in &function.blocks {
                self.registers(&block.parameters)?;
                self.count(block.instructions.len())?;
                for instruction in &block.instructions {
                    self.instruction(instruction, orders)?;
                }
                self.terminator(&block.terminator, orders)?;
            }
        }
        if let Some(entry) = unit.entry {
            self.byte(1)?;
            self.u32(orders.functions.inverse[entry.0 as usize])?;
        } else {
            self.byte(0)?;
        }
        Ok(())
    }
}

pub fn encode(program: &Program, limits: CodecLimits) -> Result<Vec<u8>, CodecError> {
    let mut budget = m::Budget::new(limits.validation);
    program
        .verified()
        .revalidate(&mut budget)
        .map_err(|e| error(0, CodecErrorKind::Validation(e)))?;
    let unit = program.verified().unit();
    let orders = Orders {
        sources: order(
            unit.sources.iter().map(|s| s.path.as_str()),
            unit.sources.len(),
            &mut budget,
        )
        .map_err(|e| error(0, CodecErrorKind::Budget(e)))?,
        definitions: order(
            unit.definitions.iter().map(|d| d.name.as_str()),
            unit.definitions.len(),
            &mut budget,
        )
        .map_err(|e| error(0, CodecErrorKind::Budget(e)))?,
        imports: order(
            unit.imports.iter().map(|i| i.name.as_str()),
            unit.imports.len(),
            &mut budget,
        )
        .map_err(|e| error(0, CodecErrorKind::Budget(e)))?,
        functions: order(
            unit.functions.iter().map(|f| f.name.as_str()),
            unit.functions.len(),
            &mut budget,
        )
        .map_err(|e| error(0, CodecErrorKind::Budget(e)))?,
    };
    let size = {
        let mut writer = Writer {
            output: None,
            length: 0,
            maximum: limits.bytes,
            budget: &mut budget,
        };
        writer.unit(unit, &orders, 0)?;
        writer.length
    };
    let payload = u32::try_from(
        size.checked_sub(HEADER)
            .ok_or_else(|| error(0, CodecErrorKind::IntegerOverflow))?,
    )
    .map_err(|_| error(0, CodecErrorKind::IntegerOverflow))?;
    budget
        .bytes(size)
        .map_err(|e| error(0, CodecErrorKind::Budget(e)))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(size)
        .map_err(|_| error(0, CodecErrorKind::AllocationFailed))?;
    let mut writer = Writer {
        output: Some(output),
        length: 0,
        maximum: size,
        budget: &mut budget,
    };
    writer.unit(unit, &orders, payload)?;
    debug_assert_eq!(writer.length, size);
    Ok(writer.output.expect("output writer"))
}
