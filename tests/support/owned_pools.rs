use super::*;

fn fixture() -> Box<OwnershipProgram> {
    checked(include_str!("../../examples/scripts/owned_calls.lang"))
}
fn layouts(typed: &b::TypedBodies) -> l::Layouts<'_> {
    l::Layouts::compile(typed.places().declarations().types(), Default::default()).unwrap()
}
fn for_name<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    typed: &b::TypedBodies,
    name: &str,
) -> l::FramePlan<'l, 't> {
    let function = typed
        .places()
        .declarations()
        .functions()
        .iter()
        .position(|f| f.name == name)
        .unwrap();
    l::FramePlan::compile(
        layouts,
        typed,
        m::FunctionId(function as u32),
        Default::default(),
        usize::MAX,
    )
    .unwrap()
}
fn assert_released(allocator: &a::Allocator<Provider>, trace: &Trace) {
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(trace.requests.len(), trace.released.len());
    let unique: std::collections::BTreeSet<_> = trace.released.iter().map(|(id, _)| *id).collect();
    assert_eq!(unique.len(), trace.released.len());
}

#[test]
fn prepared_wrappers_reuse_exact_capacity_and_budget_across_function_shapes() {
    let program = fixture();
    let typed = typed(&program);
    let layouts = layouts(typed);
    let main = for_name(&layouts, typed, "main");
    let callee = for_name(&layouts, typed, "length");
    assert!(main.cell_count() > callee.cell_count());
    let mut pool = main.prepare_pool().unwrap();
    let capacity = pool.cell_capacity();
    let usage = pool.usage();
    let (mut allocator, trace) = allocator(limits());
    assert!(trace.borrow().requests.is_empty());
    assert!(matches!(pool.cell(0), Err(l::Error::State)));
    for plan in [&main, &callee, &main, &callee] {
        let before = trace.borrow().requests.len();
        pool.admit(plan, &mut allocator).unwrap();
        let expected: Vec<_> = plan
            .requests()
            .iter()
            .copied()
            .filter(|r| r.bytes() != 0)
            .collect();
        assert_eq!(&trace.borrow().requests[before..], expected.as_slice());
        assert_eq!(pool.cell_capacity(), capacity);
        assert_eq!(pool.usage(), usage);
        assert_eq!(allocator.stats().live_allocations, plan.requests().len());
        assert!(matches!(
            pool.cell(plan.cell_count()),
            Err(l::Error::InvalidCell(_))
        ));
        let first = pool.cell(0).unwrap();
        assert!(
            first
                .data(&allocator)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
        pool.release(&mut allocator).unwrap();
        assert_released(&allocator, &trace.borrow());
    }
}

#[test]
fn every_pooled_admission_oom_rolls_back_in_reverse_and_can_retry_without_growth() {
    let program = fixture();
    let typed = typed(&program);
    let layouts = layouts(typed);
    let plan = for_name(&layouts, typed, "main");
    let nonzero = plan.requests().iter().filter(|r| r.bytes() != 0).count();
    for failed in 1..=nonzero {
        let mut pool = plan.prepare_pool().unwrap();
        let usage = pool.usage();
        let (mut allocator, trace) = allocator(limits());
        trace.borrow_mut().fail_at = failed;
        assert!(matches!(
            pool.admit(&plan, &mut allocator),
            Err(l::Error::Allocation(a::Error::OutOfMemory))
        ));
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(
            trace
                .borrow()
                .released
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            (1..failed).rev().collect::<Vec<_>>()
        );
        assert!(matches!(pool.cell(0), Err(l::Error::State)));
        assert_eq!(pool.usage(), usage);
        let attempted = allocator.stats().requests;
        let attempted_bytes = allocator.stats().cumulative_bytes;
        trace.borrow_mut().fail_at = 0;
        pool.admit(&plan, &mut allocator).unwrap();
        assert_eq!(
            allocator.stats().requests,
            attempted + plan.requests().len() as u64
        );
        assert_eq!(
            allocator.stats().cumulative_bytes,
            attempted_bytes + plan.requests().iter().map(|r| r.bytes()).sum::<u64>()
        );
        assert_eq!(pool.cell_capacity(), plan.cell_count());
        assert_eq!(pool.usage(), usage);
        pool.release(&mut allocator).unwrap();
        assert_eq!(
            trace.borrow().requests.len(),
            trace.borrow().released.len() + 1
        );
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn pooled_quota_capacity_and_active_state_fail_before_provider_callbacks() {
    let program = fixture();
    let typed = typed(&program);
    let layouts = layouts(typed);
    let main = for_name(&layouts, typed, "main");
    let callee = for_name(&layouts, typed, "length");
    let mut pool = main.prepare_pool().unwrap();
    let requested = main.requests().iter().map(|r| r.bytes()).sum::<u64>();
    let (mut denied, trace) = allocator(a::Limits {
        live_bytes: requested - 1,
        ..limits()
    });
    assert!(matches!(
        pool.admit(&main, &mut denied),
        Err(l::Error::Allocation(_))
    ));
    assert!(trace.borrow().requests.is_empty());
    let (mut allocator, trace) = allocator(limits());
    let mut small = callee.prepare_pool_after(main.usage()).unwrap();
    assert!(matches!(
        small.admit(&main, &mut allocator),
        Err(l::Error::FrameCapacity { .. })
    ));
    assert!(trace.borrow().requests.is_empty());
    pool.admit(&main, &mut allocator).unwrap();
    let callbacks = trace.borrow().calls;
    assert!(matches!(
        pool.admit(&callee, &mut allocator),
        Err(l::Error::State)
    ));
    assert_eq!(trace.borrow().calls, callbacks);
    pool.release(&mut allocator).unwrap();
    pool.admit(&callee, &mut allocator).unwrap();
    pool.release(&mut allocator).unwrap();
    assert_released(&allocator, &trace.borrow());
}

#[test]
fn released_pool_rejects_foreign_layouts_and_preserves_the_next_valid_admission() {
    let program = fixture();
    let typed = typed(&program);
    let original = layouts(typed);
    let other = layouts(typed);
    let first = for_name(&original, typed, "main");
    let foreign = for_name(&other, typed, "main");
    let mut pool = first.prepare_pool().unwrap();
    let (mut allocator, trace) = allocator(limits());
    assert!(matches!(
        pool.admit(&foreign, &mut allocator),
        Err(l::Error::ForeignLocation)
    ));
    assert!(trace.borrow().requests.is_empty());
    pool.admit(&first, &mut allocator).unwrap();
    pool.release(&mut allocator).unwrap();
    assert_released(&allocator, &trace.borrow());
}

#[test]
fn pool_host_budgets_are_exact_and_cannot_borrow_uncharged_compiler_usage() {
    let program = fixture();
    let typed = typed(&program);
    let layouts = layouts(typed);
    let plan = for_name(&layouts, typed, "main");
    let usage = plan.prepare_pool().unwrap().usage();
    let id = m::FunctionId(
        typed
            .places()
            .declarations()
            .functions()
            .iter()
            .position(|f| f.name == "main")
            .unwrap() as u32,
    );
    for short in [None, Some(0), Some(1), Some(2)] {
        let mut limits = m::Limits {
            work: usage.work,
            allocations: usage.allocation_bytes,
            elements: usage.elements,
            ..Default::default()
        };
        match short {
            Some(0) => limits.work -= 1,
            Some(1) => limits.allocations -= 1,
            Some(2) => limits.elements -= 1,
            _ => {}
        }
        let checked = l::FramePlan::compile(&layouts, typed, id, limits, usize::MAX).unwrap();
        let result = checked.prepare_pool();
        if short.is_some() {
            assert!(matches!(result, Err(l::Error::Resource { .. })));
        } else {
            assert_eq!(result.unwrap().usage(), usage);
        }
    }
    let initial = plan.usage();
    assert!(matches!(
        plan.prepare_pool_after(m::Usage {
            work: initial.work - 1,
            ..initial
        }),
        Err(l::Error::State)
    ));
}
