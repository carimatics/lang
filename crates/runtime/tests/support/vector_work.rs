use super::*;
use lang_runtime::execution::{Failure, ServiceWork};

#[test]
fn dependent_grow_copy_refuses_before_provider_and_keeps_admitted_oom_work() {
    for provider_failure in [false, true] {
        let types = model();
        let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
        let (mut allocator, trace) = allocator(Default::default());
        let input = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
        input
            .write_immediate(input.root(), l::Immediate::I64(7), &mut allocator)
            .unwrap();
        let mut input = input;
        let meter = ServiceWork::new(Some(if provider_failure { 24 } else { 15 }));
        let mut values = v::Vector::new(&layouts, o::Type::I64, &mut allocator)
            .unwrap()
            .with_service_work(meter.clone());
        values.push_copy(&mut input, None, &mut allocator).unwrap();
        assert_eq!(meter.used(), 8);
        let requests = trace.borrow().requests.len();
        if provider_failure {
            trace.borrow_mut().fail = Some(requests + 1);
        }
        let result = values.push_copy(&mut input, None, &mut allocator);
        if provider_failure {
            assert_eq!(result, Err(v::Error::Allocation(a::Error::OutOfMemory)));
            assert_eq!(meter.used(), 24);
            assert_eq!(trace.borrow().requests.len(), requests + 1);
        } else {
            assert_eq!(
                result,
                Err(v::Error::ServiceWork(Failure::ServiceWorkLimit {
                    limit: 15,
                    used: 8,
                    requested: 16
                }))
            );
            assert_eq!(meter.used(), 8);
            assert_eq!(trace.borrow().requests.len(), requests);
        }
        assert_eq!(values.len(&allocator), Ok(1));
        assert_eq!(
            input.read_immediate(input.root(), &allocator),
            Ok(l::Immediate::I64(7))
        );
        let used = meter.used();
        drain(&mut values, &mut allocator);
        assert_eq!(meter.used(), used);
        input.release_storage(&mut allocator).unwrap();
        empty(&allocator);
    }
}

#[test]
fn registered_reads_share_the_vector_meter_and_teardown_ignores_exhaustion() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, _) = allocator(Default::default());
    let mut input = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    input
        .write_immediate(input.root(), l::Immediate::I64(7), &mut allocator)
        .unwrap();
    let meter = ServiceWork::new(Some(9));
    let mut values = v::Vector::new(&layouts, o::Type::I64, &mut allocator)
        .unwrap()
        .with_service_work(meter.clone());
    values.push_copy(&mut input, None, &mut allocator).unwrap();
    let mut loan = values
        .borrow(0, o::LoanKind::Shared, &mut allocator)
        .unwrap();
    {
        let read = loan.view(&layouts, &allocator).unwrap();
        assert_eq!(
            read.read_immediate(read.root(), &allocator),
            Ok(l::Immediate::I64(7))
        );
        assert_eq!(meter.used(), 9);
        assert_eq!(
            read.read_immediate(read.root(), &allocator),
            Err(v::Error::Layout(l::Error::ServiceWork(
                Failure::ServiceWorkLimit {
                    limit: 9,
                    used: 9,
                    requested: 1
                }
            )))
        );
    }
    loan.end(&layouts, &mut allocator).unwrap();
    drain(&mut values, &mut allocator);
    input.release_storage(&mut allocator).unwrap();
    assert_eq!(meter.used(), 9);
    empty(&allocator);
}
