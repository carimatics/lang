use super::*;

impl Reservation<'_, '_> {
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
        allocator.bytes_mut(self.data)?.fill(0);
        allocator.bytes_mut(self.metadata)?.fill(0);
        self.top = 0;
        Ok(())
    }
}
