use super::*;
use m::ownership::{self as o, bodies as b, places::ProjectionKind};
fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
fn fault(error: l::Error) -> c::Cause {
    service_invariant(error)
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
    Vector {
        read: usize,
        location: l::Location,
    },
    Map {
        read: usize,
        location: l::Location,
    },
    Shared {
        read: usize,
        ty: o::Type,
        location: l::Location,
    },
    Arena {
        owner: usize,
        read: usize,
        location: l::Location,
    },
}
fn data(target: Target) -> Result<DataTarget, c::Cause> {
    match target {
        Target::Data(value) => Ok(value),
        Target::Arena { .. } => Err(bad()),
        Target::Allocator | Target::Vector { .. } | Target::Map { .. } | Target::Shared { .. } => {
            Err(bad())
        }
    }
}
#[derive(Default)]
pub(super) struct Loan {
    pub generation: u64,
    pub target: Option<Target>,
    pub vector_read: Option<lang_runtime::vector::Loan>,
    pub map_read: Option<lang_runtime::map::Loan>,
    pub shared_read: Option<lang_runtime::shared::managed::Read>,
    pub custody: Option<lang_runtime::shared::managed::Custody>,
    pub arena_read: Option<(usize, lang_runtime::arena::managed::Read)>,
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
fn valid(unit: &State<'_, '_, '_, '_>, target: DataTarget) -> n::ResultStep {
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
fn root(unit: &State<'_, '_, '_, '_>, slot: usize) -> Result<DataTarget, c::Cause> {
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
    unit: &State<'_, '_, '_, '_>,
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
    unit: &State<'_, '_, '_, '_>,
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
        Target::Shared { read, ty, location } => {
            let store = unit.store.borrow();
            let location = shared::view(&store, read, ty)?
                .child(location, field, store.allocator)
                .map_err(fault)?;
            Ok(Target::Shared { read, ty, location })
        }
        Target::Arena {
            owner,
            read,
            location,
        } => {
            let store = unit.store.borrow();
            let location = arenas::view(&store, owner, read)?
                .child(location, field, store.allocator)
                .map_err(fault)?;
            Ok(Target::Arena {
                owner,
                read,
                location,
            })
        }
        Target::Allocator => Err(bad()),
    }
}
fn dereference(unit: &State<'_, '_, '_, '_>, target: DataTarget) -> Result<Target, c::Cause> {
    valid(unit, target)?;
    let store = unit.store.borrow();
    let token = store.pools[target.physical.frame]
        .cell(target.physical.cell)
        .map_err(fault)?
        .read_token(target.location, store.allocator)
        .map_err(fault)?
        .bytes;
    let id = usize::try_from(u64::from_le_bytes(token[..8].try_into().unwrap()))
        .map_err(service_invariant)?;
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
        Target::Shared { read, ty, location } => {
            shared::view(&store, read, ty)?;
            if types.equivalent(referent, location.ty()) != Some(true) {
                return Err(bad());
            }
        }
        Target::Arena {
            owner,
            read,
            location,
        } => {
            arenas::view(&store, owner, read)?;
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
    unit: &State<'_, '_, '_, '_>,
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
pub(super) fn referent(unit: &State<'_, '_, '_, '_>, slot: usize) -> Result<Target, c::Cause> {
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
    unit: &mut State<'_, '_, '_, '_>,
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
            Target::Shared { read, ty, location } => {
                let Store {
                    pools,
                    allocator,
                    loans,
                    heap,
                    ..
                } = &mut *store;
                let read = loans[read].shared_read.ok_or_else(bad)?;
                let destination = pools[output.frame].cell(output.cell).map_err(fault)?;
                heap.as_ref()
                    .ok_or_else(bad)?
                    .view(read, ty, allocator)
                    .map_err(service_invariant)?
                    .copy_to(location, destination, destination.root(), allocator)
                    .map_err(fault)?;
            }
            Target::Arena {
                owner,
                read,
                location,
            } => {
                let Store {
                    pools,
                    allocator,
                    loans,
                    arena,
                    ..
                } = &mut *store;
                let (expected, read) = loans[read].arena_read.as_ref().ok_or_else(bad)?;
                if *expected != owner {
                    return Err(bad());
                }
                let destination = pools[output.frame].cell(output.cell).map_err(fault)?;
                arena[owner]
                    .as_ref()
                    .ok_or_else(bad)?
                    .payload(read, allocator)
                    .map_err(service_invariant)?
                    .copy_into(location, destination, destination.root(), allocator)
                    .map_err(fault)?;
            }
            Target::Allocator => return Err(bad()),
        }
    }
    values::publish(unit, result, output, span)
}
pub(super) fn load(unit: &mut State<'_, '_, '_, '_>, site: &n::PlaceLoad) -> n::ResultStep {
    copy_target(
        unit,
        resolve_reference(unit, &site.place)?,
        site.result,
        site.place.span,
    )
}
pub(super) fn variant_case(unit: &mut State<'_, '_, '_, '_>, slot: usize) -> Result<u32, c::Cause> {
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
        Target::Shared { read, ty, location } => shared::view(&store, read, ty)?
            .case(location, store.allocator)
            .map_err(fault),
        Target::Arena {
            owner,
            read,
            location,
        } => arenas::view(&store, owner, read)?
            .case(location, store.allocator)
            .map_err(fault),
        Target::Allocator => Err(bad()),
    }
}
pub(super) fn copy_field(
    unit: &mut State<'_, '_, '_, '_>,
    site: &n::CaseFieldSite,
) -> n::ResultStep {
    let target = child_target(unit, referent(unit, site.source)?, site.field)?;
    let span = unit.plan().cell_type(site.result).unwrap().span;
    copy_target(unit, target, site.result, span)
}
pub(super) fn available(unit: &State<'_, '_, '_, '_>, loan: b::LoanId) -> Result<usize, c::Cause> {
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
    unit: &mut State<'_, '_, '_, '_>,
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
    unit: &mut State<'_, '_, '_, '_>,
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
pub(super) fn borrow(unit: &mut State<'_, '_, '_, '_>, site: &n::BorrowSite) -> n::ResultStep {
    register(
        unit,
        site.loan,
        resolve_reference(unit, &site.place)?,
        site.result,
        site.span,
    )
}
pub(super) fn borrow_case(
    unit: &mut State<'_, '_, '_, '_>,
    site: &n::BorrowCaseSite,
) -> n::ResultStep {
    let target = child_target(unit, referent(unit, site.field.source)?, site.field.field)?;
    register(unit, site.loan, target, site.field.result, site.span)
}
pub(super) fn end(
    unit: &mut State<'_, '_, '_, '_>,
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
fn capture(unit: &State<'_, '_, '_, '_>, place: &n::PlaceSite) -> Result<Capture, c::Cause> {
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
pub(super) fn prepare(unit: &mut State<'_, '_, '_, '_>, site: &n::PreparedSite) -> n::ResultStep {
    let capture = capture(unit, &site.place)?;
    let mut store = unit.store.borrow_mut();
    let index = unit.depth() * store.prepared_stride + site.destination.0 as usize;
    *store.prepared.get_mut(index).ok_or_else(bad)? = Some(capture);
    Ok(())
}
pub(super) fn discard(unit: &mut State<'_, '_, '_, '_>, id: Option<b::PreparedPlaceId>) {
    let mut store = unit.store.borrow_mut();
    let base = unit.depth() * store.prepared_stride;
    if let Some(id) = id {
        store.prepared[base + id.0 as usize] = None;
    } else {
        let count = store.prepared_stride;
        store.prepared[base..base + count].fill(None);
    }
}
fn owner_slot(unit: &State<'_, '_, '_, '_>, target: DataTarget) -> Result<c::Slot, c::Cause> {
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
pub(super) fn replace(unit: &mut State<'_, '_, '_, '_>, site: &n::ReplaceSite) -> n::ResultStep {
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
        Capture::Path(
            Target::Allocator | Target::Map { .. } | Target::Shared { .. } | Target::Arena { .. },
        ) => {
            return Err(bad());
        }
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

pub(super) fn observation(
    unit: &State<'_, '_, '_, '_>,
    target: Target,
) -> Result<l::Token, c::Cause> {
    let store = unit.store.borrow();
    match target {
        Target::Data(view) => store.pools[view.physical.frame]
            .cell(view.physical.cell)
            .map_err(fault)?
            .read_token(view.location, store.allocator)
            .map_err(fault),
        Target::Vector { read, location } => vectors::view(&store, read)?
            .read_token(location, store.allocator)
            .map_err(vectors::fault),
        Target::Map { read, location } => maps::view(&store, read)?
            .read_token(location, store.allocator)
            .map_err(maps::fault),
        Target::Shared { read, ty, location } => shared::view(&store, read, ty)?
            .read_token(location, store.allocator)
            .map_err(fault),
        Target::Arena {
            owner,
            read,
            location,
        } => arenas::view(&store, owner, read)?
            .read_token(location, store.allocator)
            .map_err(fault),
        Target::Allocator => Err(bad()),
    }
}
pub(super) fn buffer_token(
    unit: &State<'_, '_, '_, '_>,
    slot: usize,
) -> Result<(Target, [u8; 32]), c::Cause> {
    let target = referent(unit, slot)?;
    Ok((target, observation(unit, target)?.bytes))
}

pub(super) fn scalar(unit: &State<'_, '_, '_, '_>, slot: usize) -> Result<i64, c::Cause> {
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
        Target::Shared { read, ty, location } => shared::view(&store, read, ty)?
            .read_immediate(location, store.allocator)
            .map_err(fault)?,
        Target::Arena {
            owner,
            read,
            location,
        } => arenas::view(&store, owner, read)?
            .read_immediate(location, store.allocator)
            .map_err(fault)?,
        Target::Allocator => return Err(bad()),
    };
    if let l::Immediate::I64(value) = value {
        Ok(value)
    } else {
        Err(bad())
    }
}

pub(super) fn arena_owner(unit: &State<'_, '_, '_, '_>, slot: usize) -> Result<usize, c::Cause> {
    let target = data(referent(unit, slot)?)?;
    valid(unit, target)?;
    let store = unit.store.borrow();
    if target.location.offset() != 0
        || !matches!(target.location.ty(), o::Type::Constructed(id)
        if matches!(store.types.table().constructed[id.0 as usize].constructor, o::Constructor::ArenaOwner(_)))
    {
        return Err(bad());
    }
    let id = target.physical.frame * unit.stride + target.physical.cell;
    if !store.arena_live.get(id).copied().ok_or_else(bad)? {
        return Err(bad());
    }
    Ok(id)
}
pub(super) fn store_token(
    unit: &mut State<'_, '_, '_, '_>,
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
