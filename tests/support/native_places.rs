use super::*;
use m::ownership::{self as o, bodies as b, places::ProjectionKind};

pub(super) const LOANS: &str = include_str!("../../examples/analysis/native_owned_loans.lang");
#[test]
fn native_borrowed_field_mutation_crosses_guest_calls() {
    let inputs: &[&[u8]] = &[b"owner", b"success", b"failure"];
    let result = execute_unit(LOANS, inputs, None, 4, 1000, None, false);
    assert_eq!(
        result.value.as_deref(),
        Some(inputs[1]),
        "{:?}",
        result.failure
    );
    assert_eq!(result.retired, [0, 2]);
    assert_eq!(vm_record_order(LOANS, 3), [0, 2, 1]);
    for fuel in 0..result.fuel {
        let failed = execute_unit(LOANS, inputs, Some(fuel), 4, 1000, None, false);
        assert!(failed.value.is_none());
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(retired, [0, 1, 2]);
        assert!(failed.report.secondary().is_empty());
    }
    for request in 1..=result.requests {
        let failed = execute_unit(LOANS, inputs, None, 4, 1000, Some(request), false);
        assert!(failed.value.is_none());
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(retired, [0, 1, 2]);
    }
}

#[test]
fn native_owned_root_and_field_replacement_retire_old_then_new_owners() {
    let field = "struct Holder { value: core::Bytes }\n\
        fn relay(old: core::Bytes, incoming: core::Bytes, kept: core::Bytes) -> core::Bytes {\n\
            let mut holder: Holder = Holder { value: old };\n\
            let mut done: i64 = replace(holder.value, incoming); return kept;\n\
        }\n\
        fn replace(slot: &mut core::Bytes, value: core::Bytes) -> i64 { (*slot) = value; return 0; }";
    let root = "fn relay(old: core::Bytes, incoming: core::Bytes, kept: core::Bytes) -> core::Bytes { let mut value: core::Bytes = old; value = incoming; return kept; }";
    let inputs: &[&[u8]] = &[b"old", b"incoming", b"kept"];
    for text in [root, field] {
        let normal = execute_unit(text, inputs, None, 4, 1000, None, false);
        assert_eq!(
            normal.value.as_deref(),
            Some(inputs[2]),
            "{:?}",
            normal.failure
        );
        assert_eq!(normal.retired, [0, 1]);
        assert_eq!(vm_record_order(text, 3), [0, 1, 2]);
        let failed = execute_unit(text, inputs, None, 4, 1000, None, true);
        assert!(failed.value.is_none());
        assert_eq!(failed.retired, [0, 1, 2]);
        for fuel in 0..normal.fuel {
            let failed = execute_unit(text, inputs, Some(fuel), 4, 1000, None, false);
            let mut retired = failed.retired;
            retired.sort_unstable();
            assert_eq!(retired, [0, 1, 2]);
        }
    }
}

#[test]
fn native_borrowed_sum_field_projection_matches_vm() {
    let text = "struct Holder { count: i64, value: core::Bytes }\n\
        fn relay(value: core::Bytes, kept: core::Bytes, unused: core::Bytes) -> core::Bytes {\n\
            let mut maybe: core::Option<Holder> = core::Option::Some(Holder { value: value, count: 3 });\n\
            match maybe { core::Option::Some(holder) => { if holder.count < 3 { return unused; } }, core::Option::None => { return unused; }, }\n\
            return kept;\n\
        }";
    let inputs: &[&[u8]] = &[b"value", b"kept", b"unused"];
    let normal = execute_unit(text, inputs, None, 4, 1000, None, false);
    assert_eq!(
        normal.value.as_deref(),
        Some(inputs[1]),
        "{:?}",
        normal.failure
    );
    let mut retired = normal.retired.clone();
    retired.push(1);
    assert_eq!(retired, vm_record_order(text, 3));
    for fuel in 0..normal.fuel {
        let failed = execute_unit(text, inputs, Some(fuel), 4, 1000, None, false);
        let mut retired = failed.retired;
        retired.sort_unstable();
        assert_eq!(retired, [0, 1, 2]);
        assert!(failed.value.is_none());
    }
}

#[test]
fn native_prepared_destination_is_published_after_rhs_guest_effects() {
    let text = "fn main() -> i64 { let mut unused: core::Option<core::Bytes> = core::Option::None; let mut x: i64 = 1; let mut y: i64 = 2; if true { let mut pointer: &mut i64 = &mut x; (*pointer) = change(pointer); } return x + y; } fn change(pointer: &mut i64) -> i64 { (*pointer) = 5; return 7; }";
    let source = source(text);
    compare_scalar(source.failure_flow(), 9);
}
#[test]
fn native_prepared_mir_path_does_not_reread_a_retargeted_reference() {
    // Source disallows exporting a callee-local loan into a caller reference.
    // Reorder two same-frame assignments in independently reverified MIR.
    let source = source(
        "fn main() -> i64 { let mut unused: core::Option<core::Bytes> = core::Option::None; let mut x: i64 = 1; let mut y: i64 = 2; if true { let mut pointer: &mut i64 = &mut x; (*pointer) = 7; pointer = &mut y; } return x + y; }",
    );
    let mut table = source.mir().activity().moved().typed().clone().into_table();
    let mut changed = false;
    for block in &mut table.functions[0].blocks {
        let prepares: Vec<_> = block
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(i, op)| {
                if let b::Op::PreparePlace { destination, .. } = op.op {
                    Some((i, destination))
                } else {
                    None
                }
            })
            .collect();
        if prepares.len() != 2 {
            continue;
        }
        let (first, id) = prepares[0];
        let end = block
            .instructions
            .iter()
            .position(
                |i| matches!(i.op, b::Op::ReplacePrepared { destination, .. } if destination == id),
            )
            .unwrap();
        let held: Vec<_> = block.instructions.drain(first + 1..=end).collect();
        let second = block.instructions.iter().position(|i| matches!(i.op, b::Op::ReplacePrepared { destination, .. } if destination == prepares[1].1)).unwrap();
        block.instructions.splice(second + 1..second + 1, held);
        changed = true;
    }
    assert!(changed);
    let flow = values::flow(table);
    compare_scalar(&flow, 9);
}
fn compare_scalar(flow: &o::failure::Elaborated, expected: i64) {
    let normal = execute_flow(flow, &[], None, 4, 1000, None, false);
    assert_eq!(normal.scalar, Some(expected), "{:?}", normal.failure);
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let vm = lang_vm::unit::owned::execute_entry(
        flow,
        &mut instance,
        Some(&mut allocator),
        Default::default(),
    )
    .unwrap();
    assert_eq!(vm.value, expected);
    assert_eq!(allocator.stats().live_allocations, 0);
}

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
fn fault(_: l::Error) -> c::Cause {
    bad()
}

#[derive(Clone, Copy)]
pub(super) struct DataTarget {
    physical: Physical,
    location: l::Location,
    frame: u64,
}
#[derive(Clone, Copy)]
pub(super) enum Target {
    Data(DataTarget),
    Allocator,
    Vector { read: usize, location: l::Location },
    Map { read: usize, location: l::Location },
}
fn data(target: Target) -> Result<DataTarget, c::Cause> {
    match target {
        Target::Data(value) => Ok(value),
        Target::Allocator | Target::Vector { .. } | Target::Map { .. } => Err(bad()),
    }
}
#[derive(Default)]
pub(super) struct Loan {
    pub generation: u64,
    pub target: Option<Target>,
    pub vector_read: Option<lang_runtime::vector::Loan>,
    pub map_read: Option<lang_runtime::map::Loan>,
}
#[derive(Clone, Copy)]
pub(super) enum Capture {
    Root {
        frame: usize,
        handle: u64,
        slot: usize,
    },
    Path(Target),
}
fn valid(unit: &Unit<'_, '_, '_, '_>, target: DataTarget) -> n::ResultStep {
    if unit
        .stack
        .get(target.physical.frame)
        .is_some_and(|frame| frame.handle == target.frame)
    {
        Ok(())
    } else {
        Err(bad())
    }
}
fn root(unit: &Unit<'_, '_, '_, '_>, slot: usize) -> Result<DataTarget, c::Cause> {
    let physical = unit
        .ledger
        .as_ref()
        .unwrap()
        .value(unit.slot(slot))
        .map_err(invariant)?
        .physical()?;
    let store = unit.store.borrow();
    let location = store.pools[physical.frame]
        .cell(physical.cell)
        .map_err(fault)?
        .root();
    Ok(DataTarget {
        physical,
        location,
        frame: unit.stack[physical.frame].handle,
    })
}
fn child(
    unit: &Unit<'_, '_, '_, '_>,
    target: DataTarget,
    field: u32,
) -> Result<DataTarget, c::Cause> {
    valid(unit, target)?;
    let store = unit.store.borrow();
    let location = store.pools[target.physical.frame]
        .cell(target.physical.cell)
        .map_err(fault)?
        .child(target.location, field, store.allocator)
        .map_err(fault)?;
    Ok(DataTarget { location, ..target })
}
fn child_target(
    unit: &Unit<'_, '_, '_, '_>,
    target: Target,
    field: u32,
) -> Result<Target, c::Cause> {
    match target {
        Target::Data(value) => Ok(Target::Data(child(unit, value, field)?)),
        Target::Vector { read, location } => {
            let store = unit.store.borrow();
            let location = vectors::view(&store, read)?
                .child(location, field, store.allocator)
                .map_err(vectors::fault)?;
            Ok(Target::Vector { read, location })
        }
        Target::Map { read, location } => {
            let store = unit.store.borrow();
            let location = maps::view(&store, read)?
                .child(location, field, store.allocator)
                .map_err(maps::fault)?;
            Ok(Target::Map { read, location })
        }
        Target::Allocator => Err(bad()),
    }
}
fn dereference(unit: &Unit<'_, '_, '_, '_>, target: DataTarget) -> Result<Target, c::Cause> {
    valid(unit, target)?;
    let store = unit.store.borrow();
    let token = store.pools[target.physical.frame]
        .cell(target.physical.cell)
        .map_err(fault)?
        .read_token(target.location, store.allocator)
        .map_err(fault)?
        .bytes;
    let id =
        usize::try_from(u64::from_le_bytes(token[..8].try_into().unwrap())).map_err(|_| bad())?;
    let generation = u64::from_le_bytes(token[8..16].try_into().unwrap());
    if token[16..].iter().any(|&byte| byte != 0) {
        return Err(bad());
    }
    let loan = store.loans.get(id).ok_or_else(bad)?;
    if loan.generation != generation {
        return Err(bad());
    }
    let resolved = loan.target.ok_or_else(bad)?;
    let types = unit
        .program
        .layout(unit.stack.last().unwrap().function)
        .unwrap()
        .layouts()
        .types();
    let o::Type::Constructed(id) = target.location.ty() else {
        return Err(bad());
    };
    let o::Constructor::Loan(_, referent) = types.table().constructed[id.0 as usize].constructor
    else {
        return Err(bad());
    };
    match resolved {
        Target::Data(value) => {
            valid(unit, value)?;
            if types.equivalent(referent, value.location.ty()) != Some(true) {
                return Err(bad());
            }
        }
        Target::Vector { read, location } => {
            vectors::view(&store, read)?;
            if types.equivalent(referent, location.ty()) != Some(true) {
                return Err(bad());
            }
        }
        Target::Map { read, location } => {
            maps::view(&store, read)?;
            if types.equivalent(referent, location.ty()) != Some(true) {
                return Err(bad());
            }
        }
        Target::Allocator => {
            if !matches!(referent, o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor, o::Constructor::Builtin(o::Builtin::Allocator)))
            {
                return Err(bad());
            }
        }
    }
    Ok(resolved)
}

fn resolve_reference(
    unit: &Unit<'_, '_, '_, '_>,
    place: &n::PlaceSite,
) -> Result<Target, c::Cause> {
    let mut target = Target::Data(root(unit, place.root)?);
    for projection in &place.projections {
        target = match projection.kind {
            ProjectionKind::Dereference => dereference(unit, data(target)?)?,
            ProjectionKind::Field(field) => child_target(unit, target, field)?,
        };
    }
    Ok(target)
}
pub(super) fn referent(unit: &Unit<'_, '_, '_, '_>, slot: usize) -> Result<Target, c::Cause> {
    let target = root(unit, slot)?;
    let types = unit
        .program
        .layout(unit.stack.last().unwrap().function)
        .unwrap()
        .layouts()
        .types();
    if matches!(target.location.ty(), o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor, o::Constructor::Loan(..)))
    {
        dereference(unit, target)
    } else {
        Ok(Target::Data(target))
    }
}
fn copy_target(
    unit: &mut Unit<'_, '_, '_, '_>,
    target: Target,
    result: usize,
    span: m::Span,
) -> n::ResultStep {
    let output = Physical {
        frame: unit.depth(),
        cell: result,
    };
    {
        let mut store = unit.store.borrow_mut();
        match target {
            Target::Data(target) => {
                valid(unit, target)?;
                let Store {
                    pools, allocator, ..
                } = &mut *store;
                let input = pools[target.physical.frame]
                    .cell(target.physical.cell)
                    .map_err(fault)?;
                let destination = pools[output.frame].cell(output.cell).map_err(fault)?;
                destination
                    .copy_from(destination.root(), input, target.location, allocator)
                    .map_err(fault)?;
            }
            Target::Vector { read, location } => {
                let Store {
                    pools,
                    allocator,
                    loans,
                    layouts,
                    ..
                } = &mut *store;
                let loan = loans[read].vector_read.as_ref().ok_or_else(bad)?;
                let destination = pools[output.frame].cell(output.cell).map_err(fault)?;
                loan.view(layouts, allocator)
                    .map_err(vectors::fault)?
                    .copy_into(location, destination, destination.root(), allocator)
                    .map_err(vectors::fault)?;
            }
            Target::Map { read, location } => {
                let Store {
                    pools,
                    allocator,
                    loans,
                    layouts,
                    ..
                } = &mut *store;
                let loan = loans[read].map_read.as_ref().ok_or_else(bad)?;
                let destination = pools[output.frame].cell(output.cell).map_err(fault)?;
                loan.view(layouts, allocator)
                    .map_err(maps::fault)?
                    .copy_into(location, destination, destination.root(), allocator)
                    .map_err(maps::fault)?;
            }
            Target::Allocator => return Err(bad()),
        }
    }
    values::publish(unit, result, output, span)
}
pub(super) fn load(unit: &mut Unit<'_, '_, '_, '_>, site: &n::PlaceLoad) -> n::ResultStep {
    copy_target(
        unit,
        resolve_reference(unit, &site.place)?,
        site.result,
        site.place.span,
    )
}
pub(super) fn variant_case(unit: &mut Unit<'_, '_, '_, '_>, slot: usize) -> Result<u32, c::Cause> {
    let target = referent(unit, slot)?;
    let store = unit.store.borrow();
    match target {
        Target::Data(target) => store.pools[target.physical.frame]
            .cell(target.physical.cell)
            .map_err(fault)?
            .case(target.location, store.allocator)
            .map_err(fault),
        Target::Vector { read, location } => vectors::view(&store, read)?
            .case(location, store.allocator)
            .map_err(vectors::fault),
        Target::Map { read, location } => maps::view(&store, read)?
            .case(location, store.allocator)
            .map_err(maps::fault),
        Target::Allocator => Err(bad()),
    }
}
pub(super) fn copy_field(
    unit: &mut Unit<'_, '_, '_, '_>,
    site: &n::CaseFieldSite,
) -> n::ResultStep {
    let target = child_target(unit, referent(unit, site.source)?, site.field)?;
    let span = unit.plan().cell_type(site.result).unwrap().span;
    copy_target(unit, target, site.result, span)
}
pub(super) fn available(unit: &Unit<'_, '_, '_, '_>, loan: b::LoanId) -> Result<usize, c::Cause> {
    let store = unit.store.borrow();
    let id = unit.depth() * store.loan_stride + loan.0 as usize;
    let entry = store.loans.get(id).ok_or_else(bad)?;
    if entry.target.is_some() {
        return Err(bad());
    }
    entry
        .generation
        .checked_add(1)
        .ok_or(c::Cause::Execution(c::ExecutionFailure::CounterOverflow))?;
    Ok(id)
}
pub(super) fn register_token(
    unit: &mut Unit<'_, '_, '_, '_>,
    loan: b::LoanId,
    target: Target,
    span: m::Span,
) -> Result<[u8; 32], c::Cause> {
    if let Target::Data(value) = target {
        valid(unit, value)?;
    }
    let id = available(unit, loan)?;
    let generation = {
        let mut store = unit.store.borrow_mut();
        let entry = &mut store.loans[id];
        entry.generation += 1;
        entry.target = Some(target);
        entry.generation
    };
    let layout = unit
        .program
        .layout(unit.stack.last().unwrap().function)
        .unwrap();
    let slot = c::Slot(unit.depth() * unit.stride + layout.loan_slot(loan).ok_or_else(bad)?);
    unit.ledger
        .as_mut()
        .unwrap()
        .initialize(slot, Ticket::Loan(id), Some(span))
        .map_err(|_| bad())?;
    let mut token = [0; 32];
    token[..8].copy_from_slice(&(id as u64).to_le_bytes());
    token[8..16].copy_from_slice(&generation.to_le_bytes());
    Ok(token)
}
pub(super) fn register(
    unit: &mut Unit<'_, '_, '_, '_>,
    loan: b::LoanId,
    target: Target,
    result: usize,
    span: m::Span,
) -> n::ResultStep {
    let token = register_token(unit, loan, target, span)?;
    let physical = Physical {
        frame: unit.depth(),
        cell: result,
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
            .copy_from_slice(&token);
    }
    values::publish(unit, result, physical, span)
}
pub(super) fn borrow(unit: &mut Unit<'_, '_, '_, '_>, site: &n::BorrowSite) -> n::ResultStep {
    register(
        unit,
        site.loan,
        resolve_reference(unit, &site.place)?,
        site.result,
        site.span,
    )
}
pub(super) fn borrow_case(
    unit: &mut Unit<'_, '_, '_, '_>,
    site: &n::BorrowCaseSite,
) -> n::ResultStep {
    let target = child_target(unit, referent(unit, site.field.source)?, site.field.field)?;
    register(unit, site.loan, target, site.field.result, site.span)
}
pub(super) fn end(
    unit: &mut Unit<'_, '_, '_, '_>,
    loan: b::LoanId,
    mode: b::EndMode,
) -> n::ResultStep {
    let store = unit.store.borrow();
    let id = unit.depth() * store.loan_stride + loan.0 as usize;
    let active = store.loans.get(id).ok_or_else(bad)?.target.is_some();
    drop(store);
    if active {
        let layout = unit
            .program
            .layout(unit.stack.last().unwrap().function)
            .unwrap();
        let slot = c::Slot(unit.depth() * unit.stride + layout.loan_slot(loan).ok_or_else(bad)?);
        unit.ledger
            .as_mut()
            .unwrap()
            .drop_value(slot)
            .map_err(invariant)
    } else if mode == b::EndMode::Required {
        Err(bad())
    } else {
        Ok(())
    }
}
fn capture(unit: &Unit<'_, '_, '_, '_>, place: &n::PlaceSite) -> Result<Capture, c::Cause> {
    if place.projections.is_empty() {
        Ok(Capture::Root {
            frame: unit.depth(),
            handle: unit.frame_identity(),
            slot: place.root,
        })
    } else {
        Ok(Capture::Path(resolve_reference(unit, place)?))
    }
}
pub(super) fn prepare(unit: &mut Unit<'_, '_, '_, '_>, site: &n::PreparedSite) -> n::ResultStep {
    let capture = capture(unit, &site.place)?;
    let mut store = unit.store.borrow_mut();
    let index = unit.depth() * store.prepared_stride + site.destination.0 as usize;
    *store.prepared.get_mut(index).ok_or_else(bad)? = Some(capture);
    Ok(())
}
pub(super) fn discard(unit: &mut Unit<'_, '_, '_, '_>, id: Option<b::PreparedPlaceId>) {
    let mut store = unit.store.borrow_mut();
    let base = unit.depth() * store.prepared_stride;
    if let Some(id) = id {
        store.prepared[base + id.0 as usize] = None;
    } else {
        let count = store.prepared_stride;
        store.prepared[base..base + count].fill(None);
    }
}
fn owner_slot(unit: &Unit<'_, '_, '_, '_>, target: DataTarget) -> Result<c::Slot, c::Cause> {
    valid(unit, target)?;
    let plan = unit
        .program
        .layout(unit.stack[target.physical.frame].function)
        .unwrap()
        .frame();
    let base = target.physical.frame * unit.stride;
    for cell in 0..plan.cell_count() {
        if let Ok(Ticket::Value(value)) = unit.ledger.as_ref().unwrap().value(c::Slot(base + cell))
            && value.frame == target.physical.frame
            && value.cell == target.physical.cell
        {
            return Ok(c::Slot(base + cell));
        }
    }
    Err(bad())
}
pub(super) fn replace(unit: &mut Unit<'_, '_, '_, '_>, site: &n::ReplaceSite) -> n::ResultStep {
    let capture = match &site.destination {
        n::ReplacementDestination::Place(place) => capture(unit, place)?,
        n::ReplacementDestination::Prepared(id) => {
            let mut store = unit.store.borrow_mut();
            let index = unit.depth() * store.prepared_stride + id.0 as usize;
            store
                .prepared
                .get_mut(index)
                .ok_or_else(bad)?
                .take()
                .ok_or_else(bad)?
        }
    };
    let source_slot = unit.slot(site.value);
    let source = unit
        .ledger
        .as_ref()
        .unwrap()
        .value(source_slot)
        .map_err(invariant)?
        .physical()?;
    let (target, target_slot) = match capture {
        Capture::Root {
            frame,
            handle,
            slot,
        } => {
            if unit.stack.get(frame).is_none_or(|f| f.handle != handle) {
                return Err(bad());
            }
            let target_slot = c::Slot(frame * unit.stride + slot);
            if let Ok(ticket) = unit.ledger.as_ref().unwrap().value(target_slot) {
                let physical = ticket.physical()?;
                let store = unit.store.borrow();
                let location = store.pools[physical.frame]
                    .cell(physical.cell)
                    .map_err(fault)?
                    .root();
                (
                    DataTarget {
                        physical,
                        location,
                        frame: handle,
                    },
                    target_slot,
                )
            } else {
                return values::relocate(unit, site.value, slot);
            }
        }
        Capture::Path(Target::Data(target)) => (target, owner_slot(unit, target)?),
        Capture::Path(Target::Vector { read, location }) => {
            return vectors::replace(unit, read, location, site.value, site.span);
        }
        Capture::Path(Target::Allocator | Target::Map { .. }) => return Err(bad()),
    };
    valid(unit, target)?;
    let copied = values::is_copy(unit, site.value);
    let (old_span, at_root) = {
        let mut store = unit.store.borrow_mut();
        let Store {
            pools, allocator, ..
        } = &mut *store;
        let at_root = pools[target.physical.frame]
            .cell(target.physical.cell)
            .map_err(fault)?
            .root()
            == target.location;
        let old = if source.frame == target.physical.frame {
            let (output, input) = pools[source.frame]
                .two_cells_mut(target.physical.cell, source.cell)
                .map_err(fault)?;
            output
                .exchange_root_at(target.location, input, site.span, allocator)
                .map_err(fault)?
        } else if target.physical.frame < source.frame {
            let (left, right) = pools.split_at_mut(source.frame);
            left[target.physical.frame]
                .cell_mut(target.physical.cell)
                .map_err(fault)?
                .exchange_root_at(
                    target.location,
                    right[0].cell_mut(source.cell).map_err(fault)?,
                    site.span,
                    allocator,
                )
                .map_err(fault)?
        } else {
            let (left, right) = pools.split_at_mut(target.physical.frame);
            right[0]
                .cell_mut(target.physical.cell)
                .map_err(fault)?
                .exchange_root_at(
                    target.location,
                    left[source.frame].cell_mut(source.cell).map_err(fault)?,
                    site.span,
                    allocator,
                )
                .map_err(fault)?
        };
        if copied {
            let output = pools[source.frame].cell(source.cell).map_err(fault)?;
            let input = pools[target.physical.frame]
                .cell(target.physical.cell)
                .map_err(fault)?;
            output
                .copy_from(output.root(), input, target.location, allocator)
                .map_err(fault)?;
        }
        (old, at_root)
    };
    if copied {
        return Ok(());
    }
    let ledger = unit.ledger.as_mut().unwrap();
    if at_root {
        *ledger.value_mut(source_slot).map_err(invariant)? = Ticket::Value(target.physical);
        *ledger.value_mut(target_slot).map_err(invariant)? = Ticket::Value(source);
        ledger.replace(source_slot, target_slot).map_err(invariant)
    } else {
        ledger
            .retire_replaced(source_slot, old_span)
            .map_err(invariant)
    }
}

pub(super) fn buffer_token(
    unit: &Unit<'_, '_, '_, '_>,
    slot: usize,
) -> Result<(Target, [u8; 32]), c::Cause> {
    let target = referent(unit, slot)?;
    let store = unit.store.borrow();
    let token = match target {
        Target::Data(view) => {
            store.pools[view.physical.frame]
                .cell(view.physical.cell)
                .map_err(fault)?
                .read_token(view.location, store.allocator)
                .map_err(fault)?
                .bytes
        }
        Target::Vector { read, location } => {
            vectors::view(&store, read)?
                .read_token(location, store.allocator)
                .map_err(vectors::fault)?
                .bytes
        }
        Target::Map { read, location } => {
            maps::view(&store, read)?
                .read_token(location, store.allocator)
                .map_err(maps::fault)?
                .bytes
        }
        Target::Allocator => return Err(bad()),
    };
    Ok((target, token))
}

pub(super) fn scalar(unit: &Unit<'_, '_, '_, '_>, slot: usize) -> Result<i64, c::Cause> {
    let target = referent(unit, slot)?;
    let store = unit.store.borrow();
    let value = match target {
        Target::Data(target) => store.pools[target.physical.frame]
            .cell(target.physical.cell)
            .map_err(fault)?
            .read_immediate(target.location, store.allocator)
            .map_err(fault)?,
        Target::Vector { read, location } => vectors::view(&store, read)?
            .read_immediate(location, store.allocator)
            .map_err(vectors::fault)?,
        Target::Map { read, location } => maps::view(&store, read)?
            .read_immediate(location, store.allocator)
            .map_err(maps::fault)?,
        Target::Allocator => return Err(bad()),
    };
    if let l::Immediate::I64(value) = value {
        Ok(value)
    } else {
        Err(bad())
    }
}
pub(super) fn store_token(
    unit: &mut Unit<'_, '_, '_, '_>,
    target: Target,
    token: [u8; 32],
) -> n::ResultStep {
    if let Target::Vector { read, location } = target {
        let mut store = unit.store.borrow_mut();
        let Store {
            loans,
            layouts,
            allocator,
            ..
        } = &mut *store;
        return loans[read]
            .vector_read
            .as_ref()
            .ok_or_else(bad)?
            .write_buffer_token(layouts, location, token, allocator)
            .map_err(vectors::fault);
    }
    let view = data(target)?;
    valid(unit, view)?;
    let mut store = unit.store.borrow_mut();
    let Store {
        pools, allocator, ..
    } = &mut *store;
    let start = view.location.offset() as usize;
    pools[view.physical.frame]
        .cell(view.physical.cell)
        .map_err(fault)?
        .data_mut(allocator)
        .map_err(fault)?
        .get_mut(start..start + 32)
        .ok_or_else(bad)?
        .copy_from_slice(&token);
    Ok(())
}
