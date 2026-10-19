typedef uint32_t (*fixture_create)(const lang_program_config *, lang_context **);
typedef uint32_t (*fixture_destroy)(lang_context *);
typedef uint32_t (*fixture_query)(lang_context *, lang_error_summary *);

EXPORT uint32_t c_context_snapshot(fixture_create create,
                                 const lang_program_config *original,
                                 lang_context **out) {
    lang_program_config config = *original;
    lang_allocator_descriptor allocator = *original->allocator;
    lang_limits_descriptor limits = *original->limits;
    lang_import_binding binding;
    char name[128];
    uint32_t status;
    if (original->import_count != 1 || original->imports->name_bytes > sizeof(name)) return 90;
    binding = *original->imports;
    memcpy(name, binding.name, (size_t)binding.name_bytes);
    binding.name = name;
    config.allocator = &allocator;
    config.limits = &limits;
    config.imports = &binding;
    status = create(&config, out);
    memset(name, 0xff, sizeof(name));
    allocator.allocate = NULL;
    allocator.deallocate = NULL;
    limits.frames = 0;
    binding.function = NULL;
    memset(binding.signature_digest, 0, sizeof(binding.signature_digest));
    memset(config.artifact_digest, 0, sizeof(config.artifact_digest));
    return status;
}

EXPORT uint32_t c_context_exercise(fixture_create create, fixture_destroy destroy,
                                 fixture_query query,
                                 const lang_program_config *original) {
    lang_program_config config = *original;
    lang_context *context = (lang_context *)(uintptr_t)UINT64_C(0x1238);
    lang_error_summary summary, before;
    config.version = 0;
    if (create(&config, &context) != LANG_STATUS_INVALID || context != (lang_context *)(uintptr_t)UINT64_C(0x1238)) return 91;
    config = *original;
    if (create(&config, &context) != LANG_STATUS_SUCCESS || !context) return 92;
    memset(&summary, 0xa5, sizeof(summary));
    before = summary;
    if (query(NULL, &summary) != LANG_STATUS_INVALID || memcmp(&summary, &before, sizeof(summary)) != 0) return 93;
    if (query(context, &summary) != LANG_STATUS_SUCCESS || summary.status != LANG_STATUS_SUCCESS || summary.panic_code != 0 || summary.source != UINT32_MAX) return 94;
    if (query(context, NULL) != LANG_STATUS_INVALID) return 95;
    if (destroy(context) != LANG_STATUS_SUCCESS) return 96;
    return 0;
}
