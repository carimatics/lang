use super::*;

pub(crate) struct Retired<'l, 't> {
    map: Map<'l, 't>,
    header: Header,
    index: u64,
    remaining: u64,
    part: Part,
    value: Option<l::Reservation<'l, 't>>,
}
impl<'l, 't> Map<'l, 't> {
    pub(crate) fn retire<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Retired<'l, 't>, Error> {
        let h = self.inspect(allocator)?;
        if h.active != 0 {
            return Err(Error::BorrowConflict);
        }
        put(allocator.bytes_mut(self.owner.unwrap())?, 0, MAGIC + 1);
        Ok(Retired {
            map: Map {
                layouts: self.layouts,
                key: self.key,
                value: self.value,
                owner: self.owner.take(),
            },
            header: h,
            index: h.tail,
            remaining: h.len,
            part: Part::Value,
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
                for handle in [h.keys, h.values, h.metadata].into_iter().flatten() {
                    allocator.release(handle)?;
                }
                allocator.release(self.map.owner.take().ok_or(Error::State)?)?;
                return Ok(None);
            }
            if self.index >= h.len {
                return Err(Error::State);
            }
            let at = self.map.record(h, self.index)?;
            let span_at = at
                + if matches!(self.part, Part::Value) {
                    16
                } else {
                    0
                };
            let span = read_span(allocator.bytes(h.metadata.unwrap())?, span_at)?;
            let mut value = self.map.element(h, self.index, self.part)?;
            value.begin_retirement(span, allocator)?;
            match self.part {
                Part::Value => {
                    put(
                        allocator.bytes_mut(self.map.owner.unwrap())?,
                        32,
                        self.remaining - 1,
                    );
                    self.part = Part::Key;
                }
                Part::Key => {
                    self.index = word(allocator.bytes(h.metadata.unwrap())?, at + 32)?;
                    self.remaining -= 1;
                    self.part = Part::Value;
                }
            }
            self.value = Some(value);
        }
    }
}
