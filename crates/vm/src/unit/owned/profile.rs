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
        let mut edges = 0usize;
        for index in 0..count {
            budget.charge(1)?;
            for _ in children(types, type_at(definitions, index)) {
                budget.charge(1)?;
                edges = edges.checked_add(1).ok_or(l::Error::Overflow)?;
            }
        }
        let mut stack = budget.vector(count.checked_add(edges).ok_or(l::Error::Overflow)?)?;
        for root in 0..count {
            stack.push((root, false));
            while let Some((index, expanded)) = stack.pop() {
                budget.charge(1)?;
                if states[index] >= 2 {
                    continue;
                }
                let ty = type_at(definitions, index);
                if expanded {
                    let supported = children(types, ty)
                        .all(|child| states[type_index(definitions, child)] == 2);
                    states[index] = if supported { 2 } else { 3 };
                } else if types.properties(ty).unwrap().is_copy
                    || matches!(
                        ty,
                        o::Type::Constructed(id)
                            if matches!(types.table().constructed[id.0 as usize].constructor,
                                o::Constructor::Builtin(o::Builtin::Bytes | o::Builtin::Text))
                    )
                {
                    states[index] = 2;
                } else if matches!(ty, o::Type::Nominal(_))
                    || matches!(ty, o::Type::Constructed(id)
                        if matches!(types.table().constructed[id.0 as usize].constructor,
                            o::Constructor::Option(_) | o::Constructor::Result(..)))
                {
                    // Verified inline types are acyclic; indirect services are
                    // leaves and cannot bring their recursive payloads here.
                    if states[index] == 1 {
                        return Err(l::Error::InvalidType(ty));
                    }
                    states[index] = 1;
                    stack.push((index, true));
                    for child in children(types, ty) {
                        stack.push((type_index(definitions, child), false));
                    }
                } else {
                    states[index] = 3;
                }
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
