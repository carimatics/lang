use super::*;

#[test]
fn explicit_panic_matches_vm_with_dynamic_codes_cleanup_and_control_prefixes() {
    let cases = [0, -42, i64::MIN, i64::MAX].into_iter().map(|code| (
        include_str!("../../examples/analysis/explicit_panic_owned.lang").replace("fail(-42)", &format!("fail({code})")), code
    )).chain([
        ("fn read(p: &i64) -> i64 { return (*p); } fn main() -> i64 { return read(core::panic::<&i64>(42)); }".to_owned(), 42),
        ("fn main() -> i64 { (*core::panic::<&mut i64>(42)) = 1; }".to_owned(), 42),
        ("fn main() -> i64 { let mut never: &core::Allocator = core::panic::<&core::Allocator>(42); }".to_owned(), 42),
        ("fn main() -> i64 { return core::panic::<i64>(core::panic::<i64>(42)); }".to_owned(), 42),
        ("struct Pair { first: core::Bytes, code: i64 } fn main() -> i64 { let mut pair: Pair = Pair { first: core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())), code: core::panic::<i64>(42) }; }".to_owned(), 42),
    ]);
    for (text, code) in cases {
        let owner = source(&text);
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
        for index in 0..flow.functions().len() {
            assert_eq!(
                program
                    .metadata(m::FunctionId(index as u32))
                    .unwrap()
                    .symbol,
                format!("lang_owned_r3_f{index}")
            );
        }
        let base = e::Control::with_limits(control::limits());
        let normal = control::native(flow, &layouts, &program, base, None);
        let (expected, requests) = control::vm(flow, base, None);
        control::same(&normal, &expected);
        assert_eq!(normal.total_requests, requests);
        let vm_failure = expected.as_ref().err().unwrap();
        assert_eq!(
            normal.fault_stack,
            vm_failure
                .stack
                .iter()
                .map(|frame| (frame.function, frame.span.unwrap()))
                .collect::<Vec<_>>()
        );
        let failure = normal.failure.as_ref().unwrap();
        assert_eq!(failure.panic_code, Some(code));
        assert!(
            text[failure.span.start as usize..failure.span.end as usize].starts_with("core::panic")
        );
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
        use object::{Object, ObjectSymbol};
        for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let artifact = n::emit_object_for_target(
                flow,
                &layouts,
                &target.parse().unwrap(),
                Default::default(),
                1000,
            )
            .unwrap();
            let object = object::File::parse(&*artifact.bytes).unwrap();
            for index in 0..flow.functions().len() {
                let function = artifact.function(m::FunctionId(index as u32)).unwrap();
                let metadata = function.metadata();
                assert_eq!(metadata.symbol, format!("lang_owned_r3_f{index}"));
                assert!(object.symbols().any(|symbol| {
                    symbol.is_definition()
                        && symbol
                            .name()
                            .is_ok_and(|name| name.trim_start_matches('_') == metadata.symbol)
                }));
                assert!(!metadata.source_ranges.is_empty());
            }
        }
    }
}

#[test]
fn explicit_panic_keeps_primary_when_owner_cleanup_fails() {
    let text = "fn fail(first: core::Bytes, second: core::Bytes) -> i64 { return core::panic::<i64>(-42); }";
    let failed = execute_unit(text, &[b"first", b"second"], None, 4, 1000, None, true);
    assert!(failed.value.is_none());
    assert_eq!(failed.retired, [1, 0]);
    let failure = failed.failure.unwrap();
    assert_eq!(failure.panic_code, Some(-42));
    assert_eq!(
        failure.reason,
        n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::Panic))
    );
    assert_eq!(failed.report.secondary().len(), 2);
}
