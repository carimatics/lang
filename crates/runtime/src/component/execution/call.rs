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
    Status { code: u32, bytes: Vec<u8> },
    MissingResult,
    InvalidResult(wire::Error),
}
#[derive(Clone, Copy)]
pub struct ResourceArgument {
    pub claim: Claim,
    pub parent: Option<usize>,
}
pub struct NativeArguments {
    pub(crate) values: wire::Values,
    pub(crate) resources: Vec<ResourceArgument>,
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
}

pub(crate) struct NativeView<'a> {
    pub loan: storage::Loan,
    pub kind: ResourceKind<'a>,
    pub access: super::super::idl::Access,
    pub bytes: super::super::native::MutBytes,
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
    pub resources: &'r Resources<'a>,
    pub memory: &'m mut dyn Memory,
    pub budget: &'b mut m::Budget,
    pub views: &'r [NativeView<'a>],
    buffers: Vec<BufferSlot>,
}
impl<'r, 'm, 'b, 'a> CallContext<'r, 'm, 'b, 'a> {
    pub fn new(
        resources: &'r Resources<'a>,
        memory: &'m mut dyn Memory,
        budget: &'b mut m::Budget,
        views: &'r [NativeView<'a>],
    ) -> Self {
        Self {
            resources,
            memory,
            budget,
            views,
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
        session
            .value_bytes(bytes.len(), 0)
            .map_err(Reason::ComponentFrame)?;
        session
            .charge(bytes.len(), 0)
            .map_err(Reason::ComponentFrame)?;
        let mut out = session
            .vector(bytes.len(), 0)
            .map_err(Reason::ComponentFrame)?;
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
        self.resources
            .check_view(view.loan, view.kind, view.access)
            .map_err(|_| 7u32)?;
        Ok(view)
    }
}
impl NativeContext for CallContext<'_, '_, '_, '_> {
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
