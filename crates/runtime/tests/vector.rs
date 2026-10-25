use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{allocation as a, buffer, cleanup as c, layout as l, vector as v};
use std::{cell::RefCell, rc::Rc};

#[path = "support/vector_access.rs"]
mod access;
#[path = "support/vector_work.rs"]
mod work;

fn span(start: u32) -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start,
        end: start + 1,
    }
}
fn ty(index: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(index))
}
fn model() -> o::VerifiedTypes {
    let field = |name: &str, ty| o::Field {
        name: name.into(),
        span: span(0),
        ty: o::TypeUse { ty, span: span(0) },
    };
    o::TypeTable {
        sources: vec![m::Source {
            path: "vector.lang".into(),
            text: "abcdefghijklmnopqrstuvwxyz".into(),
        }],
        constructed: vec![
            o::Constructor::Builtin(o::Builtin::Bytes),
            o::Constructor::Option(o::Type::Nominal(m::DefinitionId(0))),
            o::Constructor::Builtin(o::Builtin::Unit),
            o::Constructor::Option(o::Type::Nominal(m::DefinitionId(1))),
            o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
            o::Constructor::Builtin(o::Builtin::Allocator),
        ]
        .into_iter()
        .map(|constructor| o::Constructed {
            constructor,
            span: span(0),
        })
        .collect(),
        definitions: vec![
            o::Definition {
                name: "Pair".into(),
                public: false,
                span: span(0),
                kind: o::DefinitionKind::Record(vec![field("left", ty(0)), field("right", ty(0))]),
            },
            o::Definition {
                name: "CopyPair".into(),
                public: false,
                span: span(0),
                kind: o::DefinitionKind::Record(vec![
                    field("number", o::Type::I64),
                    field("flag", o::Type::Bool),
                ]),
            },
        ],
    }
    .verify(Default::default())
    .unwrap()
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
        let mut t = self.0.borrow_mut();
        t.requests.push(request);
        if t.fail == Some(t.requests.len()) {
            None
        } else {
            a::Storage::try_new(request).ok()
        }
    }
    fn deallocate(&mut self, region: Self::Region, request: a::Request) {
        self.0.borrow_mut().releases.push(request);
        drop(region);
    }
}
fn allocator(limits: a::Limits) -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let t = Rc::new(RefCell::new(Trace::default()));
    (a::Allocator::new(Provider(t.clone()), limits).unwrap(), t)
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
    frame.initialize(c::Slot(0), (), Some(span(2))).unwrap();
    frame.finish(c::Exit::Return(None)).report
}
fn bytes<P: a::Provider>(
    token: &mut l::Token,
    allocator: &mut a::Allocator<P>,
    _: c::Reason,
    _: &mut c::Context<'_>,
) -> Result<(), u32> {
    let mut b = buffer::Bytes::from_token(&mut token.bytes, allocator).unwrap();
    b.release(allocator).unwrap();
    Ok(())
}
fn drain<P: a::Provider>(vector: &mut v::Vector<'_, '_>, allocator: &mut a::Allocator<P>) {
    let report = context(|ctx| vector.cleanup(allocator, ctx, bytes).map_err(|_| 99));
    assert!(report.primary().is_none(), "{report:?}");
}
fn empty(allocator: &a::Allocator<Provider>) {
    let s = allocator.stats();
    assert_eq!(s.live_bytes, 0);
    assert_eq!(s.live_allocations, 0);
    assert_eq!(s.allocations, s.deallocations);
}
fn input<'l, 't, P: a::Provider>(
    layouts: &'l l::Layouts<'t>,
    allocator: &mut a::Allocator<P>,
    first: u8,
) -> l::Reservation<'l, 't> {
    let value = l::Reservation::allocate(layouts, ty(1), allocator).unwrap();
    value.prepare_case(value.root(), 1, allocator).unwrap();
    let pair = value.child(value.root(), 0, allocator).unwrap();
    for i in 0..2 {
        let at = value.child(pair, i, allocator).unwrap();
        let mut b = buffer::Bytes::from_slice(allocator, &[first + i as u8]).unwrap();
        let token = b.take_token(allocator).unwrap();
        let start = at.offset() as usize;
        value.data_mut(allocator).unwrap()[start..start + 32].copy_from_slice(&token);
    }
    value
        .initialize_order(pair, [(1, span(1)), (0, span(0))].into_iter(), allocator)
        .unwrap();
    value
        .initialize_order(value.root(), [(0, span(2))].into_iter(), allocator)
        .unwrap();
    value
}
fn inspect<P: a::Provider>(
    vector: &v::Vector<'_, '_>,
    index: i64,
    allocator: &a::Allocator<P>,
) -> [u8; 2] {
    let view = vector.view(index, allocator).unwrap();
    assert_eq!(view.case(view.root()).unwrap(), 1);
    let pair = view.child(view.root(), 0).unwrap();
    std::array::from_fn(|i| {
        let mut token = view
            .read_token(view.child(pair, i as u32).unwrap())
            .unwrap();
        let b = buffer::Bytes::from_token(&mut token.bytes, allocator).unwrap();
        b.as_slice(allocator).unwrap()[0]
    })
}

#[test]
fn owning_nested_elements_grow_without_retaining_or_cleaning_payloads_and_drop_in_reverse_order() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut vector = v::Vector::new(&layouts, ty(1), &mut allocator).unwrap();
    for first in [10, 20, 30, 40, 50] {
        let mut incoming = input(&layouts, &mut allocator, first);
        vector
            .push_from(&mut incoming, Some(span(3)), &mut allocator)
            .unwrap();
        incoming.release_storage(&mut allocator).unwrap();
    }
    assert_eq!(vector.len(&allocator).unwrap(), 5);
    assert_eq!(vector.capacity(&allocator).unwrap(), 8);
    for i in 0..5 {
        assert_eq!(
            inspect(&vector, i, &allocator),
            [10 + i as u8 * 10, 11 + i as u8 * 10]
        );
    }
    let before = trace.borrow().requests.len();
    let mut seen = Vec::new();
    let report = context(|ctx| {
        vector
            .cleanup(&mut allocator, ctx, |token, a, _, _| {
                let mut b = buffer::Bytes::from_token(&mut token.bytes, a).unwrap();
                seen.push((b.as_slice(a).unwrap()[0], token.leaf.span));
                b.release(a).unwrap();
                Ok(())
            })
            .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    assert_eq!(
        seen.iter().map(|x| x.0).collect::<Vec<_>>(),
        [50, 51, 40, 41, 30, 31, 20, 21, 10, 11]
    );
    assert!(
        seen.as_chunks::<2>()
            .0
            .iter()
            .all(|pair| pair[0].1 == Some(span(0)) && pair[1].1 == Some(span(1)))
    );
    assert_eq!(trace.borrow().requests.len(), before);
    empty(&allocator);
}

#[test]
fn every_constructor_and_growth_oom_preserves_existing_elements_and_the_incoming_obligation() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    t.borrow_mut().fail = Some(1);
    assert!(matches!(
        v::Vector::new(&layouts, ty(1), &mut a),
        Err(v::Error::Allocation(a::Error::OutOfMemory))
    ));
    empty(&a);
    t.borrow_mut().fail = None;
    let mut retried = v::Vector::new(&layouts, ty(1), &mut a).unwrap();
    drain(&mut retried, &mut a);
    empty(&a);
    for existing in [false, true] {
        for position in 1..=2 {
            let (mut a, t) = allocator(Default::default());
            let mut vector = v::Vector::new(&layouts, ty(1), &mut a).unwrap();
            if existing {
                let mut first = input(&layouts, &mut a, 10);
                vector.push_from(&mut first, None, &mut a).unwrap();
                first.release_storage(&mut a).unwrap();
            }
            let mut incoming = input(&layouts, &mut a, 20);
            let original = incoming.data(&a).unwrap().to_vec();
            let before = a.stats();
            let requests = t.borrow().requests.len();
            t.borrow_mut().fail = Some(requests + position);
            assert!(matches!(
                vector.push_from(&mut incoming, Some(span(3)), &mut a),
                Err(v::Error::Allocation(a::Error::OutOfMemory))
            ));
            assert_eq!(incoming.data(&a).unwrap(), original);
            assert_eq!(a.stats().live_bytes, before.live_bytes);
            assert_eq!(a.stats().live_allocations, before.live_allocations);
            assert_eq!(vector.len(&a).unwrap(), i64::from(existing));
            if existing {
                assert_eq!(inspect(&vector, 0, &a), [10, 11]);
            }
            t.borrow_mut().fail = None;
            vector
                .push_from(&mut incoming, Some(span(3)), &mut a)
                .unwrap();
            incoming.release_storage(&mut a).unwrap();
            assert_eq!(inspect(&vector, i64::from(existing), &a), [20, 21]);
            drain(&mut vector, &mut a);
            empty(&a);
        }
    }
}

fn budget_scenario(limits: a::Limits, count: i64, expected: Option<a::Limit>) -> (a::Stats, u64) {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(limits);
    let mut source = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
    let mut vector =
        v::Vector::new_with_limits(&layouts, o::Type::I64, v::Limits { loans: 0 }, &mut a).unwrap();
    for _ in 0..count {
        vector.push_copy(&mut source, None, &mut a).unwrap();
    }
    let before = a.stats();
    let calls = t.borrow().requests.len();
    let outcome = vector.push_copy(&mut source, None, &mut a);
    if let Some(limit) = expected {
        assert!(
            matches!(outcome, Err(v::Error::Allocation(a::Error::Limit(actual)))
            if actual.kind == limit && actual.attempted > actual.limit)
        );
        assert_eq!(a.stats(), before);
        assert_eq!(t.borrow().requests.len(), calls);
        assert_eq!(vector.len(&a).unwrap(), count);
    } else {
        outcome.unwrap();
        assert_eq!(vector.len(&a).unwrap(), count + 1);
    }
    let result = a.stats();
    let maximum = t.borrow().requests.iter().map(|r| r.bytes()).max().unwrap();
    source.release_storage(&mut a).unwrap();
    drain(&mut vector, &mut a);
    empty(&a);
    (result, maximum)
}
#[test]
fn complete_growth_plan_enforces_exact_and_one_below_quotas_before_provider_callbacks() {
    let (baseline, request) = budget_scenario(Default::default(), 16, None);
    let (first, _) = budget_scenario(Default::default(), 0, None);
    for which in [
        a::Limit::RequestBytes,
        a::Limit::LiveBytes,
        a::Limit::CumulativeBytes,
        a::Limit::LiveAllocations,
    ] {
        for short in [false, true] {
            let mut limits = a::Limits::default();
            let delta = u64::from(short);
            let count = match which {
                a::Limit::RequestBytes => {
                    limits.request_bytes = request - delta;
                    16
                }
                a::Limit::LiveBytes => {
                    limits.live_bytes = baseline.peak_live_bytes - delta;
                    16
                }
                a::Limit::CumulativeBytes => {
                    limits.cumulative_bytes = baseline.cumulative_bytes - delta;
                    16
                }
                a::Limit::LiveAllocations => {
                    limits.live_allocations = first.peak_live_allocations - usize::from(short);
                    0
                }
                _ => unreachable!(),
            };
            budget_scenario(limits, count, short.then_some(which));
        }
    }
}

#[test]
fn capacity_reuse_needs_no_provider_call_and_keeps_the_pushed_initializer_span() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let mut vector = v::Vector::new(&layouts, ty(0), &mut a).unwrap();
    for byte in 0..4 {
        let mut value = l::Reservation::allocate(&layouts, ty(0), &mut a).unwrap();
        let mut b = buffer::Bytes::from_slice(&mut a, &[byte]).unwrap();
        let token = b.take_token(&a).unwrap();
        value.data_mut(&mut a).unwrap().copy_from_slice(&token);
        let calls = t.borrow().requests.len();
        if byte == 3 {
            t.borrow_mut().fail = Some(calls + 1);
        }
        vector
            .push_from(&mut value, Some(span(u32::from(byte))), &mut a)
            .unwrap();
        if byte == 3 {
            assert_eq!(t.borrow().requests.len(), calls);
        }
        value.release_storage(&mut a).unwrap();
    }
    let mut seen = Vec::new();
    let report = context(|ctx| {
        vector
            .cleanup(&mut a, ctx, |token, a, r, c| {
                seen.push(token.leaf.span);
                bytes(token, a, r, c)
            })
            .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    assert_eq!(
        seen,
        vec![Some(span(3)), Some(span(2)), Some(span(1)), Some(span(0))]
    );
    empty(&a);
}

#[test]
fn element_cleanup_failures_and_panics_do_not_skip_peers_or_allocate_retirement_storage() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for (failure, panic) in [(Some(1), None), (Some(1), Some(3)), (None, Some(1))] {
        let (mut a, t) = allocator(Default::default());
        let mut vector = v::Vector::new(&layouts, ty(1), &mut a).unwrap();
        for first in [10, 20, 30] {
            let mut value = input(&layouts, &mut a, first);
            vector.push_from(&mut value, Some(span(3)), &mut a).unwrap();
            value.release_storage(&mut a).unwrap();
        }
        let requests = t.borrow().requests.len();
        let mut seen = Vec::new();
        let report = context(|ctx| {
            vector
                .cleanup(&mut a, ctx, |token, a, r, c| {
                    let mut b = buffer::Bytes::from_token(&mut token.bytes, a).unwrap();
                    seen.push(b.as_slice(a).unwrap()[0]);
                    b.release(a).unwrap();
                    if panic == Some(seen.len()) {
                        panic!("injected element failure");
                    }
                    if failure == Some(seen.len()) {
                        Err(700)
                    } else {
                        let _ = (r, c);
                        Ok(())
                    }
                })
                .map_err(|_| 99)
        });
        assert_eq!(seen, [30, 31, 20, 21, 10, 11]);
        assert_eq!(
            report.primary().unwrap().kind,
            if failure.is_some() {
                c::FailureKind::Cleanup(700)
            } else {
                c::FailureKind::CleanupPanic
            }
        );
        if panic == Some(3) {
            assert_eq!(report.secondary()[0].kind, c::FailureKind::CleanupPanic);
        }
        assert_eq!(t.borrow().requests.len(), requests);
        empty(&a);
    }
}

#[test]
fn copy_aggregate_and_zero_sized_elements_keep_value_order_and_element_locations_isolated() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, _) = allocator(Default::default());
    for element in [ty(2), ty(3)] {
        let mut value = l::Reservation::allocate(&layouts, element, &mut a).unwrap();
        if element == ty(3) {
            value.prepare_case(value.root(), 1, &mut a).unwrap();
            let pair = value.child(value.root(), 0, &a).unwrap();
            value
                .write_immediate(
                    value.child(pair, 0, &a).unwrap(),
                    l::Immediate::I64(42),
                    &mut a,
                )
                .unwrap();
            value
                .write_immediate(
                    value.child(pair, 1, &a).unwrap(),
                    l::Immediate::Bool(true),
                    &mut a,
                )
                .unwrap();
        }
        let mut vector = v::Vector::new(&layouts, element, &mut a).unwrap();
        for _ in 0..17 {
            vector.push_copy(&mut value, None, &mut a).unwrap();
        }
        let first = vector.view(0, &a).unwrap();
        let last = vector.view(16, &a).unwrap();
        assert!(matches!(
            first.read_immediate(last.root()),
            Err(v::Error::Layout(l::Error::ForeignLocation))
        ));
        if element == ty(3) {
            let pair = last.child(last.root(), 0).unwrap();
            assert_eq!(
                last.read_immediate(last.child(pair, 0).unwrap()).unwrap(),
                l::Immediate::I64(42)
            );
        } else {
            assert_eq!(
                last.read_immediate(last.root()).unwrap(),
                l::Immediate::Unit
            );
        }
        assert!(vector.view(-1, &a).is_err());
        assert!(vector.view(17, &a).is_err());
        value.release_storage(&mut a).unwrap();
        drain(&mut vector, &mut a);
        empty(&a);
    }
}

#[test]
fn admission_refuses_invalid_spans_types_orders_and_foreign_layouts_before_growth() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, t) = allocator(Default::default());
    let before = a.stats();
    for element in [ty(4), ty(5)] {
        assert!(matches!(
            v::Vector::new(&layouts, element, &mut a),
            Err(v::Error::Layout(l::Error::InvalidType(_)))
        ));
    }
    assert_eq!(a.stats(), before);
    let mut vector = v::Vector::new(&layouts, ty(1), &mut a).unwrap();
    let mut value = input(&layouts, &mut a, 10);
    let calls = t.borrow().requests.len();
    let original = value.data(&a).unwrap().to_vec();
    assert!(matches!(
        vector.push_copy(&mut value, None, &mut a),
        Err(v::Error::Layout(l::Error::InvalidType(_)))
    ));
    assert_eq!(
        vector.push_from(&mut value, Some(span(99)), &mut a),
        Err(v::Error::Layout(l::Error::InvalidSpan))
    );
    assert_eq!(value.data(&a).unwrap(), original);
    assert_eq!(t.borrow().requests.len(), calls);
    let mut malformed = l::Reservation::allocate(&layouts, ty(1), &mut a).unwrap();
    let calls = t.borrow().requests.len();
    assert!(vector.push_from(&mut malformed, None, &mut a).is_err());
    assert_eq!(t.borrow().requests.len(), calls);
    malformed.release_storage(&mut a).unwrap();
    let other = model();
    let other_layouts = l::Layouts::compile(&other, Default::default()).unwrap();
    let mut foreign = input(&other_layouts, &mut a, 20);
    let calls = t.borrow().requests.len();
    assert!(matches!(
        vector.push_from(&mut foreign, None, &mut a),
        Err(v::Error::Layout(l::Error::InvalidType(_)))
    ));
    assert_eq!(t.borrow().requests.len(), calls);
    let report = context(|ctx| {
        value.cleanup(&mut a, ctx, bytes).unwrap();
        foreign.cleanup(&mut a, ctx, bytes).unwrap();
        Ok(())
    });
    assert!(report.primary().is_none());
    drain(&mut vector, &mut a);
    empty(&a);
}

#[test]
fn owner_tokens_validate_type_layout_allocator_generation_and_consumption() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let other_layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut a, _) = allocator(Default::default());
    let mut vector = v::Vector::new(&layouts, o::Type::I64, &mut a).unwrap();
    let original = vector.take_token(&a).unwrap();
    assert_eq!(vector.len(&a), Err(v::Error::Unavailable));
    let mut bytes = original;
    assert!(v::Vector::from_token(&layouts, o::Type::Bool, &mut bytes, &a).is_err());
    assert_eq!(bytes, original);
    let mut bytes = original;
    assert!(v::Vector::from_token(&other_layouts, o::Type::I64, &mut bytes, &a).is_err());
    assert_eq!(bytes, original);
    let (foreign, _) = allocator(Default::default());
    let mut bytes = original;
    assert!(matches!(
        v::Vector::from_token(&layouts, o::Type::I64, &mut bytes, &foreign),
        Err(v::Error::Allocation(a::Error::ForeignAllocator))
    ));
    assert_eq!(bytes, original);
    let mut bytes = original;
    let mut owner = v::Vector::from_token(&layouts, o::Type::I64, &mut bytes, &a).unwrap();
    assert_eq!(bytes, [0; 32]);
    drain(&mut owner, &mut a);
    let mut bytes = original;
    assert!(matches!(
        v::Vector::from_token(&layouts, o::Type::I64, &mut bytes, &a),
        Err(v::Error::Allocation(a::Error::StaleHandle))
    ));
    empty(&a);
}
