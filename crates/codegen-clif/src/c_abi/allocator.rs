//! Original-allocator storage for C contexts. Every positive returned byte is
//! initialized before a Rust slice can expose it. Accounting, zero requests,
//! exact sizes, OOM rollback and handle provenance stay in the shared allocator.
use lang_runtime::allocation as a;
use lang_runtime::c_abi::context::{AllocatorIdentity, Context, config::Allocator};
use std::{marker::PhantomData, ptr::NonNull};

pub struct CAllocator<'a> {
    identity: AllocatorIdentity,
    inner: a::Allocator<Provider<'a>>,
}
#[derive(Debug)]
pub enum Error {
    Allocation(a::Error),
    Context(lang_runtime::c_abi::context::Fault),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "C allocator binding failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl<'a> CAllocator<'a> {
    /// # Safety
    /// The validated context's allocator/user must remain live for this borrow.
    /// Callbacks obey RFC 0008: unique writable actual storage for each positive
    /// request, matching exact deallocation, no unwind/reentry, and no retained
    /// alias to a published region. A null allocation is recoverable OOM.
    pub unsafe fn new(context: &'a Context<'a>) -> Result<Self, Error> {
        // SAFETY: the public borrow additionally prevents context destruction.
        unsafe { Self::owned(context) }
    }
    /// # Safety
    /// Used only by the opaque owning context: this allocator and all its
    /// regions must retire before that Context and its foreign code/user data.
    /// The copied provider identity never borrows the movable Context address.
    pub(crate) unsafe fn owned(context: &Context<'a>) -> Result<Self, Error> {
        let provider = Provider {
            context: context.allocator_identity(),
            descriptor: context.allocator(),
            lifetime: PhantomData,
        };
        let inner =
            a::Allocator::new(provider, context.limits().allocation).map_err(Error::Allocation)?;
        context
            .bind_allocator(inner.identity())
            .map_err(Error::Context)?;
        Ok(Self {
            identity: context.allocator_identity(),
            inner,
        })
    }
    pub fn belongs_to(&self, context: &Context<'_>) -> bool {
        self.identity == context.allocator_identity()
    }
    pub fn managed(&self) -> &a::Allocator<Provider<'a>> {
        &self.inner
    }
    pub fn managed_mut(&mut self) -> &mut a::Allocator<Provider<'a>> {
        &mut self.inner
    }
}

pub struct Provider<'a> {
    context: AllocatorIdentity,
    descriptor: Allocator,
    lifetime: PhantomData<&'a Context<'a>>,
}
impl lang_runtime::c_abi::context::scalar::ContextProvider for Provider<'_> {
    fn context_identity(&self) -> AllocatorIdentity {
        self.context
    }
}
pub struct Region<'a> {
    _context: PhantomData<&'a Context<'a>>,
    descriptor: Allocator,
    request: a::Request,
    pointer: NonNull<u8>,
}
impl a::Region for Region<'_> {
    fn bytes(&self) -> &[u8] {
        // SAFETY: construction initializes every byte of unique live storage;
        // the context borrow retains its provider's lifetime. Zero uses a
        // non-null dangling byte pointer and a zero length.
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.request.bytes() as usize) }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: the region is uniquely owned and the exclusive borrow forbids
        // another Rust view. Foreign callback alias retention is prohibited.
        unsafe {
            std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.request.bytes() as usize)
        }
    }
}
impl<'a> a::Provider for Provider<'a> {
    type Region = Region<'a>;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        let pointer = if request.bytes() == 0 {
            NonNull::dangling()
        } else {
            let allocate = self.descriptor.allocate.expect("checked context allocator");
            // SAFETY: CAllocator::new requires the actual storage/lifetime and
            // no-unwind/reentry contract. Request already proves native bounds.
            let pointer = NonNull::new(
                unsafe { allocate(self.descriptor.user, request.bytes(), request.alignment()) }
                    .cast::<u8>(),
            )?;
            // SAFETY: the trusted allocator returned this complete writable
            // range. Initialize it before exposing any bytes as Rust u8.
            unsafe { pointer.as_ptr().write_bytes(0, request.bytes() as usize) };
            pointer
        };
        Some(Region {
            _context: PhantomData,
            descriptor: self.descriptor,
            request,
            pointer,
        })
    }
    fn deallocate(&mut self, region: Self::Region, _: a::Request) {
        if region.request.bytes() == 0 {
            return;
        }
        let deallocate = region
            .descriptor
            .deallocate
            .expect("checked original allocator");
        // SAFETY: the region retains its ORIGINAL callback/user/request, even
        // if a trusted Rust provider caller supplied a different provider or
        // Request. The unique owned region is consumed exactly once here.
        unsafe {
            deallocate(
                region.descriptor.user,
                region.pointer.as_ptr().cast(),
                region.request.bytes(),
                region.request.alignment(),
            )
        };
    }
}
