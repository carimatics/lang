use super::*;

/// The finite payload service profile, including every inactive enum case.
/// Walk inline dependencies once, without recursive host-stack growth.
pub(super) struct Payloads {
    states: Vec<u8>,
    definitions: usize,
    pub usage: m::Usage,
}
impl Payloads {
    pub fn compile(types: &o::VerifiedTypes, limits: m::Limits) -> Result<Self, l::Error> {
        let definitions = types.table().definitions.len();
        let count = definitions
            .checked_add(types.table().constructed.len())
            .and_then(|n| n.checked_add(2))
            .ok_or(l::Error::Overflow)?;
        let mut budget = m::Budget::new(limits);
        let mut states = budget.vector(count)?;
        states.resize(count, 0u8);
        let leaf = |ty| {
            types.properties(ty).unwrap().is_copy
                || matches!(ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Builtin(o::Builtin::Bytes|o::Builtin::Text)|o::Constructor::ArenaOwner(_)))
        };
        let compound = |ty| {
            matches!(ty, o::Type::Nominal(_))
                || matches!(ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Option(_)|o::Constructor::Result(..)|o::Constructor::Loan(..)|o::Constructor::Shared(_)|o::Constructor::Weak(_)))
        };
        let mut edges = 0usize;
        for (index, state) in states.iter_mut().enumerate() {
            budget.charge(1)?;
            let ty = type_at(definitions, index);
            let depends = !leaf(ty) && compound(ty);
            *state = if leaf(ty) || compound(ty) { 2 } else { 3 };
            for _ in children(types, ty) {
                budget.charge(1)?;
                if depends {
                    edges = edges.checked_add(1).ok_or(l::Error::Overflow)?;
                }
            }
        }
        let mut heads = budget.vector(count)?;
        heads.resize(count, usize::MAX);
        let mut parents = budget.vector(edges)?;
        let mut pending = budget.vector(count)?;
        for (index, state) in states.iter().enumerate() {
            budget.charge(1)?;
            let ty = type_at(definitions, index);
            if *state == 3 {
                pending.push(index);
            }
            if leaf(ty) || !compound(ty) {
                continue;
            }
            for child in children(types, ty) {
                budget.charge(1)?;
                let child = type_index(definitions, child);
                parents.push((index, heads[child]));
                heads[child] = parents.len() - 1;
            }
        }
        // Cycles of supported indirect shapes are valid. Unsupported children
        // reject every dependent shape, including inactive enum payloads.
        while let Some(child) = pending.pop() {
            let mut edge = heads[child];
            while edge != usize::MAX {
                budget.charge(1)?;
                let (parent, next) = parents[edge];
                if states[parent] == 2 {
                    states[parent] = 3;
                    pending.push(parent);
                }
                edge = next;
            }
        }
        Ok(Self {
            states,
            definitions,
            usage: budget_usage(&budget),
        })
    }
    pub fn supports(&self, ty: o::Type) -> bool {
        self.states[type_index(self.definitions, ty)] == 2
    }
}
fn type_index(definitions: usize, ty: o::Type) -> usize {
    match ty {
        o::Type::I64 => 0,
        o::Type::Bool => 1,
        o::Type::Nominal(id) => 2 + id.0 as usize,
        o::Type::Constructed(id) => 2 + definitions + id.0 as usize,
    }
}
fn type_at(definitions: usize, index: usize) -> o::Type {
    match index {
        0 => o::Type::I64,
        1 => o::Type::Bool,
        n if n < 2 + definitions => o::Type::Nominal(m::DefinitionId((n - 2) as u32)),
        n => o::Type::Constructed(o::ConstructedId((n - 2 - definitions) as u32)),
    }
}
fn children(types: &o::VerifiedTypes, ty: o::Type) -> impl Iterator<Item = o::Type> + '_ {
    let definition = match ty {
        o::Type::Nominal(id) => Some(&types.table().definitions[id.0 as usize].kind),
        _ => None,
    };
    let fields = match definition {
        Some(o::DefinitionKind::Record(fields)) => fields.as_slice(),
        _ => &[],
    };
    let cases = match definition {
        Some(o::DefinitionKind::Enum(cases)) => cases.as_slice(),
        _ => &[],
    };
    let arguments = match ty {
        o::Type::Constructed(id) => match types.table().constructed[id.0 as usize].constructor {
            o::Constructor::Option(t) => [Some(t), None],
            o::Constructor::Result(t, e) => [Some(t), Some(e)],
            o::Constructor::Loan(_, t) | o::Constructor::Shared(t) | o::Constructor::Weak(t) => {
                [Some(t), None]
            }
            _ => [None, None],
        },
        _ => [None, None],
    };
    fields
        .iter()
        .map(|field| field.ty.ty)
        .chain(
            cases
                .iter()
                .flat_map(|case| case.payload.iter().map(|t| t.ty)),
        )
        .chain(arguments.into_iter().flatten())
}
