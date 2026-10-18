use super::*;

#[test]
fn nested_service_cycle_fixture_checks_but_remains_gated_until_graph_and_cleanup_integration() {
    let program = checked(include_str!("../../examples/analysis/nested_services.lang"));
    let (mut allocator, trace) = allocator_with_limits(Default::default());
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        Control::default(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        vm::Reason::Unsupported {
            core: Some(Operation::VecNew)
        }
    ));
    assert_eq!(allocator.stats(), a::Stats::default());
    assert!(trace.borrow().requests.is_empty());
}
