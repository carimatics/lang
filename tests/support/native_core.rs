use super::*;
use m::ownership::core::Operation as Op;

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
fn fault<T>(_: T) -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Fault)
}
pub(super) enum Payload {
    Token([u8; 32]),
    Immediate(l::Immediate),
}
pub(super) fn result(
    unit: &mut Unit<'_, '_, '_, '_>,
    site: &n::CoreSite,
    outcome: Result<Payload, l::Immediate>,
) -> n::ResultStep {
    let (case, payload) = match outcome {
        Ok(value) => (0, value),
        Err(error) => (1, Payload::Immediate(error)),
    };
    let physical = Physical {
        frame: unit.depth(),
        cell: site.result,
    };
    {
        let mut store = unit.store.borrow_mut();
        let Store {
            pools, allocator, ..
        } = &mut *store;
        let cell = pools[physical.frame].cell(physical.cell).map_err(fault)?;
        let root = cell.root();
        cell.prepare_case(root, case, allocator).map_err(fault)?;
        let at = cell.child(root, 0, allocator).map_err(fault)?;
        match payload {
            Payload::Immediate(value) => {
                cell.write_immediate(at, value, allocator).map_err(fault)?
            }
            Payload::Token(token) => {
                let start = at.offset() as usize;
                cell.data_mut(allocator).map_err(fault)?[start..start + 32].copy_from_slice(&token);
            }
        }
        cell.initialize_order(root, [(0, site.span)].into_iter(), allocator)
            .map_err(fault)?;
    }
    values::publish(unit, site.result, physical, site.span)
}
fn bytes(
    unit: &Unit<'_, '_, '_, '_>,
    slot: usize,
) -> Result<(places::Target, buffer::Bytes), c::Cause> {
    let (target, mut token) = places::buffer_token(unit, slot)?;
    let value =
        buffer::Bytes::from_token(&mut token, unit.store.borrow().allocator).map_err(fault)?;
    Ok((target, value))
}
fn text(unit: &Unit<'_, '_, '_, '_>, slot: usize) -> Result<buffer::Text, c::Cause> {
    let (_, mut token) = places::buffer_token(unit, slot)?;
    buffer::Text::from_token(&mut token, unit.store.borrow().allocator).map_err(fault)
}
fn allocated(
    unit: &mut Unit<'_, '_, '_, '_>,
    site: &n::CoreSite,
    outcome: buffer::Outcome<[u8; 32], buffer::AllocError>,
) -> n::ResultStep {
    match outcome {
        Ok(token) => result(unit, site, Ok(Payload::Token(token))),
        Err(buffer::Failure::Error(error)) => {
            result(unit, site, Err(l::Immediate::Allocation(error)))
        }
        Err(buffer::Failure::Fault(error)) => Err(fault(error)),
    }
}
pub(super) fn call(unit: &mut Unit<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    let arg = |index: usize| site.arguments[index];
    match site.operation {
        Op::Allocator => places::register(
            unit,
            site.loan.ok_or_else(bad)?,
            places::Target::Allocator,
            site.result,
            site.span,
        ),
        Op::BytesNew => {
            if !matches!(places::referent(unit, arg(0))?, places::Target::Allocator) {
                return Err(bad());
            }
            let outcome = {
                let mut store = unit.store.borrow_mut();
                buffer::Bytes::new(store.allocator).and_then(|mut value| {
                    value
                        .take_token(store.allocator)
                        .map_err(buffer::Failure::Fault)
                })
            };
            allocated(unit, site, outcome)
        }
        Op::BytesLen => {
            let (_, bytes) = bytes(unit, arg(0))?;
            let length = bytes.len(unit.store.borrow().allocator).map_err(fault)?;
            values::write_scalar(unit, site.result, length, site.span)
        }
        Op::BytesGet => {
            let index = values::read_scalar(unit, arg(1))?;
            let (_, bytes) = bytes(unit, arg(0))?;
            let outcome = bytes.get(unit.store.borrow().allocator, index);
            match outcome {
                Ok(value) => result(unit, site, Ok(Payload::Immediate(l::Immediate::I64(value)))),
                Err(buffer::Failure::Error(error)) => {
                    result(unit, site, Err(l::Immediate::Bounds(error)))
                }
                Err(buffer::Failure::Fault(error)) => Err(fault(error)),
            }
        }
        Op::BytesPush | Op::BytesSet => {
            let index = values::read_scalar(unit, arg(1))?;
            let value = if site.operation == Op::BytesSet {
                values::read_scalar(unit, arg(2))?
            } else {
                index
            };
            let (target, mut bytes) = bytes(unit, arg(0))?;
            let (outcome, token) = {
                let mut store = unit.store.borrow_mut();
                let outcome = if site.operation == Op::BytesSet {
                    bytes.set(store.allocator, index, value)
                } else {
                    bytes.push(store.allocator, value)
                };
                let token = bytes.take_token(store.allocator).map_err(fault)?;
                (outcome, token)
            };
            places::store_token(unit, target, token)?;
            match outcome {
                Ok(()) => result(unit, site, Ok(Payload::Immediate(l::Immediate::Unit))),
                Err(buffer::Failure::Error(error)) => {
                    result(unit, site, Err(l::Immediate::Buffer(error)))
                }
                Err(buffer::Failure::Fault(error)) => Err(fault(error)),
            }
        }
        Op::BytesClone | Op::TextClone | Op::TextBytes | Op::TextConcat => {
            if !matches!(places::referent(unit, arg(0))?, places::Target::Allocator) {
                return Err(bad());
            }
            let outcome = match site.operation {
                Op::BytesClone => {
                    let (_, bytes) = bytes(unit, arg(1))?;
                    let mut store = unit.store.borrow_mut();
                    bytes.try_clone(store.allocator).and_then(|mut value| {
                        value
                            .take_token(store.allocator)
                            .map_err(buffer::Failure::Fault)
                    })
                }
                Op::TextClone => {
                    let text = text(unit, arg(1))?;
                    let mut store = unit.store.borrow_mut();
                    text.try_clone(store.allocator).and_then(|mut value| {
                        value
                            .take_token(store.allocator)
                            .map_err(buffer::Failure::Fault)
                    })
                }
                Op::TextBytes => {
                    let text = text(unit, arg(1))?;
                    let mut store = unit.store.borrow_mut();
                    text.to_bytes(store.allocator).and_then(|mut value| {
                        value
                            .take_token(store.allocator)
                            .map_err(buffer::Failure::Fault)
                    })
                }
                Op::TextConcat => {
                    let left = text(unit, arg(1))?;
                    let right = text(unit, arg(2))?;
                    let mut store = unit.store.borrow_mut();
                    left.concat(&right, store.allocator).and_then(|mut value| {
                        value
                            .take_token(store.allocator)
                            .map_err(buffer::Failure::Fault)
                    })
                }
                _ => unreachable!(),
            };
            allocated(unit, site, outcome)
        }
        Op::TextLen => {
            let text = text(unit, arg(0))?;
            let length = text.len(unit.store.borrow().allocator).map_err(fault)?;
            values::write_scalar(unit, site.result, length, site.span)
        }
        Op::TextEqual => {
            let left = text(unit, arg(0))?;
            let right = text(unit, arg(1))?;
            let equal = left
                .equal(&right, unit.store.borrow().allocator)
                .map_err(fault)?;
            values::write_scalar(unit, site.result, i64::from(equal), site.span)
        }
        Op::TextFromUtf8 => {
            let slot = unit.slot(arg(0));
            let physical = unit
                .ledger
                .as_ref()
                .unwrap()
                .value(slot)
                .map_err(invariant)?
                .physical()?;
            let outcome = {
                let mut store = unit.store.borrow_mut();
                let Store {
                    pools, allocator, ..
                } = &mut *store;
                let cell = pools[physical.frame].cell(physical.cell).map_err(fault)?;
                let mut token = cell
                    .read_token(cell.root(), allocator)
                    .map_err(fault)?
                    .bytes;
                let mut bytes = buffer::Bytes::from_token(&mut token, allocator).map_err(fault)?;
                let outcome = bytes.take_text(allocator);
                match outcome {
                    Ok(mut text) => Ok(Payload::Token(text.take_token(allocator).map_err(fault)?)),
                    Err(buffer::Failure::Error(error)) => Err(l::Immediate::Utf8(error)),
                    Err(buffer::Failure::Fault(error)) => return Err(fault(error)),
                }
            };
            {
                let mut store = unit.store.borrow_mut();
                let Store {
                    pools, allocator, ..
                } = &mut *store;
                pools[physical.frame]
                    .cell(physical.cell)
                    .map_err(fault)?
                    .data_mut(allocator)
                    .map_err(fault)?
                    .fill(0);
            }
            unit.ledger
                .as_mut()
                .unwrap()
                .take_owner(slot)
                .map_err(invariant)?;
            result(unit, site, outcome)
        }
        _ => Err(bad()),
    }
}

const EXAMPLES: [(&str, i64); 5] = [
    (
        include_str!("../../examples/analysis/native_core_bytes.lang"),
        1,
    ),
    (include_str!("../../examples/scripts/bytes.lang"), 0),
    (include_str!("../../examples/scripts/text.lang"), 2),
    (include_str!("../../examples/scripts/utf8.lang"), 3),
    (
        include_str!("../../examples/analysis/native_core_errors.lang"),
        1,
    ),
];

#[test]
fn native_bytes_and_text_match_vm_source_results() {
    for (text, expected) in EXAMPLES {
        let result = execute_unit(text, &[], None, 4, 1000, None, false);
        assert_eq!(result.scalar, Some(expected), "{:?}", result.failure);
        assert!(result.report.secondary().is_empty());
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

#[test]
fn native_bytes_and_text_retire_each_fuel_and_provider_failure_prefix() {
    for (text, _) in EXAMPLES {
        let normal = execute_unit(text, &[], None, 4, 1000, None, false);
        for fuel in 0..normal.fuel {
            let failed = execute_unit(text, &[], Some(fuel), 4, 1000, None, false);
            assert_eq!(failed.scalar, None);
            assert_eq!(failed.fuel, fuel);
            assert_eq!(
                failed.failure.unwrap().reason,
                n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::FuelLimit))
            );
            assert!(failed.report.secondary().is_empty());
        }
        for request in 1..=normal.requests {
            let failed = execute_unit(text, &[], None, 4, 1000, Some(request), false);
            if let Some(failure) = failed.failure {
                assert_eq!(
                    failure.reason,
                    n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::Panic))
                );
                assert!(failure.panic_code.is_some());
            } else {
                assert_eq!(failed.scalar, Some(-1));
            }
            assert!(failed.report.secondary().is_empty());
        }
    }
}

#[test]
fn native_core_cleanup_failure_poison_discards_the_scalar_return() {
    let failed = execute_unit(EXAMPLES[2].0, &[], None, 4, 1000, None, true);
    assert_eq!(failed.scalar, None);
    assert!(failed.failure.is_some());
}

#[test]
fn native_core_object_sites_preserve_source_operands_and_exact_budgets() {
    check_metadata(EXAMPLES[2].0);
}
pub(super) fn check_metadata(text: &str) {
    use object::{Object, ObjectSymbol};
    let source = source(text);
    let flow = source.failure_flow();
    let typed = flow.source().activity().moved().typed();
    let types = typed.places().declarations().types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let target = target.parse().unwrap();
        let artifact =
            n::emit_object_for_target(flow, &layouts, &target, Default::default(), 1000).unwrap();
        let mut sites = 0;
        for function_index in 0..flow.functions().len() {
            let function = artifact
                .function(m::FunctionId(function_index as u32))
                .unwrap();
            let object = object::File::parse(&*artifact.bytes).unwrap();
            assert!(object.symbols().any(|s| {
                s.is_definition()
                    && s.name().is_ok_and(|name| {
                        name.trim_start_matches('_') == function.metadata().symbol
                    })
            }));
            for (index, node) in flow.functions()[function_index].nodes().iter().enumerate() {
                let Some(site) = function.core_site(index as u32) else {
                    continue;
                };
                let m::ownership::failure::Kind::Execute { site: origin, .. } = node.kind else {
                    panic!("core execute")
                };
                let instruction = &typed.functions()[function_index].blocks
                    [origin.block.0 as usize]
                    .instructions[origin.order - 1];
                let m::ownership::bodies::Op::CoreCall(call) = &instruction.op else {
                    panic!("core call")
                };
                assert_eq!(site.operation, call.operation);
                assert_eq!(site.span, instruction.span);
                assert_eq!(site.loan, call.result_loan);
                assert_eq!(
                    site.arguments,
                    call.arguments
                        .iter()
                        .map(|arg| function.layout().frame().value(*arg).unwrap())
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    site.result,
                    function
                        .layout()
                        .frame()
                        .value(instruction.results[0])
                        .unwrap()
                );
                assert_eq!(
                    site.type_arguments,
                    call.type_arguments
                        .iter()
                        .map(|ty| ty.ty)
                        .collect::<Vec<_>>()
                );
                sites += 1;
            }
        }
        assert!(sites >= 13);
        let usage = artifact.usage();
        for field in 0..3 {
            let mut exact = m::Limits::default();
            let mut short = exact;
            match field {
                0 => {
                    exact.work = usage.work;
                    short.work = usage.work - 1;
                }
                1 => {
                    exact.allocations = usage.allocation_bytes;
                    short.allocations = usage.allocation_bytes - 1;
                }
                _ => {
                    exact.elements = usage.elements;
                    short.elements = usage.elements - 1;
                }
            }
            assert_eq!(
                n::emit_object_for_target(flow, &layouts, &target, exact, 1000)
                    .unwrap()
                    .usage(),
                usage
            );
            assert!(n::emit_object_for_target(flow, &layouts, &target, short, 1000).is_err());
        }
    }
}
