use super::*;

/// The finite payload service profile, including every inactive enum case.
/// Walk inline dependencies once, without recursive host-stack growth.
pub(super) struct Payloads {
    states: Vec<u8>,
    definitions: usize,
    count: usize,
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
        let total = count.checked_mul(4).ok_or(l::Error::Overflow)?;
        let mut states = budget.vector(total)?;
        states.resize(total, 0u8);
        let mut edges = 0usize;
        for (index, state) in states.iter_mut().enumerate() {
            budget.charge(1)?;
            let ty = type_at(definitions, index % count);
            let shape = shape(types, ty, Flavor::at(index / count));
            *state = if matches!(shape, Shape::Unsupported) {
                3
            } else {
                2
            };
            if let Shape::Branch(_) = shape {
                for _ in children(types, ty) {
                    budget.charge(1)?;
                    edges = edges.checked_add(1).ok_or(l::Error::Overflow)?;
                }
            }
        }
        let mut heads = budget.vector(total)?;
        heads.resize(total, usize::MAX);
        let mut parents = budget.vector(edges)?;
        let mut pending = budget.vector(total)?;
        for (index, state) in states.iter().enumerate() {
            budget.charge(1)?;
            let ty = type_at(definitions, index % count);
            if *state == 3 {
                pending.push(index);
            }
            let Shape::Branch(flavor) = shape(types, ty, Flavor::at(index / count)) else {
                continue;
            };
            for child in children(types, ty) {
                budget.charge(1)?;
                let child = flavor as usize * count + type_index(definitions, child);
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
            count,
            usage: budget_usage(&budget),
        })
    }
    pub fn supports(&self, ty: o::Type) -> bool {
        self.accepts(Flavor::Root, ty)
    }
    pub fn vector(&self, ty: o::Type) -> bool {
        self.accepts(Flavor::Vector, ty)
    }
    pub fn map(&self, types: &o::VerifiedTypes, key: o::Type, value: o::Type) -> bool {
        // Vec and Map share this closed value profile until C07 integrates
        // nested services. Key admission remains the exact RFC 0007 policy.
        map_key(types, key) && self.vector(value)
    }
    pub fn shared(&self, ty: o::Type) -> bool {
        self.accepts(Flavor::Shared, ty)
    }
    pub fn arena(&self, ty: o::Type) -> bool {
        self.accepts(Flavor::Arena, ty)
    }
    fn accepts(&self, flavor: Flavor, ty: o::Type) -> bool {
        self.states[flavor as usize * self.count + type_index(self.definitions, ty)] == 2
    }
}
#[derive(Clone, Copy)]
enum Flavor {
    Root,
    Vector,
    Shared,
    Arena,
}
impl Flavor {
    fn at(n: usize) -> Self {
        match n {
            0 => Self::Root,
            1 => Self::Vector,
            2 => Self::Shared,
            _ => Self::Arena,
        }
    }
}
enum Shape {
    Unsupported,
    Leaf,
    Branch(Flavor),
}
fn map_key(types: &o::VerifiedTypes, ty: o::Type) -> bool {
    ty == o::Type::I64
        || matches!(ty, o::Type::Constructed(id) if types.table().constructed[id.0 as usize].constructor == o::Constructor::Builtin(o::Builtin::Text))
}
fn shape(types: &o::VerifiedTypes, ty: o::Type, flavor: Flavor) -> Shape {
    let properties = types.properties(ty).unwrap();
    if matches!(flavor, Flavor::Vector | Flavor::Shared)
        && (properties.contains_arena
            || properties.contains_loan
            || properties.contains_allocator
            || properties.contains_owner)
    {
        return Shape::Unsupported;
    }
    if properties.is_copy {
        return Shape::Leaf;
    }
    match ty {
        o::Type::Nominal(_) => Shape::Branch(flavor),
        o::Type::Constructed(id) => match types.table().constructed[id.0 as usize].constructor {
            o::Constructor::Builtin(o::Builtin::Bytes | o::Builtin::Text) => Shape::Leaf,
            o::Constructor::ArenaOwner(_) if matches!(flavor, Flavor::Root) => Shape::Leaf,
            o::Constructor::Option(_) | o::Constructor::Result(..) => Shape::Branch(flavor),
            o::Constructor::Loan(..) if matches!(flavor, Flavor::Root) => Shape::Branch(flavor),
            o::Constructor::Shared(_) | o::Constructor::Weak(_)
                if !matches!(flavor, Flavor::Vector) =>
            {
                Shape::Branch(Flavor::Shared)
            }
            o::Constructor::Vec(_) if matches!(flavor, Flavor::Root) => {
                Shape::Branch(Flavor::Vector)
            }
            o::Constructor::Map(key, _)
                if matches!(flavor, Flavor::Root) && map_key(types, key) =>
            {
                Shape::Branch(Flavor::Vector)
            }
            _ => Shape::Unsupported,
        },
        _ => Shape::Unsupported,
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
            o::Constructor::Map(k, v) => [Some(k), Some(v)],
            o::Constructor::Loan(_, t)
            | o::Constructor::Shared(t)
            | o::Constructor::Weak(t)
            | o::Constructor::Vec(t) => [Some(t), None],
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
