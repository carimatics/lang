pub(crate) const SERVICE_SHUTDOWN_FIXTURES: &[(&str, &str)] = &[
    ("idle", include_str!("../fixtures/task_service_cancel.lang")),
    (
        "active",
        include_str!("../fixtures/task_service_active_cancel.lang"),
    ),
];

pub(crate) const SERVICE_COPY_FIXTURES: &[(&str, &str)] = &[
    (
        "copy-unit",
        include_str!("../fixtures/task_service_copy_unit.lang"),
    ),
    (
        "copy-option",
        include_str!("../fixtures/task_service_copy_sum.lang"),
    ),
    (
        "copy-aggregate",
        include_str!("../fixtures/task_service_copy.lang"),
    ),
    (
        "copy-cancel",
        include_str!("../fixtures/task_service_copy_cancel.lang"),
    ),
    (
        "copy-recover",
        include_str!("../fixtures/task_service_copy_recover.lang"),
    ),
];

pub(crate) const SERVICE_STATE_FIXTURES: &[(&str, &str)] = &[
    (
        "receiver-state",
        include_str!("../fixtures/task_service_owned_state.lang"),
    ),
    (
        "state-cycle",
        include_str!("../fixtures/task_service_state_cycle.lang"),
    ),
    (
        "state-reinspect",
        include_str!("../fixtures/task_service_state_reinspect.lang"),
    ),
    (
        "state-replace",
        include_str!("../fixtures/task_service_state_replace.lang"),
    ),
    (
        "state-shared-handle",
        include_str!("../fixtures/task_service_state_shared_handle.lang"),
    ),
];

pub(crate) fn fixtures() -> impl Iterator<Item = (&'static str, &'static str)> {
    std::iter::once(("fifo-state", include_str!("../fixtures/task_service.lang")))
        .chain(SERVICE_SHUTDOWN_FIXTURES.iter().copied())
        .chain(SERVICE_COPY_FIXTURES.iter().copied())
        .chain(SERVICE_STATE_FIXTURES.iter().copied())
        .chain(std::iter::once((
            "deep-state",
            include_str!("../fixtures/task_service_state_deep.lang"),
        )))
}

// Exercise every refusal prefix on a small recursive graph; the original
// 64-level fixture separately checks depth and exact resource boundaries.
pub(crate) fn recursive_prefix_fixture() -> (&'static str, String) {
    (
        "recursive-state-prefix",
        include_str!("../fixtures/task_service_state_deep.lang").replace("depth<64", "depth<4"),
    )
}
