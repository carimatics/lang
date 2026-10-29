//! In-memory source-local locations produced by the ordinary lowering walk.
//! No backend or debugger protocol type crosses this boundary.
use super::*;

#[derive(Debug, Default)]
pub struct DebugInfo {
    pub(super) functions: Vec<DebugFunction>,
    bytes: usize,
    work: usize,
}
#[derive(Debug)]
pub struct DebugFunction {
    locals: Vec<DebugLocal>,
    pub(super) blocks: Vec<Vec<DebugLocation>>,
}
#[derive(Debug)]
pub struct DebugLocal {
    pub name: String,
    pub declaration: m::Span,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DebugBinding {
    pub local: usize,
    pub register: m::Register,
}
#[derive(Debug)]
pub struct DebugLocation {
    pub instruction: usize,
    pub bindings: Vec<DebugBinding>,
}
impl DebugInfo {
    pub(super) fn charge_block(&mut self) -> Result<()> {
        self.charge(size_of::<Vec<DebugLocation>>(), 1)
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn functions(&self) -> &[DebugFunction] {
        &self.functions
    }
    fn charge(&mut self, bytes: usize, work: usize) -> Result<()> {
        self.bytes = self.bytes.saturating_add(bytes);
        self.work = self.work.saturating_add(work);
        if self.bytes > 32 * 1024 * 1024 || self.work > 10_000_000 {
            return Err(LowerError::Capacity);
        }
        Ok(())
    }
    /// Locations describe source environments before the next instruction.
    /// Missing blocks/locations are unavailable, never inferred from names.
    pub fn location(
        &self,
        function: m::FunctionId,
        block: m::BlockId,
        instruction: usize,
    ) -> Option<(&[DebugLocal], &DebugLocation)> {
        let function = self.functions.get(function.0 as usize)?;
        let locations = function.blocks.get(block.0 as usize)?;
        let index = locations
            .partition_point(|l| l.instruction <= instruction)
            .checked_sub(1)?;
        Some((&function.locals, &locations[index]))
    }
}
impl DebugFunction {
    pub fn locals(&self) -> &[DebugLocal] {
        &self.locals
    }
    pub fn blocks(&self) -> &[Vec<DebugLocation>] {
        &self.blocks
    }
    pub(super) fn capture(c: &mut Context<'_>, function: &r::Function) -> Result<Option<Self>> {
        let Some(debug) = &mut c.debug else {
            return Ok(None);
        };
        debug.charge(
            size_of::<Self>() + function.locals.len() * size_of::<DebugLocal>(),
            function.locals.len(),
        )?;
        let mut locals = c.budget.vector(function.locals.len())?;
        for local in &function.locals {
            c.debug.as_mut().unwrap().charge(local.name.text.len(), 1)?;
            locals.push(DebugLocal {
                name: text(&mut c.budget, &local.name.text)?,
                declaration: c.span(local.name.span)?.ok_or(LowerError::MissingSource)?,
            });
        }
        Ok(Some(Self {
            locals,
            blocks: Vec::new(),
        }))
    }
}
impl Lowerer<'_, '_> {
    pub(super) fn remember(&mut self, exit: &Exit) -> Result<()> {
        let Some(debug) = &mut self.debug else {
            return Ok(());
        };
        let instruction = self.blocks[exit.block.0 as usize].instructions.len();
        self.context.budget.charge(exit.environment.len())?;
        let bytes = size_of::<DebugLocation>() + exit.environment.len() * size_of::<DebugBinding>();
        self.context
            .debug
            .as_mut()
            .unwrap()
            .charge(bytes, exit.environment.len() + 1)?;
        let mut bindings = self.context.budget.vector(exit.environment.len())?;
        // Local IDs follow the checked binder. The environment, rather than
        // register numbering or textual names, proves initialization.
        for (local, register) in exit.environment.iter().enumerate() {
            if let Some(register) = register {
                bindings.push(DebugBinding {
                    local,
                    register: *register,
                });
            }
        }
        let locations = &mut debug.blocks[exit.block.0 as usize];
        let location = DebugLocation {
            instruction,
            bindings,
        };
        if locations
            .last()
            .is_some_and(|l| l.instruction == instruction)
        {
            *locations.last_mut().unwrap() = location;
        } else {
            push(&mut self.context.budget, locations, location)?;
        }
        Ok(())
    }
}
