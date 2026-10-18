use super::*;

impl Reservation<'_, '_> {
    /// Exchange a fully initialized incoming root with an initialized root or
    /// field, without callbacks or allocation. Both obligations remain owned by
    /// the trusted adapter; it must publish the new ledger state before retiring
    /// the old value now held by `incoming`. A field becomes its parent's newest
    /// initializer and retains the replacement span, including through loans.
    pub fn exchange_root_at<P: Provider>(
        &mut self,
        target: Location,
        incoming: &mut Self,
        span: m::Span,
        allocator: &mut Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        let output = self.validate_location(target)?;
        let source = incoming.validate_location(incoming.root())?;
        if !std::ptr::eq(self.layouts.types, incoming.layouts.types)
            || self.layouts.types.equivalent(target.ty, incoming.ty) != Some(true)
            || output.size != source.size
            || output.metadata_bytes != source.metadata_bytes
            || self.data == incoming.data
        {
            return Err(Error::InvalidType(incoming.ty));
        }
        if !self.layouts.span_valid(span) {
            return Err(Error::InvalidSpan);
        }
        let size = output.size as usize;
        let metadata = output.metadata_bytes as usize;
        let offset = target.offset as usize;
        let meta_offset = target.metadata as usize;
        for (handle, start, len) in [
            (self.data, offset, size),
            (incoming.data, 0, size),
            (self.metadata, meta_offset, metadata),
            (incoming.metadata, 0, metadata),
        ] {
            if start.checked_add(len).ok_or(Error::Overflow)? > allocator.bytes(handle)?.len() {
                return Err(Error::State);
            }
        }
        // Preserve all payload/order bytes on validation failure. Only the
        // already reserved temporary walk area is used by this dry run.
        for cell in [&mut *self, &mut *incoming] {
            let result = cell.begin_retirement(None, allocator);
            cell.top = 0;
            if cell.state == State::Draining {
                cell.state = State::Prepared;
            }
            result?;
        }
        let mut parent_order = None;
        if let Some(parent) = target.parent {
            let location = Location {
                owner: self.data,
                ty: parent.ty,
                offset: parent.offset,
                metadata: parent.metadata,
                parent: None,
            };
            if self.child(location, parent.field, allocator)? != target {
                return Err(Error::ForeignLocation);
            }
            let layout = self.validate_location(location)?;
            if layout.work_slots != 0 {
                let (aggregate, _) = Self::aggregate(
                    self.layouts,
                    Work {
                        ty: location.ty,
                        offset: location.offset,
                        metadata: location.metadata,
                        span: None,
                    },
                    allocator,
                    self.data,
                )?;
                let start = parent.metadata as usize;
                let end =
                    start + ORDER_HEADER as usize + aggregate.fields.len() * ORDER_ENTRY as usize;
                let bytes = allocator
                    .bytes(self.metadata)?
                    .get(start..end)
                    .ok_or(Error::State)?;
                let rank = (0..aggregate.fields.len())
                    .find(|rank| {
                        read_u32(bytes, ORDER_HEADER as usize + rank * ORDER_ENTRY as usize)
                            == Ok(parent.field)
                    })
                    .ok_or(Error::InvalidOrder)?;
                let entry = start + ORDER_HEADER as usize + rank * ORDER_ENTRY as usize;
                let old_span = read_span(allocator.bytes(self.metadata)?, entry + 4)?;
                parent_order = Some((entry, end, parent.field, old_span));
            }
        }
        allocator.swap_bytes(self.data, offset, incoming.data, 0, size)?;
        allocator.swap_bytes(self.metadata, meta_offset, incoming.metadata, 0, metadata)?;
        if let Some((entry, end, field, old_span)) = parent_order {
            let row = allocator.bytes_mut(self.metadata)?;
            row.copy_within(entry + ORDER_ENTRY as usize..end, entry);
            let last = end - ORDER_ENTRY as usize;
            row[last..end].fill(0);
            write_u32(row, last, field)?;
            write_span(row, last + 4, Some(span))?;
            write_u32(row, last + 16, 1)?;
            Ok(old_span)
        } else {
            Ok(None)
        }
    }
    /// Move a whole initialized root into admitted, unpublished storage.
    /// The adapter must own the source obligation and an empty destination;
    /// this does not authorize replacement or moving a field out of an owner.
    /// On success the source is fully drained and can be recycled. No provider
    /// callback, token cleanup, allocation or reference-count change occurs.
    pub fn move_root_into<P: Provider>(
        &mut self,
        destination: &Self,
        target: Location,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        self.transfer_into(self.root(), destination, target, allocator)
    }

    /// Consume a whole Option/Result and transfer its sole expected payload.
    /// A wrong case leaves both obligations intact. The execution adapter
    /// supplies the guest-panic policy and atomically updates its owner ledger.
    pub fn extract_case_into<P: Provider>(
        &mut self,
        expected: u32,
        destination: &Self,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        let o::Type::Constructed(id) = self.ty else {
            return Err(Error::InvalidType(self.ty));
        };
        match self.layouts.types.table().constructed[id.0 as usize].constructor {
            o::Constructor::Option(_) if expected == 1 => {}
            o::Constructor::Result(..) if expected < 2 => {}
            _ => return Err(Error::InvalidType(self.ty)),
        }
        let root = self.root();
        let actual = self.case(root, allocator)?;
        if actual != expected {
            return Err(Error::InvalidCase(actual));
        }
        let payload = self.child(root, 0, allocator)?;
        self.transfer_into(payload, destination, destination.root(), allocator)
    }

    fn transfer_into<P: Provider>(
        &mut self,
        from: Location,
        destination: &Self,
        target: Location,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        let source = self.validate_location(from)?;
        let output = destination.validate_location(target)?;
        if !std::ptr::eq(self.layouts.types, destination.layouts.types)
            || self.layouts.types.equivalent(from.ty, target.ty) != Some(true)
            || source.size != output.size
            || source.metadata_bytes != output.metadata_bytes
            || self.data == destination.data
        {
            return Err(Error::InvalidType(from.ty));
        }
        let size = source.size;
        let metadata = source.metadata_bytes;
        for (handle, offset, size) in [
            (self.data, from.offset, size),
            (destination.data, target.offset, size),
            (self.metadata, from.metadata, metadata),
            (destination.metadata, target.metadata, metadata),
        ] {
            let end = usize::try_from(add(offset, size)?).map_err(|_| Error::Overflow)?;
            if end > allocator.bytes(handle)?.len() {
                return Err(Error::State);
            }
        }
        // Dry-run the entire active source shape before publishing any bytes.
        // This validates nested tags/order flags using its admitted work area.
        self.begin_retirement(None, allocator)?;
        allocator.copy_bytes(
            self.data,
            from.offset as usize,
            destination.data,
            target.offset as usize,
            size as usize,
        )?;
        allocator.copy_bytes(
            self.metadata,
            from.metadata as usize,
            destination.metadata,
            target.metadata as usize,
            metadata as usize,
        )?;
        // With exclusive allocator access and all ranges admitted above, these
        // writes cannot invoke a provider or encounter a new resource failure.
        // Copy only construction metadata, never the source retirement stack.
        self.clear(allocator)?;
        self.top = 0;
        Ok(())
    }
}
