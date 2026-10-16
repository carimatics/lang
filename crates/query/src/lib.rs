//! Revisioned dependency/cache infrastructure shared by compiler/editor adapters.
//!
//! Hosts supply opaque keys, immutable values and explicit query dependencies.
//! This crate does not choose language module semantics, parse source, schedule
//! threads, or implement an LSP transport. Publication tickets allow a host to
//! compute outside the graph and safely reject results superseded by an edit.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_GRAPH: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Key(u64);

impl Key {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Revision(u64);

impl Revision {
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// These bound graph bookkeeping, not the host's source/analysis value storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub nodes: usize,
    pub edges: usize,
    pub work: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Nodes,
    Edges,
    Work,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphError {
    UnknownKey(Key),
    AlreadyExists(Key),
    WrongKind(Key),
    DirtyDependency(Key),
    Cycle,
    StaleTicket,
    ForeignTicket,
    LimitExceeded(LimitKind),
    RevisionExhausted,
    GraphIdsExhausted,
}

impl Display for GraphError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "incremental query graph error: {self:?}")
    }
}

impl Error for GraphError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Input,
    Query,
}

struct Node<V> {
    kind: Kind,
    revision: Revision,
    dirty: bool,
    value: Option<Arc<V>>,
    dependencies: BTreeSet<Key>,
}

/// A ticket is bound to one graph and one input/dependency revision set.
/// It cannot be constructed or cloned by callers. Starting newer work, editing,
/// closing/reopening, or publishing another ticket makes old tickets stale.
#[derive(Debug)]
pub struct Ticket {
    graph: u64,
    query: Key,
    revision: Revision,
    dependencies: Vec<(Key, Revision)>,
}

/// Explicit dependencies, transitive invalidation, and immutable cached values.
/// All fallible mutations preflight their complete bookkeeping before changing
/// graph state. Removed keys retain incoming dependency edges as tombstones,
/// so reopening an input with the same key still invalidates its consumers.
pub struct Graph<V> {
    identity: u64,
    limits: Limits,
    clock: u64,
    nodes: BTreeMap<Key, Node<V>>,
    reverse: BTreeMap<Key, BTreeSet<Key>>,
    edges: usize,
}

struct Work(usize);

impl Work {
    fn charge(&mut self, amount: usize) -> Result<(), GraphError> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or(GraphError::LimitExceeded(LimitKind::Work))?;
        Ok(())
    }
}

impl<V> Graph<V> {
    pub fn new(limits: Limits) -> Result<Self, GraphError> {
        let identity = NEXT_GRAPH
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| GraphError::GraphIdsExhausted)?;
        Ok(Self {
            identity,
            limits,
            clock: 0,
            nodes: BTreeMap::new(),
            reverse: BTreeMap::new(),
            edges: 0,
        })
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub const fn edge_count(&self) -> usize {
        self.edges
    }

    /// Dirty and missing results are cache misses. Values remain immutable even
    /// when a host holds an Arc after the graph invalidates its cached copy.
    pub fn get(&self, key: Key) -> Option<&Arc<V>> {
        self.nodes
            .get(&key)
            .filter(|node| !node.dirty)?
            .value
            .as_ref()
    }

    pub fn revision(&self, key: Key) -> Result<Revision, GraphError> {
        self.nodes
            .get(&key)
            .map(|node| node.revision)
            .ok_or(GraphError::UnknownKey(key))
    }

    pub fn is_dirty(&self, key: Key) -> Result<bool, GraphError> {
        self.nodes
            .get(&key)
            .map(|node| node.dirty)
            .ok_or(GraphError::UnknownKey(key))
    }

    fn next_revision(&mut self) -> Result<Revision, GraphError> {
        self.clock = self
            .clock
            .checked_add(1)
            .ok_or(GraphError::RevisionExhausted)?;
        Ok(Revision(self.clock))
    }

    fn check_new_node(&self) -> Result<(), GraphError> {
        if self.nodes.len() >= self.limits.nodes {
            return Err(GraphError::LimitExceeded(LimitKind::Nodes));
        }
        Ok(())
    }

    fn affected(&self, key: Key, work: &mut Work) -> Result<BTreeSet<Key>, GraphError> {
        let mut pending = vec![key];
        let mut affected = BTreeSet::new();
        while let Some(key) = pending.pop() {
            work.charge(1)?;
            if !affected.insert(key) {
                continue;
            }
            if let Some(dependents) = self.reverse.get(&key) {
                work.charge(dependents.len())?;
                pending.extend(dependents.iter().copied());
            }
        }
        Ok(affected)
    }

    fn invalidate(&mut self, affected: BTreeSet<Key>, revision: Revision) {
        for key in affected {
            if let Some(node) = self.nodes.get_mut(&key) {
                node.dirty = true;
                node.value = None;
                node.revision = revision;
            }
        }
    }

    /// A new immutable input revision invalidates only transitive consumers.
    pub fn set_input(&mut self, key: Key, value: Arc<V>) -> Result<(), GraphError> {
        match self.nodes.get(&key) {
            Some(node) if node.kind != Kind::Input => return Err(GraphError::WrongKind(key)),
            None => self.check_new_node()?,
            _ => {}
        }
        let affected = self.affected(key, &mut Work(self.limits.work))?;
        let revision = self.next_revision()?;
        self.invalidate(affected, revision);
        self.nodes.insert(
            key,
            Node {
                kind: Kind::Input,
                revision,
                dirty: false,
                value: Some(value),
                dependencies: BTreeSet::new(),
            },
        );
        Ok(())
    }

    pub fn define_query(&mut self, key: Key) -> Result<(), GraphError> {
        if self.nodes.contains_key(&key) {
            return Err(GraphError::AlreadyExists(key));
        }
        self.check_new_node()?;
        let mut work = Work(self.limits.work);
        let affected = self.affected(key, &mut work)?;
        let revision = self.next_revision()?;
        self.invalidate(affected, revision);
        self.nodes.insert(
            key,
            Node {
                kind: Kind::Query,
                revision,
                dirty: true,
                value: None,
                dependencies: BTreeSet::new(),
            },
        );
        Ok(())
    }

    fn check_cycle(
        &self,
        query: Key,
        dependencies: &BTreeSet<Key>,
        work: &mut Work,
    ) -> Result<(), GraphError> {
        work.charge(dependencies.len())?;
        let mut pending: Vec<_> = dependencies.iter().copied().collect();
        let mut seen = BTreeSet::new();
        while let Some(key) = pending.pop() {
            work.charge(1)?;
            if key == query {
                return Err(GraphError::Cycle);
            }
            if !seen.insert(key) {
                continue;
            }
            if let Some(node) = self.nodes.get(&key) {
                work.charge(node.dependencies.len())?;
                pending.extend(node.dependencies.iter().copied());
            }
        }
        Ok(())
    }

    fn detach(&mut self, query: Key, dependencies: BTreeSet<Key>) {
        for dependency in dependencies {
            if let Some(dependents) = self.reverse.get_mut(&dependency) {
                dependents.remove(&query);
                if dependents.is_empty() {
                    self.reverse.remove(&dependency);
                }
            }
        }
    }

    /// Start/restart a query after its dependencies are ready. Dependency
    /// collection, cycle checks, edge limits, and invalidation are all bounded.
    /// Callers discover dependencies; automatic query execution is a later adapter.
    pub fn begin(
        &mut self,
        query: Key,
        dependencies: impl IntoIterator<Item = Key>,
    ) -> Result<Ticket, GraphError> {
        let node = self
            .nodes
            .get(&query)
            .ok_or(GraphError::UnknownKey(query))?;
        if node.kind != Kind::Query {
            return Err(GraphError::WrongKind(query));
        }
        let mut work = Work(self.limits.work);
        let mut keys = BTreeSet::new();
        for key in dependencies {
            work.charge(1)?;
            if !self.nodes.contains_key(&key) {
                return Err(GraphError::UnknownKey(key));
            }
            keys.insert(key);
            if keys.len() > self.limits.edges {
                return Err(GraphError::LimitExceeded(LimitKind::Edges));
            }
        }
        self.check_cycle(query, &keys, &mut work)?;
        let new_edges = self
            .edges
            .checked_sub(node.dependencies.len())
            .and_then(|count| count.checked_add(keys.len()))
            .filter(|count| *count <= self.limits.edges)
            .ok_or(GraphError::LimitExceeded(LimitKind::Edges))?;
        let mut revisions = Vec::new();
        for key in &keys {
            work.charge(1)?;
            let node = &self.nodes[key];
            if node.dirty {
                return Err(GraphError::DirtyDependency(*key));
            }
            revisions.push((*key, node.revision));
        }
        let affected = self.affected(query, &mut work)?;
        // Dependency-key copies and reverse-edge changes are charged as well.
        work.charge(node.dependencies.len())?;
        work.charge(keys.len())?;
        let revision = self.next_revision()?;
        let old = std::mem::take(
            &mut self
                .nodes
                .get_mut(&query)
                .expect("query exists")
                .dependencies,
        );
        self.detach(query, old);
        for key in &keys {
            self.reverse.entry(*key).or_default().insert(query);
        }
        self.nodes
            .get_mut(&query)
            .expect("query exists")
            .dependencies = keys;
        self.edges = new_edges;
        self.invalidate(affected, revision);
        Ok(Ticket {
            graph: self.identity,
            query,
            revision,
            dependencies: revisions,
        })
    }

    /// Publish only if the graph, query generation, and every dependency still
    /// match. Failed/stale publication leaves newer cached values untouched.
    pub fn publish(&mut self, ticket: Ticket, value: Arc<V>) -> Result<(), GraphError> {
        if ticket.graph != self.identity {
            return Err(GraphError::ForeignTicket);
        }
        let node = self
            .nodes
            .get(&ticket.query)
            .ok_or(GraphError::StaleTicket)?;
        if node.kind != Kind::Query || !node.dirty || node.revision != ticket.revision {
            return Err(GraphError::StaleTicket);
        }
        let mut work = Work(self.limits.work);
        for (key, revision) in &ticket.dependencies {
            work.charge(1)?;
            let node = self.nodes.get(key).ok_or(GraphError::StaleTicket)?;
            if node.dirty || node.revision != *revision {
                return Err(GraphError::StaleTicket);
            }
        }
        let revision = self.next_revision()?;
        let node = self.nodes.get_mut(&ticket.query).expect("query exists");
        node.dirty = false;
        node.value = Some(value);
        node.revision = revision;
        Ok(())
    }

    /// Close/remove a node, retaining references from its dependent queries.
    /// Reopening the same key cannot validate any pre-close ticket.
    pub fn remove(&mut self, key: Key) -> Result<(), GraphError> {
        let node = self.nodes.get(&key).ok_or(GraphError::UnknownKey(key))?;
        let mut work = Work(self.limits.work);
        work.charge(node.dependencies.len())?;
        let affected = self.affected(key, &mut work)?;
        let revision = self.next_revision()?;
        self.invalidate(affected, revision);
        let node = self.nodes.remove(&key).expect("node exists");
        self.edges -= node.dependencies.len();
        self.detach(key, node.dependencies);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: Limits = Limits {
        nodes: 100,
        edges: 100,
        work: 1_000,
    };

    fn key(value: u64) -> Key {
        Key::new(value)
    }

    fn input(graph: &mut Graph<i32>, id: u64, value: i32) {
        graph.set_input(key(id), Arc::new(value)).unwrap();
    }

    fn query(graph: &mut Graph<i32>, id: u64, dependencies: &[u64], value: i32) {
        graph.define_query(key(id)).unwrap();
        let ticket = graph
            .begin(key(id), dependencies.iter().copied().map(key))
            .unwrap();
        graph.publish(ticket, Arc::new(value)).unwrap();
    }

    #[test]
    fn diamond_invalidation_preserves_unrelated_cached_values() {
        let mut graph = Graph::new(LIMITS).unwrap();
        assert!(graph.is_empty());
        input(&mut graph, 1, 10);
        input(&mut graph, 2, 20);
        query(&mut graph, 3, &[1], 11);
        query(&mut graph, 4, &[1], 12);
        query(&mut graph, 5, &[3, 4], 23);
        query(&mut graph, 6, &[2], 21);
        let unrelated = graph.get(key(6)).unwrap().clone();
        let previous = graph.get(key(5)).unwrap().clone();
        let revision = graph.revision(key(1)).unwrap();
        input(&mut graph, 1, 100);
        assert!(graph.revision(key(1)).unwrap().as_u64() > revision.as_u64());
        for id in [3, 4, 5] {
            assert!(graph.is_dirty(key(id)).unwrap());
            assert!(graph.get(key(id)).is_none());
        }
        assert!(Arc::ptr_eq(&unrelated, graph.get(key(6)).unwrap()));
        assert_eq!(*previous, 23);
        assert_eq!(graph.len(), 6);
        assert_eq!(graph.edge_count(), 5);
        assert_eq!(key(6).as_u64(), 6);
    }

    #[test]
    fn newer_work_and_publication_reject_older_tickets() {
        let mut graph = Graph::new(LIMITS).unwrap();
        input(&mut graph, 1, 1);
        graph.define_query(key(2)).unwrap();
        let old = graph.begin(key(2), [key(1)]).unwrap();
        let current = graph.begin(key(2), [key(1)]).unwrap();
        graph.publish(current, Arc::new(20)).unwrap();
        let published = graph.get(key(2)).unwrap().clone();
        let revision = graph.revision(key(2)).unwrap();
        assert_eq!(
            graph.publish(old, Arc::new(10)),
            Err(GraphError::StaleTicket)
        );
        assert_eq!(graph.revision(key(2)).unwrap(), revision);
        assert!(Arc::ptr_eq(&published, graph.get(key(2)).unwrap()));
    }

    #[test]
    fn tickets_are_bound_to_their_database() {
        let mut first = Graph::new(LIMITS).unwrap();
        let mut second = Graph::new(LIMITS).unwrap();
        for graph in [&mut first, &mut second] {
            input(graph, 1, 1);
            graph.define_query(key(2)).unwrap();
        }
        let foreign = first.begin(key(2), [key(1)]).unwrap();
        let own = second.begin(key(2), [key(1)]).unwrap();
        assert_eq!(
            second.publish(foreign, Arc::new(9)),
            Err(GraphError::ForeignTicket)
        );
        assert!(second.is_dirty(key(2)).unwrap());
        second.publish(own, Arc::new(2)).unwrap();
        assert_eq!(**second.get(key(2)).unwrap(), 2);
    }

    #[test]
    fn replacing_dependencies_removes_obsolete_reverse_edges() {
        let mut graph = Graph::new(LIMITS).unwrap();
        input(&mut graph, 1, 1);
        input(&mut graph, 2, 2);
        query(&mut graph, 3, &[1], 10);
        let ticket = graph.begin(key(3), [key(2), key(2)]).unwrap();
        graph.publish(ticket, Arc::new(20)).unwrap();
        assert_eq!(graph.edge_count(), 1);
        input(&mut graph, 1, 3);
        assert_eq!(**graph.get(key(3)).unwrap(), 20);
        input(&mut graph, 2, 4);
        assert!(graph.get(key(3)).is_none());
    }

    #[test]
    fn close_reopen_and_key_reuse_cannot_revive_stale_work() {
        let mut graph = Graph::new(LIMITS).unwrap();
        input(&mut graph, 1, 1);
        graph.define_query(key(2)).unwrap();
        let old = graph.begin(key(2), [key(1)]).unwrap();
        graph.remove(key(1)).unwrap();
        assert_eq!(
            graph.begin(key(2), [key(1)]).unwrap_err(),
            GraphError::UnknownKey(key(1))
        );
        input(&mut graph, 1, 2);
        assert_eq!(
            graph.publish(old, Arc::new(10)),
            Err(GraphError::StaleTicket)
        );
        let fresh = graph.begin(key(2), [key(1)]).unwrap();
        graph.publish(fresh, Arc::new(20)).unwrap();
        input(&mut graph, 1, 3);
        assert!(
            graph.get(key(2)).is_none(),
            "reopened input retains reverse dependencies"
        );
        let old = graph.begin(key(2), [key(1)]).unwrap();
        graph.remove(key(2)).unwrap();
        assert_eq!(graph.edge_count(), 0);
        graph.define_query(key(2)).unwrap();
        let fresh = graph.begin(key(2), [key(1)]).unwrap();
        graph.publish(fresh, Arc::new(30)).unwrap();
        assert_eq!(
            graph.publish(old, Arc::new(21)),
            Err(GraphError::StaleTicket)
        );
        assert_eq!(**graph.get(key(2)).unwrap(), 30);
        graph.remove(key(1)).unwrap();
        graph.remove(key(2)).unwrap();
        assert!(graph.reverse.is_empty());
        assert!(graph.is_empty());
    }

    #[test]
    fn cycle_rejection_keeps_existing_dependencies_and_cache() {
        let mut graph = Graph::new(LIMITS).unwrap();
        input(&mut graph, 1, 1);
        query(&mut graph, 2, &[1], 2);
        query(&mut graph, 3, &[2], 3);
        let cached = graph.get(key(2)).unwrap().clone();
        let revision = graph.revision(key(2)).unwrap();
        for dependency in [key(2), key(3)] {
            assert_eq!(
                graph.begin(key(2), [dependency]).unwrap_err(),
                GraphError::Cycle
            );
            assert_eq!(graph.revision(key(2)).unwrap(), revision);
            assert!(Arc::ptr_eq(&cached, graph.get(key(2)).unwrap()));
            assert!(!graph.is_dirty(key(3)).unwrap());
            assert_eq!(graph.edge_count(), 2);
        }
    }

    #[test]
    fn unknown_dirty_and_wrong_kind_operations_preserve_state() {
        let mut graph = Graph::new(LIMITS).unwrap();
        input(&mut graph, 1, 1);
        graph.define_query(key(2)).unwrap();
        graph.define_query(key(3)).unwrap();
        let revision = graph.revision(key(2)).unwrap();
        assert_eq!(
            graph.begin(key(2), [key(3)]).unwrap_err(),
            GraphError::DirtyDependency(key(3))
        );
        assert_eq!(
            graph.begin(key(2), [key(99)]).unwrap_err(),
            GraphError::UnknownKey(key(99))
        );
        assert_eq!(
            graph.begin(key(1), []).unwrap_err(),
            GraphError::WrongKind(key(1))
        );
        assert_eq!(
            graph.set_input(key(2), Arc::new(2)),
            Err(GraphError::WrongKind(key(2)))
        );
        assert_eq!(
            graph.define_query(key(1)),
            Err(GraphError::AlreadyExists(key(1)))
        );
        assert_eq!(graph.remove(key(99)), Err(GraphError::UnknownKey(key(99))));
        assert_eq!(graph.revision(key(2)).unwrap(), revision);
        assert_eq!(
            graph.revision(key(99)),
            Err(GraphError::UnknownKey(key(99)))
        );
        assert_eq!(
            graph.is_dirty(key(99)),
            Err(GraphError::UnknownKey(key(99)))
        );
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn node_and_edge_limits_preflight_before_mutation() {
        let mut graph = Graph::new(Limits {
            nodes: 2,
            edges: 0,
            ..LIMITS
        })
        .unwrap();
        input(&mut graph, 1, 1);
        graph.define_query(key(2)).unwrap();
        let revision = graph.revision(key(2)).unwrap();
        assert_eq!(
            graph.set_input(key(3), Arc::new(3)),
            Err(GraphError::LimitExceeded(LimitKind::Nodes))
        );
        assert_eq!(
            graph.define_query(key(3)),
            Err(GraphError::LimitExceeded(LimitKind::Nodes))
        );
        assert_eq!(
            graph.begin(key(2), [key(1)]).unwrap_err(),
            GraphError::LimitExceeded(LimitKind::Edges)
        );
        assert_eq!(graph.revision(key(2)).unwrap(), revision);
        assert_eq!(graph.edge_count(), 0);
        assert_eq!(graph.len(), 2);
    }

    #[test]
    fn work_limits_bound_repeated_dependencies_and_invalidation_atomically() {
        let mut graph = Graph::new(LIMITS).unwrap();
        input(&mut graph, 1, 1);
        query(&mut graph, 2, &[1], 2);
        query(&mut graph, 3, &[2], 3);
        let cached = graph.get(key(3)).unwrap().clone();
        let clock = graph.clock;
        assert_eq!(
            graph.begin(key(3), std::iter::repeat(key(2))).unwrap_err(),
            GraphError::LimitExceeded(LimitKind::Work)
        );
        assert!(Arc::ptr_eq(&cached, graph.get(key(3)).unwrap()));
        // Fault-inject a smaller budget after construction to exercise failure
        // partway through transitive invalidation, not just at its first node.
        graph.limits.work = 2;
        assert_eq!(
            graph.set_input(key(1), Arc::new(9)),
            Err(GraphError::LimitExceeded(LimitKind::Work))
        );
        assert_eq!(
            graph.remove(key(1)),
            Err(GraphError::LimitExceeded(LimitKind::Work))
        );
        assert_eq!(graph.clock, clock);
        assert_eq!(**graph.get(key(1)).unwrap(), 1);
        assert!(Arc::ptr_eq(&cached, graph.get(key(3)).unwrap()));
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn exhausted_revisions_never_wrap_or_partially_update() {
        let mut graph = Graph::new(LIMITS).unwrap();
        input(&mut graph, 1, 1);
        query(&mut graph, 2, &[1], 2);
        let cached = graph.get(key(2)).unwrap().clone();
        graph.clock = u64::MAX;
        assert_eq!(
            graph.set_input(key(1), Arc::new(9)),
            Err(GraphError::RevisionExhausted)
        );
        assert_eq!(
            graph.begin(key(2), [key(1)]).unwrap_err(),
            GraphError::RevisionExhausted
        );
        assert_eq!(graph.remove(key(1)), Err(GraphError::RevisionExhausted));
        assert_eq!(
            graph.define_query(key(3)),
            Err(GraphError::RevisionExhausted)
        );
        assert!(Arc::ptr_eq(&cached, graph.get(key(2)).unwrap()));
        assert_eq!(graph.len(), 2);
        graph.clock = u64::MAX - 1;
        let ticket = graph.begin(key(2), [key(1)]).unwrap();
        assert_eq!(
            graph.publish(ticket, Arc::new(3)),
            Err(GraphError::RevisionExhausted)
        );
        assert!(graph.is_dirty(key(2)).unwrap());
        assert!(graph.get(key(2)).is_none());
    }
}
