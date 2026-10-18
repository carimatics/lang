use super::*;

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
