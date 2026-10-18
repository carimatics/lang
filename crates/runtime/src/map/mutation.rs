use super::*;

/// The old value is already owned by the admitted output. The adapter must
/// consume this detached key exactly once through its common cleanup context
/// before later guest effects, protecting the output if that cleanup fails.
/// Ordinary Rust disposal does not perform guest cleanup.
pub struct Displaced {
    pub key: Option<l::Token>,
    pub value_span: Option<m::Span>,
}

fn detach_key<P: a::Provider>(
    cell: &mut l::Reservation<'_, '_>,
    span: Option<m::Span>,
    allocator: &mut a::Allocator<P>,
) -> Result<Option<l::Token>, Error> {
    if cell.root().ty() == o::Type::I64 {
        return Ok(None);
    }
    cell.begin_retirement(span, allocator)?;
    let leaf = cell.next_leaf(allocator)?.ok_or(Error::State)?;
    let offset = leaf.offset as usize;
    let token = l::Token {
        leaf,
        bytes: cell.data(allocator)?[offset..offset + 32]
            .try_into()
            .unwrap(),
    };
    cell.complete_leaf()?;
    if cell.next_leaf(allocator)?.is_some() {
        return Err(Error::State);
    }
    cell.clear_drained(allocator)?;
    Ok(Some(token))
}

impl<'l, 't> Map<'l, 't> {
    /// One checked Copy i64 operand may supply both key and value in MIR. A
    /// non-owning scalar view avoids an extra guest reservation or mutable alias.
    pub fn insert_repeated_integer_into<P: a::Provider>(
        &mut self,
        value: &mut l::Reservation<'l, 't>,
        spans: Spans,
        destination: &l::Reservation<'l, 't>,
        target: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<Displaced>, Error> {
        if self.key != o::Type::I64 || self.value != o::Type::I64 {
            return Err(Error::WrongKey);
        }
        let mut key = value.i64_view()?;
        self.insert_into(&mut key, value, spans, destination, target, allocator)
    }
    fn entry_spans<P: a::Provider>(
        &self,
        h: Header,
        index: u64,
        allocator: &a::Allocator<P>,
    ) -> Result<Spans, Error> {
        let at = self.record(h, index)?;
        let bytes = allocator.bytes(h.metadata.unwrap())?;
        let spans = Spans {
            key: read_span(bytes, at)?,
            value: read_span(bytes, at + 16)?,
        };
        if [spans.key, spans.value]
            .into_iter()
            .flatten()
            .any(|s| !self.layouts.span_valid(s))
        {
            return Err(l::Error::InvalidSpan.into());
        }
        Ok(spans)
    }
    fn links<P: a::Provider>(
        &self,
        h: Header,
        index: u64,
        allocator: &a::Allocator<P>,
    ) -> Result<(u64, u64), Error> {
        if index >= h.len {
            return Err(Error::State);
        }
        let at = self.record(h, index)?;
        let bytes = allocator.bytes(h.metadata.unwrap())?;
        let previous = word(bytes, at + 32)?;
        let next = word(bytes, at + 40)?;
        if previous == index
            || next == index
            || (next == NONE && h.tail != index)
            || (next != NONE && h.tail == index)
        {
            return Err(Error::State);
        }
        if previous != NONE
            && (previous >= h.len || word(bytes, self.record(h, previous)? + 40)? != index)
        {
            return Err(Error::State);
        }
        if next != NONE && (next >= h.len || word(bytes, self.record(h, next)? + 32)? != index) {
            return Err(Error::State);
        }
        Ok((previous, next))
    }
    fn unlink<P: a::Provider>(
        &self,
        h: Header,
        links: (u64, u64),
        allocator: &mut a::Allocator<P>,
    ) -> Result<u64, Error> {
        let (previous, next) = links;
        if previous != NONE {
            let at = self.record(h, previous)?;
            put(allocator.bytes_mut(h.metadata.unwrap())?, at + 40, next);
        }
        if next != NONE {
            let at = self.record(h, next)?;
            put(allocator.bytes_mut(h.metadata.unwrap())?, at + 32, previous);
        }
        Ok(if next == NONE { previous } else { h.tail })
    }
    fn append<P: a::Provider>(
        &self,
        h: Header,
        index: u64,
        tail: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let at = self.record(h, index)?;
        let bytes = allocator.bytes_mut(h.metadata.unwrap())?;
        put(bytes, at + 32, tail);
        put(bytes, at + 40, NONE);
        if tail != NONE {
            let at = self.record(h, tail)?;
            put(allocator.bytes_mut(h.metadata.unwrap())?, at + 40, index);
        }
        Ok(())
    }
    /// Insert or replace into an admitted empty output (possibly an Option
    /// payload). None means a new entry and leaves output untouched. Some means
    /// the old value moved to output; the equal stored key is preserved and the
    /// supplied owning key is detached for common-context cleanup. Copy operands
    /// remain reusable. A value initializer span is required by this adapter.
    pub fn insert_into<P: a::Provider>(
        &mut self,
        key: &mut l::Reservation<'l, 't>,
        value: &mut l::Reservation<'l, 't>,
        spans: Spans,
        destination: &l::Reservation<'l, 't>,
        target: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<Displaced>, Error> {
        let value_span = spans.value.ok_or(l::Error::InvalidSpan)?;
        let Publication::Existing(entry) = self.publish_if_absent(key, value, spans, allocator)?
        else {
            return Ok(None);
        };
        let h = self.check_entry(entry, allocator)?;
        let version = h.version.checked_add(1).ok_or(a::Error::Overflow)?;
        let links = self.links(h, entry.index, allocator)?;
        let at = self.record(h, entry.index)?;
        let old_span = self.entry_spans(h, entry.index, allocator)?.value;
        let mut old = self.element(h, entry.index, Part::Value)?;
        // This transfer validates the complete old shape and destination before
        // mutation. New inputs were validated by publish_if_absent. With no
        // callbacks and exclusive allocator access, the following writes reuse
        // only these admitted regions and cannot encounter a resource failure.
        old.move_root_into(destination, target, allocator)?;
        let to = self.element(h, entry.index, Part::Value)?;
        if self.layouts.types().properties(self.value).unwrap().is_copy {
            to.copy_from(to.root(), value, value.root(), allocator)?;
        } else {
            value.move_root_into(&to, to.root(), allocator)?;
        }
        let tail = self.unlink(h, links, allocator)?;
        self.append(h, entry.index, tail, allocator)?;
        store_span(
            allocator.bytes_mut(h.metadata.unwrap())?,
            at + 16,
            Some(value_span),
        );
        let header = allocator.bytes_mut(self.owner.unwrap())?;
        put(header, 120, entry.index);
        put(header, 160, version);
        let key = detach_key(key, spans.key, allocator)?;
        Ok(Some(Displaced {
            key,
            value_span: old_span,
        }))
    }
    /// Move a found value into admitted empty output, detach its stored owning
    /// key and compact the last physical row. Query ownership is unchanged;
    /// absent removal leaves output and registration/version state untouched.
    pub fn remove_into<P: a::Provider>(
        &mut self,
        key: Key,
        destination: &l::Reservation<'l, 't>,
        target: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<Displaced>, Error> {
        let h = self.inspect(allocator)?;
        if h.active != 0 {
            return Err(Error::BorrowConflict);
        }
        let Some(entry) = self.lookup(key, allocator)? else {
            return Ok(None);
        };
        let version = h.version.checked_add(1).ok_or(a::Error::Overflow)?;
        let index = entry.index;
        let last = h.len - 1;
        let links = self.links(h, index, allocator)?;
        if index != last {
            self.links(h, last, allocator)?;
        }
        let spans = self.entry_spans(h, index, allocator)?;
        let key_span = spans.key;
        let value_span = spans.value;
        let mut stored_key = self.element(h, index, Part::Key)?;
        stored_key.validate_retirement(allocator)?;
        let mut value = self.element(h, index, Part::Value)?;
        value.move_root_into(destination, target, allocator)?;
        // Key/value retirement scratch is shared: finish the value transfer
        // before starting the single Text-key walk.
        let key = detach_key(&mut stored_key, key_span, allocator)?;
        let mut tail = self.unlink(h, links, allocator)?;
        let p = self.plan(h.capacity)?;
        if index != last {
            for (handle, stride) in [
                (h.keys.unwrap(), p.key_stride),
                (h.values.unwrap(), p.value_stride),
                (h.metadata.unwrap(), p.metadata_stride),
            ] {
                allocator.copy_bytes(
                    handle,
                    mul(last, stride)? as usize,
                    handle,
                    mul(index, stride)? as usize,
                    stride as usize,
                )?;
            }
            let at = self.record(h, index)?;
            let bytes = allocator.bytes(h.metadata.unwrap())?;
            let previous = word(bytes, at + 32)?;
            let next = word(bytes, at + 40)?;
            if previous != NONE {
                let at = self.record(h, previous)?;
                put(allocator.bytes_mut(h.metadata.unwrap())?, at + 40, index);
            }
            if next != NONE {
                let at = self.record(h, next)?;
                put(allocator.bytes_mut(h.metadata.unwrap())?, at + 32, index);
            }
            if tail == last {
                tail = index;
            }
        }
        for (handle, stride) in [
            (h.keys.unwrap(), p.key_stride),
            (h.values.unwrap(), p.value_stride),
            (h.metadata.unwrap(), p.metadata_stride),
        ] {
            let start = mul(last, stride)? as usize;
            allocator.bytes_mut(handle)?[start..start + stride as usize].fill(0);
        }
        let bytes = allocator.bytes_mut(self.owner.unwrap())?;
        put(bytes, 32, last);
        put(bytes, 120, tail);
        put(bytes, 160, version);
        Ok(Some(Displaced { key, value_span }))
    }
}
