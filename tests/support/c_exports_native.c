typedef uint32_t (*fixture_create)(const lang_program_config *, lang_context **);
typedef uint32_t (*fixture_destroy)(lang_context *);
typedef uint32_t (*fixture_query)(lang_context *, lang_error_summary *);
typedef uint32_t (*fixture_scalar_export)(lang_context *, int64_t, int64_t *);

typedef struct {
    lang_context *context;
    fixture_query query;
    fixture_scalar_export again;
    uint64_t calls;
} fixture_probe;

EXPORT uint32_t c_export_once(fixture_scalar_export entry, lang_context *context,
                            int64_t value, int64_t *output) {
    return entry(context, value, output);
}

EXPORT uint32_t c_probe(lang_context *context, void *user, int64_t value, int64_t *output) {
    fixture_probe *probe = (fixture_probe *)user;
    lang_error_summary summary;
    lang_error_summary sentinel;
    int64_t nested = 77;
    ++probe->calls;
    if (context != probe->context) return 81;
    memset(&summary, 0x5a, sizeof(summary));
    memcpy(&sentinel, &summary, sizeof(summary));
    if (probe->query(context, &summary) != LANG_STATUS_INVALID ||
        memcmp(&summary, &sentinel, sizeof(summary)) != 0) return 82;
    if (probe->again(context, value, &nested) != LANG_STATUS_INVALID || nested != 77) return 83;
    if (value < 0) return 123;
    *output = value + 1;
    return 0;
}

EXPORT uint32_t c_exports_exercise(fixture_create create, fixture_destroy destroy,
                                 fixture_query query, fixture_scalar_export increment,
                                 fixture_scalar_export fail,
                                 const lang_program_config *config) {
    lang_context *context = NULL;
    lang_error_summary summary;
    int64_t result = 77;
    uint32_t status = create(config, &context);
    if (status != 0 || !context) return 91;
    if (increment(context, 41, &result) != 0 || result != 42) return 92;
    result = 77;
    if (fail(context, -42, &result) != LANG_STATUS_PANIC || result != 77) return 93;
    if (query(context, &summary) != 0 || summary.status != LANG_STATUS_PANIC || summary.panic_code != -42) return 94;
    if (increment(context, 41, &result) != 0 || result != 42) return 95;
    if (query(context, &summary) != 0 || summary.status != 0) return 96;
    if (destroy(context) != 0) return 97;
    return 0;
}
