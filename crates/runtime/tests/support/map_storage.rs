use super::*;

#[test]
fn constructor_admits_all_read_rows_under_exact_header_quotas_and_refuses_overflow_before_callbacks()
 {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for slots in [0, 1, 256] {
        for short in [false, true] {
            let bytes = 168 + 16 * slots as u64;
            let (mut a, t) = allocator(a::Limits {
                request_bytes: bytes - u64::from(short),
                ..Default::default()
            });
            let result = map::Map::new_with_limits(
                &layouts,
                o::Type::I64,
                o::Type::I64,
                map::Limits {
                    loans: slots,
                    ..Default::default()
                },
                &mut a,
            );
            if short {
                assert!(matches!(
                    result,
                    Err(map::Error::Allocation(a::Error::Limit(
                        a::Limit::RequestBytes
                    )))
                ));
                assert!(t.borrow().requests.is_empty());
            } else {
                let mut map = result.unwrap();
                assert_eq!(a.stats().live_bytes, bytes);
                assert_eq!(t.borrow().requests.len(), 1);
                drain(&mut map, &mut a);
            }
            empty(&a);
        }
    }
    let (mut a, t) = allocator(Default::default());
    assert!(matches!(
        map::Map::new_with_limits(
            &layouts,
            o::Type::I64,
            o::Type::I64,
            map::Limits {
                loans: usize::MAX,
                ..Default::default()
            },
            &mut a
        ),
        Err(map::Error::Allocation(a::Error::Overflow))
    ));
    assert!(t.borrow().requests.is_empty());
    empty(&a);
}

#[test]
fn rejected_inputs_leave_storage_unchanged_and_value_views_cannot_project_into_another_entry() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, _) = allocator(Default::default());
    let mut map = map::Map::new(&layouts, o::Type::I64, ty(3), &mut a).unwrap();
    let mut key = number(&layouts, 1, &mut a);
    let mut uninitialized = l::Reservation::allocate(&layouts, ty(3), &mut a).unwrap();
    let before = a.stats();
    assert!(
        map.publish_if_absent(&mut key, &mut uninitialized, Default::default(), &mut a)
            .is_err()
    );
    assert_eq!(a.stats(), before);
    assert!(map.is_empty(&a).unwrap());
    uninitialized.release_storage(&mut a).unwrap();
    for n in 1..=2 {
        key.write_immediate(key.root(), l::Immediate::I64(n), &mut a)
            .unwrap();
        let mut value = pair(&layouts, n as u8, &mut a);
        map.publish_if_absent(&mut key, &mut value, Default::default(), &mut a)
            .unwrap();
        value.release_storage(&mut a).unwrap();
    }
    let e1 = map.lookup(map::Key::I64(1), &a).unwrap().unwrap();
    let e2 = map.lookup(map::Key::I64(2), &a).unwrap().unwrap();
    let v1 = map.view(e1, &a).unwrap();
    let v2 = map.view(e2, &a).unwrap();
    let record = v2.child(v2.root(), 0).unwrap();
    let leaf = v2.child(record, 0).unwrap();
    assert!(v1.child(record, 0).is_err());
    assert!(v1.read_token(leaf).is_err());
    let mut wrong = number(&layouts, 1, &mut a);
    let before = a.stats();
    assert!(
        map.publish_if_absent(&mut key, &mut wrong, Default::default(), &mut a)
            .is_err()
    );
    assert_eq!(a.stats(), before);
    wrong.release_storage(&mut a).unwrap();
    key.release_storage(&mut a).unwrap();
    drain(&mut map, &mut a);
    empty(&a);
}

#[test]
fn numeric_and_exact_utf8_lookup_match_independent_maps_across_growth_and_duplicates() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut map = map::Map::new(&layouts, ty(0), o::Type::I64, &mut a).unwrap();
    let mut expected = std::collections::BTreeMap::new();
    for (n, s) in [
        "",
        "a",
        "a\0",
        "a\0b",
        "é",
        "e\u{301}",
        "🦀",
        "abc",
        "ab",
        "A",
        "a",
        "é",
        "",
        "日本語",
        "日本",
        "\0",
    ]
    .into_iter()
    .enumerate()
    {
        let mut key = text(&layouts, s, &mut a);
        let original = key.data(&a).unwrap().to_vec();
        let mut value = number(&layouts, n as i64, &mut a);
        let before = t.borrow().requests.len();
        let result = map
            .publish_if_absent(
                &mut key,
                &mut value,
                map::Spans {
                    key: Some(span(3)),
                    value: Some(span(4)),
                },
                &mut a,
            )
            .unwrap();
        if let std::collections::btree_map::Entry::Vacant(entry) = expected.entry(s) {
            assert_eq!(result, map::Publication::Inserted);
            entry.insert(n as i64);
            key.release_storage(&mut a).unwrap();
        } else {
            assert!(matches!(result, map::Publication::Existing(_)));
            assert_eq!(key.data(&a).unwrap(), original);
            assert_eq!(
                value.read_immediate(value.root(), &a).unwrap(),
                l::Immediate::I64(n as i64)
            );
            assert_eq!(t.borrow().requests.len(), before);
            discard(key, &mut a);
        }
        value.release_storage(&mut a).unwrap();
        for (needle, want) in &expected {
            let query = text(&layouts, needle, &mut a);
            let before = t.borrow().requests.len();
            let entry = map
                .lookup(map.key_of(&query, query.root(), &a).unwrap(), &a)
                .unwrap()
                .unwrap();
            let view = map.view(entry, &a).unwrap();
            assert_eq!(
                view.read_immediate(view.root()).unwrap(),
                l::Immediate::I64(*want)
            );
            assert_eq!(t.borrow().requests.len(), before);
            discard(query, &mut a);
        }
        assert_eq!(map.len(&a).unwrap(), expected.len() as i64);
    }
    let absent = text(&layouts, "missing", &mut a);
    assert!(
        map.lookup(map.key_of(&absent, absent.root(), &a).unwrap(), &a)
            .unwrap()
            .is_none()
    );
    discard(absent, &mut a);
    drain(&mut map, &mut a);
    let mut map = map::Map::new(&layouts, o::Type::I64, o::Type::I64, &mut a).unwrap();
    let mut expected = std::collections::BTreeMap::new();
    for (i, n) in [
        i64::MIN,
        i64::MAX,
        0,
        -1,
        1,
        42,
        -42,
        0,
        i64::MIN,
        100,
        -100,
        9,
        8,
        7,
        6,
        5,
        4,
        3,
        2,
    ]
    .into_iter()
    .enumerate()
    {
        let mut k = number(&layouts, n, &mut a);
        let mut v = number(&layouts, i as i64, &mut a);
        let result = map
            .publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
            .unwrap();
        match expected.entry(n) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(i as i64);
                assert_eq!(result, map::Publication::Inserted);
            }
            _ => assert!(matches!(result, map::Publication::Existing(_))),
        };
        k.release_storage(&mut a).unwrap();
        v.release_storage(&mut a).unwrap();
        for (k, v) in &expected {
            let entry = map.lookup(map::Key::I64(*k), &a).unwrap().unwrap();
            let view = map.view(entry, &a).unwrap();
            assert_eq!(
                view.read_immediate(view.root()).unwrap(),
                l::Immediate::I64(*v)
            );
        }
    }
    assert!(map.lookup(map::Key::I64(123), &a).unwrap().is_none());
    drain(&mut map, &mut a);
    empty(&a);
}

#[test]
fn every_constructor_and_growth_oom_preserves_both_inputs_and_existing_owners_then_retries() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    t.borrow_mut().fail = Some(1);
    assert!(matches!(
        map::Map::new(&layouts, ty(0), ty(3), &mut a),
        Err(map::Error::Allocation(a::Error::OutOfMemory))
    ));
    empty(&a);
    for existing in [0, 1, 2, 4] {
        for position in 1..=3 {
            let (mut a, t) = allocator(Default::default());
            let mut map = map::Map::new(&layouts, ty(0), ty(3), &mut a).unwrap();
            for i in 0..existing {
                let mut k = text(&layouts, &i.to_string(), &mut a);
                let mut v = pair(&layouts, i as u8, &mut a);
                map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
                    .unwrap();
                k.release_storage(&mut a).unwrap();
                v.release_storage(&mut a).unwrap();
            }
            let mut k = text(&layouts, "next", &mut a);
            let mut v = pair(&layouts, 20, &mut a);
            let original_k = k.data(&a).unwrap().to_vec();
            let original_v = v.data(&a).unwrap().to_vec();
            let before = a.stats();
            let calls = t.borrow().requests.len();
            t.borrow_mut().fail = Some(calls + position);
            assert!(matches!(
                map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a),
                Err(map::Error::Allocation(a::Error::OutOfMemory))
            ));
            assert_eq!(a.stats().live_allocations, before.live_allocations);
            assert_eq!(a.stats().live_bytes, before.live_bytes);
            assert_eq!(map.len(&a).unwrap(), existing);
            assert_eq!(k.data(&a).unwrap(), original_k);
            assert_eq!(v.data(&a).unwrap(), original_v);
            k.validate_retirement(&mut a).unwrap();
            v.validate_retirement(&mut a).unwrap();
            for i in 0..existing {
                let query = text(&layouts, &i.to_string(), &mut a);
                let e = map
                    .lookup(map.key_of(&query, query.root(), &a).unwrap(), &a)
                    .unwrap()
                    .unwrap();
                let view = map.view(e, &a).unwrap();
                let record = view.child(view.root(), 0).unwrap();
                let mut token = view.read_token(view.child(record, 0).unwrap()).unwrap();
                let b = buffer::Bytes::from_token(&mut token.bytes, &a).unwrap();
                assert_eq!(b.as_slice(&a).unwrap(), [i as u8]);
                discard(query, &mut a);
            }
            t.borrow_mut().fail = None;
            assert_eq!(
                map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
                    .unwrap(),
                map::Publication::Inserted
            );
            k.release_storage(&mut a).unwrap();
            v.release_storage(&mut a).unwrap();
            drain(&mut map, &mut a);
            empty(&a);
        }
    }
}

fn budget(limits: a::Limits, count: i64, expected: Option<a::Limit>) -> (a::Stats, u64) {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(limits);
    let mut k = number(&layouts, 0, &mut a);
    let mut v = number(&layouts, 9, &mut a);
    let mut map = map::Map::new_with_limits(
        &layouts,
        o::Type::I64,
        o::Type::I64,
        map::Limits {
            loans: 0,
            ..Default::default()
        },
        &mut a,
    )
    .unwrap();
    for n in 0..count {
        k.write_immediate(k.root(), l::Immediate::I64(n), &mut a)
            .unwrap();
        map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
            .unwrap();
    }
    k.write_immediate(k.root(), l::Immediate::I64(count), &mut a)
        .unwrap();
    let before = a.stats();
    let calls = t.borrow().requests.len();
    let outcome = map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a);
    if let Some(limit) = expected {
        assert_eq!(outcome, Err(map::Error::Allocation(a::Error::Limit(limit))));
        assert_eq!(a.stats(), before);
        assert_eq!(t.borrow().requests.len(), calls);
        assert_eq!(map.len(&a).unwrap(), count);
    } else {
        assert_eq!(outcome.unwrap(), map::Publication::Inserted);
    }
    let stats = a.stats();
    let maximum = t.borrow().requests.iter().map(|r| r.bytes()).max().unwrap();
    k.release_storage(&mut a).unwrap();
    v.release_storage(&mut a).unwrap();
    drain(&mut map, &mut a);
    empty(&a);
    (stats, maximum)
}
#[test]
fn whole_growth_plan_enforces_exact_and_one_below_quotas_before_any_provider_callback() {
    let (baseline, request) = budget(Default::default(), 16, None);
    let (first, _) = budget(Default::default(), 0, None);
    for which in [
        a::Limit::RequestBytes,
        a::Limit::LiveBytes,
        a::Limit::CumulativeBytes,
        a::Limit::LiveAllocations,
    ] {
        for short in [false, true] {
            let mut limits = a::Limits::default();
            let delta = u64::from(short);
            let count = match which {
                a::Limit::RequestBytes => {
                    limits.request_bytes = request - delta;
                    16
                }
                a::Limit::LiveBytes => {
                    limits.live_bytes = baseline.peak_live_bytes - delta;
                    16
                }
                a::Limit::CumulativeBytes => {
                    limits.cumulative_bytes = baseline.cumulative_bytes - delta;
                    16
                }
                a::Limit::LiveAllocations => {
                    limits.live_allocations = first.peak_live_allocations - usize::from(short);
                    0
                }
                _ => unreachable!(),
            };
            budget(limits, count, short.then_some(which));
        }
    }
}

#[test]
fn reverse_entry_value_then_key_cleanup_preserves_field_spans_and_drains_after_failure_or_panic() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for fault in [0, 1, 2] {
        let (mut a, t) = allocator(Default::default());
        let mut map = map::Map::new(&layouts, ty(0), ty(3), &mut a).unwrap();
        for i in 0..5 {
            let mut k = text(&layouts, &(char::from(b'a' + i)).to_string(), &mut a);
            let mut v = pair(&layouts, 10 + i * 10, &mut a);
            map.publish_if_absent(
                &mut k,
                &mut v,
                map::Spans {
                    key: Some(span(3)),
                    value: Some(span(4)),
                },
                &mut a,
            )
            .unwrap();
            k.release_storage(&mut a).unwrap();
            v.release_storage(&mut a).unwrap();
        }
        let calls = t.borrow().requests.len();
        let mut seen = Vec::new();
        let report = context(|ctx| {
            map.cleanup(&mut a, ctx, |token, a, reason, ctx| {
                let mut observed_token = token.bytes;
                let observed = if token.leaf.ty == ty(0) {
                    let b = buffer::Text::from_token(&mut observed_token, a).unwrap();
                    b.as_str(a).unwrap().as_bytes()[0]
                } else {
                    let b = buffer::Bytes::from_token(&mut observed_token, a).unwrap();
                    b.as_slice(a).unwrap()[0]
                };
                seen.push((observed, token.leaf.span));
                clean(token, a, reason, ctx)?;
                if seen.len() == 1 {
                    if fault == 1 {
                        return Err(17);
                    }
                    if fault == 2 {
                        panic!("map leaf fault");
                    }
                }
                Ok(())
            })
            .map_err(|_| 99)
        });
        assert_eq!(report.primary().is_some(), fault != 0);
        let expected = (0..5)
            .rev()
            .flat_map(|i| {
                [
                    (10 + i * 10, Some(span(0))),
                    (11 + i * 10, Some(span(1))),
                    (b'a' + i, Some(span(3))),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(seen, expected);
        assert_eq!(t.borrow().requests.len(), calls);
        empty(&a);
    }
}

#[test]
fn registered_reads_alias_copy_and_end_without_allocating_and_block_mutation_or_normal_cleanup() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    for slots in [0, 1, 2] {
        let mut map = map::Map::new_with_limits(
            &layouts,
            o::Type::I64,
            o::Type::I64,
            map::Limits {
                loans: slots,
                ..Default::default()
            },
            &mut a,
        )
        .unwrap();
        let mut k = number(&layouts, 1, &mut a);
        let mut v = number(&layouts, 9, &mut a);
        map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
            .unwrap();
        let old = map.lookup(map::Key::I64(1), &a).unwrap().unwrap();
        let calls = t.borrow().requests.len();
        assert!(map.borrow(map::Key::I64(2), &mut a).unwrap().is_none());
        if slots == 0 {
            assert!(matches!(
                map.borrow(map::Key::I64(1), &mut a),
                Err(map::Error::LoanLimit)
            ));
        } else {
            let mut loans = (0..slots)
                .map(|_| map.borrow(map::Key::I64(1), &mut a).unwrap().unwrap())
                .collect::<Vec<_>>();
            assert!(matches!(
                map.borrow(map::Key::I64(1), &mut a),
                Err(map::Error::LoanLimit)
            ));
            let view = loans[0].view(&layouts, &a).unwrap();
            assert_eq!(
                view.read_immediate(view.root(), &a).unwrap(),
                l::Immediate::I64(9)
            );
            view.copy_into(view.root(), &v, v.root(), &mut a).unwrap();
            k.write_immediate(k.root(), l::Immediate::I64(2), &mut a)
                .unwrap();
            assert_eq!(
                map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a),
                Err(map::Error::BorrowConflict)
            );
            assert_eq!(map.take_token(&a), Err(map::Error::BorrowConflict));
            assert!(
                context(|ctx| {
                    assert_eq!(
                        map.cleanup(&mut a, ctx, clean),
                        Err(map::Error::BorrowConflict)
                    );
                    Ok(())
                })
                .primary()
                .is_none()
            );
            for loan in &mut loans {
                loan.end(&layouts, &mut a).unwrap();
                assert_eq!(loan.end(&layouts, &mut a), Err(map::Error::Unavailable));
                assert!(loan.view(&layouts, &a).is_err());
            }
        }
        assert_eq!(t.borrow().requests.len(), calls);
        k.write_immediate(k.root(), l::Immediate::I64(2), &mut a)
            .unwrap();
        map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
            .unwrap();
        assert!(map.view(old, &a).is_err());
        k.release_storage(&mut a).unwrap();
        v.release_storage(&mut a).unwrap();
        let mut token = map.take_token(&a).unwrap();
        assert!(map.len(&a).is_err());
        let mut map =
            map::Map::from_token(&layouts, o::Type::I64, o::Type::I64, &mut token, &a).unwrap();
        assert_eq!(token, [0; 32]);
        assert_eq!(map.len(&a).unwrap(), 2);
        drain(&mut map, &mut a);
        empty(&a);
    }
}

#[test]
fn abort_invalidates_abandoned_views_after_storage_reuse_and_cannot_end_a_new_registration() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut map = map::Map::new(&layouts, o::Type::I64, ty(3), &mut a).unwrap();
    let mut k = number(&layouts, 1, &mut a);
    let mut v = pair(&layouts, 10, &mut a);
    map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
        .unwrap();
    k.release_storage(&mut a).unwrap();
    v.release_storage(&mut a).unwrap();
    let mut abandoned = map.borrow(map::Key::I64(1), &mut a).unwrap().unwrap();
    let view = abandoned.view(&layouts, &a).unwrap();
    let root = view.root();
    let record = view.child(root, 0, &a).unwrap();
    let leaf = view.child(record, 0, &a).unwrap();
    let calls = t.borrow().requests.len();
    assert!(
        context(|ctx| map.abort_cleanup(&mut a, ctx, clean).map_err(|_| 99))
            .primary()
            .is_none()
    );
    assert_eq!(t.borrow().requests.len(), calls);
    empty(&a);
    assert!(view.case(root, &a).is_err());
    assert!(view.child(record, 0, &a).is_err());
    assert!(view.read_token(leaf, &a).is_err());
    let mut fresh = map::Map::new(&layouts, o::Type::I64, o::Type::I64, &mut a).unwrap();
    let mut k = number(&layouts, 1, &mut a);
    let mut v = number(&layouts, 77, &mut a);
    fresh
        .publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
        .unwrap();
    let mut live = fresh.borrow(map::Key::I64(1), &mut a).unwrap().unwrap();
    assert!(view.case(root, &a).is_err());
    assert!(abandoned.end(&layouts, &mut a).is_err());
    assert!(live.view(&layouts, &a).is_ok());
    live.end(&layouts, &mut a).unwrap();
    k.release_storage(&mut a).unwrap();
    v.release_storage(&mut a).unwrap();
    drain(&mut fresh, &mut a);
    empty(&a);
}

#[test]
fn work_budget_bounds_utf8_validation_and_comparison_without_allocating_or_retaining_queries() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    for work in [1, 2, 6, 7] {
        let mut map = map::Map::new_with_limits(
            &layouts,
            ty(0),
            o::Type::I64,
            map::Limits {
                lookup_work: work,
                ..Default::default()
            },
            &mut a,
        )
        .unwrap();
        let mut key = text(&layouts, "é", &mut a);
        let mut value = number(&layouts, 9, &mut a);
        if work < 2 {
            assert_eq!(
                map.publish_if_absent(&mut key, &mut value, Default::default(), &mut a),
                Err(map::Error::WorkLimit)
            );
            discard(key, &mut a);
        } else {
            map.publish_if_absent(&mut key, &mut value, Default::default(), &mut a)
                .unwrap();
            key.release_storage(&mut a).unwrap();
            let query = text(&layouts, "é", &mut a);
            let calls = t.borrow().requests.len();
            let result = map.lookup(map.key_of(&query, query.root(), &a).unwrap(), &a);
            if work < 7 {
                assert_eq!(result, Err(map::Error::WorkLimit));
            } else {
                assert!(result.unwrap().is_some());
            }
            assert_eq!(t.borrow().requests.len(), calls);
            discard(query, &mut a);
        }
        value.release_storage(&mut a).unwrap();
        drain(&mut map, &mut a);
        empty(&a);
    }
}

#[test]
fn invalid_types_spans_foreign_layouts_and_zero_sized_values_have_checked_boundaries() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let foreign_types = model();
    let foreign_layouts = l::Layouts::compile(&foreign_types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let before = t.borrow().requests.len();
    for key in [o::Type::Bool, ty(1), ty(2)] {
        assert!(matches!(
            map::Map::new(&layouts, key, o::Type::I64, &mut a),
            Err(map::Error::WrongKey)
        ));
    }
    for value in [ty(4), ty(5)] {
        assert!(map::Map::new(&layouts, o::Type::I64, value, &mut a).is_err());
    }
    assert_eq!(t.borrow().requests.len(), before);
    let mut map = map::Map::new(&layouts, o::Type::I64, ty(2), &mut a).unwrap();
    let mut k = number(&layouts, 0, &mut a);
    let mut value = l::Reservation::allocate(&layouts, ty(2), &mut a).unwrap();
    let query = number(&foreign_layouts, 0, &mut a);
    assert_eq!(
        map.key_of(&query, query.root(), &a),
        Err(map::Error::WrongKey)
    );
    let stats = a.stats();
    assert_eq!(
        map.publish_if_absent(
            &mut k,
            &mut value,
            map::Spans {
                key: Some(span(100)),
                value: None
            },
            &mut a
        ),
        Err(map::Error::Layout(l::Error::InvalidSpan))
    );
    assert_eq!(a.stats(), stats);
    assert_eq!(
        map.lookup(map::Key::Text([0; 32]), &a),
        Err(map::Error::WrongKey)
    );
    for n in 0..5 {
        k.write_immediate(k.root(), l::Immediate::I64(n), &mut a)
            .unwrap();
        map.publish_if_absent(&mut k, &mut value, Default::default(), &mut a)
            .unwrap();
        assert!(map.lookup(map::Key::I64(n), &a).unwrap().is_some());
    }
    k.release_storage(&mut a).unwrap();
    value.release_storage(&mut a).unwrap();
    discard(query, &mut a);
    let mut token = map.take_token(&a).unwrap();
    let saved = token;
    assert!(map::Map::from_token(&foreign_layouts, o::Type::I64, ty(2), &mut token, &a).is_err());
    assert_eq!(token, saved);
    let mut map = map::Map::from_token(&layouts, o::Type::I64, ty(2), &mut token, &a).unwrap();
    drain(&mut map, &mut a);
    empty(&a);
}
