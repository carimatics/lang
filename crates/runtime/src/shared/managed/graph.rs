use super::*;

pub(super) struct Registry<'a, 'l, 't> {
    owner: Option<a::Handle>,
    nodes: &'a [Option<Node<'l, 't>>],
    layouts: &'l l::Layouts<'t>,
    limits: Limits,
}
impl<'a, 'l, 't> Registry<'a, 'l, 't> {
    pub(super) fn new(
        owner: Option<a::Handle>,
        nodes: &'a [Option<Node<'l, 't>>],
        layouts: &'l l::Layouts<'t>,
        limits: Limits,
    ) -> Self {
        Self {
            owner,
            nodes,
            layouts,
            limits,
        }
    }
    pub(super) fn resolve<P: a::Provider>(
        &self,
        reference: Reference,
        kind: u64,
        ty: o::Type,
        customer: u64,
        allocator: &a::Allocator<P>,
    ) -> Result<usize, Error> {
        if self.owner != Some(reference.owner) || self.limits.tickets == 0 {
            return Err(unavailable());
        }
        let bytes = allocator.bytes(reference.owner)?;
        if word(bytes, 0) != MAGIC {
            return Err(Error::new(Kind::State));
        }
        let start = HEADER as usize
            + (reference.serial % self.limits.tickets as u64) as usize * ENTRY as usize;
        if word(bytes, start) != reference.serial
            || word(bytes, start + 40) != kind
            || word(bytes, start + 48) != customer
        {
            return Err(unavailable());
        }
        let index = usize::try_from(word(bytes, start + 8)).map_err(|_| unavailable())?;
        let node = self
            .nodes
            .get(index)
            .and_then(Option::as_ref)
            .ok_or_else(unavailable)?;
        if node.block.identity()?.token() != bytes[start + 16..start + 40]
            || self.layouts.types().equivalent(node.ty, ty) != Some(true)
        {
            return Err(unavailable());
        }
        if kind != WEAK {
            node.block.payload(allocator)?;
        }
        Ok(index)
    }
}
enum Value<'a, 'l, 't> {
    Input(&'a l::Reservation<'l, 't>),
    Stored(s::View<'a, 'l, 't>),
}
impl Value<'_, '_, '_> {
    fn root(&self) -> l::Location {
        match self {
            Self::Input(v) => v.root(),
            Self::Stored(v) => v.root(),
        }
    }
    fn data<'a, P: a::Provider>(
        &self,
        allocator: &'a a::Allocator<P>,
    ) -> Result<&'a [u8], l::Error> {
        match self {
            Self::Input(v) => v.data(allocator),
            Self::Stored(v) => v.data(allocator),
        }
    }
    fn child<P: a::Provider>(
        &self,
        location: l::Location,
        index: u32,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Location, l::Error> {
        match self {
            Self::Input(v) => v.child(location, index, allocator),
            Self::Stored(v) => v.child(location, index, allocator),
        }
    }
    fn case<P: a::Provider>(
        &self,
        location: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<u32, l::Error> {
        match self {
            Self::Input(v) => v.case(location, allocator),
            Self::Stored(v) => v.case(location, allocator),
        }
    }
    fn immediate<P: a::Provider>(
        &self,
        location: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Immediate, l::Error> {
        match self {
            Self::Input(v) => v.read_immediate(location, allocator),
            Self::Stored(v) => v.read_immediate(location, allocator),
        }
    }
    fn initialized<P: a::Provider>(
        &self,
        location: l::Location,
        rank: u32,
        allocator: &a::Allocator<P>,
    ) -> Result<(u32, Option<m::Span>), l::Error> {
        match self {
            Self::Input(v) => v.initialized_field(location, rank, allocator),
            Self::Stored(v) => v.initialized_field(location, rank, allocator),
        }
    }
}
struct Buffers<'a> {
    walk: &'a mut Vec<Visit>,
    edges: &'a mut Vec<Edge>,
    span: Option<m::Span>,
}
fn collect<P: a::Provider>(
    registry: Registry<'_, '_, '_>,
    buffers: Buffers<'_>,
    value: Value<'_, '_, '_>,
    customer: u64,
    allocator: &a::Allocator<P>,
    work: &mut Work,
) -> Result<(), Error> {
    let Buffers { walk, edges, span } = buffers;
    walk.clear();
    edges.clear();
    if registry.limits.value_cells == 0 {
        return Err(limit());
    }
    walk.push(Visit {
        location: value.root(),
        span,
    });
    while let Some(Visit { location, span }) = walk.pop() {
        work.charge(1)?;
        let layout = registry
            .layouts
            .get(location.ty())
            .ok_or(l::Error::InvalidType(location.ty()))?;
        let fields = match &layout.kind {
            l::Kind::Scalar | l::Kind::Error { .. } => {
                value.immediate(location, allocator)?;
                None
            }
            l::Kind::Record(fields) => Some(fields),
            l::Kind::Enum { cases, .. } => Some(
                cases
                    .get(value.case(location, allocator)? as usize)
                    .ok_or(l::Error::InvalidValue)?,
            ),
            l::Kind::Token(constructor) => {
                let start = location.offset() as usize;
                let mut bytes: [u8; 32] = value
                    .data(allocator)?
                    .get(start..start.checked_add(32).ok_or(l::Error::Overflow)?)
                    .ok_or(l::Error::State)?
                    .try_into()
                    .expect("token extent");
                match constructor {
                    o::Constructor::Shared(ty) | o::Constructor::Weak(ty) => {
                        let kind = if matches!(constructor, o::Constructor::Shared(_)) {
                            STRONG
                        } else {
                            WEAK
                        };
                        let reference = Reference::parse(bytes)?;
                        let node = registry.resolve(reference, kind, *ty, customer, allocator)?;
                        work.charge(edges.len())?;
                        if edges.iter().any(|edge| edge.reference == reference) {
                            return Err(unavailable());
                        }
                        if edges.len() == registry.limits.value_cells {
                            return Err(limit());
                        }
                        edges.push(Edge {
                            reference,
                            node,
                            kind,
                            span,
                        });
                    }
                    o::Constructor::Builtin(o::Builtin::Bytes) => {
                        buffer::Bytes::from_token(&mut bytes, allocator)
                            .and_then(|v| v.len(allocator))
                            .map_err(|e| Error::new(Kind::Buffer(e)))?;
                    }
                    o::Constructor::Builtin(o::Builtin::Text) => {
                        let text = buffer::Text::from_token(&mut bytes, allocator)
                            .map_err(|e| Error::new(Kind::Buffer(e)))?;
                        let length = usize::try_from(
                            text.len(allocator)
                                .map_err(|e| Error::new(Kind::Buffer(e)))?,
                        )
                        .map_err(|_| Error::new(Kind::State))?;
                        work.charge(length)?;
                        text.as_str(allocator)
                            .map_err(|e| Error::new(Kind::Buffer(e)))?;
                    }
                    _ => return Err(Error::new(Kind::Unsupported(location.ty()))),
                }
                None
            }
        };
        if let Some(fields) = fields {
            if walk
                .len()
                .checked_add(fields.fields.len())
                .is_none_or(|n| n > registry.limits.value_cells)
            {
                return Err(limit());
            }
            work.charge(fields.fields.len())?;
            for rank in 0..fields.fields.len() {
                let (field, span) = if layout.metadata_bytes == 0 {
                    (rank as u32, span)
                } else {
                    let (field, child_span) =
                        value.initialized(location, rank as u32, allocator)?;
                    (field, child_span.or(span))
                };
                walk.push(Visit {
                    location: value.child(location, field, allocator)?,
                    span,
                });
            }
        }
    }
    Ok(())
}
impl<'l, 't> Heap<'l, 't> {
    pub fn replace_copy<P: a::Provider>(
        &mut self,
        strong: &Strong,
        ty: o::Type,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let index = self
            .resolve(strong.0, STRONG, ty, allocator)
            .map_err(|e| e.at(Some(span)))?;
        self.replace_copy_index(index, input, span, allocator)
    }
    pub(super) fn replace_copy_index<P: a::Provider>(
        &mut self,
        index: usize,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let result = (|| {
            if !self
                .layouts
                .types()
                .properties(input.root().ty())
                .is_some_and(|p| p.is_copy)
            {
                return Err(l::Error::InvalidType(input.root().ty()).into());
            }
            self.validate_input(input, Some(span), allocator, &mut self.work())?;
            self.nodes[index]
                .as_mut()
                .unwrap()
                .block
                .replace_copy(input, span, allocator)?;
            Ok(())
        })();
        result.map_err(|e: Error| e.at(Some(span)))
    }
    pub(super) fn validate_input<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
        work: &mut Work,
    ) -> Result<(), Error> {
        if !std::ptr::eq(input.layouts(), self.layouts) {
            return Err(Error::new(Kind::State));
        }
        let ty = input.root().ty();
        let p = self
            .layouts
            .types()
            .properties(ty)
            .ok_or(l::Error::InvalidType(ty))?;
        if p.contains_loan || p.contains_allocator || p.contains_owner || p.contains_arena {
            return Err(l::Error::InvalidType(ty).into());
        }
        input.validate_retirement(allocator)?;
        collect(
            Registry::new(self.owner, &self.nodes, self.layouts, self.limits),
            Buffers {
                walk: &mut self.walk,
                edges: &mut self.incoming,
                span,
            },
            Value::Input(input),
            0,
            allocator,
            work,
        )
    }
    pub(super) fn mark<P: a::Provider>(
        &self,
        edges: &[Edge],
        customer: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let owner = self.owner(allocator)?;
        let bytes = allocator.bytes_mut(owner)?;
        for edge in edges {
            let start = HEADER as usize
                + (edge.reference.serial % self.limits.tickets as u64) as usize * ENTRY as usize;
            put(bytes, start + 48, customer);
        }
        Ok(())
    }
    fn existing<P: a::Provider>(
        &mut self,
        index: usize,
        old: bool,
        allocator: &a::Allocator<P>,
        work: &mut Work,
    ) -> Result<(), Error> {
        let node = self.nodes[index].as_ref().ok_or_else(unavailable)?;
        let span = node.block.span(allocator)?;
        let value = Value::Stored(node.block.payload(allocator)?);
        let edges = if old {
            &mut self.old_edges
        } else {
            &mut self.found
        };
        collect(
            Registry::new(self.owner, &self.nodes, self.layouts, self.limits),
            Buffers {
                walk: &mut self.walk,
                edges,
                span,
            },
            value,
            index as u64 + 1,
            allocator,
            work,
        )
    }
    fn cycles<P: a::Provider>(
        &mut self,
        index: usize,
        span: m::Span,
        allocator: &a::Allocator<P>,
        work: &mut Work,
    ) -> Result<(), Error> {
        for edge_index in 0..self.incoming.len() {
            let incoming = self.incoming[edge_index];
            if incoming.kind != STRONG {
                continue;
            }
            work.charge(self.seen.len())?;
            self.seen.fill(false);
            self.graph.clear();
            if incoming.node == index {
                return Err(core(l::OwnershipError::StrongCycle).at(Some(span)));
            }
            self.seen[incoming.node] = true;
            self.graph.push(incoming.node);
            while let Some(node) = self.graph.pop() {
                work.charge(1)?;
                self.existing(node, false, allocator, work)?;
                for edge in &self.found {
                    work.charge(1)?;
                    if edge.kind != STRONG {
                        continue;
                    }
                    if edge.node == index {
                        let mut error = core(l::OwnershipError::StrongCycle).at(Some(span));
                        error.secondary = edge.span;
                        return Err(error);
                    }
                    if !self.seen[edge.node] {
                        self.seen[edge.node] = true;
                        self.graph.push(edge.node);
                    }
                }
            }
        }
        Ok(())
    }
    /// New fields become owned by the node and old fields become owned by the
    /// detached input in one callback-free publication. Counts do not change.
    pub fn replace_from<P: a::Provider>(
        &mut self,
        strong: &Strong,
        ty: o::Type,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        let index = self
            .resolve(strong.0, STRONG, ty, allocator)
            .map_err(|e| e.at(Some(span)))?;
        self.replace_index(index, input, span, allocator)
    }
    pub(super) fn replace_index<P: a::Provider>(
        &mut self,
        index: usize,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        let result = (|| {
            let ty = self.nodes[index].as_ref().ok_or_else(unavailable)?.ty;
            if self.nodes[index]
                .as_ref()
                .unwrap()
                .block
                .counts(allocator)?
                .readers
                != 0
            {
                return Err(core(l::OwnershipError::BorrowConflict));
            }
            if !self.layouts.span_valid(span) {
                return Err(l::Error::InvalidSpan.into());
            }
            if self.layouts.types().equivalent(ty, input.root().ty()) != Some(true) {
                return Err(l::Error::InvalidType(input.root().ty()).into());
            }
            let mut work = self.work();
            self.validate_input(input, Some(span), allocator, &mut work)?;
            self.cycles(index, span, allocator, &mut work)?;
            self.existing(index, true, allocator, &mut work)?;
            let old = self.nodes[index]
                .as_mut()
                .unwrap()
                .block
                .replace_from(input, span, allocator)?;
            self.mark(&self.old_edges, 0, allocator)?;
            self.mark(&self.incoming, index as u64 + 1, allocator)?;
            Ok(old)
        })();
        result.map_err(|e: Error| e.at(Some(span)))
    }
}
