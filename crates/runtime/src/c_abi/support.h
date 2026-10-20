
#ifndef LANG_CABI_PROFILE_1_SUPPORT
#define LANG_CABI_PROFILE_1_SUPPORT
typedef struct lang_context lang_context;
typedef struct {
  uint32_t version, size;
  void *user;
  void *(*allocate)(void *, uint64_t, uint64_t);
  void (*deallocate)(void *, void *, uint64_t, uint64_t);
} lang_allocator_descriptor;
typedef struct {
  uint32_t version, size;
  uint64_t live_bytes, cumulative_bytes, live_allocations, frames, cells, host_calls, fuel;
  uint32_t cleanup_records, reserved;
} lang_limits_descriptor;
typedef struct {
  uint32_t version, size;
  const char *name;
  uint64_t name_bytes;
  uint8_t signature_digest[32];
  void (*function)(void);
  void *user;
  uint64_t flags;
} lang_import_binding;
typedef struct {
  uint32_t version, size;
  const lang_allocator_descriptor *allocator;
  const lang_limits_descriptor *limits;
  const lang_import_binding *imports;
  uint64_t import_count;
  uint8_t artifact_digest[32];
  uint64_t flags;
} lang_program_config;
typedef struct {
  int64_t panic_code;
  uint32_t status, source, start, end, cleanup_count, flags;
} lang_error_summary;
#define LANG_CABI_VERSION UINT32_C(1)
#define LANG_CABI_NATIVE_FFI UINT64_C(1)
#define LANG_STATUS_SUCCESS UINT32_C(0)
#define LANG_STATUS_INVALID UINT32_C(1)
#define LANG_STATUS_PANIC UINT32_C(2)
#define LANG_STATUS_RUNTIME UINT32_C(3)
#define LANG_STATUS_CLEANUP UINT32_C(4)
#define LANG_STATUS_CAPABILITY UINT32_C(5)
#define LANG_STATUS_POISONED UINT32_C(6)
_Static_assert(sizeof(void *) == 8 && _Alignof(void *) == 8, "data pointer");
_Static_assert(sizeof(void (*)(void)) == 8 && _Alignof(void (*)(void)) == 8, "function pointer");
_Static_assert(sizeof(int64_t) == 8 && _Alignof(int64_t) == 8, "i64");
_Static_assert(sizeof(uint32_t) == 4 && _Alignof(uint32_t) == 4, "u32");
_Static_assert(sizeof(uint8_t) == 1 && _Alignof(uint8_t) == 1, "u8");
_Static_assert(sizeof(lang_allocator_descriptor) == 32 && _Alignof(lang_allocator_descriptor) == 8, "allocator");
_Static_assert(offsetof(lang_allocator_descriptor, user) == 8, "allocator user");
_Static_assert(offsetof(lang_allocator_descriptor, allocate) == 16, "allocator allocate");
_Static_assert(offsetof(lang_allocator_descriptor, deallocate) == 24, "allocator deallocate");
_Static_assert(sizeof(lang_limits_descriptor) == 72 && _Alignof(lang_limits_descriptor) == 8, "limits");
_Static_assert(offsetof(lang_limits_descriptor, live_bytes) == 8, "limits bytes");
_Static_assert(offsetof(lang_limits_descriptor, fuel) == 56, "limits fuel");
_Static_assert(offsetof(lang_limits_descriptor, cleanup_records) == 64, "limits cleanup");
_Static_assert(offsetof(lang_limits_descriptor, reserved) == 68, "limits reserved");
_Static_assert(sizeof(lang_import_binding) == 80 && _Alignof(lang_import_binding) == 8, "binding");
_Static_assert(offsetof(lang_import_binding, name) == 8, "binding name");
_Static_assert(offsetof(lang_import_binding, name_bytes) == 16, "binding name bytes");
_Static_assert(offsetof(lang_import_binding, signature_digest) == 24, "binding digest");
_Static_assert(offsetof(lang_import_binding, function) == 56, "binding function");
_Static_assert(offsetof(lang_import_binding, user) == 64, "binding user");
_Static_assert(offsetof(lang_import_binding, flags) == 72, "binding flags");
_Static_assert(sizeof(lang_program_config) == 80 && _Alignof(lang_program_config) == 8, "config");
_Static_assert(offsetof(lang_program_config, allocator) == 8, "config allocator");
_Static_assert(offsetof(lang_program_config, limits) == 16, "config limits");
_Static_assert(offsetof(lang_program_config, imports) == 24, "config imports");
_Static_assert(offsetof(lang_program_config, import_count) == 32, "config count");
_Static_assert(offsetof(lang_program_config, artifact_digest) == 40, "config digest");
_Static_assert(offsetof(lang_program_config, flags) == 72, "config flags");
_Static_assert(sizeof(lang_error_summary) == 32 && _Alignof(lang_error_summary) == 8, "error summary");
_Static_assert(offsetof(lang_error_summary, status) == 8, "error status");
_Static_assert(offsetof(lang_error_summary, source) == 12, "error source");
_Static_assert(offsetof(lang_error_summary, start) == 16, "error start");
_Static_assert(offsetof(lang_error_summary, end) == 20, "error end");
_Static_assert(offsetof(lang_error_summary, cleanup_count) == 24, "error cleanup");
_Static_assert(offsetof(lang_error_summary, flags) == 28, "error flags");
#endif
