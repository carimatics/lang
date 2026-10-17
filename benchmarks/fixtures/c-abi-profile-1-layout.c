/* RFC 0008 proposal fixture, not a generated production header.
 * No SDK headers are needed for cross-target Clang layout checks. Production
 * headers will use stdint.h/stddef.h and their target C compiler's assertions.
 */
typedef unsigned char u8;
typedef unsigned int u32;
typedef unsigned long long u64;
typedef signed long long i64;
typedef struct lang_context lang_context;

typedef struct {
    u32 version, size;
    void *user;
    void *(*allocate)(void *, u64, u64);
    void (*deallocate)(void *, void *, u64, u64);
} allocator;
typedef struct {
    u32 version, size;
    u64 live_bytes, cumulative_bytes, live_allocations, frames, cells, host_calls, fuel;
    u32 cleanup_records, reserved;
} limits;
typedef struct {
    u32 version, size;
    const char *name;
    u64 name_bytes;
    u8 signature_digest[32];
    void (*function)(void);
    void *user;
    u64 flags;
} import_binding;
typedef struct {
    u32 version, size;
    const allocator *allocator;
    const limits *limits;
    const import_binding *imports;
    u64 import_count;
    u8 artifact_digest[32];
    u64 flags;
} config;
typedef struct {
    i64 panic_code;
    u32 status, source, start, end, cleanup_count, flags;
} error_summary;
typedef struct { u8 reserved; } unit;
typedef struct { u8 f_flag; i64 f_value; } sample_record;
typedef struct {
    u32 tag;
    union { unit empty; sample_record record; } payload;
} sample_enum;

#define LAYOUT(type, bytes, alignment) \
    _Static_assert(sizeof(type) == (bytes), #type " size"); \
    _Static_assert(_Alignof(type) == (alignment), #type " alignment")
#define OFFSET(type, field, bytes) \
    _Static_assert(__builtin_offsetof(type, field) == (bytes), #type "." #field)

LAYOUT(void *, 8, 8);
LAYOUT(u32, 4, 4);
LAYOUT(u64, 8, 8);
LAYOUT(i64, 8, 8);
LAYOUT(allocator, 32, 8);
OFFSET(allocator, user, 8);
OFFSET(allocator, allocate, 16);
OFFSET(allocator, deallocate, 24);
LAYOUT(limits, 72, 8);
OFFSET(limits, live_bytes, 8);
OFFSET(limits, fuel, 56);
OFFSET(limits, cleanup_records, 64);
LAYOUT(import_binding, 80, 8);
OFFSET(import_binding, name, 8);
OFFSET(import_binding, name_bytes, 16);
OFFSET(import_binding, signature_digest, 24);
OFFSET(import_binding, function, 56);
OFFSET(import_binding, user, 64);
OFFSET(import_binding, flags, 72);
LAYOUT(config, 80, 8);
OFFSET(config, allocator, 8);
OFFSET(config, limits, 16);
OFFSET(config, imports, 24);
OFFSET(config, import_count, 32);
OFFSET(config, artifact_digest, 40);
OFFSET(config, flags, 72);
LAYOUT(error_summary, 32, 8);
OFFSET(error_summary, status, 8);
OFFSET(error_summary, flags, 28);
LAYOUT(unit, 1, 1);
LAYOUT(sample_record, 16, 8);
OFFSET(sample_record, f_value, 8);
LAYOUT(sample_enum, 24, 8);
OFFSET(sample_enum, payload, 8);

int main(void) { return 0; }
