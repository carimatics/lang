use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit::{
    self as m,
    ownership::{bodies as b, declarations::LocalId},
};
use lang_runtime::{
    allocation as a,
    buffer::{self, AllocError, Bytes},
    cleanup as c, layout as l,
};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::{cell::RefCell, path::Path, rc::Rc};

#[path = "support/owned_pools.rs"]
mod pools;

const SOURCE: &str = "fn make(a: &core::Allocator, flag: bool) -> core::Result<core::Bytes, core::AllocError> { let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(a); if flag { return made; } return core::Result::Err(core::AllocError::Limit); }";
fn checked(source: &str) -> Box<OwnershipProgram> {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("frame.lang"),
        source.as_bytes(),
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
fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 0,
    }
}
#[derive(Default)]
struct Trace {
    calls: usize,
    fail_at: usize,
    requests: Vec<a::Request>,
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
        trace.calls += 1;
        trace.requests.push(request);
        if trace.calls == trace.fail_at {
            return None;
        }
        Some(Region {
            storage: a::Storage::try_new(request).unwrap(),
            id: trace.calls,
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
fn limits() -> a::Limits {
    a::Limits {
        live_allocations: 512,
        ..Default::default()
    }
}
fn plan<'l, 't>(layouts: &'l l::Layouts<'t>, typed: &b::TypedBodies) -> l::FramePlan<'l, 't> {
    l::FramePlan::compile(
        layouts,
        typed,
        m::FunctionId(0),
        Default::default(),
        1_000_000,
    )
    .unwrap()
}

#[test]
fn checked_frame_mapping_and_exact_guest_limits_cover_every_request_before_entry() {
    let program = checked(SOURCE);
    let typed = typed(&program);
    let layouts =
        l::Layouts::compile(typed.places().declarations().types(), Default::default()).unwrap();
    let plan = plan(&layouts, typed);
    let declaration = &typed.places().declarations().functions()[0];
    let locals = declaration.locals.len();
    let values = typed.places().functions()[0].values.len();
    assert_eq!(plan.cell_count(), locals + values + 1);
    assert_eq!(plan.return_cell(), locals + values);
    assert_eq!(
        plan.cleanup_slots(),
        locals + values + typed.functions()[0].loans
    );
    for (i, local) in declaration.locals.iter().enumerate() {
        assert_eq!(plan.local(LocalId(i as u32)), Ok(i));
        assert_eq!(plan.cell_type(i), Some(local.ty));
    }
    for (i, value) in typed.places().functions()[0].values.iter().enumerate() {
        assert_eq!(plan.value(m::Register(i as u32)), Ok(locals + i));
        assert_eq!(plan.cell_type(locals + i), Some(*value));
    }
    assert_eq!(plan.cell_type(plan.return_cell()), Some(declaration.result));
    assert!(matches!(
        plan.local(LocalId(u32::MAX)),
        Err(l::Error::InvalidCell(_))
    ));
    assert!(matches!(
        plan.value(m::Register(u32::MAX)),
        Err(l::Error::InvalidCell(_))
    ));
    let requests = plan.requests();
    let bytes: u64 = requests.iter().map(|r| r.bytes()).sum();
    let exact = a::Limits {
        request_bytes: requests.iter().map(|r| r.bytes()).max().unwrap(),
        live_bytes: bytes,
        cumulative_bytes: bytes,
        live_allocations: requests.len(),
        alignment: requests.iter().map(|r| r.alignment()).max().unwrap(),
    };
    let (mut allocator, trace) = allocator(exact);
    let mut frame = plan.reserve(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, bytes);
    assert_eq!(allocator.stats().cumulative_bytes, bytes);
    assert_eq!(allocator.stats().live_allocations, requests.len());
    assert_eq!(
        trace.borrow().requests,
        requests
            .iter()
            .copied()
            .filter(|r| r.bytes() > 0)
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        frame.cell(plan.cell_count()),
        Err(l::Error::InvalidCell(_))
    ));
    frame.release(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(matches!(frame.cell(0), Err(l::Error::State)));
    assert_eq!(frame.release(&mut allocator), Err(l::Error::State));
}

#[test]
fn combined_guest_and_host_pool_limits_deny_admission_without_provider_callbacks() {
    let program = checked(SOURCE);
    let typed = typed(&program);
    let layouts =
        l::Layouts::compile(typed.places().declarations().types(), Default::default()).unwrap();
    let plan = plan(&layouts, typed);
    let requests = plan.requests();
    let total: u64 = requests.iter().map(|r| r.bytes()).sum();
    let max = requests.iter().map(|r| r.bytes()).max().unwrap();
    for limits in [
        a::Limits {
            request_bytes: max - 1,
            ..limits()
        },
        a::Limits {
            live_bytes: total - 1,
            ..limits()
        },
        a::Limits {
            cumulative_bytes: total - 1,
            ..limits()
        },
        a::Limits {
            live_allocations: requests.len() - 1,
            ..limits()
        },
        a::Limits {
            alignment: 1,
            ..limits()
        },
    ] {
        let (mut allocator, trace) = allocator(limits);
        assert!(matches!(
            plan.reserve(&mut allocator),
            Err(l::Error::Allocation(a::Error::Limit(_)))
        ));
        assert_eq!(trace.borrow().calls, 0);
        assert_eq!(allocator.stats(), a::Stats::default());
    }
    let (mut alloc, _) = allocator(limits());
    let mut frame = plan.reserve(&mut alloc).unwrap();
    let used = frame.usage();
    frame.release(&mut alloc).unwrap();
    for limits in [
        m::Limits {
            allocations: used.allocation_bytes - 1,
            ..Default::default()
        },
        m::Limits {
            work: used.work - 1,
            ..Default::default()
        },
        m::Limits {
            elements: used.elements - 1,
            ..Default::default()
        },
    ] {
        let tight =
            l::FramePlan::compile(&layouts, typed, m::FunctionId(0), limits, 1_000_000).unwrap();
        let (mut allocator, trace) = allocator(self::limits());
        assert!(matches!(
            tight.reserve(&mut allocator),
            Err(l::Error::Resource { .. })
        ));
        assert_eq!(trace.borrow().calls, 0);
        assert_eq!(allocator.stats(), a::Stats::default());
    }
    let exact = m::Limits {
        work: used.work,
        allocations: used.allocation_bytes,
        elements: used.elements,
        ..Default::default()
    };
    let exact = l::FramePlan::compile(&layouts, typed, m::FunctionId(0), exact, 1_000_000).unwrap();
    let (mut allocator, _) = allocator(limits());
    let mut frame = exact.reserve(&mut allocator).unwrap();
    assert_eq!(frame.usage(), used);
    frame.release(&mut allocator).unwrap();
}

#[test]
fn every_provider_oom_rolls_back_the_frame_in_reverse_and_preserves_an_existing_owner() {
    let program = checked(SOURCE);
    let typed = typed(&program);
    let layouts =
        l::Layouts::compile(typed.places().declarations().types(), Default::default()).unwrap();
    let plan = plan(&layouts, typed);
    for (position, _) in plan
        .requests()
        .iter()
        .enumerate()
        .filter(|(_, r)| r.bytes() != 0)
    {
        let (mut allocator, trace) = allocator(limits());
        let sentinel = allocator.allocate(a::Request::new(7, 1).unwrap()).unwrap();
        allocator
            .bytes_mut(sentinel)
            .unwrap()
            .copy_from_slice(b"sentine");
        let before = allocator.stats();
        let callbacks = plan.requests()[..=position]
            .iter()
            .filter(|r| r.bytes() != 0)
            .count();
        trace.borrow_mut().fail_at = callbacks + 1;
        assert!(matches!(
            plan.reserve(&mut allocator),
            Err(l::Error::Allocation(a::Error::OutOfMemory))
        ));
        assert_eq!(allocator.bytes(sentinel).unwrap(), b"sentine");
        assert_eq!(allocator.stats().live_bytes, before.live_bytes);
        assert_eq!(allocator.stats().live_allocations, 1);
        assert_eq!(
            allocator.stats().cumulative_bytes,
            7 + plan.requests()[..=position]
                .iter()
                .map(|r| r.bytes())
                .sum::<u64>()
        );
        assert_eq!(
            allocator.stats().requests,
            before.requests + position as u64 + 1
        );
        assert_eq!(
            trace
                .borrow()
                .released
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            (2..callbacks + 1).rev().collect::<Vec<_>>()
        );
        allocator.release(sentinel).unwrap();
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn frame_plans_reject_foreign_graphs_invalid_functions_and_cell_limit_boundaries() {
    let first = checked(SOURCE);
    let second = checked(SOURCE);
    let typed = typed(&first);
    let layouts =
        l::Layouts::compile(typed.places().declarations().types(), Default::default()).unwrap();
    let plan = plan(&layouts, typed);
    assert!(matches!(
        l::FramePlan::compile(
            &layouts,
            self::typed(&second),
            m::FunctionId(0),
            Default::default(),
            usize::MAX
        ),
        Err(l::Error::ForeignLocation)
    ));
    assert!(matches!(
        l::FramePlan::compile(
            &layouts,
            typed,
            m::FunctionId(u32::MAX),
            Default::default(),
            usize::MAX
        ),
        Err(l::Error::InvalidFunction(_))
    ));
    let cells = plan.cleanup_slots() + 1;
    assert!(matches!(
        l::FramePlan::compile(
            &layouts,
            typed,
            m::FunctionId(0),
            Default::default(),
            cells - 1
        ),
        Err(l::Error::CellLimit { .. })
    ));
    assert!(
        l::FramePlan::compile(&layouts, typed, m::FunctionId(0), Default::default(), cells).is_ok()
    );
}

#[test]
fn a_reserved_result_cell_retires_and_recycles_repeatedly_without_new_allocation() {
    let program = checked(SOURCE);
    let typed = typed(&program);
    let types = typed.places().declarations().types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let plan = plan(&layouts, typed);
    let (mut allocator, trace) = allocator(limits());
    let mut storage = plan.reserve(&mut allocator).unwrap();
    let before = allocator.stats();
    {
        let mut trace = trace.borrow_mut();
        trace.fail_at = trace.calls + 1;
    }
    let mut instance = c::Instance::default();
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |_: &mut (), _, context: &mut c::Context<'_>| {
                let cell = storage.cell_mut(plan.return_cell()).unwrap();
                assert_eq!(cell.recycle(&mut allocator), Err(l::Error::State));
                for _ in 0..128 {
                    let root = cell.root();
                    cell.prepare_case(root, 1, &mut allocator).unwrap();
                    let payload = cell.child(root, 0, &allocator).unwrap();
                    cell.write_immediate(
                        payload,
                        l::Immediate::Allocation(AllocError::Limit),
                        &mut allocator,
                    )
                    .unwrap();
                    cell.initialize_order(root, [(0, span())].into_iter(), &mut allocator)
                        .unwrap();
                    cell.retire(&mut allocator, context, |_, _, _, _| {
                        panic!("Copy error has no owner")
                    })
                    .unwrap();
                    cell.recycle(&mut allocator).unwrap();
                    assert!(cell.data(&allocator).unwrap().iter().all(|&b| b == 0));
                }
                Ok(())
            },
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    assert!(frame.initialize(c::Slot(0), (), None).is_ok());
    let outcome = frame.finish(c::Exit::Return(None));
    assert!(outcome.report.primary().is_none());
    assert_eq!(allocator.stats(), before);
    storage.release(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn pending_aggregate_retirement_prevents_recycling_or_releasing_any_frame_prefix() {
    let program = checked(
        "struct Pair { first: core::Bytes, second: core::Bytes } fn pair(first: core::Bytes, second: core::Bytes) -> Pair { return Pair { first: first, second: second }; }",
    );
    let typed = typed(&program);
    let types = typed.places().declarations().types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let plan = plan(&layouts, typed);
    let (mut allocator, trace) = allocator(limits());
    let mut storage = plan.reserve(&mut allocator).unwrap();
    let mut owners = [
        Bytes::from_slice(&mut allocator, b"first").unwrap(),
        Bytes::from_slice(&mut allocator, b"second").unwrap(),
    ];
    let ids = [trace.borrow().calls - 1, trace.borrow().calls];
    let index = plan.return_cell();
    let cell = storage.cell_mut(index).unwrap();
    for (field, owner) in owners.iter_mut().enumerate() {
        let location = cell.child(cell.root(), field as u32, &allocator).unwrap();
        let token = owner.take_token(&allocator).unwrap();
        let start = location.offset() as usize;
        cell.data_mut(&mut allocator).unwrap()[start..start + 32].copy_from_slice(&token);
    }
    cell.initialize_order(
        cell.root(),
        [(0, span()), (1, span())].into_iter(),
        &mut allocator,
    )
    .unwrap();
    cell.begin_retirement(None, &mut allocator).unwrap();
    for rank in 0..2 {
        let cell = storage.cell_mut(index).unwrap();
        let leaf = cell.next_leaf(&mut allocator).unwrap().unwrap();
        assert_eq!(cell.recycle(&mut allocator), Err(l::Error::State));
        let offset = leaf.offset as usize;
        let mut token = l::Token {
            leaf,
            bytes: cell.data(&allocator).unwrap()[offset..offset + 32]
                .try_into()
                .unwrap(),
        };
        let released = trace.borrow().released.len();
        assert_eq!(storage.release(&mut allocator), Err(l::Error::State));
        assert_eq!(trace.borrow().released.len(), released);
        buffer::cleanup_token(types, &mut token, &mut allocator).unwrap();
        storage.cell_mut(index).unwrap().complete_leaf().unwrap();
        if rank == 0 {
            assert_eq!(
                storage.cell_mut(index).unwrap().recycle(&mut allocator),
                Err(l::Error::State)
            );
            assert_eq!(storage.release(&mut allocator), Err(l::Error::State));
        }
    }
    assert_eq!(
        trace
            .borrow()
            .released
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [ids[1], ids[0]]
    );
    storage
        .cell_mut(index)
        .unwrap()
        .recycle(&mut allocator)
        .unwrap();
    storage.release(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}
