//! Bounded, source-independent region preparation and local/call analyses.
//! A Plan records shapes and paths, not a proof or execution/serialization
//! permission. Failure admission requires the complete local/call analysis.
use super::{super as m, *};
use std::fmt::{Display, Formatter};

mod types;
pub use types::{Kind, Node, NodeId};
pub mod local;
pub use local::calls;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub work: usize,
    pub allocations: usize,
    pub elements: usize,
    pub nodes: usize,
    pub paths: usize,
    pub edges: usize,
}
impl Default for Limits {
    fn default() -> Self {
        let base = m::Limits::default();
        Self {
            work: base.work,
            allocations: base.allocations,
            elements: base.elements,
            nodes: 1_000_000,
            paths: 1_000_000,
            edges: 8_000_000,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Resource(m::ErrorKind),
    Nodes { requested: usize, limit: usize },
    Paths { requested: usize, limit: usize },
    Edges { requested: usize, limit: usize },
    InvalidPath,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub span: Option<Span>,
}
impl From<m::Error> for Error {
    fn from(value: m::Error) -> Self {
        Self {
            kind: ErrorKind::Resource(value.kind),
            span: None,
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "MIR region preparation failed: {:?}", self.kind)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Counts {
    pub nodes: usize,
    pub paths: usize,
    pub edges: usize,
}
struct Admission {
    budget: Budget,
    limits: Limits,
    counts: Counts,
}
impl Admission {
    fn edges(&mut self, n: usize, span: Option<Span>) -> Result<()> {
        self.budget.charge(n)?;
        self.counts.edges = add(self.counts.edges, n)?;
        if self.counts.edges > self.limits.edges {
            return Err(Error {
                kind: ErrorKind::Edges {
                    requested: self.counts.edges,
                    limit: self.limits.edges,
                },
                span,
            });
        }
        Ok(())
    }
    fn node(&mut self, span: Option<Span>) -> Result<()> {
        self.budget.charge(1)?;
        self.counts.nodes = add(self.counts.nodes, 1)?;
        if self.counts.nodes > self.limits.nodes {
            return Err(Error {
                kind: ErrorKind::Nodes {
                    requested: self.counts.nodes,
                    limit: self.limits.nodes,
                },
                span,
            });
        }
        Ok(())
    }
    fn path(&mut self, span: Span) -> Result<()> {
        self.budget.charge(1)?;
        self.counts.paths = add(self.counts.paths, 1)?;
        if self.counts.paths > self.limits.paths {
            return Err(Error {
                kind: ErrorKind::Paths {
                    requested: self.counts.paths,
                    limit: self.limits.paths,
                },
                span: Some(span),
            });
        }
        Ok(())
    }
    fn usage(&self) -> m::Usage {
        m::Usage {
            work: self.budget.work_used(),
            allocation_bytes: self.budget.allocation_bytes(),
            elements: self.budget.elements_used(),
        }
    }
}
fn add(a: usize, b: usize) -> std::result::Result<usize, m::Error> {
    a.checked_add(b)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))
}
fn id(value: usize) -> std::result::Result<u32, m::Error> {
    u32::try_from(value).map_err(|_| m::Error::new(m::ErrorKind::ArithmeticOverflow))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Root {
    pub slot: moves::Slot,
    pub node: NodeId,
    pub span: Span,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Step {
    pub kind: places::ProjectionKind,
    pub from: NodeId,
    pub to: NodeId,
    pub span: Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Path {
    root: Root,
    steps: Vec<Step>,
    result: NodeId,
    span: Span,
}
impl Path {
    pub const fn root(&self) -> Root {
        self.root
    }
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
    pub const fn result(&self) -> NodeId {
        self.result
    }
    pub const fn span(&self) -> Span {
        self.span
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Guest(FunctionId),
    Import(m::ImportId),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effects {
    Plain,
    Regions,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Argument {
    pub value: m::Register,
    pub node: NodeId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Call {
    pub site: bodies::Site,
    pub target: Target,
    pub span: Span,
    pub effects: Effects,
    arguments: Vec<Argument>,
}
impl Call {
    pub fn arguments(&self) -> &[Argument] {
        &self.arguments
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArenaCall {
    pub site: bodies::Site,
    pub owner: ArenaOwner,
    pub span: Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    roots: Vec<Root>,
    locals: usize,
    paths: Vec<Path>,
    calls: Vec<Call>,
    arenas: Vec<ArenaCall>,
}
impl Function {
    pub fn roots(&self) -> &[Root] {
        &self.roots
    }
    pub fn root(&self, slot: moves::Slot) -> Option<Root> {
        let index = match slot {
            moves::Slot::Local(id) if (id.0 as usize) < self.locals => id.0 as usize,
            moves::Slot::Value(id) => self.locals.checked_add(id.0 as usize)?,
            _ => return None,
        };
        self.roots.get(index).copied().filter(|r| r.slot == slot)
    }
    pub fn paths(&self) -> &[Path] {
        &self.paths
    }
    pub fn path(&self, id: places::PlaceId) -> Option<&Path> {
        self.paths.get(id.0 as usize)
    }
    pub fn calls(&self) -> &[Call] {
        &self.calls
    }
    pub fn arena_calls(&self) -> &[ArenaCall] {
        &self.arenas
    }
}

pub struct Plan<'a> {
    source: &'a loans::LoanCheckedBodies,
    nodes: Vec<Node>,
    functions: Vec<Function>,
    usage: m::Usage,
    counts: Counts,
}
impl<'a> Plan<'a> {
    pub fn source(&self) -> &'a loans::LoanCheckedBodies {
        self.source
    }
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0)
    }
    pub fn node_for(&self, ty: Type) -> Option<NodeId> {
        let types = self
            .source
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        types::index(types, ty).map(NodeId)
    }
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub const fn counts(&self) -> Counts {
        self.counts
    }
}

/// Resumes the checked input's cumulative work, allocation and element counts.
/// Indirect recursive types remain graph edges; nothing is recursively unrolled.
pub fn prepare(source: &loans::LoanCheckedBodies, limits: Limits) -> Result<Plan<'_>> {
    let mut admit = Admission {
        budget: Budget::with_usage(
            m::Limits {
                work: limits.work,
                allocations: limits.allocations,
                elements: limits.elements,
                ..Default::default()
            },
            m::Usage {
                work: source.work_used(),
                allocation_bytes: source.requested_bytes(),
                elements: source.elements_used(),
            },
        )?,
        limits,
        counts: Counts {
            nodes: 0,
            paths: 0,
            edges: 0,
        },
    };
    let typed = source.activity().moved().typed();
    let types = typed.places().declarations().types();
    let nodes = types::compile(types, &mut admit)?;
    let mut functions = admit.budget.vector(typed.functions().len())?;
    for (index, body) in typed.functions().iter().enumerate() {
        let declaration = &typed.places().declarations().functions()[index];
        let frame = &typed.places().functions()[index];
        admit.budget.charge(1)?;
        let requested = add(admit.counts.paths, frame.places.len())?;
        if requested > limits.paths {
            return Err(Error {
                kind: ErrorKind::Paths {
                    requested,
                    limit: limits.paths,
                },
                span: Some(declaration.span),
            });
        }
        let mut roots = admit
            .budget
            .vector(add(declaration.locals.len(), frame.values.len())?)?;
        for (i, local) in declaration.locals.iter().enumerate() {
            admit.edges(1, Some(local.span))?;
            roots.push(Root {
                slot: moves::Slot::Local(declarations::LocalId(id(i)?)),
                node: NodeId(types::index(types, local.ty.ty).unwrap()),
                span: local.span,
            });
        }
        for (i, value) in frame.values.iter().enumerate() {
            admit.edges(1, Some(value.span))?;
            roots.push(Root {
                slot: moves::Slot::Value(m::Register(id(i)?)),
                node: NodeId(types::index(types, value.ty).unwrap()),
                span: value.span,
            });
        }
        let mut function = Function {
            roots,
            locals: declaration.locals.len(),
            paths: admit.budget.vector(frame.places.len())?,
            calls: Vec::new(),
            arenas: Vec::new(),
        };
        for path in &frame.places {
            admit.path(path.span)?;
            let slot = match path.root {
                places::Root::Local(id) => moves::Slot::Local(id),
                places::Root::Value(id) => moves::Slot::Value(id),
            };
            let root = function.root(slot).ok_or(Error {
                kind: ErrorKind::InvalidPath,
                span: Some(path.span),
            })?;
            let mut node = root.node;
            let mut steps = admit.budget.vector(path.projections.len())?;
            for projection in &path.projections {
                admit.edges(1, Some(projection.span))?;
                let next = match (&nodes[node.0].kind, projection.kind) {
                    (Kind::Loan { target, .. }, places::ProjectionKind::Dereference) => {
                        Some(*target)
                    }
                    (Kind::Record(fields), places::ProjectionKind::Field(field)) => {
                        fields.get(field as usize).copied()
                    }
                    _ => None,
                }
                .ok_or(Error {
                    kind: ErrorKind::InvalidPath,
                    span: Some(projection.span),
                })?;
                steps.push(Step {
                    kind: projection.kind,
                    from: node,
                    to: next,
                    span: projection.span,
                });
                node = next;
            }
            function.paths.push(Path {
                root,
                steps,
                result: node,
                span: path.span,
            });
        }
        let mut calls = 0usize;
        for block in &body.blocks {
            admit.budget.charge(1)?;
            for instruction in &block.instructions {
                admit.budget.charge(1)?;
                calls = add(
                    calls,
                    usize::from(matches!(
                        instruction.op,
                        bodies::Op::Call { .. } | bodies::Op::Import { .. }
                    )),
                )?;
            }
        }
        function.calls = admit.budget.vector(calls)?;
        for (block, body) in body.blocks.iter().enumerate() {
            admit.budget.charge(1)?;
            for (order, instruction) in body.instructions.iter().enumerate() {
                admit.budget.charge(1)?;
                let (target, arguments) = match &instruction.op {
                    bodies::Op::Call {
                        function,
                        arguments,
                    } => (Target::Guest(*function), arguments),
                    bodies::Op::Import { import, arguments } => {
                        (Target::Import(*import), arguments)
                    }
                    _ => continue,
                };
                let mut args = admit.budget.vector(arguments.len())?;
                let mut effects = Effects::Plain;
                for value in arguments {
                    admit.edges(1, Some(instruction.span))?;
                    let root = function.root(moves::Slot::Value(*value)).unwrap();
                    let properties = nodes[root.node.0].properties;
                    if properties.contains_arena || properties.contains_owner {
                        effects = Effects::Regions;
                    }
                    args.push(Argument {
                        value: *value,
                        node: root.node,
                    });
                }
                function.calls.push(Call {
                    site: bodies::Site {
                        block: m::BlockId(id(block)?),
                        order: order + 1,
                    },
                    target,
                    span: instruction.span,
                    effects,
                    arguments: args,
                });
            }
        }
        let facts = typed.facts()[index].core_calls();
        function.arenas = admit.budget.vector(facts.len())?;
        for fact in facts {
            admit.budget.charge(1)?;
            if let Some(owner) = fact.checked.arena_owner {
                let span =
                    body.blocks[fact.site.block.0 as usize].instructions[fact.site.order - 1].span;
                function.arenas.push(ArenaCall {
                    site: fact.site,
                    owner,
                    span,
                });
            }
        }
        functions.push(function);
    }
    Ok(Plan {
        source,
        nodes,
        functions,
        usage: admit.usage(),
        counts: admit.counts,
    })
}
