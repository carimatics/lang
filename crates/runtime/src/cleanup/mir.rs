//! Mapping independently typed ownership MIR obligations to entry-frame cells.
//! This mapping supplies no executable payload layout or guest allocator grant.

use super::{Frame, Kind, Reason, Slot, StateError};
use lang_mir::unit::FunctionId;
use lang_mir::unit::ownership::{bodies as b, declarations as d};

pub struct MirLayout<'a> {
    typed: &'a b::TypedBodies,
    function: FunctionId,
    base: usize,
    locals: usize,
    values: usize,
    end: usize,
}
impl<'a> MirLayout<'a> {
    pub fn new(typed: &'a b::TypedBodies, function: FunctionId, base: usize) -> Option<Self> {
        let locals = typed
            .places()
            .declarations()
            .functions()
            .get(function.0 as usize)?
            .locals
            .len();
        let values = typed
            .places()
            .functions()
            .get(function.0 as usize)?
            .values
            .len();
        let loans = typed.functions().get(function.0 as usize)?.loans;
        let end = base
            .checked_add(locals)?
            .checked_add(values)?
            .checked_add(loans)?;
        Some(Self {
            typed,
            function,
            base,
            locals,
            values,
            end,
        })
    }
    /// Exclusive end cell, including reserved holes for Copy values.
    pub const fn end(&self) -> usize {
        self.end
    }
    pub fn slot(&self, target: b::CleanupTarget) -> Result<Option<(Slot, Kind)>, StateError> {
        let index = self.function.0 as usize;
        let declarations = self.typed.places().declarations();
        let (offset, ty) = match target {
            b::CleanupTarget::Local(local) | b::CleanupTarget::Arena(local) => {
                let local_decl = declarations.functions()[index]
                    .locals
                    .get(local.0 as usize)
                    .ok_or(StateError::InvalidSlot)?;
                if matches!(target, b::CleanupTarget::Arena(_))
                    && local_decl.kind != d::LocalKind::ArenaOwner
                {
                    return Err(StateError::KindMismatch);
                }
                (local.0 as usize, local_decl.ty.ty)
            }
            b::CleanupTarget::Value(value) => {
                let ty = self.typed.places().functions()[index]
                    .values
                    .get(value.0 as usize)
                    .ok_or(StateError::InvalidSlot)?
                    .ty;
                (self.locals + value.0 as usize, ty)
            }
            b::CleanupTarget::Loan(loan) => {
                if self.typed.facts()[index]
                    .loans()
                    .get(loan.0 as usize)
                    .is_none_or(Option::is_none)
                {
                    return Err(StateError::InvalidSlot);
                }
                return Ok(Some((
                    Slot(self.base + self.locals + self.values + loan.0 as usize),
                    Kind::Borrow,
                )));
            }
        };
        let properties = declarations
            .types()
            .properties(ty)
            .ok_or(StateError::InvalidSlot)?;
        // Copy payloads have no owning obligation. They need no runtime cell
        // declaration in this ledger, even when a source cleanup names them.
        Ok((!properties.is_copy).then_some((Slot(self.base + offset), Kind::Owner)))
    }
}

impl<T, C: FnMut(&mut T, Reason) -> Result<(), u32>> Frame<'_, T, C> {
    /// Consume a MIR cleanup range through the dynamic initialization ledger.
    /// Callers supply checked MIR and predeclare its non-Copy/registration cells.
    pub fn cleanup_mir(
        &mut self,
        cleanup: &b::Cleanup,
        layout: &MirLayout<'_>,
    ) -> Result<(), StateError> {
        self.running()?;
        for step in &cleanup.steps {
            if let Some((slot, kind)) = layout.slot(step.target)?
                && self.declared(slot)?.kind != kind
            {
                return Err(StateError::KindMismatch);
            }
        }
        self.cleanup_slots(cleanup.steps.iter().filter_map(|step| {
            layout
                .slot(step.target)
                .expect("prevalidated immutable MIR mapping")
                .map(|(slot, _)| slot)
        }))
    }
}
