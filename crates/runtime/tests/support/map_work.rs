use super::*;
use lang_runtime::execution::{Failure, ServiceWork};

#[test]
fn dependent_map_growth_refuses_before_provider_and_preserves_oom_admission() {
    for provider_failure in [false, true] {
        let types = model();
        let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
        let (mut allocator, trace) = allocator(Default::default());
        let meter = ServiceWork::new(Some(if provider_failure { 49 } else { 48 }));
        let mut table = map::Map::new(&layouts, o::Type::I64, o::Type::I64, &mut allocator)
            .unwrap()
            .with_service_work(meter.clone());
        let mut key = number(&layouts, 1, &mut allocator);
        let mut value = number(&layouts, 7, &mut allocator);
        let spans = map::Spans {
            key: Some(span(1)),
            value: Some(span(2)),
        };
        table
            .publish_if_absent(&mut key, &mut value, spans, &mut allocator)
            .unwrap();
        assert_eq!(meter.used(), 16);
        key.write_immediate(key.root(), l::Immediate::I64(2), &mut allocator)
            .unwrap();
        let requests = trace.borrow().requests.len();
        if provider_failure {
            trace.borrow_mut().fail = Some(requests + 1);
        }
        let result = table.publish_if_absent(&mut key, &mut value, spans, &mut allocator);
        if provider_failure {
            assert!(matches!(
                result,
                Err(map::Error::Allocation(a::Error::OutOfMemory))
            ));
            assert_eq!(meter.used(), 49);
            assert_eq!(trace.borrow().requests.len(), requests + 1);
        } else {
            assert!(matches!(
                result,
                Err(map::Error::ServiceWork(Failure::ServiceWorkLimit {
                    limit: 48,
                    used: 17,
                    requested: 32
                }))
            ));
            assert_eq!(meter.used(), 17);
            assert_eq!(trace.borrow().requests.len(), requests);
        }
        assert_eq!(table.len(&allocator), Ok(1));
        assert_eq!(
            value.read_immediate(value.root(), &allocator),
            Ok(l::Immediate::I64(7))
        );
        let used = meter.used();
        drain(&mut table, &mut allocator);
        key.release_storage(&mut allocator).unwrap();
        value.release_storage(&mut allocator).unwrap();
        assert_eq!(meter.used(), used);
        empty(&allocator);
    }
}

#[test]
fn replacement_refusal_keeps_old_value_incoming_value_and_output_unchanged() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut table = map::Map::new(&layouts, o::Type::I64, o::Type::I64, &mut allocator).unwrap();
    let mut key = number(&layouts, 1, &mut allocator);
    let mut value = number(&layouts, 7, &mut allocator);
    let spans = map::Spans {
        key: Some(span(1)),
        value: Some(span(2)),
    };
    table
        .publish_if_absent(&mut key, &mut value, spans, &mut allocator)
        .unwrap();
    value
        .write_immediate(value.root(), l::Immediate::I64(9), &mut allocator)
        .unwrap();
    let mut output = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    let original_output = output.read_immediate(output.root(), &allocator).unwrap();
    let meter = ServiceWork::new(Some(16));
    table = table.with_service_work(meter.clone());
    let requests = trace.borrow().requests.len();
    assert!(matches!(
        table.insert_into(
            &mut key,
            &mut value,
            spans,
            &output,
            output.root(),
            &mut allocator
        ),
        Err(map::Error::ServiceWork(Failure::ServiceWorkLimit {
            limit: 16,
            used: 1,
            requested: 16
        }))
    ));
    assert_eq!(meter.used(), 1);
    assert_eq!(trace.borrow().requests.len(), requests);
    assert_eq!(
        output.read_immediate(output.root(), &allocator),
        Ok(original_output)
    );
    assert_eq!(
        value.read_immediate(value.root(), &allocator),
        Ok(l::Immediate::I64(9))
    );
    table = table.with_service_work(ServiceWork::new(None));
    let mut loan = table
        .borrow(map::Key::I64(1), &mut allocator)
        .unwrap()
        .unwrap();
    let view = loan.view(&layouts, &allocator).unwrap();
    assert_eq!(
        view.read_immediate(view.root(), &allocator),
        Ok(l::Immediate::I64(7))
    );
    loan.end(&layouts, &mut allocator).unwrap();
    drain(&mut table, &mut allocator);
    key.release_storage(&mut allocator).unwrap();
    value.release_storage(&mut allocator).unwrap();
    output.release_storage(&mut allocator).unwrap();
    empty(&allocator);
}

#[test]
fn text_lookup_accounts_for_both_utf8_scans_and_full_length_comparison() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut table = map::Map::new(&layouts, ty(0), o::Type::I64, &mut allocator).unwrap();
    let mut key = text(&layouts, "ab", &mut allocator);
    let mut value = number(&layouts, 7, &mut allocator);
    table
        .publish_if_absent(
            &mut key,
            &mut value,
            map::Spans {
                key: None,
                value: None,
            },
            &mut allocator,
        )
        .unwrap();
    key.release_storage(&mut allocator).unwrap();
    value.release_storage(&mut allocator).unwrap();
    let query = text(&layouts, "ab", &mut allocator);
    let needle = table.key_of(&query, query.root(), &allocator).unwrap();
    let meters = [ServiceWork::new(Some(6)), ServiceWork::new(Some(7))];
    for (limit, meter) in [(6, &meters[0]), (7, &meters[1])] {
        table = table.with_service_work(meter.clone());
        assert!(matches!(
            table.lookup(map::Key::I64(7), &allocator),
            Err(map::Error::WrongKey)
        ));
        assert_eq!(meter.used(), 0);
        let requests = trace.borrow().requests.len();
        let result = table.lookup(needle, &allocator);
        if limit == 6 {
            assert!(matches!(
                result,
                Err(map::Error::ServiceWork(Failure::ServiceWorkLimit {
                    limit: 6,
                    used: 5,
                    requested: 2
                }))
            ));
            assert_eq!(meter.used(), 5);
        } else {
            assert!(result.unwrap().is_some());
            assert_eq!(meter.used(), 7);
        }
        assert_eq!(trace.borrow().requests.len(), requests);
        assert_eq!(table.len(&allocator), Ok(1));
    }
    let used = meters[1].used();
    discard(query, &mut allocator);
    drain(&mut table, &mut allocator);
    // Draining token payloads does not consult the exhausted service counter.
    assert_eq!(meters[1].used(), used);
    empty(&allocator);
}

#[test]
fn local_lookup_refusal_does_not_commit_global_work() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, _) = allocator(Default::default());
    let mut table = map::Map::new_with_limits(
        &layouts,
        o::Type::I64,
        o::Type::I64,
        map::Limits {
            lookup_work: 0,
            ..Default::default()
        },
        &mut allocator,
    )
    .unwrap();
    let mut key = number(&layouts, 1, &mut allocator);
    let mut value = number(&layouts, 7, &mut allocator);
    table
        .publish_if_absent(
            &mut key,
            &mut value,
            map::Spans {
                key: None,
                value: None,
            },
            &mut allocator,
        )
        .unwrap();
    key.release_storage(&mut allocator).unwrap();
    value.release_storage(&mut allocator).unwrap();
    let meter = ServiceWork::new(Some(0));
    table = table.with_service_work(meter.clone());
    assert!(matches!(
        table.lookup(map::Key::I64(1), &allocator),
        Err(map::Error::WorkLimit)
    ));
    assert_eq!(meter.used(), 0);
    drain(&mut table, &mut allocator);
    assert_eq!(meter.used(), 0);
    empty(&allocator);
}
