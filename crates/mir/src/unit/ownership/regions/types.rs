use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeId(pub usize);
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    Scalar,
    Task,
    Builtin(Builtin),
    Record(Vec<NodeId>),
    Enum(Vec<Vec<NodeId>>),
    Option(NodeId),
    Result(NodeId, NodeId),
    Vec(NodeId),
    Map(NodeId, NodeId),
    Shared(NodeId),
    Weak(NodeId),
    Arena(NodeId),
    Loan { kind: LoanKind, target: NodeId },
    ArenaOwner(ArenaOwner),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    pub ty: Type,
    pub kind: Kind,
    pub properties: Properties,
    pub span: Option<Span>,
}
impl Node {
    pub fn case(&self, case: u32) -> Option<&[NodeId]> {
        match &self.kind {
            Kind::Enum(cases) => cases.get(case as usize).map(Vec::as_slice),
            Kind::Option(_) if case == 0 => Some(&[]),
            Kind::Option(payload) if case == 1 => Some(std::slice::from_ref(payload)),
            Kind::Result(ok, _) if case == 0 => Some(std::slice::from_ref(ok)),
            Kind::Result(_, err) if case == 1 => Some(std::slice::from_ref(err)),
            _ => None,
        }
    }
}
pub(super) fn index(types: &VerifiedTypes, ty: Type) -> Option<usize> {
    let table = types.table();
    match ty {
        Type::I64 => Some(0),
        Type::Bool => Some(1),
        Type::Nominal(id) if (id.0 as usize) < table.definitions.len() => {
            2usize.checked_add(id.0 as usize)
        }
        Type::Constructed(id) if (id.0 as usize) < table.constructed.len() => 2usize
            .checked_add(table.definitions.len())?
            .checked_add(id.0 as usize),
        _ => None,
    }
}
pub(super) fn compile(types: &VerifiedTypes, admit: &mut Admission) -> Result<Vec<Node>> {
    let table = types.table();
    let count = add(add(2, table.definitions.len())?, table.constructed.len())?;
    if count > admit.limits.nodes {
        return Err(Error {
            kind: ErrorKind::Nodes {
                requested: count,
                limit: admit.limits.nodes,
            },
            span: None,
        });
    }
    let mut nodes = admit.budget.vector(count)?;
    for position in 0..count {
        let ty = if position == 0 {
            Type::I64
        } else if position == 1 {
            Type::Bool
        } else if position < table.definitions.len() + 2 {
            Type::Nominal(DefinitionId(id(position - 2)?))
        } else {
            Type::Constructed(ConstructedId(id(position - 2 - table.definitions.len())?))
        };
        let span = match ty {
            Type::Nominal(id) => Some(table.definitions[id.0 as usize].span),
            Type::Constructed(id) => Some(table.constructed[id.0 as usize].span),
            _ => None,
        };
        admit.node(span)?;
        let target = |ty| NodeId(index(types, ty).expect("verified type edge"));
        let kind = match ty {
            Type::I64 | Type::Bool => Kind::Scalar,
            Type::Nominal(id) => match &table.definitions[id.0 as usize].kind {
                DefinitionKind::Record(fields) => {
                    admit.edges(fields.len(), span)?;
                    let mut children = admit.budget.vector(fields.len())?;
                    children.extend(fields.iter().map(|field| target(field.ty.ty)));
                    Kind::Record(children)
                }
                DefinitionKind::Enum(cases) => {
                    let mut children = admit.budget.vector(cases.len())?;
                    for case in cases {
                        admit.budget.charge(1)?;
                        admit.edges(case.payload.len(), Some(case.span))?;
                        let mut payload = admit.budget.vector(case.payload.len())?;
                        payload.extend(case.payload.iter().map(|t| target(t.ty)));
                        children.push(payload);
                    }
                    Kind::Enum(children)
                }
            },
            Type::Constructed(id) => {
                let constructor = table.constructed[id.0 as usize].constructor;
                admit.edges(constructor.arguments().count(), span)?;
                match constructor {
                    Constructor::Builtin(b) => Kind::Builtin(b),
                    Constructor::Option(t) => Kind::Option(target(t)),
                    Constructor::Result(t, e) => Kind::Result(target(t), target(e)),
                    Constructor::Vec(t) => Kind::Vec(target(t)),
                    Constructor::Map(k, v) => Kind::Map(target(k), target(v)),
                    Constructor::Shared(t) => Kind::Shared(target(t)),
                    Constructor::Weak(t) => Kind::Weak(target(t)),
                    Constructor::Arena(t) => Kind::Arena(target(t)),
                    Constructor::Loan(kind, t) => Kind::Loan {
                        kind,
                        target: target(t),
                    },
                    Constructor::ArenaOwner(owner) => Kind::ArenaOwner(owner),
                    Constructor::Task(_) => Kind::Task,
                }
            }
        };
        nodes.push(Node {
            ty,
            kind,
            properties: types.properties(ty).unwrap(),
            span,
        });
    }
    Ok(nodes)
}
