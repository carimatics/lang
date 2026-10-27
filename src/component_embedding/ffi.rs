//! Rust layout mirror of the normative component-v1 C header.
//! The complete table is published by gateway::lang_component_v1_get_api.

pub use lang_runtime::component::native::{Buffer, Bytes, Context, Engine, Header, MutBytes};
pub type Status = u32;

#[repr(C)]
pub struct Module {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Provider {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Instance {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Binding {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Control {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Result {
    _private: [u8; 0],
}

pub type AllocateFn = unsafe extern "C" fn(*mut std::ffi::c_void, u64, u64) -> *mut u8;
pub type DeallocateFn = unsafe extern "C" fn(*mut std::ffi::c_void, *mut u8, u64, u64) -> ();
pub type ReleaseFn = unsafe extern "C" fn(*mut std::ffi::c_void) -> ();
pub type InvokeFn = lang_runtime::component::native::Invoke;
pub type ResourceDestroyFn = lang_runtime::component::native::Destroy;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Error {
    pub header: Header,
    pub diagnostic: u32,
    pub limit_kind: u32,
    pub limit: u64,
    pub attempted: u64,
    pub input_digest: [u8; 32],
    pub source_id: u32,
    pub start: u32,
    pub end: u32,
    pub detail_code: u32,
    pub flags: u32,
    pub reserved0: u32,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Limits {
    pub header: Header,
    pub fuel: u64,
    pub service_work: u64,
    pub host_calls: u64,
    pub heap_bytes: u64,
    pub live_allocations: u64,
    pub frames: u64,
    pub live_cells: u64,
    pub live_tasks: u64,
    pub canonical_indices: u64,
    pub preparation_work: u64,
    pub metadata_bytes: u64,
    pub value_elements: u64,
    pub serialization_bytes: u64,
    pub depth: u64,
    pub decoder_allocation_bytes: u64,
    pub suppressed_records: u64,
    pub trace_frames: u64,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Allocator {
    pub header: Header,
    pub user: *mut std::ffi::c_void,
    pub allocate: Option<AllocateFn>,
    pub deallocate: Option<DeallocateFn>,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Config {
    pub header: Header,
    pub mode: u32,
    pub flags: u32,
    pub allocator: Allocator,
    pub ceiling: Limits,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResourceType {
    pub header: Header,
    pub resource_key: Bytes,
    pub destroy: Option<ResourceDestroyFn>,
    pub user: *mut std::ffi::c_void,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Import {
    pub header: Header,
    pub function_key: Bytes,
    pub signature: [u8; 32],
    pub invoke: Option<InvokeFn>,
    pub user: *mut std::ffi::c_void,
    pub flags: u32,
    pub reserved0: u32,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProviderConfig {
    pub header: Header,
    pub contract: Bytes,
    pub imports: *const Import,
    pub import_count: u64,
    pub resource_types: *const ResourceType,
    pub resource_type_count: u64,
    pub owner: *mut std::ffi::c_void,
    pub release_owner: Option<ReleaseFn>,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeImage {
    pub header: Header,
    pub metadata: Bytes,
    pub program: *const std::ffi::c_void,
    pub owner: *mut std::ffi::c_void,
    pub release_owner: Option<ReleaseFn>,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InstanceConfig {
    pub header: Header,
    pub grants: *const Bytes,
    pub grant_count: u64,
    pub ceiling: Limits,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResultInfo {
    pub header: Header,
    pub kind: u32,
    pub flags: u32,
    pub omitted_records: u64,
    pub suppressed_count: u64,
    pub frame_count: u64,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Usage {
    pub header: Header,
    pub fuel: u64,
    pub service_work: u64,
    pub host_calls: u64,
    pub peak_heap_bytes: u64,
    pub peak_live_allocations: u64,
    pub peak_frames: u64,
    pub peak_live_cells: u64,
    pub peak_live_tasks: u64,
    pub canonical_indices: u64,
    pub reserved: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Api {
    pub header: Header,
    pub engine_create: unsafe extern "C" fn(*const Config, *mut *mut Engine, *mut Error) -> Status,
    pub engine_destroy: unsafe extern "C" fn(*mut Engine, *mut Error) -> Status,
    pub module_load: unsafe extern "C" fn(
        *mut Engine,
        u32,
        Bytes,
        Bytes,
        *mut *mut Module,
        *mut Error,
    ) -> Status,
    pub module_drop: unsafe extern "C" fn(*mut Module, *mut Error) -> Status,
    pub module_bind_native: unsafe extern "C" fn(
        *mut Engine,
        *const NativeImage,
        *mut *mut Module,
        *mut Error,
    ) -> Status,
    pub provider_register: unsafe extern "C" fn(
        *mut Engine,
        *const ProviderConfig,
        *mut *mut Provider,
        *mut Error,
    ) -> Status,
    pub provider_drop: unsafe extern "C" fn(*mut Provider, *mut *mut Result, *mut Error) -> Status,
    pub instance_create: unsafe extern "C" fn(
        *mut Module,
        *const InstanceConfig,
        *mut *mut Instance,
        *mut Error,
    ) -> Status,
    pub instance_close: unsafe extern "C" fn(*mut Instance, *mut *mut Result, *mut Error) -> Status,
    pub bind_export:
        unsafe extern "C" fn(*mut Instance, Bytes, *mut *mut Binding, *mut Error) -> Status,
    pub binding_drop: unsafe extern "C" fn(*mut Binding, *mut Error) -> Status,
    pub control_create:
        unsafe extern "C" fn(*mut Engine, *const Limits, *mut *mut Control, *mut Error) -> Status,
    pub interrupt: unsafe extern "C" fn(*mut Control) -> (),
    pub control_drop: unsafe extern "C" fn(*mut Control, *mut Error) -> Status,
    pub call: unsafe extern "C" fn(
        *mut Binding,
        *mut Control,
        Bytes,
        *mut *mut Result,
        *mut Error,
    ) -> Status,
    pub context_call: unsafe extern "C" fn(
        *const Context,
        *mut Binding,
        Bytes,
        *mut *mut Result,
        *mut Error,
    ) -> Status,
    pub context_check: unsafe extern "C" fn(*const Context, *mut Error) -> Status,
    pub context_buffer_create:
        unsafe extern "C" fn(*const Context, u64, u64, *mut *mut Buffer, *mut Error) -> Status,
    pub context_buffer_drop:
        unsafe extern "C" fn(*const Context, *mut Buffer, *mut Error) -> Status,
    pub buffer_mut_bytes:
        unsafe extern "C" fn(*const Context, *mut Buffer, *mut MutBytes, *mut Error) -> Status,
    pub result_info: unsafe extern "C" fn(*const Result, *mut ResultInfo, *mut Error) -> Status,
    pub result_usage: unsafe extern "C" fn(*const Result, *mut Usage, *mut Error) -> Status,
    pub result_bytes: unsafe extern "C" fn(*const Result, *mut Bytes, *mut Error) -> Status,
    pub result_drop: unsafe extern "C" fn(*mut Result, *mut Error) -> Status,
    pub resource_drop:
        unsafe extern "C" fn(*mut Instance, u32, *mut *mut Result, *mut Error) -> Status,
    pub resource_publish: unsafe extern "C" fn(
        *mut Instance,
        *mut Provider,
        Bytes,
        Bytes,
        u64,
        *mut u32,
        *mut Error,
    ) -> Status,
    pub context_resource_publish:
        unsafe extern "C" fn(*const Context, Bytes, *mut Buffer, *mut u32, *mut Error) -> Status,
    pub context_resource_drop:
        unsafe extern "C" fn(*const Context, u32, *mut *mut Result, *mut Error) -> Status,
    pub context_resource_read:
        unsafe extern "C" fn(*const Context, u32, *mut Bytes, *mut Error) -> Status,
    pub context_resource_write:
        unsafe extern "C" fn(*const Context, u32, *mut MutBytes, *mut Error) -> Status,
    pub context_resource_keep:
        unsafe extern "C" fn(*const Context, u32, *mut u32, *mut Error) -> Status,
    pub context_resource_take:
        unsafe extern "C" fn(*const Context, u32, *mut u32, *mut Error) -> Status,
    pub reserved: [u64; 2],
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_layouts_match_every_normative_size_alignment_and_field_offset() {
        let reference: serde_json::Value =
            serde_json::from_str(include_str!("../../docs/component-abi-v1-layouts.json")).unwrap();
        macro_rules! check {
            ($ty:ty, $name:literal, $($field:ident),* $(,)?) => {{
                let layout = &reference["layout"][$name];
                assert_eq!(std::mem::size_of::<$ty>() as u64, layout["size"].as_u64().unwrap(), "{} size", $name);
                assert_eq!(std::mem::align_of::<$ty>() as u64, layout["align"].as_u64().unwrap(), "{} align", $name);
                $(assert_eq!(std::mem::offset_of!($ty, $field) as u64, layout["offsets"][stringify!($field)].as_u64().unwrap(), "{}.{}", $name, stringify!($field));)*
            }};
        }
        check!(Bytes, "bytes", data, length);
        check!(MutBytes, "mut_bytes", data, length);
        check!(Header, "header", size, version);
        check!(
            Context, "context", header, engine, invocation, scope, reserved
        );
        check!(
            Error,
            "error",
            header,
            diagnostic,
            limit_kind,
            limit,
            attempted,
            input_digest,
            source_id,
            start,
            end,
            detail_code,
            flags,
            reserved0,
            reserved
        );
        check!(
            Limits,
            "limits",
            header,
            fuel,
            service_work,
            host_calls,
            heap_bytes,
            live_allocations,
            frames,
            live_cells,
            live_tasks,
            canonical_indices,
            preparation_work,
            metadata_bytes,
            value_elements,
            serialization_bytes,
            depth,
            decoder_allocation_bytes,
            suppressed_records,
            trace_frames,
            reserved
        );
        check!(
            Allocator,
            "allocator",
            header,
            user,
            allocate,
            deallocate,
            reserved
        );
        check!(
            Config, "config", header, mode, flags, allocator, ceiling, reserved
        );
        check!(
            ResourceType,
            "resource_type",
            header,
            resource_key,
            destroy,
            user,
            reserved
        );
        check!(
            Import,
            "import",
            header,
            function_key,
            signature,
            invoke,
            user,
            flags,
            reserved0,
            reserved
        );
        check!(
            ProviderConfig,
            "provider_config",
            header,
            contract,
            imports,
            import_count,
            resource_types,
            resource_type_count,
            owner,
            release_owner,
            reserved
        );
        check!(
            NativeImage,
            "native_image",
            header,
            metadata,
            program,
            owner,
            release_owner,
            reserved
        );
        check!(
            InstanceConfig,
            "instance_config",
            header,
            grants,
            grant_count,
            ceiling,
            reserved
        );
        check!(
            ResultInfo,
            "result_info",
            header,
            kind,
            flags,
            omitted_records,
            suppressed_count,
            frame_count,
            reserved
        );
        check!(
            Usage,
            "usage",
            header,
            fuel,
            service_work,
            host_calls,
            peak_heap_bytes,
            peak_live_allocations,
            peak_frames,
            peak_live_cells,
            peak_live_tasks,
            canonical_indices,
            reserved
        );
        check!(
            Api,
            "api",
            header,
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
            reserved
        );
    }
}
