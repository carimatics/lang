//! Windows x64 RUNTIME_FUNCTION tables derived from the emitted prologues.
use cranelift_codegen::{Context, isa::TargetIsa, isa::unwind::UnwindInfo};
use cranelift_module::FuncId;
use cranelift_object::ObjectProduct;
use lang_mir::unit as m;
use object::write::Relocation;
use object::{RelocationEncoding, RelocationFlags, RelocationKind, SectionKind};
use target_lexicon::{Architecture, OperatingSystem, Triple};

use crate::source_debug::Error;

struct Function {
    id: FuncId,
    size: u32,
    data: Vec<u8>,
}

pub(crate) struct Records {
    enabled: bool,
    functions: Vec<Function>,
}

impl Records {
    pub(crate) fn reserve(
        &mut self,
        additional: usize,
        budget: &mut m::Budget,
    ) -> Result<(), Error> {
        if !self.enabled {
            return Ok(());
        }
        let count = self
            .functions
            .len()
            .checked_add(additional)
            .ok_or(Error::Capacity)?;
        if count > self.functions.capacity() {
            let mut records = budget.vector(count).map_err(Error::Resource)?;
            records.append(&mut self.functions);
            self.functions = records;
        }
        Ok(())
    }

    pub(crate) fn new(
        target: &Triple,
        count: usize,
        budget: &mut m::Budget,
    ) -> Result<Self, Error> {
        let enabled = target.operating_system == OperatingSystem::Windows
            && target.architecture == Architecture::X86_64;
        Ok(Self {
            enabled,
            functions: if enabled {
                budget.vector(count).map_err(Error::Resource)?
            } else {
                Vec::new()
            },
        })
    }

    pub(crate) fn capture(
        &mut self,
        id: FuncId,
        context: &Context,
        isa: &dyn TargetIsa,
        budget: &mut m::Budget,
    ) -> Result<(), Error> {
        if !self.enabled {
            return Ok(());
        }
        let code = context
            .compiled_code()
            .ok_or_else(|| Error::Backend("unwind capture requires compiled code".into()))?;
        let info = match code
            .create_unwind_info(isa)
            .map_err(|e| Error::Backend(e.to_string()))?
        {
            Some(UnwindInfo::WindowsX64(info)) => info,
            None => return Ok(()),
            _ => {
                return Err(Error::Backend(
                    "expected Windows x64 unwind information".into(),
                ));
            }
        };
        budget.charge(1).map_err(Error::Resource)?;
        let len = info.emit_size();
        // Reserve the writer's section bytes, three relocations and symbols
        // in addition to the explicitly budgeted temporary record buffer.
        budget
            .bytes(
                len.checked_mul(2)
                    .and_then(|n| n.checked_add(512))
                    .ok_or(Error::Capacity)?,
            )
            .map_err(Error::Resource)?;
        let mut data = budget.vector(len).map_err(Error::Resource)?;
        data.resize(len, 0);
        info.emit(&mut data);
        let size = u32::try_from(code.code_buffer().len()).map_err(|_| Error::Capacity)?;
        if self.functions.len() == self.functions.capacity() {
            return Err(Error::Capacity);
        }
        self.functions.push(Function { id, size, data });
        Ok(())
    }

    pub(crate) fn emit(self, product: &mut ObjectProduct) -> Result<(), Error> {
        if self.functions.is_empty() {
            return Ok(());
        }
        let pdata =
            product
                .object
                .add_section(Vec::new(), b".pdata".to_vec(), SectionKind::ReadOnlyData);
        let xdata =
            product
                .object
                .add_section(Vec::new(), b".xdata".to_vec(), SectionKind::ReadOnlyData);
        let unwind_symbol = product.object.section_symbol(xdata);
        for function in self.functions {
            let symbol = product.function_symbol(function.id);
            let object = &mut product.object;
            let data = object.append_section_data(xdata, &function.data, 4);
            let data = u32::try_from(data).map_err(|_| Error::Capacity)?;
            let entry = object.append_section_data(pdata, &[0; 12], 4);
            for (field, target, addend) in [
                (0, symbol, 0),
                (4, symbol, function.size),
                (8, unwind_symbol, data),
            ] {
                object
                    .add_relocation(
                        pdata,
                        Relocation {
                            offset: entry + field,
                            symbol: target,
                            addend: i64::from(addend),
                            flags: RelocationFlags::Generic {
                                kind: RelocationKind::ImageOffset,
                                encoding: RelocationEncoding::Generic,
                                size: 32,
                            },
                        },
                    )
                    .map_err(|e| Error::Backend(e.to_string()))?;
            }
        }
        Ok(())
    }
}
