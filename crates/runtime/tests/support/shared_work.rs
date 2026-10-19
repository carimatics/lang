use super::*;
use lang_runtime::execution::{Failure, ServiceWork};

#[test]
fn shared_payload_validation_copy_and_read_share_one_counter() {
    let types = types();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let meter = ServiceWork::new(Some(10));
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 2,
            slots: 2,
            tickets: 5,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap()
    .with_service_work(meter.clone());
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    heap.open(&mut allocator).unwrap();
    let mut input = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    input
        .write_immediate(input.root(), l::Immediate::I64(7), &mut allocator)
        .unwrap();
    let strong = heap
        .allocate_copy(&mut input, Some(span()), &mut allocator)
        .unwrap();
    // One payload-node validation plus the eight copied i64 bytes.
    assert_eq!(meter.used(), 9);
    let read = heap.read(&strong, o::Type::I64, &mut allocator).unwrap();
    {
        let view = heap.view(read, o::Type::I64, &allocator).unwrap();
        assert_eq!(
            view.read_immediate(view.root(), &allocator),
            Ok(l::Immediate::I64(7))
        );
        assert_eq!(meter.used(), 10);
        assert_eq!(
            view.read_immediate(view.root(), &allocator),
            Err(l::Error::ServiceWork(Failure::ServiceWorkLimit {
                limit: 10,
                used: 10,
                requested: 1
            }))
        );
    }
    assert!(
        context(|ctx| heap
            .end_read(
                read,
                o::Type::I64,
                &mut allocator,
                ctx,
                |_, _, _, _| panic!("scalar payload")
            )
            .map_err(|_| 99))
        .primary()
        .is_none()
    );
    let mut strong = Some(strong);
    assert!(
        context(|ctx| heap
            .release_strong(
                strong.take().unwrap(),
                o::Type::I64,
                &mut allocator,
                ctx,
                |_, _, _, _| panic!("scalar payload")
            )
            .map_err(|_| 99))
        .primary()
        .is_none()
    );
    heap.close(&mut allocator).unwrap();
    input.release_storage(&mut allocator).unwrap();
    assert_eq!(meter.used(), 10);
    assert_eq!(allocator.stats().live_allocations, 0);
}
