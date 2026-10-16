//! Executable ownership experiment for cyclic object graphs.
//!
//! This model compares explicit shared/weak ownership and arena-scoped handles.
//! Shared nodes reject strong cycles by default. An experimental alternative
//! permits strong cycles and traces from live external roots before collecting
//! unreachable components. Arena-local cycles are permitted because the arena,
//! rather than individual edges, owns every node. RFC 0004 accepts the strict
//! policy and arenas, not the tracing alternative. Weak upgrade here only
//! checks liveness; it does not create a retained owner as a real runtime must.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::SourceSpan;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u32);

impl NodeId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

impl Display for NodeId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "n{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ArenaId(u32);

impl ArenaId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

impl Display for ArenaId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "a{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EdgeKind {
    Strong,
    Weak,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SharedCyclePolicy {
    #[default]
    Reject,
    CollectUnreachable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphOperationKind {
    CreateShared,
    CreateArena,
    CreateArenaNode,
    AddEdge,
    DropRoot,
    DropArena,
    UpgradeWeak,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphOperation {
    CreateShared {
        node: NodeId,
        span: SourceSpan,
    },
    CreateArena {
        arena: ArenaId,
        span: SourceSpan,
    },
    CreateArenaNode {
        arena: ArenaId,
        node: NodeId,
        span: SourceSpan,
    },
    AddEdge {
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
        span: SourceSpan,
    },
    DropRoot {
        node: NodeId,
        span: SourceSpan,
    },
    DropArena {
        arena: ArenaId,
        span: SourceSpan,
    },
    UpgradeWeak {
        from: NodeId,
        to: NodeId,
        span: SourceSpan,
    },
}

impl GraphOperation {
    pub const fn kind(self) -> GraphOperationKind {
        match self {
            Self::CreateShared { .. } => GraphOperationKind::CreateShared,
            Self::CreateArena { .. } => GraphOperationKind::CreateArena,
            Self::CreateArenaNode { .. } => GraphOperationKind::CreateArenaNode,
            Self::AddEdge { .. } => GraphOperationKind::AddEdge,
            Self::DropRoot { .. } => GraphOperationKind::DropRoot,
            Self::DropArena { .. } => GraphOperationKind::DropArena,
            Self::UpgradeWeak { .. } => GraphOperationKind::UpgradeWeak,
        }
    }

    pub const fn span(self) -> SourceSpan {
        match self {
            Self::CreateShared { span, .. }
            | Self::CreateArena { span, .. }
            | Self::CreateArenaNode { span, .. }
            | Self::AddEdge { span, .. }
            | Self::DropRoot { span, .. }
            | Self::DropArena { span, .. }
            | Self::UpgradeWeak { span, .. } => span,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphEvent {
    SharedNodeCreated(NodeId),
    ArenaCreated(ArenaId),
    ArenaNodeCreated {
        arena: ArenaId,
        node: NodeId,
    },
    EdgeAdded {
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
    },
    RootDropped(NodeId),
    WeakUpgraded {
        from: NodeId,
        to: NodeId,
    },
    NodeDropped(NodeId),
    ArenaDropped(ArenaId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GraphTrace {
    events: Vec<GraphEvent>,
}

impl GraphTrace {
    pub fn events(&self) -> &[GraphEvent] {
        &self.events
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphDiagnosticCode {
    DuplicateNode,
    UnknownNode,
    UnavailableNode,
    DuplicateArena,
    UnknownArena,
    UnavailableArena,
    DuplicateEdge,
    CrossRegionEdge,
    StrongReferenceCycle,
    MissingRoot,
    MissingWeakEdge,
    ExpiredWeak,
}

impl GraphDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateNode => "SEM0010",
            Self::UnknownNode => "SEM0011",
            Self::UnavailableNode => "SEM0012",
            Self::DuplicateArena => "SEM0013",
            Self::UnknownArena => "SEM0014",
            Self::UnavailableArena => "SEM0015",
            Self::DuplicateEdge => "SEM0016",
            Self::CrossRegionEdge => "SEM0017",
            Self::StrongReferenceCycle => "SEM0018",
            Self::MissingRoot => "SEM0019",
            Self::MissingWeakEdge => "SEM0020",
            Self::ExpiredWeak => "SEM0021",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphDiagnosticKind {
    DuplicateNode(NodeId),
    UnknownNode(NodeId),
    UnavailableNode(NodeId),
    DuplicateArena(ArenaId),
    UnknownArena(ArenaId),
    UnavailableArena(ArenaId),
    DuplicateEdge { from: NodeId, to: NodeId },
    CrossRegionEdge { from: NodeId, to: NodeId },
    StrongReferenceCycle { from: NodeId, to: NodeId },
    MissingRoot(NodeId),
    MissingWeakEdge { from: NodeId, to: NodeId },
    ExpiredWeak { from: NodeId, to: NodeId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphDiagnostic {
    pub code: GraphDiagnosticCode,
    pub primary: SourceSpan,
    pub secondary: Option<SourceSpan>,
    pub kind: GraphDiagnosticKind,
}

impl Display for GraphDiagnostic {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: ", self.code.as_str())?;
        match self.kind {
            GraphDiagnosticKind::DuplicateNode(node) => {
                write!(formatter, "graph node {node} is created more than once")
            }
            GraphDiagnosticKind::UnknownNode(node) => {
                write!(formatter, "graph node {node} does not exist")
            }
            GraphDiagnosticKind::UnavailableNode(node) => {
                write!(formatter, "graph node {node} is no longer available")
            }
            GraphDiagnosticKind::DuplicateArena(arena) => {
                write!(formatter, "arena {arena} is created more than once")
            }
            GraphDiagnosticKind::UnknownArena(arena) => {
                write!(formatter, "arena {arena} does not exist")
            }
            GraphDiagnosticKind::UnavailableArena(arena) => {
                write!(formatter, "arena {arena} has already been dropped")
            }
            GraphDiagnosticKind::DuplicateEdge { from, to } => {
                write!(formatter, "an edge from {from} to {to} already exists")
            }
            GraphDiagnosticKind::CrossRegionEdge { from, to } => write!(
                formatter,
                "edge from {from} to {to} crosses an ownership region"
            ),
            GraphDiagnosticKind::StrongReferenceCycle { from, to } => write!(
                formatter,
                "strong edge from {from} to {to} would create a shared-reference cycle"
            ),
            GraphDiagnosticKind::MissingRoot(node) => {
                write!(formatter, "shared root for {node} is not available")
            }
            GraphDiagnosticKind::MissingWeakEdge { from, to } => {
                write!(formatter, "weak edge from {from} to {to} does not exist")
            }
            GraphDiagnosticKind::ExpiredWeak { from, to } => {
                write!(formatter, "weak edge from {from} to {to} has expired")
            }
        }
    }
}

impl Error for GraphDiagnostic {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeStorage {
    Shared,
    Arena(ArenaId),
}

#[derive(Clone, Copy, Debug)]
struct NodeState {
    storage: NodeStorage,
    created_at: SourceSpan,
    root_live: bool,
    live: bool,
}

#[derive(Clone, Debug)]
struct ArenaState {
    created_at: SourceSpan,
    nodes: Vec<NodeId>,
    live: bool,
}

#[derive(Clone, Copy, Debug)]
struct EdgeState {
    kind: EdgeKind,
    span: SourceSpan,
}

#[derive(Default)]
pub struct GraphModel {
    shared_cycle_policy: SharedCyclePolicy,
    nodes: BTreeMap<NodeId, NodeState>,
    arenas: BTreeMap<ArenaId, ArenaState>,
    edges: BTreeMap<(NodeId, NodeId), EdgeState>,
    creation_order: Vec<NodeId>,
    trace: GraphTrace,
}

impl GraphModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_shared_cycle_policy(policy: SharedCyclePolicy) -> Self {
        Self {
            shared_cycle_policy: policy,
            ..Self::default()
        }
    }

    pub const fn trace(&self) -> &GraphTrace {
        &self.trace
    }

    pub fn is_live(&self, node: NodeId) -> bool {
        self.nodes.get(&node).is_some_and(|state| state.live)
    }

    pub fn evaluate(operations: &[GraphOperation]) -> Result<GraphTrace, GraphDiagnostic> {
        let mut model = Self::new();
        for operation in operations {
            model.apply(*operation)?;
        }
        Ok(model.trace)
    }

    pub fn apply(&mut self, operation: GraphOperation) -> Result<(), GraphDiagnostic> {
        match operation {
            GraphOperation::CreateShared { node, span } => self.create_shared(node, span),
            GraphOperation::CreateArena { arena, span } => self.create_arena(arena, span),
            GraphOperation::CreateArenaNode { arena, node, span } => {
                self.create_arena_node(arena, node, span)
            }
            GraphOperation::AddEdge {
                from,
                to,
                kind,
                span,
            } => self.add_edge(from, to, kind, span),
            GraphOperation::DropRoot { node, span } => self.drop_root(node, span),
            GraphOperation::DropArena { arena, span } => self.drop_arena(arena, span),
            GraphOperation::UpgradeWeak { from, to, span } => self.upgrade_weak(from, to, span),
        }
    }

    fn create_shared(&mut self, node: NodeId, span: SourceSpan) -> Result<(), GraphDiagnostic> {
        self.require_new_node(node, span)?;
        self.nodes.insert(
            node,
            NodeState {
                storage: NodeStorage::Shared,
                created_at: span,
                root_live: true,
                live: true,
            },
        );
        self.creation_order.push(node);
        self.trace.events.push(GraphEvent::SharedNodeCreated(node));
        Ok(())
    }

    fn create_arena(&mut self, arena: ArenaId, span: SourceSpan) -> Result<(), GraphDiagnostic> {
        if let Some(existing) = self.arenas.get(&arena) {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::DuplicateArena,
                primary: span,
                secondary: Some(existing.created_at),
                kind: GraphDiagnosticKind::DuplicateArena(arena),
            });
        }

        self.arenas.insert(
            arena,
            ArenaState {
                created_at: span,
                nodes: Vec::new(),
                live: true,
            },
        );
        self.trace.events.push(GraphEvent::ArenaCreated(arena));
        Ok(())
    }

    fn create_arena_node(
        &mut self,
        arena: ArenaId,
        node: NodeId,
        span: SourceSpan,
    ) -> Result<(), GraphDiagnostic> {
        self.require_live_arena(arena, span)?;
        self.require_new_node(node, span)?;

        self.nodes.insert(
            node,
            NodeState {
                storage: NodeStorage::Arena(arena),
                created_at: span,
                root_live: false,
                live: true,
            },
        );
        self.arenas
            .get_mut(&arena)
            .expect("arena liveness was checked above")
            .nodes
            .push(node);
        self.creation_order.push(node);
        self.trace
            .events
            .push(GraphEvent::ArenaNodeCreated { arena, node });
        Ok(())
    }

    fn add_edge(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
        span: SourceSpan,
    ) -> Result<(), GraphDiagnostic> {
        let from_state = self.require_live_node(from, span)?;
        let to_state = self.require_live_node(to, span)?;
        if let Some(existing) = self.edges.get(&(from, to)) {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::DuplicateEdge,
                primary: span,
                secondary: Some(existing.span),
                kind: GraphDiagnosticKind::DuplicateEdge { from, to },
            });
        }

        let same_region = match (from_state.storage, to_state.storage) {
            (NodeStorage::Shared, NodeStorage::Shared) => true,
            (NodeStorage::Arena(left), NodeStorage::Arena(right)) => left == right,
            _ => false,
        };
        if !same_region {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::CrossRegionEdge,
                primary: span,
                secondary: Some(to_state.created_at),
                kind: GraphDiagnosticKind::CrossRegionEdge { from, to },
            });
        }

        if self.shared_cycle_policy == SharedCyclePolicy::Reject
            && from_state.storage == NodeStorage::Shared
            && kind == EdgeKind::Strong
            && let Some(cycle_span) = self.strong_path_span(to, from)
        {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::StrongReferenceCycle,
                primary: span,
                secondary: Some(cycle_span),
                kind: GraphDiagnosticKind::StrongReferenceCycle { from, to },
            });
        }

        self.edges.insert((from, to), EdgeState { kind, span });
        self.trace
            .events
            .push(GraphEvent::EdgeAdded { from, to, kind });
        Ok(())
    }

    fn drop_root(&mut self, node: NodeId, span: SourceSpan) -> Result<(), GraphDiagnostic> {
        let node_state = self.require_live_node(node, span)?;
        if node_state.storage != NodeStorage::Shared || !node_state.root_live {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::MissingRoot,
                primary: span,
                secondary: Some(node_state.created_at),
                kind: GraphDiagnosticKind::MissingRoot(node),
            });
        }

        self.nodes
            .get_mut(&node)
            .expect("node liveness was checked above")
            .root_live = false;
        self.trace.events.push(GraphEvent::RootDropped(node));
        self.collect_shared_nodes();
        Ok(())
    }

    fn drop_arena(&mut self, arena: ArenaId, span: SourceSpan) -> Result<(), GraphDiagnostic> {
        self.require_live_arena(arena, span)?;
        let nodes = {
            let arena_state = self
                .arenas
                .get_mut(&arena)
                .expect("arena liveness was checked above");
            arena_state.live = false;
            arena_state.nodes.clone()
        };

        for node in nodes.into_iter().rev() {
            self.drop_node(node);
        }
        self.trace.events.push(GraphEvent::ArenaDropped(arena));
        Ok(())
    }

    fn upgrade_weak(
        &mut self,
        from: NodeId,
        to: NodeId,
        span: SourceSpan,
    ) -> Result<(), GraphDiagnostic> {
        self.require_live_node(from, span)?;
        let edge = self.edges.get(&(from, to)).copied();
        if !matches!(
            edge,
            Some(EdgeState {
                kind: EdgeKind::Weak,
                ..
            })
        ) {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::MissingWeakEdge,
                primary: span,
                secondary: edge.map(|state| state.span),
                kind: GraphDiagnosticKind::MissingWeakEdge { from, to },
            });
        }

        let Some(target) = self.nodes.get(&to).copied() else {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::UnknownNode,
                primary: span,
                secondary: None,
                kind: GraphDiagnosticKind::UnknownNode(to),
            });
        };
        if !target.live {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::ExpiredWeak,
                primary: span,
                secondary: Some(target.created_at),
                kind: GraphDiagnosticKind::ExpiredWeak { from, to },
            });
        }

        self.trace
            .events
            .push(GraphEvent::WeakUpgraded { from, to });
        Ok(())
    }

    fn require_new_node(&self, node: NodeId, span: SourceSpan) -> Result<(), GraphDiagnostic> {
        if let Some(existing) = self.nodes.get(&node) {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::DuplicateNode,
                primary: span,
                secondary: Some(existing.created_at),
                kind: GraphDiagnosticKind::DuplicateNode(node),
            });
        }
        Ok(())
    }

    fn require_live_node(
        &self,
        node: NodeId,
        span: SourceSpan,
    ) -> Result<NodeState, GraphDiagnostic> {
        let state = self.nodes.get(&node).copied().ok_or(GraphDiagnostic {
            code: GraphDiagnosticCode::UnknownNode,
            primary: span,
            secondary: None,
            kind: GraphDiagnosticKind::UnknownNode(node),
        })?;
        if !state.live {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::UnavailableNode,
                primary: span,
                secondary: Some(state.created_at),
                kind: GraphDiagnosticKind::UnavailableNode(node),
            });
        }
        Ok(state)
    }

    fn require_live_arena(&self, arena: ArenaId, span: SourceSpan) -> Result<(), GraphDiagnostic> {
        let state = self.arenas.get(&arena).ok_or(GraphDiagnostic {
            code: GraphDiagnosticCode::UnknownArena,
            primary: span,
            secondary: None,
            kind: GraphDiagnosticKind::UnknownArena(arena),
        })?;
        if !state.live {
            return Err(GraphDiagnostic {
                code: GraphDiagnosticCode::UnavailableArena,
                primary: span,
                secondary: Some(state.created_at),
                kind: GraphDiagnosticKind::UnavailableArena(arena),
            });
        }
        Ok(())
    }

    fn strong_path_span(&self, start: NodeId, goal: NodeId) -> Option<SourceSpan> {
        if start == goal {
            return self.nodes.get(&start).map(|state| state.created_at);
        }

        let mut visited = BTreeSet::new();
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            if !visited.insert(node) {
                continue;
            }
            for ((from, to), edge) in &self.edges {
                if *from != node || edge.kind != EdgeKind::Strong {
                    continue;
                }
                if !self.is_live(*from) || !self.is_live(*to) {
                    continue;
                }
                if *to == goal {
                    return Some(edge.span);
                }
                pending.push(*to);
            }
        }
        None
    }

    fn collect_shared_nodes(&mut self) {
        match self.shared_cycle_policy {
            SharedCyclePolicy::Reject => self.collect_acyclic_shared_nodes(),
            SharedCyclePolicy::CollectUnreachable => self.collect_unreachable_shared_nodes(),
        }
    }

    fn collect_acyclic_shared_nodes(&mut self) {
        loop {
            let candidate = self.creation_order.iter().rev().copied().find(|node| {
                let Some(state) = self.nodes.get(node) else {
                    return false;
                };
                state.live
                    && state.storage == NodeStorage::Shared
                    && !state.root_live
                    && self.incoming_strong_edges(*node) == 0
            });
            let Some(node) = candidate else {
                break;
            };
            self.drop_node(node);
        }
    }

    fn collect_unreachable_shared_nodes(&mut self) {
        let mut reachable = BTreeSet::new();
        let mut pending = self
            .nodes
            .iter()
            .filter_map(|(node, state)| {
                (state.live && state.root_live && state.storage == NodeStorage::Shared)
                    .then_some(*node)
            })
            .collect::<Vec<_>>();

        while let Some(node) = pending.pop() {
            if !reachable.insert(node) {
                continue;
            }
            for ((from, to), edge) in &self.edges {
                if *from == node
                    && edge.kind == EdgeKind::Strong
                    && self.is_live(*to)
                    && self
                        .nodes
                        .get(to)
                        .is_some_and(|state| state.storage == NodeStorage::Shared)
                {
                    pending.push(*to);
                }
            }
        }

        let unreachable = self
            .creation_order
            .iter()
            .rev()
            .copied()
            .filter(|node| {
                self.nodes.get(node).is_some_and(|state| {
                    state.live && state.storage == NodeStorage::Shared && !reachable.contains(node)
                })
            })
            .collect::<Vec<_>>();
        for node in unreachable {
            self.drop_node(node);
        }
    }

    fn incoming_strong_edges(&self, node: NodeId) -> usize {
        self.edges
            .iter()
            .filter(|((from, to), edge)| {
                *to == node
                    && edge.kind == EdgeKind::Strong
                    && self.is_live(*from)
                    && self.is_live(*to)
            })
            .count()
    }

    fn drop_node(&mut self, node: NodeId) {
        let state = self
            .nodes
            .get_mut(&node)
            .expect("only existing nodes can be dropped");
        if state.live {
            state.live = false;
            self.trace.events.push(GraphEvent::NodeDropped(node));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArenaId, EdgeKind, GraphDiagnostic, GraphDiagnosticCode, GraphDiagnosticKind, GraphEvent,
        GraphModel, GraphOperation, NodeId, SharedCyclePolicy,
    };
    use crate::SourceSpan;

    const fn span(start: u32) -> SourceSpan {
        SourceSpan::new(start, start + 1)
    }

    #[test]
    fn rejects_a_shared_strong_reference_cycle() {
        let left = NodeId::new(0);
        let right = NodeId::new(1);
        let mut model = GraphModel::new();
        model
            .apply(GraphOperation::CreateShared {
                node: left,
                span: span(0),
            })
            .unwrap();
        model
            .apply(GraphOperation::CreateShared {
                node: right,
                span: span(1),
            })
            .unwrap();
        model
            .apply(GraphOperation::AddEdge {
                from: left,
                to: right,
                kind: EdgeKind::Strong,
                span: span(2),
            })
            .unwrap();

        assert_eq!(
            model.apply(GraphOperation::AddEdge {
                from: right,
                to: left,
                kind: EdgeKind::Strong,
                span: span(3),
            }),
            Err(GraphDiagnostic {
                code: GraphDiagnosticCode::StrongReferenceCycle,
                primary: span(3),
                secondary: Some(span(2)),
                kind: GraphDiagnosticKind::StrongReferenceCycle {
                    from: right,
                    to: left,
                },
            })
        );
    }

    #[test]
    fn collection_policy_reclaims_an_unreachable_strong_cycle() {
        let left = NodeId::new(0);
        let right = NodeId::new(1);
        let mut model = GraphModel::with_shared_cycle_policy(SharedCyclePolicy::CollectUnreachable);
        for operation in [
            GraphOperation::CreateShared {
                node: left,
                span: span(0),
            },
            GraphOperation::CreateShared {
                node: right,
                span: span(1),
            },
            GraphOperation::AddEdge {
                from: left,
                to: right,
                kind: EdgeKind::Strong,
                span: span(2),
            },
            GraphOperation::AddEdge {
                from: right,
                to: left,
                kind: EdgeKind::Strong,
                span: span(3),
            },
            GraphOperation::DropRoot {
                node: left,
                span: span(4),
            },
        ] {
            model.apply(operation).unwrap();
        }

        assert!(model.is_live(left));
        assert!(model.is_live(right));
        model
            .apply(GraphOperation::DropRoot {
                node: right,
                span: span(5),
            })
            .unwrap();

        assert!(!model.is_live(left));
        assert!(!model.is_live(right));
        assert!(model.trace().events().ends_with(&[
            GraphEvent::RootDropped(right),
            GraphEvent::NodeDropped(right),
            GraphEvent::NodeDropped(left),
        ]));
    }

    #[test]
    fn collection_policy_does_not_trace_through_weak_edges() {
        let observer = NodeId::new(0);
        let target = NodeId::new(1);
        let mut model = GraphModel::with_shared_cycle_policy(SharedCyclePolicy::CollectUnreachable);
        for operation in [
            GraphOperation::CreateShared {
                node: observer,
                span: span(0),
            },
            GraphOperation::CreateShared {
                node: target,
                span: span(1),
            },
            GraphOperation::AddEdge {
                from: observer,
                to: target,
                kind: EdgeKind::Weak,
                span: span(2),
            },
            GraphOperation::DropRoot {
                node: target,
                span: span(3),
            },
        ] {
            model.apply(operation).unwrap();
        }

        assert!(model.is_live(observer));
        assert!(!model.is_live(target));
        assert!(model.trace().events().ends_with(&[
            GraphEvent::RootDropped(target),
            GraphEvent::NodeDropped(target),
        ]));
    }

    #[test]
    fn weak_edges_break_shared_cycles_and_allow_deterministic_release() {
        let owner = NodeId::new(0);
        let child = NodeId::new(1);
        let trace = GraphModel::evaluate(&[
            GraphOperation::CreateShared {
                node: owner,
                span: span(0),
            },
            GraphOperation::CreateShared {
                node: child,
                span: span(1),
            },
            GraphOperation::AddEdge {
                from: owner,
                to: child,
                kind: EdgeKind::Strong,
                span: span(2),
            },
            GraphOperation::AddEdge {
                from: child,
                to: owner,
                kind: EdgeKind::Weak,
                span: span(3),
            },
            GraphOperation::DropRoot {
                node: child,
                span: span(4),
            },
            GraphOperation::DropRoot {
                node: owner,
                span: span(5),
            },
        ])
        .unwrap();

        let dropped = trace
            .events()
            .iter()
            .filter_map(|event| match event {
                GraphEvent::NodeDropped(node) => Some(*node),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(dropped, vec![owner, child]);
    }

    #[test]
    fn reports_an_expired_weak_edge_without_reviving_the_target() {
        let observer = NodeId::new(0);
        let target = NodeId::new(1);
        let mut model = GraphModel::new();
        for operation in [
            GraphOperation::CreateShared {
                node: observer,
                span: span(0),
            },
            GraphOperation::CreateShared {
                node: target,
                span: span(1),
            },
            GraphOperation::AddEdge {
                from: observer,
                to: target,
                kind: EdgeKind::Weak,
                span: span(2),
            },
            GraphOperation::DropRoot {
                node: target,
                span: span(3),
            },
        ] {
            model.apply(operation).unwrap();
        }

        assert_eq!(
            model.apply(GraphOperation::UpgradeWeak {
                from: observer,
                to: target,
                span: span(4),
            }),
            Err(GraphDiagnostic {
                code: GraphDiagnosticCode::ExpiredWeak,
                primary: span(4),
                secondary: Some(span(1)),
                kind: GraphDiagnosticKind::ExpiredWeak {
                    from: observer,
                    to: target,
                },
            })
        );
        assert!(!model.is_live(target));
    }

    #[test]
    fn arena_cycles_drop_in_reverse_creation_order() {
        let arena = ArenaId::new(0);
        let first = NodeId::new(0);
        let second = NodeId::new(1);
        let trace = GraphModel::evaluate(&[
            GraphOperation::CreateArena {
                arena,
                span: span(0),
            },
            GraphOperation::CreateArenaNode {
                arena,
                node: first,
                span: span(1),
            },
            GraphOperation::CreateArenaNode {
                arena,
                node: second,
                span: span(2),
            },
            GraphOperation::AddEdge {
                from: first,
                to: second,
                kind: EdgeKind::Strong,
                span: span(3),
            },
            GraphOperation::AddEdge {
                from: second,
                to: first,
                kind: EdgeKind::Strong,
                span: span(4),
            },
            GraphOperation::DropArena {
                arena,
                span: span(5),
            },
        ])
        .unwrap();

        assert!(trace.events().ends_with(&[
            GraphEvent::NodeDropped(second),
            GraphEvent::NodeDropped(first),
            GraphEvent::ArenaDropped(arena),
        ]));
    }

    #[test]
    fn rejects_stale_arena_handles_after_bulk_drop() {
        let arena = ArenaId::new(0);
        let node = NodeId::new(0);
        let mut model = GraphModel::new();
        model
            .apply(GraphOperation::CreateArena {
                arena,
                span: span(0),
            })
            .unwrap();
        model
            .apply(GraphOperation::CreateArenaNode {
                arena,
                node,
                span: span(1),
            })
            .unwrap();
        model
            .apply(GraphOperation::DropArena {
                arena,
                span: span(2),
            })
            .unwrap();

        assert_eq!(
            model.apply(GraphOperation::AddEdge {
                from: node,
                to: node,
                kind: EdgeKind::Strong,
                span: span(3),
            }),
            Err(GraphDiagnostic {
                code: GraphDiagnosticCode::UnavailableNode,
                primary: span(3),
                secondary: Some(span(1)),
                kind: GraphDiagnosticKind::UnavailableNode(node),
            })
        );
    }

    #[test]
    fn rejects_edges_between_distinct_ownership_regions() {
        let left_arena = ArenaId::new(0);
        let right_arena = ArenaId::new(1);
        let left = NodeId::new(0);
        let right = NodeId::new(1);
        let mut model = GraphModel::new();
        for operation in [
            GraphOperation::CreateArena {
                arena: left_arena,
                span: span(0),
            },
            GraphOperation::CreateArena {
                arena: right_arena,
                span: span(1),
            },
            GraphOperation::CreateArenaNode {
                arena: left_arena,
                node: left,
                span: span(2),
            },
            GraphOperation::CreateArenaNode {
                arena: right_arena,
                node: right,
                span: span(3),
            },
        ] {
            model.apply(operation).unwrap();
        }

        assert_eq!(
            model.apply(GraphOperation::AddEdge {
                from: left,
                to: right,
                kind: EdgeKind::Strong,
                span: span(4),
            }),
            Err(GraphDiagnostic {
                code: GraphDiagnosticCode::CrossRegionEdge,
                primary: span(4),
                secondary: Some(span(3)),
                kind: GraphDiagnosticKind::CrossRegionEdge {
                    from: left,
                    to: right,
                },
            })
        );
    }
}
