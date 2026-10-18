use super::*;

impl<'l, 't> Heap<'l, 't> {
    pub fn custody<P: a::Provider>(
        &self,
        read: Read,
        ty: o::Type,
        allocator: &a::Allocator<P>,
    ) -> Result<Custody, Error> {
        self.resolve(read.0, READ, ty, allocator)?;
        Ok(Custody { read, ty })
    }
    fn observed<P: a::Provider>(
        &self,
        token: &l::Token,
        custody: Option<Custody>,
        allocator: &a::Allocator<P>,
    ) -> Result<(usize, u64, o::Type), Error> {
        let customer = if let Some(custody) = custody {
            self.resolve(custody.read.0, READ, custody.ty, allocator)? as u64 + 1
        } else {
            0
        };
        let o::Type::Constructed(id) = token.leaf.ty else {
            return Err(unavailable());
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
            _ => return Err(unavailable()),
        };
        let index = Registry::new(self.owner, &self.nodes, self.layouts, self.limits).resolve(
            Reference::parse(token.bytes)?,
            kind,
            ty,
            customer,
            allocator,
        )?;
        Ok((index, kind, ty))
    }
    pub fn read_observed<P: a::Provider>(
        &mut self,
        token: &l::Token,
        custody: Option<Custody>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Read, Error> {
        let (index, kind, _) = self.observed(token, custody, allocator)?;
        if kind != STRONG {
            return Err(unavailable());
        }
        self.read_index(index, allocator)
    }
    pub fn replace_observed<P: a::Provider>(
        &mut self,
        token: &l::Token,
        custody: Option<Custody>,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        let (index, kind, _) = self
            .observed(token, custody, allocator)
            .map_err(|e| e.at(Some(span)))?;
        if kind != STRONG {
            return Err(unavailable().at(Some(span)));
        }
        self.replace_index(index, input, span, allocator)
    }
    pub fn replace_observed_copy<P: a::Provider>(
        &mut self,
        token: &l::Token,
        custody: Option<Custody>,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let (index, kind, _) = self
            .observed(token, custody, allocator)
            .map_err(|e| e.at(Some(span)))?;
        if kind != STRONG {
            return Err(unavailable().at(Some(span)));
        }
        self.replace_copy_index(index, input, span, allocator)
    }
    pub fn retain_observed<P: a::Provider>(
        &mut self,
        token: &l::Token,
        custody: Option<Custody>,
        action: FieldAction,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Retained, Error> {
        let (index, kind, _) = self.observed(token, custody, allocator)?;
        if !matches!(
            (kind, action),
            (STRONG, FieldAction::CloneStrong | FieldAction::NewWeak)
                | (WEAK, FieldAction::CloneWeak | FieldAction::UpgradeWeak)
        ) {
            return Err(unavailable());
        }
        if action == FieldAction::UpgradeWeak
            && self.nodes[index]
                .as_ref()
                .unwrap()
                .block
                .counts(allocator)?
                .strong
                == 0
        {
            return Err(core(l::OwnershipError::ExpiredWeak));
        }
        let reference = self.vacancy(allocator, &mut self.work())?;
        let block = &mut self.nodes[index].as_mut().unwrap().block;
        let kind = match action {
            FieldAction::CloneStrong => {
                block.retain_strong(self.limits.reference_count, allocator)?;
                STRONG
            }
            FieldAction::NewWeak => {
                block.downgrade(self.limits.reference_count, allocator)?;
                WEAK
            }
            FieldAction::CloneWeak => {
                block.retain_weak(self.limits.reference_count, allocator)?;
                WEAK
            }
            FieldAction::UpgradeWeak => {
                block.upgrade(self.limits.reference_count, allocator)?;
                STRONG
            }
        };
        self.register(reference, index, kind, allocator)?;
        Ok(if kind == STRONG {
            Retained::Strong(Strong(reference))
        } else {
            Retained::Weak(Weak(reference))
        })
    }
    fn shared_field<P: a::Provider>(
        &self,
        read: Read,
        owner_type: o::Type,
        location: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<usize, Error> {
        let owner = self.resolve(read.0, READ, owner_type, allocator)?;
        let token = self.nodes[owner]
            .as_ref()
            .unwrap()
            .block
            .payload(allocator)?
            .read_token(location, allocator)?;
        let o::Type::Constructed(id) = token.leaf.ty else {
            return Err(l::Error::InvalidType(token.leaf.ty).into());
        };
        let o::Constructor::Shared(ty) =
            self.layouts.types().table().constructed[id.0 as usize].constructor
        else {
            return Err(l::Error::InvalidType(token.leaf.ty).into());
        };
        Registry::new(self.owner, &self.nodes, self.layouts, self.limits).resolve(
            Reference::parse(token.bytes)?,
            STRONG,
            ty,
            owner as u64 + 1,
            allocator,
        )
    }
    pub fn read_field<P: a::Provider>(
        &mut self,
        read: Read,
        owner_type: o::Type,
        location: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Read, Error> {
        let index = self.shared_field(read, owner_type, location, allocator)?;
        self.read_index(index, allocator)
    }
    pub fn replace_field_from<P: a::Provider>(
        &mut self,
        read: Read,
        owner_type: o::Type,
        location: l::Location,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        let index = self
            .shared_field(read, owner_type, location, allocator)
            .map_err(|e| e.at(Some(span)))?;
        self.replace_index(index, input, span, allocator)
    }
    pub fn replace_field_copy<P: a::Provider>(
        &mut self,
        read: Read,
        owner_type: o::Type,
        location: l::Location,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let index = self
            .shared_field(read, owner_type, location, allocator)
            .map_err(|e| e.at(Some(span)))?;
        self.replace_copy_index(index, input, span, allocator)
    }
    /// Retain an owner borrowed through a live shared read. The structural
    /// location must belong to that read's node and the stored ticket must name
    /// that same custodian; stale/foreign locations grant no authority.
    pub fn retain_field<P: a::Provider>(
        &mut self,
        read: Read,
        owner_type: o::Type,
        location: l::Location,
        action: FieldAction,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Retained, Error> {
        let owner = self.resolve(read.0, READ, owner_type, allocator)?;
        let token = self.nodes[owner]
            .as_ref()
            .unwrap()
            .block
            .payload(allocator)?
            .read_token(location, allocator)?;
        let o::Type::Constructed(id) = token.leaf.ty else {
            return Err(l::Error::InvalidType(token.leaf.ty).into());
        };
        let (kind, ty) = match (
            self.layouts.types().table().constructed[id.0 as usize].constructor,
            action,
        ) {
            (o::Constructor::Shared(ty), FieldAction::CloneStrong | FieldAction::NewWeak) => {
                (STRONG, ty)
            }
            (o::Constructor::Weak(ty), FieldAction::CloneWeak | FieldAction::UpgradeWeak) => {
                (WEAK, ty)
            }
            _ => return Err(l::Error::InvalidType(token.leaf.ty).into()),
        };
        let index = Registry::new(self.owner, &self.nodes, self.layouts, self.limits).resolve(
            Reference::parse(token.bytes)?,
            kind,
            ty,
            owner as u64 + 1,
            allocator,
        )?;
        if action == FieldAction::UpgradeWeak
            && self.nodes[index]
                .as_ref()
                .unwrap()
                .block
                .counts(allocator)?
                .strong
                == 0
        {
            return Err(core(l::OwnershipError::ExpiredWeak));
        }
        let reference = self.vacancy(allocator, &mut self.work())?;
        let block = &mut self.nodes[index].as_mut().unwrap().block;
        let retained = match action {
            FieldAction::CloneStrong => {
                block.retain_strong(self.limits.reference_count, allocator)?;
                STRONG
            }
            FieldAction::NewWeak => {
                block.downgrade(self.limits.reference_count, allocator)?;
                WEAK
            }
            FieldAction::CloneWeak => {
                block.retain_weak(self.limits.reference_count, allocator)?;
                WEAK
            }
            FieldAction::UpgradeWeak => {
                block.upgrade(self.limits.reference_count, allocator)?;
                STRONG
            }
        };
        self.register(reference, index, retained, allocator)?;
        Ok(if retained == STRONG {
            Retained::Strong(Strong(reference))
        } else {
            Retained::Weak(Weak(reference))
        })
    }
}
