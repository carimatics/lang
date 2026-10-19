use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{allocation as a, buffer, cleanup as c, layout as l, map};
use std::{cell::RefCell, rc::Rc};

#[path = "support/map_mutation.rs"]
mod mutation;
#[path = "support/map_mutation_failures.rs"]
mod mutation_failures;
#[path = "support/map_storage.rs"]
mod storage;
#[path = "support/map_work.rs"]
mod work;

fn ty(id: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(id))
}
fn span(start: u32) -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start,
        end: start + 1,
    }
}
fn model() -> o::VerifiedTypes {
    let field = |name: &str| o::Field {
        name: name.into(),
        span: span(0),
        ty: o::TypeUse {
            ty: ty(1),
            span: span(0),
        },
    };
    o::TypeTable {
        sources: vec![m::Source {
            path: "map.lang".into(),
            text: "abcdefghijklmnopqrstuvwxyz".into(),
        }],
        constructed: vec![
            o::Constructor::Builtin(o::Builtin::Text),
            o::Constructor::Builtin(o::Builtin::Bytes),
            o::Constructor::Builtin(o::Builtin::Unit),
            o::Constructor::Option(o::Type::Nominal(m::DefinitionId(0))),
            o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
            o::Constructor::Builtin(o::Builtin::Allocator),
            o::Constructor::Option(o::Type::I64),
            o::Constructor::Option(ty(0)),
            o::Constructor::Option(ty(3)),
        ]
        .into_iter()
        .map(|constructor| o::Constructed {
            constructor,
            span: span(0),
        })
        .collect(),
        definitions: vec![o::Definition {
            name: "Pair".into(),
            public: false,
            span: span(0),
            kind: o::DefinitionKind::Record(vec![field("left"), field("right")]),
        }],
    }
    .verify(Default::default())
    .unwrap()
}
fn text<'l, 't, P: a::Provider>(
    layouts: &'l l::Layouts<'t>,
    value: &str,
    allocator: &mut a::Allocator<P>,
) -> l::Reservation<'l, 't> {
    let cell = l::Reservation::allocate(
        layouts,
        o::Type::Constructed(o::ConstructedId(0)),
        allocator,
    )
    .unwrap();
    let mut bytes = buffer::Bytes::from_slice(allocator, value.as_bytes()).unwrap();
    let mut value = bytes.take_text(allocator).unwrap();
    let token = value.take_token(allocator).unwrap();
    cell.data_mut(allocator).unwrap().copy_from_slice(&token);
    cell
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
    frame.initialize(c::Slot(0), (), None).unwrap();
    frame.finish(c::Exit::Return(None)).report
}
fn clean<P: a::Provider>(
    token: &mut l::Token,
    allocator: &mut a::Allocator<P>,
    _: c::Reason,
    _: &mut c::Context<'_>,
) -> Result<(), u32> {
    if token.leaf.ty == ty(0) {
        buffer::Text::from_token(&mut token.bytes, allocator)
            .unwrap()
            .release(allocator)
            .unwrap();
    } else {
        buffer::Bytes::from_token(&mut token.bytes, allocator)
            .unwrap()
            .release(allocator)
            .unwrap();
    }
    Ok(())
}
fn drain<P: a::Provider>(map: &mut map::Map<'_, '_>, allocator: &mut a::Allocator<P>) {
    let report = context(|ctx| map.cleanup(allocator, ctx, clean).map_err(|_| 99));
    assert!(report.primary().is_none(), "{report:?}");
}
fn discard<P: a::Provider>(mut value: l::Reservation<'_, '_>, allocator: &mut a::Allocator<P>) {
    let report = context(|ctx| value.cleanup(allocator, ctx, clean).map_err(|_| 99));
    assert!(report.primary().is_none(), "{report:?}");
}
fn number<'l, 't, P: a::Provider>(
    layouts: &'l l::Layouts<'t>,
    n: i64,
    allocator: &mut a::Allocator<P>,
) -> l::Reservation<'l, 't> {
    let value = l::Reservation::allocate(layouts, o::Type::I64, allocator).unwrap();
    value
        .write_immediate(value.root(), l::Immediate::I64(n), allocator)
        .unwrap();
    value
}
fn pair<'l, 't, P: a::Provider>(
    layouts: &'l l::Layouts<'t>,
    first: u8,
    allocator: &mut a::Allocator<P>,
) -> l::Reservation<'l, 't> {
    let value = l::Reservation::allocate(layouts, ty(3), allocator).unwrap();
    value.prepare_case(value.root(), 1, allocator).unwrap();
    let pair = value.child(value.root(), 0, allocator).unwrap();
    for i in 0..2 {
        let at = value.child(pair, i, allocator).unwrap();
        let token = buffer::Bytes::from_slice(allocator, &[first + i as u8])
            .unwrap()
            .take_token(allocator)
            .unwrap();
        let offset = at.offset() as usize;
        value.data_mut(allocator).unwrap()[offset..offset + 32].copy_from_slice(&token);
    }
    value
        .initialize_order(pair, [(1, span(1)), (0, span(0))].into_iter(), allocator)
        .unwrap();
    value
        .initialize_order(value.root(), [(0, span(2))].into_iter(), allocator)
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
fn empty<P: a::Provider>(a: &a::Allocator<P>) {
    let stats = a.stats();
    assert_eq!(stats.live_bytes, 0);
    assert_eq!(stats.live_allocations, 0);
    assert_eq!(stats.allocations, stats.deallocations);
}
#[test]
fn real_storage_lookup_compares_integer_values_and_text_bytes_not_owner_identity() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    for key_type in [o::Type::I64, o::Type::Constructed(o::ConstructedId(0))] {
        let mut map = map::Map::new(&layouts, key_type, o::Type::I64, &mut allocator).unwrap();
        let mut key = if key_type == o::Type::I64 {
            let cell = l::Reservation::allocate(&layouts, key_type, &mut allocator).unwrap();
            cell.write_immediate(cell.root(), l::Immediate::I64(-7), &mut allocator)
                .unwrap();
            cell
        } else {
            text(&layouts, "é", &mut allocator)
        };
        let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
        value
            .write_immediate(value.root(), l::Immediate::I64(9), &mut allocator)
            .unwrap();
        assert_eq!(
            map.publish_if_absent(&mut key, &mut value, Default::default(), &mut allocator)
                .unwrap(),
            map::Publication::Inserted
        );
        key.release_storage(&mut allocator).unwrap();
        value.release_storage(&mut allocator).unwrap();
        let query = if key_type == o::Type::I64 {
            l::Reservation::allocate(&layouts, key_type, &mut allocator).unwrap()
        } else {
            text(&layouts, "é", &mut allocator)
        };
        if key_type == o::Type::I64 {
            query
                .write_immediate(query.root(), l::Immediate::I64(-7), &mut allocator)
                .unwrap();
        }
        let found = map
            .lookup(
                map.key_of(&query, query.root(), &allocator).unwrap(),
                &allocator,
            )
            .unwrap()
            .unwrap();
        let view = map.view(found, &allocator).unwrap();
        assert_eq!(
            view.read_immediate(view.root()).unwrap(),
            l::Immediate::I64(9)
        );
        let mut query = query;
        if key_type == o::Type::I64 {
            query.release_storage(&mut allocator).unwrap();
        } else {
            let mut token = query.read_token(query.root(), &allocator).unwrap();
            let mut value = buffer::Text::from_token(&mut token.bytes, &allocator).unwrap();
            value.release(&mut allocator).unwrap();
            query.release_storage(&mut allocator).unwrap();
        }
        drain(&mut map, &mut allocator);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}
