use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b},
};
use lang_runtime::shared::{Payload, SharedHeap, Strong};
use lang_runtime::{allocation as a, cleanup as c, layout as l};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::{cell::RefCell, path::Path, rc::Rc};

fn checked(text: &str) -> Box<OwnershipProgram> {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("layout.lang"),
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
fn types(typed: &b::TypedBodies) -> &o::VerifiedTypes {
    typed.places().declarations().types()
}
fn sites(typed: &b::TypedBodies) -> Vec<(b::Site, m::Register)> {
    typed.functions()[0]
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(block, body)| {
            body.instructions
                .iter()
                .enumerate()
                .filter(|(_, instruction)| {
                    matches!(instruction.op, b::Op::Record { .. } | b::Op::Variant { .. })
                })
                .map(move |(i, instruction)| {
                    (
                        b::Site {
                            block: m::BlockId(block as u32),
                            order: i + 1,
                        },
                        instruction.results[0],
                    )
                })
        })
        .collect()
}
fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 0,
    }
}
fn table() -> o::TypeTable {
    o::TypeTable {
        sources: vec![m::Source {
            path: "layout.lang".into(),
            text: String::new(),
        }],
        ..Default::default()
    }
}
fn construct(table: &mut o::TypeTable, constructor: o::Constructor) -> o::Type {
    let id = o::ConstructedId(table.constructed.len() as u32);
    table.constructed.push(o::Constructed {
        constructor,
        span: span(),
    });
    o::Type::Constructed(id)
}
fn field(name: &str, ty: o::Type) -> o::Field {
    o::Field {
        name: name.into(),
        span: span(),
        ty: o::TypeUse { ty, span: span() },
    }
}
fn allocator() -> a::Allocator<a::System> {
    a::Allocator::new(
        a::System,
        a::Limits {
            live_allocations: 16,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn all_core_constructors_have_finite_managed_layouts_and_correct_error_payloads() {
    let mut table = table();
    let unit = construct(&mut table, o::Constructor::Builtin(o::Builtin::Unit));
    let text = construct(&mut table, o::Constructor::Builtin(o::Builtin::Text));
    let bytes = construct(&mut table, o::Constructor::Builtin(o::Builtin::Bytes));
    let mut errors = Vec::new();
    for builtin in [
        o::Builtin::AllocError,
        o::Builtin::BoundsError,
        o::Builtin::BufferError,
        o::Builtin::Utf8Error,
        o::Builtin::OwnershipError,
        o::Builtin::ArenaError,
    ] {
        errors.push((
            builtin,
            construct(&mut table, o::Constructor::Builtin(builtin)),
        ));
    }
    let shared = construct(&mut table, o::Constructor::Shared(o::Type::I64));
    let weak = construct(&mut table, o::Constructor::Weak(o::Type::I64));
    let vector = construct(&mut table, o::Constructor::Vec(text));
    let map = construct(&mut table, o::Constructor::Map(o::Type::I64, bytes));
    let option = construct(&mut table, o::Constructor::Option(vector));
    let result = construct(&mut table, o::Constructor::Result(map, errors[0].1));
    construct(
        &mut table,
        o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
    );
    construct(&mut table, o::Constructor::Arena(o::Type::I64));
    table.definitions.push(o::Definition {
        name: "Mixed".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Record(vec![
            field("flag", o::Type::Bool),
            field("number", o::Type::I64),
            field("owner", shared),
        ]),
    });
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    assert_eq!(layouts.get(unit).unwrap().size, 0);
    for ty in [text, bytes, shared, weak, vector, map] {
        let layout = layouts.get(ty).unwrap();
        assert_eq!(
            (layout.size, layout.alignment, layout.work_slots),
            (32, 8, 1)
        );
    }
    for (builtin, ty) in errors {
        let layout = layouts.get(ty).unwrap();
        let expected = match builtin {
            o::Builtin::Utf8Error => (16, 8),
            o::Builtin::AllocError | o::Builtin::BoundsError => (4, 4),
            _ => (8, 4),
        };
        assert_eq!((layout.size, layout.alignment), expected);
        assert!(matches!(layout.kind, l::Kind::Error { .. }));
        assert_eq!(layout.work_slots, 0);
    }
    assert_eq!(layouts.get(option).unwrap().size, 40);
    assert_eq!(layouts.get(result).unwrap().size, 40);
    let mixed = layouts.get(o::Type::Nominal(m::DefinitionId(0))).unwrap();
    assert_eq!(
        (
            mixed.size,
            mixed.alignment,
            mixed.metadata_bytes,
            mixed.work_slots
        ),
        (48, 8, 80, 2)
    );
    let l::Kind::Record(record) = &mixed.kind else {
        panic!()
    };
    assert_eq!(
        record.fields.iter().map(|f| f.offset).collect::<Vec<_>>(),
        [0, 8, 16]
    );
    let request = mixed.storage_requests().unwrap();
    assert_eq!((request[0].bytes(), request[1].bytes()), (160, 48));
}

#[derive(Default)]
struct Trace {
    calls: usize,
    fail_on: usize,
    released: Vec<a::Request>,
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<a::Storage> {
        let mut trace = self.0.borrow_mut();
        trace.calls += 1;
        if trace.calls == trace.fail_on {
            None
        } else {
            Some(a::Storage::try_new(request).unwrap())
        }
    }
    fn deallocate(&mut self, region: a::Storage, request: a::Request) {
        self.0.borrow_mut().released.push(request);
        drop(region);
    }
}
#[test]
fn payload_and_retirement_metadata_are_preflighted_together_and_oom_rolls_back() {
    let program = checked(
        "struct R { owner: shared i64 } fn f(a: shared i64) -> R { return R { owner: a }; }",
    );
    let typed = typed(&program);
    let layouts = l::Layouts::compile(types(typed), Default::default()).unwrap();
    let ty = o::Type::Nominal(m::DefinitionId(0));
    let requests = layouts.get(ty).unwrap().storage_requests().unwrap();
    let total = requests.iter().map(|r| r.bytes()).sum::<u64>();
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut allocator = a::Allocator::new(
        Provider(Rc::clone(&trace)),
        a::Limits {
            live_bytes: total - 1,
            live_allocations: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        l::Reservation::allocate(&layouts, ty, &mut allocator),
        Err(l::Error::Allocation(a::Error::Limit(a::Limit::LiveBytes)))
    ));
    assert_eq!(trace.borrow().calls, 0);
    assert_eq!(allocator.stats(), a::Stats::default());
    let mut allocator = a::Allocator::new(
        Provider(Rc::clone(&trace)),
        a::Limits {
            live_allocations: 2,
            ..Default::default()
        },
    )
    .unwrap();
    trace.borrow_mut().fail_on = 2;
    assert!(matches!(
        l::Reservation::allocate(&layouts, ty, &mut allocator),
        Err(l::Error::Allocation(a::Error::OutOfMemory))
    ));
    assert_eq!(trace.borrow().released, [requests[0]]);
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().cumulative_bytes, total);
    assert_eq!(allocator.stats().peak_live_bytes, requests[0].bytes());
}

#[test]
fn source_record_orders_drive_reserved_storage_cleanup_with_actual_shared_owners() {
    for permutation in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let names = ["x: a", "y: b", "z: c"];
        let text = format!(
            "struct R {{ x: shared i64, y: shared i64, z: shared i64 }} fn f(a: shared i64, b: shared i64, c: shared i64) -> R {{ return R {{ {} }}; }}",
            permutation.map(|i| names[i]).join(", ")
        );
        let program = checked(&text);
        let typed = typed(&program);
        let layouts = l::Layouts::compile(types(typed), Default::default()).unwrap();
        let mut allocator = allocator();
        let mut value = l::Reservation::allocate(
            &layouts,
            o::Type::Nominal(m::DefinitionId(0)),
            &mut allocator,
        )
        .unwrap();
        let (site, _) = sites(typed)[0];
        value
            .initialize_from_mir(value.root(), typed, m::FunctionId(0), site, &mut allocator)
            .unwrap();
        let heap = SharedHeap::new(Default::default());
        let mut owners: Vec<Option<Strong>> = (0..3)
            .map(|i| {
                Some(
                    heap.allocate(
                        Payload {
                            bytes: vec![i],
                            edges: vec![],
                        },
                        None,
                    )
                    .unwrap(),
                )
            })
            .collect();
        let weak: Vec<_> = owners
            .iter()
            .map(|owner| owner.as_ref().unwrap().downgrade().unwrap())
            .collect();
        for field in 0..3 {
            let location = value.child(value.root(), field, &allocator).unwrap();
            let start = location.offset() as usize;
            value.data_mut(&mut allocator).unwrap()[start..start + 8]
                .copy_from_slice(&(field as u64).to_le_bytes());
        }
        let before = allocator.stats();
        value.begin_retirement(None, &mut allocator).unwrap();
        let mut actual = Vec::new();
        while let Some(leaf) = value.next_leaf(&mut allocator).unwrap() {
            let offset = leaf.offset as usize;
            let id = u64::from_le_bytes(
                value.data(&allocator).unwrap()[offset..offset + 8]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let owner = owners[id].take().unwrap();
            assert_eq!(owner.read().unwrap().bytes(), &[id as u8]);
            drop(owner);
            actual.push(id);
            let record = c::RecordOrder::new(typed, m::FunctionId(0), site).unwrap();
            assert_eq!(leaf.span, record.field_span(id as u32));
            assert_eq!(value.release_storage(&mut allocator), Err(l::Error::State));
            value.complete_leaf().unwrap();
        }
        assert_eq!(actual, permutation.into_iter().rev().collect::<Vec<_>>());
        assert!(weak.iter().all(|weak| weak.upgrade().is_err()));
        assert_eq!(allocator.stats(), before);
        assert_eq!(value.data_mut(&mut allocator), Err(l::Error::State));
        value.release_storage(&mut allocator).unwrap();
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(value.release_storage(&mut allocator), Err(l::Error::State));
    }
}

#[test]
fn nested_records_variants_and_copy_holes_use_disjoint_order_metadata() {
    let mut table = table();
    let shared = construct(&mut table, o::Constructor::Shared(o::Type::I64));
    table.definitions.push(o::Definition {
        name: "Inner".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Record(vec![
            field("a", shared),
            field("copy", o::Type::I64),
            field("b", shared),
        ]),
    });
    table.definitions.push(o::Definition {
        name: "Choice".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Enum(vec![
            o::Case {
                name: "Empty".into(),
                span: span(),
                payload: vec![],
            },
            o::Case {
                name: "Full".into(),
                span: span(),
                payload: vec![
                    o::TypeUse {
                        ty: o::Type::Nominal(m::DefinitionId(0)),
                        span: span(),
                    },
                    o::TypeUse {
                        ty: shared,
                        span: span(),
                    },
                ],
            },
        ]),
    });
    let verified = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    let mut allocator = allocator();
    let mut value = l::Reservation::allocate(
        &layouts,
        o::Type::Nominal(m::DefinitionId(1)),
        &mut allocator,
    )
    .unwrap();
    value.data_mut(&mut allocator).unwrap()[..4].copy_from_slice(&1_u32.to_le_bytes());
    value
        .initialize_order(
            value.root(),
            [(0, span()), (1, span())].into_iter(),
            &mut allocator,
        )
        .unwrap();
    let inner = value.child(value.root(), 0, &allocator).unwrap();
    let last = value.child(value.root(), 1, &allocator).unwrap();
    assert_eq!(
        value.begin_retirement(None, &mut allocator),
        Err(l::Error::Uninitialized)
    );
    value
        .initialize_order(
            inner,
            [(2, span()), (1, span()), (0, span())].into_iter(),
            &mut allocator,
        )
        .unwrap();
    let first_inner = value.child(inner, 0, &allocator).unwrap();
    let last_inner = value.child(inner, 2, &allocator).unwrap();
    value.begin_retirement(None, &mut allocator).unwrap();
    let mut actual = Vec::new();
    while let Some(leaf) = value.next_leaf(&mut allocator).unwrap() {
        actual.push(leaf.offset);
        value.complete_leaf().unwrap();
    }
    assert_eq!(
        actual,
        [last.offset(), first_inner.offset(), last_inner.offset()]
    );
    value.release_storage(&mut allocator).unwrap();
    let mut empty = l::Reservation::allocate(
        &layouts,
        o::Type::Nominal(m::DefinitionId(1)),
        &mut allocator,
    )
    .unwrap();
    empty
        .initialize_order(empty.root(), [].into_iter(), &mut allocator)
        .unwrap();
    empty.begin_retirement(None, &mut allocator).unwrap();
    assert_eq!(empty.next_leaf(&mut allocator).unwrap(), None);
    empty.release_storage(&mut allocator).unwrap();
}

#[test]
fn invalid_orders_spans_cases_and_foreign_locations_fail_before_retirement() {
    let program = checked(
        "struct R { x: shared i64, y: shared i64 } fn f(a: shared i64,b: shared i64) -> R { return R { y: b, x: a }; }",
    );
    let typed = typed(&program);
    let layouts = l::Layouts::compile(types(typed), Default::default()).unwrap();
    let mut allocator = allocator();
    let ty = o::Type::Nominal(m::DefinitionId(0));
    let mut value = l::Reservation::allocate(&layouts, ty, &mut allocator).unwrap();
    let mut other = l::Reservation::allocate(&layouts, ty, &mut allocator).unwrap();
    assert_eq!(
        value.initialize_order(
            other.root(),
            [(0, span()), (1, span())].into_iter(),
            &mut allocator
        ),
        Err(l::Error::ForeignLocation)
    );
    assert_eq!(
        value.initialize_order(
            value.root(),
            [(0, span()), (0, span())].into_iter(),
            &mut allocator
        ),
        Err(l::Error::InvalidOrder)
    );
    assert_eq!(
        value.begin_retirement(None, &mut allocator),
        Err(l::Error::Uninitialized)
    );
    assert_eq!(
        value.initialize_order(
            value.root(),
            [(0, span()), (2, span())].into_iter(),
            &mut allocator
        ),
        Err(l::Error::InvalidOrder)
    );
    assert_eq!(
        value.initialize_order(
            value.root(),
            [
                (0, span()),
                (
                    1,
                    m::Span {
                        source: m::SourceId(999),
                        ..span()
                    }
                )
            ]
            .into_iter(),
            &mut allocator
        ),
        Err(l::Error::InvalidSpan)
    );
    assert_eq!(
        value.child(value.root(), 99, &allocator),
        Err(l::Error::InvalidField(99))
    );
    let (site, _) = sites(typed)[0];
    value
        .initialize_from_mir(value.root(), typed, m::FunctionId(0), site, &mut allocator)
        .unwrap();
    let second_program = checked(
        "struct R { x: shared i64, y: shared i64 } fn f(a: shared i64,b: shared i64) -> R { return R { x: a, y: b }; }",
    );
    assert_eq!(
        value.initialize_from_mir(
            value.root(),
            second_program.mir().activity().moved().typed(),
            m::FunctionId(0),
            site,
            &mut allocator
        ),
        Err(l::Error::ForeignLocation)
    );
    value.release_storage(&mut allocator).unwrap();
    other.release_storage(&mut allocator).unwrap();
    let stale = value.root();
    let mut reused = l::Reservation::allocate(&layouts, ty, &mut allocator).unwrap();
    assert_eq!(
        reused.initialize_order(
            stale,
            [(0, span()), (1, span())].into_iter(),
            &mut allocator
        ),
        Err(l::Error::ForeignLocation)
    );
    reused.release_storage(&mut allocator).unwrap();
}

#[test]
fn exact_cumulative_compile_budgets_and_replay_preserve_source_positions() {
    let program = checked("struct R { a: shared i64 } fn f(a: shared i64)->R{return R{a:a};}");
    let typed = typed(&program);
    let proof = types(typed);
    let layouts = l::Layouts::compile(proof, Default::default()).unwrap();
    let usage = layouts.usage();
    let exact = m::Limits {
        work: usage.work,
        allocations: usage.allocation_bytes,
        elements: usage.elements,
        ..Default::default()
    };
    let replay = l::Layouts::compile(proof, exact).unwrap();
    assert_eq!(
        layouts.get(o::Type::Nominal(m::DefinitionId(0))),
        replay.get(o::Type::Nominal(m::DefinitionId(0)))
    );
    for limits in [
        m::Limits {
            work: usage.work - 1,
            ..exact
        },
        m::Limits {
            allocations: usage.allocation_bytes - 1,
            ..exact
        },
        m::Limits {
            elements: usage.elements - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            l::Layouts::compile(proof, limits),
            Err(l::Error::Resource { .. })
        ));
    }
    assert!(usage.work > proof.work_used());
    assert!(usage.allocation_bytes > proof.requested_bytes());
}

#[test]
fn deep_inline_drop_uses_only_normal_allocation_reserved_work_storage() {
    let depth = 1024;
    let mut table = table();
    let shared = construct(&mut table, o::Constructor::Shared(o::Type::I64));
    for i in 0..depth {
        let child = if i + 1 == depth {
            shared
        } else {
            o::Type::Nominal(m::DefinitionId(i + 1))
        };
        table.definitions.push(o::Definition {
            name: format!("R{i}"),
            public: false,
            span: span(),
            kind: o::DefinitionKind::Record(vec![field("next", child)]),
        });
    }
    let verified = table
        .verify(o::Limits {
            unit: m::Limits {
                depth: 2048,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    let layouts = l::Layouts::compile(&verified, Default::default()).unwrap();
    let mut allocator = allocator();
    let mut value = l::Reservation::allocate(
        &layouts,
        o::Type::Nominal(m::DefinitionId(0)),
        &mut allocator,
    )
    .unwrap();
    let mut location = value.root();
    for _ in 0..depth {
        value
            .initialize_order(location, [(0, span())].into_iter(), &mut allocator)
            .unwrap();
        location = value.child(location, 0, &allocator).unwrap();
    }
    let before = allocator.stats();
    value.begin_retirement(None, &mut allocator).unwrap();
    let leaf = value.next_leaf(&mut allocator).unwrap().unwrap();
    assert_eq!(leaf.ty, shared);
    assert_eq!(leaf.offset, 0);
    assert_eq!(value.next_leaf(&mut allocator), Err(l::Error::State));
    value.complete_leaf().unwrap();
    assert_eq!(value.next_leaf(&mut allocator).unwrap(), None);
    assert_eq!(allocator.stats(), before);
    value.release_storage(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn source_variant_initialization_binds_order_to_the_active_case() {
    let program =
        checked("enum E { Empty, Full(shared i64) } fn f(a: shared i64)->E{return E::Full(a);}");
    let typed = typed(&program);
    let layouts = l::Layouts::compile(types(typed), Default::default()).unwrap();
    let mut allocator = allocator();
    let mut value = l::Reservation::allocate(
        &layouts,
        o::Type::Nominal(m::DefinitionId(0)),
        &mut allocator,
    )
    .unwrap();
    let (site, _) = sites(typed)[0];
    value
        .initialize_from_mir(value.root(), typed, m::FunctionId(0), site, &mut allocator)
        .unwrap();
    assert_eq!(&value.data(&allocator).unwrap()[..4], &1_u32.to_le_bytes());
    value.data_mut(&mut allocator).unwrap()[..4].copy_from_slice(&0_u32.to_le_bytes());
    assert_eq!(
        value.begin_retirement(None, &mut allocator),
        Err(l::Error::Uninitialized)
    );
    value.data_mut(&mut allocator).unwrap()[..4].copy_from_slice(&99_u32.to_le_bytes());
    assert_eq!(
        value.begin_retirement(None, &mut allocator),
        Err(l::Error::InvalidCase(99))
    );
    value.data_mut(&mut allocator).unwrap()[..4].copy_from_slice(&1_u32.to_le_bytes());
    value.begin_retirement(None, &mut allocator).unwrap();
    assert_eq!(value.release_storage(&mut allocator), Err(l::Error::State));
    let leaf = value.next_leaf(&mut allocator).unwrap().unwrap();
    assert!(leaf.span.is_some());
    value.complete_leaf().unwrap();
    assert_eq!(value.complete_leaf(), Err(l::Error::State));
    assert_eq!(value.next_leaf(&mut allocator).unwrap(), None);
    assert_eq!(
        value.begin_retirement(None, &mut allocator),
        Err(l::Error::State)
    );
    value.release_storage(&mut allocator).unwrap();
}

#[test]
fn reserved_drop_descriptors_share_cleanup_reports_and_continue_after_leaf_failures() {
    for panic_leaf in [false, true] {
        let program = checked(
            "struct R { x: shared i64, y: shared i64, z: shared i64 } fn f(a: shared i64,b: shared i64,c: shared i64)->R{return R{z:c,x:a,y:b};}",
        );
        let typed = typed(&program);
        let layouts = l::Layouts::compile(types(typed), Default::default()).unwrap();
        let mut allocator = allocator();
        let value = l::Reservation::allocate(
            &layouts,
            o::Type::Nominal(m::DefinitionId(0)),
            &mut allocator,
        )
        .unwrap();
        let (site, _) = sites(typed)[0];
        value
            .initialize_from_mir(value.root(), typed, m::FunctionId(0), site, &mut allocator)
            .unwrap();
        let heap = SharedHeap::new(Default::default());
        let mut owners: Vec<_> = (0..3)
            .map(|i| {
                Some(
                    heap.allocate(
                        Payload {
                            bytes: vec![i],
                            edges: vec![],
                        },
                        None,
                    )
                    .unwrap(),
                )
            })
            .collect();
        let weak: Vec<_> = owners
            .iter()
            .map(|o| o.as_ref().unwrap().downgrade().unwrap())
            .collect();
        for field in 0..3 {
            let location = value.child(value.root(), field, &allocator).unwrap();
            let offset = location.offset() as usize;
            value.data_mut(&mut allocator).unwrap()[offset..offset + 8]
                .copy_from_slice(&(field as u64).to_le_bytes());
        }
        let before = allocator.stats();
        let mut trace = Vec::with_capacity(3);
        let mut instance = c::Instance::default();
        let mut frame = instance
            .enter_with_context(
                c::Limits {
                    slots: 1,
                    failure_records: 4,
                    ..Default::default()
                },
                |value: &mut l::Reservation<'_, '_>, _: c::Reason, context: &mut c::Context<'_>| {
                    value
                        .cleanup(&mut allocator, context, |token, _, _, _| {
                            let id =
                                u64::from_le_bytes(token.bytes[..8].try_into().unwrap()) as usize;
                            trace.push(id);
                            let owner = owners[id].take().unwrap();
                            drop(owner);
                            assert!(!(panic_leaf && id == 0), "host cleanup panic");
                            Err(100 + id as u32)
                        })
                        .unwrap();
                    Ok(())
                },
            )
            .unwrap();
        frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
        assert!(frame.initialize(c::Slot(0), value, Some(span())).is_ok());
        let done = frame.finish(c::Exit::Failure {
            failure: c::ExecutionFailure::FuelLimit,
            span: Some(span()),
        });
        assert_eq!(trace, [1, 0, 2]);
        assert!(done.poisoned);
        assert_eq!(
            done.report.primary().unwrap().kind,
            c::FailureKind::Execution(c::ExecutionFailure::FuelLimit)
        );
        let order = c::RecordOrder::new(typed, m::FunctionId(0), site).unwrap();
        for (record, id) in done.report.secondary().iter().zip([1, 0, 2]) {
            let expected = if panic_leaf && id == 0 {
                c::FailureKind::CleanupPanic
            } else {
                c::FailureKind::Cleanup(100 + id)
            };
            assert_eq!(record.kind, expected);
            assert_eq!(record.span, order.field_span(id));
        }
        assert!(weak.iter().all(|weak| weak.upgrade().is_err()));
        assert_eq!(allocator.stats().requests, before.requests);
        assert_eq!(allocator.stats().cumulative_bytes, before.cumulative_bytes);
        assert_eq!(allocator.stats().live_bytes, 0);
    }
}

#[test]
fn exponentially_expanding_inline_storage_overflow_is_rejected_before_entry() {
    let mut table = table();
    let shared = construct(&mut table, o::Constructor::Shared(o::Type::I64));
    for i in 0..60 {
        let child = if i == 59 {
            shared
        } else {
            o::Type::Nominal(m::DefinitionId(i + 1))
        };
        table.definitions.push(o::Definition {
            name: format!("R{i}"),
            public: false,
            span: span(),
            kind: o::DefinitionKind::Record(vec![field("left", child), field("right", child)]),
        });
    }
    let verified = table.verify(Default::default()).unwrap();
    assert!(matches!(
        l::Layouts::compile(&verified, Default::default()),
        Err(l::Error::Overflow | l::Error::Allocation(a::Error::Overflow))
    ));
}
