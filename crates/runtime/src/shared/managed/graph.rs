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
struct Buffers<'a, 'l, 't> {
    walk: &'a mut i::Inspector<'l, 't>,
    edges: &'a mut Vec<Edge>,
    span: Option<m::Span>,
    channels: Option<&'a crate::channel::managed::Channels<'l, 't>>,
}
fn collect<'l, 't, P: a::Provider>(
    registry: Registry<'_, 'l, 't>,
    buffers: Buffers<'_, 'l, 't>,
    value: Value<'_, 'l, 't>,
    customer: u64,
    allocator: &a::Allocator<P>,
    work: &mut Work,
) -> Result<(), Error> {
    let Buffers {
        walk,
        edges,
        span,
        channels,
    } = buffers;
    edges.clear();
    let value = match &value {
        Value::Input(v) => *v,
        Value::Stored(v) => v.reservation(),
    };
    let mut read = walk.read(value, span, allocator)?;
    while let Some(observation) = read.next(work)? {
        let mut bytes = observation.bytes;
        let l::Kind::Token(constructor) = &registry
            .layouts
            .get(observation.ty)
            .ok_or(l::Error::InvalidType(observation.ty))?
            .kind
        else {
            return Err(Error::new(Kind::State));
        };
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
                    span: observation.span,
                });
            }
            o::Constructor::Sender(_) | o::Constructor::ServiceHandle(_) => {
                // Sender carries no owning edge to its message type. The
                // invocation service still validates actual private authority.
                channels
                    .ok_or_else(|| Error::new(Kind::Unsupported(observation.ty)))?
                    .validate_sender_observation(registry.layouts, &observation, allocator)
                    .map_err(|_| unavailable())?;
            }
            o::Constructor::Builtin(o::Builtin::Bytes) => {
                buffer::Bytes::from_token(&mut bytes, allocator)
                    .and_then(|v| v.len(allocator))
                    .map_err(|e| Error::new(Kind::Buffer(e)))?;
            }
            o::Constructor::Builtin(o::Builtin::Text) => {
                let text = buffer::Text::from_token_metered(&mut bytes, allocator, &mut |n| {
                    work.service(n)
                })
                .map_err(|e| Error::new(Kind::Buffer(e)))?;
                let length = usize::try_from(
                    text.len(allocator)
                        .map_err(|e| Error::new(Kind::Buffer(e)))?,
                )
                .map_err(|_| Error::new(Kind::State))?;
                work.charge_event(length, length as u64)?;
                text.as_str(allocator)
                    .map_err(|e| Error::new(Kind::Buffer(e)))?;
            }
            _ => return Err(Error::new(Kind::Unsupported(observation.ty))),
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
            self.validate_input(input, Some(span), allocator, &mut self.work(), None)?;
            self.nodes[index]
                .as_mut()
                .unwrap()
                .block
                .replace_copy_with_service_work(
                    input,
                    span,
                    allocator,
                    self.service_work.as_ref(),
                )?;
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
        channels: Option<&crate::channel::managed::Channels<'l, 't>>,
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
                channels,
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
        channels: Option<&crate::channel::managed::Channels<'l, 't>>,
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
                channels,
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
        channels: Option<&crate::channel::managed::Channels<'l, 't>>,
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
                work.charge_event(1, 1)?;
                self.existing(node, false, allocator, work, channels)?;
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
        self.replace_index_with_channels(index, input, span, allocator, None)
    }
    pub(super) fn replace_index_with_channels<P: a::Provider>(
        &mut self,
        index: usize,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
        channels: Option<&crate::channel::managed::Channels<'l, 't>>,
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
            self.validate_input(input, Some(span), allocator, &mut work, channels)?;
            self.cycles(index, span, allocator, &mut work, channels)?;
            self.existing(index, true, allocator, &mut work, channels)?;
            let old = self.nodes[index]
                .as_mut()
                .unwrap()
                .block
                .replace_from_with_service_work(
                    input,
                    span,
                    allocator,
                    self.service_work.as_ref(),
                )?;
            self.mark(&self.old_edges, 0, allocator)?;
            self.mark(&self.incoming, index as u64 + 1, allocator)?;
            Ok(old)
        })();
        result.map_err(|e: Error| e.at(Some(span)))
    }
}
