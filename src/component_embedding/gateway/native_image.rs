//! Passive compatibility admission precedes all exact-SDK private pointer reads.
use super::*;
use crate::native::component::{Image, ImageError};

pub(super) struct Lease {
    pub image: Image,
    owner: *mut std::ffi::c_void,
    release: ffi::ReleaseFn,
}
impl Drop for Lease {
    fn drop(&mut self) {
        // SAFETY: successful admission consumed this owner exactly once. The
        // Image still retains its original executable owner through this return.
        unsafe { (self.release)(self.owner) };
    }
}

pub(super) fn refused(reason: ImageError, error: *mut ffi::Error) -> u32 {
    let failure = match reason {
        ImageError::Build | ImageError::Target | ImageError::Profile => {
            let mut failure = admission::Failure::new(wire::Diagnostic::UnsupportedVersion);
            failure.status = UNSUPPORTED;
            failure
        }
        ImageError::Preparation(error) => {
            admission::module(&super::super::Error::Budget(error), None)
        }
        ImageError::Module { error, prior: _ } => admission::module(&error, None),
        ImageError::Record(lang_codegen_clif::native_record::Error {
            kind: lang_codegen_clif::native_record::ErrorKind::Resource(error),
            ..
        }) => admission::module(&super::super::Error::Budget(error), None),
        ImageError::Plan(error) => match *error {
            lang_codegen_clif::tasks::Error::Resource(error)
            | lang_codegen_clif::tasks::Error::Layout(lang_runtime::layout::Error::Resource {
                error,
                ..
            }) => admission::module(&super::super::Error::Budget(error), None),
            _ => admission::Failure::new(wire::Diagnostic::HostContract),
        },
        _ => admission::Failure::new(wire::Diagnostic::HostContract),
    };
    // SAFETY: module_bind_native checked this initialized writable error slot.
    unsafe { error.write(failure.error) };
    failure.status
}

pub(super) fn program_refused(reason: program::Error, error: *mut ffi::Error) -> u32 {
    refused(
        match reason {
            program::Error::Resource(reason) => ImageError::Preparation(reason),
            _ => ImageError::Correspondence,
        },
        error,
    )
}

pub(super) fn callback_refused(reason: ImageError) -> execution::CallbackFailure {
    match reason {
        ImageError::Preparation(error)
        | ImageError::Record(lang_codegen_clif::native_record::Error {
            kind: lang_codegen_clif::native_record::ErrorKind::Resource(error),
            ..
        }) => error.into(),
        ImageError::Plan(error) => match *error {
            lang_codegen_clif::tasks::Error::Resource(error)
            | lang_codegen_clif::tasks::Error::Layout(lang_runtime::layout::Error::Resource {
                error,
                ..
            }) => error.into(),
            _ => INVALID.into(),
        },
        _ => INVALID.into(),
    }
}

/// # Safety
/// All descriptors/views and output/error slots are valid native objects. After
/// compatible metadata admission, program points to this exact SDK's live Image.
/// Owner/release storage remains valid through its final non-unwinding return;
/// successful registration consumes ownership, while refusal retains it.
pub unsafe extern "C" fn module_bind_native(
    engine: *mut ffi::Engine,
    image: *const ffi::NativeImage,
    output: *mut *mut ffi::Module,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: caller supplies complete initialized native objects.
    if output.is_null() || !unsafe { error_ready(error) } || !unsafe { header(image) } {
        return INVALID;
    }
    // SAFETY: engine is a live handle and retains its original owner metadata.
    let engine = match unsafe { metadata(engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let owner = unsafe { engine.as_ref() };
    if owner.active.get() {
        return BUSY;
    }
    let data = unsafe { owner.data.as_ref() };
    if data.config.mode != 2 {
        return UNSUPPORTED;
    }
    let descriptor = unsafe { image.read() };
    let Some(release) = descriptor.release_owner else {
        return INVALID;
    };
    if descriptor.reserved != [0; 2] || descriptor.program.is_null() {
        return INVALID;
    }
    let Some(count) = owner.handles.get().checked_add(1) else {
        return OOM;
    };
    let metadata = match unsafe { bytes(descriptor.metadata) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let mut budget = m::Budget::new(data.limits.preparation);
    if let Err(reason) = crate::native::component::compatible(metadata, data.limits, &mut budget) {
        return refused(reason, error);
    }
    // SAFETY: passive exact host/SDK admission finished first. The trusted
    // native contract now requires this exact live SDK-owned private object.
    let image = unsafe { &*descriptor.program.cast::<Image>() };
    if let Err(reason) = budget.charge(metadata.len()) {
        return refused(reason.into(), error);
    }
    if image.metadata() != metadata {
        return INVALID;
    }
    if let Err(reason) = image.validate(data.limits, &mut budget) {
        return refused(reason, error);
    }
    if let Err(reason) = budget
        .allocate::<Loaded>(1)
        .and_then(|_| budget.allocate::<Lease>(1))
        .and_then(|_| budget.allocate::<usize>(2))
    {
        return refused(reason.into(), error);
    }
    if let Err(reason) = budget.charge(image.0.semantic().len()) {
        return refused(reason.into(), error);
    }
    let digest = Sha256::digest(image.0.semantic()).into();
    let loaded = Box::new(Loaded {
        engine,
        module: image.0.module.clone(),
        digest,
        native: Some(Rc::new(Lease {
            image: image.clone(),
            owner: descriptor.owner,
            release,
        })),
    });
    owner.handles.set(count);
    // SAFETY: all admission finished; the supplied owner transfers only here.
    unsafe { output.write(Box::into_raw(loaded).cast()) };
    OK
}
