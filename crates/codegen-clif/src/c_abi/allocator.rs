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
            // SAFETY: owned retains the original descriptor's foreign code,
            // user and actual storage until every managed region is retired.
            storage: unsafe { ForeignProvider::new(context.allocator()) }
                .expect("checked context allocator"),
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
    storage: ForeignProvider<'a>,
}
/// Original foreign storage without a scalar-context identity. The component
/// gateway can use the same allocator and Region implementation directly.
pub struct ForeignProvider<'a> {
    descriptor: Allocator,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> ForeignProvider<'a> {
    /// # Safety
    /// The copied allocate/deallocate code and user storage must remain live
    /// through every returned region's final deallocation. Positive allocation
    /// returns unique, aligned writable storage of the exact request size, or
    /// null for OOM. Neither callback unwinds or reenters; no aliases are kept.
    /// Descriptor pointer validity must be established before copying fields.
    pub unsafe fn new(descriptor: Allocator) -> Option<Self> {
        if descriptor.version != 1
            || descriptor.size != std::mem::size_of::<Allocator>() as u32
            || descriptor.allocate.is_none()
            || descriptor.deallocate.is_none()
        {
            return None;
        }
        Some(Self {
            descriptor,
            lifetime: PhantomData,
        })
    }
}
impl lang_runtime::c_abi::context::scalar::ContextProvider for Provider<'_> {
    fn context_identity(&self) -> AllocatorIdentity {
        self.context
    }
}
pub struct Region<'a> {
    _provider: PhantomData<&'a ()>,
    descriptor: Allocator,
    request: a::Request,
    pointer: NonNull<u8>,
}
impl a::Region for Region<'_> {
    fn bytes(&self) -> &[u8] {
        // SAFETY: construction initializes every byte of unique live storage;
        // the provider's constructor retains its foreign lifetime. Zero uses a
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
        a::Provider::allocate(&mut self.storage, request)
    }
    fn deallocate(&mut self, region: Self::Region, request: a::Request) {
        a::Provider::deallocate(&mut self.storage, region, request);
    }
}
impl<'a> a::Provider for ForeignProvider<'a> {
    type Region = Region<'a>;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        let pointer = if request.bytes() == 0 {
            NonNull::dangling()
        } else {
            let allocate = self.descriptor.allocate.expect("checked context allocator");
            // SAFETY: ForeignProvider::new requires actual storage/lifetime and
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
            _provider: PhantomData,
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
