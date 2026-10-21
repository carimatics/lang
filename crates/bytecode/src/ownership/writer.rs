use super::*;
use o::{bodies as b, declarations as d, places as p};

struct Writer<'a> {
    version: u16,
    output: Option<Vec<u8>>,
    length: usize,
    maximum: usize,
    budget: &'a mut m::Budget,
}

impl Writer<'_> {
    fn charge(&mut self, n: usize) -> Result<(), CodecError> {
        self.budget
            .charge(n)
            .map_err(|e| error(self.length, CodecErrorKind::Budget(e)))
    }
    fn bytes(&mut self, value: &[u8]) -> Result<(), CodecError> {
        self.charge(1)?;
        let end = self
            .length
            .checked_add(value.len())
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
            output.extend_from_slice(value);
        }
        self.length = end;
        Ok(())
    }
    fn byte(&mut self, value: u8) -> Result<(), CodecError> {
        self.bytes(&[value])
    }
    fn word(&mut self, value: u32) -> Result<(), CodecError> {
        self.bytes(&value.to_le_bytes())
    }
    fn count(&mut self, value: usize) -> Result<(), CodecError> {
        self.word(
            u32::try_from(value)
                .map_err(|_| error(self.length, CodecErrorKind::IntegerOverflow))?,
        )
    }
    fn string(&mut self, value: &str) -> Result<(), CodecError> {
        self.count(value.len())?;
        self.charge(value.len() / 1024 + 1)?;
        self.bytes(value.as_bytes())
    }
    fn boolean(&mut self, value: bool) -> Result<(), CodecError> {
        self.byte(u8::from(value))
    }
    fn list<T>(
        &mut self,
        values: &[T],
        mut item: impl FnMut(&mut Self, &T) -> Result<(), CodecError>,
    ) -> Result<(), CodecError> {
        self.count(values.len())?;
        for value in values {
            self.charge(1)?;
            item(self, value)?;
        }
        Ok(())
    }
    fn span(&mut self, value: m::Span) -> Result<(), CodecError> {
        self.word(value.source.0)?;
        self.word(value.start)?;
        self.word(value.end)
    }
    fn ty(&mut self, value: o::Type) -> Result<(), CodecError> {
        match value {
            o::Type::I64 => self.byte(0),
            o::Type::Bool => self.byte(1),
            o::Type::Nominal(id) => {
                self.byte(2)?;
                self.word(id.0)
            }
            o::Type::Constructed(id) => {
                self.byte(3)?;
                self.word(id.0)
            }
        }
    }
    fn type_use(&mut self, value: &o::TypeUse) -> Result<(), CodecError> {
        self.ty(value.ty)?;
        self.span(value.span)
    }
    fn optional(&mut self, value: Option<u32>) -> Result<(), CodecError> {
        if let Some(id) = value {
            self.byte(1)?;
            self.word(id)
        } else {
            self.byte(0)
        }
    }
    fn constructor(&mut self, value: &o::Constructed) -> Result<(), CodecError> {
        use o::Constructor as C;
        match value.constructor {
            C::ServiceSetup(..)
            | C::ServiceOwner(..)
            | C::ServiceHandle(_)
            | C::Builtin(o::Builtin::ServiceError) => {
                return Err(error(
                    self.length,
                    CodecErrorKind::UnsupportedServiceProfile { span: value.span },
                ));
            }
            C::Builtin(o::Builtin::ChannelError) => {
                return Err(error(
                    self.length,
                    CodecErrorKind::UnsupportedChannelProfile { span: value.span },
                ));
            }
            C::Builtin(v) => {
                let tag = tags::builtin(v).ok_or_else(|| {
                    error(
                        self.length,
                        CodecErrorKind::UnsupportedTaskProfile { span: value.span },
                    )
                })?;
                self.byte(0)?;
                self.byte(tag)?;
            }
            C::Option(v) => {
                self.byte(1)?;
                self.ty(v)?;
            }
            C::Result(a, z) => {
                self.byte(2)?;
                self.ty(a)?;
                self.ty(z)?;
            }
            C::Vec(v) => {
                self.byte(3)?;
                self.ty(v)?;
            }
            C::Map(a, z) => {
                self.byte(4)?;
                self.ty(a)?;
                self.ty(z)?;
            }
            C::Shared(v) => {
                self.byte(5)?;
                self.ty(v)?;
            }
            C::Weak(v) => {
                self.byte(6)?;
                self.ty(v)?;
            }
            C::Arena(v) => {
                self.byte(7)?;
                self.ty(v)?;
            }
            C::Loan(kind, v) => {
                self.byte(8)?;
                self.byte(tags::loan(kind))?;
                self.ty(v)?;
            }
            C::ArenaOwner(v) => {
                self.byte(9)?;
                self.word(v.function.0)?;
                self.word(v.local)?;
            }
            C::ChannelSetup(_) | C::Sender(_) | C::Receiver(_) | C::SendResult(_) => {
                return Err(error(
                    self.length,
                    CodecErrorKind::UnsupportedChannelProfile { span: value.span },
                ));
            }
            C::Task(_) => {
                return Err(error(
                    self.length,
                    CodecErrorKind::UnsupportedTaskProfile { span: value.span },
                ));
            }
        }
        self.span(value.span)
    }
    fn definition(&mut self, value: &o::Definition) -> Result<(), CodecError> {
        self.string(&value.name)?;
        self.boolean(value.public)?;
        self.span(value.span)?;
        match &value.kind {
            o::DefinitionKind::Record(fields) => {
                self.byte(0)?;
                self.list(fields, |s, v| {
                    s.string(&v.name)?;
                    s.span(v.span)?;
                    s.type_use(&v.ty)
                })
            }
            o::DefinitionKind::Enum(cases) => {
                self.byte(1)?;
                self.list(cases, |s, v| {
                    s.string(&v.name)?;
                    s.span(v.span)?;
                    s.list(&v.payload, Self::type_use)
                })
            }
        }
    }
    fn registers(&mut self, values: &[m::Register]) -> Result<(), CodecError> {
        self.list(values, |s, v| s.word(v.0))
    }
    fn function(&mut self, value: &d::Function) -> Result<(), CodecError> {
        if let d::FunctionEffect::Asynchronous { span } = value.effect {
            return Err(error(
                self.length,
                CodecErrorKind::UnsupportedTaskProfile { span },
            ));
        }
        if self.version < 4 && value.abi != d::FunctionAbi::Guest {
            return Err(error(
                self.length,
                CodecErrorKind::UnsupportedCInterface { span: value.span },
            ));
        }
        self.string(&value.name)?;
        self.boolean(value.public)?;
        if self.version == 4 {
            self.byte(tags::function_abi(value.abi))?;
        }
        self.span(value.span)?;
        self.span(value.signature_span)?;
        self.list(&value.parameters, |s, v| s.word(v.0))?;
        self.type_use(&value.result)?;
        self.list(&value.locals, |s, v| {
            s.string(&v.name)?;
            s.span(v.span)?;
            s.byte(tags::local(v.kind))?;
            s.type_use(&v.ty)
        })
    }
    fn place(&mut self, value: &p::Place) -> Result<(), CodecError> {
        self.word(value.origin.0)?;
        match value.root {
            p::Root::Local(id) => {
                self.byte(0)?;
                self.word(id.0)?;
            }
            p::Root::Value(id) => {
                self.byte(1)?;
                self.word(id.0)?;
            }
        }
        self.list(&value.projections, |s, v| {
            match v.kind {
                p::ProjectionKind::Dereference => s.byte(0)?,
                p::ProjectionKind::Field(id) => {
                    s.byte(1)?;
                    s.word(id)?;
                }
            }
            s.span(v.span)
        })?;
        self.span(value.span)
    }
    fn cleanup(&mut self, value: &b::CleanupStep) -> Result<(), CodecError> {
        let (tag, id) = match value.target {
            b::CleanupTarget::Local(id) => (0, id.0),
            b::CleanupTarget::Value(id) => (1, id.0),
            b::CleanupTarget::Loan(id) => (2, id.0),
            b::CleanupTarget::Arena(id) => (3, id.0),
        };
        self.byte(tag)?;
        self.word(id)?;
        self.span(value.span)
    }
    fn instruction(&mut self, value: &b::Instruction) -> Result<(), CodecError> {
        if value.op.is_service() {
            return Err(error(
                self.length,
                CodecErrorKind::UnsupportedServiceProfile { span: value.span },
            ));
        }
        if value.op.is_channel() {
            return Err(error(
                self.length,
                CodecErrorKind::UnsupportedChannelProfile { span: value.span },
            ));
        }
        if value.op.is_task() {
            return Err(error(
                self.length,
                CodecErrorKind::UnsupportedTaskProfile { span: value.span },
            ));
        }
        use b::Op as O;
        self.registers(&value.results)?;
        match &value.op {
            b::Op::PrepareService { .. }
            | b::Op::ServiceSend { .. }
            | b::Op::ServiceReceive { .. } => {
                return Err(error(
                    self.length,
                    CodecErrorKind::UnsupportedServiceProfile { span: value.span },
                ));
            }
            b::Op::PrepareChannel { .. }
            | b::Op::ChannelSend { .. }
            | b::Op::ChannelReceive { .. } => {
                return Err(error(
                    self.length,
                    CodecErrorKind::UnsupportedChannelProfile { span: value.span },
                ));
            }
            b::Op::TaskScopeEnter(_)
            | b::Op::TaskScopeExit(_)
            | b::Op::TaskSpawn { .. }
            | b::Op::TaskAwait(_)
            | b::Op::TaskCancel(_)
            | b::Op::TaskYield
            | b::Op::TaskCheck
            | b::Op::TaskSaveReturn(_) => {
                return Err(error(
                    self.length,
                    CodecErrorKind::UnsupportedTaskProfile { span: value.span },
                ));
            }
            O::Panic(code) => {
                self.byte(33)?;
                self.word(code.0)?;
            }
            O::I64(v) => {
                self.byte(0)?;
                self.bytes(&v.to_le_bytes())?;
            }
            O::Bool(v) => {
                self.byte(1)?;
                self.boolean(*v)?;
            }
            O::Unit => self.byte(2)?,
            O::Add(a, z) => {
                self.byte(3)?;
                self.word(a.0)?;
                self.word(z.0)?;
            }
            O::LessThan(a, z) => {
                self.byte(4)?;
                self.word(a.0)?;
                self.word(z.0)?;
            }
            O::EqualI64(a, z) => {
                self.byte(5)?;
                self.word(a.0)?;
                self.word(z.0)?;
            }
            O::Copy(v) => {
                self.byte(6)?;
                self.word(v.0)?;
            }
            O::Move(v) => {
                self.byte(7)?;
                self.word(v.0)?;
            }
            O::CopyPlace(v) => {
                self.byte(8)?;
                self.word(v.0)?;
            }
            O::MovePlace(v) => {
                self.byte(9)?;
                self.word(v.0)?;
            }
            O::Initialize { local, value } => {
                self.byte(10)?;
                self.word(local.0)?;
                self.word(value.0)?;
            }
            O::Replace { place, value } => {
                self.byte(11)?;
                self.word(place.0)?;
                self.word(value.0)?;
            }
            O::PreparePlace { destination, place } => {
                self.byte(12)?;
                self.word(destination.0)?;
                self.word(place.0)?;
            }
            O::ReplacePrepared { destination, value } => {
                self.byte(13)?;
                self.word(destination.0)?;
                self.word(value.0)?;
            }
            O::DiscardPrepared(v) => {
                self.byte(14)?;
                self.word(v.0)?;
            }
            O::DropValue(v) => {
                self.byte(15)?;
                self.word(v.0)?;
            }
            O::DropLocal(v) => {
                self.byte(16)?;
                self.word(v.0)?;
            }
            O::DropLocalIfInitialized(v) => {
                self.byte(17)?;
                self.word(v.0)?;
            }
            O::DropValueIfAvailable(v) => {
                self.byte(18)?;
                self.word(v.0)?;
            }
            O::Cleanup(v) => {
                self.byte(19)?;
                self.list(&v.steps, Self::cleanup)?;
            }
            O::Borrow { loan, kind, place } => {
                self.byte(20)?;
                self.word(loan.0)?;
                self.byte(tags::loan(*kind))?;
                self.word(place.0)?;
            }
            O::EndBorrow { loan, mode } => {
                self.byte(21)?;
                self.word(loan.0)?;
                self.byte(tags::end(*mode))?;
            }
            O::ArenaEnter { owner, allocator } => {
                self.byte(22)?;
                self.word(owner.0)?;
                self.word(allocator.0)?;
            }
            O::ArenaExit { owner } => {
                self.byte(23)?;
                self.word(owner.0)?;
            }
            O::Call {
                function,
                arguments,
            } => {
                self.byte(24)?;
                self.word(function.0)?;
                self.registers(arguments)?;
            }
            O::Import { import, arguments } => {
                self.byte(25)?;
                self.word(import.0)?;
                self.registers(arguments)?;
            }
            O::CoreCall(v) => {
                self.byte(26)?;
                self.byte(tags::core(v.operation).ok_or_else(|| {
                    error(
                        self.length,
                        CodecErrorKind::UnsupportedChannelProfile { span: value.span },
                    )
                })?)?;
                self.list(&v.type_arguments, Self::type_use)?;
                self.registers(&v.arguments)?;
                self.optional(v.result_loan.map(|v| v.0))?;
            }
            O::Record {
                definition,
                fields,
                initialization_order,
            } => {
                self.byte(27)?;
                self.word(definition.0)?;
                self.registers(fields)?;
                self.list(initialization_order, |s, v| s.word(*v))?;
            }
            O::Variant { case, payload } => {
                self.byte(28)?;
                self.word(*case)?;
                self.registers(payload)?;
            }
            O::IsCase { value, case } => {
                self.byte(29)?;
                self.word(value.0)?;
                self.word(*case)?;
            }
            O::CopyCaseField {
                value,
                case,
                field,
                guard,
            } => {
                self.byte(30)?;
                self.word(value.0)?;
                self.word(*case)?;
                self.word(*field)?;
                self.word(guard.0)?;
            }
            O::BorrowCaseField {
                loan,
                value,
                case,
                field,
                guard,
            } => {
                self.byte(31)?;
                self.word(loan.0)?;
                self.word(value.0)?;
                self.word(*case)?;
                self.word(*field)?;
                self.word(guard.0)?;
            }
            O::UnpackVariant { value, case, guard } => {
                self.byte(32)?;
                self.word(value.0)?;
                self.word(*case)?;
                self.word(guard.0)?;
            }
        }
        self.span(value.span)
    }
    fn target(&mut self, value: &m::Target) -> Result<(), CodecError> {
        self.word(value.block.0)?;
        self.registers(&value.arguments)
    }
    fn terminator(&mut self, value: &b::Terminator) -> Result<(), CodecError> {
        match &value.kind {
            b::Term::ReturnSaved => {
                return Err(error(
                    self.length,
                    CodecErrorKind::UnsupportedTaskProfile { span: value.span },
                ));
            }
            b::Term::Return(v) => {
                self.byte(0)?;
                self.word(v.0)?;
            }
            b::Term::Jump(v) => {
                self.byte(1)?;
                self.target(v)?;
            }
            b::Term::Branch {
                condition,
                then_target,
                else_target,
            } => {
                self.byte(2)?;
                self.word(condition.0)?;
                self.target(then_target)?;
                self.target(else_target)?;
            }
            b::Term::Unreachable => self.byte(3)?,
        }
        self.span(value.span)
    }
    fn unit(&mut self, program: &Program, payload: u32) -> Result<(), CodecError> {
        let typed = program.flow.source().activity().moved().typed();
        if let Some(span) = typed.places().declarations().types().channel_feature_span() {
            return Err(error(
                self.length,
                CodecErrorKind::UnsupportedChannelProfile { span },
            ));
        }
        if let Some(span) = typed.places().declarations().task_feature_span() {
            return Err(error(
                self.length,
                CodecErrorKind::UnsupportedTaskProfile { span },
            ));
        }
        for (index, body) in typed.functions().iter().enumerate() {
            if !body.task_scopes.is_empty() || !body.task_bindings.is_empty() {
                return Err(error(
                    self.length,
                    CodecErrorKind::UnsupportedTaskProfile {
                        span: typed.places().declarations().functions()[index].span,
                    },
                ));
            }
        }
        self.bytes(MAGIC)?;
        self.bytes(&self.version.to_le_bytes())?;
        self.bytes(&0_u16.to_le_bytes())?;
        self.word(payload)?;
        let typed = program.flow.source().activity().moved().typed();
        let declarations = typed.places().declarations();
        let types = declarations.types().table();
        self.list(&types.sources, |s, v| {
            s.string(&v.path)?;
            s.string(&v.text)
        })?;
        self.list(&types.definitions, Self::definition)?;
        self.list(&types.constructed, Self::constructor)?;
        self.list(declarations.imports(), |s, v| {
            if s.version < 4 && v.abi != d::ImportAbi::Host {
                return Err(error(
                    s.length,
                    CodecErrorKind::UnsupportedCInterface { span: v.span },
                ));
            }
            s.string(&v.name)?;
            if s.version == 4 {
                s.byte(tags::import_abi(v.abi))?;
            }
            s.span(v.span)?;
            s.list(&v.parameters, Self::type_use)?;
            s.type_use(&v.result)
        })?;
        self.list(declarations.functions(), Self::function)?;
        self.list(typed.places().functions(), |s, v| {
            s.list(&v.values, Self::type_use)?;
            s.list(&v.places, Self::place)
        })?;
        self.list(typed.functions(), |s, v| {
            s.count(v.loans)?;
            s.count(v.prepared)?;
            s.list(&v.blocks, |s, v| {
                s.registers(&v.parameters)?;
                s.list(&v.instructions, Self::instruction)?;
                s.terminator(&v.terminator)?;
                s.span(v.span)
            })
        })?;
        self.optional(program.entry.map(|v| v.0))
    }
}

pub(super) fn encode(program: Program, limits: CodecLimits) -> Result<Vec<u8>, CodecError> {
    let mut budget = m::Budget::with_usage(limits.budget_limits(), program.usage())
        .map_err(|e| error(0, CodecErrorKind::Budget(e)))?;
    let typed = program.flow.source().activity().moved().typed();
    let declarations = typed.places().declarations();
    budget
        .charge(declarations.imports().len())
        .and_then(|()| budget.charge(declarations.functions().len()))
        .map_err(|e| error(0, CodecErrorKind::Budget(e)))?;
    let version = if declarations
        .imports()
        .iter()
        .any(|i| i.abi == d::ImportAbi::C)
        || declarations
            .functions()
            .iter()
            .any(|f| f.abi == d::FunctionAbi::CExport)
    {
        4
    } else if typed.has_explicit_panic() {
        3
    } else {
        2
    };
    let size = {
        let mut w = Writer {
            version,
            output: None,
            length: 0,
            maximum: limits.bytes,
            budget: &mut budget,
        };
        w.unit(&program, 0)?;
        w.length
    };
    let payload = u32::try_from(
        size.checked_sub(HEADER)
            .ok_or_else(|| error(0, CodecErrorKind::IntegerOverflow))?,
    )
    .map_err(|_| error(12, CodecErrorKind::IntegerOverflow))?;
    budget
        .bytes(size)
        .map_err(|e| error(0, CodecErrorKind::Budget(e)))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(size)
        .map_err(|_| error(0, CodecErrorKind::AllocationFailed))?;
    let mut writer = Writer {
        version,
        output: Some(output),
        length: 0,
        maximum: size,
        budget: &mut budget,
    };
    writer.unit(&program, payload)?;
    debug_assert_eq!(writer.length, size);
    Ok(writer.output.unwrap())
}

#[cfg(test)]
pub(super) fn instruction_fixture(value: &b::Instruction) -> Vec<u8> {
    let mut budget = m::Budget::new(m::Limits::default());
    let size = {
        let mut w = Writer {
            version: 3,
            output: None,
            length: 0,
            maximum: 4096,
            budget: &mut budget,
        };
        w.instruction(value).unwrap();
        w.length
    };
    let mut writer = Writer {
        version: 3,
        output: Some(Vec::with_capacity(size)),
        length: 0,
        maximum: size,
        budget: &mut budget,
    };
    writer.instruction(value).unwrap();
    writer.output.unwrap()
}

#[cfg(test)]
mod task_tests {
    use super::*;
    #[test]
    fn task_instructions_and_saved_returns_have_no_old_wire_encoding() {
        for version in [2, 3, 4] {
            for op in [
                b::Op::TaskScopeEnter(b::TaskScopeId(1)),
                b::Op::TaskScopeExit(b::TaskScopeId(0)),
                b::Op::TaskSpawn {
                    function: m::FunctionId(0),
                    arguments: vec![],
                },
                b::Op::TaskAwait(m::Register(0)),
                b::Op::TaskCancel(p::PlaceId(0)),
                b::Op::TaskYield,
                b::Op::TaskCheck,
                b::Op::TaskSaveReturn(m::Register(0)),
            ] {
                let mut budget = m::Budget::new(Default::default());
                let mut w = writer(&mut budget, version);
                let e = w
                    .instruction(&b::Instruction {
                        results: vec![m::Register(9)],
                        op,
                        span: span(),
                    })
                    .unwrap_err();
                assert!(matches!(
                    e.kind,
                    CodecErrorKind::UnsupportedTaskProfile { .. }
                ));
                assert_eq!(w.length, 0);
                assert!(w.output.unwrap().is_empty());
            }
            let mut budget = m::Budget::new(Default::default());
            let mut w = writer(&mut budget, version);
            let e = w
                .terminator(&b::Terminator {
                    kind: b::Term::ReturnSaved,
                    span: span(),
                })
                .unwrap_err();
            assert!(matches!(
                e.kind,
                CodecErrorKind::UnsupportedTaskProfile { .. }
            ));
            assert_eq!(w.length, 0);
            assert!(w.output.unwrap().is_empty());
        }
    }
    fn span() -> m::Span {
        m::Span {
            source: m::SourceId(0),
            start: 0,
            end: 5,
        }
    }
    fn writer(budget: &mut m::Budget, version: u16) -> Writer<'_> {
        Writer {
            version,
            output: Some(Vec::new()),
            length: 0,
            maximum: 1024,
            budget,
        }
    }
    #[test]
    fn old_writers_never_assign_task_tags_or_write_a_partial_constructor() {
        for version in [2, 3, 4] {
            for constructor in [
                o::Constructor::Task(o::Type::I64),
                o::Constructor::Builtin(o::Builtin::TaskError),
            ] {
                let mut budget = m::Budget::new(Default::default());
                let mut w = writer(&mut budget, version);
                let e = w
                    .constructor(&o::Constructed {
                        constructor,
                        span: span(),
                    })
                    .unwrap_err();
                assert!(
                    matches!(e.kind,CodecErrorKind::UnsupportedTaskProfile{span:s} if s==span())
                );
                assert_eq!(w.length, 0);
                assert!(w.output.unwrap().is_empty());
            }
        }
        assert_eq!(tags::builtin(o::Builtin::TaskError), None);
        assert!(matches!(
            tags::read_builtin(10, 0).unwrap_err().kind,
            CodecErrorKind::InvalidTag {
                tag: Tag::Builtin,
                value: 10
            }
        ));
    }
    #[test]
    fn old_writers_refuse_each_channel_type_before_any_constructor_bytes() {
        for version in [2, 3, 4] {
            for constructor in [
                o::Constructor::ChannelSetup(o::Type::I64),
                o::Constructor::Sender(o::Type::I64),
                o::Constructor::Receiver(o::Type::I64),
                o::Constructor::SendResult(o::Type::I64),
                o::Constructor::Builtin(o::Builtin::ChannelError),
            ] {
                let mut budget = m::Budget::new(Default::default());
                let mut w = writer(&mut budget, version);
                let error = w
                    .constructor(&o::Constructed {
                        constructor,
                        span: span(),
                    })
                    .unwrap_err();
                assert!(
                    matches!(error.kind,CodecErrorKind::UnsupportedChannelProfile{span:s} if s==span())
                );
                assert_eq!(w.length, 0);
                assert!(w.output.unwrap().is_empty());
            }
        }
    }
    #[test]
    fn channel_instructions_and_core_calls_have_no_old_wire_encoding() {
        let mut operations = vec![
            b::Op::PrepareChannel {
                destination: b::PreparedPlaceId(0),
                endpoint: m::Register(0),
                role: b::ChannelRole::Sender,
                message_type: o::TypeUse {
                    ty: o::Type::I64,
                    span: span(),
                },
            },
            b::Op::ChannelSend {
                authority: b::PreparedPlaceId(0),
                payload: m::Register(1),
            },
            b::Op::ChannelReceive {
                authority: b::PreparedPlaceId(0),
            },
        ];
        for operation in o::core::Operation::ALL
            .into_iter()
            .filter(|op| op.is_channel())
        {
            assert_eq!(tags::core(operation), None);
            operations.push(b::Op::CoreCall(o::core::Call {
                operation,
                type_arguments: vec![],
                arguments: vec![],
                result_loan: None,
            }));
        }
        for version in [2, 3, 4] {
            for op in &operations {
                let mut budget = m::Budget::new(Default::default());
                let mut w = writer(&mut budget, version);
                let error = w
                    .instruction(&b::Instruction {
                        results: vec![m::Register(9)],
                        op: op.clone(),
                        span: span(),
                    })
                    .unwrap_err();
                assert!(
                    matches!(error.kind, CodecErrorKind::UnsupportedChannelProfile { span: s } if s == span())
                );
                assert_eq!(w.length, 0);
                assert!(w.output.unwrap().is_empty());
            }
        }
        for value in 37..=u8::MAX {
            assert!(matches!(
                tags::read_core(value, 0).unwrap_err().kind,
                CodecErrorKind::InvalidTag {
                    tag: Tag::CoreOperation,
                    ..
                }
            ));
        }
        assert_eq!(tags::builtin(o::Builtin::ChannelError), None);
    }
    #[test]
    fn old_writers_refuse_even_scalar_async_declarations_before_output() {
        for version in [2, 3, 4] {
            let mut budget = m::Budget::new(Default::default());
            let mut w = writer(&mut budget, version);
            let f = d::Function {
                effect: d::FunctionEffect::Asynchronous { span: span() },
                abi: d::FunctionAbi::Guest,
                name: "f".into(),
                public: false,
                span: span(),
                signature_span: span(),
                parameters: vec![],
                result: o::TypeUse {
                    ty: o::Type::I64,
                    span: span(),
                },
                locals: vec![],
            };
            assert!(matches!(
                w.function(&f).unwrap_err().kind,
                CodecErrorKind::UnsupportedTaskProfile { .. }
            ));
            assert_eq!(w.length, 0);
            assert!(w.output.unwrap().is_empty());
        }
    }
}
