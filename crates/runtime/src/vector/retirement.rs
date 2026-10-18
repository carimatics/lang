use super::*;

/// Suspended element retirement. The live header remains reserved until every
/// descendant finishes; the outer service drain owns the yielded obligation.
pub(crate) struct Retired<'l, 't> {
    vector: Vector<'l, 't>,
    header: Header,
    index: u64,
    remaining: u64,
    value: Option<l::Reservation<'l, 't>>,
}
impl<'l, 't> Vector<'l, 't> {
    pub(crate) fn retire<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Retired<'l, 't>, Error> {
        let h = self.inspect(allocator)?;
        if h.active != 0 {
            return Err(Error::BorrowConflict);
        }
        put(allocator.bytes_mut(self.header.unwrap())?, 0, RETIRING);
        Ok(Retired {
            vector: Vector {
                layouts: self.layouts,
                ty: self.ty,
                header: self.header.take(),
            },
            header: h,
            index: h.tail,
            remaining: h.len,
            value: None,
        })
    }
}
impl Retired<'_, '_> {
    pub(crate) fn next<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<l::Token>, Error> {
        loop {
            if let Some(value) = &mut self.value {
                if let Some(leaf) = value.next_leaf(allocator)? {
                    let offset = leaf.offset as usize;
                    let bytes = value.data(allocator)?[offset..offset + 32]
                        .try_into()
                        .unwrap();
                    value.complete_leaf()?;
                    return Ok(Some(l::Token { leaf, bytes }));
                }
                self.value = None;
            }
            let h = self.header;
            if self.remaining == 0 {
                if self.index != NONE {
                    return Err(Error::State);
                }
                if let Some(data) = h.data {
                    allocator.release(data)?;
                }
                if let Some(metadata) = h.metadata {
                    allocator.release(metadata)?;
                }
                allocator.release(self.vector.header.take().ok_or(Error::State)?)?;
                return Ok(None);
            }
            if self.index >= h.len {
                return Err(Error::State);
            }
            let p = plan(self.vector.layout(), h.capacity)?;
            let offset = (mul(self.index, p.metadata_stride)? + self.vector.layout().metadata_bytes)
                as usize;
            let previous = word(allocator.bytes(h.metadata.unwrap())?, offset + 16)?;
            let bytes = &allocator.bytes(h.metadata.unwrap())?[offset..offset + 16];
            let get = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
            let span = match get(12) {
                0 => None,
                1 => Some(m::Span {
                    source: m::SourceId(get(0)),
                    start: get(4),
                    end: get(8),
                }),
                _ => return Err(Error::State),
            };
            let mut value = self.vector.element(h, self.index)?;
            value.begin_retirement(span, allocator)?;
            self.remaining -= 1;
            put(
                allocator.bytes_mut(self.vector.header.unwrap())?,
                16,
                self.remaining,
            );
            self.index = previous;
            self.value = Some(value);
        }
    }
}
