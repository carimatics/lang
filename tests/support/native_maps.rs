use super::*;
use lang_runtime::map as mm;
use m::ownership::{self as o, core::Operation as Op};

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
pub(super) fn fault(error: mm::Error) -> c::Cause {
    c::Cause::Execution(match error {
        mm::Error::LoanLimit
        | mm::Error::Allocation(a::Error::Limit(_) | a::Error::OutOfMemory) => {
            c::ExecutionFailure::AllocationLimit
        }
        mm::Error::Allocation(a::Error::Overflow) => c::ExecutionFailure::CounterOverflow,
        mm::Error::WorkLimit | mm::Error::BorrowConflict => c::ExecutionFailure::Fault,
        _ => c::ExecutionFailure::Invariant,
    })
}
fn allocation<T>(value: Result<T, mm::Error>) -> Result<Result<T, buffer::AllocError>, c::Cause> {
    match value {
        Ok(value) => Ok(Ok(value)),
        Err(mm::Error::Allocation(a::Error::Limit(_))) => Ok(Err(buffer::AllocError::Limit)),
        Err(mm::Error::Allocation(a::Error::OutOfMemory)) => {
            Ok(Err(buffer::AllocError::OutOfMemory))
        }
        Err(mm::Error::Allocation(a::Error::Overflow)) => Ok(Err(buffer::AllocError::Overflow)),
        Err(error) => Err(fault(error)),
    }
}
pub(super) fn supports(op: Op) -> bool {
    matches!(
        op,
        Op::MapNew | Op::MapLen | Op::MapInsert | Op::MapGet | Op::MapRemove
    )
}
pub(super) fn view<'s, 'l>(
    store: &'s Store<'_, 'l>,
    read: usize,
) -> Result<mm::Borrowed<'s, 'l, 'l>, c::Cause> {
    store
        .loans
        .get(read)
        .and_then(|loan| loan.map_read.as_ref())
        .ok_or_else(bad)?
        .view(store.layouts, store.allocator)
        .map_err(fault)
}
fn owner<'l>(unit: &Unit<'_, '_, '_, 'l>, site: &n::CoreSite) -> Result<mm::Map<'l, 'l>, c::Cause> {
    let (_, mut token) = places::buffer_token(unit, site.arguments[0])?;
    let store = unit.store.borrow();
    mm::Map::from_token(
        store.layouts,
        site.type_arguments[0],
        site.type_arguments[1],
        &mut token,
        store.allocator,
    )
    .map_err(fault)
}
fn key(unit: &Unit<'_, '_, '_, '_>, site: &n::CoreSite) -> Result<mm::Key, c::Cause> {
    if site.type_arguments[0] == o::Type::I64 {
        Ok(mm::Key::I64(places::scalar(unit, site.arguments[1])?))
    } else {
        Ok(mm::Key::Text(
            places::buffer_token(unit, site.arguments[1])?.1,
        ))
    }
}
fn physical(unit: &Unit<'_, '_, '_, '_>, slot: usize) -> Result<Physical, c::Cause> {
    unit.ledger
        .as_ref()
        .unwrap()
        .value(unit.slot(slot))
        .map_err(invariant)?
        .physical()
}
fn retire_key(
    unit: &mut Unit<'_, '_, '_, '_>,
    site: &n::CoreSite,
    token: l::Token,
    reason: c::Reason,
) -> n::ResultStep {
    let shared = unit.store;
    let mut cleaner = c::Contextual::new(
        |token: &mut l::Token, _: c::Reason, _: &mut c::Context<'_>| {
            let mut store = shared.borrow_mut();
            let types = store.types;
            buffer::cleanup_token(types, token, store.allocator).map_err(|_| 94u32)?;
            if store.refuse_cleanup {
                Err(95)
            } else {
                Ok(())
            }
        },
    );
    let span = token.leaf.span;
    let anchor = unit.slot(site.arguments[0]);
    unit.ledger
        .as_mut()
        .unwrap()
        .retire_detached(anchor, token, span, reason, &mut cleaner)
        .map_err(invariant)
}
fn option(
    unit: &mut Unit<'_, '_, '_, '_>,
    site: &n::CoreSite,
    token: Option<[u8; 32]>,
) -> n::ResultStep {
    let output = Physical {
        frame: unit.depth(),
        cell: site.result,
    };
    {
        let mut store = unit.store.borrow_mut();
        let Store {
            pools, allocator, ..
        } = &mut *store;
        let cell = pools[output.frame].cell(output.cell).map_err(|_| bad())?;
        let root = cell.root();
        cell.prepare_case(root, u32::from(token.is_some()), allocator)
            .map_err(|_| bad())?;
        if let Some(token) = token {
            let offset = cell.child(root, 0, allocator).map_err(|_| bad())?.offset() as usize;
            cell.data_mut(allocator).map_err(|_| bad())?[offset..offset + 32]
                .copy_from_slice(&token);
        }
        cell.initialize_order(root, token.map(|_| (0, site.span)).into_iter(), allocator)
            .map_err(|_| bad())?;
    }
    values::publish(unit, site.result, output, site.span)
}
pub(super) fn call(unit: &mut Unit<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    match site.operation {
        Op::MapNew => {
            if !matches!(
                places::referent(unit, site.arguments[0])?,
                places::Target::Allocator
            ) {
                return Err(bad());
            }
            let outcome = {
                let mut store = unit.store.borrow_mut();
                let layouts = store.layouts;
                match allocation(mm::Map::new(
                    layouts,
                    site.type_arguments[0],
                    site.type_arguments[1],
                    store.allocator,
                ))? {
                    Ok(mut value) => Ok(core::Payload::Token(
                        value.take_token(store.allocator).map_err(fault)?,
                    )),
                    Err(error) => Err(l::Immediate::Allocation(error)),
                }
            };
            core::result(unit, site, outcome)
        }
        Op::MapLen => {
            let owner = owner(unit, site)?;
            let len = owner.len(unit.store.borrow().allocator).map_err(fault)?;
            values::write_scalar(unit, site.result, len, site.span)
        }
        Op::MapGet => {
            let loan = site.loan.ok_or_else(bad)?;
            let id = places::available(unit, loan)?;
            {
                let store = unit.store.borrow();
                if store.loans[id].map_read.is_some() || store.loans[id].vector_read.is_some() {
                    return Err(bad());
                }
            }
            let custody = shared::custody(unit, places::referent(unit, site.arguments[0])?)?;
            let mut owner = owner(unit, site)?;
            let key = key(unit, site)?;
            let borrowed = owner
                .borrow(key, unit.store.borrow_mut().allocator)
                .map_err(fault)?;
            let token = if let Some(read) = borrowed {
                let location = {
                    let store = unit.store.borrow();
                    read.view(store.layouts, store.allocator)
                        .map_err(fault)?
                        .root()
                };
                unit.store.borrow_mut().loans[id].map_read = Some(read);
                unit.store.borrow_mut().loans[id].custody = custody;
                Some(places::register_token(
                    unit,
                    loan,
                    places::Target::Map { read: id, location },
                    site.span,
                )?)
            } else {
                None
            };
            option(unit, site, token)
        }
        Op::MapRemove => {
            let mut owner = owner(unit, site)?;
            let key = key(unit, site)?;
            let output = Physical {
                frame: unit.depth(),
                cell: site.result,
            };
            let detached = {
                let mut store = unit.store.borrow_mut();
                let Store {
                    pools, allocator, ..
                } = &mut *store;
                let cell = pools[output.frame].cell(output.cell).map_err(|_| bad())?;
                let root = cell.root();
                cell.prepare_case(root, 1, allocator).map_err(|_| bad())?;
                let target = cell.child(root, 0, allocator).map_err(|_| bad())?;
                match owner
                    .remove_into(key, cell, target, allocator)
                    .map_err(fault)?
                {
                    Some(old) => {
                        cell.initialize_order(
                            root,
                            [(0, old.value_span.unwrap_or(site.span))].into_iter(),
                            allocator,
                        )
                        .map_err(|_| bad())?;
                        old.key
                    }
                    None => {
                        cell.prepare_case(root, 0, allocator).map_err(|_| bad())?;
                        cell.initialize_order(root, std::iter::empty(), allocator)
                            .map_err(|_| bad())?;
                        None
                    }
                }
            };
            values::publish(unit, site.result, output, site.span)?;
            if let Some(key) = detached {
                retire_key(unit, site, key, c::Reason::Explicit)?;
            }
            Ok(())
        }
        Op::MapInsert => insert(unit, site),
        _ => Err(bad()),
    }
}
fn insert(unit: &mut Unit<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    let mut owner = owner(unit, site)?;
    let key = physical(unit, site.arguments[1])?;
    let value = physical(unit, site.arguments[2])?;
    let output = Physical {
        frame: unit.depth(),
        cell: site.result,
    };
    if key.frame != output.frame || value.frame != output.frame {
        return Err(bad());
    }
    let key_copy = values::is_copy(unit, site.arguments[1]);
    let value_copy = values::is_copy(unit, site.arguments[2]);
    let (published, detached) = {
        let mut store = unit.store.borrow_mut();
        let Store {
            pools, allocator, ..
        } = &mut *store;
        let pool = &mut pools[output.frame];
        let cell = pool.cell(output.cell).map_err(|_| bad())?;
        let root = cell.root();
        cell.prepare_case(root, 0, allocator).map_err(|_| bad())?;
        let option = cell.child(root, 0, allocator).map_err(|_| bad())?;
        cell.prepare_case(option, 1, allocator).map_err(|_| bad())?;
        let target = cell.child(option, 0, allocator).map_err(|_| bad())?;
        let spans = mm::Spans {
            key: Some(site.span),
            value: Some(site.span),
        };
        let outcome = if key.cell == value.cell {
            let (input, output) = pool
                .two_cells_mut(key.cell, output.cell)
                .map_err(|_| bad())?;
            allocation(owner.insert_repeated_integer_into(input, spans, output, target, allocator))?
        } else {
            let [key, value, output] = pool
                .three_cells_mut([key.cell, value.cell, output.cell])
                .map_err(|_| bad())?;
            allocation(owner.insert_into(key, value, spans, output, target, allocator))?
        };
        let published = outcome.is_ok();
        if published {
            if !key_copy {
                pool.cell_mut(key.cell)
                    .map_err(|_| bad())?
                    .recycle(allocator)
                    .map_err(|_| bad())?;
            }
            if !value_copy {
                pool.cell_mut(value.cell)
                    .map_err(|_| bad())?
                    .recycle(allocator)
                    .map_err(|_| bad())?;
            }
        }
        let cell = pool.cell(output.cell).map_err(|_| bad())?;
        let detached = match outcome {
            Ok(Some(old)) => {
                cell.initialize_order(
                    option,
                    [(0, old.value_span.unwrap_or(site.span))].into_iter(),
                    allocator,
                )
                .map_err(|_| bad())?;
                old.key
            }
            Ok(None) => {
                cell.prepare_case(option, 0, allocator).map_err(|_| bad())?;
                cell.initialize_order(option, std::iter::empty(), allocator)
                    .map_err(|_| bad())?;
                None
            }
            Err(error) => {
                cell.prepare_case(root, 1, allocator).map_err(|_| bad())?;
                let target = cell.child(root, 0, allocator).map_err(|_| bad())?;
                cell.write_immediate(target, l::Immediate::Allocation(error), allocator)
                    .map_err(|_| bad())?;
                None
            }
        };
        cell.initialize_order(root, [(0, site.span)].into_iter(), allocator)
            .map_err(|_| bad())?;
        (published, detached)
    };
    if published {
        for (slot, copied) in [
            (site.arguments[1], key_copy),
            (site.arguments[2], value_copy),
        ] {
            if !copied {
                let slot = unit.slot(slot);
                unit.ledger
                    .as_mut()
                    .unwrap()
                    .take_owner(slot)
                    .map_err(invariant)?;
            }
        }
    }
    values::publish(unit, site.result, output, site.span)?;
    if let Some(key) = detached {
        retire_key(unit, site, key, c::Reason::Replacement)?;
    }
    if !published {
        for (slot, copied) in [
            (site.arguments[2], value_copy),
            (site.arguments[1], key_copy),
        ] {
            if !copied {
                let slot = unit.slot(slot);
                unit.ledger
                    .as_mut()
                    .unwrap()
                    .drop_value(slot)
                    .map_err(invariant)?;
            }
        }
    }
    Ok(())
}

const EXAMPLES: [(&str, i64); 4] = [
    (include_str!("../../examples/scripts/map.lang"), 49),
    (include_str!("../../examples/scripts/map_owned.lang"), 33),
    (
        include_str!("../../examples/analysis/native_map_nested.lang"),
        2,
    ),
    (
        include_str!("../../examples/analysis/native_map_text_probe.lang"),
        1,
    ),
];

#[test]
fn native_maps_owned_nested_and_text_probes_match_vm() {
    for (text, expected) in EXAMPLES {
        let normal = execute_unit(text, &[], None, 8, 1000, None, false);
        assert_eq!(normal.scalar, Some(expected), "{:?}", normal.failure);
        let source = source(text);
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let result = lang_vm::unit::owned::execute_entry(
            source.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Default::default(),
        )
        .unwrap();
        assert_eq!(result.value, expected);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn native_maps_retire_each_fuel_and_provider_failure_prefix() {
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
fn native_map_cleanup_poison_protects_displaced_values() {
    for (text, _) in &EXAMPLES[1..] {
        let failed = execute_unit(text, &[], None, 8, 1000, None, true);
        assert!(failed.failure.is_some());
        assert_eq!(failed.scalar, None);
    }
}

#[test]
fn native_map_metadata_preserves_both_generic_arguments_and_budgets() {
    core::check_metadata(EXAMPLES[1].0);
}

#[test]
fn native_map_repeated_integer_operand_matches_reverified_vm() {
    let source = source(EXAMPLES[0].0);
    let mut table = source.mir().activity().moved().typed().clone().into_table();
    let mut changed = 0;
    for function in &mut table.functions {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if let o::bodies::Op::CoreCall(call) = &mut instruction.op
                    && call.operation == Op::MapInsert
                {
                    call.arguments[2] = call.arguments[1];
                    changed += 1;
                }
            }
        }
    }
    assert_eq!(changed, 2);
    let flow = values::flow(table);
    let normal = execute_flow(&flow, &[], None, 8, 1000, None, false);
    assert_eq!(normal.scalar, Some(3), "{:?}", normal.failure);
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let vm = lang_vm::unit::owned::execute_entry(
        &flow,
        &mut instance,
        Some(&mut allocator),
        Default::default(),
    )
    .unwrap();
    assert_eq!(vm.value, 3);
    assert_eq!(allocator.stats().live_allocations, 0);
}
