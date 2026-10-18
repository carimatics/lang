use super::*;
use crate::unit::ownership::core;

pub(super) enum Data {
    Empty,
    Brands(Vec<Brand>),
    Record(Vec<usize>),
    Sum(Vec<Vec<usize>>),
    References(Vec<usize>),
}
impl PartialEq for Data {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (Self::Brands(a), Self::Brands(b)) => a == b,
            (Self::Record(a), Self::Record(b)) | (Self::References(a), Self::References(b)) => {
                a == b
            }
            (Self::Sum(a), Self::Sum(b)) => a == b,
            _ => false,
        }
    }
}
impl Data {
    fn cost(&self) -> Result<usize> {
        Ok(match self {
            Self::Empty => 1,
            Self::Brands(v) => add(1, v.len())?,
            Self::Record(v) | Self::References(v) => add(1, v.len())?,
            Self::Sum(v) => v
                .iter()
                .try_fold(add(1, v.len())?, |n, v| add(n, v.len()))?,
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProjectionKind {
    Field(u32),
    Case(u32, u32),
    Element,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Projection {
    pub kind: ProjectionKind,
    pub node: NodeId,
}
struct Place {
    root: usize,
    path: Vec<Projection>,
}
#[derive(Clone, Copy)]
pub(super) enum RootKind {
    Slot,
    Caller { input: usize },
    Read(bodies::Site),
}
pub(super) struct Root {
    pub node: NodeId,
    pub kind: RootKind,
    pub seed: usize,
}
pub(super) struct Engine {
    pub values: Vec<Data>,
    places: Vec<Place>,
    pub roots: Vec<Root>,
    locals: usize,
    slots: usize,
    region_nodes: Vec<bool>,
    pub record: bool,
    pub stores: Vec<calls::Store>,
    pub outputs: Vec<usize>,
    pub wrote: Vec<bool>,
    pub returns: bool,
}
impl Engine {
    pub fn new(plan: &Plan<'_>, index: usize, admit: &mut Admission) -> Result<Self> {
        let f = &plan.functions[index];
        let mut region_nodes = admit.budget.vector(plan.nodes.len())?;
        for n in &plan.nodes {
            admit.budget.charge(1)?;
            region_nodes.push(n.properties.contains_arena);
        }
        let mut out = Self {
            values: Vec::new(),
            places: Vec::new(),
            roots: admit.budget.vector(f.roots.len())?,
            locals: f.locals,
            slots: f.roots.len(),
            region_nodes,
            record: false,
            stores: Vec::new(),
            outputs: Vec::new(),
            wrote: Vec::new(),
            returns: false,
        };
        out.intern(Data::Empty, admit)?;
        for root in &f.roots {
            admit.node()?;
            out.roots.push(Root {
                node: root.node,
                kind: RootKind::Slot,
                seed: 0,
            });
        }
        Ok(out)
    }
    pub fn slot(&self, slot: moves::Slot) -> Option<usize> {
        let index = match slot {
            moves::Slot::Local(id) if (id.0 as usize) < self.locals => id.0 as usize,
            moves::Slot::Value(id) => self.locals.checked_add(id.0 as usize)?,
            _ => return None,
        };
        (index < self.slots).then_some(index)
    }
    pub fn value_root(&self, value: m::Register) -> usize {
        self.locals + value.0 as usize
    }
    pub fn local_count(&self) -> usize {
        self.locals
    }
    pub fn place_parts(
        &self,
        place: usize,
        admit: &mut Admission,
    ) -> Result<(usize, Vec<Projection>)> {
        let p = &self.places[place];
        Ok((p.root, admit.copy(&p.path)?))
    }
    pub fn intern(&mut self, data: Data, admit: &mut Admission) -> Result<usize> {
        if matches!(&data,Data::Brands(v) if v.is_empty())
            || matches!(&data,Data::References(v) if v.is_empty())
            || matches!(&data,Data::Record(v) if v.iter().all(|v|*v==0))
            || matches!(&data,Data::Sum(v) if v.iter().flatten().all(|v|*v==0))
        {
            return Ok(0);
        }
        for (id, existing) in self.values.iter().enumerate() {
            admit.budget.charge(add(data.cost()?, existing.cost()?)?)?;
            if *existing == data {
                return Ok(id);
            }
        }
        admit.node()?;
        let id = self.values.len();
        admit.push(&mut self.values, data)?;
        Ok(id)
    }
    pub(super) fn data(&self, id: usize, admit: &mut Admission) -> Result<Data> {
        admit.budget.charge(1)?;
        Ok(match &self.values[id] {
            Data::Empty => Data::Empty,
            Data::Brands(v) => Data::Brands(admit.copy(v)?),
            Data::Record(v) => Data::Record(admit.copy(v)?),
            Data::References(v) => Data::References(admit.copy(v)?),
            Data::Sum(v) => {
                let mut out = admit.budget.vector(v.len())?;
                for v in v {
                    admit.budget.charge(1)?;
                    out.push(admit.copy(v)?);
                }
                Data::Sum(out)
            }
        })
    }
    pub fn place(
        &mut self,
        root: usize,
        path: &[Projection],
        admit: &mut Admission,
    ) -> Result<usize> {
        for (id, p) in self.places.iter().enumerate() {
            admit.budget.charge(add(1, path.len())?)?;
            if p.root == root && p.path == path {
                return Ok(id);
            }
        }
        let path = admit.copy(path)?;
        admit.node()?;
        let id = self.places.len();
        admit.push(&mut self.places, Place { root, path })?;
        Ok(id)
    }
    pub fn append(
        &mut self,
        place: usize,
        step: Projection,
        admit: &mut Admission,
    ) -> Result<usize> {
        let p = &self.places[place];
        let root = p.root;
        let mut path = admit.budget.vector(add(p.path.len(), 1)?)?;
        admit.budget.charge(p.path.len())?;
        path.extend_from_slice(&p.path);
        path.push(step);
        self.place(root, &path, admit)
    }
    pub fn reference(&mut self, mut places: Vec<usize>, admit: &mut Admission) -> Result<usize> {
        // Insertion sort keeps comparisons charged, including adversarial sets.
        for i in 1..places.len() {
            let mut j = i;
            while j > 0 {
                admit.budget.charge(1)?;
                if places[j - 1] <= places[j] {
                    break;
                }
                places.swap(j - 1, j);
                j -= 1;
            }
        }
        places.dedup();
        self.intern(Data::References(places), admit)
    }
    pub fn refs(&self, id: usize, admit: &mut Admission, check: bool) -> Result<Vec<usize>> {
        match &self.values[id] {
            Data::References(v) => admit.copy(v),
            Data::Empty if !check => Ok(Vec::new()),
            _ => Err(admit.error(ErrorKind::MissingReference)),
        }
    }
    pub fn brands(&mut self, mut brands: Vec<Brand>, admit: &mut Admission) -> Result<usize> {
        fn key(b: Brand) -> (u8, u32, usize, bool) {
            match b.origin {
                Origin::Arena(id) => (0, id.0, 0, b.expired),
                Origin::Caller { parameter, place } => (1, parameter.0, place, b.expired),
            }
        }
        for i in 1..brands.len() {
            let mut j = i;
            while j > 0 {
                admit.budget.charge(1)?;
                if key(brands[j - 1]) <= key(brands[j]) {
                    break;
                }
                brands.swap(j - 1, j);
                j -= 1;
            }
        }
        brands.dedup();
        self.intern(Data::Brands(brands), admit)
    }
    fn union(
        &mut self,
        out: &mut Vec<Brand>,
        incoming: &[Brand],
        admit: &mut Admission,
    ) -> Result<()> {
        for brand in incoming {
            let mut found = false;
            for old in out.iter() {
                admit.budget.charge(1)?;
                if old == brand {
                    found = true;
                    break;
                }
            }
            if !found {
                admit.push(out, *brand)?;
            }
        }
        Ok(())
    }
    pub fn collect(
        &mut self,
        row: &[usize],
        id: usize,
        admit: &mut Admission,
        depth: usize,
    ) -> Result<Vec<Brand>> {
        admit.depth(depth)?;
        let mut out = Vec::new();
        match self.data(id, admit)? {
            Data::Empty => {}
            Data::Brands(v) => return Ok(v),
            Data::Record(v) => {
                for id in v {
                    let v = self.collect(row, id, admit, depth + 1)?;
                    self.union(&mut out, &v, admit)?;
                }
            }
            Data::Sum(v) => {
                for id in v.into_iter().flatten() {
                    let v = self.collect(row, id, admit, depth + 1)?;
                    self.union(&mut out, &v, admit)?;
                }
            }
            Data::References(v) => {
                for p in v {
                    let id = self.read(row, p, admit, depth + 1)?;
                    let v = self.collect(row, id, admit, depth + 1)?;
                    self.union(&mut out, &v, admit)?;
                }
            }
        }
        Ok(out)
    }
    pub fn project(
        &mut self,
        plan: &Plan<'_>,
        value: usize,
        step: Projection,
        admit: &mut Admission,
    ) -> Result<usize> {
        admit.budget.charge(1)?;
        Ok(match &self.values[value] {
            Data::Empty => 0,
            Data::Brands(_) => {
                if plan.nodes[step.node.0].properties.contains_arena {
                    value
                } else {
                    0
                }
            }
            Data::Record(v) => match step.kind {
                ProjectionKind::Field(i) => v[i as usize],
                _ => return Err(admit.error(ErrorKind::InvalidShape)),
            },
            Data::Sum(v) => match step.kind {
                ProjectionKind::Case(c, f) => v[c as usize][f as usize],
                _ => return Err(admit.error(ErrorKind::InvalidShape)),
            },
            _ => return Err(admit.error(ErrorKind::InvalidShape)),
        })
    }
    // Preserve scalar field precision even when an element's contents use the
    // conservative container-brand abstraction.
    fn read(
        &mut self,
        row: &[usize],
        place: usize,
        admit: &mut Admission,
        depth: usize,
    ) -> Result<usize> {
        admit.depth(depth)?;
        let p = &self.places[place];
        let mut value = row[p.root];
        for step in &p.path {
            admit.budget.charge(1)?;
            value = match &self.values[value] {
                Data::Empty => 0,
                Data::Brands(_) => {
                    if self.region_nodes[step.node.0] {
                        value
                    } else {
                        0
                    }
                }
                Data::Record(v) => {
                    if let ProjectionKind::Field(i) = step.kind {
                        v[i as usize]
                    } else {
                        return Err(admit.error(ErrorKind::InvalidShape));
                    }
                }
                Data::Sum(v) => {
                    if let ProjectionKind::Case(c, f) = step.kind {
                        v[c as usize][f as usize]
                    } else {
                        return Err(admit.error(ErrorKind::InvalidShape));
                    }
                }
                _ => return Err(admit.error(ErrorKind::InvalidShape)),
            };
        }
        Ok(value)
    }
    pub fn read_place(
        &mut self,
        plan: &Plan<'_>,
        row: &[usize],
        place: usize,
        admit: &mut Admission,
    ) -> Result<usize> {
        let p = &self.places[place];
        let root = p.root;
        let path = admit.copy(&p.path)?;
        let mut value = row[root];
        for step in path {
            value = self.project(plan, value, step, admit)?;
        }
        Ok(value)
    }
    pub(super) fn child_nodes(
        plan: &Plan<'_>,
        node: NodeId,
        admit: &mut Admission,
    ) -> Result<Vec<Vec<NodeId>>> {
        let mut out = Vec::new();
        match &plan.nodes[node.0].kind {
            Kind::Record(v) => {
                let v = admit.copy(v)?;
                admit.push(&mut out, v)?;
            }
            Kind::Enum(v) => {
                for v in v {
                    let v = admit.copy(v)?;
                    admit.push(&mut out, v)?;
                }
            }
            Kind::Option(t) => {
                admit.push(&mut out, Vec::new())?;
                let v = admit.copy(&[*t])?;
                admit.push(&mut out, v)?;
            }
            Kind::Result(t, e) => {
                for t in [*t, *e] {
                    let v = admit.copy(&[t])?;
                    admit.push(&mut out, v)?;
                }
            }
            _ => {}
        }
        Ok(out)
    }
    pub fn join(
        &mut self,
        plan: &Plan<'_>,
        a: usize,
        b: usize,
        node: NodeId,
        admit: &mut Admission,
        depth: usize,
    ) -> Result<usize> {
        admit.depth(depth)?;
        if a == b || b == 0 {
            return Ok(a);
        }
        if a == 0 {
            return Ok(b);
        }
        match &plan.nodes[node.0].kind {
            Kind::Loan { .. } => {
                let mut v = self.refs(a, admit, false)?;
                let b = self.refs(b, admit, false)?;
                for b in b {
                    admit.push(&mut v, b)?;
                }
                self.reference(v, admit)
            }
            Kind::Record(_) | Kind::Enum(_) | Kind::Option(_) | Kind::Result(..) => {
                let children = Self::child_nodes(plan, node, admit)?;
                let record = matches!(plan.nodes[node.0].kind, Kind::Record(_));
                let mut cases = admit.budget.vector(children.len())?;
                for (case, nodes) in children.iter().enumerate() {
                    let mut values = admit.budget.vector(nodes.len())?;
                    for (field, node) in nodes.iter().copied().enumerate() {
                        let step = Projection {
                            kind: if record {
                                ProjectionKind::Field(field as u32)
                            } else {
                                ProjectionKind::Case(case as u32, field as u32)
                            },
                            node,
                        };
                        let a = self.project(plan, a, step, admit)?;
                        let b = self.project(plan, b, step, admit)?;
                        values.push(self.join(plan, a, b, node, admit, depth + 1)?);
                    }
                    cases.push(values);
                }
                if record {
                    self.intern(Data::Record(cases.pop().unwrap()), admit)
                } else {
                    self.intern(Data::Sum(cases), admit)
                }
            }
            _ => {
                let Data::Brands(mut v) = self.data(a, admit)? else {
                    return Err(admit.error(ErrorKind::InvalidShape));
                };
                let Data::Brands(b) = self.data(b, admit)? else {
                    return Err(admit.error(ErrorKind::InvalidShape));
                };
                self.union(&mut v, &b, admit)?;
                self.brands(v, admit)
            }
        }
    }
    pub fn parameter(
        &mut self,
        plan: &Plan<'_>,
        node: NodeId,
        parameter: declarations::LocalId,
        place: usize,
        admit: &mut Admission,
        depth: usize,
    ) -> Result<usize> {
        admit.depth(depth)?;
        if let Kind::Loan { target, .. } = plan.nodes[node.0].kind {
            admit.node()?;
            let root = self.roots.len();
            admit.push(
                &mut self.roots,
                Root {
                    node: target,
                    kind: RootKind::Caller { input: place },
                    seed: 0,
                },
            )?;
            let place = self.place(root, &[], admit)?;
            let value = self.parameter(plan, target, parameter, place, admit, depth + 1)?;
            self.roots[root].seed = value;
            let places = admit.copy(&[place])?;
            return self.reference(places, admit);
        }
        if !plan.nodes[node.0].properties.contains_arena {
            return Ok(0);
        }
        if matches!(
            plan.nodes[node.0].kind,
            Kind::Record(_) | Kind::Enum(_) | Kind::Option(_) | Kind::Result(..)
        ) {
            let record = matches!(plan.nodes[node.0].kind, Kind::Record(_));
            let children = Self::child_nodes(plan, node, admit)?;
            let mut cases = admit.budget.vector(children.len())?;
            for (case, nodes) in children.iter().enumerate() {
                let mut values = admit.budget.vector(nodes.len())?;
                for (field, node) in nodes.iter().copied().enumerate() {
                    let step = Projection {
                        kind: if record {
                            ProjectionKind::Field(field as u32)
                        } else {
                            ProjectionKind::Case(case as u32, field as u32)
                        },
                        node,
                    };
                    let p = self.append(place, step, admit)?;
                    values.push(self.parameter(plan, node, parameter, p, admit, depth + 1)?);
                }
                cases.push(values);
            }
            return if record {
                self.intern(Data::Record(cases.pop().unwrap()), admit)
            } else {
                self.intern(Data::Sum(cases), admit)
            };
        }
        let v = admit.copy(&[Brand {
            origin: Origin::Caller { parameter, place },
            expired: false,
        }])?;
        self.brands(v, admit)
    }
    pub fn read_roots(
        &mut self,
        plan: &Plan<'_>,
        index: usize,
        admit: &mut Admission,
    ) -> Result<()> {
        for (block, body) in plan.source.activity().moved().typed().functions()[index]
            .blocks
            .iter()
            .enumerate()
        {
            for (order, i) in body.instructions.iter().enumerate() {
                admit.budget.charge(1)?;
                let bodies::Op::CoreCall(call) = &i.op else {
                    continue;
                };
                let node = match call.operation {
                    core::Operation::Allocator => {
                        let Kind::Loan { target, .. } = plan.nodes[plan.functions[index]
                            .root(moves::Slot::Value(i.results[0]))
                            .unwrap()
                            .node
                            .0]
                            .kind
                        else {
                            unreachable!()
                        };
                        target
                    }
                    core::Operation::ArenaRead | core::Operation::SharedRead => {
                        plan.node_for(call.type_arguments[0].ty).unwrap()
                    }
                    _ => continue,
                };
                admit.node()?;
                admit.push(
                    &mut self.roots,
                    Root {
                        node,
                        kind: RootKind::Read(bodies::Site {
                            block: m::BlockId(block as u32),
                            order: order + 1,
                        }),
                        seed: 0,
                    },
                )?;
            }
        }
        Ok(())
    }
    pub fn synthetic(&self, site: bodies::Site) -> usize {
        self.roots
            .iter()
            .position(|r| matches!(r.kind,RootKind::Read(s) if s==site))
            .unwrap()
    }
    pub fn expire(
        &mut self,
        value: usize,
        owner: declarations::LocalId,
        admit: &mut Admission,
        depth: usize,
    ) -> Result<usize> {
        admit.depth(depth)?;
        let data = match self.data(value, admit)? {
            Data::Brands(mut v) => {
                for b in &mut v {
                    admit.budget.charge(1)?;
                    if b.origin == Origin::Arena(owner) {
                        b.expired = true;
                    }
                }
                return self.brands(v, admit);
            }
            Data::Record(mut v) => {
                for id in &mut v {
                    *id = self.expire(*id, owner, admit, depth + 1)?;
                }
                Data::Record(v)
            }
            Data::Sum(mut v) => {
                for id in v.iter_mut().flatten() {
                    *id = self.expire(*id, owner, admit, depth + 1)?;
                }
                Data::Sum(v)
            }
            _ => return Ok(value),
        };
        self.intern(data, admit)
    }
    pub fn scope(&mut self, row: &[usize], active: usize, admit: &mut Admission) -> Result<usize> {
        let mut brands = Vec::new();
        for local in 0..self.locals {
            admit.budget.charge(1)?;
            if row[active + local] == 1 {
                admit.push(
                    &mut brands,
                    Brand {
                        origin: Origin::Arena(declarations::LocalId(local as u32)),
                        expired: false,
                    },
                )?;
            }
        }
        self.brands(brands, admit)
    }
    pub fn scope_intersection(
        &mut self,
        a: usize,
        b: usize,
        admit: &mut Admission,
    ) -> Result<usize> {
        if a == usize::MAX {
            return Ok(b);
        }
        if b == usize::MAX || a == b {
            return Ok(a);
        }
        let mut v = Vec::new();
        if let (Data::Brands(a), Data::Brands(b)) = (&self.values[a], &self.values[b]) {
            for a in a {
                for b in b {
                    admit.budget.charge(1)?;
                    if a == b {
                        admit.push(&mut v, *a)?;
                        break;
                    }
                }
            }
        }
        self.brands(v, admit)
    }
    pub fn merge(
        &mut self,
        plan: &Plan<'_>,
        out: &mut [usize],
        incoming: &[usize],
        admit: &mut Admission,
    ) -> Result<bool> {
        let mut changed = false;
        let roots = self.roots.len();
        for root in 0..roots {
            let v = self.join(
                plan,
                out[root],
                incoming[root],
                self.roots[root].node,
                admit,
                0,
            )?;
            changed |= v != out[root];
            out[root] = v;
            let v = self.scope_intersection(out[roots + root], incoming[roots + root], admit)?;
            changed |= v != out[roots + root];
            out[roots + root] = v;
        }
        let active = out.len() - self.locals;
        for n in roots * 2..active {
            let mut v = self.refs(out[n], admit, false)?;
            let b = self.refs(incoming[n], admit, false)?;
            for b in b {
                admit.push(&mut v, b)?;
            }
            let v = self.reference(v, admit)?;
            changed |= v != out[n];
            out[n] = v;
        }
        for n in active..out.len() {
            admit.budget.charge(1)?;
            let v = if out[n] == incoming[n] { out[n] } else { 2 };
            changed |= v != out[n];
            out[n] = v;
        }
        Ok(changed)
    }
}
