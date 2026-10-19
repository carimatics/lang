use super::*;
use lang_mir::unit::ownership as o;
use o::bodies as b;

#[path = "native_value_edges.rs"]
mod edges;

pub(super) const SUMS: &str = include_str!("../../examples/analysis/native_owned_sums.lang");

#[test]
fn native_scalar_calls_copy_enum_guards_and_signed_arithmetic_match_vm() {
    let text = "enum Choice { A(i64), B }\n\
        fn relay(value: core::Bytes, unused: core::Bytes) -> core::Bytes {\n\
            if gate(40 + 2) { return leaf(core::Option::Some(value), unused); }\n\
            return unused;\n\
        }\n\
        fn gate(x: i64) -> bool {\n\
            let mut choice: Choice = Choice::A(x);\n\
            match choice { Choice::A(42) => { return true; }, Choice::A(_) => { return false; }, Choice::B => { return false; }, }\n\
        }\n\
        fn leaf(value: core::Option<core::Bytes>, unused: core::Bytes) -> core::Bytes { return core::option_unwrap::<core::Bytes>(value); }";
    let inputs: &[&[u8]] = &[b"value", b"unused"];
    let overflow = "fn relay(value: core::Bytes, unused: core::Bytes) -> core::Bytes { if (9223372036854775807 + 1) < 0 { return value; } return unused; }";
    for (text, selected) in [
        (text.to_owned(), 0),
        (text.replace("40 + 2", "40 + 1"), 1),
        (overflow.to_owned(), 0),
    ] {
        let normal = execute_unit(&text, inputs, None, 4, 1000, None, false);
        assert_eq!(normal.value.as_deref(), Some(inputs[selected]));
        let mut order = normal.retired.clone();
        order.push(selected);
        assert_eq!(order, vm_record_order(&text, 2));
        for fuel in 0..normal.fuel {
            let failed = execute_unit(&text, inputs, Some(fuel), 4, 1000, None, false);
            let mut retired = failed.retired;
            retired.sort_unstable();
            assert_eq!(retired, [0, 1]);
            assert!(failed.value.is_none());
            assert_eq!(
                failed.failure.unwrap().reason,
                n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::FuelLimit))
            );
        }
    }
}

fn value(table: &mut b::BodyTable, ty: o::Type) -> m::Register {
    let values = &mut table.places.functions[0].values;
    let result = m::Register(values.len() as u32);
    values.push(o::TypeUse {
        ty,
        span: table.places.declarations.functions[0].span,
    });
    result
}
fn instruction(table: &b::BodyTable, result: Option<m::Register>, op: b::Op) -> b::Instruction {
    b::Instruction {
        results: result.into_iter().collect(),
        op,
        span: table.places.declarations.functions[0].span,
    }
}
fn block(
    table: &b::BodyTable,
    parameters: Vec<m::Register>,
    instructions: Vec<b::Instruction>,
    term: b::Term,
) -> b::Block {
    let span = table.places.declarations.functions[0].span;
    b::Block {
        parameters,
        instructions,
        terminator: b::Terminator { kind: term, span },
        span,
    }
}
fn target(block: u32, arguments: impl IntoIterator<Item = m::Register>) -> m::Target {
    m::Target {
        block: m::BlockId(block),
        arguments: arguments.into_iter().collect(),
    }
}
pub(super) fn flow(table: b::BodyTable) -> o::failure::Elaborated {
    let typed = table.verify(Default::default()).unwrap();
    let moved = o::moves::analyze(typed, Default::default()).unwrap();
    let active = o::loan_activity::analyze(moved, Default::default()).unwrap();
    let loans = o::loans::analyze(active, Default::default()).unwrap();
    o::failure::elaborate(loans, Default::default()).unwrap()
}
fn copy_flow(mapping: [usize; 3], rounds: i64, selected: usize) -> o::failure::Elaborated {
    let program = source(
        "fn main() -> i64 { let mut unused: core::Option<core::Bytes> = core::Option::None; return 0; }",
    );
    let mut table = program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    let input = [
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
    ];
    let header = [
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
    ];
    let exit = [
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
        value(&mut table, o::Type::I64),
    ];
    let zero = value(&mut table, o::Type::I64);
    let count = value(&mut table, o::Type::I64);
    let one = value(&mut table, o::Type::I64);
    let next = value(&mut table, o::Type::I64);
    let limit = value(&mut table, o::Type::I64);
    let again = value(&mut table, o::Type::Bool);
    let mut initial: Vec<_> = input
        .into_iter()
        .zip([17, 29, 41])
        .map(|(r, n)| instruction(&table, Some(r), b::Op::I64(n)))
        .collect();
    initial.push(instruction(&table, Some(zero), b::Op::I64(0)));
    initial.push(instruction(&table, Some(limit), b::Op::I64(rounds + 1)));
    table.functions[0].blocks = vec![
        block(
            &table,
            vec![],
            initial,
            b::Term::Jump(target(1, input.into_iter().chain([zero]))),
        ),
        block(
            &table,
            header.into_iter().chain([count]).collect(),
            vec![
                instruction(&table, Some(one), b::Op::I64(1)),
                instruction(&table, Some(next), b::Op::Add(count, one)),
                instruction(&table, Some(again), b::Op::LessThan(next, limit)),
            ],
            b::Term::Branch {
                condition: again,
                then_target: target(1, mapping.map(|i| header[i]).into_iter().chain([next])),
                else_target: target(2, header),
            },
        ),
        block(
            &table,
            exit.to_vec(),
            vec![],
            b::Term::Return(exit[selected]),
        ),
    ];
    flow(table)
}

#[test]
fn native_parallel_copy_edges_match_vm_and_snapshot_oracle() {
    for a in 0..3 {
        for b in 0..3 {
            for c in 0..3 {
                for selected in 0..3 {
                    let mapping = [a, b, c];
                    let mut expected = [17, 29, 41];
                    for _ in 0..3 {
                        let old = expected;
                        expected = mapping.map(|i| old[i]);
                    }
                    let flow = copy_flow(mapping, 3, selected);
                    let native = execute_flow(&flow, &[], None, 1, 1000, None, false);
                    assert!(native.failure.is_none(), "{:?}", native.failure);
                    assert_eq!(
                        native.scalar,
                        Some(expected[selected]),
                        "mapping {mapping:?}, selected {selected}"
                    );
                    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
                    let mut instance = c::Instance::default();
                    let vm = lang_vm::unit::owned::execute_entry(
                        &flow,
                        &mut instance,
                        Some(&mut allocator),
                        Default::default(),
                    )
                    .unwrap();
                    assert_eq!(vm.value, expected[selected]);
                    assert_eq!(allocator.stats().live_allocations, 0);
                }
            }
        }
    }
}

#[test]
fn native_result_extraction_and_failed_option_unwrap_keep_owners_and_panic_code() {
    let result = "fn relay(value: core::Bytes, unused: core::Bytes) -> core::Bytes { return leaf(core::Result::Ok(value), unused); }\n\
        fn leaf(value: core::Result<core::Bytes, core::AllocError>, unused: core::Bytes) -> core::Bytes { return core::result_unwrap::<core::Bytes, core::AllocError>(value); }";
    let inputs: &[&[u8]] = &[b"kept", b"unused"];
    let normal = execute_unit(result, inputs, None, 4, 1000, None, false);
    assert_eq!(normal.value.as_deref(), Some(inputs[0]));
    assert_eq!(vm_record_order(result, 2), [1, 0]);
    let error_payload = result
        .replace("core::Result::Ok(value)", "core::Result::Err(value)")
        .replace("core::AllocError", "core::Bytes")
        .replace("core::result_unwrap::<", "core::result_unwrap_err::<");
    let normal = execute_unit(&error_payload, inputs, None, 4, 1000, None, false);
    assert_eq!(normal.value.as_deref(), Some(inputs[0]));
    assert_eq!(vm_record_order(&error_payload, 2), [1, 0]);
    for (text, code) in [
        (
            SUMS.replace("core::Option::Some(value)", "core::Option::None"),
            -1,
        ),
        (
            result.replace(
                "core::Result::Ok(value)",
                "core::Result::Err(core::AllocError::Limit)",
            ),
            -2,
        ),
        (
            error_payload.replace("core::Result::Err(value)", "core::Result::Ok(value)"),
            -3,
        ),
    ] {
        let failed = execute_unit(&text, inputs, None, 4, 1000, None, false);
        assert!(failed.value.is_none());
        let failure = failed.failure.unwrap();
        assert_eq!(
            failure.reason,
            n::Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::Panic))
        );
        assert_eq!(failure.panic_code, Some(code));
        assert_eq!(failure.function, m::FunctionId(1));
        assert!(text[failure.span.start as usize..failure.span.end as usize].starts_with("core::"));
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(retired, [0, 1]);
    }
}

#[test]
fn source_owned_option_constructs_and_extracts_in_native_code() {
    let inputs: &[&[u8]] = &[b"kept", b"unused"];
    let normal = execute_unit(SUMS, inputs, None, 4, 1000, None, false);
    assert_eq!(normal.value.as_deref(), Some(inputs[0]));
    assert_eq!(normal.retired, [1]);
    assert_eq!(vm_record_order(SUMS, 2), [1, 0]);
    for fuel in 0..normal.fuel {
        let failed = execute_unit(SUMS, inputs, Some(fuel), 4, 1000, None, false);
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(retired, [0, 1]);
        assert!(failed.value.is_none());
    }
}

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
pub(super) fn is_copy(unit: &Unit<'_, '_, '_, '_>, cell: usize) -> bool {
    let layout = unit
        .program
        .layout(unit.stack.last().unwrap().function)
        .unwrap();
    let ty = layout.frame().cell_type(cell).unwrap().ty;
    layout
        .source()
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types()
        .properties(ty)
        .unwrap()
        .is_copy
}
fn physical(unit: &Unit<'_, '_, '_, '_>, cell: usize) -> Result<Physical, c::Cause> {
    unit.ledger
        .as_ref()
        .unwrap()
        .value(unit.slot(cell))
        .copied()
        .map_err(invariant)?
        .physical()
}
pub(super) fn publish(
    unit: &mut Unit<'_, '_, '_, '_>,
    cell: usize,
    value: Physical,
    span: m::Span,
) -> n::ResultStep {
    let copied = is_copy(unit, cell);
    let slot = unit.slot(cell);
    let ledger = unit.ledger.as_mut().unwrap();
    if ledger.value(slot).is_ok() {
        let ty = unit
            .program
            .layout(unit.stack.last().unwrap().function)
            .unwrap()
            .frame()
            .cell_type(cell)
            .unwrap()
            .ty;
        let dropping = unit
            .program
            .layout(unit.stack.last().unwrap().function)
            .unwrap()
            .layouts()
            .types()
            .properties(ty)
            .unwrap()
            .requires_drop;
        if !copied && dropping {
            return Err(bad());
        }
        ledger.take_owner(slot).map_err(invariant)?;
    }
    ledger
        .initialize(slot, Ticket::Value(value), Some(span))
        .map_err(|_| bad())
}
fn immediate(unit: &Unit<'_, '_, '_, '_>, cell: usize) -> Result<l::Immediate, c::Cause> {
    let value = physical(unit, cell)?;
    let store = unit.store.borrow();
    let source = store.pools[value.frame]
        .cell(value.cell)
        .map_err(service_invariant)?;
    source
        .read_immediate(source.root(), store.allocator)
        .map_err(service_invariant)
}
pub(super) fn read_scalar(unit: &mut Unit<'_, '_, '_, '_>, cell: usize) -> Result<i64, c::Cause> {
    match immediate(unit, cell)? {
        l::Immediate::I64(value) => Ok(value),
        l::Immediate::Bool(value) => Ok(i64::from(value)),
        l::Immediate::Unit => Ok(0),
        _ => Err(bad()),
    }
}
fn write(
    unit: &mut Unit<'_, '_, '_, '_>,
    cell: usize,
    value: l::Immediate,
    span: m::Span,
) -> n::ResultStep {
    let target = Physical {
        frame: unit.depth(),
        cell,
    };
    {
        let mut store = unit.store.borrow_mut();
        let Store {
            pools, allocator, ..
        } = &mut *store;
        let output = pools[target.frame]
            .cell(target.cell)
            .map_err(service_invariant)?;
        output
            .write_immediate(output.root(), value, allocator)
            .map_err(service_invariant)?;
    }
    publish(unit, cell, target, span)
}
pub(super) fn write_scalar(
    unit: &mut Unit<'_, '_, '_, '_>,
    cell: usize,
    bits: i64,
    span: m::Span,
) -> n::ResultStep {
    let ty = unit.plan().cell_type(cell).unwrap().ty;
    let value = match ty {
        o::Type::I64 => l::Immediate::I64(bits),
        o::Type::Bool if bits == 0 || bits == 1 => l::Immediate::Bool(bits == 1),
        o::Type::Constructed(_) if bits == 0 => l::Immediate::Unit,
        _ => return Err(bad()),
    };
    write(unit, cell, value, span)
}
pub(super) fn copy_physical(
    store: &mut Store<'_, '_>,
    from: Physical,
    to: Physical,
) -> n::ResultStep {
    let Store {
        pools, allocator, ..
    } = store;
    let input = pools[from.frame]
        .cell(from.cell)
        .map_err(service_invariant)?;
    let output = pools[to.frame].cell(to.cell).map_err(service_invariant)?;
    output
        .copy_from(output.root(), input, input.root(), allocator)
        .map_err(service_invariant)
}
pub(super) fn copy_slot(
    unit: &mut Unit<'_, '_, '_, '_>,
    from: usize,
    to: usize,
    span: m::Span,
) -> n::ResultStep {
    let input = physical(unit, from)?;
    let target = Physical {
        frame: unit.depth(),
        cell: to,
    };
    copy_physical(&mut unit.store.borrow_mut(), input, target)?;
    publish(unit, to, target, span)
}
pub(super) fn aggregate(
    unit: &mut Unit<'_, '_, '_, '_>,
    record: &n::RecordSite,
    case: Option<u32>,
) -> n::ResultStep {
    let target = Physical {
        frame: unit.depth(),
        cell: record.result,
    };
    let layout = unit
        .program
        .layout(unit.stack.last().unwrap().function)
        .unwrap();
    let types = layout
        .source()
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    if let (Some(case), o::Type::Constructed(id)) =
        (case, layout.frame().cell_type(record.result).unwrap().ty)
        && let o::Constructor::Builtin(builtin) =
            types.table().constructed[id.0 as usize].constructor
        && matches!(
            builtin,
            o::Builtin::AllocError
                | o::Builtin::BoundsError
                | o::Builtin::BufferError
                | o::Builtin::Utf8Error
                | o::Builtin::OwnershipError
                | o::Builtin::ArenaError
        )
    {
        let payload = record
            .fields
            .first()
            .map(|&cell| immediate(unit, cell))
            .transpose()?;
        return write(
            unit,
            record.result,
            l::Immediate::from_case(builtin, case, payload).map_err(service_invariant)?,
            record.span,
        );
    }
    let base = unit.depth() * unit.stride;
    let ledger = unit.ledger.as_mut().unwrap();
    let mut store = unit.store.borrow_mut();
    let Store {
        pools, allocator, ..
    } = &mut *store;
    let output = pools[target.frame]
        .cell(target.cell)
        .map_err(service_invariant)?;
    if let Some(case) = case {
        output
            .prepare_case(output.root(), case, allocator)
            .map_err(service_invariant)?;
    }
    for (field, slot) in record.fields.iter().enumerate() {
        let source = ledger
            .value(c::Slot(base + slot))
            .map_err(invariant)?
            .physical()?;
        if source.frame != target.frame {
            return Err(bad());
        }
        let input = pools[source.frame]
            .cell(source.cell)
            .map_err(service_invariant)?;
        let child = output
            .child(output.root(), field as u32, allocator)
            .map_err(service_invariant)?;
        if types.equivalent(input.root().ty(), child.ty()) != Some(true) {
            return Err(bad());
        }
    }
    output
        .initialize_order(
            output.root(),
            record.initialization_order.iter().copied(),
            allocator,
        )
        .map_err(service_invariant)?;
    for (field, slot) in record.fields.iter().enumerate() {
        let source = ledger
            .value(c::Slot(base + slot))
            .map_err(invariant)?
            .physical()?;
        let (input, output) = pools[target.frame]
            .two_cells_mut(source.cell, target.cell)
            .map_err(service_invariant)?;
        let child = output
            .child(output.root(), field as u32, allocator)
            .map_err(service_invariant)?;
        if types.properties(input.root().ty()).unwrap().is_copy {
            output
                .copy_from(child, input, input.root(), allocator)
                .map_err(service_invariant)?;
        } else {
            input
                .move_root_into(output, child, allocator)
                .map_err(service_invariant)?;
            input.recycle(allocator).map_err(service_invariant)?;
            ledger.take_owner(c::Slot(base + slot)).map_err(invariant)?;
        }
    }
    drop(store);
    publish(unit, record.result, target, record.span)
}
pub(super) fn extract(unit: &mut Unit<'_, '_, '_, '_>, site: &n::ExtractSite) -> n::ResultStep {
    let copied = is_copy(unit, site.source);
    let source = physical(unit, site.source)?;
    let target = Physical {
        frame: unit.depth(),
        cell: site.result,
    };
    {
        let mut store = unit.store.borrow_mut();
        let Store {
            pools, allocator, ..
        } = &mut *store;
        let (input, output) = pools[source.frame]
            .two_cells_mut(source.cell, target.cell)
            .map_err(service_invariant)?;
        if copied {
            let child = input
                .child(input.root(), 0, allocator)
                .map_err(service_invariant)?;
            output
                .copy_from(output.root(), input, child, allocator)
                .map_err(service_invariant)?;
        } else {
            input
                .extract_case_into(site.case, output, allocator)
                .map_err(service_invariant)?;
            input.recycle(allocator).map_err(service_invariant)?;
        }
    }
    if !copied {
        let slot = unit.slot(site.source);
        unit.ledger
            .as_mut()
            .unwrap()
            .take_owner(slot)
            .map_err(invariant)?;
    }
    let span = unit.plan().cell_type(site.result).unwrap().span;
    publish(unit, site.result, target, span)
}
pub(super) fn transfer(unit: &mut Unit<'_, '_, '_, '_>, site: &n::TransferSite) -> n::ResultStep {
    for step in &site.moves {
        relocate(unit, step.source, step.scratch)?;
    }
    for step in &site.moves {
        relocate(unit, step.scratch, step.target)?;
    }
    Ok(())
}
pub(super) fn relocate(unit: &mut Unit<'_, '_, '_, '_>, from: usize, to: usize) -> n::ResultStep {
    let span = unit.plan().cell_type(to).unwrap().span;
    move_value(unit, from, to, span)
}
pub(super) fn move_value(
    unit: &mut Unit<'_, '_, '_, '_>,
    from: usize,
    to: usize,
    span: m::Span,
) -> n::ResultStep {
    if is_copy(unit, from) {
        return copy_slot(unit, from, to, span);
    }
    let source_slot = unit.slot(from);
    let target_slot = unit.slot(to);
    let source = physical(unit, from)?;
    let target = Physical {
        frame: unit.depth(),
        cell: to,
    };
    let dropping = {
        let layout = unit
            .program
            .layout(unit.stack.last().unwrap().function)
            .unwrap();
        layout
            .layouts()
            .types()
            .properties(layout.frame().cell_type(to).unwrap().ty)
            .unwrap()
            .requires_drop
    };
    let ledger = unit.ledger.as_mut().unwrap();
    if ledger.value(target_slot).is_ok() {
        if dropping {
            return Err(bad());
        }
        ledger.take_owner(target_slot).map_err(invariant)?;
    }
    unit.store.borrow_mut().transfer(source, target)?;
    *ledger.value_mut(source_slot).map_err(invariant)? = Ticket::Value(target);
    ledger
        .move_value(source_slot, target_slot)
        .map_err(invariant)
}
