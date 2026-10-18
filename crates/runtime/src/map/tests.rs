use super::*;

fn scalar_model() -> o::VerifiedTypes {
    o::TypeTable {
        sources: vec![m::Source {
            path: "map.lang".into(),
            text: "abcdef".into(),
        }],
        ..Default::default()
    }
    .verify(Default::default())
    .unwrap()
}
fn span(start: u32) -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start,
        end: start + 1,
    }
}

#[test]
fn replacement_and_removal_version_exhaustion_preserve_map_inputs_and_output() {
    let types = scalar_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut a = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut map = Map::new(&layouts, o::Type::I64, o::Type::I64, &mut a).unwrap();
    let mut k = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    let mut v = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    let mut out = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    v.write_immediate(v.root(), l::Immediate::I64(9), &mut a)
        .unwrap();
    map.publish_if_absent(
        &mut k,
        &mut v,
        Spans {
            key: Some(span(0)),
            value: Some(span(1)),
        },
        &mut a,
    )
    .unwrap();
    v.write_immediate(v.root(), l::Immediate::I64(20), &mut a)
        .unwrap();
    out.write_immediate(out.root(), l::Immediate::I64(77), &mut a)
        .unwrap();
    put(a.bytes_mut(map.owner.unwrap()).unwrap(), 160, u64::MAX);
    let before = a.stats();
    assert!(matches!(
        map.insert_into(
            &mut k,
            &mut v,
            Spans {
                key: Some(span(0)),
                value: Some(span(2))
            },
            &out,
            out.root(),
            &mut a
        ),
        Err(Error::Allocation(a::Error::Overflow))
    ));
    assert!(matches!(
        map.remove_into(Key::I64(0), &out, out.root(), &mut a),
        Err(Error::Allocation(a::Error::Overflow))
    ));
    assert_eq!(a.stats(), before);
    assert_eq!(
        out.read_immediate(out.root(), &a).unwrap(),
        l::Immediate::I64(77)
    );
    assert_eq!(
        v.read_immediate(v.root(), &a).unwrap(),
        l::Immediate::I64(20)
    );
    let entry = map.lookup(Key::I64(0), &a).unwrap().unwrap();
    let view = map.view(entry, &a).unwrap();
    assert_eq!(
        view.read_immediate(view.root()).unwrap(),
        l::Immediate::I64(9)
    );
    k.release_storage(&mut a).unwrap();
    v.release_storage(&mut a).unwrap();
    out.release_storage(&mut a).unwrap();
    drain(&mut map, &mut a);
    assert_eq!(a.stats().live_allocations, 0);
}

#[test]
fn malformed_existing_key_value_spans_or_order_links_are_rejected_before_value_transfer() {
    let types = scalar_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for bad in 0..3 {
        let mut a = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut map = Map::new(&layouts, o::Type::I64, o::Type::I64, &mut a).unwrap();
        let mut k = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
        let mut v = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
        let mut out = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
        v.write_immediate(v.root(), l::Immediate::I64(9), &mut a)
            .unwrap();
        map.publish_if_absent(
            &mut k,
            &mut v,
            Spans {
                key: Some(span(0)),
                value: Some(span(1)),
            },
            &mut a,
        )
        .unwrap();
        v.write_immediate(v.root(), l::Immediate::I64(20), &mut a)
            .unwrap();
        out.write_immediate(out.root(), l::Immediate::I64(77), &mut a)
            .unwrap();
        let h = map.inspect(&a).unwrap();
        let at = map.record(h, 0).unwrap();
        let saved = a.bytes(h.metadata.unwrap()).unwrap().to_vec();
        let bytes = a.bytes_mut(h.metadata.unwrap()).unwrap();
        match bad {
            0 => store_span(bytes, at, Some(span(100))),
            1 => store_span(bytes, at + 16, Some(span(100))),
            _ => put(bytes, at + 32, 0),
        }
        let before = a.stats();
        assert!(
            map.insert_into(
                &mut k,
                &mut v,
                Spans {
                    key: Some(span(0)),
                    value: Some(span(2))
                },
                &out,
                out.root(),
                &mut a
            )
            .is_err()
        );
        assert!(
            map.remove_into(Key::I64(0), &out, out.root(), &mut a)
                .is_err()
        );
        assert_eq!(a.stats(), before);
        assert_eq!(
            out.read_immediate(out.root(), &a).unwrap(),
            l::Immediate::I64(77)
        );
        assert_eq!(
            v.read_immediate(v.root(), &a).unwrap(),
            l::Immediate::I64(20)
        );
        let entry = map.lookup(Key::I64(0), &a).unwrap().unwrap();
        let view = map.view(entry, &a).unwrap();
        assert_eq!(
            view.read_immediate(view.root()).unwrap(),
            l::Immediate::I64(9)
        );
        a.bytes_mut(h.metadata.unwrap())
            .unwrap()
            .copy_from_slice(&saved);
        k.release_storage(&mut a).unwrap();
        v.release_storage(&mut a).unwrap();
        out.release_storage(&mut a).unwrap();
        drain(&mut map, &mut a);
        assert_eq!(a.stats().live_allocations, 0);
    }
}

pub(super) fn drain(map: &mut Map<'_, '_>, allocator: &mut a::Allocator<a::System>) {
    let mut instance = c::Instance::default();
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |_: &mut (), _, context: &mut c::Context<'_>| {
                map.cleanup(allocator, context, |_, _, _, _| panic!("Copy leaf"))
                    .map_err(|_| 99)
            },
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    frame.initialize(c::Slot(0), (), None).unwrap();
    assert!(
        frame
            .finish(c::Exit::Return(None))
            .report
            .primary()
            .is_none()
    );
}

#[test]
fn lease_serial_and_entry_version_exhaustion_never_wrap_or_consume_input_obligations() {
    let types = o::TypeTable::default().verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut a = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut map = Map::new_with_limits(
        &layouts,
        o::Type::I64,
        o::Type::I64,
        Limits {
            loans: 2,
            ..Default::default()
        },
        &mut a,
    )
    .unwrap();
    let mut k = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    let mut v = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
        .unwrap();
    put(a.bytes_mut(map.owner.unwrap()).unwrap(), 144, u64::MAX);
    let mut last = map.borrow(Key::I64(0), &mut a).unwrap().unwrap();
    let before = a.stats();
    assert!(matches!(
        map.borrow(Key::I64(0), &mut a),
        Err(Error::Allocation(a::Error::Overflow))
    ));
    assert_eq!(a.stats(), before);
    assert!(last.view(&layouts, &a).is_ok());
    last.end(&layouts, &mut a).unwrap();
    assert!(matches!(
        map.borrow(Key::I64(0), &mut a),
        Err(Error::Allocation(a::Error::Overflow))
    ));
    put(a.bytes_mut(map.owner.unwrap()).unwrap(), 160, u64::MAX);
    k.write_immediate(k.root(), l::Immediate::I64(1), &mut a)
        .unwrap();
    let before = a.stats();
    assert_eq!(
        map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a),
        Err(Error::Allocation(a::Error::Overflow))
    );
    assert_eq!(a.stats(), before);
    assert_eq!(map.len(&a).unwrap(), 1);
    assert_eq!(
        k.read_immediate(k.root(), &a).unwrap(),
        l::Immediate::I64(1)
    );
    k.release_storage(&mut a).unwrap();
    v.release_storage(&mut a).unwrap();
    drain(&mut map, &mut a);
    assert_eq!(a.stats().live_allocations, 0);
}

#[test]
fn malformed_text_is_budgeted_before_utf8_scan_and_foreign_or_stale_tokens_never_authorize_access()
{
    let text = o::Type::Constructed(o::ConstructedId(0));
    let types = o::TypeTable {
        sources: vec![m::Source {
            path: "map.lang".into(),
            text: "a".into(),
        }],
        definitions: vec![],
        constructed: vec![o::Constructed {
            constructor: o::Constructor::Builtin(o::Builtin::Text),
            span: m::Span {
                source: m::SourceId(0),
                start: 0,
                end: 1,
            },
        }],
    }
    .verify(Default::default())
    .unwrap();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut a = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut b = buffer::Bytes::from_slice(&mut a, b"ok").unwrap();
    let mut t = b.take_text(&mut a).unwrap();
    let token = t.take_token(&a).unwrap();
    let handle = a::Handle::from_token(token[..24].try_into().unwrap()).unwrap();
    a.bytes_mut(handle).unwrap()[24] = 0xff;
    for work in [1, 2] {
        let mut map = Map::new_with_limits(
            &layouts,
            text,
            o::Type::I64,
            Limits {
                lookup_work: work,
                ..Default::default()
            },
            &mut a,
        )
        .unwrap();
        let before = a.stats();
        assert_eq!(
            map.lookup(Key::Text(token), &a),
            Err(if work == 1 {
                Error::WorkLimit
            } else {
                Error::WrongKey
            })
        );
        assert_eq!(a.stats(), before);
        drain(&mut map, &mut a);
    }
    a.bytes_mut(handle).unwrap()[24] = b'o';
    let mut map = Map::new(&layouts, text, o::Type::I64, &mut a).unwrap();
    let mut foreign = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut b = buffer::Bytes::from_slice(&mut foreign, b"ok").unwrap();
    let mut t = b.take_text(&mut foreign).unwrap();
    let mut foreign_token = t.take_token(&foreign).unwrap();
    assert!(matches!(
        map.lookup(Key::Text(foreign_token), &a),
        Err(Error::Buffer(buffer::Fault::Allocator(
            a::Error::ForeignAllocator
        )))
    ));
    buffer::Text::from_token(&mut foreign_token, &foreign)
        .unwrap()
        .release(&mut foreign)
        .unwrap();
    let mut copy = token;
    buffer::Text::from_token(&mut copy, &a)
        .unwrap()
        .release(&mut a)
        .unwrap();
    let before = a.stats();
    assert!(map.lookup(Key::Text(token), &a).is_err());
    assert_eq!(a.stats(), before);
    drain(&mut map, &mut a);
    assert_eq!(a.stats().live_allocations, 0);
    assert_eq!(foreign.stats().live_allocations, 0);
}
