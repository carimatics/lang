//! Retained arena storage for accepted RFC 0004/0007. A single owner controls
//! each region; references copy without retaining nodes. Exit revokes the
//! region before an allocation-free reverse-order drain outside the mutex.
//! This host-storage engine is not the source allocator/FFI ABI. Arc allocation
//! may abort; guest cleanup and VM/native/source integration remain separate.

use crate::shared;
use lang_mir::unit::Span;
use std::error::Error as StdError;
use std::fmt::{Debug, Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub arenas: usize,
    pub arena_slots: usize,
    pub nodes: usize,
    /// Includes retired payloads retained by host read/drain snapshots.
    pub payloads: usize,
    pub payload_bytes: usize,
    pub edges: usize,
    pub publication_work: usize,
    pub readers: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            arenas: 256,
            arena_slots: 8192,
            nodes: 4096,
            payloads: 8192,
            payload_bytes: 16 * 1024 * 1024,
            edges: 65_536,
            publication_work: 1_000_000,
            readers: u64::MAX,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Arenas,
    ArenaSlots,
    Nodes,
    Payloads,
    PayloadBytes,
    Edges,
    PublicationWork,
    Readers,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Unavailable,
    CrossRegion,
    BorrowConflict,
    CountOverflow,
    AllocationFailed,
    Poisoned,
    WrongEdgeKind,
    Limit(LimitKind),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub primary: Option<Span>,
    pub secondary: Option<Span>,
}
impl Error {
    fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            primary: None,
            secondary: None,
        }
    }
    fn at(mut self, span: Option<Span>) -> Self {
        self.primary = span;
        self
    }
    fn related(mut self, span: Option<Span>) -> Self {
        self.secondary = span;
        self
    }
    pub const fn identity(&self) -> &'static str {
        match self.kind {
            ErrorKind::Unavailable => "ownership.unavailable-arena-reference",
            ErrorKind::CrossRegion => "ownership.cross-region-edge",
            ErrorKind::BorrowConflict => "ownership.borrow-conflict",
            _ => "ownership.arena-runtime",
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:?}", self.identity(), self.kind)
    }
}
impl StdError for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Key {
    slot: usize,
    generation: u64,
}
/// Logical, untrusted identity, not an address, wire encoding, or authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Identity {
    pub runtime: u64,
    pub arena_slot: usize,
    pub generation: u64,
    pub node: usize,
}
/// Copying this token neither owns nor keeps a region/node alive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reference {
    identity: Identity,
    origin: Option<Span>,
}
impl Reference {
    pub const fn identity(self) -> Identity {
        self.identity
    }
}

#[derive(Debug)]
pub enum Target {
    Arena(Reference),
    Shared(shared::Strong),
    Weak(shared::Weak),
}
#[derive(Debug)]
pub struct Edge {
    pub target: Target,
    pub span: Option<Span>,
}
impl Edge {
    pub const fn arena(reference: Reference, span: Option<Span>) -> Self {
        Self {
            target: Target::Arena(reference),
            span,
        }
    }
}
#[derive(Debug, Default)]
pub struct Payload {
    pub bytes: Vec<u8>,
    pub edges: Vec<Edge>,
}
#[derive(Debug)]
pub struct Failure {
    pub error: Error,
    pub payload: Payload,
}
impl Display for Failure {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.error, f)
    }
}
impl StdError for Failure {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Counts {
    payloads: usize,
    bytes: usize,
    edges: usize,
}
struct Budget {
    limits: Limits,
    counts: Mutex<Counts>,
}
impl Budget {
    fn lock(&self) -> Result<MutexGuard<'_, Counts>, Error> {
        self.counts
            .lock()
            .map_err(|_| Error::new(ErrorKind::Poisoned))
    }
    fn reserve(self: &Arc<Self>, payload: &Payload) -> Result<Reservation, Error> {
        let mut counts = self.lock()?;
        let next = Counts {
            payloads: checked(
                counts.payloads.checked_add(1),
                self.limits.payloads,
                LimitKind::Payloads,
            )?,
            bytes: checked(
                counts.bytes.checked_add(payload.bytes.len()),
                self.limits.payload_bytes,
                LimitKind::PayloadBytes,
            )?,
            edges: checked(
                counts.edges.checked_add(payload.edges.len()),
                self.limits.edges,
                LimitKind::Edges,
            )?,
        };
        *counts = next;
        Ok(Reservation {
            budget: self.clone(),
            counts: Counts {
                payloads: 1,
                bytes: payload.bytes.len(),
                edges: payload.edges.len(),
            },
        })
    }
}
struct Reservation {
    budget: Arc<Budget>,
    counts: Counts,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        // Recovery releases storage only. Poison still denies all publication.
        let mut counts = self
            .budget
            .counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        counts.payloads = counts.payloads.saturating_sub(self.counts.payloads);
        counts.bytes = counts.bytes.saturating_sub(self.counts.bytes);
        counts.edges = counts.edges.saturating_sub(self.counts.edges);
    }
}
struct StoredPayload {
    payload: Option<Payload>,
    _reservation: Reservation,
}
impl StoredPayload {
    fn payload(&self) -> &Payload {
        self.payload.as_ref().expect("published payload")
    }
}
struct Node {
    payload: Arc<StoredPayload>,
    span: Option<Span>,
}
struct Region {
    nodes: Vec<Node>,
    readers: u64,
}
impl Drop for Region {
    fn drop(&mut self) {
        while self.nodes.pop().is_some() {}
    }
}
struct Slot {
    generation: u64,
    region: Option<Region>,
}
#[derive(Default)]
struct State {
    slots: Vec<Slot>,
    free: Vec<usize>,
    arenas: usize,
    nodes: usize,
    retired_nodes: u64,
    replaced_payloads: u64,
}
struct Inner {
    id: u64,
    limits: Limits,
    state: Mutex<State>,
    budget: Arc<Budget>,
}
impl Inner {
    fn lock(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.state
            .lock()
            .map_err(|_| Error::new(ErrorKind::Poisoned))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    pub live_arenas: usize,
    pub arena_slots: usize,
    pub live_nodes: usize,
    pub resident_payloads: usize,
    pub resident_bytes: usize,
    pub resident_edges: usize,
    /// Saturating instrumentation, never an identity or generation.
    pub retired_nodes: u64,
    pub replaced_payloads: u64,
}
#[derive(Clone)]
pub struct ArenaHeap {
    inner: Arc<Inner>,
}
impl ArenaHeap {
    pub fn new(limits: Limits) -> Result<Self, Error> {
        let id = next_runtime(&NEXT_RUNTIME)?;
        Ok(Self {
            inner: Arc::new(Inner {
                id,
                limits,
                state: Mutex::new(State::default()),
                budget: Arc::new(Budget {
                    limits,
                    counts: Mutex::new(Counts::default()),
                }),
            }),
        })
    }
    pub fn open(&self, span: Option<Span>) -> Result<Arena, Error> {
        let result = (|| {
            let mut state = self.inner.lock()?;
            checked(
                state.arenas.checked_add(1),
                self.inner.limits.arenas,
                LimitKind::Arenas,
            )?;
            let slot = if let Some(slot) = state.free.pop() {
                slot
            } else {
                let count = checked(
                    state.slots.len().checked_add(1),
                    self.inner.limits.arena_slots,
                    LimitKind::ArenaSlots,
                )?;
                reserve(&mut state.slots, 1)?;
                let free_len = state.free.len();
                reserve(&mut state.free, count - free_len)?;
                let slot = state.slots.len();
                state.slots.push(Slot {
                    generation: 1,
                    region: None,
                });
                slot
            };
            let key = Key {
                slot,
                generation: state.slots[slot].generation,
            };
            state.slots[slot].region = Some(Region {
                nodes: Vec::new(),
                readers: 0,
            });
            state.arenas += 1;
            Ok(Arena {
                inner: self.inner.clone(),
                key: Some(key),
                origin: span,
            })
        })();
        result.map_err(|e: Error| e.at(span))
    }
    pub fn stats(&self) -> Result<Stats, Error> {
        let state = self.inner.lock()?;
        let counts = self.inner.budget.lock()?;
        Ok(Stats {
            live_arenas: state.arenas,
            arena_slots: state.slots.len(),
            live_nodes: state.nodes,
            resident_payloads: counts.payloads,
            resident_bytes: counts.bytes,
            resident_edges: counts.edges,
            retired_nodes: state.retired_nodes,
            replaced_payloads: state.replaced_payloads,
        })
    }
}
/// Sole owner; deliberately no Clone. A host may move it but cannot duplicate
/// node ownership by copying References or the heap registry.
pub struct Arena {
    inner: Arc<Inner>,
    key: Option<Key>,
    origin: Option<Span>,
}
impl Debug for Arena {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Arena")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}
impl Arena {
    fn key(&self) -> Result<Key, Error> {
        self.key
            .ok_or_else(|| Error::new(ErrorKind::Unavailable).related(self.origin))
    }
    pub fn allocate(&mut self, payload: Payload, span: Option<Span>) -> Result<Reference, Failure> {
        let result = (|| {
            let key = self.key()?;
            let mut state = self.inner.lock()?;
            let region = state.region(key)?;
            if region.readers != 0 {
                return Err(Error::new(ErrorKind::BorrowConflict));
            }
            state.prepare(&self.inner, key, &payload)?;
            checked(
                state.nodes.checked_add(1),
                self.inner.limits.nodes,
                LimitKind::Nodes,
            )?;
            reserve(&mut state.region_mut(key)?.nodes, 1)?;
            let reservation = self.inner.budget.reserve(&payload)?;
            Ok((key, state, reservation))
        })();
        match result {
            Err(error) => Err(Failure {
                error: error.at(span),
                payload,
            }),
            Ok((key, mut state, reservation)) => {
                let region = state.region_mut(key).expect("validated arena");
                let index = region.nodes.len();
                let payload = Arc::new(StoredPayload {
                    payload: Some(payload),
                    _reservation: reservation,
                });
                region.nodes.push(Node { payload, span });
                state.nodes += 1;
                Ok(Reference {
                    identity: identity(self.inner.id, key, index),
                    origin: span,
                })
            }
        }
    }
    /// Validates an untrusted identity at the owner boundary. Identity bytes
    /// alone neither keep data alive nor bypass owner/provenance checks.
    pub fn resolve(&self, identity: Identity, span: Option<Span>) -> Result<Reference, Error> {
        let result = (|| {
            if identity.runtime != self.inner.id {
                return Err(Error::new(ErrorKind::Unavailable));
            }
            let key = self.key()?;
            let state = self.inner.lock()?;
            let reference = Reference {
                identity,
                origin: None,
            };
            let node = state.validate(&self.inner, key, reference)?;
            Ok(Reference {
                identity,
                origin: node.span,
            })
        })();
        result.map_err(|e: Error| e.at(span))
    }
    pub fn read(&self, reference: Reference, span: Option<Span>) -> Result<Read, Error> {
        let result = (|| {
            let key = self.key()?;
            let mut state = self.inner.lock()?;
            let payload = state.validate(&self.inner, key, reference)?.payload.clone();
            let region = state.region_mut(key)?;
            let readers = region
                .readers
                .checked_add(1)
                .ok_or_else(|| Error::new(ErrorKind::CountOverflow))?;
            if readers > self.inner.limits.readers {
                return Err(Error::new(ErrorKind::Limit(LimitKind::Readers)));
            }
            region.readers = readers;
            Ok(Read {
                inner: self.inner.clone(),
                reference,
                payload,
            })
        })();
        result.map_err(|e: Error| e.at(span))
    }
    pub fn replace(
        &mut self,
        reference: Reference,
        payload: Payload,
        span: Option<Span>,
    ) -> Result<Retired, Failure> {
        let result = (|| {
            let key = self.key()?;
            let state = self.inner.lock()?;
            state.validate(&self.inner, key, reference)?;
            if state.region(key)?.readers != 0 {
                return Err(Error::new(ErrorKind::BorrowConflict));
            }
            state.prepare(&self.inner, key, &payload)?;
            let reservation = self.inner.budget.reserve(&payload)?;
            Ok((key, state, reservation))
        })();
        match result {
            Err(error) => Err(Failure {
                error: error.at(span),
                payload,
            }),
            Ok((key, mut state, reservation)) => {
                let replacement = Arc::new(StoredPayload {
                    payload: Some(payload),
                    _reservation: reservation,
                });
                let node = &mut state.region_mut(key).expect("validated arena").nodes
                    [reference.identity.node];
                let payload = std::mem::replace(&mut node.payload, replacement);
                state.replaced_payloads = state.replaced_payloads.saturating_add(1);
                Ok(Retired { reference, payload })
            }
        }
    }
    /// An explicit normal exit requires all reads to have ended. Success
    /// revokes references before returning an allocation-free teardown drain.
    pub fn close(&mut self, span: Option<Span>) -> Result<Teardown, Error> {
        let result = (|| {
            let key = self.key()?;
            let mut state = self.inner.lock()?;
            if state.region(key)?.readers != 0 {
                return Err(Error::new(ErrorKind::BorrowConflict));
            }
            let region = state.retire(key)?;
            self.key = None;
            Ok(Teardown {
                runtime: self.inner.id,
                key,
                region,
            })
        })();
        result.map_err(|e: Error| e.at(span))
    }
}
impl Drop for Arena {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            let retired = if let Ok(mut state) = self.inner.lock() {
                state.retire(key).ok()
            } else {
                None
            };
            // The lock has ended. No payload cleanup can reenter a locked heap.
            if let Some(region) = retired {
                drop(Teardown {
                    runtime: self.inner.id,
                    key,
                    region,
                });
            }
        }
    }
}
/// A registered arena-wide loan with a retained payload snapshot. Dropping
/// the owner revokes guard methods; already issued Rust slices remain backed
/// by this snapshot until the guard ends. This is not authority after exit.
pub struct Read {
    inner: Arc<Inner>,
    reference: Reference,
    payload: Arc<StoredPayload>,
}
impl Debug for Read {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArenaRead")
            .field("reference", &self.reference)
            .finish_non_exhaustive()
    }
}
impl Read {
    fn validate(&self) -> Result<(), Error> {
        let key = reference_key(self.reference);
        let state = self.inner.lock()?;
        state.validate(&self.inner, key, self.reference)?;
        Ok(())
    }
    pub fn bytes(&self) -> Result<&[u8], Error> {
        self.validate()?;
        Ok(&self.payload.payload().bytes)
    }
    pub fn edge_count(&self) -> Result<usize, Error> {
        self.validate()?;
        Ok(self.payload.payload().edges.len())
    }
    pub fn edge(&self, index: usize) -> Result<Reference, Error> {
        self.validate()?;
        match self.payload.payload().edges.get(index).map(|e| &e.target) {
            Some(Target::Arena(reference)) => Ok(*reference),
            _ => Err(Error::new(ErrorKind::WrongEdgeKind)),
        }
    }
}
impl Drop for Read {
    fn drop(&mut self) {
        if let Ok(mut state) = self.inner.lock()
            && let Ok(region) = state.region_mut(reference_key(self.reference))
        {
            region.readers = region.readers.saturating_sub(1);
        }
    }
}
/// A detached old payload. It grants no node access and can be cleaned by an
/// executor outside the publication boundary. Snapshots remain budgeted.
pub struct Retired {
    reference: Reference,
    payload: Arc<StoredPayload>,
}
impl Debug for Retired {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetiredArenaPayload")
            .field("reference", &self.reference)
            .finish_non_exhaustive()
    }
}
impl Retired {
    pub const fn identity(&self) -> Identity {
        self.reference.identity
    }
    pub fn bytes(&self) -> &[u8] {
        &self.payload.payload().bytes
    }
    pub fn edge_count(&self) -> usize {
        self.payload.payload().edges.len()
    }
    pub fn try_into_payload(self) -> Result<Payload, Self> {
        match Arc::try_unwrap(self.payload) {
            Ok(mut stored) => Ok(stored.payload.take().expect("retired payload")),
            Err(payload) => Err(Self {
                reference: self.reference,
                payload,
            }),
        }
    }
}
pub struct Teardown {
    runtime: u64,
    key: Key,
    region: Region,
}
impl Debug for Teardown {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArenaTeardown")
            .field("remaining", &self.region.nodes.len())
            .finish_non_exhaustive()
    }
}
impl Iterator for Teardown {
    type Item = Retired;
    fn next(&mut self) -> Option<Self::Item> {
        let node = self.region.nodes.pop();
        let index = self.region.nodes.len();
        if index == 0 {
            self.region.nodes = Vec::new();
        }
        node.map(|node| Retired {
            reference: Reference {
                identity: identity(self.runtime, self.key, index),
                origin: node.span,
            },
            payload: node.payload,
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.region.nodes.len();
        (n, Some(n))
    }
}
impl ExactSizeIterator for Teardown {}
impl std::iter::FusedIterator for Teardown {}
impl Drop for Teardown {
    fn drop(&mut self) {
        for retired in self.by_ref() {
            drop(retired);
        }
    }
}

fn next_runtime(counter: &AtomicU64) -> Result<u64, Error> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| Error::new(ErrorKind::CountOverflow))
}

fn identity(runtime: u64, key: Key, node: usize) -> Identity {
    Identity {
        runtime,
        arena_slot: key.slot,
        generation: key.generation,
        node,
    }
}
fn reference_key(reference: Reference) -> Key {
    Key {
        slot: reference.identity.arena_slot,
        generation: reference.identity.generation,
    }
}
fn checked(n: Option<usize>, maximum: usize, kind: LimitKind) -> Result<usize, Error> {
    n.filter(|n| *n <= maximum)
        .ok_or_else(|| Error::new(ErrorKind::Limit(kind)))
}
fn reserve<T>(v: &mut Vec<T>, n: usize) -> Result<(), Error> {
    v.try_reserve(n)
        .map_err(|_| Error::new(ErrorKind::AllocationFailed))
}
impl State {
    fn region(&self, key: Key) -> Result<&Region, Error> {
        self.slots
            .get(key.slot)
            .filter(|s| s.generation == key.generation)
            .and_then(|s| s.region.as_ref())
            .ok_or_else(|| Error::new(ErrorKind::Unavailable))
    }
    fn region_mut(&mut self, key: Key) -> Result<&mut Region, Error> {
        self.slots
            .get_mut(key.slot)
            .filter(|s| s.generation == key.generation)
            .and_then(|s| s.region.as_mut())
            .ok_or_else(|| Error::new(ErrorKind::Unavailable))
    }
    fn validate(&self, inner: &Inner, owner: Key, reference: Reference) -> Result<&Node, Error> {
        self.region(owner)?;
        if reference.identity.runtime != inner.id {
            return Err(Error::new(ErrorKind::CrossRegion).related(reference.origin));
        }
        let region = self
            .region(reference_key(reference))
            .map_err(|e| e.related(reference.origin))?;
        let node = region
            .nodes
            .get(reference.identity.node)
            .ok_or_else(|| Error::new(ErrorKind::Unavailable).related(reference.origin))?;
        if reference_key(reference) != owner {
            return Err(Error::new(ErrorKind::CrossRegion).related(reference.origin));
        }
        Ok(node)
    }
    fn prepare(&self, inner: &Inner, owner: Key, payload: &Payload) -> Result<(), Error> {
        let mut work = 1usize;
        checked(
            Some(work),
            inner.limits.publication_work,
            LimitKind::PublicationWork,
        )?;
        for edge in &payload.edges {
            work = checked(
                work.checked_add(1),
                inner.limits.publication_work,
                LimitKind::PublicationWork,
            )?;
            match edge.target {
                Target::Arena(reference) => {
                    self.validate(inner, owner, reference).map_err(|e| {
                        if edge.span.is_some() {
                            e.related(edge.span)
                        } else {
                            e
                        }
                    })?;
                }
                Target::Shared(_) | Target::Weak(_) => {
                    return Err(Error::new(ErrorKind::CrossRegion).related(edge.span));
                }
            }
        }
        Ok(())
    }
    fn retire(&mut self, key: Key) -> Result<Region, Error> {
        self.region(key)?;
        let slot = &mut self.slots[key.slot];
        let mut region = slot.region.take().expect("validated live arena");
        if region.nodes.is_empty() {
            region.nodes = Vec::new();
        }
        self.arenas -= 1;
        self.nodes -= region.nodes.len();
        self.retired_nodes = self.retired_nodes.saturating_add(region.nodes.len() as u64);
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free.push(key.slot);
        }
        Ok(region)
    }
}

#[cfg(test)]
mod tests;
