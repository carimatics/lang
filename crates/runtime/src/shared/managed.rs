//! Prepared shared ownership service. Tokens name live guest-backed registry
//! entries, not unchecked copies of reference counts or host model handles.
use super::storage as s;
use crate::{allocation as a, buffer, cleanup as c, layout as l};
use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};
mod access;
mod graph;
mod retirement;
use graph::Registry;

const HEADER: u64 = 40;
const ENTRY: u64 = 56;
const MAGIC: u64 = 0x7368_6172_6564_6772;
const STRONG: u64 = 1;
const WEAK: u64 = 2;
const READ: u64 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub nodes: usize,
    pub slots: usize,
    pub tickets: usize,
    pub readers: usize,
    pub value_cells: usize,
    pub publication_work: usize,
    pub reference_count: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 4096,
            slots: 8192,
            tickets: 8192,
            readers: 256,
            value_cells: 4096,
            publication_work: 1_000_000,
            reference_count: u64::MAX,
        }
    }
}
#[derive(Debug, Eq, PartialEq)]
pub enum Kind {
    Ownership(l::OwnershipError),
    Allocator(a::Error),
    Layout(l::Error),
    Buffer(buffer::Fault),
    Unsupported(o::Type),
    State,
}
#[derive(Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: Kind,
    pub primary: Option<m::Span>,
    pub secondary: Option<m::Span>,
}
impl Error {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            primary: None,
            secondary: None,
        }
    }
    fn at(mut self, span: Option<m::Span>) -> Self {
        self.primary = span;
        self
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "managed shared failure: {:?}", self.kind)
    }
}
impl std::error::Error for Error {}
fn core(error: l::OwnershipError) -> Error {
    Error::new(Kind::Ownership(error))
}
fn unavailable() -> Error {
    core(l::OwnershipError::Unavailable)
}
fn limit() -> Error {
    core(l::OwnershipError::Allocation(buffer::AllocError::Limit))
}
impl From<a::Error> for Error {
    fn from(error: a::Error) -> Self {
        match error {
            a::Error::Limit(_) => limit(),
            a::Error::Overflow => core(l::OwnershipError::Allocation(buffer::AllocError::Overflow)),
            a::Error::OutOfMemory => core(l::OwnershipError::Allocation(
                buffer::AllocError::OutOfMemory,
            )),
            error => Self::new(Kind::Allocator(error)),
        }
    }
}
impl From<l::Error> for Error {
    fn from(error: l::Error) -> Self {
        match error {
            l::Error::Allocation(error) => error.into(),
            error => Self::new(Kind::Layout(error)),
        }
    }
}
impl From<s::Error> for Error {
    fn from(error: s::Error) -> Self {
        match error {
            s::Error::Allocation(error) => error.into(),
            s::Error::Layout(error) => error.into(),
            s::Error::Unavailable => unavailable(),
            s::Error::ExpiredWeak => core(l::OwnershipError::ExpiredWeak),
            s::Error::BorrowConflict => core(l::OwnershipError::BorrowConflict),
            s::Error::CountOverflow => core(l::OwnershipError::CountOverflow),
            s::Error::State => Self::new(Kind::State),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Reference {
    owner: a::Handle,
    serial: u64,
}
impl Reference {
    fn bytes(self) -> [u8; 32] {
        let mut out = [0; 32];
        out[..24].copy_from_slice(&self.owner.token());
        out[24..].copy_from_slice(&self.serial.to_le_bytes());
        out
    }
    fn parse(bytes: [u8; 32]) -> Result<Self, Error> {
        Ok(Self {
            owner: a::Handle::from_token(bytes[..24].try_into().expect("handle"))
                .map_err(|_| unavailable())?,
            serial: u64::from_le_bytes(bytes[24..].try_into().expect("serial")),
        })
    }
}
#[derive(Debug)]
pub struct Strong(Reference);
#[derive(Debug)]
pub struct Weak(Reference);
macro_rules! owner_token {
    ($ty:ident) => {
        impl $ty {
            pub fn into_token(self) -> [u8; 32] {
                self.0.bytes()
            }
            pub fn from_token(bytes: &mut [u8; 32]) -> Result<Self, Error> {
                let reference = Reference::parse(*bytes)?;
                bytes.fill(0);
                Ok(Self(reference))
            }
        }
    };
}
owner_token!(Strong);
owner_token!(Weak);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Read(Reference);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldAction {
    CloneStrong,
    NewWeak,
    CloneWeak,
    UpgradeWeak,
}
#[derive(Debug)]
pub enum Retained {
    Strong(Strong),
    Weak(Weak),
}
struct Node<'l, 't> {
    block: s::Block<'l, 't>,
    ty: o::Type,
}
#[derive(Clone, Copy)]
struct Visit {
    location: l::Location,
    span: Option<m::Span>,
}
#[derive(Clone, Copy)]
struct Edge {
    reference: Reference,
    node: usize,
    kind: u64,
    span: Option<m::Span>,
}
struct Retirement<'l, 't> {
    index: usize,
    value: s::Retired<'l, 't>,
}
struct Work {
    remaining: usize,
}
impl Work {
    fn charge(&mut self, amount: usize) -> Result<(), Error> {
        self.remaining = self.remaining.checked_sub(amount).ok_or_else(limit)?;
        Ok(())
    }
}
pub struct Heap<'l, 't> {
    layouts: &'l l::Layouts<'t>,
    limits: Limits,
    owner: Option<a::Handle>,
    nodes: Vec<Option<Node<'l, 't>>>,
    walk: Vec<Visit>,
    incoming: Vec<Edge>,
    old_edges: Vec<Edge>,
    found: Vec<Edge>,
    graph: Vec<usize>,
    seen: Vec<bool>,
    retiring: Vec<Retirement<'l, 't>>,
    usage: m::Usage,
}
impl<'l, 't> Heap<'l, 't> {
    /// Exact fixed-pool admission without allocating trial wrapper vectors.
    pub fn preparation_usage(
        layouts: &'l l::Layouts<'t>,
        limits: Limits,
        host: m::Limits,
        initial: m::Usage,
    ) -> Result<m::Usage, Error> {
        let base = layouts.usage();
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(Error::new(Kind::State));
        }
        let mut budget = m::Budget::with_usage(host, initial).map_err(l::Error::from)?;
        budget
            .allocate::<Option<Node<'l, 't>>>(limits.slots)
            .map_err(l::Error::from)?;
        budget.charge(limits.slots).map_err(l::Error::from)?;
        budget
            .allocate::<Visit>(limits.value_cells)
            .map_err(l::Error::from)?;
        for _ in 0..3 {
            budget
                .allocate::<Edge>(limits.value_cells)
                .map_err(l::Error::from)?;
        }
        budget
            .allocate::<usize>(limits.slots)
            .map_err(l::Error::from)?;
        budget
            .allocate::<bool>(limits.slots)
            .map_err(l::Error::from)?;
        budget.charge(limits.slots).map_err(l::Error::from)?;
        budget
            .allocate::<Retirement<'l, 't>>(limits.slots)
            .map_err(l::Error::from)?;
        Self::request(limits)?;
        Ok(m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        })
    }
    pub fn prepare(
        layouts: &'l l::Layouts<'t>,
        limits: Limits,
        host: m::Limits,
        initial: m::Usage,
    ) -> Result<Self, Error> {
        let base = layouts.usage();
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(Error::new(Kind::State));
        }
        let mut budget = m::Budget::with_usage(host, initial).map_err(l::Error::from)?;
        let mut nodes = budget.vector(limits.slots).map_err(l::Error::from)?;
        budget.charge(limits.slots).map_err(l::Error::from)?;
        nodes.resize_with(limits.slots, || None);
        let walk = budget.vector(limits.value_cells).map_err(l::Error::from)?;
        let incoming = budget.vector(limits.value_cells).map_err(l::Error::from)?;
        let old_edges = budget.vector(limits.value_cells).map_err(l::Error::from)?;
        let found = budget.vector(limits.value_cells).map_err(l::Error::from)?;
        let graph = budget.vector(limits.slots).map_err(l::Error::from)?;
        let mut seen = budget.vector(limits.slots).map_err(l::Error::from)?;
        budget.charge(limits.slots).map_err(l::Error::from)?;
        seen.resize(limits.slots, false);
        let retiring = budget.vector(limits.slots).map_err(l::Error::from)?;
        Self::request(limits)?;
        Ok(Self {
            layouts,
            limits,
            owner: None,
            nodes,
            walk,
            incoming,
            old_edges,
            found,
            graph,
            seen,
            retiring,
            usage: m::Usage {
                work: budget.work_used(),
                allocation_bytes: budget.allocation_bytes(),
                elements: budget.elements_used(),
            },
        })
    }
    fn request(limits: Limits) -> Result<a::Request, Error> {
        Ok(a::Request::new(
            (limits.tickets as u64)
                .checked_mul(ENTRY)
                .and_then(|n| n.checked_add(HEADER))
                .ok_or(a::Error::Overflow)?,
            8,
        )?)
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub const fn is_open(&self) -> bool {
        self.owner.is_some()
    }
    pub fn open<P: a::Provider>(&mut self, allocator: &mut a::Allocator<P>) -> Result<(), Error> {
        if self.owner.is_some() {
            return Err(Error::new(Kind::State));
        }
        let owner = allocator.allocate(Self::request(self.limits)?)?;
        let bytes = allocator.bytes_mut(owner)?;
        bytes.fill(0);
        put(bytes, 0, MAGIC);
        for slot in 0..self.limits.tickets {
            put(bytes, HEADER as usize + slot * ENTRY as usize, slot as u64);
        }
        self.owner = Some(owner);
        Ok(())
    }
    fn owner<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<a::Handle, Error> {
        let owner = self.owner.ok_or_else(unavailable)?;
        if word(allocator.bytes(owner)?, 0) != MAGIC {
            return Err(Error::new(Kind::State));
        }
        Ok(owner)
    }
    fn work(&self) -> Work {
        Work {
            remaining: self.limits.publication_work,
        }
    }
    fn vacancy<P: a::Provider>(
        &self,
        allocator: &a::Allocator<P>,
        work: &mut Work,
    ) -> Result<Reference, Error> {
        let owner = self.owner(allocator)?;
        let bytes = allocator.bytes(owner)?;
        for slot in 0..self.limits.tickets {
            work.charge(1)?;
            let start = HEADER as usize + slot * ENTRY as usize;
            if word(bytes, start + 40) == 0
                && let Some(serial) = word(bytes, start).checked_add(self.limits.tickets as u64)
            {
                return Ok(Reference { owner, serial });
            }
        }
        Err(limit())
    }
    fn start(&self, reference: Reference) -> Result<usize, Error> {
        if self.owner != Some(reference.owner) || self.limits.tickets == 0 {
            return Err(unavailable());
        }
        Ok(HEADER as usize
            + (reference.serial % self.limits.tickets as u64) as usize * ENTRY as usize)
    }
    fn register<P: a::Provider>(
        &self,
        reference: Reference,
        node: usize,
        kind: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let start = self.start(reference)?;
        let header = self.nodes[node]
            .as_ref()
            .ok_or_else(unavailable)?
            .block
            .identity()?;
        let bytes = allocator.bytes_mut(reference.owner)?;
        put(bytes, start, reference.serial);
        put(bytes, start + 8, node as u64);
        bytes[start + 16..start + 40].copy_from_slice(&header.token());
        put(bytes, start + 40, kind);
        put(bytes, start + 48, 0);
        Ok(())
    }
    fn resolve<P: a::Provider>(
        &self,
        reference: Reference,
        kind: u64,
        ty: o::Type,
        allocator: &a::Allocator<P>,
    ) -> Result<usize, Error> {
        Registry::new(self.owner, &self.nodes, self.layouts, self.limits)
            .resolve(reference, kind, ty, 0, allocator)
    }
    fn consume<P: a::Provider>(
        &self,
        reference: Reference,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let start = self.start(reference)?;
        put(allocator.bytes_mut(reference.owner)?, start + 40, 0);
        Ok(())
    }
    pub fn allocate_from<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Strong, Error> {
        self.allocate(input, span, allocator, false)
    }
    pub fn allocate_copy<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Strong, Error> {
        self.allocate(input, span, allocator, true)
    }
    fn allocate<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
        copy: bool,
    ) -> Result<Strong, Error> {
        let result = (|| {
            let owner = self.owner(allocator)?;
            let mut work = self.work();
            work.charge(1)?;
            if self.limits.reference_count == 0 {
                return Err(core(l::OwnershipError::CountOverflow));
            }
            let ty = input.root().ty();
            if copy
                && !self
                    .layouts
                    .types()
                    .properties(ty)
                    .is_some_and(|p| p.is_copy)
            {
                return Err(l::Error::InvalidType(ty).into());
            }
            self.validate_input(input, span, allocator, &mut work)?;
            if word(allocator.bytes(owner)?, 8) >= self.limits.nodes as u64 {
                return Err(limit());
            }
            let mut free = None;
            for (i, node) in self.nodes.iter().enumerate() {
                work.charge(1)?;
                if node.is_none() {
                    free = Some(i);
                    break;
                }
            }
            let index = free.ok_or_else(limit)?;
            let reference = self.vacancy(allocator, &mut work)?;
            let block = if copy {
                s::Block::allocate_copy(input, span, allocator)?
            } else {
                s::Block::allocate_from(input, span, allocator)?
            };
            self.nodes[index] = Some(Node { block, ty });
            self.register(reference, index, STRONG, allocator)?;
            self.mark(&self.incoming, index as u64 + 1, allocator)?;
            let bytes = allocator.bytes_mut(owner)?;
            put(bytes, 8, word(bytes, 8) + 1);
            Ok(Strong(reference))
        })();
        result.map_err(|e: Error| e.at(span))
    }
    pub fn strong_count<P: a::Provider>(
        &self,
        strong: &Strong,
        ty: o::Type,
        allocator: &a::Allocator<P>,
    ) -> Result<u64, Error> {
        let index = self.resolve(strong.0, STRONG, ty, allocator)?;
        Ok(self.nodes[index]
            .as_ref()
            .unwrap()
            .block
            .counts(allocator)?
            .strong)
    }
    pub fn clone_strong<P: a::Provider>(
        &mut self,
        strong: &Strong,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Strong, Error> {
        let index = self.resolve(strong.0, STRONG, ty, allocator)?;
        let reference = self.vacancy(allocator, &mut self.work())?;
        self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .retain_strong(self.limits.reference_count, allocator)?;
        self.register(reference, index, STRONG, allocator)?;
        Ok(Strong(reference))
    }
    pub fn downgrade<P: a::Provider>(
        &mut self,
        strong: &Strong,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Weak, Error> {
        let index = self.resolve(strong.0, STRONG, ty, allocator)?;
        let reference = self.vacancy(allocator, &mut self.work())?;
        self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .downgrade(self.limits.reference_count, allocator)?;
        self.register(reference, index, WEAK, allocator)?;
        Ok(Weak(reference))
    }
    pub fn upgrade<P: a::Provider>(
        &mut self,
        weak: &Weak,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Strong, Error> {
        let index = self.resolve(weak.0, WEAK, ty, allocator)?;
        if self.nodes[index]
            .as_ref()
            .unwrap()
            .block
            .counts(allocator)?
            .strong
            == 0
        {
            return Err(core(l::OwnershipError::ExpiredWeak));
        }
        let reference = self.vacancy(allocator, &mut self.work())?;
        self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .upgrade(self.limits.reference_count, allocator)?;
        self.register(reference, index, STRONG, allocator)?;
        Ok(Strong(reference))
    }
    pub fn clone_weak<P: a::Provider>(
        &mut self,
        weak: &Weak,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Weak, Error> {
        let index = self.resolve(weak.0, WEAK, ty, allocator)?;
        let reference = self.vacancy(allocator, &mut self.work())?;
        self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .retain_weak(self.limits.reference_count, allocator)?;
        self.register(reference, index, WEAK, allocator)?;
        Ok(Weak(reference))
    }
    pub fn release_strong<P: a::Provider>(
        &mut self,
        strong: Strong,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        self.drop_reference(strong.0, STRONG, ty, 0, allocator)?;
        self.drain_pending(allocator, context, &mut cleaner)
    }
    pub fn release_weak<P: a::Provider>(
        &mut self,
        weak: Weak,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.drop_reference(weak.0, WEAK, ty, 0, allocator)
    }
    pub fn close<P: a::Provider>(&mut self, allocator: &mut a::Allocator<P>) -> Result<(), Error> {
        let owner = self.owner(allocator)?;
        if self.nodes.iter().any(Option::is_some) {
            return Err(Error::new(Kind::State));
        }
        allocator.release(owner)?;
        self.owner = None;
        Ok(())
    }
}
fn word(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("registry word"))
}
fn put(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
