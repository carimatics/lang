#ifndef LANG_COMPONENT_ABI_V1_H
#define LANG_COMPONENT_ABI_V1_H
/* Generated from RFC 0027. Handles, scopes and buffers obey its lifetimes. */
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
typedef struct lang_component_v1_context lang_component_v1_context;
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
typedef uint8_t *(LANG_COMPONENT_V1_CALL *lang_component_v1_allocate_fn)(void *,uint64_t,uint64_t);
typedef void (LANG_COMPONENT_V1_CALL *lang_component_v1_deallocate_fn)(void *,uint8_t *,uint64_t,uint64_t);
typedef void (LANG_COMPONENT_V1_CALL *lang_component_v1_release_fn)(void *);
typedef lang_component_v1_status (LANG_COMPONENT_V1_CALL *lang_component_v1_invoke_fn)(void *,const lang_component_v1_context *,lang_component_v1_bytes,lang_component_v1_buffer **);
typedef lang_component_v1_status (LANG_COMPONENT_V1_CALL *lang_component_v1_resource_destroy_fn)(void *,const lang_component_v1_context *,lang_component_v1_mut_bytes,lang_component_v1_buffer **);
typedef struct lang_component_v1_context {
lang_component_v1_header header;
lang_component_v1_engine *engine;
uint64_t invocation;
uint64_t scope;
uint64_t reserved[2];
} lang_component_v1_context;
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
typedef struct lang_component_v1_allocator {
lang_component_v1_header header;
void *user;
lang_component_v1_allocate_fn allocate;
lang_component_v1_deallocate_fn deallocate;
uint64_t reserved[2];
} lang_component_v1_allocator;
typedef struct lang_component_v1_config {
lang_component_v1_header header;
uint32_t mode;
uint32_t flags;
lang_component_v1_allocator allocator;
lang_component_v1_limits ceiling;
uint64_t reserved[2];
} lang_component_v1_config;
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
uint32_t kind;
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
typedef struct lang_component_v1_api {
lang_component_v1_header header;
lang_component_v1_status (LANG_COMPONENT_V1_CALL *engine_create)(const lang_component_v1_config *,lang_component_v1_engine **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *engine_destroy)(lang_component_v1_engine *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *module_load)(lang_component_v1_engine *,uint32_t,lang_component_v1_bytes,lang_component_v1_bytes,lang_component_v1_module **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *module_drop)(lang_component_v1_module *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *module_bind_native)(lang_component_v1_engine *,const lang_component_v1_native_image *,lang_component_v1_module **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *provider_register)(lang_component_v1_engine *,const lang_component_v1_provider_config *,lang_component_v1_provider **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *provider_drop)(lang_component_v1_provider *,lang_component_v1_result **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *instance_create)(lang_component_v1_module *,const lang_component_v1_instance_config *,lang_component_v1_instance **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *instance_close)(lang_component_v1_instance *,lang_component_v1_result **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *bind_export)(lang_component_v1_instance *,lang_component_v1_bytes,lang_component_v1_binding **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *binding_drop)(lang_component_v1_binding *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *control_create)(lang_component_v1_engine *,const lang_component_v1_limits *,lang_component_v1_control **,lang_component_v1_error *);
void (LANG_COMPONENT_V1_CALL *interrupt)(lang_component_v1_control *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *control_drop)(lang_component_v1_control *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *call)(lang_component_v1_binding *,lang_component_v1_control *,lang_component_v1_bytes,lang_component_v1_result **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_call)(const lang_component_v1_context *,lang_component_v1_binding *,lang_component_v1_bytes,lang_component_v1_result **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_check)(const lang_component_v1_context *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_buffer_create)(const lang_component_v1_context *,uint64_t,uint64_t,lang_component_v1_buffer **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_buffer_drop)(const lang_component_v1_context *,lang_component_v1_buffer *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *buffer_mut_bytes)(const lang_component_v1_context *,lang_component_v1_buffer *,lang_component_v1_mut_bytes *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *result_info)(const lang_component_v1_result *,lang_component_v1_result_info *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *result_usage)(const lang_component_v1_result *,lang_component_v1_usage *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *result_bytes)(const lang_component_v1_result *,lang_component_v1_bytes *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *result_drop)(lang_component_v1_result *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *resource_drop)(lang_component_v1_instance *,uint32_t,lang_component_v1_result **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *resource_publish)(lang_component_v1_instance *,lang_component_v1_provider *,lang_component_v1_bytes,lang_component_v1_bytes,uint64_t,uint32_t *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_publish)(const lang_component_v1_context *,lang_component_v1_bytes,lang_component_v1_buffer *,uint32_t *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_drop)(const lang_component_v1_context *,uint32_t,lang_component_v1_result **,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_read)(const lang_component_v1_context *,uint32_t,lang_component_v1_bytes *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_write)(const lang_component_v1_context *,uint32_t,lang_component_v1_mut_bytes *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_keep)(const lang_component_v1_context *,uint32_t,uint32_t *,lang_component_v1_error *);
lang_component_v1_status (LANG_COMPONENT_V1_CALL *context_resource_take)(const lang_component_v1_context *,uint32_t,uint32_t *,lang_component_v1_error *);
uint64_t reserved[2];
} lang_component_v1_api;
lang_component_v1_status LANG_COMPONENT_V1_CALL lang_component_v1_get_api(uint32_t,uint32_t,const lang_component_v1_api **,lang_component_v1_error *);
#ifdef __cplusplus
}
#endif
#endif

#include <stdint.h>
#ifndef LANG_COMPONENT_VALUE_V1_H
#define LANG_COMPONENT_VALUE_V1_H
/* Generated typed views borrow frame/scratch storage. Native pointers must
 * denote the stated extents and must not alias codec state/output/scratch.
 * Contextual indices are untrusted claims; only the runtime adapter grants
 * ownership/loans. No pointer, padding or resource backing is serialized. */
#include <stddef.h>
#if defined(__clang__) || defined(__GNUC__)
#define LC_VALUE_INLINE static inline __attribute__((unused))
#else
#define LC_VALUE_INLINE static inline
#endif
#ifdef __cplusplus
#define LC_VALUE_ALIGNOF(T) alignof(T)
#else
#define LC_VALUE_ALIGNOF(T) _Alignof(T)
#endif
typedef struct lc_value_unit { uint8_t unused; } lc_value_unit;
typedef struct lc_value_limits { uint64_t work, elements, depth; } lc_value_limits;
typedef struct lc_value_writer {
    uint8_t *data; uint64_t capacity, at;
    lc_value_limits left; uint32_t status, contextual;
} lc_value_writer;
typedef struct lc_value_reader {
    const uint8_t *data; uint64_t length, at;
    uint8_t *scratch; uint64_t capacity, used;
    lc_value_limits left; uint32_t status, contextual;
} lc_value_reader;
/* Codec status: 0 success, 1 extent/truncation, 2 malformed, 3 finite limit.
 * Refusal may consume work/scratch and write scratch; no root is published. */
LC_VALUE_INLINE int lc_value_bad(uint32_t *status,uint32_t why) {
    if (*status==0) *status=why;
    return 0;
}
LC_VALUE_INLINE int lc_value_work(lc_value_limits *left,uint32_t *status,uint64_t n) {
    if (*status) return 0;
    if (n>left->work) return lc_value_bad(status,3);
    left->work-=n; return 1;
}
LC_VALUE_INLINE int lc_value_enter(lc_value_limits *left,uint32_t *status,uint64_t depth) {
    if (!lc_value_work(left,status,1)) return 0;
    if (depth==UINT64_MAX || depth>left->depth || left->elements==0) return lc_value_bad(status,3);
    --left->elements; return 1;
}
LC_VALUE_INLINE int lc_value_put(lc_value_writer *w,const uint8_t *data,uint64_t n) {
    uint64_t i;
    if (n==UINT64_MAX) return lc_value_bad(&w->status,3);
    if (!lc_value_work(&w->left,&w->status,n+1)) return 0;
    if (w->at>w->capacity || n>w->capacity-w->at || (n && (!data || !w->data))) return lc_value_bad(&w->status,1);
    for(i=0;i<n;++i) w->data[w->at+i]=data[i];
    w->at+=n; return 1;
}
LC_VALUE_INLINE int lc_value_get(lc_value_reader *r,uint64_t n,const uint8_t **data) {
    if (n==UINT64_MAX) return lc_value_bad(&r->status,3);
    if (!lc_value_work(&r->left,&r->status,n+1)) return 0;
    if (r->at>r->length || n>r->length-r->at || (n && !r->data)) return lc_value_bad(&r->status,1);
    *data=r->data ? r->data+r->at : r->data; r->at+=n; return 1;
}
LC_VALUE_INLINE int lc_value_unsigned(lc_value_writer *w,uint64_t value,uint32_t n) {
    uint8_t bytes[8]; uint32_t i;
    for(i=0;i<n;++i) {bytes[i]=(uint8_t)value;value>>=8;}
    return lc_value_put(w,bytes,n);
}
LC_VALUE_INLINE int lc_value_number(lc_value_reader *r,uint64_t *value,uint32_t n) {
    const uint8_t *bytes; uint32_t i; uint64_t result=0;
    if (!lc_value_get(r,n,&bytes)) return 0;
    for(i=0;i<n;++i) result|=((uint64_t)bytes[i])<<(8*i);
    *value=result; return 1;
}
LC_VALUE_INLINE int lc_value_utf8(const uint8_t *p,uint64_t n,lc_value_limits *left,uint32_t *status) {
    uint64_t i=0; uint32_t c,k,need,min;
    if (n==UINT64_MAX) return lc_value_bad(status,3);
    if (!lc_value_work(left,status,n+1)) return 0;
    if(n && !p) return lc_value_bad(status,1);
    while(i<n) {
        c=p[i++]; if(c<128) continue;
        if(c>=194 && c<=223) {need=1;min=128;c&=31;}
        else if(c>=224 && c<=239) {need=2;min=2048;c&=15;}
        else if(c>=240 && c<=244) {need=3;min=65536;c&=7;}
        else return lc_value_bad(status,2);
        if(need>n-i) return lc_value_bad(status,2);
        for(k=0;k<need;++k) {uint32_t b=p[i++];if(b<128 || b>191)return lc_value_bad(status,2);c=(c<<6)|(b&63);}
        if(c<min || c>1114111 || (c>=55296 && c<=57343)) return lc_value_bad(status,2);
    }
    return 1;
}
LC_VALUE_INLINE int lc_value_blob_write(lc_value_writer *w,const lang_component_v1_bytes *v,int text) {
    if(v->length>UINT32_MAX) return lc_value_bad(&w->status,2);
    if(text && !lc_value_utf8(v->data,v->length,&w->left,&w->status)) return 0;
    return lc_value_unsigned(w,v->length,4) && lc_value_put(w,v->data,v->length);
}
LC_VALUE_INLINE int lc_value_blob_read(lc_value_reader *r,lang_component_v1_bytes *v,int text) {
    uint64_t n; const uint8_t *p;
    if(!lc_value_number(r,&n,4) || !lc_value_get(r,n,&p)) return 0;
    if(text && !lc_value_utf8(p,n,&r->left,&r->status)) return 0;
    v->data=p;v->length=n;return 1;
}
LC_VALUE_INLINE void *lc_value_array(lc_value_reader *r,uint64_t count,uint64_t size,uint64_t alignment) {
    uintptr_t base,address; uint64_t padding,n;
    if(!lc_value_work(&r->left,&r->status,1))return NULL;
    if(count==0)return NULL;
    if(size==0 || count>UINT64_MAX/size || r->used>r->capacity || !r->scratch) {lc_value_bad(&r->status,3);return NULL;}
    n=count*size;base=(uintptr_t)r->scratch;
    if(r->used>UINTPTR_MAX-base) {lc_value_bad(&r->status,3);return NULL;}
    address=base+(uintptr_t)r->used;padding=(alignment-address%alignment)%alignment;
    if(padding>r->capacity-r->used || n>r->capacity-r->used-padding || padding>UINTPTR_MAX-address || n>UINTPTR_MAX-address-padding) {lc_value_bad(&r->status,3);return NULL;}
    r->used+=padding+n;return (void *)(address+(uintptr_t)padding);
}
LC_VALUE_INLINE int lc_value_frame_write(lc_value_writer *w,const uint8_t digest[32],uint32_t kind) {
    static const uint8_t magic[4]={'L','C','V','L'};
    if(w->at!=0)return lc_value_bad(&w->status,2);
    return lc_value_put(w,magic,4) && lc_value_unsigned(w,1,4) && lc_value_unsigned(w,kind,1) && lc_value_put(w,digest,32) && lc_value_unsigned(w,0,4);
}
LC_VALUE_INLINE int lc_value_frame_end(lc_value_writer *w) {
    uint64_t n;uint32_t i;
    if(w->status)return 0;
    if(w->at<45 || w->at-45>UINT32_MAX)return lc_value_bad(&w->status,2);
    if(!lc_value_work(&w->left,&w->status,5))return 0;
    n=w->at-45;for(i=0;i<4;++i){w->data[41+i]=(uint8_t)n;n>>=8;}return 1;
}
LC_VALUE_INLINE int lc_value_frame_read(lc_value_reader *r,const uint8_t digest[32],uint32_t kind) {
    const uint8_t *p;uint32_t i;uint64_t n;
    if(r->at!=0)return lc_value_bad(&r->status,2);
    if(!lc_value_get(r,45,&p))return 0;
    if(p[0]!='L'||p[1]!='C'||p[2]!='V'||p[3]!='L'||p[4]!=1||p[5]||p[6]||p[7]||p[8]!=kind)return lc_value_bad(&r->status,2);
    for(i=0;i<32;++i)if(p[9+i]!=digest[i])return lc_value_bad(&r->status,2);
    n=(uint64_t)p[41]|((uint64_t)p[42]<<8)|((uint64_t)p[43]<<16)|((uint64_t)p[44]<<24);
    if(n!=r->length-r->at)return lc_value_bad(&r->status,2);
    return 1;
}
/* commit: 0 known no commit, 1 committed, 2 unknown. Inspect/drop result
 * explicitly. A result/codec failure never silently recreates an owner. */
typedef struct lc_value_outcome {
    lang_component_v1_status status;
    uint32_t commit,poisoned;
    lang_component_v1_result *result;
} lc_value_outcome;
LC_VALUE_INLINE lc_value_outcome lc_value_observe(const lang_component_v1_api *api,lang_component_v1_status status,lang_component_v1_result *result,lang_component_v1_error *error) {
    lc_value_outcome out={status,0,0,result};
    if(status==LANG_COMPONENT_V1_OK || status==LANG_COMPONENT_V1_CALL_FAILED) {
        lang_component_v1_result_info info={0};info.header.size=sizeof(info);info.header.version=1;out.commit=2;
        if(result && api->result_info(result,&info,error)==LANG_COMPONENT_V1_OK) {out.commit=info.flags&1;out.poisoned=(info.flags>>1)&1;}
    }
    return out;
}
#endif

#ifndef component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_HEADER
#define component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_HEADER
typedef int64_t component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_t0;
LC_VALUE_INLINE int component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_e0(lc_value_writer *s,const component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_t0 *v,uint64_t depth){
if(!lc_value_enter(&s->left,&s->status,depth))return 0;
return lc_value_unsigned(s,(uint64_t)*v,8);
}
LC_VALUE_INLINE int component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_d0(lc_value_reader *s,component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_t0 *v,uint64_t depth){
if(!lc_value_enter(&s->left,&s->status,depth))return 0;
uint64_t n;if(!lc_value_number(s,&n,8))return 0;*v=n<=INT64_MAX?(int64_t)n:-INT64_C(1)-(int64_t)(UINT64_MAX-n);return 1;
}
/* demo:embed/api#answer; capability demo.embed.answer; sync. */
typedef struct {component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_t0 p0;} component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_a0;
static const uint8_t component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_signature[32]={230,10,35,241,82,126,51,234,214,217,96,125,235,127,194,121,213,200,48,248,185,244,154,229,196,176,167,58,167,130,84,40};
LC_VALUE_INLINE int component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_encode_arguments(lc_value_writer *s,const component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_a0 *value){
if(!lc_value_frame_write(s,component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_signature,0))return 0;
if(!component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_e0(s,&value->p0,1))return 0;
(void)value;return lc_value_frame_end(s);
}
LC_VALUE_INLINE int component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_encode_result(lc_value_writer *s,const component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_t0 *value){
if(!lc_value_frame_write(s,component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_signature,1))return 0;
if(!component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_e0(s,value,1))return 0;
(void)value;return lc_value_frame_end(s);
}
LC_VALUE_INLINE int component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_decode_arguments(lc_value_reader *s,component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_a0 *value){
if(!lc_value_frame_read(s,component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_signature,0))return 0;
component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_a0 temporary={0};
if(!component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_d0(s,&temporary.p0,1))return 0;
if(s->at!=s->length)return lc_value_bad(&s->status,2);*value=temporary;return 1;
}
LC_VALUE_INLINE int component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_decode_result(lc_value_reader *s,component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_t0 *value){
if(!lc_value_frame_read(s,component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_signature,1))return 0;
component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_t0 temporary={0};
if(!component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_d0(s,&temporary,1))return 0;
if(s->at!=s->length)return lc_value_bad(&s->status,2);*value=temporary;return 1;
}
LC_VALUE_INLINE lc_value_outcome component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_call(const lang_component_v1_api *api,lang_component_v1_control *control,lang_component_v1_binding *binding,const component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_a0 *arguments,lc_value_writer *scratch,lang_component_v1_error *error){
lang_component_v1_result *result=NULL;lang_component_v1_bytes bytes;lang_component_v1_status status;
scratch->contextual=1;if(!component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_encode_arguments(scratch,arguments)){lc_value_outcome out={LANG_COMPONENT_V1_INVALID_REQUEST,0,0,NULL};return out;}
bytes.data=scratch->data;bytes.length=scratch->at;
status=api->call(binding,control,bytes,&result,error);return lc_value_observe(api,status,result,error);
}
LC_VALUE_INLINE lc_value_outcome component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_context_call(const lang_component_v1_api *api,const lang_component_v1_context *context,lang_component_v1_binding *binding,const component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_a0 *arguments,lc_value_writer *scratch,lang_component_v1_error *error){
lang_component_v1_result *result=NULL;lang_component_v1_bytes bytes;lang_component_v1_status status;
scratch->contextual=1;if(!component_c_4964fd7785ff72760d2c4f96663b21a8a2dd9e418259f18556d0cb0b191ad851_f0_encode_arguments(scratch,arguments)){lc_value_outcome out={LANG_COMPONENT_V1_INVALID_REQUEST,0,0,NULL};return out;}
bytes.data=scratch->data;bytes.length=scratch->at;
status=api->context_call(context,binding,bytes,&result,error);return lc_value_observe(api,status,result,error);
}
#endif
