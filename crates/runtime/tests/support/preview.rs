use super::*;

#[test]
fn read_only_preview_preserves_owner_across_limits_and_rejects_corrupt_tokens() {
    let mut table = table();
    let bytes = builtin(&mut table, o::Builtin::Bytes);
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    let mut allocator = allocator();
    let mut cell = l::Reservation::allocate(&layouts, bytes, &mut allocator).unwrap();
    let mut original = Bytes::from_slice(&mut allocator, &[0, 255]).unwrap();
    let token = original.take_token(&allocator).unwrap();
    cell.data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&token);
    let before = allocator.stats();
    let expected = "core::Bytes [0, 255]";
    for limit in [0, expected.len() - 1, expected.len()] {
        let preview =
            l::Preview::prepare(&layouts, bytes, limit, Default::default(), layouts.usage())
                .unwrap();
        let result = preview.render(&cell, &allocator);
        if limit == expected.len() {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert_eq!(result, Err(l::PreviewError::Limit));
        }
        assert_eq!(allocator.stats(), before);
        assert_eq!(cell.data(&allocator).unwrap(), token);
    }
    cell.data_mut(&mut allocator).unwrap().fill(0xFF);
    let preview =
        l::Preview::prepare(&layouts, bytes, 64, Default::default(), layouts.usage()).unwrap();
    assert!(matches!(
        preview.render(&cell, &allocator),
        Err(l::PreviewError::Buffer(_))
    ));
    cell.data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&token);
    let mut token = token;
    let mut restored = Bytes::from_token(&mut token, &allocator).unwrap();
    restored.release(&mut allocator).unwrap();
    cell.data_mut(&mut allocator).unwrap().fill(0);
    cell.release_storage(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
}

#[test]
fn deep_zero_sized_copy_records_use_the_bounded_iterative_stack() {
    let mut table = table();
    let unit = builtin(&mut table, o::Builtin::Unit);
    let mut root = unit;
    let depth = 1024;
    for index in 0..depth {
        table.definitions.push(o::Definition {
            name: format!("R{index}"),
            public: false,
            span: span(),
            kind: o::DefinitionKind::Record(vec![o::Field {
                name: "value".into(),
                span: span(),
                ty: o::TypeUse {
                    ty: root,
                    span: span(),
                },
            }]),
        });
        root = o::Type::Nominal(m::DefinitionId(index));
    }
    let limits = m::Limits {
        depth: 4096,
        ..Default::default()
    };
    let verified = table
        .verify(o::Limits {
            unit: limits,
            ..Default::default()
        })
        .unwrap();
    let layouts = l::Layouts::compile(&verified, limits).unwrap();
    assert_eq!(layouts.get(root).unwrap().size, 0);
    let mut allocator = allocator();
    let mut cell = l::Reservation::allocate(&layouts, root, &mut allocator).unwrap();
    let expected = (0..depth)
        .rev()
        .map(|n| format!("R{n} {{ value: "))
        .collect::<String>()
        + "core::Unit {}"
        + &" }".repeat(depth as usize);
    for cap in [expected.len() - 1, expected.len()] {
        let preview = l::Preview::prepare(&layouts, root, cap, limits, layouts.usage()).unwrap();
        let result = preview.render(&cell, &allocator);
        if cap == expected.len() {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert_eq!(result, Err(l::PreviewError::Limit));
        }
    }
    cell.release_storage(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn preparation_obeys_cumulative_work_storage_and_inactive_payload_boundaries() {
    let mut table = table();
    let forbidden = construct(&mut table, o::Constructor::Vec(o::Type::I64));
    let inactive = construct(&mut table, o::Constructor::Option(forbidden));
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    let base = layouts.usage();
    let preview =
        l::Preview::prepare(&layouts, o::Type::Bool, 64, Default::default(), base).unwrap();
    let used = preview.usage();
    let exact = m::Limits {
        work: used.work,
        allocations: used.allocation_bytes,
        elements: used.elements,
        ..Default::default()
    };
    assert!(l::Preview::prepare(&layouts, o::Type::Bool, 64, exact, base).is_ok());
    for limits in [
        m::Limits {
            work: exact.work - 1,
            ..exact
        },
        m::Limits {
            allocations: exact.allocations - 1,
            ..exact
        },
        m::Limits {
            elements: exact.elements - 1,
            ..exact
        },
    ] {
        assert!(l::Preview::prepare(&layouts, o::Type::Bool, 64, limits, base).is_err());
    }
    assert!(matches!(
        l::Preview::prepare(&layouts, o::Type::Bool, 64, exact, Default::default()),
        Err(l::Error::State)
    ));
    assert!(
        matches!(l::Preview::prepare(&layouts, inactive, 64, Default::default(), base), Err(l::Error::InvalidType(t)) if t == forbidden)
    );
    let mut allocator = allocator();
    let mut cell = l::Reservation::allocate(&layouts, o::Type::Bool, &mut allocator).unwrap();
    cell.data_mut(&mut allocator).unwrap()[0] = 2;
    assert!(matches!(
        preview.render(&cell, &allocator),
        Err(l::PreviewError::Value(l::Error::InvalidValue))
    ));
    cell.release_storage(&mut allocator).unwrap();
    assert_eq!(
        V::from_case(o::Builtin::Utf8Error, 0, None),
        Err(l::Error::InvalidValue)
    );
    assert_eq!(
        V::from_case(o::Builtin::BufferError, 2, Some(V::I64(0))),
        Err(l::Error::InvalidValue)
    );
    assert_eq!(
        V::from_case(o::Builtin::AllocError, 3, None),
        Err(l::Error::InvalidCase(3))
    );
}
