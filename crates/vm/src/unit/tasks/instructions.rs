use super::*;
use frame::Location;

pub(super) use lang_runtime::task::runner::finite_copy_supported as supported;
pub(super) fn execute<P: a::Provider>(
    typed: &b::TypedBodies,
    frame: &mut Frame<'_, '_>,
    site: b::Site,
    allocator: &mut a::Allocator<P>,
) -> Result<(), Reason> {
    let i = &typed.functions()[frame.function.0 as usize].blocks[site.block.0 as usize]
        .instructions[site.order - 1];
    let locals = frame.locals;
    let value = |r: m::Register| locals + r.0 as usize;
    let out = i.results.first().map(|r| value(*r));
    let result = || out.ok_or(Reason::Invariant);
    let paths = &typed.places().functions()[frame.function.0 as usize].places;
    match &i.op {
        b::Op::I64(n) => frame.write(result()?, l::Immediate::I64(*n), allocator)?,
        b::Op::Bool(v) => frame.write(result()?, l::Immediate::Bool(*v), allocator)?,
        b::Op::Unit => frame.write(result()?, l::Immediate::Unit, allocator)?,
        b::Op::Panic(code) => {
            let l::Immediate::I64(code) = frame.read(value(*code), allocator)? else {
                return Err(Reason::Invariant);
            };
            return Err(Reason::Panic(Some(code)));
        }
        op @ (b::Op::Add(a, b) | b::Op::LessThan(a, b) | b::Op::EqualI64(a, b)) => {
            let l::Immediate::I64(a) = frame.read(value(*a), allocator)? else {
                return Err(Reason::Invariant);
            };
            let l::Immediate::I64(b) = frame.read(value(*b), allocator)? else {
                return Err(Reason::Invariant);
            };
            let result_value = match op {
                b::Op::Add(..) => l::Immediate::I64(a.wrapping_add(b)),
                b::Op::LessThan(..) => l::Immediate::Bool(a < b),
                _ => l::Immediate::Bool(a == b),
            };
            frame.write(result()?, result_value, allocator)?;
        }
        b::Op::Copy(from) | b::Op::Move(from) => {
            frame.transfer(value(*from), result()?, allocator)?
        }
        b::Op::Initialize { local, value: from } => {
            frame.transfer(value(*from), local.0 as usize, allocator)?
        }
        b::Op::CopyPlace(place) | b::Op::MovePlace(place) => {
            let from = frame.place(&paths[place.0 as usize], allocator)?;
            if frame.is_claim(from.slot)? {
                frame.transfer(from.slot, result()?, allocator)?;
            } else {
                frame.copy_location(from, frame.root(result()?)?, allocator)?;
            }
        }
        b::Op::PreparePlace { destination, place } => {
            frame.prepared[destination.0 as usize] =
                Some(frame.place(&paths[place.0 as usize], allocator)?)
        }
        b::Op::Replace { place, value: from } => {
            let to = frame.place(&paths[place.0 as usize], allocator)?;
            if frame.is_claim(to.slot)? {
                frame.transfer(value(*from), to.slot, allocator)?;
            } else {
                frame.copy_location(frame.root(value(*from))?, to, allocator)?;
            }
        }
        b::Op::ReplacePrepared {
            destination,
            value: from,
        } => {
            let to = frame.prepared[destination.0 as usize]
                .take()
                .ok_or(Reason::Invariant)?;
            if frame.is_claim(to.slot)? {
                frame.transfer(value(*from), to.slot, allocator)?;
            } else {
                frame.copy_location(frame.root(value(*from))?, to, allocator)?;
            }
        }
        b::Op::DiscardPrepared(destination) => frame.prepared[destination.0 as usize] = None,
        b::Op::DropLocal(local) | b::Op::DropLocalIfInitialized(local) => {
            frame.forget(local.0 as usize)
        }
        b::Op::DropValue(from) | b::Op::DropValueIfAvailable(from) => frame.forget(value(*from)),
        b::Op::Cleanup(cleanup) => {
            for step in &cleanup.steps {
                match step.target {
                    b::CleanupTarget::Local(local) => frame.forget(local.0 as usize),
                    b::CleanupTarget::Value(from) => frame.forget(value(from)),
                    _ => return Err(Reason::Unsupported),
                }
            }
        }
        b::Op::Record { fields, .. } => {
            let to = result()?;
            let root = frame.root(to)?;
            frame.data()?.cell(to)?.initialize_from_mir(
                root.at,
                typed,
                frame.function,
                site,
                allocator,
            )?;
            for (index, from) in fields.iter().enumerate() {
                let at = frame
                    .data()?
                    .cell(to)?
                    .child(root.at, index as u32, allocator)?;
                frame.copy_location(
                    frame.root(value(*from))?,
                    Location { slot: to, at },
                    allocator,
                )?;
            }
            frame.live[to] = true;
        }
        b::Op::Variant { case, payload } => {
            let to = result()?;
            let root = frame.root(to)?;
            let ty = root.at.ty();
            let builtin = match ty {
                o::Type::Constructed(id) => {
                    match typed.places().declarations().types().table().constructed[id.0 as usize]
                        .constructor
                    {
                        o::Constructor::Builtin(b) => Some(b),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(builtin) = builtin {
                let payload = payload
                    .first()
                    .map(|from| frame.read(value(*from), allocator))
                    .transpose()?;
                frame.write(
                    to,
                    l::Immediate::from_case(builtin, *case, payload)?,
                    allocator,
                )?;
            } else {
                frame
                    .data()?
                    .cell(to)?
                    .prepare_case(root.at, *case, allocator)?;
                for (index, from) in payload.iter().enumerate() {
                    let at = frame
                        .data()?
                        .cell(to)?
                        .child(root.at, index as u32, allocator)?;
                    frame.copy_location(
                        frame.root(value(*from))?,
                        Location { slot: to, at },
                        allocator,
                    )?;
                }
                frame.live[to] = true;
            }
        }
        b::Op::IsCase { value: from, case } => {
            frame.available(value(*from))?;
            let actual = frame
                .data()?
                .cell(value(*from))?
                .case(frame.root(value(*from))?.at, allocator)?;
            frame.write(result()?, l::Immediate::Bool(actual == *case), allocator)?;
        }
        b::Op::CopyCaseField {
            value: from,
            case,
            field,
            guard,
        } => {
            if frame.read(value(*guard), allocator)? != l::Immediate::Bool(true) {
                return Err(Reason::Invariant);
            }
            copy_field(frame, value(*from), *case, *field, result()?, allocator)?;
        }
        b::Op::UnpackVariant {
            value: from,
            case,
            guard,
        } => {
            if frame.read(value(*guard), allocator)? != l::Immediate::Bool(true) {
                return Err(Reason::Invariant);
            }
            for (field, to) in i.results.iter().enumerate() {
                copy_field(
                    frame,
                    value(*from),
                    *case,
                    field as u32,
                    value(*to),
                    allocator,
                )?;
            }
        }
        b::Op::CoreCall(call) => {
            let from = value(call.arguments[0]);
            let expected = if call.operation == o::core::Operation::OptionUnwrap {
                1
            } else {
                0
            };
            frame.available(from)?;
            if frame
                .data()?
                .cell(from)?
                .case(frame.root(from)?.at, allocator)?
                != expected
            {
                return Err(Reason::Panic(None));
            }
            copy_field(frame, from, expected, 0, result()?, allocator)?;
        }
        b::Op::EndBorrow { .. } => {}
        _ => return Err(Reason::Unsupported),
    }
    Ok(())
}
fn copy_field<P: a::Provider>(
    frame: &mut Frame<'_, '_>,
    from: usize,
    case: u32,
    field: u32,
    to: usize,
    allocator: &mut a::Allocator<P>,
) -> Result<(), Reason> {
    frame.available(from)?;
    let source = frame.data()?.cell(from)?;
    let root = source.root();
    if source.case(root, allocator)? != case {
        return Err(Reason::Invariant);
    }
    if matches!(
        source
            .layouts()
            .get(root.ty())
            .ok_or(Reason::Invariant)?
            .kind,
        l::Kind::Error { .. }
    ) {
        let payload = source
            .read_immediate(root, allocator)?
            .payload()
            .ok_or(Reason::Invariant)?;
        if field != 0 {
            return Err(Reason::Invariant);
        }
        frame.write(to, payload, allocator)
    } else {
        let at = source.child(root, field, allocator)?;
        frame.copy_location(Location { slot: from, at }, frame.root(to)?, allocator)
    }
}
