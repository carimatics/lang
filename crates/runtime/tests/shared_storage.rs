use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{allocation as a, buffer, cleanup as c, layout as l, shared::storage as s};
use std::{cell::RefCell, rc::Rc};

fn span(start: u32) -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start,
        end: start + 1,
    }
}
fn model() -> o::VerifiedTypes {
    o::TypeTable {
        sources: vec![m::Source {
            path: "shared.lang".into(),
            text: "abcd".into(),
        }],
        constructed: vec![o::Constructed {
            constructor: o::Constructor::Builtin(o::Builtin::Bytes),
            span: span(0),
        }],
        definitions: vec![o::Definition {
            name: "Pair".into(),
            public: false,
            span: m::Span {
                source: m::SourceId(0),
                start: 0,
                end: 4,
            },
            kind: o::DefinitionKind::Record(
                (0..2)
                    .map(|i| o::Field {
                        name: format!("field{i}"),
                        span: span(i),
                        ty: o::TypeUse {
                            ty: o::Type::Constructed(o::ConstructedId(0)),
                            span: span(i),
                        },
                    })
                    .collect(),
            ),
        }],
    }
    .verify(Default::default())
    .unwrap()
}
fn pair() -> o::Type {
    o::Type::Nominal(m::DefinitionId(0))
}
fn input<'l, 't, P: a::Provider>(
    layouts: &'l l::Layouts<'t>,
    allocator: &mut a::Allocator<P>,
    values: [u8; 2],
) -> l::Reservation<'l, 't> {
    let value = l::Reservation::allocate(layouts, pair(), allocator).unwrap();
    for (i, byte) in values.into_iter().enumerate() {
        let location = value.child(value.root(), i as u32, allocator).unwrap();
        let mut bytes = buffer::Bytes::from_slice(allocator, &[byte]).unwrap();
        let offset = location.offset() as usize;
        let token = bytes.take_token(allocator).unwrap();
        value.data_mut(allocator).unwrap()[offset..offset + 32].copy_from_slice(&token);
    }
    value
        .initialize_order(
            value.root(),
            [(1, span(1)), (0, span(0))].into_iter(),
            allocator,
        )
        .unwrap();
    value
}
#[derive(Default)]
struct Trace {
    requests: Vec<a::Request>,
    releases: Vec<a::Request>,
    fail: Option<usize>,
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        let mut trace = self.0.borrow_mut();
        trace.requests.push(request);
        if trace.fail == Some(trace.requests.len()) {
            return None;
        }
        a::Storage::try_new(request).ok()
    }
    fn deallocate(&mut self, region: Self::Region, request: a::Request) {
        self.0.borrow_mut().releases.push(request);
        drop(region);
    }
}
fn allocator(limits: a::Limits) -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    (
        a::Allocator::new(Provider(trace.clone()), limits).unwrap(),
        trace,
    )
}
fn context(mut clean: impl FnMut(&mut c::Context<'_>) -> Result<(), u32>) -> c::Report {
    let mut instance = c::Instance::default();
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |_: &mut (), _, ctx: &mut c::Context<'_>| clean(ctx),
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    frame.initialize(c::Slot(0), (), Some(span(3))).unwrap();
    frame.finish(c::Exit::Return(None)).report
}
fn bytes<P: a::Provider>(
    token: &mut l::Token,
    allocator: &mut a::Allocator<P>,
    _: c::Reason,
    _: &mut c::Context<'_>,
) -> Result<(), u32> {
    let mut value = buffer::Bytes::from_token(&mut token.bytes, allocator).unwrap();
    value.release(allocator).unwrap();
    Ok(())
}
fn drain<P: a::Provider>(block: &mut s::Block<'_, '_>, allocator: &mut a::Allocator<P>) {
    assert_eq!(block.release_strong(allocator).unwrap(), s::Release::Retire);
    let report = context(|ctx| block.cleanup(allocator, ctx, bytes).map_err(|_| 99));
    assert!(report.primary().is_none(), "{report:?}");
}

#[test]
fn every_control_payload_and_metadata_oom_preserves_input_and_existing_shared_value() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for position in 1..=3 {
        let (mut allocator, trace) = allocator(Default::default());
        let mut first = input(&layouts, &mut allocator, [1, 2]);
        let mut published =
            s::Block::allocate_from(&mut first, Some(span(2)), &mut allocator).unwrap();
        first.release_storage(&mut allocator).unwrap();
        let existing = published
            .payload(&allocator)
            .unwrap()
            .data(&allocator)
            .unwrap()
            .to_vec();
        let mut incoming = input(&layouts, &mut allocator, [7, 8]);
        let original = incoming.data(&allocator).unwrap().to_vec();
        let before = allocator.stats();
        let requests = trace.borrow().requests.len();
        trace.borrow_mut().fail = Some(requests + position);
        let error = s::Block::allocate_from(&mut incoming, Some(span(3)), &mut allocator)
            .err()
            .unwrap();
        assert_eq!(error, s::Error::Allocation(a::Error::OutOfMemory));
        assert_eq!(incoming.data(&allocator).unwrap(), original);
        assert_eq!(
            published
                .payload(&allocator)
                .unwrap()
                .data(&allocator)
                .unwrap(),
            existing
        );
        assert_eq!(allocator.stats().live_bytes, before.live_bytes);
        assert_eq!(allocator.stats().live_allocations, before.live_allocations);
        assert_eq!(
            allocator.stats().requests,
            before.requests + position as u64
        );
        assert!(allocator.stats().cumulative_bytes > before.cumulative_bytes);
        trace.borrow_mut().fail = None;
        let mut retried =
            s::Block::allocate_from(&mut incoming, Some(span(3)), &mut allocator).unwrap();
        incoming.release_storage(&mut allocator).unwrap();
        drain(&mut published, &mut allocator);
        drain(&mut retried, &mut allocator);
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
    }
}

#[test]
fn actual_guest_control_payload_and_retirement_storage_survive_until_their_last_obligation() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut value = input(&layouts, &mut allocator, [4, 5]);
    let before = allocator.stats();
    let mut block = s::Block::allocate_from(&mut value, Some(span(2)), &mut allocator).unwrap();
    let plan = layouts.get(pair()).unwrap().storage_requests().unwrap();
    assert_eq!(
        allocator.stats().live_bytes - before.live_bytes,
        56 + plan[0].bytes() + plan[1].bytes()
    );
    assert_eq!(
        allocator.stats().live_allocations - before.live_allocations,
        3
    );
    value.release_storage(&mut allocator).unwrap();
    let header = block.identity().unwrap();
    block.downgrade(9, &mut allocator).unwrap();
    let words = allocator.bytes(header).unwrap();
    assert_eq!(u64::from_le_bytes(words[16..24].try_into().unwrap()), 1);
    assert_eq!(u64::from_le_bytes(words[24..32].try_into().unwrap()), 1);
    let requests = trace.borrow().requests.len();
    drain(&mut block, &mut allocator);
    assert_eq!(trace.borrow().requests.len(), requests);
    assert_eq!(allocator.stats().live_bytes, 56);
    assert_eq!(allocator.stats().live_allocations, 1);
    assert_eq!(block.upgrade(9, &mut allocator), Err(s::Error::ExpiredWeak));
    assert!(block.payload(&allocator).is_err());
    block.release_weak(&mut allocator).unwrap();
    assert_eq!(allocator.bytes(header), Err(a::Error::StaleHandle));
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn read_leases_preserve_payload_exclude_replacement_and_revoke_before_cleanup() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, _) = allocator(Default::default());
    let mut value = input(&layouts, &mut allocator, [4, 5]);
    let mut block = s::Block::allocate_from(&mut value, Some(span(2)), &mut allocator).unwrap();
    value.release_storage(&mut allocator).unwrap();
    let mut incoming = input(&layouts, &mut allocator, [7, 8]);
    let original = incoming.data(&allocator).unwrap().to_vec();
    block.downgrade(9, &mut allocator).unwrap();
    block.begin_read(9, &mut allocator).unwrap();
    assert_eq!(
        block.replace_from(&mut incoming, span(3), &mut allocator),
        Err(s::Error::BorrowConflict)
    );
    assert_eq!(incoming.data(&allocator).unwrap(), original);
    assert_eq!(
        block.release_strong(&mut allocator).unwrap(),
        s::Release::Retained
    );
    assert_eq!(block.release_strong(&mut allocator), Err(s::Error::State));
    assert!(block.payload(&allocator).is_ok());
    block.upgrade(9, &mut allocator).unwrap();
    assert_eq!(
        block.release_strong(&mut allocator).unwrap(),
        s::Release::Retained
    );
    assert_eq!(block.end_read(&mut allocator).unwrap(), s::Release::Retire);
    assert_eq!(block.upgrade(9, &mut allocator), Err(s::Error::ExpiredWeak));
    assert_eq!(block.end_read(&mut allocator), Err(s::Error::Unavailable));
    context(|ctx| block.cleanup(&mut allocator, ctx, bytes).map_err(|_| 99));
    block.release_weak(&mut allocator).unwrap();
    context(|ctx| incoming.cleanup(&mut allocator, ctx, bytes).map_err(|_| 99));
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn count_refusals_leave_both_read_and_strong_counts_unchanged() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, _) = allocator(Default::default());
    let mut value = input(&layouts, &mut allocator, [4, 5]);
    let mut block = s::Block::allocate_from(&mut value, Some(span(2)), &mut allocator).unwrap();
    value.release_storage(&mut allocator).unwrap();
    let before = block.counts(&allocator).unwrap();
    assert_eq!(
        block.retain_strong(1, &mut allocator),
        Err(s::Error::CountOverflow)
    );
    assert_eq!(
        block.downgrade(0, &mut allocator),
        Err(s::Error::CountOverflow)
    );
    assert_eq!(
        block.begin_read(1, &mut allocator),
        Err(s::Error::CountOverflow)
    );
    assert_eq!(block.counts(&allocator).unwrap(), before);
    block.begin_read(2, &mut allocator).unwrap();
    let before = block.counts(&allocator).unwrap();
    assert_eq!(
        block.begin_read(2, &mut allocator),
        Err(s::Error::CountOverflow)
    );
    assert_eq!(block.counts(&allocator).unwrap(), before);
    assert_eq!(
        block.end_read(&mut allocator).unwrap(),
        s::Release::Retained
    );
    drain(&mut block, &mut allocator);
}

#[test]
fn replacement_publishes_without_allocation_and_detaches_old_values_in_initializer_order() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut value = input(&layouts, &mut allocator, [4, 5]);
    let mut block = s::Block::allocate_from(&mut value, Some(span(2)), &mut allocator).unwrap();
    value.release_storage(&mut allocator).unwrap();
    let mut incoming = input(&layouts, &mut allocator, [7, 8]);
    let new_bytes = incoming.data(&allocator).unwrap().to_vec();
    let before = trace.borrow().requests.len();
    assert_eq!(
        block
            .replace_from(&mut incoming, span(3), &mut allocator)
            .unwrap(),
        Some(span(2))
    );
    assert_eq!(block.span(&allocator).unwrap(), Some(span(3)));
    assert_eq!(
        block.payload(&allocator).unwrap().data(&allocator).unwrap(),
        new_bytes
    );
    let mut seen = Vec::new();
    let report = context(|ctx| {
        incoming
            .cleanup(&mut allocator, ctx, |token, alloc, _, _| {
                let mut value = buffer::Bytes::from_token(&mut token.bytes, alloc).unwrap();
                seen.push((value.as_slice(alloc).unwrap()[0], token.leaf.span));
                value.release(alloc).unwrap();
                Ok(())
            })
            .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    assert_eq!(seen, [(4, Some(span(0))), (5, Some(span(1)))]);
    assert_eq!(trace.borrow().requests.len(), before);
    drain(&mut block, &mut allocator);
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn copy_operands_remain_reusable_and_owning_copy_rejection_precedes_provider_calls() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut scalar = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    scalar
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&9i64.to_le_bytes());
    let mut block = s::Block::allocate_copy(&mut scalar, Some(span(0)), &mut allocator).unwrap();
    assert_eq!(scalar.data(&allocator).unwrap(), 9i64.to_le_bytes());
    assert_eq!(
        block.payload(&allocator).unwrap().data(&allocator).unwrap(),
        scalar.data(&allocator).unwrap()
    );
    let mut owner = input(&layouts, &mut allocator, [4, 5]);
    let before = trace.borrow().requests.len();
    assert_eq!(
        s::Block::allocate_copy(&mut owner, None, &mut allocator)
            .err()
            .unwrap(),
        s::Error::Layout(l::Error::InvalidType(pair()))
    );
    assert_eq!(trace.borrow().requests.len(), before);
    scalar.release_storage(&mut allocator).unwrap();
    drain(&mut block, &mut allocator);
    context(|ctx| owner.cleanup(&mut allocator, ctx, bytes).map_err(|_| 99));
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn complete_plan_checks_exact_and_one_below_live_cumulative_and_count_limits_before_callbacks() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut probe, _) = allocator(Default::default());
    let mut value = input(&layouts, &mut probe, [4, 5]);
    let baseline = probe.stats();
    let plan = layouts.get(pair()).unwrap().storage_requests().unwrap();
    let added = 56 + plan[0].bytes() + plan[1].bytes();
    context(|ctx| value.cleanup(&mut probe, ctx, bytes).map_err(|_| 99));
    for limit in [
        a::Limit::LiveBytes,
        a::Limit::CumulativeBytes,
        a::Limit::LiveAllocations,
    ] {
        for below in [false, true] {
            let mut limits = a::Limits::default();
            let subtract = u64::from(below);
            match limit {
                a::Limit::LiveBytes => limits.live_bytes = baseline.live_bytes + added - subtract,
                a::Limit::CumulativeBytes => {
                    limits.cumulative_bytes = baseline.cumulative_bytes + added - subtract
                }
                a::Limit::LiveAllocations => {
                    limits.live_allocations = baseline.live_allocations + 3 - usize::from(below)
                }
                _ => unreachable!(),
            }
            let (mut allocator, trace) = allocator(limits);
            let mut value = input(&layouts, &mut allocator, [4, 5]);
            let original = value.data(&allocator).unwrap().to_vec();
            let before = allocator.stats();
            let calls = trace.borrow().requests.len();
            let result = s::Block::allocate_from(&mut value, Some(span(2)), &mut allocator);
            if below {
                assert_eq!(
                    result.err().unwrap(),
                    s::Error::Allocation(a::Error::Limit(limit))
                );
                assert_eq!(allocator.stats(), before);
                assert_eq!(trace.borrow().requests.len(), calls);
                assert_eq!(value.data(&allocator).unwrap(), original);
                context(|ctx| value.cleanup(&mut allocator, ctx, bytes).map_err(|_| 99));
            } else {
                let mut block = result.unwrap();
                value.release_storage(&mut allocator).unwrap();
                drain(&mut block, &mut allocator);
            }
            assert_eq!(allocator.stats().live_bytes, 0);
        }
    }
}

#[test]
fn invalid_spans_types_and_allocator_authority_leave_payloads_and_counts_unchanged() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut owner = input(&layouts, &mut allocator, [4, 5]);
    let original = owner.data(&allocator).unwrap().to_vec();
    let calls = trace.borrow().requests.len();
    let invalid = m::Span {
        source: m::SourceId(1),
        start: 0,
        end: 1,
    };
    assert_eq!(
        s::Block::allocate_from(&mut owner, Some(invalid), &mut allocator)
            .err()
            .unwrap(),
        s::Error::Layout(l::Error::InvalidSpan)
    );
    assert_eq!(trace.borrow().requests.len(), calls);
    assert_eq!(owner.data(&allocator).unwrap(), original);
    let mut block = s::Block::allocate_from(&mut owner, Some(span(2)), &mut allocator).unwrap();
    owner.release_storage(&mut allocator).unwrap();
    let counts = block.counts(&allocator).unwrap();
    let mut other = a::Allocator::new(a::System, Default::default()).unwrap();
    assert_eq!(
        block.begin_read(9, &mut other),
        Err(s::Error::Allocation(a::Error::ForeignAllocator))
    );
    assert_eq!(other.stats().requests, 0);
    let mut wrong = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    wrong
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&9i64.to_le_bytes());
    let before = allocator.stats();
    assert!(matches!(
        block.replace_from(&mut wrong, span(3), &mut allocator),
        Err(s::Error::Layout(l::Error::InvalidType(o::Type::I64)))
    ));
    assert_eq!(allocator.stats(), before);
    assert_eq!(block.counts(&allocator).unwrap(), counts);
    assert_eq!(block.span(&allocator).unwrap(), Some(span(2)));
    assert_eq!(
        block.payload(&allocator).unwrap().data(&allocator).unwrap(),
        original
    );
    assert_eq!(wrong.data(&allocator).unwrap(), 9i64.to_le_bytes());
    wrong.release_storage(&mut allocator).unwrap();
    drain(&mut block, &mut allocator);
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn detached_cleanup_can_release_weak_counts_but_only_a_completed_drain_reclaims_control() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut value = input(&layouts, &mut allocator, [4, 5]);
    let mut block = s::Block::allocate_from(&mut value, Some(span(2)), &mut allocator).unwrap();
    value.release_storage(&mut allocator).unwrap();
    block.downgrade(2, &mut allocator).unwrap();
    block.retain_weak(2, &mut allocator).unwrap();
    assert_eq!(
        block.release_strong(&mut allocator).unwrap(),
        s::Release::Retire
    );
    assert_eq!(
        block.downgrade(2, &mut allocator),
        Err(s::Error::Unavailable)
    );
    let mut retired = block.detach(&allocator).unwrap();
    assert!(block.detach(&allocator).is_err());
    let header = block.identity().unwrap();
    let calls = trace.borrow().requests.len();
    let mut completed = None;
    let report = context(|ctx| {
        completed = Some(
            retired
                .cleanup(&mut allocator, ctx, |token, alloc, reason, ctx| {
                    assert_eq!(block.upgrade(9, alloc), Err(s::Error::ExpiredWeak));
                    block.release_weak(alloc).unwrap();
                    assert!(alloc.request(header).is_ok());
                    bytes(token, alloc, reason, ctx)
                })
                .unwrap(),
        );
        Ok(())
    });
    assert!(report.primary().is_none());
    assert_eq!(allocator.stats().live_bytes, 56);
    assert_eq!(block.counts(&allocator).unwrap().weak, 0);
    context(|ctx| {
        assert!(retired.cleanup(&mut allocator, ctx, bytes).is_err());
        Ok(())
    });
    block
        .complete_retirement(completed.unwrap(), &mut allocator)
        .unwrap();
    assert_eq!(allocator.bytes(header), Err(a::Error::StaleHandle));
    assert_eq!(trace.borrow().requests.len(), calls);
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn cleanup_failure_and_panic_preserve_span_order_and_release_every_independent_owner() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for panic in [false, true] {
        let (mut allocator, trace) = allocator(Default::default());
        let mut value = input(&layouts, &mut allocator, [4, 5]);
        let mut block = s::Block::allocate_from(&mut value, Some(span(2)), &mut allocator).unwrap();
        value.release_storage(&mut allocator).unwrap();
        assert_eq!(
            block.release_strong(&mut allocator).unwrap(),
            s::Release::Retire
        );
        let calls = trace.borrow().requests.len();
        let mut seen = Vec::new();
        let report = context(|ctx| {
            block
                .cleanup(&mut allocator, ctx, |token, alloc, reason, ctx| {
                    seen.push(token.leaf.span);
                    bytes(token, alloc, reason, ctx)?;
                    if panic && seen.len() == 1 {
                        panic!("contained shared child");
                    }
                    Err(7)
                })
                .map_err(|_| 99)
        });
        assert_eq!(seen, [Some(span(0)), Some(span(1))]);
        assert_eq!(report.primary().unwrap().span, Some(span(0)));
        assert_eq!(
            report.primary().unwrap().kind,
            if panic {
                c::FailureKind::CleanupPanic
            } else {
                c::FailureKind::Cleanup(7)
            }
        );
        assert_eq!(report.secondary().len(), 1);
        assert_eq!(report.secondary()[0].span, Some(span(1)));
        assert_eq!(trace.borrow().requests.len(), calls);
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
    }
}
