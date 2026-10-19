use super::*;
use o::{bodies as b, declarations as d, places as p};

struct Reader<'a> {
    version: u16,
    seen_panic: bool,
    input: &'a [u8],
    offset: usize,
    limits: CodecLimits,
    budget: m::Budget,
    totals: [usize; 6],
    source_bytes: usize,
}
impl Reader<'_> {
    fn fail(&self, kind: CodecErrorKind) -> CodecError {
        error(self.offset, kind)
    }
    fn charge(&mut self, n: usize) -> Result<(), CodecError> {
        self.budget
            .charge(n)
            .map_err(|e| self.fail(CodecErrorKind::Budget(e)))
    }
    fn take(&mut self, n: usize) -> Result<&[u8], CodecError> {
        let end = self
            .offset
            .checked_add(n)
            .ok_or_else(|| self.fail(CodecErrorKind::IntegerOverflow))?;
        let bytes = self
            .input
            .get(self.offset..end)
            .ok_or_else(|| self.fail(CodecErrorKind::Truncated))?;
        self.offset = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, CodecError> {
        self.charge(1)?;
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u32, CodecError> {
        self.charge(1)?;
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn integer(&mut self) -> Result<i64, CodecError> {
        self.charge(1)?;
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn boolean(&mut self) -> Result<bool, CodecError> {
        let offset = self.offset;
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(error(offset, CodecErrorKind::InvalidBoolean(v))),
        }
    }
    fn cap(&mut self, table: Table, requested: usize, limit: usize) -> Result<(), CodecError> {
        let slot = match table {
            Table::Values => Some(0),
            Table::Places => Some(1),
            Table::Blocks => Some(2),
            Table::Instructions => Some(3),
            Table::Loans => Some(4),
            Table::Prepared => Some(5),
            _ => None,
        };
        let next = if let Some(i) = slot {
            self.totals[i]
                .checked_add(requested)
                .ok_or_else(|| self.fail(CodecErrorKind::IntegerOverflow))?
        } else {
            requested
        };
        if next > limit {
            return Err(self.fail(CodecErrorKind::Limit {
                table,
                limit,
                requested: next,
            }));
        }
        if let Some(i) = slot {
            self.totals[i] = next;
        }
        Ok(())
    }
    fn count(&mut self, table: Table, limit: usize, min: usize) -> Result<usize, CodecError> {
        let count = usize::try_from(self.word()?)
            .map_err(|_| self.fail(CodecErrorKind::IntegerOverflow))?;
        self.cap(table, count, limit)?;
        let bytes = count
            .checked_mul(min)
            .ok_or_else(|| self.fail(CodecErrorKind::IntegerOverflow))?;
        if bytes > self.input.len() - self.offset {
            return Err(self.fail(CodecErrorKind::Truncated));
        }
        Ok(count)
    }
    fn list<T>(
        &mut self,
        table: Table,
        limit: usize,
        min: usize,
        mut item: impl FnMut(&mut Self) -> Result<T, CodecError>,
    ) -> Result<Vec<T>, CodecError> {
        let count = self.count(table, limit, min)?;
        let mut values = self
            .budget
            .vector(count)
            .map_err(|e| self.fail(CodecErrorKind::Budget(e)))?;
        for _ in 0..count {
            self.charge(1)?;
            values.push(item(self)?);
        }
        Ok(values)
    }
    fn string(&mut self, maximum: usize) -> Result<String, CodecError> {
        let length = usize::try_from(self.word()?)
            .map_err(|_| self.fail(CodecErrorKind::IntegerOverflow))?;
        if length > maximum {
            return Err(self.fail(CodecErrorKind::TooLarge {
                limit: maximum,
                requested: length,
            }));
        }
        let start = self.offset;
        self.charge(length / 1024 + 1)?;
        self.budget
            .bytes(length)
            .map_err(|e| self.fail(CodecErrorKind::Budget(e)))?;
        let text = std::str::from_utf8(self.take(length)?)
            .map_err(|e| error(start + e.valid_up_to(), CodecErrorKind::InvalidUtf8))?;
        let mut result = String::new();
        result
            .try_reserve_exact(length)
            .map_err(|_| error(start, CodecErrorKind::AllocationFailed))?;
        result.push_str(text);
        Ok(result)
    }
    fn name(&mut self) -> Result<String, CodecError> {
        self.string(self.limits.bytes)
    }
    fn span(&mut self) -> Result<m::Span, CodecError> {
        Ok(m::Span {
            source: m::SourceId(self.word()?),
            start: self.word()?,
            end: self.word()?,
        })
    }
    fn optional(&mut self) -> Result<Option<u32>, CodecError> {
        let offset = self.offset;
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.word()?)),
            value => Err(error(
                offset,
                CodecErrorKind::InvalidTag {
                    tag: Tag::OptionalId,
                    value,
                },
            )),
        }
    }
    fn kind<T>(&mut self, read: fn(u8, usize) -> Result<T, CodecError>) -> Result<T, CodecError> {
        let offset = self.offset;
        read(self.byte()?, offset)
    }
    fn invalid(&self, tag: Tag, value: u8, offset: usize) -> CodecError {
        error(offset, CodecErrorKind::InvalidTag { tag, value })
    }
    fn ty(&mut self) -> Result<o::Type, CodecError> {
        let offset = self.offset;
        Ok(match self.byte()? {
            0 => o::Type::I64,
            1 => o::Type::Bool,
            2 => o::Type::Nominal(m::DefinitionId(self.word()?)),
            3 => o::Type::Constructed(o::ConstructedId(self.word()?)),
            v => return Err(self.invalid(Tag::Type, v, offset)),
        })
    }
    fn type_use(&mut self) -> Result<o::TypeUse, CodecError> {
        Ok(o::TypeUse {
            ty: self.ty()?,
            span: self.span()?,
        })
    }
    fn constructor(&mut self) -> Result<o::Constructed, CodecError> {
        use o::Constructor as C;
        let offset = self.offset;
        let constructor = match self.byte()? {
            0 => C::Builtin(self.kind(tags::read_builtin)?),
            1 => C::Option(self.ty()?),
            2 => C::Result(self.ty()?, self.ty()?),
            3 => C::Vec(self.ty()?),
            4 => C::Map(self.ty()?, self.ty()?),
            5 => C::Shared(self.ty()?),
            6 => C::Weak(self.ty()?),
            7 => C::Arena(self.ty()?),
            8 => C::Loan(self.kind(tags::read_loan)?, self.ty()?),
            9 => C::ArenaOwner(o::ArenaOwner {
                function: m::FunctionId(self.word()?),
                local: self.word()?,
            }),
            v => return Err(self.invalid(Tag::Constructor, v, offset)),
        };
        Ok(o::Constructed {
            constructor,
            span: self.span()?,
        })
    }
    fn definition(&mut self) -> Result<o::Definition, CodecError> {
        let name = self.name()?;
        let public = self.boolean()?;
        let span = self.span()?;
        let offset = self.offset;
        let fields = self.budget.limits().fields;
        let kind = match self.byte()? {
            0 => o::DefinitionKind::Record(self.list(Table::Fields, fields, 30, |s| {
                Ok(o::Field {
                    name: s.name()?,
                    span: s.span()?,
                    ty: s.type_use()?,
                })
            })?),
            1 => o::DefinitionKind::Enum(self.list(Table::Fields, fields, 20, |s| {
                Ok(o::Case {
                    name: s.name()?,
                    span: s.span()?,
                    payload: s.list(Table::Fields, fields, 13, Self::type_use)?,
                })
            })?),
            v => return Err(self.invalid(Tag::Definition, v, offset)),
        };
        Ok(o::Definition {
            name,
            public,
            span,
            kind,
        })
    }
    fn ids(&mut self, max: usize) -> Result<Vec<u32>, CodecError> {
        self.list(Table::Elements, max, 4, Self::word)
    }
    fn registers(&mut self) -> Result<Vec<m::Register>, CodecError> {
        self.list(Table::Elements, self.budget.limits().registers, 4, |s| {
            Ok(m::Register(s.word()?))
        })
    }
    fn import(&mut self) -> Result<d::Import, CodecError> {
        Ok(d::Import {
            name: self.name()?,
            abi: if self.version == 4 {
                self.kind(tags::read_import_abi)?
            } else {
                d::ImportAbi::Host
            },
            span: self.span()?,
            parameters: self.list(
                Table::Fields,
                self.budget.limits().fields,
                13,
                Self::type_use,
            )?,
            result: self.type_use()?,
        })
    }
    fn local(&mut self) -> Result<d::Local, CodecError> {
        Ok(d::Local {
            name: self.name()?,
            span: self.span()?,
            kind: self.kind(tags::read_local)?,
            ty: self.type_use()?,
        })
    }
    fn function(&mut self) -> Result<d::Function, CodecError> {
        Ok(d::Function {
            name: self.name()?,
            public: self.boolean()?,
            abi: if self.version == 4 {
                self.kind(tags::read_function_abi)?
            } else {
                d::FunctionAbi::Guest
            },
            span: self.span()?,
            signature_span: self.span()?,
            parameters: self.list(Table::Fields, self.budget.limits().fields, 4, |s| {
                Ok(d::LocalId(s.word()?))
            })?,
            result: self.type_use()?,
            locals: self.list(
                Table::Locals,
                self.budget.limits().registers,
                31,
                Self::local,
            )?,
        })
    }
    fn place(&mut self) -> Result<p::Place, CodecError> {
        let origin = m::Register(self.word()?);
        let offset = self.offset;
        let root = match self.byte()? {
            0 => p::Root::Local(d::LocalId(self.word()?)),
            1 => p::Root::Value(m::Register(self.word()?)),
            v => return Err(self.invalid(Tag::Root, v, offset)),
        };
        let projections = self.list(Table::Elements, self.budget.limits().depth, 13, |s| {
            let offset = s.offset;
            let kind = match s.byte()? {
                0 => p::ProjectionKind::Dereference,
                1 => p::ProjectionKind::Field(s.word()?),
                v => return Err(s.invalid(Tag::Projection, v, offset)),
            };
            Ok(p::Projection {
                kind,
                span: s.span()?,
            })
        })?;
        Ok(p::Place {
            origin,
            root,
            projections,
            span: self.span()?,
        })
    }
    fn cleanup(&mut self) -> Result<b::CleanupStep, CodecError> {
        let offset = self.offset;
        let tag = self.byte()?;
        let id = self.word()?;
        let target = match tag {
            0 => b::CleanupTarget::Local(d::LocalId(id)),
            1 => b::CleanupTarget::Value(m::Register(id)),
            2 => b::CleanupTarget::Loan(b::LoanId(id)),
            3 => b::CleanupTarget::Arena(d::LocalId(id)),
            v => return Err(self.invalid(Tag::CleanupTarget, v, offset)),
        };
        Ok(b::CleanupStep {
            target,
            span: self.span()?,
        })
    }
    fn instruction(&mut self) -> Result<b::Instruction, CodecError> {
        use b::Op as O;
        let results = self.registers()?;
        let offset = self.offset;
        let op = match self.byte()? {
            33 if self.version >= 3 => {
                self.seen_panic = true;
                O::Panic(m::Register(self.word()?))
            }
            0 => O::I64(self.integer()?),
            1 => O::Bool(self.boolean()?),
            2 => O::Unit,
            3 => O::Add(m::Register(self.word()?), m::Register(self.word()?)),
            4 => O::LessThan(m::Register(self.word()?), m::Register(self.word()?)),
            5 => O::EqualI64(m::Register(self.word()?), m::Register(self.word()?)),
            6 => O::Copy(m::Register(self.word()?)),
            7 => O::Move(m::Register(self.word()?)),
            8 => O::CopyPlace(p::PlaceId(self.word()?)),
            9 => O::MovePlace(p::PlaceId(self.word()?)),
            10 => O::Initialize {
                local: d::LocalId(self.word()?),
                value: m::Register(self.word()?),
            },
            11 => O::Replace {
                place: p::PlaceId(self.word()?),
                value: m::Register(self.word()?),
            },
            12 => O::PreparePlace {
                destination: b::PreparedPlaceId(self.word()?),
                place: p::PlaceId(self.word()?),
            },
            13 => O::ReplacePrepared {
                destination: b::PreparedPlaceId(self.word()?),
                value: m::Register(self.word()?),
            },
            14 => O::DiscardPrepared(b::PreparedPlaceId(self.word()?)),
            15 => O::DropValue(m::Register(self.word()?)),
            16 => O::DropLocal(d::LocalId(self.word()?)),
            17 => O::DropLocalIfInitialized(d::LocalId(self.word()?)),
            18 => O::DropValueIfAvailable(m::Register(self.word()?)),
            19 => O::Cleanup(b::Cleanup {
                steps: self.list(
                    Table::Elements,
                    self.budget.limits().elements,
                    17,
                    Self::cleanup,
                )?,
            }),
            20 => O::Borrow {
                loan: b::LoanId(self.word()?),
                kind: self.kind(tags::read_loan)?,
                place: p::PlaceId(self.word()?),
            },
            21 => O::EndBorrow {
                loan: b::LoanId(self.word()?),
                mode: self.kind(tags::read_end)?,
            },
            22 => O::ArenaEnter {
                owner: d::LocalId(self.word()?),
                allocator: m::Register(self.word()?),
            },
            23 => O::ArenaExit {
                owner: d::LocalId(self.word()?),
            },
            24 => O::Call {
                function: m::FunctionId(self.word()?),
                arguments: self.registers()?,
            },
            25 => O::Import {
                import: m::ImportId(self.word()?),
                arguments: self.registers()?,
            },
            26 => O::CoreCall(o::core::Call {
                operation: self.kind(tags::read_core)?,
                type_arguments: self.list(
                    Table::Fields,
                    self.budget.limits().fields,
                    13,
                    Self::type_use,
                )?,
                arguments: self.registers()?,
                result_loan: self.optional()?.map(b::LoanId),
            }),
            27 => O::Record {
                definition: m::DefinitionId(self.word()?),
                fields: self.registers()?,
                initialization_order: self.ids(self.budget.limits().fields)?,
            },
            28 => O::Variant {
                case: self.word()?,
                payload: self.registers()?,
            },
            29 => O::IsCase {
                value: m::Register(self.word()?),
                case: self.word()?,
            },
            30 => O::CopyCaseField {
                value: m::Register(self.word()?),
                case: self.word()?,
                field: self.word()?,
                guard: m::Register(self.word()?),
            },
            31 => O::BorrowCaseField {
                loan: b::LoanId(self.word()?),
                value: m::Register(self.word()?),
                case: self.word()?,
                field: self.word()?,
                guard: m::Register(self.word()?),
            },
            32 => O::UnpackVariant {
                value: m::Register(self.word()?),
                case: self.word()?,
                guard: m::Register(self.word()?),
            },
            v => return Err(self.invalid(Tag::Instruction, v, offset)),
        };
        Ok(b::Instruction {
            results,
            op,
            span: self.span()?,
        })
    }
    fn target(&mut self) -> Result<m::Target, CodecError> {
        Ok(m::Target {
            block: m::BlockId(self.word()?),
            arguments: self.registers()?,
        })
    }
    fn terminator(&mut self) -> Result<b::Terminator, CodecError> {
        let offset = self.offset;
        let kind = match self.byte()? {
            0 => b::Term::Return(m::Register(self.word()?)),
            1 => b::Term::Jump(self.target()?),
            2 => b::Term::Branch {
                condition: m::Register(self.word()?),
                then_target: self.target()?,
                else_target: self.target()?,
            },
            3 => b::Term::Unreachable,
            v => return Err(self.invalid(Tag::Terminator, v, offset)),
        };
        Ok(b::Terminator {
            kind,
            span: self.span()?,
        })
    }
    fn block(&mut self) -> Result<b::Block, CodecError> {
        Ok(b::Block {
            parameters: self.registers()?,
            instructions: self.list(
                Table::Instructions,
                self.budget.limits().instructions,
                17,
                Self::instruction,
            )?,
            terminator: self.terminator()?,
            span: self.span()?,
        })
    }
    fn body(&mut self) -> Result<b::FunctionBody, CodecError> {
        let loans = self.count(Table::Loans, self.limits.verification.bodies.loans, 0)?;
        let prepared = self.count(Table::Prepared, self.limits.verification.bodies.prepared, 0)?;
        Ok(b::FunctionBody {
            loans,
            prepared,
            blocks: self.list(Table::Blocks, self.budget.limits().blocks, 33, Self::block)?,
        })
    }
    fn table(&mut self) -> Result<(b::BodyTable, Option<m::FunctionId>), CodecError> {
        let cfg = self.budget.limits();
        let sources = self.list(Table::Sources, cfg.sources, 8, |s| {
            let path = s.name()?;
            let remaining = cfg.total_source_bytes.saturating_sub(s.source_bytes);
            let text = s.string(cfg.source_bytes.min(remaining))?;
            s.source_bytes = s
                .source_bytes
                .checked_add(text.len())
                .ok_or_else(|| s.fail(CodecErrorKind::IntegerOverflow))?;
            Ok(m::Source { path, text })
        })?;
        let definitions = self.list(Table::Definitions, cfg.definitions, 18, Self::definition)?;
        let constructed = self.list(
            Table::Constructed,
            self.limits.verification.bodies.places.ownership.constructed,
            14,
            Self::constructor,
        )?;
        let abi_byte = usize::from(self.version == 4);
        let imports = self.list(Table::Imports, cfg.imports, 33 + abi_byte, Self::import)?;
        let functions = self.list(
            Table::Functions,
            cfg.functions,
            46 + abi_byte,
            Self::function,
        )?;
        let places = self.list(Table::Functions, functions.len(), 8, |s| {
            Ok(p::FunctionPlaces {
                values: s.list(Table::Values, cfg.registers, 13, Self::type_use)?,
                places: s.list(
                    Table::Places,
                    s.limits.verification.bodies.places.places,
                    25,
                    Self::place,
                )?,
            })
        })?;
        let bodies = self.list(Table::Functions, functions.len(), 12, Self::body)?;
        let entry = self.optional()?.map(m::FunctionId);
        Ok((
            b::BodyTable {
                places: p::PlaceTable {
                    declarations: d::Declarations {
                        types: o::TypeTable {
                            sources,
                            definitions,
                            constructed,
                        },
                        imports,
                        functions,
                    },
                    functions: places,
                },
                functions: bodies,
            },
            entry,
        ))
    }
}

pub(super) fn decode(bytes: &[u8], limits: CodecLimits) -> Result<Program, CodecError> {
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
    if !matches!(version, 2..=4) {
        return Err(error(8, CodecErrorKind::Version(version)));
    }
    let flags = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
    if flags != 0 {
        return Err(error(10, CodecErrorKind::Flags(flags)));
    }
    let length = usize::try_from(u32::from_le_bytes(bytes[12..16].try_into().unwrap()))
        .map_err(|_| error(12, CodecErrorKind::IntegerOverflow))?;
    if length != bytes.len() - HEADER {
        return Err(error(12, CodecErrorKind::LengthMismatch));
    }
    let mut reader = Reader {
        input: bytes,
        version,
        seen_panic: false,
        offset: HEADER,
        limits,
        budget: m::Budget::new(limits.budget_limits()),
        totals: [0; 6],
        source_bytes: 0,
    };
    reader.charge(4)?;
    let (table, entry) = reader.table()?;
    if reader.offset != bytes.len() {
        return Err(reader.fail(CodecErrorKind::TrailingBytes));
    }
    if version != 4 && (version == 3) != reader.seen_panic {
        return Err(error(8, CodecErrorKind::ProfileVersion(version)));
    }
    verify(table, entry, reader.budget, limits, reader.offset)
}

#[cfg(test)]
pub(super) fn instruction_fixture(bytes: &[u8]) -> Result<b::Instruction, CodecError> {
    let limits = CodecLimits::default();
    let mut reader = Reader {
        input: bytes,
        version: 3,
        seen_panic: false,
        offset: 0,
        limits,
        budget: m::Budget::new(limits.budget_limits()),
        totals: [0; 6],
        source_bytes: 0,
    };
    let result = reader.instruction()?;
    assert_eq!(reader.offset, bytes.len());
    Ok(result)
}
