use super::*;
use lang_runtime::shared::managed as sh;
use m::ownership::{self as o, core::Operation as Op};

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
fn outcome<T>(value: Result<T, sh::Error>) -> Result<Result<T, l::OwnershipError>, c::Cause> {
    match value {
        Ok(value) => Ok(Ok(value)),
        Err(sh::Error {
            kind: sh::Kind::Ownership(error),
            ..
        }) => Ok(Err(error)),
        Err(sh::Error {
            kind: sh::Kind::Allocator(_) | sh::Kind::Buffer(_),
            ..
        }) => Err(c::Cause::Execution(c::ExecutionFailure::Fault)),
        Err(_) => Err(bad()),
    }
}
pub(super) fn supports(op: Op) -> bool {
    matches!(
        op,
        Op::SharedNew
            | Op::SharedClone
            | Op::SharedRead
            | Op::SharedReplace
            | Op::WeakNew
            | Op::WeakClone
            | Op::WeakUpgrade
    )
}
pub(super) fn view<'s, 'l>(
    store: &'s Store<'_, 'l>,
    read: usize,
    ty: o::Type,
) -> Result<lang_runtime::shared::storage::View<'s, 'l, 'l>, c::Cause> {
    let read = store
        .loans
        .get(read)
        .and_then(|loan| loan.shared_read)
        .ok_or_else(bad)?;
    store
        .heap
        .as_ref()
        .ok_or_else(bad)?
        .view(read, ty, store.allocator)
        .map_err(|_| bad())
}
pub(super) fn custody(
    unit: &Unit<'_, '_, '_, '_>,
    target: places::Target,
) -> Result<Option<sh::Custody>, c::Cause> {
    let store = unit.store.borrow();
    match target {
        places::Target::Shared { read, ty, .. } => {
            let read = store.loans[read].shared_read.ok_or_else(bad)?;
            store
                .heap
                .as_ref()
                .ok_or_else(bad)?
                .custody(read, ty, store.allocator)
                .map(Some)
                .map_err(|_| bad())
        }
        places::Target::Map { read, .. } | places::Target::Vector { read, .. } => {
            Ok(store.loans[read].custody)
        }
        _ => Ok(None),
    }
}
fn result(
    unit: &mut Unit<'_, '_, '_, '_>,
    site: &n::CoreSite,
    value: Result<Option<[u8; 32]>, l::OwnershipError>,
) -> n::ResultStep {
    core::result(
        unit,
        site,
        value
            .map(|token| match token {
                Some(token) => core::Payload::Token(token),
                None => core::Payload::Immediate(l::Immediate::Unit),
            })
            .map_err(l::Immediate::Ownership),
    )
}
pub(super) fn call(unit: &mut Unit<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    if matches!(site.operation, Op::SharedNew | Op::SharedReplace) {
        return store(unit, site);
    }
    let target = places::referent(unit, site.arguments[0])?;
    let observed = places::observation(unit, target)?;
    let custody = custody(unit, target)?;
    match site.operation {
        Op::SharedClone | Op::WeakNew | Op::WeakClone | Op::WeakUpgrade => {
            let action = match site.operation {
                Op::SharedClone => sh::FieldAction::CloneStrong,
                Op::WeakNew => sh::FieldAction::NewWeak,
                Op::WeakClone => sh::FieldAction::CloneWeak,
                Op::WeakUpgrade => sh::FieldAction::UpgradeWeak,
                _ => return Err(bad()),
            };
            let value = {
                let mut state = unit.store.borrow_mut();
                let Store {
                    heap, allocator, ..
                } = &mut *state;
                outcome(
                    heap.as_mut()
                        .ok_or_else(bad)?
                        .retain_observed(&observed, custody, action, allocator),
                )?
                .map(|value| {
                    Some(match value {
                        sh::Retained::Strong(value) => value.into_token(),
                        sh::Retained::Weak(value) => value.into_token(),
                    })
                })
            };
            result(unit, site, value)
        }
        Op::SharedRead => {
            let loan = site.loan.ok_or_else(bad)?;
            let id = places::available(unit, loan)?;
            {
                let state = unit.store.borrow();
                let entry = &state.loans[id];
                if entry.shared_read.is_some()
                    || entry.vector_read.is_some()
                    || entry.map_read.is_some()
                {
                    return Err(bad());
                }
            }
            let value = {
                let mut state = unit.store.borrow_mut();
                let Store {
                    heap, allocator, ..
                } = &mut *state;
                outcome(
                    heap.as_mut()
                        .ok_or_else(bad)?
                        .read_observed(&observed, custody, allocator),
                )?
            };
            let value = match value {
                Ok(read) => {
                    let ty = site.type_arguments[0];
                    let location = {
                        let state = unit.store.borrow();
                        state
                            .heap
                            .as_ref()
                            .ok_or_else(bad)?
                            .view(read, ty, state.allocator)
                            .map_err(|_| bad())?
                            .root()
                    };
                    unit.store.borrow_mut().loans[id].shared_read = Some(read);
                    Ok(Some(places::register_token(
                        unit,
                        loan,
                        places::Target::Shared {
                            read: id,
                            ty,
                            location,
                        },
                        site.span,
                    )?))
                }
                Err(error) => Err(error),
            };
            result(unit, site, value)
        }
        _ => Err(bad()),
    }
}

enum Transfer {
    Copy,
    Moved,
    Replaced(Option<m::Span>),
    Rejected,
}
fn store(unit: &mut Unit<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    let source_slot = *site.arguments.last().ok_or_else(bad)?;
    let physical = unit
        .ledger
        .as_ref()
        .unwrap()
        .value(unit.slot(source_slot))
        .map_err(invariant)?
        .physical()?;
    let copied = values::is_copy(unit, source_slot);
    let target = if site.operation == Op::SharedReplace {
        Some(places::referent(unit, site.arguments[0])?)
    } else {
        if !matches!(
            places::referent(unit, site.arguments[0])?,
            places::Target::Allocator
        ) {
            return Err(bad());
        }
        None
    };
    let observed = target
        .map(|target| places::observation(unit, target))
        .transpose()?;
    let custody = target
        .map(|target| custody(unit, target))
        .transpose()?
        .flatten();
    let (value, transfer) = {
        let mut state = unit.store.borrow_mut();
        let Store {
            pools,
            heap,
            allocator,
            ..
        } = &mut *state;
        let heap = heap.as_mut().ok_or_else(bad)?;
        let ready = if heap.is_open() {
            Ok(())
        } else {
            outcome(heap.open(allocator))?
        };
        let input = pools[physical.frame]
            .cell_mut(physical.cell)
            .map_err(|_| bad())?;
        let value = match ready {
            Err(error) => Err(error),
            Ok(()) if target.is_some() => {
                if copied {
                    outcome(heap.replace_observed_copy(
                        observed.as_ref().unwrap(),
                        custody,
                        input,
                        site.span,
                        allocator,
                    ))?
                    .map(|()| (None, Transfer::Copy))
                } else {
                    outcome(heap.replace_observed(
                        observed.as_ref().unwrap(),
                        custody,
                        input,
                        site.span,
                        allocator,
                    ))?
                    .map(|span| (None, Transfer::Replaced(span)))
                }
            }
            Ok(()) => outcome(if copied {
                heap.allocate_copy(input, Some(site.span), allocator)
            } else {
                heap.allocate_from(input, Some(site.span), allocator)
            })?
            .map(|owner| {
                (
                    Some(owner.into_token()),
                    if copied {
                        Transfer::Copy
                    } else {
                        Transfer::Moved
                    },
                )
            }),
        };
        let (value, transfer) = match value {
            Ok((token, transfer)) => (Ok(token), transfer),
            Err(error) => (Err(error), Transfer::Rejected),
        };
        if matches!(transfer, Transfer::Moved) {
            input.recycle(allocator).map_err(|_| bad())?;
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
fn native_shared_owned_cycles_custody_and_weak_match_vm() {
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
    (include_str!("../../examples/scripts/shared.lang"), 9),
    (include_str!("../../examples/scripts/shared_owned.lang"), 13),
    (include_str!("../../examples/scripts/nested_cycle.lang"), 21),
    (
        include_str!("../../examples/analysis/native_shared_custody.lang"),
        24,
    ),
    (
        include_str!("../../examples/analysis/native_weak_expired.lang"),
        1,
    ),
];

#[test]
fn native_shared_retire_each_fuel_and_provider_failure_prefix() {
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
fn native_shared_cleanup_poison_releases_payloads_and_weak_metadata() {
    for (text, _) in EXAMPLES {
        let failed = execute_unit(text, &[], None, 8, 1000, None, true);
        assert!(failed.failure.is_some());
        assert_eq!(failed.scalar, None);
    }
}

#[test]
fn native_shared_metadata_preserves_generic_types_and_budgets() {
    core::check_metadata(EXAMPLES[1].0);
}
