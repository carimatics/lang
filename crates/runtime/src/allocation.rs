//! Fallible instance storage allocation under RFC 0008.
//!
//! Every request is exact requested storage, whether guest data or required
//! guest-controlled metadata. Source layout/container adapters must request all
//! such storage before publication. The fixed locator table is trusted instance
//! context reserved before entry; no locator allocation occurs in guest calls.
//! This managed Rust provider is not the raw C allocator bridge.
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    MissingAuthority,
    InUse { allocations: usize },
    ForeignAllocator,
    StaleHandle,
    InvalidAlignment(u64),
    InvalidLimits,
    Overflow,
    Limit(Limit),
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
}
impl<P: Provider> Allocator<P> {
    /// Host/frame setup. Reserve all locator slots before any guest entry.
    pub fn new(provider: P, limits: Limits) -> Result<Self, Error> {
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
    pub const fn stats(&self) -> Stats {
        self.stats
    }
    pub fn revoke(&mut self) -> Result<P, Error> {
        if self.stats.live_allocations != 0 {
            return Err(Error::InUse {
                allocations: self.stats.live_allocations,
            });
        }
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
            return Err(Error::Limit(Limit::LiveAllocations));
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
                return Err(Error::Limit(Limit::Alignment));
            }
            if request.bytes > self.limits.request_bytes {
                return Err(Error::Limit(Limit::RequestBytes));
            }
            live = live.checked_add(request.bytes).ok_or(Error::Overflow)?;
            cumulative = cumulative
                .checked_add(request.bytes)
                .ok_or(Error::Overflow)?;
            if live > self.limits.live_bytes {
                return Err(Error::Limit(Limit::LiveBytes));
            }
            if cumulative > self.limits.cumulative_bytes {
                return Err(Error::Limit(Limit::CumulativeBytes));
            }
        }
        Ok((live, cumulative, count))
    }
    /// Preflight a data/metadata plan under exclusive instance access. Required
    /// metadata must be included, not silently allocated by a container later.
    pub fn check(&self, requests: &[Request]) -> Result<(), Error> {
        self.projected(requests).map(|_| ())
    }
    pub fn allocate(&mut self, request: Request) -> Result<Handle, Error> {
        let (live, cumulative, count) = self.projected(&[request])?;
        let slot = *self
            .free
            .last()
            .ok_or(Error::Limit(Limit::LiveAllocations))? as usize;
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
        Ok(self.slots[self.index(handle)?]
            .as_ref()
            .expect("validated live allocation")
            .request)
    }
    pub fn bytes(&self, handle: Handle) -> Result<&[u8], Error> {
        let allocation = self.slots[self.index(handle)?]
            .as_ref()
            .expect("validated live allocation");
        Ok(allocation.region.as_ref().map_or(&[], |region| {
            &region.bytes()[..allocation.request.bytes as usize]
        }))
    }
    pub fn bytes_mut(&mut self, handle: Handle) -> Result<&mut [u8], Error> {
        let index = self.index(handle)?;
        let allocation = self.slots[index]
            .as_mut()
            .expect("validated live allocation");
        Ok(allocation.region.as_mut().map_or(&mut [], |region| {
            &mut region.bytes_mut()[..allocation.request.bytes as usize]
        }))
    }
    pub fn release(&mut self, handle: Handle) -> Result<(), Error> {
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
    }
    /// Allocate/copy before publication. Failure preserves the old owner/data;
    /// success publishes the replacement before releasing old storage.
    pub fn resize(&mut self, handle: &mut Handle, bytes: u64) -> Result<(), Error> {
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
