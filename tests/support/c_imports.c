#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#define EXPORT __declspec(dllexport)
#else
#define EXPORT __attribute__((visibility("default")))
#endif

typedef struct {
  uint64_t effects, allocations, frees;
  uint32_t fail, wrong;
} fixture_stats;
typedef struct {
  void *base;
  uint64_t bytes, alignment;
} fixture_allocation;

EXPORT void *c_allocate(void *user, uint64_t bytes, uint64_t alignment) {
  fixture_stats *stats = user;
  if (stats->fail) { --stats->fail; return NULL; }
  const size_t a = alignment < sizeof(void *) ? sizeof(void *) : (size_t)alignment;
  void *base = malloc((size_t)bytes + a - 1 + sizeof(fixture_allocation));
  if (!base) return NULL;
  uintptr_t start = ((uintptr_t)base + sizeof(fixture_allocation) + a - 1) & ~(uintptr_t)(a - 1);
  fixture_allocation *header = (fixture_allocation *)(start - sizeof(fixture_allocation));
  header->base = base; header->bytes = bytes; header->alignment = alignment;
  memset((void *)start, 0xa5, (size_t)bytes);
  ++stats->allocations;
  return (void *)start;
}
EXPORT void c_deallocate(void *user, void *pointer, uint64_t bytes, uint64_t alignment) {
  fixture_stats *stats = user;
  fixture_allocation *header = (fixture_allocation *)((uintptr_t)pointer - sizeof(fixture_allocation));
  if (header->bytes != bytes || header->alignment != alignment) ++stats->wrong;
  ++stats->frees;
  free(header->base);
}
EXPORT uint32_t c_increment(lang_context *context, void *user, int64_t value, int64_t *out) {
  if (!context) return 1;
  ++((fixture_stats *)user)->effects;
  if (value == -1) return 2;
  if (value == -2) return UINT32_MAX;
  *out = value + 1;
  return 0;
}
EXPORT uint32_t c_flag(lang_context *context, void *user, uint8_t value, uint8_t *out) {
  if (!context) return 1;
  ++((fixture_stats *)user)->effects;
  *out = value ? 1 : 2;
  return 0;
}
EXPORT uint32_t c_notify(lang_context *context, void *user, const UNIT_TYPE *value) {
  if (!context || value->reserved) return 1;
  ++((fixture_stats *)user)->effects;
  return 0;
}
EXPORT uint32_t c_combine(lang_context *context, void *user,
    int64_t a, uint8_t b, const UNIT_TYPE *c, int64_t d, uint8_t e, int64_t f,
    uint8_t g, int64_t h, int64_t i, uint8_t j, int64_t k, uint8_t l, int64_t *out) {
  if (!context || c->reserved) return 1;
  ++((fixture_stats *)user)->effects;
  *out = a + b + d + e + f + g + h + i + j + k + l;
  return 0;
}
