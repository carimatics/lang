use lang_hir::core::{Operation, ResultBinding, TypeArgumentRule, TypePattern as P};
use lang_hir::resolved::LoanKind;

fn ty(pattern: P) -> String {
    match pattern {
        P::I64 => "i64".into(),
        P::Bool => "bool".into(),
        P::Builtin(builtin) => builtin.name().into(),
        P::Argument(0) => "T".into(),
        P::Argument(1) => "V".into(),
        P::Argument(other) => panic!("unexpected type parameter {other}"),
        P::Option(t) => format!("Option<{}>", ty(*t)),
        P::Result(t, e) => format!("Result<{}, {}>", ty(*t), ty(*e)),
        P::Vec(t) => format!("Vec<{}>", ty(*t)),
        P::Map(k, v) => format!("Map<{}, {}>", ty(*k), ty(*v)),
        P::Shared(t) => format!("shared {}", ty(*t)),
        P::Weak(t) => format!("weak {}", ty(*t)),
        P::Arena(t) => format!("arena {}", ty(*t)),
        P::Loan(LoanKind::Shared, t) => format!("&{}", ty(*t)),
        P::Loan(LoanKind::Exclusive, t) => format!("&mut {}", ty(*t)),
        P::ArenaOwner => "arena-owner".into(),
    }
}

#[test]
fn every_core_signature_matches_the_accepted_rfc_table() {
    // Independent source-contract expectations, including parameter order,
    // owning inputs, borrowed results, and every operation-specific error.
    let expected: Vec<_> = include_str!("../../../tests/fixtures/core-signatures.txt")
        .lines()
        .chain(include_str!("../../../tests/fixtures/source-intrinsic-signatures.txt").lines())
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let mut fields = line.split('|');
            let name = fields.next().unwrap();
            let arity: usize = fields.next().unwrap().parse().unwrap();
            let signature = fields.next().unwrap();
            assert!(fields.next().is_none());
            (name, arity, signature)
        })
        .collect();
    assert_eq!(expected.len(), Operation::ALL.len());
    let mut seen = std::collections::BTreeSet::new();
    for (name, arity, text) in expected {
        let operation = Operation::from_name(name).unwrap();
        assert!(seen.insert(operation));
        assert_eq!(operation.name(), name);
        let signature = operation.signature();
        assert_eq!(signature.type_arguments.len(), arity, "{name}");
        let actual = format!(
            "({}) -> {}",
            signature
                .parameters
                .iter()
                .map(|p| ty(*p))
                .collect::<Vec<_>>()
                .join(", "),
            ty(signature.result)
        );
        assert_eq!(actual, text, "{name}");
    }
    assert!(Operation::from_name("vec_clear").is_none());
    assert!(Operation::from_name("VecLen").is_none());
}

#[test]
fn signatures_record_result_provenance_and_concrete_argument_restrictions() {
    use ResultBinding as R;
    use TypeArgumentRule as T;
    for operation in Operation::ALL {
        let signature = operation.signature();
        let expected = match operation {
            Operation::Allocator => R::InstanceAllocator,
            Operation::VecGet
            | Operation::VecGetMut
            | Operation::MapGet
            | Operation::SharedRead
            | Operation::ArenaRead => R::ProvisionalLoan { parameter: 0 },
            Operation::ArenaNew => R::ArenaReference { owner: 0 },
            _ => R::Value,
        };
        assert_eq!(signature.result_binding, expected, "{operation:?}");
        let rules: &[T] = if operation.name().starts_with("map_") {
            &[T::MapKey, T::Storable]
        } else if operation.name().starts_with("shared_") || operation.name().starts_with("weak_") {
            &[T::SharedPayload]
        } else if operation.name().starts_with("result_") {
            &[T::Storable; 2]
        } else if operation.name().starts_with("vec_")
            || operation.name().starts_with("arena_")
            || operation.name().starts_with("option_")
            || operation == Operation::Drop
        {
            &[T::Storable]
        } else {
            &[]
        };
        assert_eq!(signature.type_arguments, rules, "{operation:?}");
        if let R::ProvisionalLoan { parameter } = expected {
            assert!(matches!(signature.parameters[parameter], P::Loan(..)));
        }
    }
}

#[test]
fn core_case_families_and_payloads_match_the_accepted_closed_sums() {
    use lang_hir::core::{Case, Family};
    let expected = [
        ("Option", "None", ""),
        ("Option", "Some", "T"),
        ("Result", "Ok", "T"),
        ("Result", "Err", "V"),
        ("AllocError", "Limit", ""),
        ("AllocError", "OutOfMemory", ""),
        ("AllocError", "Overflow", ""),
        ("BoundsError", "OutOfBounds", ""),
        ("BufferError", "Bounds", ""),
        ("BufferError", "InvalidByte", ""),
        ("BufferError", "Allocation", "AllocError"),
        ("Utf8Error", "Invalid", "i64"),
        ("OwnershipError", "Allocation", "AllocError"),
        ("OwnershipError", "StrongCycle", ""),
        ("OwnershipError", "ExpiredWeak", ""),
        ("OwnershipError", "BorrowConflict", ""),
        ("OwnershipError", "Unavailable", ""),
        ("OwnershipError", "CountOverflow", ""),
        ("ArenaError", "Allocation", "AllocError"),
        ("ArenaError", "CrossRegion", ""),
        ("ArenaError", "Unavailable", ""),
        ("ArenaError", "BorrowConflict", ""),
    ];
    assert_eq!(expected.len(), Case::ALL.len());
    let mut seen = std::collections::BTreeSet::new();
    for (family, name, payload) in expected {
        let case = Case::from_path(family, name).unwrap();
        assert!(seen.insert(case));
        assert_eq!(case.family().name(), family);
        assert_eq!(case.name(), name);
        assert_eq!(
            case.payload()
                .iter()
                .map(|p| ty(*p))
                .collect::<Vec<_>>()
                .join(", "),
            payload
        );
    }
    let mut grouped = std::collections::BTreeSet::new();
    for family in Family::ALL {
        assert_eq!(
            family.type_arguments(),
            match family {
                Family::Option => 1,
                Family::Result => 2,
                _ => 0,
            }
        );
        for case in family.cases() {
            assert_eq!(case.family(), family);
            assert!(grouped.insert(*case));
        }
    }
    assert_eq!(grouped, seen);
    assert!(Case::from_path("Option", "Ok").is_none());
    assert!(Case::from_path("ArenaError", "StrongCycle").is_none());
}
