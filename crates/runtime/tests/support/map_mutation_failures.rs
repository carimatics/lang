use super::*;

fn retire(
    value: &mut l::Reservation<'_, '_>,
    span: Option<m::Span>,
    a: &mut a::Allocator<Provider>,
    ctx: &mut c::Context<'_>,
    mut cleaner: impl FnMut(
        &mut l::Token,
        &mut a::Allocator<Provider>,
        c::Reason,
        &mut c::Context<'_>,
    ) -> Result<(), u32>,
) -> Result<(), u32> {
    value.begin_retirement(span, a).unwrap();
    while let Some(leaf) = value.next_leaf(a).unwrap() {
        let start = leaf.offset as usize;
        let token = l::Token {
            leaf,
            bytes: value.data(a).unwrap()[start..start + 32]
                .try_into()
                .unwrap(),
        };
        ctx.field(
            token,
            leaf.span,
            &mut c::Contextual::new(|token: &mut l::Token, reason, ctx: &mut c::Context<'_>| {
                cleaner(token, a, reason, ctx)
            }),
        );
        value.complete_leaf().unwrap();
    }
    value.release_storage(a).unwrap();
    Ok(())
}

#[test]
fn every_insert_growth_oom_preserves_output_and_arguments_for_retry_or_one_common_context_cleanup()
{
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for existing in [false, true] {
        for position in 1..=3 {
            for retry in [false, true] {
                let (mut a, t) = allocator(Default::default());
                let mut map = map::Map::new(&layouts, ty(0), ty(3), &mut a).unwrap();
                let mut previous = None;
                if existing {
                    let mut k = text(&layouts, "old", &mut a);
                    let observation = map.key_of(&k, k.root(), &a).unwrap();
                    let mut v = pair(&layouts, 10, &mut a);
                    map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
                        .unwrap();
                    previous = map.lookup(observation, &a).unwrap();
                    k.release_storage(&mut a).unwrap();
                    v.release_storage(&mut a).unwrap();
                }
                let mut k = text(&layouts, "next", &mut a);
                let mut v = pair(&layouts, 20, &mut a);
                let mut output = l::Reservation::allocate(&layouts, ty(3), &mut a).unwrap();
                let key_before = k.data(&a).unwrap().to_vec();
                let value_before = v.data(&a).unwrap().to_vec();
                let output_before = output.data(&a).unwrap().to_vec();
                let stats = a.stats();
                let calls = t.borrow().requests.len();
                t.borrow_mut().fail = Some(calls + position);
                assert!(matches!(
                    map.insert_into(
                        &mut k,
                        &mut v,
                        map::Spans {
                            key: Some(span(3)),
                            value: Some(span(4))
                        },
                        &output,
                        output.root(),
                        &mut a
                    ),
                    Err(map::Error::Allocation(a::Error::OutOfMemory))
                ));
                assert_eq!(k.data(&a).unwrap(), key_before);
                assert_eq!(v.data(&a).unwrap(), value_before);
                assert_eq!(output.data(&a).unwrap(), output_before);
                assert_eq!(a.stats().live_bytes, stats.live_bytes);
                assert_eq!(a.stats().live_allocations, stats.live_allocations);
                assert_eq!(map.len(&a).unwrap(), i64::from(existing));
                if let Some(entry) = previous {
                    assert!(map.view(entry, &a).is_ok());
                }
                t.borrow_mut().fail = None;
                if retry {
                    assert!(
                        map.insert_into(
                            &mut k,
                            &mut v,
                            map::Spans {
                                key: Some(span(3)),
                                value: Some(span(4))
                            },
                            &output,
                            output.root(),
                            &mut a
                        )
                        .unwrap()
                        .is_none()
                    );
                    k.release_storage(&mut a).unwrap();
                    v.release_storage(&mut a).unwrap();
                } else {
                    let calls = t.borrow().requests.len();
                    let mut seen = Vec::new();
                    let report = context(|ctx| {
                        retire(
                            &mut v,
                            Some(span(4)),
                            &mut a,
                            ctx,
                            |token, a, reason, ctx| {
                                seen.push(token.leaf.span);
                                clean(token, a, reason, ctx)
                            },
                        )?;
                        retire(
                            &mut k,
                            Some(span(3)),
                            &mut a,
                            ctx,
                            |token, a, reason, ctx| {
                                seen.push(token.leaf.span);
                                clean(token, a, reason, ctx)
                            },
                        )
                    });
                    assert!(report.primary().is_none());
                    assert_eq!(seen, [Some(span(0)), Some(span(1)), Some(span(3))]);
                    assert_eq!(t.borrow().requests.len(), calls);
                }
                output.release_storage(&mut a).unwrap();
                drain(&mut map, &mut a);
                empty(&a);
            }
        }
    }
}

#[test]
fn live_reads_bad_output_type_location_and_spans_refuse_before_any_ownership_or_version_change() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut map = map::Map::new(&layouts, o::Type::I64, ty(0), &mut a).unwrap();
    let mut key = number(&layouts, 1, &mut a);
    let mut value = text(&layouts, "old", &mut a);
    let output = l::Reservation::allocate(&layouts, ty(0), &mut a).unwrap();
    let spans = map::Spans {
        key: Some(span(0)),
        value: Some(span(1)),
    };
    map.insert_into(&mut key, &mut value, spans, &output, output.root(), &mut a)
        .unwrap();
    value.release_storage(&mut a).unwrap();
    let mut value = text(&layouts, "new", &mut a);
    let wrong_type = number(&layouts, 0, &mut a);
    let other = l::Reservation::allocate(&layouts, ty(0), &mut a).unwrap();
    let entry = map.lookup(map::Key::I64(1), &a).unwrap().unwrap();
    let original = value.data(&a).unwrap().to_vec();
    let stats = a.stats();
    let calls = t.borrow().requests.len();
    for (destination, target) in [(&wrong_type, wrong_type.root()), (&output, other.root())] {
        assert!(
            map.insert_into(&mut key, &mut value, spans, destination, target, &mut a)
                .is_err()
        );
        assert!(
            map.remove_into(map::Key::I64(1), destination, target, &mut a)
                .is_err()
        );
    }
    for bad in [
        map::Spans {
            value: None,
            ..spans
        },
        map::Spans {
            value: Some(span(100)),
            ..spans
        },
        map::Spans {
            key: Some(span(100)),
            ..spans
        },
    ] {
        assert!(
            map.insert_into(&mut key, &mut value, bad, &output, output.root(), &mut a)
                .is_err()
        );
    }
    assert_eq!(a.stats(), stats);
    assert_eq!(t.borrow().requests.len(), calls);
    assert_eq!(value.data(&a).unwrap(), original);
    assert!(output.data(&a).unwrap().iter().all(|b| *b == 0));
    assert!(map.view(entry, &a).is_ok());
    let mut read = map.borrow(map::Key::I64(1), &mut a).unwrap().unwrap();
    assert!(matches!(
        map.insert_into(&mut key, &mut value, spans, &output, output.root(), &mut a),
        Err(map::Error::BorrowConflict)
    ));
    assert!(matches!(
        map.remove_into(map::Key::I64(1), &output, output.root(), &mut a),
        Err(map::Error::BorrowConflict)
    ));
    assert_eq!(a.stats(), stats);
    let view = read.view(&layouts, &a).unwrap();
    let mut token = view.read_token(view.root(), &a).unwrap();
    assert_eq!(
        buffer::Text::from_token(&mut token.bytes, &a)
            .unwrap()
            .as_str(&a)
            .unwrap(),
        "old"
    );
    read.end(&layouts, &mut a).unwrap();
    let replaced = map
        .insert_into(&mut key, &mut value, spans, &output, output.root(), &mut a)
        .unwrap()
        .unwrap();
    assert!(replaced.key.is_none());
    assert!(map.view(entry, &a).is_err());
    let current = map.lookup(map::Key::I64(1), &a).unwrap().unwrap();
    assert!(
        map.remove_into(map::Key::I64(2), &output, output.root(), &mut a)
            .unwrap()
            .is_none()
    );
    assert!(map.view(current, &a).is_ok());
    discard(output, &mut a);
    key.release_storage(&mut a).unwrap();
    value.release_storage(&mut a).unwrap();
    let mut wrong_type = wrong_type;
    wrong_type.release_storage(&mut a).unwrap();
    let mut other = other;
    other.release_storage(&mut a).unwrap();
    drain(&mut map, &mut a);
    empty(&a);
}

#[test]
fn projected_option_outputs_receive_owning_values_with_original_spans() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut map = map::Map::new(&layouts, ty(0), ty(0), &mut a).unwrap();
    let mut key = text(&layouts, "same", &mut a);
    let original = key.read_token(key.root(), &a).unwrap().bytes;
    let mut value = text(&layouts, "old", &mut a);
    let output = l::Reservation::allocate(&layouts, ty(7), &mut a).unwrap();
    output.prepare_case(output.root(), 1, &mut a).unwrap();
    let target = output.child(output.root(), 0, &a).unwrap();
    let spans = map::Spans {
        key: Some(span(3)),
        value: Some(span(4)),
    };
    assert!(
        map.insert_into(&mut key, &mut value, spans, &output, target, &mut a)
            .unwrap()
            .is_none()
    );
    key.release_storage(&mut a).unwrap();
    value.release_storage(&mut a).unwrap();
    let mut key = text(&layouts, "same", &mut a);
    let mut value = text(&layouts, "new", &mut a);
    let calls = t.borrow().requests.len();
    let replaced = map
        .insert_into(
            &mut key,
            &mut value,
            map::Spans {
                key: Some(span(5)),
                value: Some(span(6)),
            },
            &output,
            target,
            &mut a,
        )
        .unwrap()
        .unwrap();
    let mut token = output.read_token(target, &a).unwrap();
    assert_eq!(
        buffer::Text::from_token(&mut token.bytes, &a)
            .unwrap()
            .as_str(&a)
            .unwrap(),
        "old"
    );
    output
        .initialize_order(
            output.root(),
            [(0, replaced.value_span.unwrap())].into_iter(),
            &mut a,
        )
        .unwrap();
    super::mutation::consume_key(replaced, &mut a);
    key.release_storage(&mut a).unwrap();
    value.release_storage(&mut a).unwrap();
    assert_eq!(t.borrow().requests.len(), calls);
    discard(output, &mut a);
    let query = text(&layouts, "same", &mut a);
    let output = l::Reservation::allocate(&layouts, ty(7), &mut a).unwrap();
    output.prepare_case(output.root(), 1, &mut a).unwrap();
    let target = output.child(output.root(), 0, &a).unwrap();
    let removed = map
        .remove_into(
            map.key_of(&query, query.root(), &a).unwrap(),
            &output,
            target,
            &mut a,
        )
        .unwrap()
        .unwrap();
    assert_eq!(removed.key.as_ref().unwrap().bytes, original);
    assert_eq!(removed.value_span, Some(span(6)));
    output
        .initialize_order(
            output.root(),
            [(0, removed.value_span.unwrap())].into_iter(),
            &mut a,
        )
        .unwrap();
    let mut token = output.read_token(target, &a).unwrap();
    assert_eq!(
        buffer::Text::from_token(&mut token.bytes, &a)
            .unwrap()
            .as_str(&a)
            .unwrap(),
        "new"
    );
    super::mutation::consume_key(removed, &mut a);
    discard(output, &mut a);
    discard(query, &mut a);
    drain(&mut map, &mut a);
    empty(&a);
}

#[test]
fn all_three_entry_removal_permutations_and_zero_sized_values_preserve_dense_lookup_and_reuse_capacity()
 {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for value_type in [o::Type::I64, ty(2)] {
        for x in 0..3 {
            for y in 0..3 {
                for z in 0..3 {
                    if x == y || x == z || y == z {
                        continue;
                    }
                    let (mut a, t) = allocator(Default::default());
                    let mut map =
                        map::Map::new(&layouts, o::Type::I64, value_type, &mut a).unwrap();
                    let mut k = number(&layouts, 0, &mut a);
                    let mut v = l::Reservation::allocate(&layouts, value_type, &mut a).unwrap();
                    let mut output =
                        l::Reservation::allocate(&layouts, value_type, &mut a).unwrap();
                    let mut live = std::collections::BTreeSet::new();
                    let spans = map::Spans {
                        key: Some(span(0)),
                        value: Some(span(1)),
                    };
                    for n in 0..3 {
                        k.write_immediate(k.root(), l::Immediate::I64(n), &mut a)
                            .unwrap();
                        if value_type == o::Type::I64 {
                            v.write_immediate(v.root(), l::Immediate::I64(10 + n), &mut a)
                                .unwrap();
                        }
                        map.insert_into(&mut k, &mut v, spans, &output, output.root(), &mut a)
                            .unwrap();
                        live.insert(n);
                    }
                    let calls = t.borrow().requests.len();
                    for n in [x, y, z] {
                        let old = map
                            .remove_into(map::Key::I64(n), &output, output.root(), &mut a)
                            .unwrap()
                            .unwrap();
                        assert!(old.key.is_none());
                        if value_type == o::Type::I64 {
                            assert_eq!(
                                output.read_immediate(output.root(), &a).unwrap(),
                                l::Immediate::I64(10 + n)
                            );
                        }
                        live.remove(&n);
                        for query in 0..3 {
                            assert_eq!(
                                map.lookup(map::Key::I64(query), &a).unwrap().is_some(),
                                live.contains(&query)
                            );
                        }
                        assert_eq!(map.len(&a).unwrap(), live.len() as i64);
                    }
                    map.insert_into(&mut k, &mut v, spans, &output, output.root(), &mut a)
                        .unwrap();
                    assert_eq!(map.len(&a).unwrap(), 1);
                    assert_eq!(t.borrow().requests.len(), calls);
                    k.release_storage(&mut a).unwrap();
                    v.release_storage(&mut a).unwrap();
                    output.release_storage(&mut a).unwrap();
                    drain(&mut map, &mut a);
                    empty(&a);
                }
            }
        }
    }
}

#[test]
fn key_cleanup_error_or_panic_happens_after_publication_and_still_drains_returned_and_stored_values()
 {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for removal in [false, true] {
        for panic in [false, true] {
            let (mut a, t) = allocator(Default::default());
            let mut map = map::Map::new(&layouts, ty(0), ty(0), &mut a).unwrap();
            let mut k = text(&layouts, "same", &mut a);
            let original = k.read_token(k.root(), &a).unwrap().bytes;
            let mut v = text(&layouts, "old", &mut a);
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
            let mut incoming = text(&layouts, "same", &mut a);
            let incoming_token = incoming.read_token(incoming.root(), &a).unwrap().bytes;
            let mut value = if removal {
                None
            } else {
                Some(text(&layouts, "new", &mut a))
            };
            let mut output = l::Reservation::allocate(&layouts, ty(0), &mut a).unwrap();
            let calls = t.borrow().requests.len();
            let mut displaced = if removal {
                map.remove_into(
                    map.key_of(&incoming, incoming.root(), &a).unwrap(),
                    &output,
                    output.root(),
                    &mut a,
                )
                .unwrap()
                .unwrap()
            } else {
                map.insert_into(
                    &mut incoming,
                    value.as_mut().unwrap(),
                    map::Spans {
                        key: Some(span(5)),
                        value: Some(span(6)),
                    },
                    &output,
                    output.root(),
                    &mut a,
                )
                .unwrap()
                .unwrap()
            };
            assert_eq!(
                displaced.key.as_ref().unwrap().bytes,
                if removal { original } else { incoming_token }
            );
            let mut seen = Vec::new();
            let report = context(|ctx| {
                assert_eq!(map.len(&a).unwrap(), i64::from(!removal));
                let token = displaced.key.take().unwrap();
                let source = token.leaf.span;
                ctx.field(
                    token,
                    source,
                    &mut c::Contextual::new(
                        |token: &mut l::Token, reason, ctx: &mut c::Context<'_>| {
                            let mut copy = token.bytes;
                            let b = buffer::Text::from_token(&mut copy, &a).unwrap();
                            seen.push((b.as_str(&a).unwrap().to_owned(), token.leaf.span));
                            clean(token, &mut a, reason, ctx)?;
                            if panic {
                                panic!("detached Map key");
                            }
                            Err(41)
                        },
                    ),
                );
                retire(
                    &mut output,
                    displaced.value_span,
                    &mut a,
                    ctx,
                    |token, a, reason, ctx| {
                        let mut copy = token.bytes;
                        let b = buffer::Text::from_token(&mut copy, a).unwrap();
                        seen.push((b.as_str(a).unwrap().to_owned(), token.leaf.span));
                        clean(token, a, reason, ctx)?;
                        Err(42)
                    },
                )?;
                map.cleanup(&mut a, ctx, |token, a, reason, ctx| {
                    let mut copy = token.bytes;
                    let b = buffer::Text::from_token(&mut copy, a).unwrap();
                    seen.push((b.as_str(a).unwrap().to_owned(), token.leaf.span));
                    clean(token, a, reason, ctx)
                })
                .unwrap();
                if removal {
                    retire(
                        &mut incoming,
                        Some(span(5)),
                        &mut a,
                        ctx,
                        |token, a, reason, ctx| {
                            let mut copy = token.bytes;
                            let b = buffer::Text::from_token(&mut copy, a).unwrap();
                            seen.push((b.as_str(a).unwrap().to_owned(), token.leaf.span));
                            clean(token, a, reason, ctx)
                        },
                    )?;
                }
                Ok(())
            });
            assert_eq!(
                report.primary().unwrap().span,
                Some(span(if removal { 3 } else { 5 }))
            );
            assert_eq!(
                report.primary().unwrap().kind,
                if panic {
                    c::FailureKind::CleanupPanic
                } else {
                    c::FailureKind::Cleanup(41)
                }
            );
            assert_eq!(report.secondary().len(), 1);
            assert_eq!(report.secondary()[0].span, Some(span(4)));
            assert_eq!(report.secondary()[0].kind, c::FailureKind::Cleanup(42));
            let expected = if removal {
                vec![("same", 3), ("old", 4), ("same", 5)]
            } else {
                vec![("same", 5), ("old", 4), ("new", 6), ("same", 3)]
            };
            assert_eq!(
                seen,
                expected
                    .into_iter()
                    .map(|(s, n)| (s.to_owned(), Some(span(n))))
                    .collect::<Vec<_>>()
            );
            assert_eq!(t.borrow().requests.len(), calls);
            if !removal {
                incoming.release_storage(&mut a).unwrap();
                value.as_mut().unwrap().release_storage(&mut a).unwrap();
            }
            empty(&a);
        }
    }
}

#[test]
fn lookup_work_limit_refuses_replacement_and_removal_without_changing_owners_or_output() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut map = map::Map::new_with_limits(
        &layouts,
        ty(0),
        o::Type::I64,
        map::Limits {
            lookup_work: 2,
            ..Default::default()
        },
        &mut a,
    )
    .unwrap();
    let mut k = text(&layouts, "ok", &mut a);
    let mut v = number(&layouts, 9, &mut a);
    let mut output = number(&layouts, 0, &mut a);
    let spans = map::Spans {
        key: Some(span(0)),
        value: Some(span(1)),
    };
    map.insert_into(&mut k, &mut v, spans, &output, output.root(), &mut a)
        .unwrap();
    k.release_storage(&mut a).unwrap();
    let mut k = text(&layouts, "ok", &mut a);
    let query = map.key_of(&k, k.root(), &a).unwrap();
    let original = k.data(&a).unwrap().to_vec();
    let before = a.stats();
    let calls = t.borrow().requests.len();
    assert!(matches!(
        map.insert_into(&mut k, &mut v, spans, &output, output.root(), &mut a),
        Err(map::Error::WorkLimit)
    ));
    assert!(matches!(
        map.remove_into(query, &output, output.root(), &mut a),
        Err(map::Error::WorkLimit)
    ));
    assert_eq!(a.stats(), before);
    assert_eq!(t.borrow().requests.len(), calls);
    assert_eq!(k.data(&a).unwrap(), original);
    assert_eq!(
        output.read_immediate(output.root(), &a).unwrap(),
        l::Immediate::I64(0)
    );
    assert_eq!(map.len(&a).unwrap(), 1);
    discard(k, &mut a);
    v.release_storage(&mut a).unwrap();
    output.release_storage(&mut a).unwrap();
    drain(&mut map, &mut a);
    empty(&a);
}
