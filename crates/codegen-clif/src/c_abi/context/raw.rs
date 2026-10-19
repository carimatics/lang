use c::config::{self, Allocator, Binding, Descriptor, InvalidConfig, LimitsDescriptor};
use lang_runtime::c_abi::{Digest, context as c, validation as v};
use std::{
    ffi::c_void,
    mem::{align_of, size_of},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

/// Exact profile-1 C descriptor. Descriptor/name storage is copied; callback
/// code and user data retain their original lifetime obligations.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProgramConfig {
    pub version: u32,
    pub size: u32,
    pub allocator: *const Allocator,
    pub limits: *const LimitsDescriptor,
    pub imports: *const ImportBinding,
    pub import_count: u64,
    pub artifact: [u8; 32],
    pub flags: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ImportBinding {
    pub version: u32,
    pub size: u32,
    pub name: *const u8,
    pub name_bytes: u64,
    pub signature: [u8; 32],
    pub function: Option<config::ErasedFunction>,
    pub user: *mut c_void,
    pub flags: u64,
}
struct RawContext<'a> {
    // Drop accounting/regions before the gate and its borrowed registration.
    allocator: std::cell::RefCell<super::super::allocator::CAllocator<'a>>,
    program: &'a c::Program<'a>,
    context: c::Context<'a>,
}
#[derive(Clone, Copy)]
struct Range {
    start: usize,
    end: usize,
}
fn range(
    pointer: *const u8,
    size: usize,
    align: usize,
    object: v::Object,
) -> Result<Range, c::Fault> {
    let start = pointer.addr();
    let kind = if start == 0 {
        Some(v::Storage::Null)
    } else if !start.is_multiple_of(align) {
        Some(v::Storage::Misaligned { align })
    } else if size > isize::MAX as usize || start.checked_add(size).is_none() {
        Some(v::Storage::Overflow)
    } else {
        None
    };
    if let Some(kind) = kind {
        return Err(c::Fault::new(c::Kind::Value(v::Error::Storage {
            object,
            kind,
        })));
    }
    Ok(Range {
        start,
        end: start + size,
    })
}
fn object<T>(pointer: *const T, which: v::Object) -> Result<Range, c::Fault> {
    range(pointer.cast(), size_of::<T>(), align_of::<T>(), which)
}
fn disjoint(output: Range, input: Range, which: v::Object) -> Result<(), c::Fault> {
    if output.start < input.end && input.start < output.end {
        return Err(c::Fault::new(c::Kind::Value(v::Error::OutputOverlap(
            which,
        ))));
    }
    Ok(())
}
#[repr(C)]
struct Header {
    version: u32,
    size: u32,
}
unsafe fn header<T>(pointer: *const T, size: u32, which: Descriptor) -> Result<(), c::Fault> {
    if pointer.is_null() {
        return Err(InvalidConfig::Missing(which).into());
    }
    object(pointer, v::Object::Argument(0))?;
    // SAFETY: the caller supplies a live initialized version/size prefix. Only
    // these fields are read until the declared complete object size is admitted.
    let h = unsafe { ptr::read(pointer.cast::<Header>()) };
    if h.version != config::VERSION || h.size != size {
        return Err(InvalidConfig::Header(which).into());
    }
    Ok(())
}
macro_rules! fields {
    ($pointer:expr, $ty:ident { $($field:ident),+ $(,)? }) => {
        $ty { $($field: ptr::addr_of!((*$pointer).$field).read()),+ }
    };
}
struct Views<'a> {
    config: ProgramConfig,
    allocator: Allocator,
    limits: LimitsDescriptor,
    imports: Vec<Binding<'a>>,
}
impl Views<'_> {
    fn config(&self) -> config::Config<'_> {
        config::Config {
            version: self.config.version,
            size: self.config.size,
            allocator: Some(&self.allocator),
            limits: Some(&self.limits),
            imports: Some(&self.imports),
            import_count: self.config.import_count,
            artifact: Digest(self.config.artifact),
            flags: self.config.flags,
        }
    }
}
unsafe fn views<'a>(
    program: &c::Program<'_>,
    pointer: *const ProgramConfig,
    output: Range,
) -> Result<Views<'a>, c::Fault> {
    // SAFETY: forwarded C descriptor lifetime/access contract; each complete
    // object is read field-wise only after its numeric range/header admission.
    unsafe {
        header(pointer, 80, Descriptor::Config)?;
    }
    disjoint(
        output,
        object(pointer, v::Object::Argument(0))?,
        v::Object::Argument(0),
    )?;
    // SAFETY: header and actual-object caller contract admit these fields;
    // no C padding or pointed-to object is read by this copy.
    let config = unsafe {
        fields!(
            pointer,
            ProgramConfig {
                version,
                size,
                allocator,
                limits,
                imports,
                import_count,
                artifact,
                flags
            }
        )
    };
    if config.flags != 0 {
        return Err(InvalidConfig::Flags(Descriptor::Config).into());
    }
    if config.artifact != program.artifact().digest().0 {
        return Err(InvalidConfig::Artifact.into());
    }
    let count = usize::try_from(config.import_count)
        .ok()
        .filter(|n| *n <= config::MAX_IMPORTS)
        .ok_or(InvalidConfig::ImportCount)?;
    // SAFETY: each descriptor prefix/complete extent follows the same caller contract.
    unsafe {
        header(config.allocator, 32, Descriptor::Allocator)?;
        header(config.limits, 72, Descriptor::Limits)?;
    }
    disjoint(
        output,
        object(config.allocator, v::Object::Argument(1))?,
        v::Object::Argument(1),
    )?;
    disjoint(
        output,
        object(config.limits, v::Object::Argument(2))?,
        v::Object::Argument(2),
    )?;
    // SAFETY: full declared fields are initialized and admitted; padding is not read.
    let allocator = unsafe {
        fields!(
            config.allocator,
            Allocator {
                version,
                size,
                user,
                allocate,
                deallocate
            }
        )
    };
    // SAFETY: same complete initialized-field contract for the limits object.
    let limits = unsafe {
        fields!(
            config.limits,
            LimitsDescriptor {
                version,
                size,
                live_bytes,
                cumulative_bytes,
                live_allocations,
                frames,
                cells,
                host_calls,
                fuel,
                cleanup_records,
                reserved
            }
        )
    };
    if allocator.allocate.is_none() || allocator.deallocate.is_none() {
        return Err(InvalidConfig::Missing(Descriptor::Allocator).into());
    }
    if limits.reserved != 0 {
        return Err(InvalidConfig::Flags(Descriptor::Limits).into());
    }
    if count != 0 {
        let bytes = count
            .checked_mul(size_of::<ImportBinding>())
            .ok_or(InvalidConfig::ImportCount)?;
        let input = range(
            config.imports.cast(),
            bytes,
            align_of::<ImportBinding>(),
            v::Object::Argument(3),
        )?;
        disjoint(output, input, v::Object::Argument(3))?;
    }
    let allocation = || c::Fault::new(c::Kind::Allocation);
    let mut metadata = Vec::new();
    metadata
        .try_reserve_exact(count)
        .map_err(|_| allocation())?;
    let mut name_bytes = 0usize;
    // Validate ALL descriptor/name addresses and lengths before reading names.
    for index in 0..count {
        // SAFETY: count*stride and full array extent were admitted above.
        let pointer = unsafe { config.imports.add(index) };
        // SAFETY: initialized header precedes field reads of this actual C object.
        unsafe {
            header(pointer, 80, Descriptor::Binding(index))?;
        }
        // SAFETY: admitted complete actual C object; copying fields never reads padding.
        let binding = unsafe {
            fields!(
                pointer,
                ImportBinding {
                    version,
                    size,
                    name,
                    name_bytes,
                    signature,
                    function,
                    user,
                    flags
                }
            )
        };
        if binding.flags & !config::NATIVE_FFI != 0 {
            return Err(InvalidConfig::Flags(Descriptor::Binding(index)).into());
        }
        if binding.function.is_none() {
            return Err(InvalidConfig::Function(index).into());
        }
        let bytes = usize::try_from(binding.name_bytes)
            .ok()
            .filter(|n| *n != 0)
            .ok_or(InvalidConfig::Name(index))?;
        name_bytes = name_bytes
            .checked_add(bytes)
            .filter(|n| *n <= config::MAX_NAME_BYTES)
            .ok_or(InvalidConfig::NameBytes)?;
        let input = range(binding.name, bytes, 1, v::Object::Argument(index + 4))?;
        disjoint(output, input, v::Object::Argument(index + 4))?;
        metadata.push((binding, bytes));
    }
    let mut imports = Vec::new();
    imports.try_reserve_exact(count).map_err(|_| allocation())?;
    for (binding, bytes) in metadata {
        // SAFETY: every name range passed the complete address phase; the
        // caller keeps the initialized byte objects live until create returns.
        let name = unsafe { std::slice::from_raw_parts(binding.name, bytes) };
        imports.push(Binding {
            version: binding.version,
            size: binding.size,
            name: Some(name),
            name_bytes: binding.name_bytes,
            signature: Digest(binding.signature),
            function: binding.function,
            user: binding.user,
            flags: binding.flags,
        });
    }
    Ok(Views {
        config,
        allocator,
        limits,
        imports,
    })
}
fn contain(operation: impl FnOnce() -> Result<(), c::Fault>) -> u32 {
    let result = match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => result,
        Err(payload) => Err(c::Fault::host_panic(payload)),
    };
    result.map_or_else(
        |fault| fault.status().code(),
        |()| c::Status::Success.code(),
    )
}
unsafe fn context<'a>(
    program: &'a c::Program<'a>,
    pointer: *mut c_void,
) -> Result<&'a RawContext<'a>, c::Fault> {
    object(pointer.cast::<RawContext<'_>>(), v::Object::Context)?;
    // SAFETY: opaque pointers must be live, unconsumed contexts created by this
    // adapter, on their synchronous owner thread. No mutable reference is formed.
    let context = unsafe { &*pointer.cast::<RawContext<'_>>() };
    if !ptr::eq(program, context.program) {
        return Err(c::Fault::new(c::Kind::WrongProgram));
    }
    Ok(context)
}
/// Borrow the safe gate for a generated export adapter, without publishing a
/// Rust reference or descriptor pointer to C.
///
/// # Safety
/// pointer is a live opaque context created by this adapter; a different
/// Program is refused before operation. Program/code/allocator/user storage
/// outlives the context. Use on its owner
/// thread only. It cannot be destroyed until this operation has returned.
pub unsafe fn with_context<T>(
    program: &c::Program<'_>,
    pointer: *mut c_void,
    operation: impl FnOnce(&c::Context<'_>) -> Result<T, c::Fault>,
) -> Result<T, c::Fault> {
    // SAFETY: forwarded live-context and synchronous lifetime contract.
    let context = unsafe { context(program, pointer)? };
    operation(&context.context)
}
/// Internal export bridge. The gate must be entered before borrowing the
/// allocator mutably; an imported callback can then refuse reentry atomically.
/// # Safety
/// Same live-context and synchronous no-destruction contract as with_context.
pub(crate) unsafe fn with_allocator<'a, T>(
    program: &'a c::Program<'a>,
    pointer: *mut c_void,
    operation: impl FnOnce(
        &c::Context<'a>,
        &std::cell::RefCell<super::super::allocator::CAllocator<'a>>,
        v::Address,
    ) -> Result<T, c::Fault>,
) -> Result<T, c::Fault> {
    // SAFETY: forwarded live opaque owner/lifetime contract.
    let context = unsafe { context(program, pointer)? };
    operation(
        &context.context,
        &context.allocator,
        v::Address {
            start: pointer.addr(),
            extent: size_of::<RawContext<'_>>(),
        },
    )
}
pub(super) unsafe extern "C" fn create(
    program: *const c::Program<'_>,
    config: *const ProgramConfig,
    out: *mut *mut c_void,
) -> u32 {
    contain(|| {
        let output = object(out, v::Object::Result)?;
        // SAFETY: generated code supplies its borrowed, stationary Program.
        let program = unsafe { &*program };
        // SAFETY: unsafe C entry caller supplies actual initialized objects
        // for all admitted reads, valid output storage, and synchronous lifetime.
        let views = unsafe { views(program, config, output)? };
        let context = program.create(views.config())?;
        drop(views); // No caller descriptor/name borrow survives publication.
        // SAFETY: this owning box drops the allocator before Context. Foreign
        // callback/user lifetimes are the same admitted C create contract.
        let allocator = unsafe { super::super::allocator::CAllocator::owned(&context) }
            .map_err(|_| c::Fault::new(c::Kind::Allocation))?;
        let pointer = Box::into_raw(Box::new(RawContext {
            allocator: std::cell::RefCell::new(allocator),
            program,
            context,
        }))
        .cast();
        // SAFETY: output range is admitted, disjoint, writable and possibly
        // uninitialized. Publication is the final, infallible action.
        unsafe {
            out.write(pointer);
        }
        Ok(())
    })
}
pub(super) unsafe extern "C" fn destroy(
    program: *const c::Program<'_>,
    pointer: *mut c_void,
) -> u32 {
    contain(|| {
        // SAFETY: generated Program and caller's live opaque context contracts.
        let context = unsafe { context(&*program, pointer)? };
        let result = context.context.close(&mut || Ok(()));
        if result
            .as_ref()
            .is_err_and(|fault| fault.kind == c::Kind::Reentry)
        {
            return result; // Active entry still owns the context; no revocation occurred.
        }
        // SAFETY: close revoked entry before consumption; no active borrow
        // remains. This is exactly the Box allocated by create, consumed once.
        unsafe {
            drop(Box::from_raw(pointer.cast::<RawContext<'_>>()));
        }
        result
    })
}
pub(super) unsafe extern "C" fn last_error(
    program: *const c::Program<'_>,
    pointer: *mut c_void,
    out: *mut c::ErrorSummary,
) -> u32 {
    contain(|| {
        let output = object(out, v::Object::Result)?;
        disjoint(
            output,
            object(pointer.cast::<RawContext<'_>>(), v::Object::Context)?,
            v::Object::Context,
        )?;
        // SAFETY: generated Program and caller's live opaque context contracts.
        let context = unsafe { context(&*program, pointer)? };
        let summary = context.context.last_error()?;
        // SAFETY: admitted disjoint writable actual output object; only a
        // completely initialized integer-only summary is published.
        unsafe {
            out.write(summary);
        }
        Ok(())
    })
}
