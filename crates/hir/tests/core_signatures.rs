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
    let expected = [
        ("allocator", 0, "() -> &Allocator"),
        ("text_from_utf8", 0, "(Bytes) -> Result<Text, Utf8Error>"),
        (
            "text_clone",
            0,
            "(&Allocator, &Text) -> Result<Text, AllocError>",
        ),
        ("text_len", 0, "(&Text) -> i64"),
        (
            "text_bytes",
            0,
            "(&Allocator, &Text) -> Result<Bytes, AllocError>",
        ),
        ("text_equal", 0, "(&Text, &Text) -> bool"),
        (
            "text_concat",
            0,
            "(&Allocator, &Text, &Text) -> Result<Text, AllocError>",
        ),
        ("bytes_new", 0, "(&Allocator) -> Result<Bytes, AllocError>"),
        (
            "bytes_clone",
            0,
            "(&Allocator, &Bytes) -> Result<Bytes, AllocError>",
        ),
        ("bytes_len", 0, "(&Bytes) -> i64"),
        ("bytes_get", 0, "(&Bytes, i64) -> Result<i64, BoundsError>"),
        (
            "bytes_push",
            0,
            "(&mut Bytes, i64) -> Result<Unit, BufferError>",
        ),
        (
            "bytes_set",
            0,
            "(&mut Bytes, i64, i64) -> Result<Unit, BufferError>",
        ),
        ("vec_new", 1, "(&Allocator) -> Result<Vec<T>, AllocError>"),
        ("vec_len", 1, "(&Vec<T>) -> i64"),
        (
            "vec_push",
            1,
            "(&mut Vec<T>, T) -> Result<Unit, AllocError>",
        ),
        ("vec_pop", 1, "(&mut Vec<T>) -> Option<T>"),
        ("vec_get", 1, "(&Vec<T>, i64) -> Result<&T, BoundsError>"),
        (
            "vec_get_mut",
            1,
            "(&mut Vec<T>, i64) -> Result<&mut T, BoundsError>",
        ),
        (
            "map_new",
            2,
            "(&Allocator) -> Result<Map<T, V>, AllocError>",
        ),
        ("map_len", 2, "(&Map<T, V>) -> i64"),
        (
            "map_insert",
            2,
            "(&mut Map<T, V>, T, V) -> Result<Option<V>, AllocError>",
        ),
        ("map_get", 2, "(&Map<T, V>, &T) -> Option<&V>"),
        ("map_remove", 2, "(&mut Map<T, V>, &T) -> Option<V>"),
        (
            "shared_new",
            1,
            "(&Allocator, T) -> Result<shared T, OwnershipError>",
        ),
        (
            "shared_clone",
            1,
            "(&shared T) -> Result<shared T, OwnershipError>",
        ),
        (
            "shared_read",
            1,
            "(&shared T) -> Result<&T, OwnershipError>",
        ),
        (
            "shared_replace",
            1,
            "(&mut shared T, T) -> Result<Unit, OwnershipError>",
        ),
        (
            "weak_new",
            1,
            "(&shared T) -> Result<weak T, OwnershipError>",
        ),
        (
            "weak_clone",
            1,
            "(&weak T) -> Result<weak T, OwnershipError>",
        ),
        (
            "weak_upgrade",
            1,
            "(&weak T) -> Result<shared T, OwnershipError>",
        ),
        (
            "arena_new",
            1,
            "(&mut arena-owner, T) -> Result<arena T, ArenaError>",
        ),
        (
            "arena_read",
            1,
            "(&arena-owner, arena T) -> Result<&T, ArenaError>",
        ),
        (
            "arena_replace",
            1,
            "(&mut arena-owner, arena T, T) -> Result<Unit, ArenaError>",
        ),
    ];
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
        } else if operation.name().starts_with("vec_") || operation.name().starts_with("arena_") {
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
