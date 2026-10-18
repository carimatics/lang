use super::*;

pub(super) fn consume_key<P: a::Provider>(mut displaced: map::Displaced, a: &mut a::Allocator<P>) {
    let report = context(|ctx| {
        if let Some(token) = displaced.key.take() {
            let span = token.leaf.span;
            ctx.field(
                token,
                span,
                &mut c::Contextual::new(
                    |token: &mut l::Token, reason, ctx: &mut c::Context<'_>| {
                        clean(token, a, reason, ctx)
                    },
                ),
            );
        }
        Ok(())
    });
    assert!(report.primary().is_none(), "{report:?}");
}
fn contents(value: &l::Reservation<'_, '_>, a: &a::Allocator<Provider>) -> [u8; 2] {
    let record = value.child(value.root(), 0, a).unwrap();
    std::array::from_fn(|i| {
        let mut token = value
            .read_token(value.child(record, i as u32, a).unwrap(), a)
            .unwrap();
        buffer::Bytes::from_token(&mut token.bytes, a)
            .unwrap()
            .as_slice(a)
            .unwrap()[0]
    })
}
fn inspect(
    map: &map::Map<'_, '_>,
    key: &str,
    layouts: &l::Layouts<'_>,
    a: &mut a::Allocator<Provider>,
) -> [u8; 2] {
    let query = text(layouts, key, a);
    let entry = map
        .lookup(map.key_of(&query, query.root(), a).unwrap(), a)
        .unwrap()
        .unwrap();
    let view = map.view(entry, a).unwrap();
    let record = view.child(view.root(), 0).unwrap();
    let result = std::array::from_fn(|i| {
        let mut token = view
            .read_token(view.child(record, i as u32).unwrap())
            .unwrap();
        buffer::Bytes::from_token(&mut token.bytes, a)
            .unwrap()
            .as_slice(a)
            .unwrap()[0]
    });
    discard(query, a);
    result
}

#[test]
fn text_replacement_preserves_the_original_key_then_remove_transfers_that_exact_owner_and_spans() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut map = map::Map::new(&layouts, ty(0), ty(3), &mut a).unwrap();
    let mut k = text(&layouts, "é", &mut a);
    let original = k.read_token(k.root(), &a).unwrap().bytes;
    let mut v = pair(&layouts, 10, &mut a);
    let output = l::Reservation::allocate(&layouts, ty(3), &mut a).unwrap();
    let spans = map::Spans {
        key: Some(span(3)),
        value: Some(span(4)),
    };
    assert!(
        map.insert_into(&mut k, &mut v, spans, &output, output.root(), &mut a)
            .unwrap()
            .is_none()
    );
    k.release_storage(&mut a).unwrap();
    v.release_storage(&mut a).unwrap();
    let mut k = text(&layouts, "é", &mut a);
    let incoming = k.read_token(k.root(), &a).unwrap().bytes;
    assert_ne!(incoming, original);
    let mut v = pair(&layouts, 20, &mut a);
    let calls = t.borrow().requests.len();
    let displaced = map
        .insert_into(
            &mut k,
            &mut v,
            map::Spans {
                key: Some(span(5)),
                value: Some(span(6)),
            },
            &output,
            output.root(),
            &mut a,
        )
        .unwrap()
        .unwrap();
    assert_eq!(t.borrow().requests.len(), calls);
    assert_eq!(displaced.key.as_ref().unwrap().bytes, incoming);
    assert_eq!(displaced.key.as_ref().unwrap().leaf.span, Some(span(5)));
    assert_eq!(displaced.value_span, Some(span(4)));
    assert!(k.read_token(k.root(), &a).is_err());
    assert!(k.data(&a).unwrap().iter().all(|b| *b == 0));
    assert_eq!(contents(&output, &a), [10, 11]);
    assert_eq!(inspect(&map, "é", &layouts, &mut a), [20, 21]);
    consume_key(displaced, &mut a);
    k.release_storage(&mut a).unwrap();
    v.release_storage(&mut a).unwrap();
    discard(output, &mut a);
    let query = text(&layouts, "é", &mut a);
    let query_bytes = query.data(&a).unwrap().to_vec();
    let output = l::Reservation::allocate(&layouts, ty(3), &mut a).unwrap();
    let calls = t.borrow().requests.len();
    let removed = map
        .remove_into(
            map.key_of(&query, query.root(), &a).unwrap(),
            &output,
            output.root(),
            &mut a,
        )
        .unwrap()
        .unwrap();
    assert_eq!(removed.key.as_ref().unwrap().bytes, original);
    assert_eq!(removed.key.as_ref().unwrap().leaf.span, Some(span(3)));
    assert_eq!(removed.value_span, Some(span(6)));
    assert_eq!(query.data(&a).unwrap(), query_bytes);
    assert_eq!(contents(&output, &a), [20, 21]);
    assert!(map.is_empty(&a).unwrap());
    consume_key(removed, &mut a);
    assert_eq!(t.borrow().requests.len(), calls);
    discard(output, &mut a);
    discard(query, &mut a);
    drain(&mut map, &mut a);
    empty(&a);
}

#[test]
fn mixed_insert_replace_remove_and_growth_match_independent_values_keys_and_retirement_order() {
    #[derive(Clone)]
    struct Expected {
        first: u8,
        key: [u8; 32],
        key_span: m::Span,
        value_span: m::Span,
    }
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut map = map::Map::new(&layouts, ty(0), ty(3), &mut a).unwrap();
    let mut expected = std::collections::BTreeMap::<String, Expected>::new();
    let mut order = Vec::new();
    for step in 0..128u8 {
        let name = format!(
            "k{}",
            if step < 20 {
                step
            } else {
                step.wrapping_mul(13) % 23
            }
        );
        let initializer = span(u32::from(step % 20));
        let output = l::Reservation::allocate(&layouts, ty(3), &mut a).unwrap();
        if step < 20 || step % 7 < 4 {
            let mut k = text(&layouts, &name, &mut a);
            let token = k.read_token(k.root(), &a).unwrap().bytes;
            let mut v = pair(&layouts, step, &mut a);
            let old = map
                .insert_into(
                    &mut k,
                    &mut v,
                    map::Spans {
                        key: Some(initializer),
                        value: Some(initializer),
                    },
                    &output,
                    output.root(),
                    &mut a,
                )
                .unwrap();
            let original = if let Some(previous) = expected.get(&name) {
                let old = old.unwrap();
                assert_eq!(contents(&output, &a), [previous.first, previous.first + 1]);
                assert_eq!(old.value_span, Some(previous.value_span));
                assert_eq!(old.key.as_ref().unwrap().bytes, token);
                assert_eq!(old.key.as_ref().unwrap().leaf.span, Some(initializer));
                consume_key(old, &mut a);
                discard(output, &mut a);
                (previous.key, previous.key_span)
            } else {
                assert!(old.is_none());
                assert!(output.data(&a).unwrap().iter().all(|b| *b == 0));
                let mut output = output;
                output.release_storage(&mut a).unwrap();
                (token, initializer)
            };
            k.release_storage(&mut a).unwrap();
            v.release_storage(&mut a).unwrap();
            expected.insert(
                name.clone(),
                Expected {
                    first: step,
                    key: original.0,
                    key_span: original.1,
                    value_span: initializer,
                },
            );
            order.retain(|s| s != &name);
            order.push(name.clone());
        } else {
            let query = text(&layouts, &name, &mut a);
            let query_original = query.data(&a).unwrap().to_vec();
            let old = map
                .remove_into(
                    map.key_of(&query, query.root(), &a).unwrap(),
                    &output,
                    output.root(),
                    &mut a,
                )
                .unwrap();
            if let Some(previous) = expected.remove(&name) {
                let old = old.unwrap();
                assert_eq!(contents(&output, &a), [previous.first, previous.first + 1]);
                assert_eq!(old.value_span, Some(previous.value_span));
                assert_eq!(old.key.as_ref().unwrap().bytes, previous.key);
                assert_eq!(old.key.as_ref().unwrap().leaf.span, Some(previous.key_span));
                consume_key(old, &mut a);
                discard(output, &mut a);
                order.retain(|s| s != &name);
            } else {
                assert!(old.is_none());
                assert!(output.data(&a).unwrap().iter().all(|b| *b == 0));
                let mut output = output;
                output.release_storage(&mut a).unwrap();
            }
            assert_eq!(query.data(&a).unwrap(), query_original);
            discard(query, &mut a);
        }
        assert_eq!(map.len(&a).unwrap(), expected.len() as i64);
        for (key, want) in &expected {
            assert_eq!(
                inspect(&map, key, &layouts, &mut a),
                [want.first, want.first + 1]
            );
        }
    }
    let calls = t.borrow().requests.len();
    let mut observed = Vec::new();
    let report = context(|ctx| {
        map.cleanup(&mut a, ctx, |token, a, reason, ctx| {
            let mut copy = token.bytes;
            let value = if token.leaf.ty == ty(0) {
                format!(
                    "t:{}",
                    buffer::Text::from_token(&mut copy, a)
                        .unwrap()
                        .as_str(a)
                        .unwrap()
                )
            } else {
                format!(
                    "b:{}",
                    buffer::Bytes::from_token(&mut copy, a)
                        .unwrap()
                        .as_slice(a)
                        .unwrap()[0]
                )
            };
            observed.push((value, token.leaf.span));
            clean(token, a, reason, ctx)
        })
        .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    let want = order
        .iter()
        .rev()
        .flat_map(|key| {
            let v = &expected[key];
            [
                (format!("b:{}", v.first), Some(span(0))),
                (format!("b:{}", v.first + 1), Some(span(1))),
                (format!("t:{key}"), Some(v.key_span)),
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(observed, want);
    assert_eq!(t.borrow().requests.len(), calls);
    empty(&a);
}

#[test]
fn integer_replacement_and_removal_publish_real_values_and_preserve_copy_operands_without_allocating()
 {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut map = map::Map::new(&layouts, o::Type::I64, o::Type::I64, &mut a).unwrap();
    let mut key = number(&layouts, 1, &mut a);
    let mut value = number(&layouts, 9, &mut a);
    let mut output = number(&layouts, 0, &mut a);
    let spans = map::Spans {
        key: Some(span(0)),
        value: Some(span(1)),
    };
    assert!(
        map.insert_into(&mut key, &mut value, spans, &output, output.root(), &mut a)
            .unwrap()
            .is_none()
    );
    value
        .write_immediate(value.root(), l::Immediate::I64(20), &mut a)
        .unwrap();
    let calls = t.borrow().requests.len();
    let replaced = map
        .insert_into(
            &mut key,
            &mut value,
            map::Spans {
                value: Some(span(2)),
                ..spans
            },
            &output,
            output.root(),
            &mut a,
        )
        .unwrap()
        .unwrap();
    assert!(replaced.key.is_none());
    assert_eq!(replaced.value_span, Some(span(1)));
    assert_eq!(
        output.read_immediate(output.root(), &a).unwrap(),
        l::Immediate::I64(9)
    );
    assert_eq!(
        value.read_immediate(value.root(), &a).unwrap(),
        l::Immediate::I64(20)
    );
    let removed = map
        .remove_into(map::Key::I64(1), &output, output.root(), &mut a)
        .unwrap()
        .unwrap();
    assert!(removed.key.is_none());
    assert_eq!(removed.value_span, Some(span(2)));
    assert_eq!(
        output.read_immediate(output.root(), &a).unwrap(),
        l::Immediate::I64(20)
    );
    assert!(map.is_empty(&a).unwrap());
    assert!(
        map.remove_into(map::Key::I64(1), &output, output.root(), &mut a)
            .unwrap()
            .is_none()
    );
    assert_eq!(t.borrow().requests.len(), calls);
    key.release_storage(&mut a).unwrap();
    value.release_storage(&mut a).unwrap();
    output.release_storage(&mut a).unwrap();
    drain(&mut map, &mut a);
    empty(&a);
}
