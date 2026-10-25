//! Persistent provider ownership over the checked live native resource scope.
use super::*;

/// # Safety
/// Context and initialized output/error objects obey the fixed C contract.
pub unsafe extern "C" fn context_resource_keep(
    context: *const ffi::Context,
    index: u32,
    output: *mut u32,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: forward the exact synchronous native objects unchanged.
    unsafe { transfer(context, index, output, error, false) }
}
/// # Safety
/// Context and initialized output/error objects obey the fixed C contract.
pub unsafe extern "C" fn context_resource_take(
    context: *const ffi::Context,
    index: u32,
    output: *mut u32,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: forward the exact synchronous native objects unchanged.
    unsafe { transfer(context, index, output, error, true) }
}
unsafe fn transfer(
    context: *const ffi::Context,
    index: u32,
    output: *mut u32,
    error: *mut ffi::Error,
    take: bool,
) -> u32 {
    // SAFETY: the native caller retains readable/writable fixed objects.
    if context.is_null() || output.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: validate thread, token, phase and control before touching Cells.
    let status = unsafe { component::context_check(context) };
    if status != 0 {
        return status;
    }
    // SAFETY: checked immutable routing identifies the original live engine.
    let owner = match unsafe { metadata((*context).engine) } {
        Ok(owner) => unsafe { owner.as_ref() },
        Err(status) => return status,
    };
    if !owner.active.get() {
        return BUSY;
    }
    let Some(current) = owner.current.borrow().clone() else {
        return BUSY;
    };
    // SAFETY: only the original live resource capability is lent. No Invoker,
    // new context or guest reentry permission is created by this operation.
    let result = unsafe {
        component::with_resource_context(context, |call| {
            let leases = current.instance.leases.try_borrow().map_err(|_| BUSY)?;
            let registration = leases
                .iter()
                .find(|lease| {
                    lease
                        .0
                        .provider
                        .as_ref()
                        .is_some_and(|provider| call.matches_provider(provider))
                })
                .ok_or(BUSY)?;
            let mut persistent = registration
                .0
                .persistent
                .try_borrow_mut()
                .map_err(|_| BUSY)?;
            let persistent = persistent.as_mut().ok_or(BUSY)?;
            if take {
                call.resource_take(index, persistent)
            } else {
                call.resource_keep(index, persistent)
            }
        })
    };
    match result {
        Ok(index) => {
            // SAFETY: preflight and atomic transfer completed; publish last.
            unsafe { output.write(index) };
            OK
        }
        Err(status @ 0..=6) => status,
        Err(12 | 17) => OOM,
        Err(_) => INVALID,
    }
}
