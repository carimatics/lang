//! Typed, allocator-backed Map storage and exact bounded lookup. This is a
//! private storage boundary; adapters retain source move/loan and nested-service
//! admission. Explicit cleanup owns retirement; Rust wrapper disposal does not.
use crate::{allocation as a, buffer, cleanup as c, layout as l};
use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};

mod access;
pub use access::{Borrowed, Loan};
#[cfg(test)]
mod tests;

const HEADER: u64 = 168;
const LOAN_ENTRY: u64 = 16;
const MAGIC: u64 = 0x6d61_705f_3030_3031;
const NONE: u64 = u64::MAX;
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    Allocation(a::Error),
    Layout(l::Error),
    Buffer(buffer::Fault),
    State,
    Unavailable,
    WrongKey,
    WorkLimit,
    BorrowConflict,
    LoanLimit,
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
impl From<buffer::Fault> for Error {
    fn from(e: buffer::Fault) -> Self {
        Self::Buffer(e)
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "map storage failure: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub loans: usize,
    pub lookup_work: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            loans: 256,
            lookup_work: 1_000_000,
        }
    }
}
/// Borrow-only key observation. Text is validated against the supplied
/// allocator on every lookup; copying this observation never retains an owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    I64(i64),
    Text([u8; 32]),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Entry {
    owner: a::Handle,
    version: u64,
    index: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Publication {
    Inserted,
    Existing(Entry),
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Spans {
    pub key: Option<m::Span>,
    pub value: Option<m::Span>,
}
#[derive(Clone, Copy)]
enum Part {
    Key,
    Value,
}
#[derive(Clone, Copy)]
struct Header {
    len: u64,
    capacity: u64,
    keys: Option<a::Handle>,
    values: Option<a::Handle>,
    metadata: Option<a::Handle>,
    tail: u64,
    loans: u64,
    active: u64,
    serial: u64,
    work: u64,
    version: u64,
}
struct Plan {
    requests: [a::Request; 3],
    key_stride: u64,
    value_stride: u64,
    metadata_stride: u64,
    value_meta: u64,
    record: u64,
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
fn word(bytes: &[u8], at: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(
        bytes
            .get(at..at + 8)
            .ok_or(Error::State)?
            .try_into()
            .map_err(|_| Error::State)?,
    ))
}
fn put(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn type_key(ty: o::Type) -> u64 {
    match ty {
        o::Type::I64 => 0,
        o::Type::Bool => 1,
        o::Type::Nominal(id) => 2 | (u64::from(id.0) << 32),
        o::Type::Constructed(id) => 3 | (u64::from(id.0) << 32),
    }
}
fn plan(key: &l::Layout, value: &l::Layout, capacity: u64) -> Result<Plan, Error> {
    if capacity > i64::MAX as u64 {
        return Err(a::Error::Overflow.into());
    }
    let key_stride = add(key.size, key.alignment - 1)? & !(key.alignment - 1);
    let value_stride = add(value.size, value.alignment - 1)? & !(value.alignment - 1);
    let record = add(key.metadata_bytes, value.metadata_bytes)?;
    let metadata_stride = add(record, 48)?;
    let work = mul(capacity, metadata_stride)?;
    Ok(Plan {
        requests: [
            a::Request::new(
                add(
                    work,
                    mul(key.work_slots.max(value.work_slots), l::WORK_ENTRY)?,
                )?,
                8,
            )?,
            a::Request::new(mul(capacity, key_stride)?, key.alignment)?,
            a::Request::new(mul(capacity, value_stride)?, value.alignment)?,
        ],
        key_stride,
        value_stride,
        metadata_stride,
        value_meta: key.metadata_bytes,
        record,
        work,
    })
}
struct Work {
    left: u64,
}
impl Work {
    fn charge(&mut self, n: u64) -> Result<(), Error> {
        self.left = self.left.checked_sub(n).ok_or(Error::WorkLimit)?;
        Ok(())
    }
}
enum Compared<'a> {
    I64(i64),
    Text(&'a [u8]),
}
pub struct Map<'l, 't> {
    layouts: &'l l::Layouts<'t>,
    key: o::Type,
    value: o::Type,
    owner: Option<a::Handle>,
}
pub struct View<'a, 'l, 't, P: a::Provider> {
    value: l::Reservation<'l, 't>,
    allocator: &'a a::Allocator<P>,
}
impl<P: a::Provider> View<'_, '_, '_, P> {
    pub fn root(&self) -> l::Location {
        self.value.root()
    }
    pub fn child(&self, at: l::Location, field: u32) -> Result<l::Location, Error> {
        Ok(self.value.child(at, field, self.allocator)?)
    }
    pub fn case(&self, at: l::Location) -> Result<u32, Error> {
        Ok(self.value.case(at, self.allocator)?)
    }
    pub fn read_immediate(&self, at: l::Location) -> Result<l::Immediate, Error> {
        Ok(self.value.read_immediate(at, self.allocator)?)
    }
    pub fn read_token(&self, at: l::Location) -> Result<l::Token, Error> {
        Ok(self.value.read_token(at, self.allocator)?)
    }
}
impl<'l, 't> Map<'l, 't> {
    pub fn new<P: a::Provider>(
        layouts: &'l l::Layouts<'t>,
        key: o::Type,
        value: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Self, Error> {
        Self::new_with_limits(layouts, key, value, Limits::default(), allocator)
    }
    pub fn new_with_limits<P: a::Provider>(
        layouts: &'l l::Layouts<'t>,
        key: o::Type,
        value: o::Type,
        limits: Limits,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Self, Error> {
        let text = matches!(key,o::Type::Constructed(id)if layouts.types().table().constructed.get(id.0 as usize).is_some_and(|t|t.constructor==o::Constructor::Builtin(o::Builtin::Text)));
        if key != o::Type::I64 && !text {
            return Err(Error::WrongKey);
        }
        layouts.get(key).ok_or(l::Error::InvalidType(key))?;
        let props = layouts
            .types()
            .properties(value)
            .ok_or(l::Error::InvalidType(value))?;
        if props.contains_loan || props.contains_allocator || props.contains_owner {
            return Err(l::Error::InvalidType(value).into());
        }
        let request = a::Request::new(add(HEADER, mul(limits.loans as u64, LOAN_ENTRY)?)?, 8)?;
        let owner = allocator.allocate(request)?;
        let bytes = allocator.bytes_mut(owner)?;
        bytes.fill(0);
        put(bytes, 0, MAGIC);
        put(bytes, 8, type_key(key));
        put(bytes, 16, type_key(value));
        put(bytes, 24, layouts.identity());
        put(bytes, 120, NONE);
        put(bytes, 128, limits.loans as u64);
        put(bytes, 144, 1);
        put(bytes, 152, limits.lookup_work);
        put(bytes, 160, 1);
        Ok(Self {
            layouts,
            key,
            value,
            owner: Some(owner),
        })
    }
    fn plan(&self, capacity: u64) -> Result<Plan, Error> {
        plan(
            self.layouts.get(self.key).unwrap(),
            self.layouts.get(self.value).unwrap(),
            capacity,
        )
    }
    fn inspect<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<Header, Error> {
        let owner = self.owner.ok_or(Error::Unavailable)?;
        let request = allocator.request(owner)?;
        if request.bytes() < HEADER || request.alignment() != 8 {
            return Err(Error::State);
        }
        let bytes = allocator.bytes(owner)?;
        if word(bytes, 0)? != MAGIC
            || word(bytes, 8)? != type_key(self.key)
            || word(bytes, 16)? != type_key(self.value)
            || word(bytes, 24)? != self.layouts.identity()
        {
            return Err(Error::State);
        }
        let len = word(bytes, 32)?;
        let capacity = word(bytes, 40)?;
        let tail = word(bytes, 120)?;
        let loans = word(bytes, 128)?;
        let active = word(bytes, 136)?;
        let serial = word(bytes, 144)?;
        let work = word(bytes, 152)?;
        let version = word(bytes, 160)?;
        if request != a::Request::new(add(HEADER, mul(loans, LOAN_ENTRY)?)?, 8)?
            || len > capacity
            || capacity > i64::MAX as u64
            || active > loans
            || version == 0
            || (len == 0 && tail != NONE)
            || (len != 0 && tail >= len)
        {
            return Err(Error::State);
        }
        let (keys, values, metadata) = if capacity == 0 {
            if bytes[48..120].iter().any(|b| *b != 0) {
                return Err(Error::State);
            }
            (None, None, None)
        } else {
            let decode = |at| {
                a::Handle::from_token(bytes[at..at + 24].try_into().unwrap()).map_err(Error::from)
            };
            let keys = decode(48)?;
            let values = decode(72)?;
            let metadata = decode(96)?;
            let p = self.plan(capacity)?;
            if keys == values
                || keys == metadata
                || values == metadata
                || [keys, values, metadata].contains(&owner)
                || allocator.request(metadata)? != p.requests[0]
                || allocator.request(keys)? != p.requests[1]
                || allocator.request(values)? != p.requests[2]
            {
                return Err(Error::State);
            }
            (Some(keys), Some(values), Some(metadata))
        };
        Ok(Header {
            len,
            capacity,
            keys,
            values,
            metadata,
            tail,
            loans,
            active,
            serial,
            work,
            version,
        })
    }
    fn element(&self, h: Header, index: u64, part: Part) -> Result<l::Reservation<'l, 't>, Error> {
        if index >= h.capacity {
            return Err(Error::State);
        }
        let p = self.plan(h.capacity)?;
        let (ty, data, offset, meta) = match part {
            Part::Key => (
                self.key,
                h.keys,
                mul(index, p.key_stride)?,
                mul(index, p.metadata_stride)?,
            ),
            Part::Value => (
                self.value,
                h.values,
                mul(index, p.value_stride)?,
                add(mul(index, p.metadata_stride)?, p.value_meta)?,
            ),
        };
        Ok(l::Reservation::element(
            self.layouts,
            ty,
            data.ok_or(Error::State)?,
            h.metadata.ok_or(Error::State)?,
            offset,
            meta,
            p.work,
        ))
    }
    fn key_at<P: a::Provider>(
        &self,
        value: &l::Reservation<'_, '_>,
        at: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<Key, Error> {
        if self.layouts.types().equivalent(at.ty(), self.key) != Some(true) {
            return Err(Error::WrongKey);
        }
        if self.key == o::Type::I64 {
            let l::Immediate::I64(n) = value.read_immediate(at, allocator)? else {
                return Err(Error::WrongKey);
            };
            Ok(Key::I64(n))
        } else {
            Ok(Key::Text(value.read_token(at, allocator)?.bytes))
        }
    }
    pub fn key_of<P: a::Provider>(
        &self,
        value: &l::Reservation<'_, '_>,
        at: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<Key, Error> {
        self.inspect(allocator)?;
        if !std::ptr::eq(self.layouts.types(), value.layouts().types()) {
            return Err(Error::WrongKey);
        }
        self.key_at(value, at, allocator)
    }
    fn compared<'a, P: a::Provider>(
        &self,
        key: Key,
        work: &mut Work,
        allocator: &'a a::Allocator<P>,
    ) -> Result<Compared<'a>, Error> {
        match key {
            Key::I64(n) if self.key == o::Type::I64 => Ok(Compared::I64(n)),
            Key::Text(token) if self.key != o::Type::I64 => {
                let bytes = buffer::text_token_bytes(token, allocator)?;
                work.charge(bytes.len() as u64)?;
                std::str::from_utf8(bytes).map_err(|_| Error::WrongKey)?;
                Ok(Compared::Text(bytes))
            }
            _ => Err(Error::WrongKey),
        }
    }
    pub fn lookup<P: a::Provider>(
        &self,
        key: Key,
        allocator: &a::Allocator<P>,
    ) -> Result<Option<Entry>, Error> {
        let h = self.inspect(allocator)?;
        let mut work = Work { left: h.work };
        let needle = self.compared(key, &mut work, allocator)?;
        for index in 0..h.len {
            work.charge(1)?;
            let cell = self.element(h, index, Part::Key)?;
            let stored = self.compared(
                self.key_at(&cell, cell.root(), allocator)?,
                &mut work,
                allocator,
            )?;
            let equal = match (&needle, stored) {
                (Compared::I64(a), Compared::I64(b)) => *a == b,
                (Compared::Text(a), Compared::Text(b)) => {
                    if a.len() == b.len() {
                        work.charge(b.len() as u64)?;
                        *a == b
                    } else {
                        false
                    }
                }
                _ => return Err(Error::WrongKey),
            };
            if equal {
                return Ok(Some(Entry {
                    owner: self.owner.unwrap(),
                    version: h.version,
                    index,
                }));
            }
        }
        Ok(None)
    }
    pub fn len<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<i64, Error> {
        Ok(self.inspect(allocator)?.len as i64)
    }
    pub fn is_empty<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<bool, Error> {
        Ok(self.len(allocator)? == 0)
    }
    pub fn view<'a, P: a::Provider>(
        &self,
        entry: Entry,
        allocator: &'a a::Allocator<P>,
    ) -> Result<View<'a, 'l, 't, P>, Error> {
        let h = self.check_entry(entry, allocator)?;
        Ok(View {
            value: self.element(h, entry.index, Part::Value)?,
            allocator,
        })
    }
    fn check_entry<P: a::Provider>(
        &self,
        entry: Entry,
        allocator: &a::Allocator<P>,
    ) -> Result<Header, Error> {
        let h = self.inspect(allocator)?;
        if Some(entry.owner) != self.owner || entry.version != h.version || entry.index >= h.len {
            return Err(Error::Unavailable);
        }
        Ok(h)
    }
    fn record(&self, h: Header, index: u64) -> Result<usize, Error> {
        if index >= h.capacity {
            return Err(Error::State);
        }
        let p = self.plan(h.capacity)?;
        Ok(add(mul(index, p.metadata_stride)?, p.record)? as usize)
    }
    /// Storage publication primitive. A present key returns its checked entry
    /// and leaves both incoming obligations unchanged for the replacement layer.
    /// Absence reserves all three regions before consuming either owning input.
    pub fn publish_if_absent<P: a::Provider>(
        &mut self,
        key: &mut l::Reservation<'l, 't>,
        value: &mut l::Reservation<'l, 't>,
        spans: Spans,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Publication, Error> {
        let h = self.inspect(allocator)?;
        if h.active != 0 {
            return Err(Error::BorrowConflict);
        }
        for (cell, ty) in [(&mut *key, self.key), (&mut *value, self.value)] {
            if !std::ptr::eq(self.layouts.types(), cell.layouts().types())
                || self.layouts.types().equivalent(cell.root().ty(), ty) != Some(true)
            {
                return Err(l::Error::InvalidType(cell.root().ty()).into());
            }
            cell.validate_retirement(allocator)?;
        }
        if [spans.key, spans.value]
            .into_iter()
            .flatten()
            .any(|s| !self.layouts.span_valid(s))
        {
            return Err(l::Error::InvalidSpan.into());
        }
        if let Some(entry) = self.lookup(self.key_at(key, key.root(), allocator)?, allocator)? {
            return Ok(Publication::Existing(entry));
        }
        let len = add(h.len, 1)?;
        let version = h.version.checked_add(1).ok_or(a::Error::Overflow)?;
        if len > i64::MAX as u64 {
            return Err(a::Error::Overflow.into());
        }
        let mut next = h;
        if len > h.capacity {
            let capacity = h.capacity.saturating_mul(2).min(i64::MAX as u64).max(len);
            let p = self.plan(capacity)?;
            allocator.check(&p.requests)?;
            let metadata = allocator.allocate(p.requests[0])?;
            let keys = match allocator.allocate(p.requests[1]) {
                Ok(h) => h,
                Err(e) => {
                    allocator.release(metadata)?;
                    return Err(e.into());
                }
            };
            let values = match allocator.allocate(p.requests[2]) {
                Ok(h) => h,
                Err(e) => {
                    allocator.release(keys)?;
                    allocator.release(metadata)?;
                    return Err(e.into());
                }
            };
            for region in [metadata, keys, values] {
                allocator.bytes_mut(region)?.fill(0);
            }
            if let Some(from) = h.keys {
                allocator.copy_bytes(from, 0, keys, 0, mul(h.len, p.key_stride)? as usize)?;
            }
            if let Some(from) = h.values {
                allocator.copy_bytes(from, 0, values, 0, mul(h.len, p.value_stride)? as usize)?;
            }
            if let Some(from) = h.metadata {
                allocator.copy_bytes(
                    from,
                    0,
                    metadata,
                    0,
                    mul(h.len, p.metadata_stride)? as usize,
                )?;
            }
            next = Header {
                capacity,
                keys: Some(keys),
                values: Some(values),
                metadata: Some(metadata),
                ..h
            };
        }
        for (cell, part, ty) in [(key, Part::Key, self.key), (value, Part::Value, self.value)] {
            let to = self.element(next, h.len, part)?;
            if self.layouts.types().properties(ty).unwrap().is_copy {
                to.copy_from(to.root(), cell, cell.root(), allocator)?;
            } else {
                cell.move_root_into(&to, to.root(), allocator)?;
            }
        }
        let at = self.record(next, h.len)?;
        let bytes = allocator.bytes_mut(next.metadata.unwrap())?;
        store_span(bytes, at, spans.key);
        store_span(bytes, at + 16, spans.value);
        put(bytes, at + 32, h.tail);
        put(bytes, at + 40, NONE);
        if h.tail != NONE {
            let at = self.record(next, h.tail)?;
            put(allocator.bytes_mut(next.metadata.unwrap())?, at + 40, h.len);
        }
        let bytes = allocator.bytes_mut(self.owner.unwrap())?;
        put(bytes, 32, len);
        put(bytes, 40, next.capacity);
        bytes[48..72].copy_from_slice(&next.keys.unwrap().token());
        bytes[72..96].copy_from_slice(&next.values.unwrap().token());
        bytes[96..120].copy_from_slice(&next.metadata.unwrap().token());
        put(bytes, 120, h.len);
        put(bytes, 160, version);
        if next.capacity != h.capacity {
            for handle in [h.keys, h.values, h.metadata].into_iter().flatten() {
                allocator.release(handle)?;
            }
        }
        Ok(Publication::Inserted)
    }
    pub fn take_token<P: a::Provider>(
        &mut self,
        allocator: &a::Allocator<P>,
    ) -> Result<[u8; 32], Error> {
        if self.inspect(allocator)?.active != 0 {
            return Err(Error::BorrowConflict);
        }
        let mut bytes = [0; 32];
        bytes[..24].copy_from_slice(&self.owner.take().unwrap().token());
        bytes[24..].copy_from_slice(&MAGIC.to_le_bytes());
        Ok(bytes)
    }
    pub fn from_token<P: a::Provider>(
        layouts: &'l l::Layouts<'t>,
        key: o::Type,
        value: o::Type,
        bytes: &mut [u8; 32],
        allocator: &a::Allocator<P>,
    ) -> Result<Self, Error> {
        if word(bytes, 24)? != MAGIC || layouts.get(key).is_none() || layouts.get(value).is_none() {
            return Err(Error::State);
        }
        let owner = Some(a::Handle::from_token(bytes[..24].try_into().unwrap())?);
        let map = Self {
            layouts,
            key,
            value,
            owner,
        };
        map.inspect(allocator)?;
        bytes.fill(0);
        Ok(map)
    }
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
        put(allocator.bytes_mut(self.owner.unwrap())?, 0, MAGIC + 1);
        let mut index = h.tail;
        for remaining in (0..h.len).rev() {
            if index >= h.len {
                return Err(Error::State);
            }
            let at = self.record(h, index)?;
            let previous = word(allocator.bytes(h.metadata.unwrap())?, at + 32)?;
            put(allocator.bytes_mut(self.owner.unwrap())?, 32, remaining);
            for (part, span_at) in [(Part::Value, at + 16), (Part::Key, at)] {
                let span = read_span(allocator.bytes(h.metadata.unwrap())?, span_at)?;
                let mut cell = self.element(h, index, part)?;
                cell.begin_retirement(span, allocator)?;
                while let Some(leaf) = cell.next_leaf(allocator)? {
                    let offset = leaf.offset as usize;
                    let token = l::Token {
                        leaf,
                        bytes: cell.data(allocator)?[offset..offset + 32]
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
                    cell.complete_leaf()?;
                }
            }
            index = previous;
        }
        if index != NONE {
            return Err(Error::State);
        }
        for handle in [h.keys, h.values, h.metadata].into_iter().flatten() {
            allocator.release(handle)?;
        }
        allocator.release(self.owner.take().unwrap())?;
        Ok(())
    }
}
fn store_span(bytes: &mut [u8], at: usize, span: Option<m::Span>) {
    bytes[at..at + 16].fill(0);
    if let Some(s) = span {
        bytes[at..at + 4].copy_from_slice(&s.source.0.to_le_bytes());
        bytes[at + 4..at + 8].copy_from_slice(&s.start.to_le_bytes());
        bytes[at + 8..at + 12].copy_from_slice(&s.end.to_le_bytes());
        bytes[at + 12..at + 16].copy_from_slice(&1u32.to_le_bytes());
    }
}
fn read_span(bytes: &[u8], at: usize) -> Result<Option<m::Span>, Error> {
    let get = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    match get(at + 12) {
        0 => Ok(None),
        1 => Ok(Some(m::Span {
            source: m::SourceId(get(at)),
            start: get(at + 4),
            end: get(at + 8),
        })),
        _ => Err(Error::State),
    }
}
