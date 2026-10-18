//! Fallible payload/order/work reservations. Cleanup consumers resolve indirect
//! tokens through their owning service; this module never forges such authority.
use super::*;
use allocation::{Allocator, Handle, Provider};

mod preview;
mod transfer;
mod value;
pub use preview::{Preview, PreviewError};
pub use value::{ArenaError, Immediate, OwnershipError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Location {
    owner: Handle,
    ty: o::Type,
    offset: u64,
    metadata: u64,
    parent: Option<Parent>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Parent {
    ty: o::Type,
    offset: u64,
    metadata: u64,
    field: u32,
}
impl Location {
    pub const fn ty(self) -> o::Type {
        self.ty
    }
    pub const fn offset(self) -> u64 {
        self.offset
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Leaf {
    pub ty: o::Type,
    pub offset: u64,
    pub span: Option<m::Span>,
}
/// Detached private token bytes. Their meaning/liveness is checked by the
/// corresponding buffer/collection/shared/arena service before cleanup.
pub struct Token {
    pub leaf: Leaf,
    pub bytes: [u8; 32],
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum State {
    Prepared,
    Draining,
    Released,
}
#[derive(Clone, Copy)]
struct Work {
    ty: o::Type,
    offset: u64,
    metadata: u64,
    span: Option<m::Span>,
}

/// Unpublished inline storage, with order and iterative retirement capacity
/// reserved before field ownership is transferred. It is deliberately not Copy.
/// The execution adapter owns cancellation/retirement; dropping this token does
/// not run guest cleanup. The instance allocator still owns outstanding storage.
pub struct Reservation<'layouts, 'types> {
    layouts: &'layouts Layouts<'types>,
    ty: o::Type,
    data: Handle,
    metadata: Handle,
    top: usize,
    outstanding: bool,
    state: State,
}
impl<'layouts, 'types> Reservation<'layouts, 'types> {
    pub const fn layouts(&self) -> &'layouts Layouts<'types> {
        self.layouts
    }
    /// Validate the active construction-order/retirement shape without taking
    /// ownership or changing payload/order bytes. Copy leaves need their normal
    /// value checks. Reuses the admitted scratch work area.
    pub fn validate_retirement<P: Provider>(
        &mut self,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        if self.state != State::Prepared {
            return Err(Error::State);
        }
        let result = self.begin_retirement(None, allocator);
        self.top = 0;
        if self.state == State::Draining {
            self.state = State::Prepared;
        }
        result
    }
    pub fn allocate<P: Provider>(
        layouts: &'layouts Layouts<'types>,
        ty: o::Type,
        allocator: &mut Allocator<P>,
    ) -> Result<Self, Error> {
        let requests = layouts
            .get(ty)
            .ok_or(Error::InvalidType(ty))?
            .storage_requests()?;
        allocator.check(&requests)?;
        let metadata = allocator.allocate(requests[0])?;
        let data = match allocator.allocate(requests[1]) {
            Ok(data) => data,
            Err(error) => {
                allocator.release(metadata)?;
                return Err(error.into());
            }
        };
        allocator.bytes_mut(data)?.fill(0);
        allocator.bytes_mut(metadata)?.fill(0);
        Ok(Self {
            layouts,
            ty,
            data,
            metadata,
            top: 0,
            outstanding: false,
            state: State::Prepared,
        })
    }
    pub const fn root(&self) -> Location {
        Location {
            owner: self.data,
            ty: self.ty,
            offset: 0,
            metadata: 0,
            parent: None,
        }
    }
    pub fn data<'a, P: Provider>(&self, allocator: &'a Allocator<P>) -> Result<&'a [u8], Error> {
        if self.state == State::Released {
            return Err(Error::State);
        }
        Ok(allocator.bytes(self.data)?)
    }
    pub fn data_mut<'a, P: Provider>(
        &self,
        allocator: &'a mut Allocator<P>,
    ) -> Result<&'a mut [u8], Error> {
        if self.state != State::Prepared {
            return Err(Error::State);
        }
        Ok(allocator.bytes_mut(self.data)?)
    }
    fn validate_location(&self, location: Location) -> Result<&Layout, Error> {
        if self.state != State::Prepared {
            return Err(Error::State);
        }
        if location.owner != self.data {
            return Err(Error::ForeignLocation);
        }
        self.layouts
            .get(location.ty)
            .ok_or(Error::InvalidType(location.ty))
    }
    fn aggregate<'a, P: Provider>(
        layouts: &'a Layouts<'_>,
        work: Work,
        allocator: &Allocator<P>,
        data: Handle,
    ) -> Result<(&'a Aggregate, u64), Error> {
        let layout = layouts.get(work.ty).ok_or(Error::InvalidType(work.ty))?;
        match &layout.kind {
            Kind::Record(aggregate) => Ok((aggregate, work.offset)),
            Kind::Enum {
                payload_offset,
                cases,
            } => {
                let bytes = allocator.bytes(data)?;
                let case = read_u32(bytes, work.offset as usize)?;
                Ok((
                    cases.get(case as usize).ok_or(Error::InvalidCase(case))?,
                    add(work.offset, *payload_offset)?,
                ))
            }
            _ => Err(Error::InvalidType(work.ty)),
        }
    }
    pub fn child<P: Provider>(
        &self,
        location: Location,
        field: u32,
        allocator: &Allocator<P>,
    ) -> Result<Location, Error> {
        self.validate_location(location)?;
        let work = Work {
            ty: location.ty,
            offset: location.offset,
            metadata: location.metadata,
            span: None,
        };
        let (aggregate, payload) = Self::aggregate(self.layouts, work, allocator, self.data)?;
        let index = field;
        let field = aggregate
            .fields
            .get(field as usize)
            .ok_or(Error::InvalidField(field))?;
        Ok(Location {
            owner: self.data,
            ty: field.ty,
            offset: add(payload, field.offset)?,
            metadata: add(location.metadata, field.metadata_offset)?,
            parent: Some(Parent {
                ty: location.ty,
                offset: location.offset,
                metadata: location.metadata,
                field: index,
            }),
        })
    }
    /// Read a validated initializer entry in its actual construction order.
    /// Copy-only aggregates have no order ledger and return InvalidOrder.
    pub fn initialized_field<P: Provider>(
        &self,
        location: Location,
        rank: u32,
        allocator: &Allocator<P>,
    ) -> Result<(u32, Option<m::Span>), Error> {
        let layout = self.validate_location(location)?;
        if layout.metadata_bytes == 0 {
            return Err(Error::InvalidOrder);
        }
        let work = Work {
            ty: location.ty,
            offset: location.offset,
            metadata: location.metadata,
            span: None,
        };
        let (aggregate, _) = Self::aggregate(self.layouts, work, allocator, self.data)?;
        let case = if matches!(layout.kind, Kind::Enum { .. }) {
            u64::from(self.case(location, allocator)?)
        } else {
            0
        };
        if read_u64(allocator.bytes(self.metadata)?, location.metadata as usize)?
            != (case << 32) | 1
        {
            return Err(Error::Uninitialized);
        }
        if rank as usize >= aggregate.fields.len() {
            return Err(Error::InvalidOrder);
        }
        let entry = location.metadata as usize
            + ORDER_HEADER as usize
            + rank as usize * ORDER_ENTRY as usize;
        let bytes = allocator.bytes(self.metadata)?;
        let field = read_u32(bytes, entry)?;
        if field as usize >= aggregate.fields.len() {
            return Err(Error::InvalidOrder);
        }
        let span = read_span(bytes, entry + 4)?;
        if span.is_some_and(|span| !self.layouts.span_valid(span)) {
            return Err(Error::InvalidSpan);
        }
        Ok((field, span))
    }
    /// Record actual initializer order and spans without allocating. A failed
    /// order check leaves this aggregate unpublished/uninitialized for retry.
    pub fn initialize_order<P: Provider>(
        &self,
        location: Location,
        order: impl ExactSizeIterator<Item = (u32, m::Span)>,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        let layout = self.validate_location(location)?;
        let work = Work {
            ty: location.ty,
            offset: location.offset,
            metadata: location.metadata,
            span: None,
        };
        let (aggregate, _) = Self::aggregate(self.layouts, work, allocator, self.data)?;
        let count = order.len();
        if count != aggregate.fields.len() {
            return Err(Error::InvalidOrder);
        }
        if layout.work_slots == 0 {
            return Ok(());
        }
        let case = if matches!(layout.kind, Kind::Enum { .. }) {
            u64::from(read_u32(
                allocator.bytes(self.data)?,
                location.offset as usize,
            )?)
        } else {
            0
        };
        let start = location.metadata as usize;
        let size = add(ORDER_HEADER, mul(order.len() as u64, ORDER_ENTRY)?)? as usize;
        let row = allocator
            .bytes_mut(self.metadata)?
            .get_mut(start..start.checked_add(size).ok_or(Error::Overflow)?)
            .ok_or(Error::State)?;
        row.fill(0);
        let mut written = 0;
        for (rank, (field, span)) in order.enumerate() {
            if rank >= count || field as usize >= count || !self.layouts.span_valid(span) {
                row.fill(0);
                return Err(if rank >= count || field as usize >= count {
                    Error::InvalidOrder
                } else {
                    Error::InvalidSpan
                });
            }
            let seen = ORDER_HEADER as usize + field as usize * ORDER_ENTRY as usize + 16;
            if read_u32(row, seen)? != 0 {
                row.fill(0);
                return Err(Error::InvalidOrder);
            }
            write_u32(row, seen, 1)?;
            let entry = ORDER_HEADER as usize + rank * ORDER_ENTRY as usize;
            write_u32(row, entry, field)?;
            write_span(row, entry + 4, Some(span))?;
            written += 1;
        }
        if written != count {
            row.fill(0);
            return Err(Error::InvalidOrder);
        }
        write_u64(row, 0, (case << 32) | 1)?;
        Ok(())
    }
    /// Transfer construction order directly from this layout's independently
    /// checked MIR, with no temporary guest-path vector or inferred field order.
    pub fn initialize_from_mir<P: Provider>(
        &self,
        location: Location,
        typed: &o::bodies::TypedBodies,
        function: m::FunctionId,
        site: o::bodies::Site,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        self.validate_location(location)?;
        if !std::ptr::eq(self.layouts.types, typed.places().declarations().types()) {
            return Err(Error::ForeignLocation);
        }
        let index = function.0 as usize;
        let instruction = typed
            .functions()
            .get(index)
            .and_then(|f| f.blocks.get(site.block.0 as usize))
            .and_then(|b| {
                site.order
                    .checked_sub(1)
                    .and_then(|order| b.instructions.get(order))
            })
            .ok_or(Error::InvalidOrder)?;
        let values = &typed.places().functions()[index].values;
        let result = instruction
            .results
            .first()
            .and_then(|value| values.get(value.0 as usize))
            .ok_or(Error::InvalidOrder)?;
        if self.layouts.types.equivalent(result.ty, location.ty) != Some(true) {
            return Err(Error::InvalidType(result.ty));
        }
        match &instruction.op {
            o::bodies::Op::Record {
                fields,
                initialization_order,
                ..
            } => self.initialize_order(
                location,
                initialization_order
                    .iter()
                    .map(|&field| (field, values[fields[field as usize].0 as usize].span)),
                allocator,
            ),
            o::bodies::Op::Variant { case, payload } => {
                let layout = self
                    .layouts
                    .get(location.ty)
                    .ok_or(Error::InvalidType(location.ty))?;
                let Kind::Enum { cases, .. } = &layout.kind else {
                    return Err(Error::InvalidType(location.ty));
                };
                let aggregate = cases.get(*case as usize).ok_or(Error::InvalidCase(*case))?;
                if aggregate.fields.len() != payload.len() {
                    return Err(Error::InvalidOrder);
                }
                write_u32(
                    allocator.bytes_mut(self.data)?,
                    location.offset as usize,
                    *case,
                )?;
                self.initialize_order(
                    location,
                    payload
                        .iter()
                        .enumerate()
                        .map(|(field, value)| (field as u32, values[value.0 as usize].span)),
                    allocator,
                )
            }
            _ => Err(Error::InvalidOrder),
        }
    }
    fn push<P: Provider>(&mut self, work: Work, allocator: &mut Allocator<P>) -> Result<(), Error> {
        let root = self
            .layouts
            .get(self.ty)
            .ok_or(Error::InvalidType(self.ty))?;
        if self.top as u64 >= root.work_slots {
            return Err(Error::State);
        }
        let index = add(root.metadata_bytes, mul(self.top as u64, WORK_ENTRY)?)? as usize;
        let bytes = allocator.bytes_mut(self.metadata)?;
        write_u64(
            bytes,
            index,
            type_index(self.layouts.types, work.ty).ok_or(Error::InvalidType(work.ty))? as u64,
        )?;
        write_u64(bytes, index + 8, work.offset)?;
        write_u64(bytes, index + 16, work.metadata)?;
        write_span(bytes, index + 24, work.span)?;
        self.top += 1;
        Ok(())
    }
    fn pop<P: Provider>(&mut self, allocator: &Allocator<P>) -> Result<Option<Work>, Error> {
        if self.top == 0 {
            return Ok(None);
        }
        self.top -= 1; // Consume before handing any leaf to a cleanup callback.
        let root = self
            .layouts
            .get(self.ty)
            .ok_or(Error::InvalidType(self.ty))?;
        let index = add(root.metadata_bytes, mul(self.top as u64, WORK_ENTRY)?)? as usize;
        let bytes = allocator.bytes(self.metadata)?;
        let ty = type_at(
            self.layouts.types,
            usize::try_from(read_u64(bytes, index)?).map_err(|_| Error::Overflow)?,
        )
        .ok_or(Error::State)?;
        Ok(Some(Work {
            ty,
            offset: read_u64(bytes, index + 8)?,
            metadata: read_u64(bytes, index + 16)?,
            span: read_span(bytes, index + 24)?,
        }))
    }
    fn seed<P: Provider>(
        &mut self,
        span: Option<m::Span>,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        self.top = 0;
        if self
            .layouts
            .get(self.ty)
            .ok_or(Error::InvalidType(self.ty))?
            .work_slots
            != 0
        {
            self.push(
                Work {
                    ty: self.ty,
                    offset: 0,
                    metadata: 0,
                    span,
                },
                allocator,
            )?;
        }
        Ok(())
    }
    fn step<P: Provider>(
        &mut self,
        allocator: &mut Allocator<P>,
        retiring: bool,
    ) -> Result<Option<Leaf>, Error> {
        let layouts = self.layouts;
        while let Some(work) = self.pop(allocator)? {
            let layout = layouts.get(work.ty).ok_or(Error::InvalidType(work.ty))?;
            if layout.work_slots == 0 {
                continue;
            }
            if matches!(layout.kind, Kind::Token(_)) {
                return Ok(Some(Leaf {
                    ty: work.ty,
                    offset: work.offset,
                    span: work.span,
                }));
            }
            let (aggregate, payload) = Self::aggregate(layouts, work, allocator, self.data)?;
            let start = work.metadata as usize;
            let state = read_u64(allocator.bytes(self.metadata)?, start)?;
            let case = if matches!(layout.kind, Kind::Enum { .. }) {
                u64::from(read_u32(allocator.bytes(self.data)?, work.offset as usize)?)
            } else {
                0
            };
            if state != (case << 32) | 1 {
                return Err(Error::Uninitialized);
            }
            if retiring {
                write_u64(allocator.bytes_mut(self.metadata)?, start, (case << 32) | 2)?;
            }
            for rank in 0..aggregate.fields.len() {
                let entry = start + ORDER_HEADER as usize + rank * ORDER_ENTRY as usize;
                let bytes = allocator.bytes(self.metadata)?;
                let index = read_u32(bytes, entry)?;
                let span = read_span(bytes, entry + 4)?;
                let field = aggregate
                    .fields
                    .get(index as usize)
                    .ok_or(Error::InvalidOrder)?;
                if layouts
                    .get(field.ty)
                    .ok_or(Error::InvalidType(field.ty))?
                    .work_slots
                    != 0
                {
                    self.push(
                        Work {
                            ty: field.ty,
                            offset: add(payload, field.offset)?,
                            metadata: add(work.metadata, field.metadata_offset)?,
                            span,
                        },
                        allocator,
                    )?;
                }
            }
        }
        Ok(None)
    }
    /// Check the complete active aggregate shape before yielding any obligation.
    /// No provider callback, guest fuel charge, or allocation occurs here.
    pub fn begin_retirement<P: Provider>(
        &mut self,
        span: Option<m::Span>,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        if self.state != State::Prepared {
            return Err(Error::State);
        }
        if span.is_some_and(|span| !self.layouts.span_valid(span)) {
            return Err(Error::InvalidSpan);
        }
        // Validate both handles before touching the reserved work area.
        allocator.request(self.data)?;
        allocator.request(self.metadata)?;
        self.seed(span, allocator)?;
        while self.step(allocator, false)?.is_some() {}
        self.seed(span, allocator)?;
        self.state = State::Draining;
        Ok(())
    }
    /// Each leaf is consumed before return. The adapter must clean/report it,
    /// continue after independent cleanup failures, and finish the entire drain.
    pub fn next_leaf<P: Provider>(
        &mut self,
        allocator: &mut Allocator<P>,
    ) -> Result<Option<Leaf>, Error> {
        if self.state != State::Draining || self.outstanding {
            return Err(Error::State);
        }
        let leaf = self.step(allocator, true)?;
        self.outstanding = leaf.is_some();
        Ok(leaf)
    }
    /// Acknowledge a detached leaf after reporting its cleanup outcome. This
    /// never retries cleanup and is required even when that cleanup failed.
    pub fn complete_leaf(&mut self) -> Result<(), Error> {
        if self.state != State::Draining || !self.outstanding {
            return Err(Error::State);
        }
        self.outstanding = false;
        Ok(())
    }
    /// Drain every indirect token through the entry's shared cleanup context.
    /// Individual failure/panic is contained and reported without preventing
    /// independent leaf cleanup. The callback must consume its token obligation
    /// before effects and must not allocate retirement storage or retry cleanup.
    pub fn cleanup<P: Provider>(
        &mut self,
        allocator: &mut Allocator<P>,
        context: &mut crate::cleanup::Context<'_>,
        cleaner: impl FnMut(
            &mut Token,
            &mut Allocator<P>,
            crate::cleanup::Reason,
            &mut crate::cleanup::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        self.retire(allocator, context, cleaner)?;
        self.release_storage(allocator)
    }
    /// Retire active owners while keeping both regions for an admitted frame.
    /// `recycle` is allowed only after every yielded obligation was acknowledged.
    pub fn retire<P: Provider>(
        &mut self,
        allocator: &mut Allocator<P>,
        context: &mut crate::cleanup::Context<'_>,
        mut cleaner: impl FnMut(
            &mut Token,
            &mut Allocator<P>,
            crate::cleanup::Reason,
            &mut crate::cleanup::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        self.begin_retirement(None, allocator)?;
        while let Some(leaf) = self.next_leaf(allocator)? {
            let offset = leaf.offset as usize;
            let bytes = self
                .data(allocator)?
                .get(offset..offset.checked_add(32).ok_or(Error::Overflow)?)
                .ok_or(Error::State)?
                .try_into()
                .map_err(|_| Error::State)?;
            let token = Token { leaf, bytes };
            let mut callback = crate::cleanup::Contextual::new(
                |token: &mut Token, reason, context: &mut crate::cleanup::Context<'_>| {
                    cleaner(token, allocator, reason, context)
                },
            );
            context.field(token, leaf.span, &mut callback);
            self.complete_leaf()?;
        }
        Ok(())
    }
    /// Reset an exhausted retirement without a provider callback/allocation.
    /// Old structural Locations confer no initialization or loan authority.
    pub fn recycle<P: Provider>(&mut self, allocator: &mut Allocator<P>) -> Result<(), Error> {
        if self.state != State::Draining || self.outstanding || self.top != 0 {
            return Err(Error::State);
        }
        allocator.request(self.data)?;
        allocator.request(self.metadata)?;
        allocator.bytes_mut(self.data)?.fill(0);
        allocator.bytes_mut(self.metadata)?.fill(0);
        self.state = State::Prepared;
        Ok(())
    }
    pub(super) fn check_release<P: Provider>(&self, allocator: &Allocator<P>) -> Result<(), Error> {
        if self.state == State::Released
            || self.outstanding
            || (self.state == State::Draining && self.top != 0)
        {
            return Err(Error::State);
        }
        allocator.request(self.data)?;
        allocator.request(self.metadata)?;
        Ok(())
    }
    /// Cancel unpublished storage (operands still belong to temporaries), or
    /// release a fully drained value. Never release storage before child cleanup.
    pub fn release_storage<P: Provider>(
        &mut self,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        self.check_release(allocator)?;
        allocator.release(self.data)?;
        allocator.release(self.metadata)?;
        self.state = State::Released;
        Ok(())
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset.checked_add(4).ok_or(Error::Overflow)?)
            .ok_or(Error::State)?
            .try_into()
            .map_err(|_| Error::State)?,
    ))
}
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset.checked_add(8).ok_or(Error::Overflow)?)
            .ok_or(Error::State)?
            .try_into()
            .map_err(|_| Error::State)?,
    ))
}
fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), Error> {
    bytes
        .get_mut(offset..offset.checked_add(4).ok_or(Error::Overflow)?)
        .ok_or(Error::State)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn write_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), Error> {
    bytes
        .get_mut(offset..offset.checked_add(8).ok_or(Error::Overflow)?)
        .ok_or(Error::State)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn write_span(bytes: &mut [u8], offset: usize, span: Option<m::Span>) -> Result<(), Error> {
    let (source, start, end) = span.map_or((u32::MAX, 0, 0), |s| (s.source.0, s.start, s.end));
    write_u32(bytes, offset, source)?;
    write_u32(bytes, offset + 4, start)?;
    write_u32(bytes, offset + 8, end)
}
fn read_span(bytes: &[u8], offset: usize) -> Result<Option<m::Span>, Error> {
    let source = read_u32(bytes, offset)?;
    if source == u32::MAX {
        return Ok(None);
    }
    Ok(Some(m::Span {
        source: m::SourceId(source),
        start: read_u32(bytes, offset + 4)?,
        end: read_u32(bytes, offset + 8)?,
    }))
}
