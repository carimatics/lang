//! Typed guest arena storage under the instance allocator. Prepare bounded host
//! wrappers before entry; payload, construction order and retirement work use
//! real guest reservations. No Arc, lock or host allocation occurs in operations.
//! Like Reservation, wrapper disposal alone runs no guest cleanup: the adapter
//! must drain every node/descendant before releasing its allocator authority.
use crate::{allocation as a, buffer, cleanup as c, inspection as i, layout as l};
use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};

const HEADER: u64 = 40;
const READ_ENTRY: u64 = 16;
const NODE_HEADER: u64 = 56;
const MAGIC: u64 = 0x616e_6572_612d_326d;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub nodes: usize,
    pub value_cells: usize,
    pub publication_work: usize,
    pub readers: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 4096,
            value_cells: 4096,
            publication_work: 1_000_000,
            readers: 256,
        }
    }
}
#[derive(Debug, Eq, PartialEq)]
pub enum Kind {
    Arena(l::ArenaError),
    Allocator(a::Error),
    Layout(l::Error),
    Buffer(buffer::Fault),
    Inspection(i::Error),
    Unsupported(o::Type),
    State,
}
#[derive(Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: Kind,
    pub span: Option<m::Span>,
}
impl Error {
    fn new(kind: Kind) -> Self {
        Self { kind, span: None }
    }
    fn at(mut self, span: Option<m::Span>) -> Self {
        self.span = span;
        self
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "managed arena failure: {:?}", self.kind)
    }
}
impl std::error::Error for Error {}
impl From<l::Error> for Error {
    fn from(error: l::Error) -> Self {
        match error {
            l::Error::Allocation(error) => error.into(),
            error => Self::new(Kind::Layout(error)),
        }
    }
}
impl From<i::Error> for Error {
    fn from(error: i::Error) -> Self {
        match error {
            i::Error::Cells | i::Error::Work => limit(),
            i::Error::Layout(e) => e.into(),
            e => Self::new(Kind::Inspection(e)),
        }
    }
}
impl From<a::Error> for Error {
    fn from(error: a::Error) -> Self {
        match error {
            a::Error::Limit(_) => core(l::ArenaError::Allocation(buffer::AllocError::Limit)),
            a::Error::Overflow => core(l::ArenaError::Allocation(buffer::AllocError::Overflow)),
            a::Error::OutOfMemory => {
                core(l::ArenaError::Allocation(buffer::AllocError::OutOfMemory))
            }
            error => Self::new(Kind::Allocator(error)),
        }
    }
}
fn core(error: l::ArenaError) -> Error {
    Error::new(Kind::Arena(error))
}
fn limit() -> Error {
    core(l::ArenaError::Allocation(buffer::AllocError::Limit))
}
fn unavailable() -> Error {
    core(l::ArenaError::Unavailable)
}

/// A logical Copy token. Its private 32-byte representation is not a wire/ABI
/// format, allocator authority or ownership of the referenced node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reference {
    owner: a::Handle,
    node: u64,
}
impl Reference {
    pub fn token(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[..24].copy_from_slice(&self.owner.token());
        bytes[24..].copy_from_slice(&self.node.to_le_bytes());
        bytes
    }
    pub fn from_token(bytes: [u8; 32]) -> Result<Self, Error> {
        Ok(Self {
            owner: a::Handle::from_token(bytes[..24].try_into().expect("fixed token"))
                .map_err(|_| unavailable())?,
            node: u64::from_le_bytes(bytes[24..].try_into().expect("fixed token")),
        })
    }
}
/// Explicit registered read, consumed by `end_read`. Forgetting a trusted host
/// wrapper keeps the read registered until abort; no implicit guest cleanup.
pub struct Read {
    reference: Reference,
    slot: usize,
    serial: u64,
    active: bool,
}
/// A checked read view never exposes the reservation's mutation/retirement
/// operations. Retaining this Rust borrow also excludes arena mutation.
pub struct View<'a, 'l, 't> {
    payload: &'a l::Reservation<'l, 't>,
}
impl<'l, 't> View<'_, 'l, 't> {
    pub fn copy_into<P: a::Provider>(
        &self,
        from: l::Location,
        destination: &l::Reservation<'l, 't>,
        target: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), l::Error> {
        destination.copy_from(target, self.payload, from, allocator)
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exit {
    Normal,
    Abort,
}
#[derive(Clone, Copy)]
enum State {
    Closed,
    Open(a::Handle),
    Retiring(a::Handle),
}
struct Node<'l, 't> {
    payload: l::Reservation<'l, 't>,
    header: a::Handle,
}
pub struct Retired<'l, 't> {
    pub reference: Reference,
    pub payload: l::Reservation<'l, 't>,
    pub span: Option<m::Span>,
}
/// Shared by the inline walk and an indirect service's recursive validation.
/// Services must charge before inspecting each nested value/edge.
pub struct PublicationBudget {
    work: i::Work,
}
impl PublicationBudget {
    pub fn charge(&mut self, amount: usize) -> Result<(), Error> {
        self.work.charge(amount).map_err(|_| limit())
    }
    pub fn inspect_next<P: a::Provider>(
        &mut self,
        read: &mut i::Read<'_, '_, '_, P>,
    ) -> Result<Option<i::Observation>, Error> {
        read.next(&mut self.work).map_err(Into::into)
    }
}
pub struct Arena<'l, 't> {
    layouts: &'l l::Layouts<'t>,
    limits: Limits,
    state: State,
    nodes: Vec<Node<'l, 't>>,
    pending: Vec<l::Location>,
    opening: a::Request,
    usage: m::Usage,
}
impl<'l, 't> Arena<'l, 't> {
    /// Exact host-pool admission without allocating trial buffers. Execution
    /// adapters use this while choosing a bounded activation capacity.
    pub fn preparation_usage(
        layouts: &'l l::Layouts<'t>,
        limits: Limits,
        host_limits: m::Limits,
        initial: m::Usage,
    ) -> Result<m::Usage, Error> {
        let base = layouts.usage();
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(Error::new(Kind::State));
        }
        let mut budget = m::Budget::with_usage(host_limits, initial).map_err(l::Error::from)?;
        budget
            .allocate::<Node<'l, 't>>(limits.nodes)
            .map_err(l::Error::from)?;
        budget
            .allocate::<l::Location>(limits.value_cells)
            .map_err(l::Error::from)?;
        a::Request::new(
            (limits.readers as u64)
                .checked_mul(READ_ENTRY)
                .and_then(|n| n.checked_add(HEADER))
                .ok_or(a::Error::Overflow)?,
            8,
        )?;
        Ok(m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        })
    }
    /// Trusted pre-entry preparation; both host wrapper/walk pools are bounded and
    /// charged to the caller's cumulative preparation budget.
    pub fn prepare(
        layouts: &'l l::Layouts<'t>,
        limits: Limits,
        host_limits: m::Limits,
        initial: m::Usage,
    ) -> Result<Self, Error> {
        let base = layouts.usage();
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(Error::new(Kind::State));
        }
        let mut budget = m::Budget::with_usage(host_limits, initial).map_err(l::Error::from)?;
        let nodes = budget.vector(limits.nodes).map_err(l::Error::from)?;
        let pending = budget.vector(limits.value_cells).map_err(l::Error::from)?;
        let opening = a::Request::new(
            (limits.readers as u64)
                .checked_mul(READ_ENTRY)
                .and_then(|n| n.checked_add(HEADER))
                .ok_or(a::Error::Overflow)?,
            8,
        )?;
        Ok(Self {
            layouts,
            limits,
            state: State::Closed,
            nodes,
            pending,
            opening,
            usage: m::Usage {
                work: budget.work_used(),
                allocation_bytes: budget.allocation_bytes(),
                elements: budget.elements_used(),
            },
        })
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub const fn opening_request(&self) -> a::Request {
        self.opening
    }
    pub fn open<P: a::Provider>(&mut self, allocator: &mut a::Allocator<P>) -> Result<(), Error> {
        if !matches!(self.state, State::Closed) || !self.nodes.is_empty() {
            return Err(Error::new(Kind::State));
        }
        let owner = allocator.allocate(self.opening)?;
        let bytes = allocator.bytes_mut(owner)?;
        bytes.fill(0);
        bytes[16..24].copy_from_slice(&1u64.to_le_bytes());
        bytes[24..32].copy_from_slice(&MAGIC.to_le_bytes());
        write(bytes, 32, 1);
        self.state = State::Open(owner);
        Ok(())
    }
    fn owner<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<a::Handle, Error> {
        let State::Open(owner) = self.state else {
            return Err(unavailable());
        };
        let bytes = allocator.bytes(owner)?;
        if bytes.len() != self.opening.bytes() as usize
            || word(bytes, 0)? != self.nodes.len() as u64
            || word(bytes, 8)?
                != bytes[HEADER as usize..]
                    .as_chunks::<{ READ_ENTRY as usize }>()
                    .0
                    .iter()
                    .filter(|r| word(r.as_slice(), 0) != Ok(0))
                    .count() as u64
            || word(bytes, 16)? != 1
            || word(bytes, 24)? != MAGIC
        {
            return Err(Error::new(Kind::State));
        }
        Ok(owner)
    }
    fn writable<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<a::Handle, Error> {
        let owner = self.owner(allocator)?;
        if word(allocator.bytes(owner)?, 8)? != 0 {
            return Err(core(l::ArenaError::BorrowConflict));
        }
        Ok(owner)
    }
    /// Validate liveness, owner provenance, index and payload type. A live
    /// different owner is CrossRegion; a stale/forged owner is Unavailable.
    pub fn resolve<P: a::Provider>(
        &self,
        reference: Reference,
        ty: o::Type,
        allocator: &a::Allocator<P>,
    ) -> Result<(), Error> {
        let owner = self.owner(allocator)?;
        if !allocator.bytes(reference.owner).is_ok_and(|bytes| {
            bytes.len() >= HEADER as usize
                && (bytes.len() - HEADER as usize).is_multiple_of(READ_ENTRY as usize)
                && word(bytes, 16) == Ok(1)
                && word(bytes, 24) == Ok(MAGIC)
        }) {
            return Err(unavailable());
        }
        if owner != reference.owner {
            return Err(core(l::ArenaError::CrossRegion));
        }
        let node = usize::try_from(reference.node)
            .ok()
            .and_then(|index| self.nodes.get(index))
            .ok_or_else(unavailable)?;
        node_header(node.header, reference, allocator)?;
        if self
            .layouts
            .types()
            .equivalent(node.payload.root().ty(), ty)
            != Some(true)
        {
            return Err(unavailable());
        }
        Ok(())
    }
    fn validate<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        allocator: &mut a::Allocator<P>,
        inspect: &mut impl FnMut(
            &l::Token,
            &Self,
            &a::Allocator<P>,
            &mut PublicationBudget,
        ) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if !std::ptr::eq(input.layouts(), self.layouts) {
            return Err(Error::new(Kind::State));
        }
        input.validate_retirement(allocator)?;
        let mut budget = PublicationBudget {
            work: i::Work::new(self.limits.publication_work),
        };
        self.pending.clear();
        if self.limits.value_cells == 0 {
            return Err(limit());
        }
        self.pending.push(input.root());
        while let Some(location) = self.pending.pop() {
            budget.charge(1)?;
            let layout = self
                .layouts
                .get(location.ty())
                .ok_or_else(|| Error::new(Kind::State))?;
            let fields = match &layout.kind {
                l::Kind::Record(fields) => Some(fields),
                l::Kind::Enum { cases, .. } => {
                    let case = input.case(location, allocator)?;
                    Some(
                        cases
                            .get(case as usize)
                            .ok_or_else(|| Error::new(Kind::State))?,
                    )
                }
                l::Kind::Scalar | l::Kind::Error { .. } => {
                    input.read_immediate(location, allocator)?;
                    None
                }
                l::Kind::Token(constructor) => {
                    let start = location.offset() as usize;
                    let mut bytes: [u8; 32] = input
                        .data(allocator)?
                        .get(
                            start
                                ..start
                                    .checked_add(32)
                                    .ok_or_else(|| Error::new(Kind::State))?,
                        )
                        .ok_or_else(|| Error::new(Kind::State))?
                        .try_into()
                        .map_err(|_| Error::new(Kind::State))?;
                    match constructor {
                        o::Constructor::Arena(ty) => {
                            self.resolve(Reference::from_token(bytes)?, *ty, allocator)?
                        }
                        o::Constructor::Shared(_) | o::Constructor::Weak(_) => {
                            return Err(core(l::ArenaError::CrossRegion));
                        }
                        o::Constructor::Builtin(o::Builtin::Bytes) => {
                            buffer::Bytes::from_token(&mut bytes, allocator)
                                .and_then(|value| value.len(allocator))
                                .map_err(|e| Error::new(Kind::Buffer(e)))?;
                        }
                        o::Constructor::Builtin(o::Builtin::Text) => {
                            let text = buffer::Text::from_token(&mut bytes, allocator)
                                .map_err(|e| Error::new(Kind::Buffer(e)))?;
                            let length = usize::try_from(
                                text.len(allocator)
                                    .map_err(|e| Error::new(Kind::Buffer(e)))?,
                            )
                            .map_err(|_| Error::new(Kind::State))?;
                            budget.charge(length)?;
                            text.as_str(allocator)
                                .map_err(|e| Error::new(Kind::Buffer(e)))?;
                        }
                        o::Constructor::Vec(_) | o::Constructor::Map(..) => inspect(
                            &l::Token {
                                leaf: l::Leaf {
                                    ty: location.ty(),
                                    offset: location.offset(),
                                    span: None,
                                },
                                bytes,
                            },
                            self,
                            allocator,
                            &mut budget,
                        )?,
                        _ => return Err(Error::new(Kind::Unsupported(location.ty()))),
                    }
                    None
                }
            };
            if let Some(fields) = fields {
                if self
                    .pending
                    .len()
                    .checked_add(fields.fields.len())
                    .is_none_or(|n| n > self.limits.value_cells)
                {
                    return Err(limit());
                }
                budget.charge(fields.fields.len())?;
                for field in 0..fields.fields.len() {
                    self.pending
                        .push(input.child(location, field as u32, allocator)?);
                }
            }
        }
        Ok(())
    }
    /// Allocate all payload/order/work storage before moving the operand. On
    /// failure the input obligation and every published node remain unchanged.
    /// Collection services must validate all nested brands with `inspect`;
    /// unsupported services must return an error, never silently skip a token.
    pub fn allocate_from<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
        inspect: impl FnMut(
            &l::Token,
            &Self,
            &a::Allocator<P>,
            &mut PublicationBudget,
        ) -> Result<(), Error>,
    ) -> Result<Reference, Error> {
        self.allocate(input, span, allocator, inspect, false)
    }
    /// Preserve a Copy operand while publishing a node with the copied value.
    pub fn allocate_copy<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
        inspect: impl FnMut(
            &l::Token,
            &Self,
            &a::Allocator<P>,
            &mut PublicationBudget,
        ) -> Result<(), Error>,
    ) -> Result<Reference, Error> {
        self.allocate(input, span, allocator, inspect, true)
    }
    fn allocate<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
        mut inspect: impl FnMut(
            &l::Token,
            &Self,
            &a::Allocator<P>,
            &mut PublicationBudget,
        ) -> Result<(), Error>,
        copy: bool,
    ) -> Result<Reference, Error> {
        let result = (|| {
            let owner = self.writable(allocator)?;
            if copy
                && !self
                    .layouts
                    .types()
                    .properties(input.root().ty())
                    .is_some_and(|p| p.is_copy)
            {
                return Err(l::Error::InvalidType(input.root().ty()).into());
            }
            if span.is_some_and(|span| !self.layouts.span_valid(span)) {
                return Err(l::Error::InvalidSpan.into());
            }
            if self.nodes.len() == self.limits.nodes {
                return Err(limit());
            }
            self.validate(input, allocator, &mut inspect)?;
            let requests = self
                .layouts
                .get(input.root().ty())
                .ok_or_else(|| Error::new(Kind::State))?
                .storage_requests()?;
            let node_request = a::Request::new(NODE_HEADER, 8)?;
            allocator.check(&[node_request, requests[0], requests[1]])?;
            let header = allocator.allocate(node_request)?;
            let mut payload =
                match l::Reservation::allocate(self.layouts, input.root().ty(), allocator) {
                    Ok(payload) => payload,
                    Err(error) => {
                        allocator.release(header)?;
                        return Err(error.into());
                    }
                };
            let transferred = if copy {
                payload.copy_from(payload.root(), input, input.root(), allocator)
            } else {
                input.move_root_into(&payload, payload.root(), allocator)
            };
            if let Err(error) = transferred {
                payload.release_storage(allocator)?;
                allocator.release(header)?;
                return Err(error.into());
            }
            let node = self.nodes.len() as u64;
            let bytes = allocator.bytes_mut(header)?;
            bytes.fill(0);
            bytes[..24].copy_from_slice(&owner.token());
            write(bytes, 24, node);
            store_span(bytes, span);
            write(bytes, 48, 1);
            self.nodes.push(Node { payload, header });
            write(allocator.bytes_mut(owner)?, 0, node + 1);
            Ok(Reference { owner, node })
        })();
        result.map_err(|e: Error| e.at(span))
    }
    /// Publication exchanges the new root into the node without allocation.
    /// Success leaves the old value in `input` for cleanup outside publication.
    pub fn replace_from<P: a::Provider>(
        &mut self,
        reference: Reference,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
        inspect: impl FnMut(
            &l::Token,
            &Self,
            &a::Allocator<P>,
            &mut PublicationBudget,
        ) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.replace(reference, input, span, allocator, inspect, false)
            .map(|_| ())
    }
    /// Publish an owning replacement and return the old node's span for its
    /// detached cleanup obligation, which remains in `input`.
    pub fn replace_owned<P: a::Provider>(
        &mut self,
        reference: Reference,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
        inspect: impl FnMut(
            &l::Token,
            &Self,
            &a::Allocator<P>,
            &mut PublicationBudget,
        ) -> Result<(), Error>,
    ) -> Result<Option<m::Span>, Error> {
        self.replace(reference, input, span, allocator, inspect, false)
    }
    /// Copy replacement has no old owning obligation to retire.
    pub fn replace_copy<P: a::Provider>(
        &mut self,
        reference: Reference,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
        inspect: impl FnMut(
            &l::Token,
            &Self,
            &a::Allocator<P>,
            &mut PublicationBudget,
        ) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.replace(reference, input, span, allocator, inspect, true)
            .map(|_| ())
    }
    fn replace<P: a::Provider>(
        &mut self,
        reference: Reference,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
        mut inspect: impl FnMut(
            &l::Token,
            &Self,
            &a::Allocator<P>,
            &mut PublicationBudget,
        ) -> Result<(), Error>,
        copy: bool,
    ) -> Result<Option<m::Span>, Error> {
        let result = (|| {
            self.writable(allocator)?;
            if !self.layouts.span_valid(span) {
                return Err(l::Error::InvalidSpan.into());
            }
            self.resolve(reference, input.root().ty(), allocator)?;
            self.validate(input, allocator, &mut inspect)?;
            let node = &mut self.nodes[reference.node as usize];
            let old_span = node_header(node.header, reference, allocator)?;
            if copy {
                node.payload
                    .copy_from(node.payload.root(), input, input.root(), allocator)?;
            } else {
                node.payload
                    .exchange_root_at(node.payload.root(), input, span, allocator)?;
            }
            store_span(allocator.bytes_mut(node.header)?, Some(span));
            Ok(old_span)
        })();
        result.map_err(|e: Error| e.at(Some(span)))
    }
    pub fn read<P: a::Provider>(
        &mut self,
        reference: Reference,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Read, Error> {
        self.resolve(reference, ty, allocator)?;
        let owner = self.owner(allocator)?;
        let bytes = allocator.bytes(owner)?;
        let serial = word(bytes, 32)?;
        if serial == 0 {
            return Err(core(l::ArenaError::Allocation(
                buffer::AllocError::Overflow,
            )));
        }
        let slot = bytes[HEADER as usize..]
            .as_chunks::<{ READ_ENTRY as usize }>()
            .0
            .iter()
            .position(|row| word(row, 0) == Ok(0))
            .ok_or_else(limit)?;
        let count = word(bytes, 8)? + 1;
        let bytes = allocator.bytes_mut(owner)?;
        write(bytes, 32, serial.checked_add(1).unwrap_or(0));
        write(bytes, HEADER as usize + slot * READ_ENTRY as usize, serial);
        write(
            bytes,
            HEADER as usize + slot * READ_ENTRY as usize + 8,
            reference.node,
        );
        write(bytes, 8, count);
        Ok(Read {
            reference,
            slot,
            serial,
            active: true,
        })
    }
    pub fn payload<P: a::Provider>(
        &self,
        read: &Read,
        allocator: &a::Allocator<P>,
    ) -> Result<View<'_, 'l, 't>, Error> {
        let owner = self.owner(allocator)?;
        if !read.active || owner != read.reference.owner || read.slot >= self.limits.readers {
            return Err(unavailable());
        }
        let bytes = allocator.bytes(owner)?;
        let offset = HEADER as usize + read.slot * READ_ENTRY as usize;
        if word(bytes, offset)? != read.serial || word(bytes, offset + 8)? != read.reference.node {
            return Err(unavailable());
        }
        let node = self
            .nodes
            .get(read.reference.node as usize)
            .ok_or_else(unavailable)?;
        node_header(node.header, read.reference, allocator)?;
        Ok(View {
            payload: &node.payload,
        })
    }
    pub fn end_read<P: a::Provider>(
        &mut self,
        read: &mut Read,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.payload(read, allocator)?;
        let owner = self.owner(allocator)?;
        read.active = false;
        let count = word(allocator.bytes(owner)?, 8)? - 1;
        let bytes = allocator.bytes_mut(owner)?;
        write(bytes, HEADER as usize + read.slot * READ_ENTRY as usize, 0);
        write(
            bytes,
            HEADER as usize + read.slot * READ_ENTRY as usize + 8,
            0,
        );
        write(bytes, 8, count);
        Ok(())
    }
    /// Revoke all token/read access before any child cleanup. Normal exit
    /// refuses live reads; abort also invalidates abandoned trusted host reads.
    pub fn begin_close<P: a::Provider>(
        &mut self,
        exit: Exit,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let owner = match self.state {
            State::Open(_) => self.owner(allocator)?,
            State::Retiring(_) => return Ok(()),
            State::Closed => return Err(unavailable()),
        };
        if exit == Exit::Normal && word(allocator.bytes(owner)?, 8)? != 0 {
            return Err(core(l::ArenaError::BorrowConflict));
        }
        self.state = State::Retiring(owner);
        let bytes = allocator.bytes_mut(owner)?;
        write(bytes, 8, 0);
        bytes[HEADER as usize..].fill(0);
        write(bytes, 16, 0);
        Ok(())
    }
    /// Detach each node once, in reverse allocation order, without allocating.
    /// The adapter must drain its descendants before releasing its reservation.
    pub fn take_last<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<Retired<'l, 't>>, Error> {
        let State::Retiring(owner) = self.state else {
            return Err(Error::new(Kind::State));
        };
        let Some(index) = self.nodes.len().checked_sub(1) else {
            return Ok(None);
        };
        let reference = Reference {
            owner,
            node: index as u64,
        };
        let span = node_header(self.nodes[index].header, reference, allocator)?;
        let node = self.nodes.pop().expect("checked node");
        write(allocator.bytes_mut(node.header)?, 48, 0);
        allocator.release(node.header)?;
        Ok(Some(Retired {
            reference,
            payload: node.payload,
            span,
        }))
    }
    pub fn finish_close<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let State::Retiring(owner) = self.state else {
            return Err(Error::new(Kind::State));
        };
        if !self.nodes.is_empty() {
            return Err(Error::new(Kind::State));
        }
        allocator.request(owner)?;
        self.state = State::Closed;
        allocator.release(owner)?;
        Ok(())
    }
    /// All descendant errors/panics share the caller's cleanup report. Consume
    /// nodes before callbacks and continue independent nodes after a layout fault.
    pub fn cleanup<P: a::Provider>(
        &mut self,
        exit: Exit,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        if matches!(self.state, State::Closed) {
            return Ok(());
        }
        self.begin_close(exit, allocator)?;
        let mut first = None;
        while let Some(retired) = self.take_last(allocator)? {
            let mut nested = c::Contextual::new(
                |payload: &mut l::Reservation<'l, 't>, _, context: &mut c::Context<'_>| {
                    payload
                        .cleanup(allocator, context, &mut cleaner)
                        .map_err(|error| {
                            first.get_or_insert_with(|| Error::from(error).at(retired.span));
                            // A malformed trusted payload is an invariant failure;
                            // independently valid nodes must still retire.
                            let _ = payload.release_storage(allocator);
                            u32::MAX
                        })
                },
            );
            context.field(retired.payload, retired.span, &mut nested);
        }
        self.finish_close(allocator)?;
        first.map_or(Ok(()), Err)
    }
}
fn word(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    bytes
        .get(offset..offset + 8)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| Error::new(Kind::State))
}
fn node_header<P: a::Provider>(
    header: a::Handle,
    reference: Reference,
    allocator: &a::Allocator<P>,
) -> Result<Option<m::Span>, Error> {
    let bytes = allocator.bytes(header)?;
    if bytes.len() != NODE_HEADER as usize
        || bytes[..24] != reference.owner.token()
        || word(bytes, 24)? != reference.node
        || word(bytes, 48)? != 1
    {
        return Err(Error::new(Kind::State));
    }
    match u32::from_le_bytes(bytes[44..48].try_into().expect("fixed span")) {
        0 => Ok(None),
        1 => Ok(Some(m::Span {
            source: m::SourceId(u32::from_le_bytes(
                bytes[32..36].try_into().expect("fixed source"),
            )),
            start: u32::from_le_bytes(bytes[36..40].try_into().expect("fixed start")),
            end: u32::from_le_bytes(bytes[40..44].try_into().expect("fixed end")),
        })),
        _ => Err(Error::new(Kind::State)),
    }
}
fn store_span(bytes: &mut [u8], span: Option<m::Span>) {
    bytes[32..48].fill(0);
    if let Some(span) = span {
        bytes[32..36].copy_from_slice(&span.source.0.to_le_bytes());
        bytes[36..40].copy_from_slice(&span.start.to_le_bytes());
        bytes[40..44].copy_from_slice(&span.end.to_le_bytes());
        bytes[44..48].copy_from_slice(&1u32.to_le_bytes());
    }
}
fn write(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
