//! Scoped native callback storage. Payloads use the original allocator; stable
//! handle headers are bounded caller metadata and remain until callback close.
use super::*;
use crate::allocation as a;
use std::ptr;

#[derive(Clone, Copy)]
pub struct NativeReturn {
    pub status: u32,
    pub buffer: *mut super::super::native::Buffer,
}
#[derive(Debug)]
pub enum NativeFailure {
    Invocation(u32),
    Status {
        code: u32,
        bytes: Vec<u8>,
    },
    MissingResult,
    InvalidResult(wire::Error),
    Resource {
        index: u32,
        kind: NativeResourceFailure,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeResourceFailure {
    UnknownIndex,
    NotOwned,
    WrongKind,
    Aliased,
}
#[derive(Clone, Copy)]
pub struct ResourceArgument {
    pub claim: Claim,
    pub parent: Option<usize>,
}
pub struct NativeArguments {
    pub(crate) values: wire::Values,
    pub(crate) resources: Vec<ResourceArgument>,
    /// A commit receipt, not another owner store. Source cells use these exact
    /// old claims to discard consumed tokens while retiring their containers.
    pub(crate) consumed: Vec<Claim>,
    pub(crate) cleanup: Option<c::Session>,
}
/// Unpublished native result. The original table retains custody until every
/// guest representation is ready. A failed staging operation must abort this
/// receipt and retire its original claims through the same cleanup Session.
pub struct NativeResult {
    pub(crate) values: wire::Values,
    pub(crate) nodes: Vec<usize>,
    pub(crate) claims: Vec<Claim>,
    pub(crate) originals: Vec<Claim>,
    pub(crate) holder: Option<Holder>,
    pub(crate) transfer: Option<Transfer>,
    pub(crate) committed: bool,
}
impl NativeResult {
    pub(crate) fn claim(&self, node: usize) -> Result<Claim, Reason> {
        let index = self
            .nodes
            .iter()
            .position(|value| *value == node)
            .ok_or(Reason::Entry)?;
        self.claims.get(index).copied().ok_or(Reason::Entry)
    }
}
pub(crate) enum ReadRoot<'l, 't> {
    Value(l::ReadView<'l, 't>),
    Resource(ResourceArgument),
}

/// The actual callback's borrowed capabilities. Raw native pointers never
/// construct this service, and the bridge expires it before returning to guest.
pub trait NativeContext {
    fn allocator_identity(&self) -> u64;
    fn buffer_create(
        &mut self,
        length: u64,
        alignment: u64,
    ) -> Result<*mut super::super::native::Buffer, u32>;
    fn buffer_drop(&mut self, buffer: *mut super::super::native::Buffer) -> Result<(), u32>;
    fn buffer_mut_bytes(
        &mut self,
        buffer: *mut super::super::native::Buffer,
    ) -> Result<super::super::native::MutBytes, u32>;
    fn resource_read(&self, index: u32) -> Result<super::super::native::Bytes, u32>;
    fn resource_write(&self, index: u32) -> Result<super::super::native::MutBytes, u32>;
    fn resource_publish(
        &mut self,
        key: &[u8],
        buffer: *mut super::super::native::Buffer,
    ) -> Result<u32, u32>;
}

pub(crate) struct NativeView<'a> {
    pub authority: NativeAuthority,
    pub kind: ResourceKind<'a>,
    pub access: super::super::idl::Access,
    pub bytes: super::super::native::MutBytes,
}
pub(crate) enum NativeAuthority {
    Loan(storage::Loan),
    Own(Claim, storage::Holder),
}
struct BufferHeader {
    allocation: Option<a::Handle>,
}
struct BufferSlot {
    header: Vec<BufferHeader>,
}
impl BufferSlot {
    fn pointer(&self) -> *mut super::super::native::Buffer {
        self.header.as_ptr().cast_mut().cast()
    }
}
pub(crate) struct CallContext<'r, 'm, 'b, 'a> {
    pub resources: &'r mut Resources<'a>,
    pub memory: &'m mut dyn Memory,
    pub budget: &'b mut m::Budget,
    pub views: &'r mut Vec<NativeView<'a>>,
    pub owners: &'r mut Vec<(Claim, usize)>,
    pub holder: Holder,
    pub provider: &'a super::super::native::Provider<'a>,
    buffers: Vec<BufferSlot>,
}
impl<'r, 'm, 'b, 'a> CallContext<'r, 'm, 'b, 'a> {
    pub fn new(
        resources: &'r mut Resources<'a>,
        memory: &'m mut dyn Memory,
        budget: &'b mut m::Budget,
        views: &'r mut Vec<NativeView<'a>>,
        owners: &'r mut Vec<(Claim, usize)>,
        holder: Holder,
        provider: &'a super::super::native::Provider<'a>,
    ) -> Self {
        Self {
            resources,
            memory,
            budget,
            views,
            owners,
            holder,
            provider,
            buffers: Vec::new(),
        }
    }
    fn slot(&self, buffer: *mut super::super::native::Buffer) -> Result<usize, u32> {
        if buffer.is_null() {
            return Err(2);
        }
        self.buffers
            .iter()
            .position(|slot| slot.pointer() == buffer && slot.header[0].allocation.is_some())
            .ok_or(5)
    }
    pub fn result(
        &mut self,
        buffer: *mut super::super::native::Buffer,
        session: &mut wire::Session,
    ) -> Result<Vec<u8>, Reason> {
        let index = self.slot(buffer).map_err(|_| Reason::Invariant)?;
        let allocation = self.buffers[index].header[0]
            .allocation
            .ok_or(Reason::Invariant)?;
        let bytes = self.memory.bytes(allocation).map_err(|e| fault(e.into()))?;
        session.value_bytes(bytes.len(), 0).map_err(Reason::from)?;
        session.value_work(bytes.len(), 0).map_err(Reason::from)?;
        let mut out = session.vector(bytes.len(), 0).map_err(Reason::from)?;
        out.extend_from_slice(bytes);
        Ok(out)
    }
    /// Unmetered, unlink-before-release draining, including a rejected result.
    pub fn close(&mut self) -> Result<(), Reason> {
        let mut failure = None;
        for slot in self.buffers.iter_mut().rev() {
            if let Some(allocation) = slot.header[0].allocation.take()
                && let Err(error) = self.memory.release(allocation)
            {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), |e| Err(fault(e.into())))
    }
    fn view(&self, index: u32) -> Result<&NativeView<'a>, u32> {
        let index = index.checked_sub(1).ok_or(7u32)? as usize;
        let view = self.views.get(index).ok_or(7u32)?;
        match view.authority {
            NativeAuthority::Loan(loan) => self.resources.check_view(loan, view.kind, view.access),
            NativeAuthority::Own(claim, holder) => self
                .resources
                .bytes(claim, holder, view.kind, self.memory)
                .map(|_| ()),
        }
        .map_err(|_| 7u32)?;
        Ok(view)
    }
}
impl NativeContext for CallContext<'_, '_, '_, '_> {
    fn resource_publish(
        &mut self,
        key: &[u8],
        buffer: *mut super::super::native::Buffer,
    ) -> Result<u32, u32> {
        self.budget.charge(self.buffers.len()).map_err(|_| 12u32)?;
        let slot = self.slot(buffer)?;
        let kind = self.provider.resource_kind_bytes(key, self.budget)?;
        let index = u32::try_from(self.views.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(12u32)?;
        if self.views.len() == self.views.capacity() || self.owners.len() == self.owners.capacity()
        {
            return Err(12);
        }
        let allocation = self.buffers[slot].header[0].allocation.ok_or(5u32)?;
        let bytes = self.memory.bytes_mut(allocation).map_err(|_| 17u32)?;
        let bytes = super::super::native::MutBytes {
            data: bytes.as_mut_ptr(),
            length: bytes.len() as u64,
        };
        let claim = self
            .resources
            .publish_backing(kind, self.holder, allocation, self.memory)
            .map_err(|_| 12u32)?;
        // The complete metadata capacity and index were checked before custody
        // moved; publication now performs no allocation or foreign callback.
        self.buffers[slot].header[0].allocation = None;
        self.owners.push((claim, self.views.len()));
        self.views.push(NativeView {
            authority: NativeAuthority::Own(claim, self.holder),
            kind,
            access: super::super::idl::Access::Exclusive,
            bytes,
        });
        Ok(index)
    }
    fn allocator_identity(&self) -> u64 {
        self.memory.identity().get()
    }
    fn buffer_create(
        &mut self,
        length: u64,
        alignment: u64,
    ) -> Result<*mut super::super::native::Buffer, u32> {
        let request = a::Request::new(length, alignment).map_err(|_| 2u32)?;
        if self.buffers.len() == self.buffers.capacity() {
            let capacity = self
                .buffers
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(2))
                .ok_or(12u32)?;
            let mut slots = self.budget.vector(capacity).map_err(|_| 12u32)?;
            slots.append(&mut self.buffers);
            self.buffers = slots;
        }
        let mut header = self.budget.vector(1).map_err(|_| 12u32)?;
        let allocation = self.memory.allocate(request).map_err(|_| 12u32)?;
        header.push(BufferHeader {
            allocation: Some(allocation),
        });
        let slot = BufferSlot { header };
        let pointer = slot.pointer();
        self.buffers.push(slot);
        Ok(pointer)
    }
    fn buffer_drop(&mut self, buffer: *mut super::super::native::Buffer) -> Result<(), u32> {
        let index = self.slot(buffer)?;
        let allocation = self.buffers[index].header[0]
            .allocation
            .take()
            .ok_or(5u32)?;
        self.memory.release(allocation).map_err(|_| 17u32)
    }
    fn buffer_mut_bytes(
        &mut self,
        buffer: *mut super::super::native::Buffer,
    ) -> Result<super::super::native::MutBytes, u32> {
        self.budget.charge(self.buffers.len()).map_err(|_| 12u32)?;
        let index = self.slot(buffer)?;
        let allocation = self.buffers[index].header[0].allocation.ok_or(5u32)?;
        let bytes = self.memory.bytes_mut(allocation).map_err(|_| 17u32)?;
        Ok(super::super::native::MutBytes {
            data: if bytes.is_empty() {
                ptr::null_mut()
            } else {
                bytes.as_mut_ptr()
            },
            length: bytes.len() as u64,
        })
    }
    fn resource_read(&self, index: u32) -> Result<super::super::native::Bytes, u32> {
        let view = self.view(index)?;
        Ok(super::super::native::Bytes {
            data: view.bytes.data.cast_const(),
            length: view.bytes.length,
        })
    }
    fn resource_write(&self, index: u32) -> Result<super::super::native::MutBytes, u32> {
        let view = self.view(index)?;
        if view.access != super::super::idl::Access::Exclusive {
            return Err(7);
        }
        Ok(view.bytes)
    }
}
