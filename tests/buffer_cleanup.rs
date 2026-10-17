//! Source-checked aggregate layout with the production Bytes/Text token service.
//! This constructs checked MIR values; it is not a complete ownership VM.
use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b},
};
use lang_runtime::{
    allocation as a,
    buffer::{self, Bytes, Text},
    cleanup as c, layout as l,
};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::{cell::RefCell, path::Path, rc::Rc};

fn checked(text: &str) -> Box<OwnershipProgram> {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("buffer.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let Program::Ownership(program) = analysis.result.unwrap() else {
        panic!("ownership profile")
    };
    program
}
fn typed(program: &OwnershipProgram) -> &b::TypedBodies {
    program.mir().activity().moved().typed()
}
fn sites(typed: &b::TypedBodies) -> Vec<(b::Site, o::Type)> {
    typed.functions()[0]
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(block, body)| {
            body.instructions
                .iter()
                .enumerate()
                .filter(|(_, i)| matches!(i.op, b::Op::Record { .. } | b::Op::Variant { .. }))
                .map(move |(index, i)| {
                    (
                        b::Site {
                            block: m::BlockId(block as u32),
                            order: index + 1,
                        },
                        typed.places().functions()[0].values[i.results[0].0 as usize].ty,
                    )
                })
        })
        .collect()
}
#[derive(Default)]
struct Trace {
    allocations: usize,
    released: Vec<(usize, a::Request)>,
}
struct Region {
    storage: a::Storage,
    id: usize,
}
impl a::Region for Region {
    fn bytes(&self) -> &[u8] {
        self.storage.bytes()
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        self.storage.bytes_mut()
    }
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = Region;
    fn allocate(&mut self, request: a::Request) -> Option<Region> {
        let mut trace = self.0.borrow_mut();
        trace.allocations += 1;
        Some(Region {
            storage: a::Storage::try_new(request).unwrap(),
            id: trace.allocations,
        })
    }
    fn deallocate(&mut self, region: Region, request: a::Request) {
        self.0.borrow_mut().released.push((region.id, request));
    }
}
fn allocator(limits: a::Limits) -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    (
        a::Allocator::new(Provider(Rc::clone(&trace)), limits).unwrap(),
        trace,
    )
}
fn text<P: a::Provider>(allocator: &mut a::Allocator<P>, value: &str) -> Text {
    Bytes::from_slice(allocator, value.as_bytes())
        .unwrap()
        .take_text(allocator)
        .unwrap()
}
fn finish(
    value: l::Reservation<'_, '_>,
    typed: &b::TypedBodies,
    allocator: &mut a::Allocator<Provider>,
    failure: bool,
) -> (bool, Vec<(o::Type, Option<m::Span>)>) {
    let mut instance = c::Instance::default();
    let mut leaves = Vec::with_capacity(2);
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |value: &mut l::Reservation<'_, '_>, _: c::Reason, context: &mut c::Context<'_>| {
                value
                    .cleanup(allocator, context, |token, allocator, _, _| {
                        leaves.push((token.leaf.ty, token.leaf.span));
                        buffer::cleanup_token(
                            typed.places().declarations().types(),
                            token,
                            allocator,
                        )
                        .map_err(|_| 90)
                    })
                    .map_err(|_| 91)
            },
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    assert!(frame.initialize(c::Slot(0), value, None).is_ok());
    let done = frame.finish(if failure {
        c::Exit::Failure {
            failure: c::ExecutionFailure::FuelLimit,
            span: None,
        }
    } else {
        c::Exit::Return(None)
    });
    if failure {
        assert_eq!(
            done.report.primary().unwrap().kind,
            c::FailureKind::Execution(c::ExecutionFailure::FuelLimit)
        );
        assert!(done.report.secondary().is_empty());
    } else {
        assert!(done.report.primary().is_none());
    }
    (done.poisoned, leaves)
}

#[test]
fn source_record_tokens_resolve_and_free_real_buffers_in_actual_initialization_order() {
    for (written, expected) in [
        ("raw: b, text: t", [2, 1, 4, 3]),
        ("text: t, raw: b", [1, 2, 4, 3]),
    ] {
        let program = checked(&format!(
            "struct Pair {{ raw: core::Bytes, text: core::Text }} fn f(b: core::Bytes,t: core::Text)->Pair{{return Pair{{{written}}};}}"
        ));
        let typed = typed(&program);
        let types = typed.places().declarations().types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let (mut allocator, trace) = allocator(a::Limits {
            live_allocations: 8,
            ..Default::default()
        });
        let mut raw = Bytes::from_slice(&mut allocator, b"r").unwrap();
        let mut text = text(&mut allocator, "t");
        let value = l::Reservation::allocate(
            &layouts,
            o::Type::Nominal(m::DefinitionId(0)),
            &mut allocator,
        )
        .unwrap();
        for (field, token) in [
            (0, raw.take_token(&allocator).unwrap()),
            (1, text.take_token(&allocator).unwrap()),
        ] {
            let location = value.child(value.root(), field, &allocator).unwrap();
            let offset = location.offset() as usize;
            value.data_mut(&mut allocator).unwrap()[offset..offset + 32].copy_from_slice(&token);
        }
        let (site, _) = sites(typed)[0];
        value
            .initialize_from_mir(value.root(), typed, m::FunctionId(0), site, &mut allocator)
            .unwrap();
        let before = allocator.stats();
        let (poisoned, leaves) = finish(value, typed, &mut allocator, true);
        assert!(!poisoned);
        assert_eq!(
            trace
                .borrow()
                .released
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            expected
        );
        let order = c::RecordOrder::new(typed, m::FunctionId(0), site).unwrap();
        assert_eq!(
            leaves.iter().map(|(_, span)| *span).collect::<Vec<_>>(),
            order
                .reverse()
                .map(|field| order.field_span(field))
                .collect::<Vec<_>>()
        );
        assert_eq!(allocator.stats().requests, before.requests);
        assert_eq!(allocator.stats().cumulative_bytes, before.cumulative_bytes);
        assert_eq!(allocator.stats().live_bytes, 0);
        assert!(raw.release(&mut allocator).is_ok());
        assert!(text.release(&mut allocator).is_ok());
        assert_eq!(trace.borrow().released.len(), 4);
    }
}

#[test]
fn nested_source_enum_retirement_uses_real_buffer_adapter_and_preserves_copy_fields() {
    let program = checked(
        "struct P {raw:core::Bytes,flag:bool} enum E {Empty,Full(P,core::Text)} fn f(b:core::Bytes,t:core::Text)->E{return E::Full(P{flag:true,raw:b},t);}",
    );
    let typed = typed(&program);
    let layouts =
        l::Layouts::compile(typed.places().declarations().types(), Default::default()).unwrap();
    let (mut allocator, trace) = allocator(a::Limits {
        live_allocations: 8,
        ..Default::default()
    });
    let mut raw = Bytes::from_slice(&mut allocator, b"r").unwrap();
    let mut text = text(&mut allocator, "t");
    let value = l::Reservation::allocate(
        &layouts,
        o::Type::Nominal(m::DefinitionId(1)),
        &mut allocator,
    )
    .unwrap();
    let sites = sites(typed);
    let variant = sites
        .iter()
        .find(|(_, ty)| *ty == o::Type::Nominal(m::DefinitionId(1)))
        .unwrap()
        .0;
    value
        .initialize_from_mir(
            value.root(),
            typed,
            m::FunctionId(0),
            variant,
            &mut allocator,
        )
        .unwrap();
    let inner = value.child(value.root(), 0, &allocator).unwrap();
    let raw_field = value.child(inner, 0, &allocator).unwrap();
    let flag = value.child(inner, 1, &allocator).unwrap();
    let text_field = value.child(value.root(), 1, &allocator).unwrap();
    for (location, token) in [
        (raw_field, raw.take_token(&allocator).unwrap()),
        (text_field, text.take_token(&allocator).unwrap()),
    ] {
        let offset = location.offset() as usize;
        value.data_mut(&mut allocator).unwrap()[offset..offset + 32].copy_from_slice(&token);
    }
    value.data_mut(&mut allocator).unwrap()[flag.offset() as usize] = 1;
    let record = sites
        .iter()
        .find(|(_, ty)| *ty == o::Type::Nominal(m::DefinitionId(0)))
        .unwrap()
        .0;
    value
        .initialize_from_mir(inner, typed, m::FunctionId(0), record, &mut allocator)
        .unwrap();
    assert_eq!(value.data(&allocator).unwrap()[flag.offset() as usize], 1);
    let (poisoned, leaves) = finish(value, typed, &mut allocator, false);
    assert!(!poisoned);
    assert_eq!(leaves.len(), 2);
    assert_eq!(
        trace
            .borrow()
            .released
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [2, 1, 4, 3]
    );
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn failed_record_storage_preflight_keeps_buffer_temporaries_available_for_failure_cleanup() {
    let program = checked(
        "struct Pair {raw:core::Bytes,text:core::Text} fn f(b:core::Bytes,t:core::Text)->Pair{return Pair{raw:b,text:t};}",
    );
    let typed = typed(&program);
    let layouts =
        l::Layouts::compile(typed.places().declarations().types(), Default::default()).unwrap();
    let (mut allocator, trace) = allocator(a::Limits {
        request_bytes: 128,
        live_allocations: 8,
        ..Default::default()
    });
    let mut raw = Bytes::from_slice(&mut allocator, b"r").unwrap();
    let mut text = text(&mut allocator, "t");
    let before = allocator.stats();
    assert!(matches!(
        l::Reservation::allocate(
            &layouts,
            o::Type::Nominal(m::DefinitionId(0)),
            &mut allocator
        ),
        Err(l::Error::Allocation(a::Error::Limit(
            a::Limit::RequestBytes
        )))
    ));
    assert_eq!(allocator.stats(), before);
    assert_eq!(trace.borrow().allocations, 2);
    assert!(trace.borrow().released.is_empty());
    assert_eq!(raw.as_slice(&allocator).unwrap(), b"r");
    assert_eq!(text.as_str(&allocator), Ok("t"));
    text.release(&mut allocator).unwrap();
    raw.release(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
}
