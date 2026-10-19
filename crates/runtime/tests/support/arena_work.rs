use super::*;
use lang_runtime::execution::{Failure, ServiceWork};

#[test]
fn arena_payload_validation_copy_and_read_share_one_counter() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, _) = allocator(Default::default());
    let meter = ServiceWork::new(Some(10));
    let mut arena = prepare(&layouts, Default::default()).with_service_work(meter.clone());
    arena.open(&mut allocator).unwrap();
    let mut input = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    input
        .write_immediate(input.root(), l::Immediate::I64(7), &mut allocator)
        .unwrap();
    let reference = arena
        .allocate_copy(&mut input, Some(span()), &mut allocator, no_containers)
        .unwrap();
    assert_eq!(meter.used(), 9);
    let mut read = arena.read(reference, o::Type::I64, &mut allocator).unwrap();
    {
        let view = arena.payload(&read, &allocator).unwrap();
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
    arena.end_read(&mut read, &mut allocator).unwrap();
    input.release_storage(&mut allocator).unwrap();
    assert!(
        retire(
            &mut arena,
            &mut allocator,
            g::Exit::Normal,
            |_, _, _, _| panic!("scalar payload")
        )
        .primary()
        .is_none()
    );
    assert_eq!(meter.used(), 10);
    assert_eq!(allocator.stats().live_allocations, 0);
}
