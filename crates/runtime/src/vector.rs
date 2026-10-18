//! Allocator-backed typed Vec storage. This is a private storage boundary, not
//! source loan/region or nested-service admission. Its adapter owns those proofs
//! and must clean evaluated incoming owners after a refused push. Dropping a
//! Rust wrapper does not run guest cleanup.
use crate::{allocation as a, cleanup as c, layout as l};
use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};

mod access;
pub use access::{Borrowed, Limits, Loan};

const HEADER: u64 = 128;
const LOAN_ENTRY: u64 = 24;
const NONE: u64 = u64::MAX;
const LIVE: u64 = 0x7665_6374_6f72_3031;
const RETIRING: u64 = LIVE + 1;

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    Allocation(a::Error),
    Layout(l::Error),
    Unavailable,
    State,
    BorrowConflict,
    LoanLimit,
    Bounds(crate::buffer::BoundsError),
}
impl From<a::Error> for Error {
    fn from(e: a::Error) -> Self {
        Self::Allocation(e)
    }
}
impl From<l::Error> for Error {
    fn from(e: l::Error) -> Self {
        match e {
            l::Error::Allocation(e) => Self::Allocation(e),
            e => Self::Layout(e),
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "vector storage failure: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy)]
struct Header {
    len: u64,
    capacity: u64,
    data: Option<a::Handle>,
    metadata: Option<a::Handle>,
    tail: u64,
    loans: u64,
    active: u64,
    exclusive: u64,
    serial: u64,
}
struct Plan {
    requests: [a::Request; 2],
    stride: u64,
    metadata_stride: u64,
    work: u64,
}
fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b)
        .ok_or(Error::Allocation(a::Error::Overflow))
}
fn mul(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_mul(b)
        .ok_or(Error::Allocation(a::Error::Overflow))
}
fn plan(layout: &l::Layout, capacity: u64) -> Result<Plan, Error> {
    if capacity > i64::MAX as u64 {
        return Err(a::Error::Overflow.into());
    }
    let stride = add(layout.size, layout.alignment - 1)? & !(layout.alignment - 1);
    let metadata_stride = add(layout.metadata_bytes, 32)?;
    let work = mul(metadata_stride, capacity)?;
    Ok(Plan {
        requests: [
            a::Request::new(add(work, mul(layout.work_slots, l::WORK_ENTRY)?)?, 8)?,
            a::Request::new(mul(stride, capacity)?, layout.alignment)?,
        ],
        stride,
        metadata_stride,
        work,
    })
}
fn word(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or(Error::State)?
            .try_into()
            .map_err(|_| Error::State)?,
    ))
}
fn put(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn key(ty: o::Type) -> u64 {
    match ty {
        o::Type::I64 => 0,
        o::Type::Bool => 1,
        o::Type::Nominal(id) => 2 | (u64::from(id.0) << 32),
        o::Type::Constructed(id) => 3 | (u64::from(id.0) << 32),
    }
}

/// Fixed owner wrapper. Header, capacity, element initializer spans/order and
/// one serially reused element retirement stack are real guest reservations.
pub struct Vector<'l, 't> {
    layouts: &'l l::Layouts<'t>,
    ty: o::Type,
    header: Option<a::Handle>,
}
/// A trusted host observation borrowing the allocator, so storage cannot be
/// changed while it is observed. Guest loan registrations are separate work.
pub struct View<'a, 'l, 't, P: a::Provider> {
    value: l::Reservation<'l, 't>,
    allocator: &'a a::Allocator<P>,
}
impl<P: a::Provider> View<'_, '_, '_, P> {
    pub fn root(&self) -> l::Location {
        self.value.root()
    }
    pub fn child(&self, at: l::Location, index: u32) -> Result<l::Location, Error> {
        Ok(self.value.child(at, index, self.allocator)?)
    }
    pub fn read_immediate(&self, at: l::Location) -> Result<l::Immediate, Error> {
        Ok(self.value.read_immediate(at, self.allocator)?)
    }
    pub fn case(&self, at: l::Location) -> Result<u32, Error> {
        Ok(self.value.case(at, self.allocator)?)
    }
    pub fn read_token(&self, at: l::Location) -> Result<l::Token, Error> {
        Ok(self.value.read_token(at, self.allocator)?)
    }
}
impl<'l, 't> Vector<'l, 't> {
    pub fn new<P: a::Provider>(
        layouts: &'l l::Layouts<'t>,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Self, Error> {
        Self::new_with_limits(layouts, ty, Limits::default(), allocator)
    }
    pub fn new_with_limits<P: a::Provider>(
        layouts: &'l l::Layouts<'t>,
        ty: o::Type,
        limits: Limits,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Self, Error> {
        let properties = layouts
            .types()
            .properties(ty)
            .ok_or(l::Error::InvalidType(ty))?;
        if properties.contains_loan || properties.contains_allocator || properties.contains_owner {
            return Err(l::Error::InvalidType(ty).into());
        }
        layouts.get(ty).ok_or(l::Error::InvalidType(ty))?;
        let header = allocator.allocate(a::Request::new(
            add(HEADER, mul(limits.loans as u64, LOAN_ENTRY)?)?,
            8,
        )?)?;
        let bytes = allocator.bytes_mut(header)?;
        bytes.fill(0);
        put(bytes, 0, LIVE);
        put(bytes, 8, key(ty));
        put(bytes, 80, layouts.identity());
        put(bytes, 88, limits.loans as u64);
        put(bytes, 112, 1);
        put(bytes, 120, NONE);
        Ok(Self {
            layouts,
            ty,
            header: Some(header),
        })
    }
    fn layout(&self) -> &l::Layout {
        self.layouts.get(self.ty).expect("admitted vector element")
    }
    fn inspect<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<Header, Error> {
        let owner = self.header.ok_or(Error::Unavailable)?;
        if allocator.request(owner)?.bytes() < HEADER || allocator.request(owner)?.alignment() != 8
        {
            return Err(Error::State);
        }
        let bytes = allocator.bytes(owner)?;
        if word(bytes, 0)? != LIVE
            || word(bytes, 8)? != key(self.ty)
            || word(bytes, 80)? != self.layouts.identity()
        {
            return Err(Error::State);
        }
        let len = word(bytes, 16)?;
        let capacity = word(bytes, 24)?;
        let loans = word(bytes, 88)?;
        let active = word(bytes, 96)?;
        let exclusive = word(bytes, 104)?;
        let serial = word(bytes, 112)?;
        let tail = word(bytes, 120)?;
        if allocator.request(owner)? != a::Request::new(add(HEADER, mul(loans, LOAN_ENTRY)?)?, 8)?
            || active > loans
            || (exclusive != 0 && active != 1)
            || (len == 0 && tail != NONE)
            || (len != 0 && tail >= len)
        {
            return Err(Error::State);
        }
        if len > capacity || capacity > i64::MAX as u64 {
            return Err(Error::State);
        }
        if capacity == 0 {
            if bytes[32..80].iter().any(|b| *b != 0) {
                return Err(Error::State);
            }
            return Ok(Header {
                len,
                capacity,
                data: None,
                metadata: None,
                tail,
                loans,
                active,
                exclusive,
                serial,
            });
        }
        let data = a::Handle::from_token(bytes[32..56].try_into().unwrap())?;
        let metadata = a::Handle::from_token(bytes[56..80].try_into().unwrap())?;
        let plan = plan(self.layout(), capacity)?;
        if data == metadata
            || data == owner
            || metadata == owner
            || allocator.request(data)? != plan.requests[1]
            || allocator.request(metadata)? != plan.requests[0]
        {
            return Err(Error::State);
        }
        Ok(Header {
            len,
            capacity,
            data: Some(data),
            metadata: Some(metadata),
            tail,
            loans,
            active,
            exclusive,
            serial,
        })
    }
    pub fn len<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<i64, Error> {
        let h = self.inspect(allocator)?;
        if h.exclusive != 0 {
            return Err(Error::BorrowConflict);
        }
        Ok(h.len as i64)
    }
    pub fn is_empty<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<bool, Error> {
        Ok(self.len(allocator)? == 0)
    }
    pub fn capacity<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<i64, Error> {
        let h = self.inspect(allocator)?;
        if h.exclusive != 0 {
            return Err(Error::BorrowConflict);
        }
        Ok(h.capacity as i64)
    }
    fn element(&self, header: Header, index: u64) -> Result<l::Reservation<'l, 't>, Error> {
        if index >= header.capacity {
            return Err(Error::State);
        }
        let p = plan(self.layout(), header.capacity)?;
        Ok(l::Reservation::element(
            self.layouts,
            self.ty,
            header.data.ok_or(Error::State)?,
            header.metadata.ok_or(Error::State)?,
            mul(index, p.stride)?,
            mul(index, p.metadata_stride)?,
            p.work,
        ))
    }
    pub fn view<'a, P: a::Provider>(
        &self,
        index: i64,
        allocator: &'a a::Allocator<P>,
    ) -> Result<View<'a, 'l, 't, P>, Error> {
        let h = self.inspect(allocator)?;
        if h.exclusive != 0 {
            return Err(Error::BorrowConflict);
        }
        let index = u64::try_from(index).map_err(|_| Error::State)?;
        if index >= h.len {
            return Err(Error::State);
        }
        Ok(View {
            value: self.element(h, index)?,
            allocator,
        })
    }
    /// Move an admitted whole owning value only after all growth reservations
    /// succeed. Refusal preserves the collection and the incoming obligation;
    /// the source adapter must retire the evaluated argument on that error edge.
    pub fn push_from<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.push(input, span, allocator, false)
    }
    /// Preserve a Copy SSA operand, including aggregate/enum values.
    pub fn push_copy<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.push(input, span, allocator, true)
    }
    fn push<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
        copy: bool,
    ) -> Result<(), Error> {
        if !std::ptr::eq(self.layouts.types(), input.layouts().types())
            || self.layouts.types().equivalent(self.ty, input.root().ty()) != Some(true)
            || (copy && !self.layouts.types().properties(self.ty).unwrap().is_copy)
        {
            return Err(l::Error::InvalidType(input.root().ty()).into());
        }
        if span.is_some_and(|s| !self.layouts.span_valid(s)) {
            return Err(l::Error::InvalidSpan.into());
        }
        input.validate_retirement(allocator)?;
        let old = self.inspect(allocator)?;
        if old.active != 0 {
            return Err(Error::BorrowConflict);
        }
        let len = add(old.len, 1)?;
        if len > i64::MAX as u64 {
            return Err(a::Error::Overflow.into());
        }
        let mut next = old;
        if len > old.capacity {
            let capacity = old.capacity.saturating_mul(2).min(i64::MAX as u64).max(len);
            let p = plan(self.layout(), capacity)?;
            allocator.check(&p.requests)?;
            let metadata = allocator.allocate(p.requests[0])?;
            let data = match allocator.allocate(p.requests[1]) {
                Ok(h) => h,
                Err(e) => {
                    allocator.release(metadata)?;
                    return Err(e.into());
                }
            };
            allocator.bytes_mut(metadata)?.fill(0);
            allocator.bytes_mut(data)?.fill(0);
            if let Some(from) = old.data {
                allocator.copy_bytes(from, 0, data, 0, mul(old.len, p.stride)? as usize)?;
            }
            if let Some(from) = old.metadata {
                allocator.copy_bytes(
                    from,
                    0,
                    metadata,
                    0,
                    mul(old.len, p.metadata_stride)? as usize,
                )?;
            }
            next = Header {
                capacity,
                data: Some(data),
                metadata: Some(metadata),
                ..old
            };
        }
        let destination = self.element(next, old.len)?;
        self.check_tail(old, allocator)?;
        let result = if copy {
            destination.copy_from(destination.root(), input, input.root(), allocator)
        } else {
            input.move_root_into(&destination, destination.root(), allocator)
        };
        if let Err(e) = result {
            if next.capacity != old.capacity {
                allocator.release(next.data.unwrap())?;
                allocator.release(next.metadata.unwrap())?;
            }
            return Err(e.into());
        }
        let p = plan(self.layout(), next.capacity)?;
        let offset = (mul(old.len, p.metadata_stride)? + self.layout().metadata_bytes) as usize;
        let bytes = &mut allocator.bytes_mut(next.metadata.unwrap())?[offset..offset + 16];
        bytes.fill(0);
        if let Some(s) = span {
            bytes[..4].copy_from_slice(&s.source.0.to_le_bytes());
            bytes[4..8].copy_from_slice(&s.start.to_le_bytes());
            bytes[8..12].copy_from_slice(&s.end.to_le_bytes());
            bytes[12..].copy_from_slice(&1u32.to_le_bytes());
        }
        self.append_order(next, old.len, allocator)?;
        let bytes = allocator.bytes_mut(self.header.unwrap())?;
        put(bytes, 16, len);
        put(bytes, 24, next.capacity);
        bytes[32..56].copy_from_slice(&next.data.unwrap().token());
        bytes[56..80].copy_from_slice(&next.metadata.unwrap().token());
        if next.capacity != old.capacity {
            if let Some(h) = old.data {
                allocator.release(h)?;
            }
            if let Some(h) = old.metadata {
                allocator.release(h)?;
            }
        }
        Ok(())
    }
    /// Consume the guest owner into a checked, private in-memory token.
    pub fn take_token<P: a::Provider>(
        &mut self,
        allocator: &a::Allocator<P>,
    ) -> Result<[u8; 32], Error> {
        if self.inspect(allocator)?.active != 0 {
            return Err(Error::BorrowConflict);
        }
        let mut bytes = [0; 32];
        bytes[..24].copy_from_slice(&self.header.take().unwrap().token());
        bytes[24..].copy_from_slice(&key(self.ty).to_le_bytes());
        Ok(bytes)
    }
    pub fn from_token<P: a::Provider>(
        layouts: &'l l::Layouts<'t>,
        ty: o::Type,
        bytes: &mut [u8; 32],
        allocator: &a::Allocator<P>,
    ) -> Result<Self, Error> {
        if word(bytes, 24)? != key(ty) {
            return Err(l::Error::InvalidType(ty).into());
        }
        if layouts.get(ty).is_none() {
            return Err(l::Error::InvalidType(ty).into());
        }
        let value = Self {
            layouts,
            ty,
            header: Some(a::Handle::from_token(bytes[..24].try_into().unwrap())?),
        };
        value.inspect(allocator)?;
        bytes.fill(0);
        Ok(value)
    }
    /// Reverse element publication order and each element's actual initializer
    /// order. The common context contains leaf failures/panics and drains peers.
    pub fn cleanup<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        let h = self.inspect(allocator)?;
        if h.active != 0 {
            return Err(Error::BorrowConflict);
        }
        put(allocator.bytes_mut(self.header.unwrap())?, 0, RETIRING);
        let mut index = h.tail;
        for remaining in (0..h.len).rev() {
            if index >= h.len {
                return Err(Error::State);
            }
            let p = plan(self.layout(), h.capacity)?;
            let offset = (mul(index, p.metadata_stride)? + self.layout().metadata_bytes) as usize;
            let previous = word(allocator.bytes(h.metadata.unwrap())?, offset + 16)?;
            let bytes = &allocator.bytes(h.metadata.unwrap())?[offset..offset + 16];
            let get = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
            let span = match get(12) {
                0 => None,
                1 => Some(m::Span {
                    source: m::SourceId(get(0)),
                    start: get(4),
                    end: get(8),
                }),
                _ => return Err(Error::State),
            };
            let mut value = self.element(h, index)?;
            value.begin_retirement(span, allocator)?;
            put(allocator.bytes_mut(self.header.unwrap())?, 16, remaining);
            while let Some(leaf) = value.next_leaf(allocator)? {
                let offset = leaf.offset as usize;
                let token = l::Token {
                    leaf,
                    bytes: value.data(allocator)?[offset..offset + 32]
                        .try_into()
                        .unwrap(),
                };
                context.field(
                    token,
                    leaf.span,
                    &mut c::Contextual::new(
                        |token: &mut l::Token, reason, context: &mut c::Context<'_>| {
                            cleaner(token, allocator, reason, context)
                        },
                    ),
                );
                value.complete_leaf()?;
            }
            index = previous;
        }
        if index != NONE {
            return Err(Error::State);
        }
        if let Some(h) = h.data {
            allocator.release(h)?;
        }
        if let Some(h) = h.metadata {
            allocator.release(h)?;
        }
        allocator.release(self.header.take().unwrap())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn array_request_arithmetic_rejects_unrepresentable_capacities() {
        let types = o::TypeTable::default().verify(Default::default()).unwrap();
        let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
        let layout = layouts.get(o::Type::I64).unwrap();
        for capacity in [i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX] {
            assert!(matches!(
                plan(layout, capacity),
                Err(Error::Allocation(a::Error::Overflow))
            ));
        }
    }
    #[test]
    fn copy_push_grows_real_guest_storage_and_preserves_operand() {
        let types = o::TypeTable::default().verify(Default::default()).unwrap();
        let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
        let mut vector = Vector::new(&layouts, o::Type::I64, &mut allocator).unwrap();
        for i in 0..17 {
            value
                .write_immediate(value.root(), l::Immediate::I64(i), &mut allocator)
                .unwrap();
            vector.push_copy(&mut value, None, &mut allocator).unwrap();
        }
        assert_eq!(vector.len(&allocator).unwrap(), 17);
        assert_eq!(vector.capacity(&allocator).unwrap(), 32);
        for i in 0..17 {
            let view = vector.view(i, &allocator).unwrap();
            assert_eq!(
                view.read_immediate(view.root()).unwrap(),
                l::Immediate::I64(i)
            );
        }
        assert_eq!(
            value.read_immediate(value.root(), &allocator).unwrap(),
            l::Immediate::I64(16)
        );
        value.release_storage(&mut allocator).unwrap();
        let mut instance = c::Instance::default();
        let mut frame = instance
            .enter_with_context(
                c::Limits {
                    slots: 1,
                    ..Default::default()
                },
                |_: &mut (), _, ctx: &mut c::Context<'_>| {
                    vector
                        .cleanup(&mut allocator, ctx, |_, _, _, _| {
                            panic!("Copy has no cleanup")
                        })
                        .map_err(|_| 99)
                },
            )
            .unwrap();
        frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
        frame.initialize(c::Slot(0), (), None).unwrap();
        let report = frame.finish(c::Exit::Return(None)).report;
        assert!(report.primary().is_none());
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}
