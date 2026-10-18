//! Runtime ownership engine for RFC 0004. Reference-count transitions and
//! topology publication are linearized by one mutex; no user callback runs
//! under that mutex. This is not the source heap/allocator ABI. Payload bytes
//! are opaque, while every strong/weak ownership field is a tracked `Edge`.
//! Source value lowering, guest destructors and allocator callbacks integrate
//! separately; Rust allocation failure may still abort at Arc allocation.
//! `storage` supplies typed guest-backed control/payload blocks, and `managed`
//! connects their production graph/token service. This opaque engine remains
//! its model; source VM dispatch and nested collection integration are separate.

use std::error::Error as StdError;
use std::fmt::{Debug, Display, Formatter};
use std::sync::{Arc, Mutex, MutexGuard};

use lang_mir::unit::Span;

pub mod managed;
pub mod storage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub nodes: usize,
    pub slots: usize,
    pub edges: usize,
    pub payload_bytes: usize,
    pub publication_work: usize,
    pub reference_count: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 4096,
            slots: 8192,
            edges: 65_536,
            payload_bytes: 16 * 1024 * 1024,
            publication_work: 1_000_000,
            reference_count: u64::MAX,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Nodes,
    Slots,
    Edges,
    PayloadBytes,
    PublicationWork,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    StrongCycle,
    ExpiredWeak,
    Unavailable,
    ForeignHeap,
    BorrowConflict,
    CountOverflow,
    AllocationFailed,
    WrongEdgeKind,
    Poisoned,
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
    pub const fn identity(&self) -> &'static str {
        match self.kind {
            ErrorKind::StrongCycle => "ownership.strong-cycle",
            ErrorKind::ExpiredWeak => "ownership.expired-weak",
            ErrorKind::BorrowConflict => "ownership.borrow-conflict",
            _ => "ownership.shared-runtime",
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:?}", self.identity(), self.kind)
    }
}
impl StdError for Error {}

/// Owning input. Failed publication returns it unchanged, so the executor can
/// perform guest cleanup outside the atomic boundary. Dropping it releases its
/// incoming references; this is separate from failure-atomic publication.
#[derive(Debug, Default)]
pub struct Payload {
    pub bytes: Vec<u8>,
    pub edges: Vec<Edge>,
}
#[derive(Debug)]
pub enum Edge {
    Strong(Strong),
    Weak(Weak),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Key {
    slot: usize,
    generation: u64,
}
#[derive(Clone, Copy)]
struct StoredEdge {
    key: Key,
    weak: bool,
    span: Option<Span>,
}
struct Node {
    bytes: Arc<[u8]>,
    edges: Vec<StoredEdge>,
    strong: u64,
    readers: u64,
}
struct Slot {
    generation: u64,
    weak: u64,
    node: Option<Node>,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    pub live_nodes: usize,
    pub slots: usize,
    pub stored_edges: usize,
    pub payload_bytes: usize,
    /// Saturating instrumentation only; it is not an ownership identifier.
    pub reclaimed_nodes: u64,
}
#[derive(Default)]
struct State {
    slots: Vec<Slot>,
    free: Vec<usize>,
    pending: Vec<StoredEdge>,
    stats: Stats,
}
struct Inner {
    limits: Limits,
    state: Mutex<State>,
}
impl Inner {
    fn lock(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.state
            .lock()
            .map_err(|_| Error::new(ErrorKind::Poisoned))
    }
}
#[derive(Clone)]
pub struct SharedHeap {
    inner: Arc<Inner>,
}
impl SharedHeap {
    pub fn new(limits: Limits) -> Self {
        Self {
            inner: Arc::new(Inner {
                limits,
                state: Mutex::new(State::default()),
            }),
        }
    }
    pub fn stats(&self) -> Result<Stats, Error> {
        Ok(self.inner.lock()?.stats)
    }
    pub fn allocate(&self, mut payload: Payload, span: Option<Span>) -> Result<Strong, Failure> {
        let result = (|| {
            let mut state = self.inner.lock()?;
            let edges = state.prepare(&self.inner, &payload, None, span)?;
            check(
                state.stats.live_nodes.checked_add(1),
                self.inner.limits.nodes,
                LimitKind::Nodes,
            )?;
            if self.inner.limits.reference_count == 0 {
                return Err(Error::new(ErrorKind::CountOverflow));
            }
            let fresh = state.free.is_empty();
            if fresh {
                let slots = check(
                    state.slots.len().checked_add(1),
                    self.inner.limits.slots,
                    LimitKind::Slots,
                )?;
                reserve(&mut state.slots, 1)?;
                // Reserve for future infallible retirement even while free is empty.
                let free_len = state.free.len();
                reserve(&mut state.free, slots - free_len)?;
            }
            let bytes: Arc<[u8]> = Arc::from(std::mem::take(&mut payload.bytes));
            let node = Node {
                bytes,
                edges,
                strong: 1,
                readers: 0,
            };
            let slot = if fresh {
                let slot = state.slots.len();
                state.slots.push(Slot {
                    generation: 1,
                    weak: 0,
                    node: Some(node),
                });
                slot
            } else {
                let slot = state.free.pop().expect("nonempty free list");
                state.slots[slot].node = Some(node);
                slot
            };
            let key = Key {
                slot,
                generation: state.slots[slot].generation,
            };
            state.stats.live_nodes += 1;
            state.stats.slots = state.slots.len();
            state.stats.stored_edges += payload.edges.len();
            state.stats.payload_bytes += state.node(key)?.bytes.len();
            disarm(&mut payload.edges);
            Ok(Strong {
                inner: self.inner.clone(),
                key: Some(key),
            })
        })();
        result.map_err(|e| Failure {
            error: e.at(span),
            payload,
        })
    }
}
impl Default for SharedHeap {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

/// No Clone implementation: increasing strong ownership is explicit/fallible.
pub struct Strong {
    inner: Arc<Inner>,
    key: Option<Key>,
}
pub struct Weak {
    inner: Arc<Inner>,
    key: Option<Key>,
}
macro_rules! handle_debug {
    ($name:ident) => {
        impl Debug for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("key", &self.key)
                    .finish_non_exhaustive()
            }
        }
    };
}
handle_debug!(Strong);
handle_debug!(Weak);
impl Strong {
    fn key(&self) -> Key {
        self.key.expect("live external strong token")
    }
    pub fn try_clone(&self) -> Result<Self, Error> {
        let key = self.key();
        self.inner
            .lock()?
            .retain(key, self.inner.limits.reference_count)?;
        Ok(Self {
            inner: self.inner.clone(),
            key: Some(key),
        })
    }
    pub fn downgrade(&self) -> Result<Weak, Error> {
        let key = self.key();
        let mut state = self.inner.lock()?;
        state.node(key)?;
        state.retain_weak(key, self.inner.limits.reference_count)?;
        Ok(Weak {
            inner: self.inner.clone(),
            key: Some(key),
        })
    }
    pub fn strong_count(&self) -> Result<u64, Error> {
        Ok(self.inner.lock()?.node(self.key())?.strong)
    }
    /// A read guard owns a retained lease as well as a read registration. This
    /// keeps the value live even when a Rust host drops its original token or
    /// deliberately forgets the guard. Counts include these internal leases.
    pub fn read(&self) -> Result<Read, Error> {
        let mut state = self.inner.lock()?;
        let node = state.node_mut(self.key())?;
        let readers = increment(node.readers, self.inner.limits.reference_count)?;
        let strong = increment(node.strong, self.inner.limits.reference_count)?;
        let bytes = node.bytes.clone();
        node.readers = readers;
        node.strong = strong;
        Ok(Read {
            owner: Strong {
                inner: self.inner.clone(),
                key: Some(self.key()),
            },
            bytes,
        })
    }
    pub fn replace(&self, mut payload: Payload, span: Option<Span>) -> Result<(), Failure> {
        let result = (|| {
            let mut state = self.inner.lock()?;
            let key = self.key();
            if state.node(key)?.readers != 0 {
                return Err(Error::new(ErrorKind::BorrowConflict));
            }
            let edges = state.prepare(&self.inner, &payload, Some(key), span)?;
            let bytes: Arc<[u8]> = Arc::from(std::mem::take(&mut payload.bytes));
            let new_bytes = bytes.len();
            let node = state.node_mut(key)?;
            let old_bytes = std::mem::replace(&mut node.bytes, bytes);
            let old_edges = std::mem::replace(&mut node.edges, edges);
            disarm(&mut payload.edges);
            state.stats.payload_bytes = state.stats.payload_bytes - old_bytes.len() + new_bytes;
            state.stats.stored_edges =
                state.stats.stored_edges - old_edges.len() + payload.edges.len();
            // Both new and removed ownership fields are now committed. Release
            // old edges only afterwards, without a callback between transitions.
            state.release_edges(old_edges);
            drop(old_bytes);
            Ok(())
        })();
        result.map_err(|e| Failure {
            error: e.at(span),
            payload,
        })
    }
}
impl Weak {
    fn key(&self) -> Key {
        self.key.expect("live external weak token")
    }
    pub fn try_clone(&self) -> Result<Self, Error> {
        let key = self.key();
        self.inner
            .lock()?
            .retain_weak(key, self.inner.limits.reference_count)?;
        Ok(Self {
            inner: self.inner.clone(),
            key: Some(key),
        })
    }
    pub fn upgrade(&self) -> Result<Strong, Error> {
        let key = self.key();
        let mut state = self.inner.lock()?;
        if state.slot(key)?.node.is_none() {
            return Err(Error::new(ErrorKind::ExpiredWeak));
        }
        // Retain happens under the same lock as the live check; this cannot
        // return a merely observed, already-released target.
        state.retain(key, self.inner.limits.reference_count)?;
        Ok(Strong {
            inner: self.inner.clone(),
            key: Some(key),
        })
    }
}
pub struct Read {
    owner: Strong,
    bytes: Arc<[u8]>,
}
impl Read {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn edge_count(&self) -> Result<usize, Error> {
        Ok(self.owner.inner.lock()?.node(self.owner.key())?.edges.len())
    }
    pub fn clone_strong_edge(&self, index: usize) -> Result<Strong, Error> {
        let mut state = self.owner.inner.lock()?;
        let edge = state.edge(self.owner.key(), index)?;
        if edge.weak {
            return Err(Error::new(ErrorKind::WrongEdgeKind));
        }
        state.retain(edge.key, self.owner.inner.limits.reference_count)?;
        Ok(Strong {
            inner: self.owner.inner.clone(),
            key: Some(edge.key),
        })
    }
    pub fn clone_weak_edge(&self, index: usize) -> Result<Weak, Error> {
        let mut state = self.owner.inner.lock()?;
        let edge = state.edge(self.owner.key(), index)?;
        if !edge.weak {
            return Err(Error::new(ErrorKind::WrongEdgeKind));
        }
        state.retain_weak(edge.key, self.owner.inner.limits.reference_count)?;
        Ok(Weak {
            inner: self.owner.inner.clone(),
            key: Some(edge.key),
        })
    }
}
impl Drop for Read {
    fn drop(&mut self) {
        if let Ok(mut state) = self.owner.inner.lock() {
            state
                .node_mut(self.owner.key())
                .expect("read borrows live owner")
                .readers -= 1;
        }
    }
}
impl Drop for Strong {
    fn drop(&mut self) {
        if let Some(key) = self.key.take()
            && let Ok(mut state) = self.inner.lock()
        {
            state.release(StoredEdge {
                key,
                weak: false,
                span: None,
            });
        }
        // Poison quarantines state instead of invoking cleanup on unproven
        // invariants. No user code can poison the lock through this API.
    }
}
impl Drop for Weak {
    fn drop(&mut self) {
        if let Some(key) = self.key.take()
            && let Ok(mut state) = self.inner.lock()
        {
            state.release(StoredEdge {
                key,
                weak: true,
                span: None,
            });
        }
    }
}
fn disarm(edges: &mut [Edge]) {
    for edge in edges {
        match edge {
            Edge::Strong(h) => h.key = None,
            Edge::Weak(h) => h.key = None,
        }
    }
}
fn increment(value: u64, maximum: u64) -> Result<u64, Error> {
    value
        .checked_add(1)
        .filter(|n| *n <= maximum)
        .ok_or_else(|| Error::new(ErrorKind::CountOverflow))
}
fn check(request: Option<usize>, maximum: usize, kind: LimitKind) -> Result<usize, Error> {
    request
        .filter(|n| *n <= maximum)
        .ok_or_else(|| Error::new(ErrorKind::Limit(kind)))
}
fn reserve<T>(buffer: &mut Vec<T>, additional: usize) -> Result<(), Error> {
    buffer
        .try_reserve(additional)
        .map_err(|_| Error::new(ErrorKind::AllocationFailed))
}
impl State {
    fn slot(&self, key: Key) -> Result<&Slot, Error> {
        self.slots
            .get(key.slot)
            .filter(|s| s.generation == key.generation)
            .ok_or_else(|| Error::new(ErrorKind::Unavailable))
    }
    fn slot_mut(&mut self, key: Key) -> Result<&mut Slot, Error> {
        self.slots
            .get_mut(key.slot)
            .filter(|s| s.generation == key.generation)
            .ok_or_else(|| Error::new(ErrorKind::Unavailable))
    }
    fn node(&self, key: Key) -> Result<&Node, Error> {
        self.slot(key)?
            .node
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::Unavailable))
    }
    fn node_mut(&mut self, key: Key) -> Result<&mut Node, Error> {
        self.slot_mut(key)?
            .node
            .as_mut()
            .ok_or_else(|| Error::new(ErrorKind::Unavailable))
    }
    fn edge(&self, owner: Key, index: usize) -> Result<StoredEdge, Error> {
        self.node(owner)?
            .edges
            .get(index)
            .copied()
            .ok_or_else(|| Error::new(ErrorKind::WrongEdgeKind))
    }
    fn retain(&mut self, key: Key, maximum: u64) -> Result<(), Error> {
        let node = self.node_mut(key)?;
        node.strong = increment(node.strong, maximum)?;
        Ok(())
    }
    fn retain_weak(&mut self, key: Key, maximum: u64) -> Result<(), Error> {
        let slot = self.slot_mut(key)?;
        slot.weak = increment(slot.weak, maximum)?;
        Ok(())
    }
    fn prepare(
        &mut self,
        inner: &Arc<Inner>,
        payload: &Payload,
        replacing: Option<Key>,
        span: Option<Span>,
    ) -> Result<Vec<StoredEdge>, Error> {
        let old = replacing.map(|k| self.node(k)).transpose()?;
        check(
            self.stats
                .payload_bytes
                .checked_sub(old.map_or(0, |n| n.bytes.len()))
                .and_then(|n| n.checked_add(payload.bytes.len())),
            inner.limits.payload_bytes,
            LimitKind::PayloadBytes,
        )?;
        check(
            self.stats
                .stored_edges
                .checked_sub(old.map_or(0, |n| n.edges.len()))
                .and_then(|n| n.checked_add(payload.edges.len())),
            inner.limits.edges,
            LimitKind::Edges,
        )?;
        let mut edges = Vec::new();
        reserve(&mut edges, payload.edges.len())?;
        let mut work = Work {
            used: 0,
            limit: inner.limits.publication_work,
        };
        work.charge(1)?;
        for edge in &payload.edges {
            work.charge(1)?;
            let (owner, key, weak) = match edge {
                Edge::Strong(h) => (&h.inner, h.key(), false),
                Edge::Weak(h) => (&h.inner, h.key(), true),
            };
            if !Arc::ptr_eq(inner, owner) {
                return Err(Error::new(ErrorKind::ForeignHeap));
            }
            if weak {
                self.slot(key)?;
            } else {
                self.node(key)?;
            }
            edges.push(StoredEdge { key, weak, span });
        }
        if let Some(from) = replacing
            && edges.iter().any(|edge| !edge.weak)
        {
            // Reserve traversal memory before visiting the graph. All repeated
            // initialization, vertices and edges count against publication work.
            work.charge(self.slots.len())?;
            let mut seen = Vec::new();
            reserve(&mut seen, self.slots.len())?;
            seen.resize(self.slots.len(), false);
            let mut stack = Vec::new();
            reserve(&mut stack, self.slots.len())?;
            for edge in edges.iter().filter(|e| !e.weak) {
                work.charge(self.slots.len())?;
                seen.fill(false);
                stack.clear();
                if edge.key == from {
                    return Err(Error::new(ErrorKind::StrongCycle));
                }
                seen[edge.key.slot] = true;
                stack.push(edge.key);
                while let Some(key) = stack.pop() {
                    work.charge(1)?;
                    for path_edge in &self.node(key)?.edges {
                        work.charge(1)?;
                        if path_edge.weak {
                            continue;
                        }
                        if path_edge.key == from {
                            return Err(Error {
                                kind: ErrorKind::StrongCycle,
                                primary: span,
                                secondary: path_edge.span,
                            });
                        }
                        if !seen[path_edge.key.slot] {
                            seen[path_edge.key.slot] = true;
                            stack.push(path_edge.key);
                        }
                    }
                }
            }
        }
        // Retiring either the old or new graph never allocates. The pending
        // stack holds at most all stored ownership fields plus one root.
        let scratch = self
            .stats
            .stored_edges
            .checked_add(edges.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| Error::new(ErrorKind::AllocationFailed))?;
        let pending_len = self.pending.len();
        reserve(&mut self.pending, scratch - pending_len)?;
        Ok(edges)
    }
    fn recycle(&mut self, key: Key) {
        let slot = &mut self.slots[key.slot];
        if slot.node.is_none()
            && slot.weak == 0
            && let Some(next) = slot.generation.checked_add(1)
        {
            slot.generation = next;
            self.free.push(key.slot);
        }
    }
    fn release_edges(&mut self, edges: Vec<StoredEdge>) {
        debug_assert!(self.pending.is_empty());
        self.pending.extend(edges);
        self.drain_releases();
    }
    fn release(&mut self, edge: StoredEdge) {
        debug_assert!(self.pending.is_empty());
        self.pending.push(edge);
        self.drain_releases();
    }
    fn drain_releases(&mut self) {
        while let Some(edge) = self.pending.pop() {
            let slot = self
                .slot_mut(edge.key)
                .expect("retained field has valid generation");
            if edge.weak {
                slot.weak -= 1;
                self.recycle(edge.key);
                continue;
            }
            let node = slot.node.as_mut().expect("retained strong target is live");
            node.strong -= 1;
            if node.strong != 0 {
                continue;
            }
            debug_assert_eq!(node.readers, 0);
            // Revoke access before releasing fields, including self weak edges.
            let node = slot.node.take().expect("last strong node");
            self.stats.live_nodes -= 1;
            self.stats.payload_bytes -= node.bytes.len();
            self.stats.stored_edges -= node.edges.len();
            self.stats.reclaimed_nodes = self.stats.reclaimed_nodes.saturating_add(1);
            self.pending.extend(node.edges);
            drop(node.bytes);
            self.recycle(edge.key);
        }
    }
}
struct Work {
    used: usize,
    limit: usize,
}
impl Work {
    fn charge(&mut self, n: usize) -> Result<(), Error> {
        self.used = check(
            self.used.checked_add(n),
            self.limit,
            LimitKind::PublicationWork,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_keys_never_alias_reused_or_generation_exhausted_slots() {
        let heap = SharedHeap::default();
        let original = heap.allocate(Payload::default(), None).unwrap();
        let key = original.key();
        drop(original);
        let replacement = heap.allocate(Payload::default(), None).unwrap();
        assert_eq!(replacement.key().slot, key.slot);
        assert_ne!(replacement.key().generation, key.generation);
        assert_eq!(
            heap.inner.lock().unwrap().slot(key).err().unwrap().kind,
            ErrorKind::Unavailable
        );
        drop(replacement);
        let mut exhausted = heap.allocate(Payload::default(), None).unwrap();
        let key = exhausted.key();
        heap.inner.lock().unwrap().slots[key.slot].generation = u64::MAX;
        exhausted.key.as_mut().unwrap().generation = u64::MAX;
        drop(exhausted);
        let fresh = heap.allocate(Payload::default(), None).unwrap();
        assert_ne!(fresh.key().slot, key.slot);
    }

    #[test]
    fn actual_u64_count_overflow_does_not_wrap_or_modify_the_original_count() {
        let heap = SharedHeap::default();
        let owner = heap.allocate(Payload::default(), None).unwrap();
        let key = owner.key();
        heap.inner.lock().unwrap().node_mut(key).unwrap().strong = u64::MAX;
        assert_eq!(
            owner.try_clone().unwrap_err().kind,
            ErrorKind::CountOverflow
        );
        assert_eq!(owner.read().err().unwrap().kind, ErrorKind::CountOverflow);
        assert_eq!(owner.strong_count().unwrap(), u64::MAX);
        heap.inner.lock().unwrap().node_mut(key).unwrap().strong = 1;
    }

    #[test]
    fn poisoned_topology_is_quarantined_at_every_access_boundary() {
        let heap = SharedHeap::default();
        let owner = heap.allocate(Payload::default(), None).unwrap();
        let inner = heap.inner.clone();
        assert!(
            std::thread::spawn(move || {
                let _lock = inner.state.lock().unwrap();
                panic!("poison fixture");
            })
            .join()
            .is_err()
        );
        assert_eq!(owner.try_clone().unwrap_err().kind, ErrorKind::Poisoned);
        assert_eq!(
            heap.allocate(Payload::default(), None)
                .unwrap_err()
                .error
                .kind,
            ErrorKind::Poisoned
        );
        drop(owner); // Quarantine, rather than a destructor panic during unwind.
    }
}
