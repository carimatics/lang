//! One immutable complete table. Rejected queries never change success output.
use super::*;

static API: ffi::Api = ffi::Api {
    header: ffi::Header {
        size: size_of::<ffi::Api>() as u32,
        version: 1,
    },
    engine_create,
    engine_destroy,
    module_load,
    module_drop,
    module_bind_native,
    provider_register,
    provider_drop,
    instance_create,
    instance_close,
    bind_export,
    binding_drop,
    control_create,
    interrupt,
    control_drop,
    call,
    context_call,
    context_check,
    context_buffer_create,
    context_buffer_drop,
    buffer_mut_bytes,
    result_info,
    result_usage,
    result_bytes,
    result_drop,
    resource_drop,
    resource_publish,
    context_resource_publish,
    context_resource_drop,
    context_resource_read,
    context_resource_write,
    context_resource_keep,
    context_resource_take,
    reserved: [0; 2],
};

/// # Safety
/// Non-null output/error slots are valid, writable native objects. Error must
/// have the exact initialized size/version header and remain valid throughout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lang_component_v1_get_api(
    version: u32,
    api_size: u32,
    output: *mut *const ffi::Api,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: the caller retains initialized fixed native storage.
    if output.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    let diagnostic = if version != 1 {
        wire::Diagnostic::UnsupportedVersion
    } else if api_size != size_of::<ffi::Api>() as u32 {
        wire::Diagnostic::InvalidRequest
    } else {
        // SAFETY: only a complete, compatible immutable table is published.
        unsafe { output.write(&API) };
        return OK;
    };
    let failure = admission::Failure::new(diagnostic);
    // SAFETY: the initialized error slot was validated before any mutation.
    unsafe { error.write(failure.error) };
    if version != 1 { UNSUPPORTED } else { INVALID }
}
