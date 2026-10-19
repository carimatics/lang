//! Checked, callback-free observations of indirect result payloads.
use super::*;

pub(super) struct MapWalk {
    key: o::Type,
    value: o::Type,
    token: [u8; 32],
    customer: u64,
    start: usize,
    next: usize,
    end: usize,
}
impl From<vector::Error> for PreviewError {
    fn from(e: vector::Error) -> Self {
        Self::Vector(e)
    }
}
impl From<map::Error> for PreviewError {
    fn from(e: map::Error) -> Self {
        Self::Map(e)
    }
}
impl From<shared::Error> for PreviewError {
    fn from(e: shared::Error) -> Self {
        Self::Shared(e)
    }
}
fn less<P: Provider>(
    a: OrderedKey,
    b: OrderedKey,
    allocator: &Allocator<P>,
) -> Result<bool, PreviewError> {
    match (a.key, b.key) {
        (map::Key::I64(a), map::Key::I64(b)) => Ok(a < b),
        (map::Key::Text(a), map::Key::Text(b)) => {
            Ok(buffer::text_token_bytes(a, allocator)? < buffer::text_token_bytes(b, allocator)?)
        }
        _ => Err(Error::InvalidValue.into()),
    }
}
// Iterative fallible heapsort. No allocation, recursive visitor, guest key
// comparison, mutation, or provider callback occurs while ordering indices.
fn sift<P: Provider>(
    order: &mut [OrderedKey],
    mut root: usize,
    end: usize,
    allocator: &Allocator<P>,
) -> Result<(), PreviewError> {
    while let Some(mut child) = root
        .checked_mul(2)
        .and_then(|n| n.checked_add(1))
        .filter(|n| *n < end)
    {
        if child + 1 < end && less(order[child], order[child + 1], allocator)? {
            child += 1;
        }
        if !less(order[root], order[child], allocator)? {
            break;
        }
        order.swap(root, child);
        root = child;
    }
    Ok(())
}
fn sort<P: Provider>(
    order: &mut [OrderedKey],
    allocator: &Allocator<P>,
) -> Result<(), PreviewError> {
    let len = order.len();
    for root in (0..len / 2).rev() {
        sift(order, root, len, allocator)?;
    }
    for end in (1..order.len()).rev() {
        order.swap(0, end);
        sift(order, 0, end, allocator)?;
    }
    Ok(())
}
impl<'l, 't> Preview<'l, 't> {
    pub(super) fn vector<P: Provider>(
        &mut self,
        site: Site<'l, 't>,
        ty: o::Type,
        allocator: &Allocator<P>,
    ) -> Result<(), PreviewError> {
        let token = site.value.token(site.location, allocator)?.bytes;
        let vector = vector::Vector::from_token(self.layouts, ty, &mut { token }, allocator)?;
        let len = vector.len(allocator)? as u64;
        self.text("core::Vec [")?;
        if len > (self.output.limit - self.output.text.len()) as u64 {
            return Err(PreviewError::Limit);
        }
        self.push(Print::Vector {
            ty,
            token,
            customer: site.customer,
            next: 0,
            len,
        })
    }
    pub(super) fn vector_next<P: Provider>(
        &mut self,
        ty: o::Type,
        token: [u8; 32],
        customer: u64,
        next: u64,
        len: u64,
        allocator: &Allocator<P>,
    ) -> Result<(), PreviewError> {
        if next == len {
            return self.text("]");
        }
        let vector = vector::Vector::from_token(self.layouts, ty, &mut { token }, allocator)?;
        let (view, _) = vector.observation(next, allocator)?;
        if next != 0 {
            self.text(", ")?;
        }
        self.push(Print::Vector {
            ty,
            token,
            customer,
            next: next + 1,
            len,
        })?;
        self.push(Print::Value(Site::root(view, customer)))
    }
    pub(super) fn map<P: Provider>(
        &mut self,
        site: Site<'l, 't>,
        key: o::Type,
        value: o::Type,
        allocator: &Allocator<P>,
    ) -> Result<(), PreviewError> {
        let token = site.value.token(site.location, allocator)?.bytes;
        let map = map::Map::from_token(self.layouts, key, value, &mut { token }, allocator)?;
        let len = usize::try_from(map.len(allocator)?).map_err(|_| PreviewError::Limit)?;
        if len == 0 {
            return self.text("core::Map {}");
        }
        self.text("core::Map { ")?;
        let start = self.order.len();
        let end = start.checked_add(len).ok_or(PreviewError::Limit)?;
        let available = self.output.limit - self.output.text.len();
        let mut key_bytes = len.checked_mul(4).ok_or(PreviewError::Limit)?;
        if end > self.order.capacity() || key_bytes > available {
            return Err(PreviewError::Limit);
        }
        for index in 0..len as u64 {
            let (view, _) = map.observation(index, map::Part::Key, allocator)?;
            let key = if key == o::Type::I64 {
                let Immediate::I64(n) = view.immediate(view.root(), allocator)? else {
                    return Err(Error::InvalidValue.into());
                };
                map::Key::I64(n)
            } else {
                let token = view.token(view.root(), allocator)?.bytes;
                let bytes = buffer::text_token_bytes(token, allocator)?;
                key_bytes = key_bytes
                    .checked_add(bytes.len())
                    .ok_or(PreviewError::Limit)?;
                if key_bytes > available {
                    return Err(PreviewError::Limit);
                }
                std::str::from_utf8(bytes).map_err(|_| Error::InvalidValue)?;
                map::Key::Text(token)
            };
            self.order.push(OrderedKey { key, index });
        }
        sort(&mut self.order[start..end], allocator)?;
        self.push(Print::Map(MapWalk {
            key,
            value,
            token,
            customer: site.customer,
            start,
            next: start,
            end,
        }))
    }
    pub(super) fn map_next<P: Provider>(
        &mut self,
        walk: MapWalk,
        allocator: &Allocator<P>,
    ) -> Result<(), PreviewError> {
        if walk.next == walk.end {
            self.order.truncate(walk.start);
            return self.text(" }");
        }
        let index = self.order[walk.next].index;
        let map = map::Map::from_token(
            self.layouts,
            walk.key,
            walk.value,
            &mut { walk.token },
            allocator,
        )?;
        let (key, _) = map.observation(index, map::Part::Key, allocator)?;
        let (value, _) = map.observation(index, map::Part::Value, allocator)?;
        if walk.next != walk.start {
            self.text(", ")?;
        }
        let customer = walk.customer;
        self.push(Print::Map(MapWalk {
            next: walk.next + 1,
            ..walk
        }))?;
        self.push(Print::Value(Site::root(value, customer)))?;
        self.push(Print::Text(": "))?;
        self.push(Print::Value(Site::root(key, customer)))
    }
    pub(super) fn shared<P: Provider>(
        &mut self,
        site: Site<'l, 't>,
        heap: &shared::Heap<'l, 't>,
        allocator: &Allocator<P>,
    ) -> Result<(), PreviewError> {
        let token = site.value.token(site.location, allocator)?;
        match heap.preview_observation(&token, site.customer, allocator)? {
            shared::PreviewObservation::Strong { value, customer } => {
                self.text("shared(")?;
                self.push(Print::Text(")"))?;
                self.push(Print::Value(Site::root(value, customer)))
            }
            shared::PreviewObservation::Weak { live } => {
                self.text(if live { "weak::Live" } else { "weak::Expired" })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_results_are_refused_before_allocator_authority() {
        let span = m::Span {
            source: m::SourceId(0),
            start: 0,
            end: 1,
        };
        let arena = o::Type::Constructed(o::ConstructedId(0));
        let vector = o::Type::Constructed(o::ConstructedId(1));
        let map = o::Type::Constructed(o::ConstructedId(2));
        let types = o::TypeTable {
            sources: vec![m::Source {
                path: "preview.lang".into(),
                text: "x".into(),
            }],
            constructed: vec![
                o::Constructed {
                    constructor: o::Constructor::Arena(o::Type::I64),
                    span,
                },
                o::Constructed {
                    constructor: o::Constructor::Vec(arena),
                    span,
                },
                o::Constructed {
                    constructor: o::Constructor::Map(o::Type::I64, o::Type::I64),
                    span,
                },
            ],
            ..Default::default()
        }
        .verify(Default::default())
        .unwrap();
        let layouts = Layouts::compile(&types, Default::default()).unwrap();
        assert!(Preview::prepare(&layouts, map, 64, Default::default(), layouts.usage()).is_ok());
        assert!(
            Preview::prepare(
                &layouts,
                map,
                64,
                m::Limits {
                    allocations: layouts.usage().allocation_bytes,
                    ..Default::default()
                },
                layouts.usage()
            )
            .is_err()
        );
        for result in [arena, vector] {
            assert!(matches!(
                Preview::prepare(&layouts, result, 65536, Default::default(), layouts.usage()),
                Err(Error::InvalidType(ty)) if ty == arena
            ));
        }
    }
}
