use super::{Destroy, Invoke};
use std::ffi::c_void;

/// A trusted callback. Safe code cannot replace its callback or user pointer.
///
/// ```compile_fail,E0133
/// use lang_runtime::component::native::{Method, Context, Bytes, Buffer};
/// extern "C" fn callback(_: *mut std::ffi::c_void, _: *const Context, _: Bytes, _: *mut *mut Buffer) -> u32 { 2 }
/// let method = Method::new(0, 0, callback, std::ptr::null_mut(), 0);
/// ```
/// ```compile_fail,E0616
/// use lang_runtime::component::native::Method;
/// fn replace(method: &mut Method) { method.user = std::ptr::null_mut(); }
/// ```
/// ```compile_fail,E0616
/// use lang_runtime::component::native::Method;
/// fn enable_reentry(method: &mut Method) { method.flags = 1; }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Method {
    pub interface: usize,
    pub member: usize,
    invoke: Invoke,
    user: *mut c_void,
    /// Bit 0 permits nested synchronous guest calls; all other bits are zero.
    flags: u32,
}
impl Method {
    /// # Safety
    /// Code and user storage must remain valid for every use of every copy.
    /// The callback must not unwind and must safely handle any input byte view
    /// with the live owner-thread context/output slot provided by the adapter.
    /// It may only borrow these views until return and must follow component-v1
    /// buffer ownership. Metadata validation and CodeOwner do not prove this.
    /// If bit 0 of flags permits reentry, the callback must remain sound across
    /// nested calls, including aliases of its user storage.
    pub const unsafe fn new(
        interface: usize,
        member: usize,
        invoke: Invoke,
        user: *mut c_void,
        flags: u32,
    ) -> Self {
        Self {
            interface,
            member,
            invoke,
            user,
            flags,
        }
    }
    pub const fn invoke_fn(&self) -> Invoke {
        self.invoke
    }
    pub const fn user_data(&self) -> *mut c_void {
        self.user
    }
    pub const fn flags(&self) -> u32 {
        self.flags
    }
}

/// A trusted destructor; its code and user pointer cannot be replaced safely.
#[derive(Clone, Copy, Debug)]
pub struct ResourceType {
    pub interface: usize,
    pub resource: usize,
    destroy: Destroy,
    user: *mut c_void,
}
impl ResourceType {
    /// # Safety
    /// Code/user storage must survive all descriptor copies and resource leases.
    /// The destructor must not unwind and must safely handle the adapter's live
    /// context, unique backing byte view and output slot, without retaining views
    /// after return. It follows component-v1 buffer ownership even on failure.
    pub const unsafe fn new(
        interface: usize,
        resource: usize,
        destroy: Destroy,
        user: *mut c_void,
    ) -> Self {
        Self {
            interface,
            resource,
            destroy,
            user,
        }
    }
    pub const fn destroy_fn(&self) -> Destroy {
        self.destroy
    }
    pub const fn user_data(&self) -> *mut c_void {
        self.user
    }
}
