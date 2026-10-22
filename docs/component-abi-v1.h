#ifndef LANG_COMPONENT_ABI_V1_H
#define LANG_COMPONENT_ABI_V1_H

/* Normative RFC 0027 reference. No library implementation is implied.
 * I04 must generate this surface and matching target layout fingerprints.
 * All native callers/providers obey the lifetimes specified in RFC 0027. */
#include <stdint.h>
#if defined(_WIN32)
#define LANG_COMPONENT_V1_CALL __cdecl
#else
#define LANG_COMPONENT_V1_CALL
#endif
#ifdef __cplusplus
extern "C" {
#endif

#define LANG_COMPONENT_V1_VERSION UINT32_C(1)
#define LANG_COMPONENT_V1_OK UINT32_C(0)
#define LANG_COMPONENT_V1_CALL_FAILED UINT32_C(1)
#define LANG_COMPONENT_V1_INVALID_REQUEST UINT32_C(2)
#define LANG_COMPONENT_V1_UNSUPPORTED_VERSION UINT32_C(3)
#define LANG_COMPONENT_V1_BUSY UINT32_C(4)
#define LANG_COMPONENT_V1_STALE UINT32_C(5)
#define LANG_COMPONENT_V1_OUT_OF_MEMORY UINT32_C(6)
#define LANG_COMPONENT_V1_SOURCE UINT32_C(0)
#define LANG_COMPONENT_V1_LANGBC UINT32_C(1)

typedef uint32_t lang_component_v1_status;
typedef struct lang_component_v1_engine lang_component_v1_engine;
typedef struct lang_component_v1_module lang_component_v1_module;
typedef struct lang_component_v1_provider lang_component_v1_provider;
typedef struct lang_component_v1_instance lang_component_v1_instance;
typedef struct lang_component_v1_binding lang_component_v1_binding;
typedef struct lang_component_v1_control lang_component_v1_control;
typedef struct lang_component_v1_buffer lang_component_v1_buffer;
typedef struct lang_component_v1_result lang_component_v1_result;

/* Views are fixed-layout leaves. Every other public data structure has a
 * size/version header; all reserved fields must be zero. */
typedef struct lang_component_v1_bytes {
    const uint8_t *data;
    uint64_t length;
} lang_component_v1_bytes;
typedef struct lang_component_v1_mut_bytes {
    uint8_t *data;
    uint64_t length;
} lang_component_v1_mut_bytes;
typedef struct lang_component_v1_header {
    uint32_t size;
    uint32_t version;
} lang_component_v1_header;

/* Copying these bytes creates no new scope. Every use checks engine/thread,
 * invocation and live scope. A copied expired token is a structured refusal
 * while its engine remains alive; no stack address is retained. */
typedef struct lang_component_v1_context {
    lang_component_v1_header header;
    lang_component_v1_engine *engine;
    uint64_t invocation;
    uint64_t scope;
    uint64_t reserved[2];
} lang_component_v1_context;

/* Fixed admission diagnostic. Zero diagnostic means no diagnostic. Numeric
 * diagnostic identities and limit kinds are specified in RFC 0027. */
typedef struct lang_component_v1_error {
    lang_component_v1_header header;
    uint32_t diagnostic;
    uint32_t limit_kind;
    uint64_t limit;
    uint64_t attempted;
    uint8_t input_digest[32];
    uint32_t source_id;
    uint32_t start;
    uint32_t end;
    uint32_t detail_code;
    /* low 2 bits: none 0, source 1, IDL 2, artifact 3; bit 2: span valid */
    uint32_t flags;
    uint32_t reserved0;
    uint64_t reserved[2];
} lang_component_v1_error;

typedef struct lang_component_v1_limits {
    lang_component_v1_header header;
    uint64_t fuel;
    uint64_t service_work;
    uint64_t host_calls;
    uint64_t heap_bytes;
    uint64_t live_allocations;
    uint64_t frames;
    uint64_t live_cells;
    uint64_t live_tasks;
    uint64_t canonical_indices;
    uint64_t preparation_work;
    uint64_t metadata_bytes;
    uint64_t value_elements;
    uint64_t serialization_bytes;
    uint64_t depth;
    uint64_t decoder_allocation_bytes;
    uint64_t suppressed_records;
    uint64_t trace_frames;
    uint64_t reserved[2];
} lang_component_v1_limits;

typedef uint8_t *(LANG_COMPONENT_V1_CALL *lang_component_v1_allocate_fn)(
    void *user, uint64_t size, uint64_t alignment);
typedef void (LANG_COMPONENT_V1_CALL *lang_component_v1_deallocate_fn)(
    void *user, uint8_t *data, uint64_t size, uint64_t alignment);
typedef void (LANG_COMPONENT_V1_CALL *lang_component_v1_release_fn)(void *user);

typedef struct lang_component_v1_allocator {
    lang_component_v1_header header;
    void *user;
    lang_component_v1_allocate_fn allocate;
    lang_component_v1_deallocate_fn deallocate;
    uint64_t reserved[2];
} lang_component_v1_allocator;
typedef struct lang_component_v1_config {
    lang_component_v1_header header;
    /* 0 = verified VM, 1 = verified JIT, 2 = trusted native AOT. */
    uint32_t mode;
    uint32_t flags; /* bit 0 permits host-sanitized messages; other bits zero */
    lang_component_v1_allocator allocator;
    lang_component_v1_limits ceiling;
    uint64_t reserved[2];
} lang_component_v1_config;

/* On success the provider returns one context-allocated canonical value
 * frame. On CALL_FAILED it returns a context-allocated schema-3 report.
 * CALL_FAILED may use null when this context already latched a failure.
 * The adapter validates and consumes the buffer; all other callback-local
 * buffers are reclaimed at callback exit. Other statuses violate the host
 * callback contract. No unwind/exception crosses this C function. */
typedef lang_component_v1_status
(LANG_COMPONENT_V1_CALL *lang_component_v1_invoke_fn)(
    void *user, const lang_component_v1_context *context,
    lang_component_v1_bytes arguments, lang_component_v1_buffer **output);
typedef lang_component_v1_status
(LANG_COMPONENT_V1_CALL *lang_component_v1_resource_destroy_fn)(
    void *user, const lang_component_v1_context *context,
    lang_component_v1_mut_bytes backing, lang_component_v1_buffer **failure);
/* Destruction OK requires null failure; CALL_FAILED may return null for a
 * generic destructor failure. The runtime frees backing after either exit. */
typedef struct lang_component_v1_resource_type {
    lang_component_v1_header header;
    lang_component_v1_bytes resource_key;
    lang_component_v1_resource_destroy_fn destroy;
    void *user;
    uint64_t reserved[2];
} lang_component_v1_resource_type;
typedef struct lang_component_v1_import {
    lang_component_v1_header header;
    lang_component_v1_bytes function_key;
    uint8_t signature[32];
    lang_component_v1_invoke_fn invoke;
    void *user;
    /* bit 0 permits nested synchronous guest calls; other bits are zero. */
    uint32_t flags;
    uint32_t reserved0;
    uint64_t reserved[2];
} lang_component_v1_import;
typedef struct lang_component_v1_provider_config {
    lang_component_v1_header header;
    lang_component_v1_bytes contract;
    const lang_component_v1_import *imports;
    uint64_t import_count;
    const lang_component_v1_resource_type *resource_types;
    uint64_t resource_type_count;
    void *owner;
    lang_component_v1_release_fn release_owner;
    uint64_t reserved[2];
} lang_component_v1_provider_config;
/* Exact SDK-owned native program data, read only after bounded metadata and
 * image/compiler/SDK matching. It is never a serialized address or arbitrary
 * native-source ABI. Owner is consumed only after successful registration. */
typedef struct lang_component_v1_native_image {
    lang_component_v1_header header;
    lang_component_v1_bytes metadata;
    const void *program;
    void *owner;
    lang_component_v1_release_fn release_owner;
    uint64_t reserved[2];
} lang_component_v1_native_image;
typedef struct lang_component_v1_instance_config {
    lang_component_v1_header header;
    const lang_component_v1_bytes *grants;
    uint64_t grant_count;
    lang_component_v1_limits ceiling;
    uint64_t reserved[2];
} lang_component_v1_instance_config;

typedef struct lang_component_v1_result_info {
    lang_component_v1_header header;
    /* kind 0 = value (including declared Err), 1 = boundary failure. */
    uint32_t kind;
    /* bits: 0 argument commit occurred, 1 instance poisoned. */
    uint32_t flags;
    uint64_t omitted_records;
    uint64_t suppressed_count;
    uint64_t frame_count;
    uint64_t reserved[2];
} lang_component_v1_result_info;
typedef struct lang_component_v1_usage {
    lang_component_v1_header header;
    uint64_t fuel;
    uint64_t service_work;
    uint64_t host_calls;
    uint64_t peak_heap_bytes;
    uint64_t peak_live_allocations;
    uint64_t peak_frames;
    uint64_t peak_live_cells;
    uint64_t peak_live_tasks;
    uint64_t canonical_indices;
    uint64_t reserved[2];
} lang_component_v1_usage;

/* All functions except interrupt are owner-thread calls. Error pointers must
 * be valid initialized error structures. Non-call failures leave success
 * outputs untouched and may fill error. A completed call sets result for OK
 * or CALL_FAILED; the immutable result owns its value/report buffers. */
typedef struct lang_component_v1_api {
    lang_component_v1_header header;
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *engine_create)(
        const lang_component_v1_config *, lang_component_v1_engine **,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *engine_destroy)(
        lang_component_v1_engine *, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *module_load)(
        lang_component_v1_engine *, uint32_t format,
        lang_component_v1_bytes program, lang_component_v1_bytes contract,
        lang_component_v1_module **, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *module_drop)(
        lang_component_v1_module *, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *module_bind_native)(
        lang_component_v1_engine *, const lang_component_v1_native_image *,
        lang_component_v1_module **, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *provider_register)(
        lang_component_v1_engine *, const lang_component_v1_provider_config *,
        lang_component_v1_provider **, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *provider_drop)(
        lang_component_v1_provider *, lang_component_v1_result **,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *instance_create)(
        lang_component_v1_module *, const lang_component_v1_instance_config *,
        lang_component_v1_instance **, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *instance_close)(
        lang_component_v1_instance *, lang_component_v1_result **,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *bind_export)(
        lang_component_v1_instance *, lang_component_v1_bytes projection,
        lang_component_v1_binding **, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *binding_drop)(
        lang_component_v1_binding *, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *control_create)(
        lang_component_v1_engine *, const lang_component_v1_limits *,
        lang_component_v1_control **, lang_component_v1_error *);
    void (LANG_COMPONENT_V1_CALL *interrupt)(lang_component_v1_control *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *control_drop)(
        lang_component_v1_control *, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *call)(
        lang_component_v1_binding *, lang_component_v1_control *,
        lang_component_v1_bytes, lang_component_v1_result **,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_call)(
        const lang_component_v1_context *, lang_component_v1_binding *,
        lang_component_v1_bytes, lang_component_v1_result **,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_check)(
        const lang_component_v1_context *, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_buffer_create)(
        const lang_component_v1_context *, uint64_t length, uint64_t alignment,
        lang_component_v1_buffer **, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_buffer_drop)(
        const lang_component_v1_context *, lang_component_v1_buffer *,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *buffer_mut_bytes)(
        const lang_component_v1_context *, lang_component_v1_buffer *,
        lang_component_v1_mut_bytes *, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *result_info)(
        const lang_component_v1_result *, lang_component_v1_result_info *,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *result_usage)(
        const lang_component_v1_result *, lang_component_v1_usage *,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *result_bytes)(
        const lang_component_v1_result *, lang_component_v1_bytes *,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *result_drop)(
        lang_component_v1_result *, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *resource_drop)(
        lang_component_v1_instance *, uint32_t index,
        lang_component_v1_result **, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *resource_publish)(
        lang_component_v1_instance *, lang_component_v1_provider *,
        lang_component_v1_bytes resource_key, lang_component_v1_bytes backing,
        uint64_t alignment, uint32_t *index, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_publish)(
        const lang_component_v1_context *, lang_component_v1_bytes resource_key,
        lang_component_v1_buffer *backing, uint32_t *index,
        lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_drop)(
        const lang_component_v1_context *, uint32_t index,
        lang_component_v1_result **, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_read)(
        const lang_component_v1_context *, uint32_t index,
        lang_component_v1_bytes *, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_write)(
        const lang_component_v1_context *, uint32_t index,
        lang_component_v1_mut_bytes *, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_keep)(
        const lang_component_v1_context *, uint32_t index,
        uint32_t *persistent_index, lang_component_v1_error *);
    lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_take)(
        const lang_component_v1_context *, uint32_t persistent_index,
        uint32_t *index, lang_component_v1_error *);
    uint64_t reserved[2];
} lang_component_v1_api;

lang_component_v1_status LANG_COMPONENT_V1_CALL lang_component_v1_get_api(
    uint32_t version, uint32_t api_size, const lang_component_v1_api **output,
    lang_component_v1_error *error);

#ifdef __cplusplus
}
#endif
#endif
