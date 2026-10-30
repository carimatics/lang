//! Fallible instance storage allocation under RFC 0008.
//!
//! Every request is exact requested storage, whether guest data or required
//! guest-controlled metadata. Source layout/container adapters must request all
//! such storage before publication. The fixed locator table is trusted instance
//! context reserved before entry; no locator allocation occurs in guest calls.
//! This managed Rust provider is not the raw C allocator bridge.
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};

mod trace;
pub use trace::{
    Event, EventKind, FailureKind, MAX_TRACE_EVENTS, Trace, TraceError, TraceSnapshot,
};

static NEXT_ALLOCATOR: AtomicU64 = AtomicU64::new(1);
pub const MAX_ALIGNMENT: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Identity(u64);
impl Identity {
    pub const fn get(self) -> u64 {
        self.0
    }
}
/// Runtime identity, not a source Copy classification or allocator capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Handle {
    allocator: Identity,
    slot: u32,
    generation: u64,
}
impl Handle {
    /// Toolchain-private managed token, never an artifact/ABI encoding.
    pub(crate) fn token(self) -> [u8; 24] {
        let mut bytes = [0; 24];
        bytes[..8].copy_from_slice(&self.allocator.0.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.slot.to_le_bytes());
        bytes[16..].copy_from_slice(&self.generation.to_le_bytes());
        bytes
    }
    pub(crate) fn from_token(bytes: [u8; 24]) -> Result<Self, Error> {
        if bytes[12..16] != [0; 4] {
            return Err(Error::InvalidStorage);
        }
        let allocator = u64::from_le_bytes(bytes[..8].try_into().expect("fixed token"));
        let slot = u32::from_le_bytes(bytes[8..12].try_into().expect("fixed token"));
        let generation = u64::from_le_bytes(bytes[16..].try_into().expect("fixed token"));
        if allocator == 0 || generation == 0 {
            return Err(Error::StaleHandle);
        }
        Ok(Self {
            allocator: Identity(allocator),
            slot,
            generation,
        })
    }
    pub const fn allocator(self) -> Identity {
        self.allocator
    }
    pub const fn generation(self) -> u64 {
        self.generation
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    bytes: u64,
    alignment: u64,
}
impl Request {
    pub fn new(bytes: u64, alignment: u64) -> Result<Self, Error> {
        if alignment == 0 || !alignment.is_power_of_two() || alignment > MAX_ALIGNMENT {
            return Err(Error::InvalidAlignment(alignment));
        }
        let size = usize::try_from(bytes).map_err(|_| Error::Overflow)?;
        if bytes > i64::MAX as u64
            || (bytes != 0
                && size
                    .checked_add(alignment as usize - 1)
                    .is_none_or(|n| n > isize::MAX as usize))
        {
            return Err(Error::Overflow);
        }
        Ok(Self { bytes, alignment })
    }
    pub fn elements(length: i64, element_bytes: u64, alignment: u64) -> Result<Self, Error> {
        let length = u64::try_from(length).map_err(|_| Error::Overflow)?;
        Self::new(
            length.checked_mul(element_bytes).ok_or(Error::Overflow)?,
            alignment,
        )
    }
    pub const fn bytes(self) -> u64 {
        self.bytes
    }
    pub const fn alignment(self) -> u64 {
        self.alignment
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub request_bytes: u64,
    pub live_bytes: u64,
    pub cumulative_bytes: u64,
    pub live_allocations: usize,
    pub alignment: u64,
}

/// Temporarily lower the original allocator's limits without resetting usage.
/// Drop restores only the policy; storage, identity and counters stay original.
pub struct LimitScope<'a, P: Provider> {
    allocator: &'a mut Allocator<P>,
    previous: Limits,
}
impl<P: Provider> std::ops::Deref for LimitScope<'_, P> {
    type Target = Allocator<P>;
    fn deref(&self) -> &Self::Target {
        self.allocator
    }
}
impl<P: Provider> std::ops::DerefMut for LimitScope<'_, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.allocator
    }
}
impl<P: Provider> Drop for LimitScope<'_, P> {
    fn drop(&mut self) {
        self.allocator.limits = self.previous;
    }
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            request_bytes: 64 * 1024 * 1024,
            live_bytes: 64 * 1024 * 1024,
            cumulative_bytes: 256 * 1024 * 1024,
            live_allocations: 65_536,
            alignment: MAX_ALIGNMENT,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Limit {
    RequestBytes,
    LiveBytes,
    CumulativeBytes,
    LiveAllocations,
    Alignment,
}
/// The exact rejected check, retained before effects and mandatory cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LimitViolation {
    pub kind: Limit,
    pub limit: u64,
    pub attempted: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    MissingAuthority,
    BorrowedProvider,
    InUse { allocations: usize },
    ForeignAllocator,
    StaleHandle,
    InvalidAlignment(u64),
    InvalidLimits,
    Overflow,
    Limit(LimitViolation),
    OutOfMemory,
    InvalidStorage,
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "instance allocation failed: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    pub live_bytes: u64,
    pub cumulative_bytes: u64,
    pub peak_live_bytes: u64,
    pub live_allocations: usize,
    pub peak_live_allocations: usize,
    /// Authorized attempts, including zero-size requests and provider OOM.
    pub requests: u64,
    pub allocations: u64,
    pub deallocations: u64,
}
/// A provider region remains uniquely owned, writable, at a stable address,
/// with the same byte range for shared/mutable access until deallocation.
pub trait Region {
    fn bytes(&self) -> &[u8];
    fn bytes_mut(&mut self) -> &mut [u8];
}
/// Trusted host primitive. Allocation returns a region or recoverable OOM
/// without unwinding/reentering. Deallocation must match the original exact
/// request and must not fail, unwind, retain storage or reenter the instance.
/// Callers invoke these methods outside topology/instance locks. Raw C pointer
/// validity/uniqueness/lifetime remain the foreign provider's trusted contract.
pub trait Provider {
    type Region: Region;
    fn allocate(&mut self, request: Request) -> Option<Self::Region>;
    fn deallocate(&mut self, region: Self::Region, request: Request);
    /// Scoped views lend existing storage; ordinary providers return None.
    fn allocator_view(&self) -> Option<&dyn Memory> {
        None
    }
    fn allocator_view_mut(&mut self) -> Option<&mut dyn Memory> {
        None
    }
}

/// Erased access to the original allocator. A view delegates every operation;
/// neither this trait nor a handle manufactures another allocation authority.
pub trait Memory {
    fn identity(&self) -> Identity;
    fn limits(&self) -> Limits;
    fn stats(&self) -> Stats;
    fn preflight(&self) -> Result<(), Error>;
    fn check(&self, requests: &[Request]) -> Result<(), Error>;
    fn allocate(&mut self, request: Request) -> Result<Handle, Error>;
    fn request(&self, handle: Handle) -> Result<Request, Error>;
    fn bytes(&self, handle: Handle) -> Result<&[u8], Error>;
    fn bytes_mut(&mut self, handle: Handle) -> Result<&mut [u8], Error>;
    fn release(&mut self, handle: Handle) -> Result<(), Error>;
    fn resize(&mut self, handle: &mut Handle, bytes: u64) -> Result<(), Error>;
    fn copy_bytes(
        &mut self,
        source: Handle,
        from: usize,
        target: Handle,
        to: usize,
        len: usize,
    ) -> Result<(), Error>;
    fn swap_bytes(
        &mut self,
        a: Handle,
        from: usize,
        b: Handle,
        to: usize,
        len: usize,
    ) -> Result<(), Error>;
}

/// Lifetime-bearing capability used only by Allocator::borrow. It owns no
/// region and cannot be passed to the ordinary allocator constructor.
pub struct Borrowed<'a> {
    memory: &'a mut dyn Memory,
}
pub enum NoRegion {}
impl Region for NoRegion {
    fn bytes(&self) -> &[u8] {
        match *self {}
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        match *self {}
    }
}
impl Provider for Borrowed<'_> {
    type Region = NoRegion;
    fn allocate(&mut self, _: Request) -> Option<NoRegion> {
        None
    }
    fn deallocate(&mut self, region: NoRegion, _: Request) {
        match region {}
    }
    fn allocator_view(&self) -> Option<&dyn Memory> {
        Some(&*self.memory)
    }
    fn allocator_view_mut(&mut self) -> Option<&mut dyn Memory> {
        Some(&mut *self.memory)
    }
}

/// A safe Rust implementation with aligned views into fallibly reserved bytes.
/// Padding/capacity are host allocator overhead, not requested guest storage.
pub struct Storage {
    bytes: Vec<u8>,
    offset: usize,
    len: usize,
}
impl Storage {
    pub fn try_new(request: Request) -> Result<Self, Error> {
        let len = request.bytes as usize;
        if len == 0 {
            return Ok(Self {
                bytes: Vec::new(),
                offset: 0,
                len: 0,
            });
        }
        let total = len
            .checked_add(request.alignment as usize - 1)
            .ok_or(Error::Overflow)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total)
            .map_err(|_| Error::OutOfMemory)?;
        bytes.resize(total, 0);
        let mask = request.alignment as usize - 1;
        let offset = (request.alignment as usize - (bytes.as_ptr().addr() & mask)) & mask;
        Ok(Self { bytes, offset, len })
    }
}
impl Region for Storage {
    fn bytes(&self) -> &[u8] {
        &self.bytes[self.offset..self.offset + self.len]
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes[self.offset..self.offset + self.len]
    }
}
#[derive(Default)]
pub struct System;
impl Provider for System {
    type Region = Storage;
    fn allocate(&mut self, request: Request) -> Option<Storage> {
        Storage::try_new(request).ok()
    }
    fn deallocate(&mut self, region: Storage, _: Request) {
        drop(region);
    }
}

struct Allocation<R> {
    region: Option<R>,
    request: Request,
    generation: u64,
}
pub struct Allocator<P: Provider> {
    id: Identity,
    provider: Option<P>,
    slots: Vec<Option<Allocation<P::Region>>>,
    free: Vec<u32>,
    limits: Limits,
    stats: Stats,
    next_generation: Option<u64>,
    trace: Option<std::sync::Arc<Trace>>,
}
impl<P: Provider> Allocator<P> {
    /// Host/frame setup. Reserve all locator slots before any guest entry.
    pub fn new(provider: P, limits: Limits) -> Result<Self, Error> {
        if provider.allocator_view().is_some() {
            return Err(Error::BorrowedProvider);
        }
        if limits.alignment == 0 || limits.alignment > MAX_ALIGNMENT {
            return Err(Error::InvalidLimits);
        }
        u32::try_from(limits.live_allocations).map_err(|_| Error::Overflow)?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(limits.live_allocations)
            .map_err(|_| Error::OutOfMemory)?;
        slots.resize_with(limits.live_allocations, || None);
        let mut free = Vec::new();
        free.try_reserve_exact(limits.live_allocations)
            .map_err(|_| Error::OutOfMemory)?;
        free.extend((0..limits.live_allocations as u32).rev());
        let id = NEXT_ALLOCATOR
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map(Identity)
            .map_err(|_| Error::Overflow)?;
        Ok(Self {
            id,
            provider: Some(provider),
            slots,
            free,
            limits,
            stats: Stats::default(),
            next_generation: Some(1),
            trace: None,
        })
    }
    /// Missing/revoked authority is rejected before symbol lookup or callbacks.
    pub fn preflight(authority: Option<&Self>) -> Result<(), Error> {
        if authority.is_some_and(|authority| authority.provider.is_some()) {
            Ok(())
        } else {
            Err(Error::MissingAuthority)
        }
    }
    pub const fn identity(&self) -> Identity {
        self.id
    }
    /// Reserve bounded observation metadata before the first admitted request.
    /// The returned receipt has no allocator or guest storage authority.
    pub fn start_trace(&mut self, limit: usize) -> Result<std::sync::Arc<Trace>, TraceError> {
        if self.trace.is_some() {
            return Err(TraceError::AlreadyTracing);
        }
        let provider = self.provider.as_ref().ok_or(TraceError::MissingAuthority)?;
        if provider.allocator_view().is_some() {
            return Err(TraceError::BorrowedAllocator);
        }
        if self.stats != Stats::default() {
            return Err(TraceError::NotFresh);
        }
        let trace = std::sync::Arc::new(Trace::new(self.id, limit)?);
        self.trace = Some(std::sync::Arc::clone(&trace));
        Ok(trace)
    }
    fn trace_event(&self, kind: EventKind, request: Request) {
        if let Some(trace) = &self.trace {
            trace.record(Event {
                kind,
                request,
                stats: self.stats,
            });
        }
    }
    pub fn provider(&self) -> Option<&P> {
        self.provider.as_ref()
    }
    pub fn stats(&self) -> Stats {
        self.provider
            .as_ref()
            .and_then(Provider::allocator_view)
            .map_or(self.stats, Memory::stats)
    }
    pub const fn limits(&self) -> Limits {
        self.limits
    }
    /// Constrain a storage-owning allocator for one synchronous invocation.
    /// Borrowed views cannot substitute policy for their original allocator.
    /// Existing regions remain releasable even when their usage exceeds a new cap.
    pub fn limit_scope(&mut self, ceiling: Limits) -> Result<LimitScope<'_, P>, Error> {
        if self
            .provider
            .as_ref()
            .and_then(Provider::allocator_view)
            .is_some()
        {
            return Err(Error::BorrowedProvider);
        }
        let previous = self.limits;
        self.limits = Limits {
            request_bytes: previous.request_bytes.min(ceiling.request_bytes),
            live_bytes: previous.live_bytes.min(ceiling.live_bytes),
            cumulative_bytes: previous.cumulative_bytes.min(ceiling.cumulative_bytes),
            live_allocations: previous.live_allocations.min(ceiling.live_allocations),
            alignment: previous.alignment.min(ceiling.alignment),
        };
        Ok(LimitScope {
            allocator: self,
            previous,
        })
    }
    pub fn revoke(&mut self) -> Result<P, Error> {
        if self.stats().live_allocations != 0 {
            return Err(Error::InUse {
                allocations: self.stats().live_allocations,
            });
        }
        self.stats = self.stats();
        self.provider.take().ok_or(Error::MissingAuthority)
    }
    fn projected(&self, requests: &[Request]) -> Result<(u64, u64, usize), Error> {
        Self::preflight(Some(self))?;
        let mut live = self.stats.live_bytes;
        let mut cumulative = self.stats.cumulative_bytes;
        let count = self
            .stats
            .live_allocations
            .checked_add(requests.len())
            .ok_or(Error::Overflow)?;
        if count > self.limits.live_allocations {
            return Err(Error::Limit(LimitViolation {
                kind: Limit::LiveAllocations,
                limit: self.limits.live_allocations as u64,
                attempted: count as u64,
            }));
        }
        if !requests.is_empty() {
            let available = self.next_generation.ok_or(Error::Overflow)?;
            if available.checked_add(requests.len() as u64 - 1).is_none() {
                return Err(Error::Overflow);
            }
        }
        self.stats
            .requests
            .checked_add(requests.len() as u64)
            .ok_or(Error::Overflow)?;
        self.stats
            .allocations
            .checked_add(requests.len() as u64)
            .ok_or(Error::Overflow)?;
        // Reserve room for eventual exact-once releases before calling a provider.
        self.stats
            .deallocations
            .checked_add(count as u64)
            .ok_or(Error::Overflow)?;
        for request in requests {
            if request.alignment > self.limits.alignment {
                return Err(Error::Limit(LimitViolation {
                    kind: Limit::Alignment,
                    limit: self.limits.alignment,
                    attempted: request.alignment,
                }));
            }
            if request.bytes > self.limits.request_bytes {
                return Err(Error::Limit(LimitViolation {
                    kind: Limit::RequestBytes,
                    limit: self.limits.request_bytes,
                    attempted: request.bytes,
                }));
            }
            live = live.checked_add(request.bytes).ok_or(Error::Overflow)?;
            cumulative = cumulative
                .checked_add(request.bytes)
                .ok_or(Error::Overflow)?;
            if live > self.limits.live_bytes {
                return Err(Error::Limit(LimitViolation {
                    kind: Limit::LiveBytes,
                    limit: self.limits.live_bytes,
                    attempted: live,
                }));
            }
            if cumulative > self.limits.cumulative_bytes {
                return Err(Error::Limit(LimitViolation {
                    kind: Limit::CumulativeBytes,
                    limit: self.limits.cumulative_bytes,
                    attempted: cumulative,
                }));
            }
        }
        Ok((live, cumulative, count))
    }
    /// Preflight a data/metadata plan under exclusive instance access. Required
    /// metadata must be included, not silently allocated by a container later.
    pub fn check(&self, requests: &[Request]) -> Result<(), Error> {
        if let Some(memory) = self.provider.as_ref().and_then(Provider::allocator_view) {
            return memory.check(requests);
        }
        self.projected(requests).map(|_| ())
    }
    pub fn allocate(&mut self, request: Request) -> Result<Handle, Error> {
        if let Some(memory) = self
            .provider
            .as_mut()
            .and_then(Provider::allocator_view_mut)
        {
            return memory.allocate(request);
        }
        let (live, cumulative, count) = self.projected(&[request])?;
        let slot = *self.free.last().ok_or(Error::Limit(LimitViolation {
            kind: Limit::LiveAllocations,
            limit: self.limits.live_allocations as u64,
            attempted: count as u64,
        }))? as usize;
        let generation = self.next_generation.ok_or(Error::Overflow)?;
        self.stats.requests += 1;
        self.stats.cumulative_bytes = cumulative;
        // Reserve live/count budgets before the callback, refunding on OOM.
        self.stats.live_bytes = live;
        self.stats.live_allocations = count;
        let region = if request.bytes == 0 {
            None
        } else {
            let Some(mut region) = self
                .provider
                .as_mut()
                .expect("preflight authority")
                .allocate(request)
            else {
                self.stats.live_bytes -= request.bytes;
                self.stats.live_allocations -= 1;
                self.trace_event(
                    EventKind::AllocationFailed(FailureKind::OutOfMemory),
                    request,
                );
                return Err(Error::OutOfMemory);
            };
            let bytes = region.bytes_mut();
            if bytes.len() < request.bytes as usize
                || bytes.as_ptr().addr() & (request.alignment as usize - 1) != 0
            {
                self.stats.live_bytes -= request.bytes;
                self.stats.live_allocations -= 1;
                self.provider
                    .as_mut()
                    .expect("preflight authority")
                    .deallocate(region, request);
                self.trace_event(
                    EventKind::AllocationFailed(FailureKind::InvalidStorage),
                    request,
                );
                return Err(Error::InvalidStorage);
            }
            Some(region)
        };
        self.free.pop();
        self.slots[slot] = Some(Allocation {
            region,
            request,
            generation,
        });
        self.next_generation = generation.checked_add(1);
        self.stats.allocations += 1;
        self.stats.peak_live_bytes = self.stats.peak_live_bytes.max(live);
        self.stats.peak_live_allocations = self.stats.peak_live_allocations.max(count);
        self.trace_event(EventKind::Allocated { generation }, request);
        Ok(Handle {
            allocator: self.id,
            slot: slot as u32,
            generation,
        })
    }
    fn index(&self, handle: Handle) -> Result<usize, Error> {
        if handle.allocator != self.id {
            return Err(Error::ForeignAllocator);
        }
        let index = handle.slot as usize;
        if self
            .slots
            .get(index)
            .and_then(Option::as_ref)
            .is_none_or(|allocation| allocation.generation != handle.generation)
        {
            return Err(Error::StaleHandle);
        }
        Ok(index)
    }
    pub fn request(&self, handle: Handle) -> Result<Request, Error> {
        if let Some(memory) = self.provider.as_ref().and_then(Provider::allocator_view) {
            return memory.request(handle);
        }
        Ok(self.slots[self.index(handle)?]
            .as_ref()
            .expect("validated live allocation")
            .request)
    }
    pub fn bytes(&self, handle: Handle) -> Result<&[u8], Error> {
        if let Some(memory) = self.provider.as_ref().and_then(Provider::allocator_view) {
            return memory.bytes(handle);
        }
        let allocation = self.slots[self.index(handle)?]
            .as_ref()
            .expect("validated live allocation");
        Ok(allocation.region.as_ref().map_or(&[], |region| {
            &region.bytes()[..allocation.request.bytes as usize]
        }))
    }
    pub fn bytes_mut(&mut self, handle: Handle) -> Result<&mut [u8], Error> {
        if self
            .provider
            .as_ref()
            .and_then(Provider::allocator_view)
            .is_some()
        {
            return self
                .provider
                .as_mut()
                .and_then(Provider::allocator_view_mut)
                .ok_or(Error::MissingAuthority)?
                .bytes_mut(handle);
        }
        let index = self.index(handle)?;
        let allocation = self.slots[index]
            .as_mut()
            .expect("validated live allocation");
        Ok(allocation.region.as_mut().map_or(&mut [], |region| {
            &mut region.bytes_mut()[..allocation.request.bytes as usize]
        }))
    }
    pub fn release(&mut self, handle: Handle) -> Result<(), Error> {
        if let Some(memory) = self
            .provider
            .as_mut()
            .and_then(Provider::allocator_view_mut)
        {
            return memory.release(handle);
        }
        let index = self.index(handle)?;
        self.release_index(index);
        Ok(())
    }
    /// Copy validated ranges without a temporary guest allocation. Both live
    /// identities and both ranges are checked before mutation.
    pub(crate) fn copy_bytes(
        &mut self,
        source: Handle,
        from: usize,
        target: Handle,
        to: usize,
        len: usize,
    ) -> Result<(), Error> {
        if let Some(memory) = self
            .provider
            .as_mut()
            .and_then(Provider::allocator_view_mut)
        {
            return memory.copy_bytes(source, from, target, to, len);
        }
        let source_index = self.index(source)?;
        let target_index = self.index(target)?;
        let end = from.checked_add(len).ok_or(Error::Overflow)?;
        let target_end = to.checked_add(len).ok_or(Error::Overflow)?;
        if end > self.request(source)?.bytes as usize
            || target_end > self.request(target)?.bytes as usize
        {
            return Err(Error::InvalidStorage);
        }
        if len == 0 {
            return Ok(());
        }
        if source_index == target_index {
            self.bytes_mut(target)?.copy_within(from..end, to);
        } else {
            let (source, target) = if source_index < target_index {
                let (left, right) = self.slots.split_at_mut(target_index);
                (
                    left[source_index].as_ref().unwrap(),
                    right[0].as_mut().unwrap(),
                )
            } else {
                let (left, right) = self.slots.split_at_mut(source_index);
                (
                    right[0].as_ref().unwrap(),
                    left[target_index].as_mut().unwrap(),
                )
            };
            target.region.as_mut().expect("nonzero copy").bytes_mut()[to..target_end]
                .copy_from_slice(&source.region.as_ref().expect("nonzero copy").bytes()[from..end]);
        }
        Ok(())
    }
    /// Exchange disjoint admitted regions after validating both complete ranges.
    pub(crate) fn swap_bytes(
        &mut self,
        a: Handle,
        from: usize,
        b: Handle,
        to: usize,
        len: usize,
    ) -> Result<(), Error> {
        if let Some(memory) = self
            .provider
            .as_mut()
            .and_then(Provider::allocator_view_mut)
        {
            return memory.swap_bytes(a, from, b, to, len);
        }
        let a_index = self.index(a)?;
        let b_index = self.index(b)?;
        let a_end = from.checked_add(len).ok_or(Error::Overflow)?;
        let b_end = to.checked_add(len).ok_or(Error::Overflow)?;
        if a_index == b_index
            || a_end > self.request(a)?.bytes as usize
            || b_end > self.request(b)?.bytes as usize
        {
            return Err(Error::InvalidStorage);
        }
        if len == 0 {
            return Ok(());
        }
        let (a, b) = if a_index < b_index {
            let (left, right) = self.slots.split_at_mut(b_index);
            (left[a_index].as_mut().unwrap(), right[0].as_mut().unwrap())
        } else {
            let (left, right) = self.slots.split_at_mut(a_index);
            (right[0].as_mut().unwrap(), left[b_index].as_mut().unwrap())
        };
        a.region.as_mut().unwrap().bytes_mut()[from..a_end]
            .swap_with_slice(&mut b.region.as_mut().unwrap().bytes_mut()[to..b_end]);
        Ok(())
    }
    fn release_index(&mut self, index: usize) {
        let allocation = self.slots[index].take().expect("validated live allocation");
        // Capacity for every locator was reserved before entry.
        self.free.push(index as u32);
        self.stats.live_bytes -= allocation.request.bytes;
        self.stats.live_allocations -= 1;
        self.stats.deallocations += 1;
        if let Some(region) = allocation.region {
            self.provider
                .as_mut()
                .expect("authority outlives allocations")
                .deallocate(region, allocation.request);
        }
        self.trace_event(
            EventKind::Retired {
                generation: allocation.generation,
            },
            allocation.request,
        );
    }
    /// Allocate/copy before publication. Failure preserves the old owner/data;
    /// success publishes the replacement before releasing old storage.
    pub fn resize(&mut self, handle: &mut Handle, bytes: u64) -> Result<(), Error> {
        if let Some(memory) = self
            .provider
            .as_mut()
            .and_then(Provider::allocator_view_mut)
        {
            return memory.resize(handle, bytes);
        }
        let old_index = self.index(*handle)?;
        let old_request = self.request(*handle)?;
        if bytes == old_request.bytes {
            return Ok(());
        }
        let next = self.allocate(Request::new(bytes, old_request.alignment)?)?;
        let next_index = next.slot as usize;
        let len = old_request.bytes.min(bytes) as usize;
        if len != 0 {
            let (old, replacement) = if old_index < next_index {
                let (left, right) = self.slots.split_at_mut(next_index);
                (
                    left[old_index].as_ref().unwrap(),
                    right[0].as_mut().unwrap(),
                )
            } else {
                let (left, right) = self.slots.split_at_mut(old_index);
                (
                    right[0].as_ref().unwrap(),
                    left[next_index].as_mut().unwrap(),
                )
            };
            replacement
                .region
                .as_mut()
                .expect("nonzero replacement")
                .bytes_mut()[..len]
                .copy_from_slice(&old.region.as_ref().expect("nonzero old storage").bytes()[..len]);
        }
        *handle = next;
        self.release_index(old_index);
        Ok(())
    }
    /// Cross-allocator cloning is explicit and leaves the original allocation.
    pub fn clone_from<Q: Provider>(
        &mut self,
        source: &Allocator<Q>,
        handle: Handle,
    ) -> Result<Handle, Error> {
        let request = source.request(handle)?;
        let bytes = source.bytes(handle)?;
        let result = self.allocate(request)?;
        self.bytes_mut(result)?.copy_from_slice(bytes);
        Ok(result)
    }
}
impl<'a> Allocator<Borrowed<'a>> {
    /// A scoped typed view of existing storage, with no new identity, locator
    /// storage, regions or accounting. Drop only ends this exclusive borrow.
    pub fn borrow(memory: &'a mut dyn Memory) -> Result<Self, Error> {
        memory.preflight()?;
        let id = memory.identity();
        let limits = memory.limits();
        Ok(Self {
            id,
            provider: Some(Borrowed { memory }),
            slots: Vec::new(),
            free: Vec::new(),
            limits,
            stats: Stats::default(),
            next_generation: None,
            trace: None,
        })
    }
}
impl<P: Provider> Memory for Allocator<P> {
    fn identity(&self) -> Identity {
        self.identity()
    }
    fn limits(&self) -> Limits {
        self.limits()
    }
    fn stats(&self) -> Stats {
        self.stats()
    }
    fn preflight(&self) -> Result<(), Error> {
        Self::preflight(Some(self))
    }
    fn check(&self, requests: &[Request]) -> Result<(), Error> {
        self.check(requests)
    }
    fn allocate(&mut self, request: Request) -> Result<Handle, Error> {
        self.allocate(request)
    }
    fn request(&self, handle: Handle) -> Result<Request, Error> {
        self.request(handle)
    }
    fn bytes(&self, handle: Handle) -> Result<&[u8], Error> {
        self.bytes(handle)
    }
    fn bytes_mut(&mut self, handle: Handle) -> Result<&mut [u8], Error> {
        self.bytes_mut(handle)
    }
    fn release(&mut self, handle: Handle) -> Result<(), Error> {
        self.release(handle)
    }
    fn resize(&mut self, handle: &mut Handle, bytes: u64) -> Result<(), Error> {
        self.resize(handle, bytes)
    }
    fn copy_bytes(
        &mut self,
        source: Handle,
        from: usize,
        target: Handle,
        to: usize,
        len: usize,
    ) -> Result<(), Error> {
        self.copy_bytes(source, from, target, to, len)
    }
    fn swap_bytes(
        &mut self,
        a: Handle,
        from: usize,
        b: Handle,
        to: usize,
        len: usize,
    ) -> Result<(), Error> {
        self.swap_bytes(a, from, b, to, len)
    }
}
impl<P: Provider> Drop for Allocator<P> {
    fn drop(&mut self) {
        for index in 0..self.slots.len() {
            if self.slots[index].is_some() {
                self.release_index(index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_scope_preserves_storage_usage_and_restores_policy_after_unwind() {
        let mut allocator = Allocator::new(System, Default::default()).unwrap();
        let original = allocator.limits();
        let identity = allocator.identity();
        let request = Request::new(8, 8).unwrap();
        let live = allocator.allocate(request).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut scope = allocator
                .limit_scope(Limits {
                    live_bytes: 0,
                    live_allocations: 0,
                    ..original
                })
                .unwrap();
            assert_eq!(scope.identity(), identity);
            assert_eq!(scope.stats().live_bytes, 8);
            assert!(matches!(scope.allocate(request), Err(Error::Limit(_))));
            // Cleanup must work even when retained usage exceeds the new cap.
            scope.release(live).unwrap();
            let mut view = Allocator::borrow(&mut *scope).unwrap();
            assert!(matches!(view.allocate(request), Err(Error::Limit(_))));
            assert!(matches!(
                view.limit_scope(original),
                Err(Error::BorrowedProvider)
            ));
            panic!("exercise policy restoration");
        }));
        assert!(result.is_err());
        assert_eq!(allocator.limits(), original);
        assert_eq!(allocator.stats().cumulative_bytes, 8);
        assert_eq!(allocator.stats().deallocations, 1);
        let next = allocator.allocate(request).unwrap();
        allocator.release(next).unwrap();
        assert_eq!(allocator.stats().cumulative_bytes, 16);
    }

    #[test]
    fn borrowed_views_keep_original_requests_handles_regions_and_failure_accounting() {
        use std::{
            cell::{Cell, RefCell},
            rc::Rc,
        };
        struct Trace {
            calls: Rc<RefCell<Vec<Request>>>,
            frees: Rc<RefCell<Vec<Request>>>,
            fail: Rc<Cell<bool>>,
        }
        impl Provider for Trace {
            type Region = Storage;
            fn allocate(&mut self, request: Request) -> Option<Storage> {
                self.calls.borrow_mut().push(request);
                if self.fail.get() {
                    None
                } else {
                    System.allocate(request)
                }
            }
            fn deallocate(&mut self, region: Storage, request: Request) {
                self.frees.borrow_mut().push(request);
                System.deallocate(region, request);
            }
        }
        let calls = Rc::new(RefCell::new(Vec::new()));
        let frees = Rc::new(RefCell::new(Vec::new()));
        let fail = Rc::new(Cell::new(false));
        let mut original = Allocator::new(
            Trace {
                calls: calls.clone(),
                frees: frees.clone(),
                fail: fail.clone(),
            },
            Limits {
                request_bytes: 32,
                live_bytes: 64,
                cumulative_bytes: 128,
                live_allocations: 4,
                alignment: 64,
            },
        )
        .unwrap();
        let request = Request::new(8, 8).unwrap();
        let first = original.allocate(request).unwrap();
        original
            .bytes_mut(first)
            .unwrap()
            .copy_from_slice(b"12345678");
        let pointer = original.bytes(first).unwrap().as_ptr();
        let identity = original.identity();
        let before = original.stats();
        let mut foreign = allocator();
        let alien = foreign.allocate(Request::new(4, 1).unwrap()).unwrap();
        let mut second;
        {
            let mut view = Allocator::borrow(&mut original).unwrap();
            assert_eq!(view.identity(), identity);
            assert_eq!(view.stats(), before);
            assert!(view.slots.is_empty() && view.free.is_empty());
            {
                let mut nested = Allocator::borrow(&mut view).unwrap();
                assert_eq!(nested.identity(), identity);
                assert_eq!(nested.bytes(first).unwrap().as_ptr(), pointer);
                second = nested.allocate(request).unwrap();
                let before_refusal = nested.stats();
                assert_eq!(nested.bytes(alien), Err(Error::ForeignAllocator));
                assert_eq!(
                    nested.copy_bytes(first, 0, alien, 0, 4),
                    Err(Error::ForeignAllocator)
                );
                assert_eq!(
                    nested.swap_bytes(first, 0, alien, 0, 4),
                    Err(Error::ForeignAllocator)
                );
                assert_eq!(nested.release(alien), Err(Error::ForeignAllocator));
                assert_eq!(
                    nested.allocate(Request::new(33, 1).unwrap()),
                    Err(Error::Limit(LimitViolation {
                        kind: Limit::RequestBytes,
                        limit: 32,
                        attempted: 33
                    }))
                );
                assert_eq!(nested.stats(), before_refusal);
                nested.copy_bytes(first, 0, second, 0, 8).unwrap();
                assert_eq!(nested.bytes(second).unwrap(), b"12345678");
                nested
                    .bytes_mut(second)
                    .unwrap()
                    .copy_from_slice(b"ABCDEFGH");
                nested.swap_bytes(first, 0, second, 0, 8).unwrap();
                assert_eq!(nested.bytes(first).unwrap(), b"ABCDEFGH");
                assert_eq!(nested.bytes(second).unwrap(), b"12345678");
                assert_eq!(
                    nested.swap_bytes(first, 0, first, 0, 8),
                    Err(Error::InvalidStorage)
                );
                let old = second;
                nested.resize(&mut second, 16).unwrap();
                assert_eq!(nested.bytes(old), Err(Error::StaleHandle));
                assert_eq!(&nested.bytes(second).unwrap()[..8], b"12345678");
                fail.set(true);
                let retained = second;
                assert_eq!(nested.resize(&mut second, 24), Err(Error::OutOfMemory));
                assert_eq!(second, retained);
                assert_eq!(
                    nested.request(second).unwrap(),
                    Request::new(16, 8).unwrap()
                );
                assert_eq!(&nested.bytes(second).unwrap()[..8], b"12345678");
                fail.set(false);
                let zero = nested.allocate(Request::new(0, 64).unwrap()).unwrap();
                assert!(nested.bytes(zero).unwrap().is_empty());
                nested.release(zero).unwrap();
                assert!(nested.slots.is_empty() && nested.free.is_empty());
                assert!(matches!(
                    nested.revoke(),
                    Err(Error::InUse { allocations: 2 })
                ));
            }
            assert_eq!(frees.borrow().len(), 1);
            assert_eq!(view.stats().live_allocations, 2);
            assert!(view.slots.is_empty() && view.free.is_empty());
        }
        assert_eq!(original.bytes(first).unwrap().as_ptr(), pointer);
        assert_eq!(original.bytes(first).unwrap(), b"ABCDEFGH");
        assert_eq!(original.stats().live_allocations, 2);
        original.release(second).unwrap();
        original.release(first).unwrap();
        foreign.release(alien).unwrap();
        assert_eq!(original.stats().live_allocations, 0);
        assert_eq!(original.stats().requests, 5);
        assert_eq!(
            &*calls.borrow(),
            &[
                request,
                request,
                Request::new(16, 8).unwrap(),
                Request::new(24, 8).unwrap()
            ]
        );
        assert_eq!(
            &*frees.borrow(),
            &[request, Request::new(16, 8).unwrap(), request]
        );
    }

    #[test]
    fn borrowed_view_construction_never_creates_owned_authority_or_drains_its_owner() {
        let mut original = allocator();
        let identity = original.identity();
        {
            let mut view = Allocator::borrow(&mut original).unwrap();
            let provider = view.revoke().unwrap();
            assert!(matches!(
                Allocator::new(provider, Limits::default()),
                Err(Error::BorrowedProvider)
            ));
        }
        assert_eq!(original.identity(), identity);
        assert_eq!(original.stats(), Stats::default());
        Allocator::preflight(Some(&original)).unwrap();
        let _provider = original.revoke().unwrap();
        assert!(matches!(
            Allocator::borrow(&mut original),
            Err(Error::MissingAuthority)
        ));
    }

    #[test]
    fn borrowed_views_cannot_reset_parent_limits_or_consumed_storage() {
        for kind in [
            Limit::RequestBytes,
            Limit::LiveBytes,
            Limit::CumulativeBytes,
            Limit::LiveAllocations,
            Limit::Alignment,
        ] {
            let mut limits = Limits::default();
            match kind {
                Limit::RequestBytes => limits.request_bytes = 8,
                Limit::LiveBytes => limits.live_bytes = 8,
                Limit::CumulativeBytes => limits.cumulative_bytes = 8,
                Limit::LiveAllocations => limits.live_allocations = 1,
                Limit::Alignment => limits.alignment = 8,
            }
            let mut original = Allocator::new(System, limits).unwrap();
            let first = original.allocate(Request::new(8, 8).unwrap()).unwrap();
            if kind == Limit::CumulativeBytes {
                original.release(first).unwrap();
            }
            let next = match kind {
                Limit::RequestBytes => Request::new(9, 8).unwrap(),
                Limit::Alignment => Request::new(0, 16).unwrap(),
                Limit::LiveAllocations => Request::new(0, 8).unwrap(),
                _ => Request::new(1, 8).unwrap(),
            };
            let before = original.stats();
            for _ in 0..2 {
                let mut view = Allocator::borrow(&mut original).unwrap();
                assert_eq!(view.limits(), limits);
                let (limit, attempted) = match kind {
                    Limit::RequestBytes => (8, 9),
                    Limit::Alignment => (8, 16),
                    Limit::LiveAllocations => (1, 2),
                    _ => (8, 9),
                };
                let expected = Error::Limit(LimitViolation {
                    kind,
                    limit,
                    attempted,
                });
                assert_eq!(view.check(&[next]), Err(expected));
                assert_eq!(view.allocate(next), Err(expected));
                assert_eq!(view.stats(), before);
            }
            assert_eq!(original.stats(), before);
            if kind != Limit::CumulativeBytes {
                original.release(first).unwrap();
            }
            assert_eq!(original.stats().live_allocations, 0);
        }
    }

    fn allocator() -> Allocator<System> {
        Allocator::new(
            System,
            Limits {
                request_bytes: u64::MAX,
                live_bytes: u64::MAX,
                cumulative_bytes: u64::MAX,
                live_allocations: 4,
                ..Limits::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn checked_counters_reject_before_attempt_accounting() {
        for stats in [
            Stats {
                requests: u64::MAX,
                ..Stats::default()
            },
            Stats {
                allocations: u64::MAX,
                ..Stats::default()
            },
            Stats {
                deallocations: u64::MAX,
                ..Stats::default()
            },
            Stats {
                live_bytes: u64::MAX,
                ..Stats::default()
            },
            Stats {
                cumulative_bytes: u64::MAX,
                ..Stats::default()
            },
            Stats {
                live_allocations: usize::MAX,
                ..Stats::default()
            },
        ] {
            let mut allocator = allocator();
            allocator.stats = stats;
            assert_eq!(
                allocator.allocate(Request::new(1, 1).unwrap()),
                Err(Error::Overflow)
            );
            assert_eq!(allocator.stats(), stats);
        }
    }

    #[test]
    fn generation_exhaustion_never_wraps_or_prevents_final_release() {
        let mut allocator = allocator();
        allocator.next_generation = Some(u64::MAX);
        let request = Request::new(0, 1).unwrap();
        assert_eq!(allocator.check(&[request, request]), Err(Error::Overflow));
        assert_eq!(allocator.stats(), Stats::default());
        let handle = allocator.allocate(request).unwrap();
        assert_eq!(handle.generation(), u64::MAX);
        allocator.check(&[]).unwrap();
        assert_eq!(allocator.allocate(request), Err(Error::Overflow));
        allocator.release(handle).unwrap();
        assert_eq!(allocator.allocate(request), Err(Error::Overflow));
        assert_eq!(allocator.release(handle), Err(Error::StaleHandle));
        assert_eq!(allocator.stats().deallocations, 1);
    }
}
