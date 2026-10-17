//! Mapping independently typed ownership MIR obligations to entry-frame cells.
//! This mapping supplies no executable payload layout or guest allocator grant.

use super::{Frame, Kind, Reason, Slot, StateError};
use lang_mir::unit::FunctionId;

/// Construction order borrowed only from independently verified ownership MIR.
/// Physical field storage remains in declaration order. Copy/vacant fields may
/// be represented by None when draining owning payloads.
#[derive(Clone, Copy, Debug)]
pub struct RecordOrder<'a> {
    order: &'a [u32],
    fields: &'a [lang_mir::unit::Register],
    values: &'a [lang_mir::unit::ownership::TypeUse],
}
impl<'a> RecordOrder<'a> {
    pub fn new(typed: &'a b::TypedBodies, function: FunctionId, site: b::Site) -> Option<Self> {
        let index = function.0 as usize;
        let instruction = typed
            .functions()
            .get(index)?
            .blocks
            .get(site.block.0 as usize)?
            .instructions
            .get(site.order.checked_sub(1)?)?;
        let b::Op::Record {
            fields,
            initialization_order,
            ..
        } = &instruction.op
        else {
            return None;
        };
        Some(Self {
            order: initialization_order,
            fields,
            values: &typed.places().functions()[index].values,
        })
    }
    pub const fn field_count(&self) -> usize {
        self.fields.len()
    }
    pub fn reverse(&self) -> impl Iterator<Item = u32> + '_ {
        self.order.iter().rev().copied()
    }
    pub fn field_span(&self, field: u32) -> Option<lang_mir::unit::Span> {
        let value = self.fields.get(field as usize)?;
        Some(self.values[value.0 as usize].span)
    }
    /// Consume each field before yielding it to cleanup. A failed field cleanup
    /// can therefore be recorded and iteration continued without retry. The
    /// caller must contain/report each trusted cleanup and exhaust the drain.
    /// Dropping this iterator leaves unvisited fields with their current owner.
    pub fn drain<'fields, T>(
        &self,
        fields: &'fields mut [Option<T>],
    ) -> Result<RecordDrain<'a, 'fields, T>, StateError> {
        if fields.len() != self.fields.len() {
            return Err(StateError::AggregateArity {
                expected: self.fields.len(),
                actual: fields.len(),
            });
        }
        Ok(RecordDrain {
            order: self.order.iter().rev(),
            fields,
        })
    }
}
#[must_use = "exhaust the drain to clean all selected fields"]
pub struct RecordDrain<'order, 'fields, T> {
    order: std::iter::Rev<std::slice::Iter<'order, u32>>,
    fields: &'fields mut [Option<T>],
}
impl<T> Iterator for RecordDrain<'_, '_, T> {
    type Item = (u32, T);
    fn next(&mut self) -> Option<Self::Item> {
        for &field in self.order.by_ref() {
            if let Some(value) = self.fields[field as usize].take() {
                return Some((field, value));
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.order.len()))
    }
}
impl<T> std::iter::FusedIterator for RecordDrain<'_, '_, T> {}
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
