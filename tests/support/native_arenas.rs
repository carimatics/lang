use super::*;
use lang_runtime::{arena::managed as g, inspection as ins};
use m::ownership::{self as o, core::Operation as Op};

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
fn outcome<T>(value: Result<T, g::Error>) -> Result<Result<T, l::ArenaError>, c::Cause> {
    if let Err(error) = &value
        && let Some(failure) = lang_runtime::execution::service_failure(error)
    {
        return Err(c::Cause::Execution(failure.into()));
    }

    match value {
        Ok(value) => Ok(Ok(value)),
        Err(g::Error {
            kind: g::Kind::Arena(error),
            ..
        }) => Ok(Err(error)),
        Err(g::Error {
            kind: g::Kind::Allocator(_) | g::Kind::Buffer(_),
            ..
        }) => Err(c::Cause::Execution(c::ExecutionFailure::Fault)),
        Err(_) => Err(bad()),
    }
}
fn inspect<P: a::Provider>(
    layouts: &l::Layouts<'_>,
    inspector: &mut Option<ins::Inspector<'_, '_>>,
    token: &l::Token,
    arena: &g::Arena<'_, '_>,
    allocator: &a::Allocator<P>,
    budget: &mut g::PublicationBudget,
) -> Result<(), g::Error> {
    let mut read = inspector
        .as_mut()
        .ok_or(g::Error {
            kind: g::Kind::State,
            span: None,
        })?
        .token(token, allocator)?;
    while let Some(observed) = budget.inspect_next(&mut read)? {
        let mut bytes = observed.bytes;
        let Some(l::Layout {
            kind: l::Kind::Token(constructor),
            ..
        }) = layouts.get(observed.ty)
        else {
            return Err(g::Error {
                kind: g::Kind::State,
                span: observed.span,
            });
        };
        match constructor {
            o::Constructor::Arena(ty) => {
                arena.resolve(g::Reference::from_token(bytes)?, *ty, allocator)?
            }
            o::Constructor::Shared(_) | o::Constructor::Weak(_) => {
                return Err(g::Error {
                    kind: g::Kind::Arena(l::ArenaError::CrossRegion),
                    span: observed.span,
                });
            }
            o::Constructor::Builtin(o::Builtin::Bytes) => {
                buffer::Bytes::from_token(&mut bytes, allocator)
                    .and_then(|v| v.len(allocator))
                    .map_err(|e| g::Error {
                        kind: g::Kind::Buffer(e),
                        span: observed.span,
                    })?;
            }
            o::Constructor::Builtin(o::Builtin::Text) => {
                let text = buffer::Text::from_token_metered(&mut bytes, allocator, &mut |n| {
                    budget.service(n)
                })
                .map_err(|e| g::Error {
                    kind: g::Kind::Buffer(e),
                    span: observed.span,
                })?;
                let length = usize::try_from(text.len(allocator).map_err(|e| g::Error {
                    kind: g::Kind::Buffer(e),
                    span: observed.span,
                })?)
                .map_err(|_| g::Error {
                    kind: g::Kind::State,
                    span: observed.span,
                })?;
                budget.observe(length)?;
                text.as_str(allocator).map_err(|e| g::Error {
                    kind: g::Kind::Buffer(e),
                    span: observed.span,
                })?;
            }
            _ => {
                return Err(g::Error {
                    kind: g::Kind::Unsupported(observed.ty),
                    span: observed.span,
                });
            }
        }
    }
    Ok(())
}

pub(super) fn supports(op: Op) -> bool {
    matches!(op, Op::ArenaNew | Op::ArenaRead | Op::ArenaReplace)
}
pub(super) fn view<'s, 'l>(
    store: &'s Store<'_, 'l>,
    owner: usize,
    read: usize,
) -> Result<g::View<'s, 'l, 'l>, c::Cause> {
    let (expected, read) = store
        .loans
        .get(read)
        .and_then(|loan| loan.arena_read.as_ref())
        .ok_or_else(bad)?;
    if *expected != owner || !store.arena_live[owner] {
        return Err(bad());
    }
    store.arena[owner]
        .as_ref()
        .ok_or_else(bad)?
        .payload(read, store.allocator)
        .map_err(service_invariant)
}
pub(super) fn enter(unit: &mut Unit<'_, '_, '_, '_>, site: &n::ArenaSite) -> n::ResultStep {
    if !matches!(
        places::referent(unit, site.allocator)?,
        places::Target::Allocator
    ) {
        return Err(bad());
    }
    let depth = unit.depth();
    let id = depth * unit.stride + site.owner;
    {
        let mut store = unit.store.borrow_mut();
        let Store {
            arena,
            arena_live,
            allocator,
            ..
        } = &mut *store;
        if arena_live[id] {
            return Err(bad());
        }
        arena[id]
            .as_mut()
            .ok_or_else(bad)?
            .open(allocator)
            .map_err(|_| c::Cause::Execution(c::ExecutionFailure::AllocationLimit))?;
        arena_live[id] = true;
    }
    values::publish(
        unit,
        site.owner,
        Physical {
            frame: depth,
            cell: site.owner,
        },
        site.span,
    )
}
fn result(
    unit: &mut Unit<'_, '_, '_, '_>,
    site: &n::CoreSite,
    value: Result<Option<g::Reference>, l::ArenaError>,
) -> n::ResultStep {
    core::result(
        unit,
        site,
        value
            .map(|value| match value {
                Some(reference) => core::Payload::Token(reference.token()),
                None => core::Payload::Immediate(l::Immediate::Unit),
            })
            .map_err(l::Immediate::Arena),
    )
}
pub(super) fn call(unit: &mut Unit<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    let owner = places::arena_owner(unit, site.arguments[0])?;
    if site.operation == Op::ArenaRead {
        let reference = g::Reference::from_token(places::buffer_token(unit, site.arguments[1])?.1)
            .map_err(service_invariant)?;
        let loan = site.loan.ok_or_else(bad)?;
        let id = places::available(unit, loan)?;
        {
            let store = unit.store.borrow();
            let entry = &store.loans[id];
            if entry.arena_read.is_some()
                || entry.shared_read.is_some()
                || entry.map_read.is_some()
                || entry.vector_read.is_some()
            {
                return Err(bad());
            }
        }
        let value = {
            let mut store = unit.store.borrow_mut();
            let Store {
                arena, allocator, ..
            } = &mut *store;
            outcome(arena[owner].as_mut().ok_or_else(bad)?.read(
                reference,
                site.type_arguments[0],
                allocator,
            ))?
        };
        let value = match value {
            Ok(read) => {
                let location = {
                    let store = unit.store.borrow();
                    store.arena[owner]
                        .as_ref()
                        .ok_or_else(bad)?
                        .payload(&read, store.allocator)
                        .map_err(service_invariant)?
                        .root()
                };
                unit.store.borrow_mut().loans[id].arena_read = Some((owner, read));
                Ok(core::Payload::Token(places::register_token(
                    unit,
                    loan,
                    places::Target::Arena {
                        owner,
                        read: id,
                        location,
                    },
                    site.span,
                )?))
            }
            Err(error) => Err(l::Immediate::Arena(error)),
        };
        return core::result(unit, site, value);
    }
    store(unit, site, owner)
}
enum Transfer {
    Copy,
    Moved,
    Replaced(Option<m::Span>),
    Rejected,
}
fn store(unit: &mut Unit<'_, '_, '_, '_>, site: &n::CoreSite, owner: usize) -> n::ResultStep {
    let source_slot = *site.arguments.last().ok_or_else(bad)?;
    let source = unit
        .ledger
        .as_ref()
        .unwrap()
        .value(unit.slot(source_slot))
        .map_err(invariant)?
        .physical()?;
    let copied = values::is_copy(unit, source_slot);
    let reference = if site.operation == Op::ArenaReplace {
        Some(
            g::Reference::from_token(places::buffer_token(unit, site.arguments[1])?.1)
                .map_err(service_invariant)?,
        )
    } else {
        None
    };
    let (value, transfer) = {
        let mut state = unit.store.borrow_mut();
        let Store {
            pools,
            arena,
            allocator,
            layouts,
            inspection,
            ..
        } = &mut *state;
        let input = pools[source.frame]
            .cell_mut(source.cell)
            .map_err(service_invariant)?;
        let arena = arena[owner].as_mut().ok_or_else(bad)?;
        let inspect = |token: &l::Token,
                       arena: &g::Arena<'_, '_>,
                       allocator: &a::Allocator<Provider>,
                       budget: &mut g::PublicationBudget| {
            inspect(layouts, inspection, token, arena, allocator, budget)
        };
        let value = if let Some(reference) = reference {
            if copied {
                outcome(arena.replace_copy(reference, input, site.span, allocator, inspect))?
                    .map(|()| (None, Transfer::Copy))
            } else {
                outcome(arena.replace_owned(reference, input, site.span, allocator, inspect))?
                    .map(|span| (None, Transfer::Replaced(span)))
            }
        } else {
            outcome(if copied {
                arena.allocate_copy(input, Some(site.span), allocator, inspect)
            } else {
                arena.allocate_from(input, Some(site.span), allocator, inspect)
            })?
            .map(|reference| {
                (
                    Some(reference),
                    if copied {
                        Transfer::Copy
                    } else {
                        Transfer::Moved
                    },
                )
            })
        };
        let (value, transfer) = match value {
            Ok((value, transfer)) => (Ok(value), transfer),
            Err(error) => (Err(error), Transfer::Rejected),
        };
        if matches!(transfer, Transfer::Moved) {
            input.recycle(allocator).map_err(service_invariant)?;
        }
        (value, transfer)
    };
    result(unit, site, value)?;
    if !copied {
        let slot = unit.slot(source_slot);
        let ledger = unit.ledger.as_mut().unwrap();
        match transfer {
            Transfer::Moved => {
                ledger.take_owner(slot).map_err(invariant)?;
            }
            Transfer::Replaced(span) => ledger.retire_replaced(slot, span).map_err(invariant)?,
            Transfer::Rejected => ledger.drop_value(slot).map_err(invariant)?,
            Transfer::Copy => return Err(bad()),
        }
    }
    Ok(())
}
#[test]
fn native_arenas_owned_nested_reuse_and_brands_match_vm() {
    for (text, expected) in EXAMPLES {
        let result = execute_unit(text, &[], None, 8, 1000, None, false);
        assert_eq!(result.scalar, Some(expected), "{:?}", result.failure);
        let source = source(text);
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let vm = lang_vm::unit::owned::execute_entry(
            source.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Default::default(),
        )
        .unwrap();
        assert_eq!(vm.value, expected);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

const EXAMPLES: [(&str, i64); 5] = [
    (include_str!("../../examples/scripts/arena.lang"), 9),
    (include_str!("../../examples/scripts/arena_owned.lang"), 5),
    (include_str!("../../examples/scripts/nested_arena.lang"), 4),
    (
        include_str!("../../examples/analysis/native_arena_reuse.lang"),
        3,
    ),
    (
        include_str!("../../examples/analysis/native_arena_regions.lang"),
        40,
    ),
];

#[test]
fn native_arena_retire_each_fuel_and_provider_failure_prefix() {
    for (text, _) in EXAMPLES {
        let source = source(text);
        let flow = source.failure_flow();
        let types = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let program = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
        let run = |fuel, refuse| {
            execute_prepared(
                flow,
                &layouts,
                &program,
                Run {
                    inputs: &[],
                    fuel,
                    max_frames: 8,
                    cells: 1000,
                    refuse,
                    refuse_cleanup: false,
                },
            )
        };
        let normal = run(None, None);
        for fuel in 0..normal.fuel {
            let failed = run(Some(fuel), None);
            assert_eq!(failed.scalar, None);
            assert_eq!(failed.fuel, fuel);
            assert_eq!(
                failed.failure.unwrap().reason,
                n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::FuelLimit))
            );
            assert!(failed.report.secondary().is_empty());
        }
        for request in 1..=normal.requests {
            let failed = run(None, Some(request));
            if let Some(failure) = failed.failure {
                if failure.reason
                    == n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::Fault))
                {
                    assert!(
                        flow.source().activity().moved().typed().functions()
                            [failure.function.0 as usize]
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .any(|i| i.span == failure.span
                                && matches!(i.op, o::bodies::Op::Call { .. })),
                        "request {request}: {failure:?}"
                    );
                } else {
                    assert!(
                        matches!(
                            failure.reason,
                            n::Reason::Runtime(c::Cause::Execution(
                                c::ExecutionFailure::Panic | c::ExecutionFailure::AllocationLimit
                            ))
                        ),
                        "request {request}: {failure:?}"
                    );
                }
            } else {
                assert!(failed.scalar.is_some());
            }
            assert!(failed.report.secondary().is_empty());
        }
    }
}

#[test]
fn native_arena_cleanup_poison_discards_results_and_closes_owners() {
    for (text, _) in EXAMPLES {
        let failed = execute_unit(text, &[], None, 8, 1000, None, true);
        assert!(failed.failure.is_some());
        assert_eq!(failed.scalar, None);
    }
}

#[test]
fn native_arena_metadata_preserves_owner_slots_and_budgets() {
    core::check_metadata(EXAMPLES[2].0);
}

#[test]
fn native_arena_publication_callback_rejects_foreign_indirect_values() {
    let mut text = EXAMPLES[4].0.to_owned();
    text.push_str("fn unused(owner: shared i64) -> i64 { let mut held: core::Option<core::Vec<shared i64>> = core::Option::None; return 0; }");
    let source = source(&text);
    let types = source
        .failure_flow()
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let locate = |constructor| {
        o::Type::Constructed(o::ConstructedId(
            types
                .table()
                .constructed
                .iter()
                .position(|ty| ty.constructor == constructor)
                .unwrap() as u32,
        ))
    };
    let reference_ty = locate(o::Constructor::Arena(o::Type::I64));
    let shared_ty = locate(o::Constructor::Shared(o::Type::I64));
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut outer = g::Arena::prepare(
        &layouts,
        Default::default(),
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let mut inner = g::Arena::prepare(
        &layouts,
        Default::default(),
        Default::default(),
        outer.usage(),
    )
    .unwrap();
    let mut heap = lang_runtime::shared::managed::Heap::prepare(
        &layouts,
        Default::default(),
        Default::default(),
        inner.usage(),
    )
    .unwrap();
    let mut inspection =
        Some(ins::Inspector::prepare(&layouts, 4096, Default::default(), heap.usage()).unwrap());
    let mut retirement = lang_runtime::retirement::Drain::prepare(
        &layouts,
        &allocator,
        Default::default(),
        inspection.as_ref().unwrap().usage(),
    )
    .unwrap();
    outer.open(&mut allocator).unwrap();
    inner.open(&mut allocator).unwrap();
    heap.open(&mut allocator).unwrap();
    let mut scalar = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    scalar
        .write_immediate(scalar.root(), l::Immediate::I64(7), &mut allocator)
        .unwrap();
    let reference = outer
        .allocate_copy(&mut scalar, None, &mut allocator, |_, _, _, _| Ok(()))
        .unwrap();
    let strong = heap
        .allocate_copy(&mut scalar, None, &mut allocator)
        .unwrap();
    let mut inputs = Vec::new();
    for (element, token) in [
        (reference_ty, reference.token()),
        (shared_ty, strong.into_token()),
    ] {
        let mut value = l::Reservation::allocate(&layouts, element, &mut allocator).unwrap();
        value
            .data_mut(&mut allocator)
            .unwrap()
            .copy_from_slice(&token);
        let mut vector =
            lang_runtime::vector::Vector::new(&layouts, element, &mut allocator).unwrap();
        if types.properties(element).unwrap().is_copy {
            vector.push_copy(&mut value, None, &mut allocator).unwrap();
        } else {
            vector.push_from(&mut value, None, &mut allocator).unwrap();
            value.recycle(&mut allocator).unwrap();
        }
        value.release_storage(&mut allocator).unwrap();
        let token = vector.take_token(&allocator).unwrap();
        let mut input = l::Reservation::allocate(
            &layouts,
            locate(o::Constructor::Vec(element)),
            &mut allocator,
        )
        .unwrap();
        input
            .data_mut(&mut allocator)
            .unwrap()
            .copy_from_slice(&token);
        let error = inner
            .allocate_from(
                &mut input,
                None,
                &mut allocator,
                |token, arena, allocator, budget| {
                    inspect(&layouts, &mut inspection, token, arena, allocator, budget)
                },
            )
            .unwrap_err();
        assert!(matches!(
            error.kind,
            g::Kind::Arena(l::ArenaError::CrossRegion)
        ));
        assert_eq!(
            input.read_token(input.root(), &allocator).unwrap().bytes,
            token,
            "rejection preserves the incoming owner"
        );
        inputs.push(input);
    }
    let mut instance = c::Instance::default();
    let cleaner = c::Contextual::new(|_: &mut (), _: c::Reason, context: &mut c::Context<'_>| {
        for input in &mut inputs {
            input
                .cleanup(&mut allocator, context, |token, allocator, _, context| {
                    retirement.token(
                        token,
                        Some(&mut heap),
                        allocator,
                        context,
                        |leaf, allocator, _, _| {
                            buffer::cleanup_token(types, leaf, allocator).map_err(|_| 94u32)
                        },
                    )
                })
                .map_err(|_| 108u32)?;
        }
        for arena in [&mut inner, &mut outer] {
            arena
                .cleanup(
                    g::Exit::Normal,
                    &mut allocator,
                    context,
                    |token, allocator, _, context| {
                        retirement.token(
                            token,
                            Some(&mut heap),
                            allocator,
                            context,
                            |leaf, allocator, _, _| {
                                buffer::cleanup_token(types, leaf, allocator).map_err(|_| 94u32)
                            },
                        )
                    },
                )
                .map_err(|_| 109u32)?;
        }
        scalar.release_storage(&mut allocator).map_err(|_| 110u32)?;
        heap.close(&mut allocator).map_err(|_| 111u32)?;
        Ok(())
    });
    let mut frame = instance
        .enter_cleaner(
            c::Limits {
                slots: 1,
                scopes: 1,
                failure_records: 16,
            },
            cleaner,
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    frame.initialize(c::Slot(0), (), None).unwrap();
    let completion = frame.finish(c::Exit::Return(None));
    assert!(!completion.poisoned, "{:?}", completion.report);
    assert!(completion.report.primary().is_none());
    assert_eq!(allocator.stats().live_allocations, 0);
}
