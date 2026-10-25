use super::*;

#[test]
fn mixed_push_pop_and_root_replacement_match_an_independent_value_and_retirement_order_oracle() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector =
        v::Vector::new_with_limits(&layouts, ty(0), v::Limits { loans: 1 }, &mut a).unwrap();
    let mut values: Vec<(u8, m::Span)> = Vec::new();
    let mut order: Vec<usize> = Vec::new();
    for step in 0u8..96 {
        let initializer = span(u32::from(step % 20));
        match step % 7 {
            0..=2 => {
                let mut incoming = l::Reservation::allocate(&layouts, ty(0), &mut a).unwrap();
                let mut b = buffer::Bytes::from_slice(&mut a, &[step]).unwrap();
                let token = b.take_token(&a).unwrap();
                incoming.data_mut(&mut a).unwrap().copy_from_slice(&token);
                vector
                    .push_from(&mut incoming, Some(initializer), &mut a)
                    .unwrap();
                incoming.release_storage(&mut a).unwrap();
                order.push(values.len());
                values.push((step, initializer));
            }
            3 | 4 => {
                let index = (usize::from(step) * 7) % values.len();
                let mut incoming = l::Reservation::allocate(&layouts, ty(0), &mut a).unwrap();
                let mut b = buffer::Bytes::from_slice(&mut a, &[step]).unwrap();
                let token = b.take_token(&a).unwrap();
                incoming.data_mut(&mut a).unwrap().copy_from_slice(&token);
                let calls = t.borrow().requests.len();
                let mut write = vector
                    .borrow(index as i64, o::LoanKind::Exclusive, &mut a)
                    .unwrap();
                let root = write.view(&layouts, &a).unwrap().root();
                assert_eq!(
                    write
                        .replace_from(&layouts, root, &mut incoming, initializer, &mut a)
                        .unwrap(),
                    Some(values[index].1)
                );
                write.end(&layouts, &mut a).unwrap();
                let mut old = incoming.read_token(incoming.root(), &a).unwrap();
                let observed = buffer::Bytes::from_token(&mut old.bytes, &a).unwrap();
                assert_eq!(observed.as_slice(&a).unwrap(), [values[index].0]);
                let report = context(|ctx| incoming.cleanup(&mut a, ctx, bytes).map_err(|_| 99));
                assert!(report.primary().is_none());
                assert_eq!(t.borrow().requests.len(), calls);
                values[index] = (step, initializer);
                order.retain(|i| *i != index);
                order.push(index);
            }
            _ => {
                let output = l::Reservation::allocate(&layouts, ty(0), &mut a).unwrap();
                let calls = t.borrow().requests.len();
                assert!(vector.pop_into(&output, output.root(), &mut a).unwrap());
                let expected = values.pop().unwrap();
                order.retain(|i| *i != values.len());
                let mut token = output.read_token(output.root(), &a).unwrap();
                let b = buffer::Bytes::from_token(&mut token.bytes, &a).unwrap();
                assert_eq!(b.as_slice(&a).unwrap(), [expected.0]);
                let mut output = output;
                let report = context(|ctx| output.cleanup(&mut a, ctx, bytes).map_err(|_| 99));
                assert!(report.primary().is_none());
                assert_eq!(t.borrow().requests.len(), calls);
            }
        }
        assert_eq!(vector.len(&a).unwrap(), values.len() as i64);
        for (index, expected) in values.iter().enumerate() {
            let view = vector.view(index as i64, &a).unwrap();
            let mut token = view.read_token(view.root()).unwrap();
            let b = buffer::Bytes::from_token(&mut token.bytes, &a).unwrap();
            assert_eq!(b.as_slice(&a).unwrap(), [expected.0]);
        }
    }
    let mut seen = Vec::new();
    let report = context(|ctx| {
        vector
            .cleanup(&mut a, ctx, |token, a, _, _| leaf(token, a, &mut seen))
            .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    let expected = order
        .into_iter()
        .rev()
        .map(|i| (values[i].0, Some(values[i].1)))
        .collect::<Vec<_>>();
    assert_eq!(seen, expected);
    empty(&a);
}

fn leaf(
    token: &mut l::Token,
    a: &mut a::Allocator<Provider>,
    seen: &mut Vec<(u8, Option<m::Span>)>,
) -> Result<(), u32> {
    let mut b = buffer::Bytes::from_token(&mut token.bytes, a).unwrap();
    seen.push((b.as_slice(a).unwrap()[0], token.leaf.span));
    b.release(a).unwrap();
    Ok(())
}
fn contents(value: &l::Reservation<'_, '_>, a: &a::Allocator<Provider>) -> [u8; 2] {
    let pair = value.child(value.root(), 0, a).unwrap();
    std::array::from_fn(|i| {
        let mut token = value
            .read_token(value.child(pair, i as u32, a).unwrap(), a)
            .unwrap();
        let b = buffer::Bytes::from_token(&mut token.bytes, a).unwrap();
        b.as_slice(a).unwrap()[0]
    })
}
fn stored<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    a: &mut a::Allocator<Provider>,
    values: &[u8],
) -> v::Vector<'l, 't> {
    let mut vector = v::Vector::new_with_limits(layouts, ty(1), v::Limits { loans: 2 }, a).unwrap();
    for first in values {
        let mut incoming = input(layouts, a, *first);
        vector.push_from(&mut incoming, Some(span(2)), a).unwrap();
        incoming.release_storage(a).unwrap();
    }
    vector
}

#[test]
fn checked_element_loan_pop_and_replacement_share_one_allocation_free_lifecycle() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector =
        v::Vector::new_with_limits(&layouts, o::Type::I64, v::Limits { loans: 2 }, &mut a).unwrap();
    let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    value
        .write_immediate(value.root(), l::Immediate::I64(9), &mut a)
        .unwrap();
    vector.push_copy(&mut value, None, &mut a).unwrap();
    value
        .write_immediate(value.root(), l::Immediate::I64(4), &mut a)
        .unwrap();
    let calls = t.borrow().requests.len();
    let mut read = vector.borrow(0, o::LoanKind::Shared, &mut a).unwrap();
    let view = read.view(&layouts, &a).unwrap();
    assert_eq!(
        view.read_immediate(view.root(), &a).unwrap(),
        l::Immediate::I64(9)
    );
    assert_eq!(
        vector.pop_into(&value, value.root(), &mut a),
        Err(v::Error::BorrowConflict)
    );
    assert_eq!(vector.take_token(&a), Err(v::Error::BorrowConflict));
    read.end(&layouts, &mut a).unwrap();
    assert_eq!(read.end(&layouts, &mut a), Err(v::Error::Unavailable));
    let mut write = vector.borrow(0, o::LoanKind::Exclusive, &mut a).unwrap();
    let at = write.view(&layouts, &a).unwrap().root();
    vector
        .replace_copy(&write, at, &value, value.root(), span(0), &mut a)
        .unwrap();
    write.end(&layouts, &mut a).unwrap();
    assert!(vector.pop_into(&value, value.root(), &mut a).unwrap());
    assert_eq!(
        value.read_immediate(value.root(), &a).unwrap(),
        l::Immediate::I64(4)
    );
    assert!(!vector.pop_into(&value, value.root(), &mut a).unwrap());
    assert_eq!(t.borrow().requests.len(), calls);
    value.release_storage(&mut a).unwrap();
    drain(&mut vector, &mut a);
    empty(&a);
}

#[test]
fn owning_pop_removes_the_last_index_from_the_replacement_order_and_moves_only_that_element() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector = stored(&layouts, &mut a, &[10, 20, 30]);
    let mut incoming = input(&layouts, &mut a, 40);
    let output = l::Reservation::allocate(&layouts, ty(1), &mut a).unwrap();
    let calls = t.borrow().requests.len();
    let mut write = vector.borrow(0, o::LoanKind::Exclusive, &mut a).unwrap();
    let root = write.view(&layouts, &a).unwrap().root();
    assert_eq!(
        vector
            .replace_from(&write, root, &mut incoming, span(3), &mut a)
            .unwrap(),
        Some(span(2))
    );
    assert_eq!(contents(&incoming, &a), [10, 11]);
    write.end(&layouts, &mut a).unwrap();
    assert!(vector.pop_into(&output, output.root(), &mut a).unwrap());
    assert_eq!(contents(&output, &a), [30, 31]);
    assert_eq!(inspect(&vector, 0, &a), [40, 41]);
    assert_eq!(inspect(&vector, 1, &a), [20, 21]);
    assert_eq!(vector.len(&a).unwrap(), 2);
    let mut seen = Vec::new();
    let mut output = output;
    let report = context(|ctx| {
        incoming
            .cleanup(&mut a, ctx, |token, a, _, _| leaf(token, a, &mut seen))
            .unwrap();
        vector
            .cleanup(&mut a, ctx, |token, a, _, _| leaf(token, a, &mut seen))
            .unwrap();
        output
            .cleanup(&mut a, ctx, |token, a, _, _| leaf(token, a, &mut seen))
            .unwrap();
        Ok(())
    });
    assert!(report.primary().is_none());
    assert_eq!(
        seen.iter().map(|v| v.0).collect::<Vec<_>>(),
        [10, 11, 40, 41, 20, 21, 30, 31]
    );
    assert_eq!(t.borrow().requests.len(), calls);
    empty(&a);
}

#[test]
fn projected_owning_replacement_updates_only_its_field_ledger_and_returns_the_old_span() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector = stored(&layouts, &mut a, &[10, 20]);
    let mut incoming = l::Reservation::allocate(&layouts, ty(0), &mut a).unwrap();
    let mut bytes = buffer::Bytes::from_slice(&mut a, &[99]).unwrap();
    let token = bytes.take_token(&a).unwrap();
    incoming.data_mut(&mut a).unwrap().copy_from_slice(&token);
    let calls = t.borrow().requests.len();
    let mut write = vector.borrow(0, o::LoanKind::Exclusive, &mut a).unwrap();
    let view = write.view(&layouts, &a).unwrap();
    let pair = view.child(view.root(), 0, &a).unwrap();
    let field = view.child(pair, 1, &a).unwrap();
    assert_eq!(
        write
            .replace_from(&layouts, field, &mut incoming, span(3), &mut a)
            .unwrap(),
        Some(span(1))
    );
    write.end(&layouts, &mut a).unwrap();
    assert_eq!(inspect(&vector, 0, &a), [10, 99]);
    let mut seen = Vec::new();
    let report = context(|ctx| {
        incoming
            .cleanup(&mut a, ctx, |token, a, _, _| leaf(token, a, &mut seen))
            .unwrap();
        vector
            .cleanup(&mut a, ctx, |token, a, _, _| leaf(token, a, &mut seen))
            .unwrap();
        Ok(())
    });
    assert!(report.primary().is_none());
    assert_eq!(
        seen.iter().map(|v| v.0).collect::<Vec<_>>(),
        [11, 20, 21, 99, 10]
    );
    assert_eq!(seen[3].1, Some(span(3)));
    assert_eq!(seen[4].1, Some(span(0)));
    assert_eq!(t.borrow().requests.len(), calls);
    empty(&a);
}

#[test]
fn bounded_shared_and_exclusive_registrations_reject_aliases_and_reuse_without_effects() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector =
        v::Vector::new_with_limits(&layouts, o::Type::I64, v::Limits { loans: 2 }, &mut a).unwrap();
    let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    vector.push_copy(&mut value, None, &mut a).unwrap();
    let calls = t.borrow().requests.len();
    for index in [-1, 1, i64::MAX] {
        assert!(matches!(
            vector.borrow(index, o::LoanKind::Shared, &mut a),
            Err(v::Error::Bounds(buffer::BoundsError::OutOfBounds))
        ));
    }
    let mut first = vector.borrow(0, o::LoanKind::Shared, &mut a).unwrap();
    let mut second = vector.borrow(0, o::LoanKind::Shared, &mut a).unwrap();
    assert!(matches!(
        vector.borrow(0, o::LoanKind::Shared, &mut a),
        Err(v::Error::LoanLimit)
    ));
    assert!(matches!(
        vector.borrow(0, o::LoanKind::Exclusive, &mut a),
        Err(v::Error::BorrowConflict)
    ));
    assert_eq!(
        vector.push_copy(&mut value, None, &mut a),
        Err(v::Error::BorrowConflict)
    );
    context(|ctx| {
        assert_eq!(
            vector.cleanup(&mut a, ctx, bytes),
            Err(v::Error::BorrowConflict)
        );
        Ok(())
    });
    first.end(&layouts, &mut a).unwrap();
    let mut replacement = vector.borrow(0, o::LoanKind::Shared, &mut a).unwrap();
    assert!(matches!(
        first.view(&layouts, &a),
        Err(v::Error::Unavailable)
    ));
    assert_eq!(first.end(&layouts, &mut a), Err(v::Error::Unavailable));
    replacement.end(&layouts, &mut a).unwrap();
    second.end(&layouts, &mut a).unwrap();
    for _ in 0..64 {
        let mut write = vector.borrow(0, o::LoanKind::Exclusive, &mut a).unwrap();
        assert!(matches!(
            vector.borrow(0, o::LoanKind::Shared, &mut a),
            Err(v::Error::BorrowConflict)
        ));
        assert_eq!(vector.len(&a), Err(v::Error::BorrowConflict));
        assert!(matches!(vector.view(0, &a), Err(v::Error::BorrowConflict)));
        let view = write.view(&layouts, &a).unwrap();
        view.copy_into(view.root(), &value, value.root(), &mut a)
            .unwrap();
        write.end(&layouts, &mut a).unwrap();
    }
    assert_eq!(t.borrow().requests.len(), calls);
    value.release_storage(&mut a).unwrap();
    drain(&mut vector, &mut a);
    empty(&a);
}

#[test]
fn abort_revokes_live_borrowed_views_and_drains_every_owning_element_without_new_requests() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let other_layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector = stored(&layouts, &mut a, &[10, 20]);
    let mut read = vector.borrow(0, o::LoanKind::Shared, &mut a).unwrap();
    assert!(read.view(&other_layouts, &a).is_err());
    assert!(read.end(&other_layouts, &mut a).is_err());
    let view = read.view(&layouts, &a).unwrap();
    let pair = view.child(view.root(), 0, &a).unwrap();
    let at = view.child(pair, 0, &a).unwrap();
    let calls = t.borrow().requests.len();
    let mut seen = Vec::new();
    let report = context(|ctx| {
        vector
            .abort_cleanup(&mut a, ctx, |token, a, _, _| leaf(token, a, &mut seen))
            .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    assert_eq!(
        seen.iter().map(|v| v.0).collect::<Vec<_>>(),
        [20, 21, 10, 11]
    );
    assert!(view.read_token(at, &a).is_err());
    assert!(read.end(&layouts, &mut a).is_err());
    assert_eq!(t.borrow().requests.len(), calls);
    empty(&a);
}

#[test]
fn failed_pop_and_replacement_preserve_published_values_and_incoming_owners() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector = stored(&layouts, &mut a, &[10, 20]);
    let peer = vector.view(1, &a).unwrap().root();
    let mut incoming = input(&layouts, &mut a, 30);
    let original = incoming.data(&a).unwrap().to_vec();
    let mut wrong = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    let calls = t.borrow().requests.len();
    assert!(vector.pop_into(&wrong, wrong.root(), &mut a).is_err());
    assert_eq!(vector.len(&a).unwrap(), 2);
    let mut read = vector.borrow(0, o::LoanKind::Shared, &mut a).unwrap();
    let root = read.view(&layouts, &a).unwrap().root();
    assert_eq!(
        vector.replace_from(&read, root, &mut incoming, span(3), &mut a),
        Err(v::Error::BorrowConflict)
    );
    read.end(&layouts, &mut a).unwrap();
    let mut write = vector.borrow(0, o::LoanKind::Exclusive, &mut a).unwrap();
    let root = write.view(&layouts, &a).unwrap().root();
    assert_eq!(
        vector.replace_from(&write, root, &mut incoming, span(99), &mut a),
        Err(v::Error::Layout(l::Error::InvalidSpan))
    );
    assert_eq!(
        vector.replace_from(&write, peer, &mut incoming, span(3), &mut a),
        Err(v::Error::Layout(l::Error::ForeignLocation))
    );
    assert!(
        vector
            .replace_copy(&write, root, &wrong, wrong.root(), span(3), &mut a)
            .is_err()
    );
    assert_eq!(incoming.data(&a).unwrap(), original);
    write.end(&layouts, &mut a).unwrap();
    assert_eq!(inspect(&vector, 0, &a), [10, 11]);
    assert_eq!(inspect(&vector, 1, &a), [20, 21]);
    assert_eq!(t.borrow().requests.len(), calls);
    wrong.release_storage(&mut a).unwrap();
    let report = context(|ctx| incoming.cleanup(&mut a, ctx, bytes).map_err(|_| 99));
    assert!(report.primary().is_none());
    drain(&mut vector, &mut a);
    empty(&a);
}

#[test]
fn constructor_loan_pool_limits_and_oom_fail_before_publication_and_zero_slots_stay_valid() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector =
        v::Vector::new_with_limits(&layouts, o::Type::I64, v::Limits { loans: 2 }, &mut a).unwrap();
    let size = t.borrow().requests[0].bytes();
    assert_eq!(size, 128 + 2 * 24);
    drain(&mut vector, &mut a);
    empty(&a);
    for which in [
        a::Limit::RequestBytes,
        a::Limit::LiveBytes,
        a::Limit::CumulativeBytes,
        a::Limit::LiveAllocations,
    ] {
        for short in [false, true] {
            let mut limits = a::Limits::default();
            match which {
                a::Limit::RequestBytes => limits.request_bytes = size - u64::from(short),
                a::Limit::LiveBytes => limits.live_bytes = size - u64::from(short),
                a::Limit::CumulativeBytes => limits.cumulative_bytes = size - u64::from(short),
                a::Limit::LiveAllocations => limits.live_allocations = usize::from(!short),
                _ => unreachable!(),
            };
            let (mut a, t) = allocator(limits);
            let result =
                v::Vector::new_with_limits(&layouts, o::Type::I64, v::Limits { loans: 2 }, &mut a);
            if short {
                assert!(
                    matches!(result,Err(v::Error::Allocation(a::Error::Limit(actual)))if actual.kind==which&&actual.attempted>actual.limit)
                );
                assert!(t.borrow().requests.is_empty());
            } else {
                let mut v = result.unwrap();
                drain(&mut v, &mut a);
            }
            empty(&a);
        }
    }
    let (mut a, t) = allocator(Default::default());
    assert!(matches!(
        v::Vector::new_with_limits(
            &layouts,
            o::Type::I64,
            v::Limits { loans: usize::MAX },
            &mut a
        ),
        Err(v::Error::Allocation(a::Error::Overflow))
    ));
    assert!(t.borrow().requests.is_empty());
    t.borrow_mut().fail = Some(1);
    assert!(matches!(
        v::Vector::new_with_limits(&layouts, o::Type::I64, v::Limits { loans: 2 }, &mut a),
        Err(v::Error::Allocation(a::Error::OutOfMemory))
    ));
    empty(&a);
    t.borrow_mut().fail = None;
    let mut vector =
        v::Vector::new_with_limits(&layouts, o::Type::I64, v::Limits { loans: 0 }, &mut a).unwrap();
    let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    vector.push_copy(&mut value, None, &mut a).unwrap();
    let before = a.stats();
    assert!(matches!(
        vector.borrow(0, o::LoanKind::Shared, &mut a),
        Err(v::Error::LoanLimit)
    ));
    assert_eq!(a.stats(), before);
    value.release_storage(&mut a).unwrap();
    drain(&mut vector, &mut a);
    empty(&a);
}

#[test]
fn displaced_cleanup_failure_keeps_new_elements_published_and_abort_still_drains_all_peers() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector = stored(&layouts, &mut a, &[10, 20]);
    let mut incoming = input(&layouts, &mut a, 30);
    let write = vector.borrow(0, o::LoanKind::Exclusive, &mut a).unwrap();
    let root = write.view(&layouts, &a).unwrap().root();
    vector
        .replace_from(&write, root, &mut incoming, span(3), &mut a)
        .unwrap();
    let calls = t.borrow().requests.len();
    let mut seen = Vec::new();
    let report = context(|ctx| {
        incoming
            .cleanup(&mut a, ctx, |token, a, _, _| {
                leaf(token, a, &mut seen)?;
                if seen.len() == 1 {
                    Err(700)
                } else {
                    panic!("injected displaced payload cleanup");
                }
            })
            .unwrap();
        let view = write.view(&layouts, &a).unwrap();
        assert_eq!(view.case(view.root(), &a).unwrap(), 1);
        vector
            .abort_cleanup(&mut a, ctx, |token, a, _, _| leaf(token, a, &mut seen))
            .unwrap();
        Ok(())
    });
    assert_eq!(report.primary().unwrap().kind, c::FailureKind::Cleanup(700));
    assert_eq!(report.secondary()[0].kind, c::FailureKind::CleanupPanic);
    assert_eq!(
        seen.iter().map(|v| v.0).collect::<Vec<_>>(),
        [10, 11, 30, 31, 20, 21]
    );
    assert_eq!(t.borrow().requests.len(), calls);
    empty(&a);
}
