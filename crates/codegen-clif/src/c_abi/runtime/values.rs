use super::*;
use m::ownership as o;
fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
pub(super) fn is_copy(unit: &State<'_, '_, '_, '_>, cell: usize) -> bool {
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
fn physical(unit: &State<'_, '_, '_, '_>, cell: usize) -> Result<Physical, c::Cause> {
    unit.ledger
        .as_ref()
        .unwrap()
        .value(unit.slot(cell))
        .copied()
        .map_err(invariant)?
        .physical()
}
pub(super) fn publish(
    unit: &mut State<'_, '_, '_, '_>,
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
fn immediate(unit: &State<'_, '_, '_, '_>, cell: usize) -> Result<l::Immediate, c::Cause> {
    let value = physical(unit, cell)?;
    let store = unit.store.borrow();
    let source = store.pools[value.frame]
        .cell(value.cell)
        .map_err(service_invariant)?;
    source
        .read_immediate(source.root(), store.allocator)
        .map_err(service_invariant)
}
pub(super) fn read_scalar(unit: &mut State<'_, '_, '_, '_>, cell: usize) -> Result<i64, c::Cause> {
    match immediate(unit, cell)? {
        l::Immediate::I64(value) => Ok(value),
        l::Immediate::Bool(value) => Ok(i64::from(value)),
        l::Immediate::Unit => Ok(0),
        _ => Err(bad()),
    }
}
fn write(
    unit: &mut State<'_, '_, '_, '_>,
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
    unit: &mut State<'_, '_, '_, '_>,
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
    unit: &mut State<'_, '_, '_, '_>,
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
    unit: &mut State<'_, '_, '_, '_>,
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
pub(super) fn extract(unit: &mut State<'_, '_, '_, '_>, site: &n::ExtractSite) -> n::ResultStep {
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
pub(super) fn transfer(unit: &mut State<'_, '_, '_, '_>, site: &n::TransferSite) -> n::ResultStep {
    for step in &site.moves {
        relocate(unit, step.source, step.scratch)?;
    }
    for step in &site.moves {
        relocate(unit, step.scratch, step.target)?;
    }
    Ok(())
}
pub(super) fn relocate(unit: &mut State<'_, '_, '_, '_>, from: usize, to: usize) -> n::ResultStep {
    let span = unit.plan().cell_type(to).unwrap().span;
    move_value(unit, from, to, span)
}
pub(super) fn move_value(
    unit: &mut State<'_, '_, '_, '_>,
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
