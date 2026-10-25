use lang_mir::unit::{Budget, Limits};

#[test]
fn checkpoints_follow_the_original_ledger_across_moves_and_scopes() {
    let mut budget = Budget::new(Limits::default());
    budget.charge(3).unwrap();
    let checkpoint = {
        let mut scope = budget.scope_usage();
        scope.charge(5).unwrap();
        scope.checkpoint().unwrap()
    };
    let mut moved = budget;
    assert!(moved.includes_checkpoint(checkpoint));
    moved.charge(7).unwrap();
    assert!(moved.includes_checkpoint(checkpoint));
    moved
        .advance_to(lang_mir::unit::Usage {
            work: 19,
            allocation_bytes: 2,
            elements: 3,
        })
        .unwrap();
    assert!(moved.includes_checkpoint(checkpoint));
    assert!(
        moved
            .advance_to(lang_mir::unit::Usage {
                work: 18,
                allocation_bytes: 2,
                elements: 3
            })
            .is_err()
    );
    assert_eq!(moved.work_used(), 19);
    let mut other = Budget::new(Limits::default());
    other.charge(15).unwrap();
    let different = other.checkpoint().unwrap();
    assert!(!other.includes_checkpoint(checkpoint));
    assert!(!moved.includes_checkpoint(different));
}

#[test]
fn intrinsic_reporting_keeps_original_admission_and_nested_prefixes() {
    let mut budget = Budget::new(Limits {
        work: 12,
        allocations: 12,
        elements: 3,
        ..Limits::default()
    });
    budget.charge(3).unwrap();
    budget.allocate::<u8>(2).unwrap();
    {
        let mut scope = budget.scope_usage();
        assert_eq!(scope.work_used(), 0);
        assert_eq!(scope.allocation_bytes(), 0);
        assert_eq!(scope.elements_used(), 0);
        scope.charge(4).unwrap();
        assert!(scope.allocate::<u16>(2).is_err());
        assert_eq!(scope.allocation_bytes(), 0);
        assert_eq!(scope.elements_used(), 0);
        scope.allocate::<u8>(1).unwrap();
        {
            let mut nested = scope.scope_usage();
            nested.charge(2).unwrap();
            assert_eq!(nested.work_used(), 2);
            assert!(nested.allocate::<u8>(1).is_err());
        }
        assert_eq!(scope.work_used(), 6);
        assert_eq!(scope.allocation_bytes(), 1);
        assert_eq!(scope.elements_used(), 1);
        assert!(scope.charge(4).is_err());
    }
    assert_eq!(budget.work_used(), 9);
    assert_eq!(budget.allocation_bytes(), 3);
    assert_eq!(budget.elements_used(), 3);
}

#[test]
fn intrinsic_reporting_restores_after_unwind_without_refunding_charges() {
    let mut budget = Budget::new(Limits::default());
    budget.charge(7).unwrap();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = budget.scope_usage();
        scope.charge(11).unwrap();
        panic!("contained proof failure");
    }));
    assert!(failure.is_err());
    assert_eq!(budget.work_used(), 18);
    {
        let mut scope = budget.scope_usage();
        scope.charge(5).unwrap();
        assert_eq!(scope.work_used(), 5);
    }
    assert_eq!(budget.work_used(), 23);
}
