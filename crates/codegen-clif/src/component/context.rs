//! Private subset of the fixed callback API. No borrowed pointer is restored
//! until the live engine/thread/header/invocation/scope has been checked.
use super::*;

unsafe fn with_call<T>(
    context: *const native::Context,
    cleanup: bool,
    operation: impl FnOnce(&mut dyn NativeContext) -> Result<T, u32>,
) -> Result<T, u32> {
    if context.is_null() {
        return Err(2);
    }
    // SAFETY: native callers supply readable initialized context bytes and an
    // engine created here that remains live; copied expired tokens are allowed.
    let context = unsafe { &*context };
    if context.engine.is_null() {
        return Err(2);
    }
    let state = unsafe { &mut *context.engine.cast::<State>() };
    let status = check(state, context);
    if status != 0 {
        return Err(status);
    }
    // SAFETY: the active scope retains the original control until callback end.
    if !cleanup && unsafe { &*state.control }.check().is_err() {
        return Err(1);
    }
    let call = state.call.ok_or(4u32)?;
    // SAFETY: owner-thread scope validation succeeded. The active guard holds
    // this unique original capability borrow, and operations cannot reenter.
    operation(unsafe { &mut *call })
}

/// # Safety
/// `context` satisfies `context_check`; non-null output is writable and aligned.
pub unsafe extern "C" fn context_buffer_create(
    context: *const native::Context,
    length: u64,
    alignment: u64,
    output: *mut *mut native::Buffer,
) -> u32 {
    if output.is_null() {
        return 2;
    }
    // SAFETY: caller guarantees readable context and writable output storage.
    match unsafe { with_call(context, false, |call| call.buffer_create(length, alignment)) } {
        Ok(buffer) => {
            unsafe { output.write(buffer) };
            0
        }
        Err(status) => status,
    }
}
/// # Safety
/// `context` satisfies `context_check`. The opaque buffer is compared before use.
pub unsafe extern "C" fn context_buffer_drop(
    context: *const native::Context,
    buffer: *mut native::Buffer,
) -> u32 {
    // SAFETY: only live original capabilities are restored; cleanup is unmetered.
    match unsafe { with_call(context, true, |call| call.buffer_drop(buffer)) } {
        Ok(()) => 0,
        Err(status) => status,
    }
}
/// # Safety
/// Context/output storage is valid; returned views expire at drop/callback end.
pub unsafe extern "C" fn buffer_mut_bytes(
    context: *const native::Context,
    buffer: *mut native::Buffer,
    output: *mut native::MutBytes,
) -> u32 {
    if output.is_null() {
        return 2;
    }
    // SAFETY: caller guarantees context/output storage validity and unique use.
    match unsafe { with_call(context, false, |call| call.buffer_mut_bytes(buffer)) } {
        Ok(bytes) => {
            unsafe { output.write(bytes) };
            0
        }
        Err(status) => status,
    }
}
/// # Safety
/// Context/output storage is valid. The borrowed view expires at callback end.
pub unsafe extern "C" fn context_resource_read(
    context: *const native::Context,
    index: u32,
    output: *mut native::Bytes,
) -> u32 {
    if output.is_null() {
        return 2;
    }
    // SAFETY: actual table/implementation/scope authority precedes publication.
    match unsafe { with_call(context, false, |call| call.resource_read(index)) } {
        Ok(bytes) => {
            unsafe { output.write(bytes) };
            0
        }
        Err(status) => status,
    }
}
/// # Safety
/// Context/output storage is valid; native callers preserve exclusive view use.
pub unsafe extern "C" fn context_resource_write(
    context: *const native::Context,
    index: u32,
    output: *mut native::MutBytes,
) -> u32 {
    if output.is_null() {
        return 2;
    }
    // SAFETY: actual exclusive authority precedes publication; no raw index grants it.
    match unsafe { with_call(context, false, |call| call.resource_write(index)) } {
        Ok(bytes) => {
            unsafe { output.write(bytes) };
            0
        }
        Err(status) => status,
    }
}

/// # Safety
/// Context and key storage remain readable for this synchronous operation;
/// output is writable and aligned. Only registered keys and live context
/// buffers can publish an owner; refusal leaves buffer/output unchanged.
pub unsafe extern "C" fn context_resource_publish(
    context: *const native::Context,
    key: native::Bytes,
    buffer: *mut native::Buffer,
    output: *mut u32,
) -> u32 {
    if output.is_null() || key.data.is_null() || key.length > isize::MAX as u64 {
        return 2;
    }
    // SAFETY: the native caller guarantees this readable key region. Context
    // validation and cumulative work admission precede key inspection/use.
    match unsafe {
        with_call(context, false, |call| {
            let key = std::slice::from_raw_parts(key.data, key.length as usize);
            call.resource_publish(key, buffer)
        })
    } {
        Ok(index) => {
            unsafe { output.write(index) };
            0
        }
        Err(status) => status,
    }
}
