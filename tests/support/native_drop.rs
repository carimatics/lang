use super::*;

const DROP: &str = include_str!("../../examples/analysis/explicit_drop.lang");

#[test]
fn source_explicit_drop_matches_vm_at_fuel_work_and_allocation_boundaries() {
    for text in [
        DROP,
        include_str!("../../examples/analysis/explicit_drop_shared.lang"),
        "struct Pair { number: i64, flag: bool } fn main() -> i64 { let mut pair: Pair = Pair { number: 42, flag: true }; let mut d: core::Unit = core::drop::<Pair>(pair); if pair.flag { return pair.number; } return -1; }",
        "fn main() -> i64 { let mut d: core::Unit = core::drop::<core::Result<core::Bytes, core::AllocError>>(core::bytes_new(core::allocator())); return 42; }",
    ] {
        let owner = source(text);
        let flow = owner.failure_flow();
        let types = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let program = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
        let base = e::Control::with_limits(control::limits());
        let normal = control::native(flow, &layouts, &program, base, None);
        assert_eq!(normal.scalar, Some(42));
        let (expected, requests) = control::vm(flow, base, None);
        control::same(&normal, &expected);
        assert_eq!(normal.total_requests, requests);
        for fuel in 0..=normal.usage.fuel {
            let configured = e::Control::with_limits(e::Limits {
                fuel: Some(fuel),
                ..control::limits()
            });
            let actual = control::native(flow, &layouts, &program, configured, None);
            let (expected, requests) = control::vm(flow, configured, None);
            control::same(&actual, &expected);
            assert_eq!(actual.total_requests, requests);
        }
        for work in 0..=normal.usage.service_work {
            let configured = base.and_service_work(Some(work));
            let actual = control::native(flow, &layouts, &program, configured, None);
            let (expected, requests) = control::vm(flow, configured, None);
            control::same(&actual, &expected);
            assert_eq!(actual.total_requests, requests);
        }
        for request in 1..=normal.requests {
            let actual = execute_controlled(
                flow,
                &layouts,
                &program,
                Run {
                    inputs: &[],
                    fuel: None,
                    max_frames: 8,
                    cells: 1000,
                    refuse: Some(request),
                    refuse_cleanup: false,
                },
                base,
                None,
            );
            let at = normal.total_requests - normal.requests + request;
            let (expected, requests) = control::vm_with_refusal(flow, base, None, Some(at));
            control::same(&actual, &expected);
            assert_eq!(actual.total_requests, requests);
        }
    }
}

#[test]
fn explicit_drop_retires_before_later_scope_cleanup_and_never_retries_failure() {
    let text = "fn relay(first: core::Bytes, second: core::Bytes, kept: core::Bytes) -> core::Bytes { let mut d: core::Unit = core::drop::<core::Bytes>(first); return kept; }";
    let inputs: &[&[u8]] = &[b"first", b"second", b"kept"];
    let normal = execute_unit(text, inputs, None, 4, 1000, None, false);
    assert_eq!(normal.value.as_deref(), Some(inputs[2]));
    assert_eq!(
        normal.retired,
        [0, 1],
        "first retires at drop, then the later parameter at return"
    );
    assert_eq!(vm_record_order(text, 3), [0, 1, 2]);
    for fuel in 0..normal.fuel {
        let failed = execute_unit(text, inputs, Some(fuel), 4, 1000, None, false);
        assert!(failed.value.is_none());
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(retired, [0, 1, 2]);
    }
    let failed = execute_unit(text, inputs, None, 4, 1000, None, true);
    assert!(failed.value.is_none());
    assert_eq!(failed.retired, [0, 2, 1]);
    let failure = failed.failure.unwrap();
    let start = text.find("core::drop").unwrap() as u32;
    assert_eq!(failure.span.start, start);
    assert!(failed.report.primary().is_some());
    assert_eq!(failed.report.secondary().len(), 2);
}
