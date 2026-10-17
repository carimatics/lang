use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{
    allocation as a,
    buffer::{self, AllocError as A, BoundsError, BufferError as B, Bytes, Utf8Error},
    cleanup as c,
    layout::{self as l, ArenaError as R, Immediate as V, OwnershipError as O},
};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 0,
    }
}
fn table() -> o::TypeTable {
    o::TypeTable {
        sources: vec![m::Source {
            path: "values.lang".into(),
            text: String::new(),
        }],
        ..Default::default()
    }
}
fn construct(table: &mut o::TypeTable, constructor: o::Constructor) -> o::Type {
    let id = o::ConstructedId(table.constructed.len() as u32);
    table.constructed.push(o::Constructed {
        constructor,
        span: span(),
    });
    o::Type::Constructed(id)
}
fn builtin(table: &mut o::TypeTable, builtin: o::Builtin) -> o::Type {
    construct(table, o::Constructor::Builtin(builtin))
}
fn allocator() -> a::Allocator<a::System> {
    a::Allocator::new(
        a::System,
        a::Limits {
            live_allocations: 16,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn finite_error_cases_and_payloads_match_independent_vectors_without_allocation() {
    use o::Builtin as T;
    // Explicit vectors use the accepted source case order, not the codec's encoder.
    let cases = [
        (T::AllocError, V::Allocation(A::Limit), 0, None),
        (T::AllocError, V::Allocation(A::OutOfMemory), 1, None),
        (T::AllocError, V::Allocation(A::Overflow), 2, None),
        (T::BoundsError, V::Bounds(BoundsError::OutOfBounds), 0, None),
        (T::BufferError, V::Buffer(B::Bounds), 0, None),
        (T::BufferError, V::Buffer(B::InvalidByte), 1, None),
        (
            T::BufferError,
            V::Buffer(B::Allocation(A::Limit)),
            2,
            Some(V::Allocation(A::Limit)),
        ),
        (
            T::BufferError,
            V::Buffer(B::Allocation(A::OutOfMemory)),
            2,
            Some(V::Allocation(A::OutOfMemory)),
        ),
        (
            T::BufferError,
            V::Buffer(B::Allocation(A::Overflow)),
            2,
            Some(V::Allocation(A::Overflow)),
        ),
        (
            T::Utf8Error,
            V::Utf8(Utf8Error::Invalid(i64::MIN)),
            0,
            Some(V::I64(i64::MIN)),
        ),
        (
            T::Utf8Error,
            V::Utf8(Utf8Error::Invalid(i64::MAX)),
            0,
            Some(V::I64(i64::MAX)),
        ),
        (
            T::OwnershipError,
            V::Ownership(O::Allocation(A::Limit)),
            0,
            Some(V::Allocation(A::Limit)),
        ),
        (
            T::OwnershipError,
            V::Ownership(O::Allocation(A::OutOfMemory)),
            0,
            Some(V::Allocation(A::OutOfMemory)),
        ),
        (
            T::OwnershipError,
            V::Ownership(O::Allocation(A::Overflow)),
            0,
            Some(V::Allocation(A::Overflow)),
        ),
        (T::OwnershipError, V::Ownership(O::StrongCycle), 1, None),
        (T::OwnershipError, V::Ownership(O::ExpiredWeak), 2, None),
        (T::OwnershipError, V::Ownership(O::BorrowConflict), 3, None),
        (T::OwnershipError, V::Ownership(O::Unavailable), 4, None),
        (T::OwnershipError, V::Ownership(O::CountOverflow), 5, None),
        (
            T::ArenaError,
            V::Arena(R::Allocation(A::Limit)),
            0,
            Some(V::Allocation(A::Limit)),
        ),
        (
            T::ArenaError,
            V::Arena(R::Allocation(A::OutOfMemory)),
            0,
            Some(V::Allocation(A::OutOfMemory)),
        ),
        (
            T::ArenaError,
            V::Arena(R::Allocation(A::Overflow)),
            0,
            Some(V::Allocation(A::Overflow)),
        ),
        (T::ArenaError, V::Arena(R::CrossRegion), 1, None),
        (T::ArenaError, V::Arena(R::Unavailable), 2, None),
        (T::ArenaError, V::Arena(R::BorrowConflict), 3, None),
    ];
    let mut table = table();
    let types: Vec<_> = cases
        .iter()
        .map(|(t, ..)| builtin(&mut table, *t))
        .collect();
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    let mut allocator = allocator();
    for ((_, value, tag, payload), ty) in cases.into_iter().zip(types) {
        let mut cell = l::Reservation::allocate(&layouts, ty, &mut allocator).unwrap();
        let before = allocator.stats();
        cell.data_mut(&mut allocator).unwrap().fill(0xA5);
        cell.write_immediate(cell.root(), value, &mut allocator)
            .unwrap();
        assert_eq!(cell.read_immediate(cell.root(), &allocator), Ok(value));
        assert_eq!(cell.case(cell.root(), &allocator), Ok(tag));
        assert_eq!(value.case(), Some(tag));
        assert_eq!(value.payload(), payload);
        let mut expected = vec![0; layouts.get(ty).unwrap().size as usize];
        expected[..4].copy_from_slice(&tag.to_le_bytes());
        match payload {
            Some(V::Allocation(error)) => {
                let tag: u32 = match error {
                    A::Limit => 0,
                    A::OutOfMemory => 1,
                    A::Overflow => 2,
                };
                expected[4..8].copy_from_slice(&tag.to_le_bytes());
            }
            Some(V::I64(offset)) => expected[8..16].copy_from_slice(&offset.to_le_bytes()),
            None => {}
            _ => panic!("unexpected error payload"),
        }
        assert_eq!(cell.data(&allocator).unwrap(), expected);
        assert_eq!(allocator.stats(), before);
        cell.release_storage(&mut allocator).unwrap();
    }
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn scalars_and_zero_sized_unit_obey_exact_field_bounds_and_reject_wrong_types() {
    let mut table = table();
    let unit = builtin(&mut table, o::Builtin::Unit);
    table.definitions.push(o::Definition {
        name: "Tail".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Record(vec![
            o::Field {
                name: "flag".into(),
                span: span(),
                ty: o::TypeUse {
                    ty: o::Type::Bool,
                    span: span(),
                },
            },
            o::Field {
                name: "unit".into(),
                span: span(),
                ty: o::TypeUse {
                    ty: unit,
                    span: span(),
                },
            },
        ]),
    });
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    let mut allocator = allocator();
    let mut cell = l::Reservation::allocate(
        &layouts,
        o::Type::Nominal(m::DefinitionId(0)),
        &mut allocator,
    )
    .unwrap();
    let flag = cell.child(cell.root(), 0, &allocator).unwrap();
    let end = cell.child(cell.root(), 1, &allocator).unwrap();
    assert_eq!(end.offset(), cell.data(&allocator).unwrap().len() as u64);
    cell.write_immediate(flag, V::Bool(true), &mut allocator)
        .unwrap();
    cell.write_immediate(end, V::Unit, &mut allocator).unwrap();
    assert_eq!(cell.data(&allocator).unwrap(), [1]);
    assert_eq!(cell.read_immediate(end, &allocator), Ok(V::Unit));
    assert_eq!(cell.read_immediate(flag, &allocator), Ok(V::Bool(true)));
    assert_eq!(
        cell.write_immediate(flag, V::I64(7), &mut allocator),
        Err(l::Error::InvalidType(o::Type::Bool))
    );
    assert_eq!(cell.data(&allocator).unwrap(), [1]);
    for bad in 2..=u8::MAX {
        cell.data_mut(&mut allocator).unwrap()[0] = bad;
        assert_eq!(
            cell.read_immediate(flag, &allocator),
            Err(l::Error::InvalidValue)
        );
    }
    cell.release_storage(&mut allocator).unwrap();
    let mut scalar = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    for value in [i64::MIN, -1, 0, 1, i64::MAX] {
        scalar
            .write_immediate(scalar.root(), V::I64(value), &mut allocator)
            .unwrap();
        assert_eq!(scalar.data(&allocator).unwrap(), value.to_le_bytes());
        assert_eq!(
            scalar.read_immediate(scalar.root(), &allocator),
            Ok(V::I64(value))
        );
    }
    assert_eq!(
        scalar.read_immediate(flag, &allocator),
        Err(l::Error::ForeignLocation)
    );
    scalar.begin_retirement(None, &mut allocator).unwrap();
    assert_eq!(
        scalar.write_immediate(scalar.root(), V::I64(0), &mut allocator),
        Err(l::Error::State)
    );
    scalar.release_storage(&mut allocator).unwrap();
    assert_eq!(
        scalar.read_immediate(scalar.root(), &allocator),
        Err(l::Error::State)
    );
}

#[test]
fn nested_option_result_cases_clear_inactive_payload_and_preserve_invalid_writes() {
    let mut table = table();
    let error = builtin(&mut table, o::Builtin::BufferError);
    let utf8 = builtin(&mut table, o::Builtin::Utf8Error);
    let inner = construct(&mut table, o::Constructor::Result(o::Type::I64, error));
    let option = construct(&mut table, o::Constructor::Option(inner));
    let result = construct(&mut table, o::Constructor::Result(option, utf8));
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    let mut allocator = allocator();
    let mut cell = l::Reservation::allocate(&layouts, result, &mut allocator).unwrap();
    let before = allocator.stats();
    let root = cell.root();
    for n in 0..128 {
        cell.data_mut(&mut allocator).unwrap().fill(0xA5);
        cell.prepare_case(root, 0, &mut allocator).unwrap();
        assert!(cell.data(&allocator).unwrap().iter().all(|&b| b == 0));
        let option = cell.child(root, 0, &allocator).unwrap();
        cell.prepare_case(option, 1, &mut allocator).unwrap();
        let inner = cell.child(option, 0, &allocator).unwrap();
        let case = n % 2;
        cell.prepare_case(inner, case, &mut allocator).unwrap();
        let payload = cell.child(inner, 0, &allocator).unwrap();
        let value = if case == 0 {
            V::I64(i64::MIN + i64::from(n))
        } else {
            V::Buffer(B::Allocation(A::Overflow))
        };
        cell.write_immediate(payload, value, &mut allocator)
            .unwrap();
        assert_eq!(cell.read_immediate(payload, &allocator), Ok(value));
        assert_eq!(cell.case(root, &allocator), Ok(0));
        assert_eq!(cell.case(option, &allocator), Ok(1));
        assert_eq!(cell.case(inner, &allocator), Ok(case));
        let old = cell.data(&allocator).unwrap().to_vec();
        assert_eq!(
            cell.prepare_case(root, 2, &mut allocator),
            Err(l::Error::InvalidCase(2))
        );
        assert_eq!(cell.data(&allocator).unwrap(), old);
        cell.prepare_case(option, 0, &mut allocator).unwrap();
        assert_eq!(
            cell.child(option, 0, &allocator),
            Err(l::Error::InvalidField(0))
        );
        cell.prepare_case(root, 1, &mut allocator).unwrap();
        let payload = cell.child(root, 0, &allocator).unwrap();
        cell.write_immediate(
            payload,
            V::Utf8(Utf8Error::Invalid(i64::MAX)),
            &mut allocator,
        )
        .unwrap();
        assert_eq!(
            cell.read_immediate(payload, &allocator),
            Ok(V::Utf8(Utf8Error::Invalid(i64::MAX)))
        );
    }
    assert_eq!(allocator.stats(), before);
    cell.data_mut(&mut allocator).unwrap()[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        cell.case(root, &allocator),
        Err(l::Error::InvalidCase(u32::MAX))
    );
    assert_eq!(
        cell.child(root, 0, &allocator),
        Err(l::Error::InvalidCase(u32::MAX))
    );
    cell.release_storage(&mut allocator).unwrap();
}

#[test]
fn invalid_error_discriminants_and_nested_allocation_tags_are_rejected() {
    let mut table = table();
    let builtins = [
        o::Builtin::AllocError,
        o::Builtin::BoundsError,
        o::Builtin::BufferError,
        o::Builtin::Utf8Error,
        o::Builtin::OwnershipError,
        o::Builtin::ArenaError,
    ];
    let types: Vec<_> = builtins
        .into_iter()
        .map(|b| builtin(&mut table, b))
        .collect();
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    let mut allocator = allocator();
    for ((builtin, count), ty) in builtins.into_iter().zip([3, 1, 3, 1, 6, 4]).zip(types) {
        let mut cell = l::Reservation::allocate(&layouts, ty, &mut allocator).unwrap();
        for invalid in [count, 256, u32::MAX] {
            cell.data_mut(&mut allocator).unwrap()[..4].copy_from_slice(&invalid.to_le_bytes());
            assert_eq!(
                cell.read_immediate(cell.root(), &allocator),
                Err(l::Error::InvalidCase(invalid))
            );
            assert_eq!(
                cell.case(cell.root(), &allocator),
                Err(l::Error::InvalidCase(invalid))
            );
        }
        if matches!(
            builtin,
            o::Builtin::BufferError | o::Builtin::OwnershipError | o::Builtin::ArenaError
        ) {
            let tag: u32 = if builtin == o::Builtin::BufferError {
                2
            } else {
                0
            };
            for invalid in [3u32, 256, u32::MAX] {
                let bytes = cell.data_mut(&mut allocator).unwrap();
                bytes[..4].copy_from_slice(&tag.to_le_bytes());
                bytes[4..8].copy_from_slice(&invalid.to_le_bytes());
                assert_eq!(
                    cell.read_immediate(cell.root(), &allocator),
                    Err(l::Error::InvalidCase(invalid))
                );
            }
        }
        cell.release_storage(&mut allocator).unwrap();
    }
}

#[test]
fn owned_result_option_construction_requires_order_and_cleans_only_active_bytes() {
    let mut table = table();
    let bytes = builtin(&mut table, o::Builtin::Bytes);
    let error = builtin(&mut table, o::Builtin::AllocError);
    let option = construct(&mut table, o::Constructor::Option(bytes));
    let result = construct(&mut table, o::Constructor::Result(option, error));
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    for arm in 0..3 {
        let mut allocator = allocator();
        let mut cell = l::Reservation::allocate(&layouts, result, &mut allocator).unwrap();
        let root = cell.root();
        cell.prepare_case(root, u32::from(arm == 2), &mut allocator)
            .unwrap();
        let payload = cell.child(root, 0, &allocator).unwrap();
        if arm == 2 {
            cell.write_immediate(payload, V::Allocation(A::OutOfMemory), &mut allocator)
                .unwrap();
        } else {
            cell.prepare_case(payload, u32::from(arm == 0), &mut allocator)
                .unwrap();
            if arm == 0 {
                let leaf = cell.child(payload, 0, &allocator).unwrap();
                let mut bytes = Bytes::from_slice(&mut allocator, b"owned result").unwrap();
                let token = bytes.take_token(&allocator).unwrap();
                let start = leaf.offset() as usize;
                cell.data_mut(&mut allocator).unwrap()[start..start + 32].copy_from_slice(&token);
            }
            // Preparing a reused variant clears its previous publication state.
            assert_eq!(
                cell.begin_retirement(None, &mut allocator),
                Err(l::Error::Uninitialized)
            );
            let count = usize::from(arm == 0);
            cell.initialize_order(
                payload,
                (0..count).map(|i| (i as u32, span())),
                &mut allocator,
            )
            .unwrap();
        }
        cell.initialize_order(root, [(0, span())].into_iter(), &mut allocator)
            .unwrap();
        let cumulative = allocator.stats().cumulative_bytes;
        let mut calls = 0;
        let mut instance = c::Instance::default();
        let mut frame = instance
            .enter_with_context(
                c::Limits {
                    slots: 1,
                    ..Default::default()
                },
                |cell: &mut l::Reservation<'_, '_>, _: c::Reason, context: &mut c::Context<'_>| {
                    cell.cleanup(&mut allocator, context, |token, allocator, _, _| {
                        calls += 1;
                        buffer::cleanup_token(&verified, token, allocator).map_err(|_| 10)
                    })
                    .map_err(|_| 11)
                },
            )
            .unwrap();
        frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
        assert!(frame.initialize(c::Slot(0), cell, None).is_ok());
        let outcome = frame.finish(c::Exit::Return(None));
        assert!(outcome.report.primary().is_none());
        assert_eq!(calls, usize::from(arm == 0));
        assert_eq!(allocator.stats().cumulative_bytes, cumulative);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn nested_whole_transfers_preserve_order_spans_and_failure_cleanup_without_allocation() {
    let mut table = table();
    table.sources[0].text = "abc".into();
    let bytes = builtin(&mut table, o::Builtin::Bytes);
    let error = builtin(&mut table, o::Builtin::AllocError);
    let pair = o::Type::Nominal(m::DefinitionId(0));
    table.definitions.push(o::Definition {
        name: "Pair".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Record(
            ["a", "b"]
                .into_iter()
                .map(|name| o::Field {
                    name: name.into(),
                    span: span(),
                    ty: o::TypeUse {
                        ty: bytes,
                        span: span(),
                    },
                })
                .collect(),
        ),
    });
    let option = construct(&mut table, o::Constructor::Option(pair));
    let result = construct(&mut table, o::Constructor::Result(option, error));
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    for cleanup_error in [false, true] {
        let mut allocator = allocator();
        let mut leaf = l::Reservation::allocate(&layouts, pair, &mut allocator).unwrap();
        let mut optional = l::Reservation::allocate(&layouts, option, &mut allocator).unwrap();
        let mut outer = l::Reservation::allocate(&layouts, result, &mut allocator).unwrap();
        let mut wrong = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
        wrong
            .write_immediate(wrong.root(), V::I64(42), &mut allocator)
            .unwrap();
        for (field, data) in [(1, b"bb".as_slice()), (0, b"a".as_slice())] {
            let mut value = Bytes::from_slice(&mut allocator, data).unwrap();
            let token = value.take_token(&allocator).unwrap();
            let at = leaf.child(leaf.root(), field, &allocator).unwrap().offset() as usize;
            leaf.data_mut(&mut allocator).unwrap()[at..at + 32].copy_from_slice(&token);
        }
        leaf.initialize_order(
            leaf.root(),
            [
                (
                    1,
                    m::Span {
                        start: 2,
                        end: 3,
                        ..span()
                    },
                ),
                (
                    0,
                    m::Span {
                        start: 1,
                        end: 2,
                        ..span()
                    },
                ),
            ]
            .into_iter(),
            &mut allocator,
        )
        .unwrap();
        optional
            .prepare_case(optional.root(), 1, &mut allocator)
            .unwrap();
        optional
            .initialize_order(optional.root(), [(0, span())].into_iter(), &mut allocator)
            .unwrap();
        outer.prepare_case(outer.root(), 0, &mut allocator).unwrap();
        outer
            .initialize_order(outer.root(), [(0, span())].into_iter(), &mut allocator)
            .unwrap();
        let before = allocator.stats();
        let child = optional.child(optional.root(), 0, &allocator).unwrap();
        leaf.move_root_into(&optional, child, &mut allocator)
            .unwrap();
        let child = outer.child(outer.root(), 0, &allocator).unwrap();
        optional
            .move_root_into(&outer, child, &mut allocator)
            .unwrap();
        leaf.recycle(&mut allocator).unwrap();
        optional.recycle(&mut allocator).unwrap();
        assert_eq!(
            outer.extract_case_into(1, &optional, &mut allocator),
            Err(l::Error::InvalidCase(0))
        );
        assert!(matches!(
            outer.extract_case_into(0, &wrong, &mut allocator),
            Err(l::Error::InvalidType(_))
        ));
        assert_eq!(
            wrong.read_immediate(wrong.root(), &allocator),
            Ok(V::I64(42))
        );
        outer
            .extract_case_into(0, &optional, &mut allocator)
            .unwrap();
        optional
            .extract_case_into(1, &leaf, &mut allocator)
            .unwrap();
        assert_eq!(
            outer.extract_case_into(0, &optional, &mut allocator),
            Err(l::Error::State)
        );
        assert_eq!(allocator.stats(), before);
        outer.release_storage(&mut allocator).unwrap();
        optional.release_storage(&mut allocator).unwrap();
        wrong.release_storage(&mut allocator).unwrap();
        let mut order = Vec::new();
        let mut instance = c::Instance::default();
        let mut frame = instance
            .enter_with_context(
                c::Limits {
                    slots: 1,
                    ..Default::default()
                },
                |cell: &mut l::Reservation<'_, '_>, _, context: &mut c::Context<'_>| {
                    cell.cleanup(&mut allocator, context, |token, allocator, _, _| {
                        let mut snapshot = token.bytes;
                        let value =
                            Bytes::from_token(&mut snapshot, allocator).map_err(|_| 11u32)?;
                        order.push((
                            value.len(allocator).map_err(|_| 12u32)?,
                            token.leaf.span.unwrap().start,
                        ));
                        buffer::cleanup_token(&verified, token, allocator).map_err(|_| 13u32)?;
                        if cleanup_error { Err(77) } else { Ok(()) }
                    })
                    .map_err(|_| 14u32)
                },
            )
            .unwrap();
        frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
        assert!(frame.initialize(c::Slot(0), leaf, None).is_ok());
        let exit = if cleanup_error {
            c::Exit::Failure {
                failure: c::ExecutionFailure::Panic,
                span: Some(span()),
            }
        } else {
            c::Exit::Return(None)
        };
        let completion = frame.finish(exit);
        if cleanup_error {
            assert_eq!(
                completion.report.primary().unwrap().kind,
                c::FailureKind::Execution(c::ExecutionFailure::Panic)
            );
            assert_eq!(completion.report.secondary().len(), 2);
            assert!(
                completion
                    .report
                    .secondary()
                    .iter()
                    .all(|r| r.kind == c::FailureKind::Cleanup(77))
            );
        } else {
            assert!(completion.report.primary().is_none());
        }
        assert_eq!(order, [(1, 1), (2, 2)]);
        assert_eq!(instance.is_poisoned(), cleanup_error);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn invalid_construction_rejects_transfer_without_publishing_or_losing_the_payload() {
    let mut table = table();
    let bytes = builtin(&mut table, o::Builtin::Bytes);
    let option = construct(&mut table, o::Constructor::Option(bytes));
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    let mut allocator = allocator();
    let mut source = l::Reservation::allocate(&layouts, option, &mut allocator).unwrap();
    let destination = l::Reservation::allocate(&layouts, bytes, &mut allocator).unwrap();
    source
        .prepare_case(source.root(), 1, &mut allocator)
        .unwrap();
    let at = source.child(source.root(), 0, &allocator).unwrap().offset() as usize;
    let mut value = Bytes::from_slice(&mut allocator, b"retained").unwrap();
    let token = value.take_token(&allocator).unwrap();
    source.data_mut(&mut allocator).unwrap()[at..at + 32].copy_from_slice(&token);
    let original = source.data(&allocator).unwrap().to_vec();
    let before = allocator.stats();
    assert_eq!(
        source.extract_case_into(1, &destination, &mut allocator),
        Err(l::Error::Uninitialized)
    );
    assert_eq!(source.data(&allocator).unwrap(), original);
    assert!(
        destination
            .data(&allocator)
            .unwrap()
            .iter()
            .all(|b| *b == 0)
    );
    source
        .initialize_order(source.root(), [(0, span())].into_iter(), &mut allocator)
        .unwrap();
    source
        .extract_case_into(1, &destination, &mut allocator)
        .unwrap();
    assert_eq!(destination.data(&allocator).unwrap(), token);
    assert_eq!(allocator.stats(), before);
    source.release_storage(&mut allocator).unwrap();
    let mut instance = c::Instance::default();
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |cell: &mut l::Reservation<'_, '_>, _, context: &mut c::Context<'_>| {
                cell.cleanup(&mut allocator, context, |token, allocator, _, _| {
                    buffer::cleanup_token(&verified, token, allocator).map_err(|_| 1u32)
                })
                .map_err(|_| 2u32)
            },
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    assert!(frame.initialize(c::Slot(0), destination, None).is_ok());
    assert!(
        frame
            .finish(c::Exit::Return(None))
            .report
            .primary()
            .is_none()
    );
    assert_eq!(allocator.stats().live_allocations, 0);
}
