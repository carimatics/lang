//! Typed shared control/payload storage under the instance allocator.
//!
//! This is the storage boundary for the production graph/token adapter, not a
//! replacement for its topology proof. The adapter must validate nested tokens
//! and cycles before publication and serialize count/publication transitions.
//! Provider calls and detached guest cleanup must run outside topology locks.
//! Wrapper disposal alone runs no guest cleanup, like `layout::Reservation`.
use crate::{allocation as a, cleanup as c, layout as l};
use lang_mir::unit as m;
use std::fmt::{Display, Formatter};

const HEADER_BYTES: u64 = 56;
const MAGIC: u64 = 0x7368_6172_6564_326d;
const LIVE: u64 = 1;
const RETIRING: u64 = 2;
const DEAD: u64 = 3;

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    Allocation(a::Error),
    Layout(l::Error),
    Unavailable,
    ExpiredWeak,
    BorrowConflict,
    CountOverflow,
    State,
}
impl From<a::Error> for Error {
    fn from(error: a::Error) -> Self {
        Self::Allocation(error)
    }
}
impl From<l::Error> for Error {
    fn from(error: l::Error) -> Self {
        match error {
            l::Error::Allocation(error) => Self::Allocation(error),
            error => Self::Layout(error),
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "shared storage failure: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Counts {
    /// Includes retained read leases.
    pub strong: u64,
    pub weak: u64,
    pub readers: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Release {
    Retained,
    /// Access is revoked. The adapter must drain the detached obligation.
    Retire,
}

/// A fixed wrapper; control counts/span and payload/order/retirement work live
/// in actual guest reservations. No Arc or host allocation occurs in methods.
pub struct Block<'l, 't> {
    header: Option<a::Handle>,
    payload: Option<l::Reservation<'l, 't>>,
}
/// The graph can keep its control block addressable while draining this value
/// outside its publication lock, including weak back-edges to the same block.
pub struct Retired<'l, 't> {
    owner: a::Handle,
    payload: l::Reservation<'l, 't>,
    span: Option<m::Span>,
    started: bool,
}
/// Only a complete drain can authorize final control-block retirement.
pub struct Completed {
    owner: a::Handle,
}
impl Retired<'_, '_> {
    /// Seed an iterative graph drain once. The adapter can pause after each
    /// acknowledged leaf to completely retire its child before its next sibling.
    pub fn begin<P: a::Provider>(&mut self, allocator: &mut a::Allocator<P>) -> Result<(), Error> {
        if self.started {
            return Err(Error::State);
        }
        allocator.request(self.owner)?;
        self.payload.begin_retirement(self.span, allocator)?;
        self.started = true;
        Ok(())
    }
    pub fn next_token<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<l::Token>, Error> {
        if !self.started {
            return Err(Error::State);
        }
        let Some(leaf) = self.payload.next_leaf(allocator)? else {
            return Ok(None);
        };
        let start = leaf.offset as usize;
        let bytes = self.payload.data(allocator)?[start..start + 32]
            .try_into()
            .expect("validated token extent");
        Ok(Some(l::Token { leaf, bytes }))
    }
    pub fn complete_leaf(&mut self) -> Result<(), Error> {
        Ok(self.payload.complete_leaf()?)
    }
    pub fn finish<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Completed, Error> {
        if !self.started {
            return Err(Error::State);
        }
        self.payload.release_storage(allocator)?;
        Ok(Completed { owner: self.owner })
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
    ) -> Result<Completed, Error> {
        self.begin(allocator)?;
        while let Some(token) = self.next_token(allocator)? {
            let span = token.leaf.span;
            let mut callback = c::Contextual::new(
                |token: &mut l::Token, reason, context: &mut c::Context<'_>| {
                    cleaner(token, allocator, reason, context)
                },
            );
            context.field(token, span, &mut callback);
            self.complete_leaf()?;
        }
        self.finish(allocator)
    }
}
/// Checked read surface; structural locations cannot expose mutable payloads.
pub struct View<'b, 'l, 't> {
    payload: &'b l::Reservation<'l, 't>,
}
impl<'l, 't> View<'_, 'l, 't> {
    pub(crate) fn reservation(&self) -> &l::Reservation<'l, 't> {
        self.payload
    }
    pub fn read_token<P: a::Provider>(
        &self,
        location: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Token, l::Error> {
        self.payload.read_token(location, allocator)
    }
    pub fn initialized_field<P: a::Provider>(
        &self,
        location: l::Location,
        rank: u32,
        allocator: &a::Allocator<P>,
    ) -> Result<(u32, Option<m::Span>), l::Error> {
        self.payload.initialized_field(location, rank, allocator)
    }
    pub fn copy_to<P: a::Provider>(
        &self,
        location: l::Location,
        destination: &l::Reservation<'_, '_>,
        target: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), l::Error> {
        destination.copy_from(target, self.payload, location, allocator)
    }
    pub fn root(&self) -> l::Location {
        self.payload.root()
    }
    pub fn data<'a, P: a::Provider>(
        &self,
        allocator: &'a a::Allocator<P>,
    ) -> Result<&'a [u8], l::Error> {
        self.payload.data(allocator)
    }
    pub fn child<P: a::Provider>(
        &self,
        location: l::Location,
        field: u32,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Location, l::Error> {
        self.payload.child(location, field, allocator)
    }
    pub fn read_immediate<P: a::Provider>(
        &self,
        location: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Immediate, l::Error> {
        self.payload.read_immediate(location, allocator)
    }
    pub fn case<P: a::Provider>(
        &self,
        location: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<u32, l::Error> {
        self.payload.case(location, allocator)
    }
}
impl<'l, 't> Block<'l, 't> {
    /// Reserve the complete control/data/order/work plan before moving input.
    /// Failure leaves its obligation and all published graph state unchanged.
    /// Topology validation is the calling adapter's separate precondition.
    pub fn allocate_from<P: a::Provider>(
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Self, Error> {
        Self::allocate(input, span, allocator, false)
    }
    /// Preserve a Copy operand; owning values require `allocate_from`.
    pub fn allocate_copy<P: a::Provider>(
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Self, Error> {
        Self::allocate(input, span, allocator, true)
    }
    fn allocate<P: a::Provider>(
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
        copy: bool,
    ) -> Result<Self, Error> {
        let layouts = input.layouts();
        let ty = input.root().ty();
        if span.is_some_and(|span| !layouts.span_valid(span)) {
            return Err(l::Error::InvalidSpan.into());
        }
        if copy && !layouts.types().properties(ty).is_some_and(|p| p.is_copy) {
            return Err(l::Error::InvalidType(ty).into());
        }
        input.validate_retirement(allocator)?;
        let requests = layouts
            .get(ty)
            .ok_or(l::Error::InvalidType(ty))?
            .storage_requests()?;
        let control = a::Request::new(HEADER_BYTES, 8)?;
        allocator.check(&[control, requests[0], requests[1]])?;
        let header = allocator.allocate(control)?;
        let mut payload = match l::Reservation::allocate(layouts, ty, allocator) {
            Ok(payload) => payload,
            Err(error) => {
                allocator.release(header)?;
                return Err(error.into());
            }
        };
        let bytes = allocator.bytes_mut(header)?;
        bytes.fill(0);
        put(bytes, 0, MAGIC);
        put(bytes, 8, LIVE);
        put(bytes, 16, 1);
        store_span(bytes, span);
        let transfer = if copy {
            payload.copy_from(payload.root(), input, input.root(), allocator)
        } else {
            input.move_root_into(&payload, payload.root(), allocator)
        };
        if let Err(error) = transfer {
            payload.release_storage(allocator)?;
            allocator.release(header)?;
            return Err(error.into());
        }
        Ok(Self {
            header: Some(header),
            payload: Some(payload),
        })
    }
    /// Private in-memory identity, not a wire format or ownership ticket.
    pub fn identity(&self) -> Result<a::Handle, Error> {
        self.header.ok_or(Error::Unavailable)
    }
    fn header<'a, P: a::Provider>(
        &self,
        allocator: &'a a::Allocator<P>,
    ) -> Result<&'a [u8], Error> {
        let bytes = allocator.bytes(self.identity()?)?;
        if bytes.len() != HEADER_BYTES as usize || get(bytes, 0) != MAGIC {
            return Err(Error::State);
        }
        Ok(bytes)
    }
    pub fn counts<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<Counts, Error> {
        let bytes = self.header(allocator)?;
        Ok(Counts {
            strong: get(bytes, 16),
            weak: get(bytes, 24),
            readers: get(bytes, 32),
        })
    }
    fn live<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<(), Error> {
        if get(self.header(allocator)?, 8) != LIVE {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn span<P: a::Provider>(
        &self,
        allocator: &a::Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        Ok(load_span(self.header(allocator)?))
    }
    /// Trusted graph adapters use this only under a validated live read/loan.
    pub fn payload<P: a::Provider>(
        &self,
        allocator: &a::Allocator<P>,
    ) -> Result<View<'_, 'l, 't>, Error> {
        self.live(allocator)?;
        Ok(View {
            payload: self.payload.as_ref().ok_or(Error::State)?,
        })
    }
    pub fn retain_strong<P: a::Provider>(
        &mut self,
        maximum: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.live(allocator)?;
        let count = increment(self.counts(allocator)?.strong, maximum)?;
        put(allocator.bytes_mut(self.identity()?)?, 16, count);
        Ok(())
    }
    pub fn downgrade<P: a::Provider>(
        &mut self,
        maximum: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.live(allocator)?;
        self.increment_weak(maximum, allocator)
    }
    pub fn retain_weak<P: a::Provider>(
        &mut self,
        maximum: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        if self.counts(allocator)?.weak == 0 {
            return Err(Error::State);
        }
        self.increment_weak(maximum, allocator)
    }
    fn increment_weak<P: a::Provider>(
        &mut self,
        maximum: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let count = increment(self.counts(allocator)?.weak, maximum)?;
        put(allocator.bytes_mut(self.identity()?)?, 24, count);
        Ok(())
    }
    pub fn upgrade<P: a::Provider>(
        &mut self,
        maximum: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        if self.counts(allocator)?.weak == 0 {
            return Err(Error::State);
        }
        if get(self.header(allocator)?, 8) != LIVE {
            return Err(Error::ExpiredWeak);
        }
        self.retain_strong(maximum, allocator)
    }
    /// Check both increments before changing either count. A read lease keeps
    /// payload alive even after all external strong owners are released.
    pub fn begin_read<P: a::Provider>(
        &mut self,
        maximum: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.live(allocator)?;
        let counts = self.counts(allocator)?;
        let strong = increment(counts.strong, maximum)?;
        let readers = increment(counts.readers, maximum)?;
        let bytes = allocator.bytes_mut(self.identity()?)?;
        put(bytes, 16, strong);
        put(bytes, 32, readers);
        Ok(())
    }
    pub fn release_strong<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Release, Error> {
        self.live(allocator)?;
        let counts = self.counts(allocator)?;
        // Read leases cannot be consumed as external strong owners.
        if counts.strong <= counts.readers {
            return Err(Error::State);
        }
        self.release_count(counts.strong - 1, allocator)
    }
    pub fn end_read<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Release, Error> {
        self.live(allocator)?;
        let counts = self.counts(allocator)?;
        if counts.readers == 0 || counts.strong < counts.readers {
            return Err(Error::State);
        }
        put(
            allocator.bytes_mut(self.identity()?)?,
            32,
            counts.readers - 1,
        );
        self.release_count(counts.strong - 1, allocator)
    }
    fn release_count<P: a::Provider>(
        &mut self,
        strong: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Release, Error> {
        let bytes = allocator.bytes_mut(self.identity()?)?;
        put(bytes, 16, strong);
        if strong != 0 {
            return Ok(Release::Retained);
        }
        put(bytes, 8, RETIRING);
        Ok(Release::Retire)
    }
    /// Exchange complete roots without allocation. The old payload stays in
    /// `input` for cleanup after graph publication, outside any topology lock.
    pub fn replace_from<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        self.live(allocator)?;
        if self.counts(allocator)?.readers != 0 {
            return Err(Error::BorrowConflict);
        }
        let old = self.span(allocator)?;
        let payload = self.payload.as_mut().ok_or(Error::State)?;
        payload.exchange_root_at(payload.root(), input, span, allocator)?;
        store_span(allocator.bytes_mut(self.identity()?)?, Some(span));
        Ok(old)
    }
    pub fn replace_copy<P: a::Provider>(
        &mut self,
        input: &l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.live(allocator)?;
        if self.counts(allocator)?.readers != 0 {
            return Err(Error::BorrowConflict);
        }
        let payload = self.payload.as_ref().ok_or(Error::State)?;
        if !payload.layouts().span_valid(span) {
            return Err(l::Error::InvalidSpan.into());
        }
        payload.copy_from(payload.root(), input, input.root(), allocator)?;
        store_span(allocator.bytes_mut(self.identity()?)?, Some(span));
        Ok(())
    }
    /// Revoke under the graph lock, detach, unlock, drain, then complete. The
    /// control block cannot be reclaimed while the detached payload is pending.
    pub fn detach<P: a::Provider>(
        &mut self,
        allocator: &a::Allocator<P>,
    ) -> Result<Retired<'l, 't>, Error> {
        if get(self.header(allocator)?, 8) != RETIRING {
            return Err(Error::State);
        }
        let span = self.span(allocator)?;
        Ok(Retired {
            owner: self.identity()?,
            payload: self.payload.take().ok_or(Error::State)?,
            span,
            started: false,
        })
    }
    pub fn complete_retirement<P: a::Provider>(
        &mut self,
        completed: Completed,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        if completed.owner != self.identity()?
            || self.payload.is_some()
            || get(self.header(allocator)?, 8) != RETIRING
        {
            return Err(Error::State);
        }
        put(allocator.bytes_mut(self.identity()?)?, 8, DEAD);
        if self.counts(allocator)?.weak == 0 {
            self.release_header(allocator)?;
        }
        Ok(())
    }
    /// Convenience for adapters without a graph lock. Child failure/panic is
    /// contained by the shared context; every independent leaf still drains.
    pub fn cleanup<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        let mut retired = self.detach(allocator)?;
        let completed = retired.cleanup(allocator, context, cleaner)?;
        self.complete_retirement(completed, allocator)
    }
    pub fn release_weak<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let count = self
            .counts(allocator)?
            .weak
            .checked_sub(1)
            .ok_or(Error::State)?;
        let bytes = allocator.bytes_mut(self.identity()?)?;
        put(bytes, 24, count);
        if count == 0 && get(bytes, 8) == DEAD {
            self.release_header(allocator)?;
        }
        Ok(())
    }
    fn release_header<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let header = self.identity()?;
        allocator.release(header)?;
        self.header = None;
        Ok(())
    }
}
fn increment(value: u64, maximum: u64) -> Result<u64, Error> {
    value
        .checked_add(1)
        .filter(|value| *value <= maximum)
        .ok_or(Error::CountOverflow)
}
fn get(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed control word"),
    )
}
fn put(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn store_span(bytes: &mut [u8], span: Option<m::Span>) {
    bytes[40..56].fill(0);
    if let Some(span) = span {
        for (offset, value) in [
            (40, span.source.0),
            (44, span.start),
            (48, span.end),
            (52, 1),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
}
fn load_span(bytes: &[u8]) -> Option<m::Span> {
    let word = |offset| {
        u32::from_le_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("fixed span word"),
        )
    };
    (word(52) == 1).then(|| m::Span {
        source: m::SourceId(word(40)),
        start: word(44),
        end: word(48),
    })
}
