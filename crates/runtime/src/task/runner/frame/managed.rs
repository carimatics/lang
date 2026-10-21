use super::*;
use crate::cleanup as c;
use std::num::NonZeroUsize;

#[derive(Clone, Copy)]
pub struct Reference {
    pub activation: ActivationId,
    pub loan: usize,
    pub generation: u64,
}
#[derive(Clone, Copy)]
pub enum Target {
    Allocator,
    Cell(Location),
    Vector {
        origin: Reference,
        at: l::Location,
    },
    Map {
        origin: Reference,
        at: l::Location,
    },
    Shared {
        origin: Reference,
        ty: o::Type,
        at: l::Location,
    },
    Arena {
        origin: Reference,
        at: l::Location,
    },
}
pub struct Loan {
    pub generation: u64,
    pub target: Option<Target>,
    pub vector: Option<crate::vector::Loan>,
    pub map: Option<crate::map::Loan>,
    pub shared: Option<(crate::shared::managed::Read, o::Type)>,
    pub custody: Option<crate::shared::managed::Custody>,
    pub arena: Option<(usize, crate::arena::managed::Read)>,
}
pub struct Managed {
    pub session: c::Session,
    pub references: Vec<Option<Reference>>,
    pub loans: Vec<Loan>,
    pub older: Vec<Option<usize>>,
    pub newer: Vec<Option<usize>>,
    pub active: Vec<bool>,
    pub selected: Vec<bool>,
    pub spans: Vec<Option<m::Span>>,
    pub head: Option<usize>,
}
impl Managed {
    pub fn prepare(
        cells: usize,
        loans: usize,
        records: NonZeroUsize,
        budget: &mut m::Budget,
    ) -> Result<Self, Reason> {
        fn vacant<T>(budget: &mut m::Budget, len: usize) -> Result<Vec<Option<T>>, Reason> {
            let mut v = budget.vector(len)?;
            v.resize_with(len, || None);
            Ok(v)
        }
        let session = c::Session::prepare(records, budget)?;
        let references = vacant(budget, cells)?;
        let older = vacant(budget, cells)?;
        let newer = vacant(budget, cells)?;
        let spans = vacant(budget, cells)?;
        let mut active = budget.vector(cells)?;
        active.resize(cells, false);
        let mut selected = budget.vector(cells)?;
        selected.resize(cells, false);
        let mut entries = budget.vector(loans)?;
        entries.resize_with(loans, || Loan {
            generation: 0,
            target: None,
            vector: None,
            map: None,
            shared: None,
            custody: None,
            arena: None,
        });
        Ok(Self {
            session,
            references,
            loans: entries,
            older,
            newer,
            active,
            selected,
            spans,
            head: None,
        })
    }
    pub fn insert(&mut self, slot: usize, span: m::Span) {
        if self.active[slot] {
            return;
        }
        self.older[slot] = self.head;
        self.newer[slot] = None;
        if let Some(head) = self.head {
            self.newer[head] = Some(slot);
        }
        self.head = Some(slot);
        self.active[slot] = true;
        self.spans[slot] = Some(span);
    }
    pub fn remove(&mut self, slot: usize) {
        if !self.active[slot] {
            return;
        }
        let older = self.older[slot].take();
        let newer = self.newer[slot].take();
        if let Some(newer) = newer {
            self.older[newer] = older;
        } else {
            self.head = older;
        }
        if let Some(older) = older {
            self.newer[older] = newer;
        }
        self.active[slot] = false;
    }
}

impl Frame<'_, '_> {
    pub fn is_copy(&self, slot: usize) -> Result<bool, Reason> {
        Ok(self
            .types
            .properties(self.root(slot)?.at.ty())
            .ok_or(Reason::Invariant)?
            .is_copy)
    }
    pub fn is_allocator(&self, slot: usize) -> Result<bool, Reason> {
        Ok(
            matches!(self.root(slot)?.at.ty(),o::Type::Constructed(id) if matches!(self.types.table().constructed[id.0 as usize].constructor,o::Constructor::Builtin(o::Builtin::Allocator))),
        )
    }
    pub fn is_reference(&self, slot: usize) -> Result<bool, Reason> {
        Ok(
            matches!(self.root(slot)?.at.ty(),o::Type::Constructed(id) if matches!(self.types.table().constructed[id.0 as usize].constructor,o::Constructor::Loan(..))),
        )
    }
    pub fn is_owner(&self, slot: usize) -> Result<bool, Reason> {
        let p = self
            .types
            .properties(self.root(slot)?.at.ty())
            .ok_or(Reason::Invariant)?;
        Ok(!p.is_copy && !p.contains_loan && !p.contains_task && !p.contains_allocator)
    }
    pub fn mark(&mut self, slot: usize, span: m::Span) -> Result<(), Reason> {
        if self.is_owner(slot)? {
            self.managed
                .as_mut()
                .ok_or(Reason::Unsupported)?
                .insert(slot, span);
        }
        self.live[slot] = true;
        Ok(())
    }
    pub fn reference(&self, slot: usize) -> Result<Target, Reason> {
        self.available(slot)?;
        let managed = self.managed.as_ref().ok_or(Reason::Unsupported)?;
        let reference = managed.references[slot].ok_or(Reason::Invariant)?;
        if reference.activation != self.activation {
            return Err(Reason::Invariant);
        }
        let loan = managed.loans.get(reference.loan).ok_or(Reason::Invariant)?;
        if reference.generation != loan.generation {
            return Err(Reason::Invariant);
        }
        loan.target.ok_or(Reason::Invariant)
    }
    pub fn borrow<P: a::Provider>(
        &mut self,
        loan: b::LoanId,
        place: &p::Place,
        to: usize,
        allocator: &a::Allocator<P>,
    ) -> Result<(), Reason> {
        let target = self.place_target(place, allocator)?;
        self.borrow_target(loan, target, to)
    }
    pub fn borrow_target(
        &mut self,
        loan: b::LoanId,
        target: Target,
        to: usize,
    ) -> Result<(), Reason> {
        let managed = self.managed.as_mut().ok_or(Reason::Unsupported)?;
        let entry = managed
            .loans
            .get_mut(loan.0 as usize)
            .ok_or(Reason::Invariant)?;
        if entry.target.is_some() {
            return Err(Reason::Invariant);
        }
        entry.generation = entry.generation.checked_add(1).ok_or(Reason::Invariant)?;
        entry.target = Some(target);
        managed.references[to] = Some(Reference {
            activation: self.activation,
            loan: loan.0 as usize,
            generation: entry.generation,
        });
        self.live[to] = true;
        Ok(())
    }
    pub fn end_loan<P: a::Provider>(
        &mut self,
        id: b::LoanId,
        allocator: &mut a::Allocator<P>,
        services: &mut crate::task::runner::services::Services<'_, '_>,
    ) -> Result<(), Reason> {
        let meter = services.work.clone();
        meter.unmetered(|| {
            let types = self.types;
            let managed = self.managed.as_mut().ok_or(Reason::Invariant)?;
            let loan = managed
                .loans
                .get_mut(id.0 as usize)
                .ok_or(Reason::Invariant)?;
            loan.target = None;
            loan.custody = None;
            let mut failure = None;
            if let Some(mut read) = loan.vector.take() {
                let mut cleaner = c::Contextual::new(|_: &mut (), _, _: &mut c::Context<'_>| {
                    read.end(services.layouts, allocator).map_err(|_| 6u32)
                });
                if managed
                    .session
                    .retire(
                        c::Slot(id.0 as usize),
                        (),
                        Some(self.origin),
                        c::Reason::ScopeExit,
                        &mut cleaner,
                    )
                    .is_err()
                {
                    failure = Some(Reason::Cleanup);
                }
            }
            if let Some(mut read) = loan.map.take() {
                let mut cleaner = c::Contextual::new(|_: &mut (), _, _: &mut c::Context<'_>| {
                    read.end(services.layouts, allocator).map_err(|_| 7u32)
                });
                if managed
                    .session
                    .retire(
                        c::Slot(id.0 as usize),
                        (),
                        Some(self.origin),
                        c::Reason::ScopeExit,
                        &mut cleaner,
                    )
                    .is_err()
                {
                    failure = Some(Reason::Cleanup);
                }
            }
            if let Some((read, ty)) = loan.shared.take() {
                let mut cleaner =
                    c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
                        let drain = services.drain.as_mut().ok_or(8u32)?;
                        let heap = services.heap.as_mut().ok_or(8u32)?;
                        drain
                            .end_read_with_services(
                                read,
                                ty,
                                crate::retirement::Services {
                                    heap: Some(heap),
                                    channels: services
                                        .channels
                                        .as_mut()
                                        .map(|channels| (channels, &mut services.scheduler)),
                                },
                                allocator,
                                context,
                                |token, allocator, _, _| {
                                    crate::buffer::cleanup_token(types, token, allocator)
                                        .map_err(|_| 3u32)
                                },
                            )
                            .map_err(|_| 8u32)
                    });
                if managed
                    .session
                    .retire(
                        c::Slot(id.0 as usize),
                        (),
                        Some(self.origin),
                        c::Reason::ScopeExit,
                        &mut cleaner,
                    )
                    .is_err()
                {
                    failure = Some(Reason::Cleanup);
                }
            }
            if let Some((slot, mut read)) = loan.arena.take() {
                let arena = self
                    .arenas
                    .get_mut(slot)
                    .and_then(Option::as_mut)
                    .ok_or(Reason::Invariant)?;
                let mut cleaner = c::Contextual::new(|_: &mut (), _, _: &mut c::Context<'_>| {
                    arena.end_read(&mut read, allocator).map_err(|_| 9u32)
                });
                if managed
                    .session
                    .retire(
                        c::Slot(id.0 as usize),
                        (),
                        Some(self.origin),
                        c::Reason::ScopeExit,
                        &mut cleaner,
                    )
                    .is_err()
                {
                    failure = Some(Reason::Cleanup);
                }
            }
            failure.map_or(Ok(()), Err)
        })
    }
    pub fn end_loans<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
        services: &mut crate::task::runner::services::Services<'_, '_>,
    ) -> Result<(), Reason> {
        let count = self.managed.as_ref().map_or(0, |m| m.loans.len());
        let mut failure = None;
        // A retained shared read can own a collection's last strong count.
        // Unregister all collection views before any shared read can free it.
        for shared in [false, true] {
            for id in (0..count).rev() {
                if self
                    .managed
                    .as_ref()
                    .is_some_and(|m| m.loans[id].shared.is_some())
                    != shared
                {
                    continue;
                }
                if let Err(reason) = self.end_loan(b::LoanId(id as u32), allocator, services) {
                    failure.get_or_insert(reason);
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }
    pub fn loans_ended(&self) -> bool {
        self.managed
            .as_ref()
            .is_none_or(|m| m.loans.iter().all(|l| l.target.is_none()))
    }
    pub fn cleanup_slot<P: a::Provider>(
        &mut self,
        slot: usize,
        reason: c::Reason,
        allocator: &mut a::Allocator<P>,
        services: &mut crate::task::runner::services::Services<'_, '_>,
    ) -> Result<(), Reason> {
        let meter = services.work.clone();
        meter.unmetered(|| {
            if !self.live[slot] {
                return Ok(());
            }
            if !self.is_owner(slot)? {
                self.forget(slot);
                return Ok(());
            }
            let types = self.types;
            let arena_owner = self.is_arena_owner(slot)?;
            let managed = self.managed.as_mut().ok_or(Reason::Unsupported)?;
            let span = managed.spans[slot];
            managed.remove(slot);
            self.live[slot] = false;
            if arena_owner {
                let arena = self
                    .arenas
                    .get_mut(slot)
                    .and_then(Option::as_mut)
                    .ok_or(Reason::Invariant)?;
                let mut cleaner =
                    c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
                        let exit = if context.reason() == c::Reason::Failure {
                            crate::arena::managed::Exit::Abort
                        } else {
                            crate::arena::managed::Exit::Normal
                        };
                        arena
                            .cleanup(exit, allocator, context, |token, allocator, _, context| {
                                services.retire(types, token, allocator, context)
                            })
                            .map_err(|_| 10u32)
                    });
                return managed
                    .session
                    .retire(c::Slot(slot), (), span, reason, &mut cleaner)
                    .map_err(|_| Reason::Cleanup);
            }
            let cell = self
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .cell_mut(slot)?;
            let mut cleaner = c::Contextual::new(
                |cell: &mut &mut l::Reservation<'_, '_>, _, context: &mut c::Context<'_>| {
                    cell.retire(allocator, context, |token, allocator, _, context| {
                        services.retire(types, token, allocator, context)
                    })
                    .map_err(|_| 4u32)?;
                    cell.recycle(allocator).map_err(|_| 5u32)
                },
            );
            managed
                .session
                .retire(c::Slot(slot), cell, span, reason, &mut cleaner)
                .map_err(|_| Reason::Cleanup)
        })
    }
    pub fn cleanup_selected<P: a::Provider>(
        &mut self,
        slots: impl IntoIterator<Item = usize>,
        reason: c::Reason,
        allocator: &mut a::Allocator<P>,
        services: &mut crate::task::runner::services::Services<'_, '_>,
    ) -> Result<(), Reason> {
        if let Some(managed) = &mut self.managed {
            managed.selected.fill(false);
        }
        for slot in slots {
            if self.is_owner(slot)? {
                self.managed.as_mut().ok_or(Reason::Unsupported)?.selected[slot] = true;
            } else {
                self.forget(slot);
            }
        }
        let mut next = self.managed.as_ref().and_then(|m| m.head);
        let mut failure = None;
        while let Some(slot) = next {
            let m = self.managed.as_ref().ok_or(Reason::Invariant)?;
            next = m.older[slot];
            if m.selected[slot]
                && let Err(reason) = self.cleanup_slot(slot, reason, allocator, services)
            {
                failure.get_or_insert(reason);
            }
        }
        match failure {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }
    pub fn cleanup_all<P: a::Provider>(
        &mut self,
        keep: Option<usize>,
        reason: c::Reason,
        allocator: &mut a::Allocator<P>,
        services: &mut crate::task::runner::services::Services<'_, '_>,
    ) -> Result<(), Reason> {
        let ended = self.end_loans(allocator, services);
        let mut next = self.managed.as_ref().and_then(|m| m.head);
        let mut failure = ended.err();
        while let Some(slot) = next {
            next = self.managed.as_ref().ok_or(Reason::Invariant)?.older[slot];
            if Some(slot) != keep
                && let Err(reason) = self.cleanup_slot(slot, reason, allocator, services)
            {
                failure.get_or_insert(reason);
            }
        }
        match failure {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }
}
