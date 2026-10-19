use super::*;

impl<'l, 't> Heap<'l, 't> {
    /// Internal service-drain boundary. Custody is inherited unchanged through
    /// containers; only this heap can interpret a shared node's stored tickets.
    pub(crate) fn retire_obligation<P: a::Provider>(
        &mut self,
        token: &mut l::Token,
        customer: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<usize>, Error> {
        let o::Type::Constructed(id) = token.leaf.ty else {
            return Err(Error::new(Kind::State));
        };
        let (kind, ty) = match self
            .layouts
            .types()
            .table()
            .constructed
            .get(id.0 as usize)
            .map(|t| t.constructor)
        {
            Some(o::Constructor::Shared(ty)) => (STRONG, ty),
            Some(o::Constructor::Weak(ty)) => (WEAK, ty),
            _ => return Err(Error::new(Kind::State)),
        };
        let before = self.retiring.len();
        self.drop_reference(
            Reference::parse(token.bytes)?,
            kind,
            ty,
            customer,
            allocator,
        )?;
        token.bytes.fill(0);
        Ok((self.retiring.len() > before).then_some(self.retiring.len()))
    }
    pub(crate) fn retirement_step<P: a::Provider>(
        &mut self,
        level: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<(l::Token, u64)>, Error> {
        if level == 0 || self.retiring.len() != level {
            return Err(Error::new(Kind::State));
        }
        let frame = self.retiring.last_mut().unwrap();
        if let Some(token) = frame.value.next_token(allocator)? {
            frame.value.complete_leaf()?;
            return Ok(Some((token, frame.index as u64 + 1)));
        }
        let mut frame = self.retiring.pop().unwrap();
        let completed = frame.value.finish(allocator)?;
        self.nodes[frame.index]
            .as_mut()
            .unwrap()
            .block
            .complete_retirement(completed, allocator)?;
        self.reclaimed(frame.index);
        Ok(None)
    }
    fn reclaimed(&mut self, index: usize) {
        if self.nodes[index]
            .as_ref()
            .is_some_and(|n| n.block.identity().is_err())
        {
            self.nodes[index] = None;
        }
    }
    pub(super) fn enqueue<P: a::Provider>(
        &mut self,
        index: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        if self.retiring.len() == self.limits.slots {
            return Err(Error::new(Kind::State));
        }
        let owner = self.owner(allocator)?;
        let bytes = allocator.bytes_mut(owner)?;
        let live = word(bytes, 8)
            .checked_sub(1)
            .ok_or_else(|| Error::new(Kind::State))?;
        put(bytes, 8, live);
        let mut value = self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .detach(allocator)?;
        value.begin(allocator)?;
        self.retiring.push(Retirement { index, value });
        Ok(())
    }
    pub(super) fn drop_reference<P: a::Provider>(
        &mut self,
        reference: Reference,
        kind: u64,
        ty: o::Type,
        customer: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let index = Registry::new(self.owner, &self.nodes, self.layouts, self.limits)
            .resolve(reference, kind, ty, customer, allocator)?;
        self.consume(reference, allocator)?;
        if kind == WEAK {
            self.nodes[index]
                .as_mut()
                .unwrap()
                .block
                .release_weak(allocator)?;
            self.reclaimed(index);
        } else if kind == STRONG {
            if self.nodes[index]
                .as_mut()
                .unwrap()
                .block
                .release_strong(allocator)?
                == s::Release::Retire
            {
                self.enqueue(index, allocator)?;
            }
        } else {
            return Err(Error::new(Kind::State));
        }
        Ok(())
    }
    fn retire_token<P: a::Provider, F>(
        &mut self,
        token: &mut l::Token,
        customer: u64,
        allocator: &mut a::Allocator<P>,
        reason: c::Reason,
        context: &mut c::Context<'_>,
        cleaner: &mut F,
    ) -> Result<(), u32>
    where
        F: FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    {
        let edge = match token.leaf.ty {
            o::Type::Constructed(id) => {
                match self.layouts.types().table().constructed[id.0 as usize].constructor {
                    o::Constructor::Shared(ty) => Some((STRONG, ty)),
                    o::Constructor::Weak(ty) => Some((WEAK, ty)),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some((kind, ty)) = edge {
            let reference = Reference::parse(token.bytes).map_err(|_| 0x5352_0001u32)?;
            token.bytes.fill(0);
            self.drop_reference(reference, kind, ty, customer, allocator)
                .map_err(|_| 0x5352_0001u32)
        } else {
            cleaner(token, allocator, reason, context)
        }
    }
    /// Depth-first retirement uses admitted frames. A child completes before
    /// the next sibling; no Rust recursion or new retirement storage is used.
    pub(super) fn drain_pending<P: a::Provider, F>(
        &mut self,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: &mut F,
    ) -> Result<(), Error>
    where
        F: FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    {
        while let Some(frame) = self.retiring.last_mut() {
            let index = frame.index;
            if let Some(token) = frame.value.next_token(allocator)? {
                frame.value.complete_leaf()?;
                let span = token.leaf.span;
                let mut callback =
                    c::Contextual::new(|token: &mut l::Token, reason, ctx: &mut c::Context<'_>| {
                        self.retire_token(token, index as u64 + 1, allocator, reason, ctx, cleaner)
                    });
                context.field(token, span, &mut callback);
            } else {
                let mut frame = self.retiring.pop().expect("pending frame");
                let completed = frame.value.finish(allocator)?;
                self.nodes[index]
                    .as_mut()
                    .unwrap()
                    .block
                    .complete_retirement(completed, allocator)?;
                self.reclaimed(index);
            }
        }
        Ok(())
    }
    /// Drain a rejected input or detached replacement outside publication, but
    /// retain its frame storage for the caller's recycle/release policy.
    pub fn retire_value<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        if !self.retiring.is_empty() {
            return Err(Error::new(Kind::State));
        }
        input.begin_retirement(span, allocator)?;
        while let Some(leaf) = input.next_leaf(allocator)? {
            let start = leaf.offset as usize;
            let bytes = input.data(allocator)?[start..start + 32]
                .try_into()
                .expect("token extent");
            input.complete_leaf()?;
            let mut callback =
                c::Contextual::new(|token: &mut l::Token, reason, ctx: &mut c::Context<'_>| {
                    self.retire_token(token, 0, allocator, reason, ctx, &mut cleaner)
                });
            context.field(l::Token { leaf, bytes }, leaf.span, &mut callback);
            self.drain_pending(allocator, context, &mut cleaner)?;
        }
        Ok(())
    }
    pub fn read<P: a::Provider>(
        &mut self,
        strong: &Strong,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Read, Error> {
        let index = self.resolve(strong.0, STRONG, ty, allocator)?;
        self.read_index(index, allocator)
    }
    pub(super) fn read_index<P: a::Provider>(
        &mut self,
        index: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Read, Error> {
        let owner = self.owner(allocator)?;
        let readers = word(allocator.bytes(owner)?, 16);
        if readers >= self.limits.readers as u64 {
            return Err(limit());
        }
        let reference = self.vacancy(allocator, &mut self.work())?;
        self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .begin_read(self.limits.reference_count, allocator)?;
        self.register(reference, index, READ, allocator)?;
        put(allocator.bytes_mut(owner)?, 16, readers + 1);
        Ok(Read(reference))
    }
    pub fn view<P: a::Provider>(
        &self,
        read: Read,
        ty: o::Type,
        allocator: &a::Allocator<P>,
    ) -> Result<s::View<'_, 'l, 't>, Error> {
        let index = self.resolve(read.0, READ, ty, allocator)?;
        Ok(self.nodes[index]
            .as_ref()
            .unwrap()
            .block
            .payload(allocator)?
            .with_service_work(self.service_work.clone()))
    }
    pub fn end_read<P: a::Provider>(
        &mut self,
        read: Read,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        self.retire_read(read, ty, allocator)?;
        self.drain_pending(allocator, context, &mut cleaner)
    }
    pub(crate) fn retire_read<P: a::Provider>(
        &mut self,
        read: Read,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<usize>, Error> {
        let before = self.retiring.len();
        let index = self.resolve(read.0, READ, ty, allocator)?;
        let owner = self.owner(allocator)?;
        let readers = word(allocator.bytes(owner)?, 16)
            .checked_sub(1)
            .ok_or_else(|| Error::new(Kind::State))?;
        self.consume(read.0, allocator)?;
        put(allocator.bytes_mut(owner)?, 16, readers);
        if self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .end_read(allocator)?
            == s::Release::Retire
        {
            self.enqueue(index, allocator)?;
        }
        Ok((self.retiring.len() > before).then_some(self.retiring.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{map, retirement::Drain, vector};

    #[test]
    fn suspended_shared_map_vector_children_keep_custody_and_resume_after_the_last_read() {
        for retained_read in [false, true] {
            for fail in [false, true] {
                let span = |n| m::Span {
                    source: m::SourceId(0),
                    start: n,
                    end: n + 1,
                };
                let ty = |n| o::Type::Constructed(o::ConstructedId(n));
                let types = o::TypeTable {
                    sources: vec![m::Source {
                        path: "custody.lang".into(),
                        text: "x".repeat(256),
                    }],
                    constructed: [
                        o::Constructor::Builtin(o::Builtin::Bytes),
                        o::Constructor::Shared(ty(0)),
                        o::Constructor::Vec(ty(1)),
                        o::Constructor::Map(o::Type::I64, ty(2)),
                        o::Constructor::Option(ty(3)),
                        o::Constructor::Shared(ty(4)),
                    ]
                    .into_iter()
                    .map(|constructor| o::Constructed {
                        constructor,
                        span: span(0),
                    })
                    .collect(),
                    ..Default::default()
                }
                .verify(Default::default())
                .unwrap();
                let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
                let mut allocator = a::Allocator::new(
                    a::System,
                    a::Limits {
                        live_allocations: 64,
                        ..Default::default()
                    },
                )
                .unwrap();
                let mut heap = Heap::prepare(
                    &layouts,
                    Limits {
                        nodes: 4,
                        slots: 4,
                        tickets: 8,
                        ..Default::default()
                    },
                    Default::default(),
                    layouts.usage(),
                )
                .unwrap();
                let mut drain =
                    Drain::prepare(&layouts, &allocator, Default::default(), heap.usage()).unwrap();
                heap.open(&mut allocator).unwrap();
                let mut incoming =
                    l::Reservation::allocate(&layouts, ty(4), &mut allocator).unwrap();
                incoming
                    .prepare_case(incoming.root(), 0, &mut allocator)
                    .unwrap();
                incoming
                    .initialize_order(incoming.root(), [].into_iter(), &mut allocator)
                    .unwrap();
                let root = heap
                    .allocate_from(&mut incoming, Some(span(50)), &mut allocator)
                    .unwrap();
                incoming.release_storage(&mut allocator).unwrap();
                let mut vector = vector::Vector::new(&layouts, ty(1), &mut allocator).unwrap();
                for n in 1..=2 {
                    let mut value =
                        l::Reservation::allocate(&layouts, ty(0), &mut allocator).unwrap();
                    let bytes = buffer::Bytes::from_slice(&mut allocator, &[n])
                        .unwrap()
                        .take_token(&allocator)
                        .unwrap();
                    value
                        .data_mut(&mut allocator)
                        .unwrap()
                        .copy_from_slice(&bytes);
                    let strong = heap
                        .allocate_from(&mut value, Some(span(10 + u32::from(n))), &mut allocator)
                        .unwrap();
                    value.release_storage(&mut allocator).unwrap();
                    let mut value =
                        l::Reservation::allocate(&layouts, ty(1), &mut allocator).unwrap();
                    value
                        .data_mut(&mut allocator)
                        .unwrap()
                        .copy_from_slice(&strong.into_token());
                    vector
                        .push_from(&mut value, Some(span(60)), &mut allocator)
                        .unwrap();
                    value.release_storage(&mut allocator).unwrap();
                }
                let mut map = map::Map::new(&layouts, o::Type::I64, ty(2), &mut allocator).unwrap();
                let mut key =
                    l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
                let mut value = l::Reservation::allocate(&layouts, ty(2), &mut allocator).unwrap();
                let bytes = vector.take_token(&allocator).unwrap();
                value
                    .data_mut(&mut allocator)
                    .unwrap()
                    .copy_from_slice(&bytes);
                map.publish_if_absent(
                    &mut key,
                    &mut value,
                    map::Spans {
                        key: None,
                        value: Some(span(61)),
                    },
                    &mut allocator,
                )
                .unwrap();
                key.release_storage(&mut allocator).unwrap();
                value.release_storage(&mut allocator).unwrap();
                let mut incoming =
                    l::Reservation::allocate(&layouts, ty(4), &mut allocator).unwrap();
                incoming
                    .prepare_case(incoming.root(), 1, &mut allocator)
                    .unwrap();
                let at = incoming
                    .child(incoming.root(), 0, &allocator)
                    .unwrap()
                    .offset() as usize;
                let bytes = map.take_token(&allocator).unwrap();
                incoming.data_mut(&mut allocator).unwrap()[at..at + 32].copy_from_slice(&bytes);
                incoming
                    .initialize_order(incoming.root(), [(0, span(62))].into_iter(), &mut allocator)
                    .unwrap();
                // The full production path now collects container edges,
                // proves cycles/custody, publishes and retires the old value.
                heap.replace_from(&root, ty(4), &mut incoming, span(63), &mut allocator)
                    .unwrap();
                incoming.begin_retirement(None, &mut allocator).unwrap();
                assert!(incoming.next_leaf(&mut allocator).unwrap().is_none());
                incoming.release_storage(&mut allocator).unwrap();
                let parent_read = heap.read(&root, ty(4), &mut allocator).unwrap();
                let custody = heap.custody(parent_read, ty(4), &allocator).unwrap();
                let observed = {
                    let view = heap.view(parent_read, ty(4), &allocator).unwrap();
                    let mut inspector =
                        i::Inspector::prepare(&layouts, 16, Default::default(), heap.usage())
                            .unwrap();
                    let mut read = inspector
                        .read(view.reservation(), None, &allocator)
                        .unwrap();
                    let leaf = read.next(&mut i::Work::new(1000)).unwrap().unwrap();
                    l::Token {
                        leaf: l::Leaf {
                            ty: leaf.ty,
                            offset: 0,
                            span: leaf.span,
                        },
                        bytes: leaf.bytes,
                    }
                };
                let unchanged = allocator.stats();
                let counts: Vec<_> = heap
                    .nodes
                    .iter()
                    .flatten()
                    .map(|node| node.block.counts(&allocator).unwrap())
                    .collect();
                let root_token = l::Token {
                    leaf: l::Leaf {
                        ty: ty(5),
                        offset: 0,
                        span: None,
                    },
                    bytes: root.0.bytes(),
                };
                let PreviewObservation::Strong { customer, .. } = heap
                    .preview_observation(&root_token, 0, &allocator)
                    .unwrap()
                else {
                    panic!("strong root must expose a checked payload");
                };
                assert!(heap.preview_observation(&observed, 0, &allocator).is_err());
                assert!(matches!(
                    heap.preview_observation(&observed, customer, &allocator),
                    Ok(PreviewObservation::Strong { .. })
                ));
                let mut forged = l::Token {
                    leaf: l::Leaf {
                        ty: ty(1),
                        offset: 0,
                        span: None,
                    },
                    bytes: observed.bytes,
                };
                forged.bytes[24] ^= 0x80;
                assert!(
                    heap.preview_observation(&forged, customer, &allocator)
                        .is_err()
                );
                forged.bytes = observed.bytes;
                forged.leaf.ty = ty(5);
                assert!(
                    heap.preview_observation(&forged, customer, &allocator)
                        .is_err()
                );
                assert_eq!(
                    counts,
                    heap.nodes
                        .iter()
                        .flatten()
                        .map(|node| node.block.counts(&allocator).unwrap())
                        .collect::<Vec<_>>()
                );
                assert_eq!(allocator.stats(), unchanged);
                assert!(heap.read_observed(&observed, None, &mut allocator).is_err());
                assert!(
                    heap.retire_read(parent_read, ty(4), &mut allocator)
                        .unwrap()
                        .is_none()
                );
                assert!(
                    heap.retain_observed(
                        &observed,
                        Some(custody),
                        FieldAction::CloneStrong,
                        &mut allocator
                    )
                    .is_err()
                );
                assert_eq!(allocator.stats(), unchanged);
                let read = retained_read.then(|| heap.read(&root, ty(4), &mut allocator).unwrap());
                let mut token = l::Token {
                    leaf: l::Leaf {
                        ty: ty(5),
                        offset: 0,
                        span: Some(span(64)),
                    },
                    bytes: root.into_token(),
                };
                let before = allocator.stats();
                let mut found = Vec::new();
                let mut instance = c::Instance::default();
                let mut frame = instance
                    .enter_with_context(
                        c::Limits {
                            slots: 1,
                            ..Default::default()
                        },
                        |_: &mut (), _, ctx: &mut c::Context<'_>| {
                            let mut cleaner =
                                |token: &mut l::Token,
                                 allocator: &mut a::Allocator<a::System>,
                                 _,
                                 ctx: &mut c::Context<'_>| {
                                    let mut bytes =
                                        buffer::Bytes::from_token(&mut token.bytes, allocator)
                                            .unwrap();
                                    found.push((bytes.get(allocator, 0).unwrap(), ctx.span()));
                                    bytes.release(allocator).unwrap();
                                    if fail { Err(77) } else { Ok(()) }
                                };
                            drain.token(
                                &mut token,
                                Some(&mut heap),
                                &mut allocator,
                                ctx,
                                &mut cleaner,
                            )?;
                            if let Some(read) = read {
                                drain
                                    .end_read(
                                        read,
                                        ty(4),
                                        &mut heap,
                                        &mut allocator,
                                        ctx,
                                        &mut cleaner,
                                    )
                                    .map_err(|_| 99u32)?;
                            }
                            Ok(())
                        },
                    )
                    .unwrap();
                frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
                frame.initialize(c::Slot(0), (), Some(span(200))).unwrap();
                let report = frame.finish(c::Exit::Return(None)).report;
                assert_eq!(found, [(2, Some(span(12))), (1, Some(span(11)))]);
                assert_eq!(report.primary().is_some(), fail);
                assert_eq!(allocator.stats().allocations, before.allocations);
                heap.close(&mut allocator).unwrap();
                assert_eq!(allocator.stats().live_allocations, 0);
                assert_eq!(allocator.stats().live_bytes, 0);
            }
        }
    }
}
