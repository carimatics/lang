use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(loan: &Loan) -> Loan {
        Loan {
            owner: loan.owner,
            ty: loan.ty,
            slot: loan.slot,
            serial: loan.serial,
            index: loan.index,
            kind: loan.kind,
            active: loan.active,
        }
    }
    fn drain(vector: &mut Vector<'_, '_>, allocator: &mut a::Allocator<a::System>) {
        let mut instance = c::Instance::default();
        let mut frame = instance
            .enter_with_context(
                c::Limits {
                    slots: 1,
                    ..Default::default()
                },
                |_: &mut (), _, ctx: &mut c::Context<'_>| {
                    vector
                        .cleanup(allocator, ctx, |_, _, _, _| panic!("Copy leaf"))
                        .map_err(|_| 99)
                },
            )
            .unwrap();
        frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
        frame.initialize(c::Slot(0), (), None).unwrap();
        assert!(
            frame
                .finish(c::Exit::Return(None))
                .report
                .primary()
                .is_none()
        );
    }
    #[test]
    fn forged_stale_wrong_kind_and_wrong_slot_leases_do_not_consume_a_reused_registration() {
        let types = o::TypeTable::default().verify(Default::default()).unwrap();
        let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
        let mut a = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut vector =
            Vector::new_with_limits(&layouts, o::Type::I64, Limits { loans: 1 }, &mut a).unwrap();
        let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
        vector.push_copy(&mut value, None, &mut a).unwrap();
        let mut first = vector.borrow(0, o::LoanKind::Shared, &mut a).unwrap();
        let mut stale = snapshot(&first);
        first.end(&layouts, &mut a).unwrap();
        let mut live = vector.borrow(0, o::LoanKind::Shared, &mut a).unwrap();
        let before = a.stats();
        assert_eq!(stale.end(&layouts, &mut a), Err(Error::Unavailable));
        assert!(stale.view(&layouts, &a).is_err());
        for which in 0..3 {
            let mut fake = snapshot(&live);
            match which {
                0 => fake.kind = o::LoanKind::Exclusive,
                1 => fake.slot = u64::MAX,
                _ => fake.serial = 0,
            };
            assert_eq!(fake.end(&layouts, &mut a), Err(Error::Unavailable));
        }
        assert_eq!(a.stats(), before);
        assert!(live.view(&layouts, &a).is_ok());
        live.end(&layouts, &mut a).unwrap();
        value.release_storage(&mut a).unwrap();
        drain(&mut vector, &mut a);
        assert_eq!(a.stats().live_allocations, 0);
    }
    #[test]
    fn lease_serial_exhaustion_never_wraps_or_invalidates_the_last_live_registration() {
        let types = o::TypeTable::default().verify(Default::default()).unwrap();
        let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
        let mut a = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut vector =
            Vector::new_with_limits(&layouts, o::Type::I64, Limits { loans: 2 }, &mut a).unwrap();
        let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
        vector.push_copy(&mut value, None, &mut a).unwrap();
        put(a.bytes_mut(vector.header.unwrap()).unwrap(), 112, u64::MAX);
        let mut last = vector.borrow(0, o::LoanKind::Shared, &mut a).unwrap();
        assert_eq!(last.serial, u64::MAX);
        let before = a.stats();
        assert!(matches!(
            vector.borrow(0, o::LoanKind::Shared, &mut a),
            Err(Error::Allocation(a::Error::Overflow))
        ));
        assert_eq!(a.stats(), before);
        assert!(last.view(&layouts, &a).is_ok());
        last.end(&layouts, &mut a).unwrap();
        assert!(matches!(
            vector.borrow(0, o::LoanKind::Shared, &mut a),
            Err(Error::Allocation(a::Error::Overflow))
        ));
        value.release_storage(&mut a).unwrap();
        drain(&mut vector, &mut a);
        assert_eq!(a.stats().live_allocations, 0);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub loans: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self { loans: 256 }
    }
}
/// One registered lease, ended explicitly. Ordinary wrapper disposal neither
/// unregisters nor releases the vector. Abort invalidates abandoned leases.
pub struct Loan {
    owner: a::Handle,
    ty: o::Type,
    slot: u64,
    serial: u64,
    index: u64,
    kind: o::LoanKind,
    active: bool,
}
/// Every access revalidates its lease, including after host abort. Locations
/// remain structural offsets and never authorize another element or mutation.
pub struct Borrowed<'a, 'l, 't> {
    loan: &'a Loan,
    value: l::Reservation<'l, 't>,
}
impl Loan {
    /// Trusted buffer-growth adapter: publish the validated replacement token
    /// for the same owning leaf, without creating another cleanup obligation.
    pub fn write_buffer_token<P: a::Provider>(
        &self,
        layouts: &l::Layouts<'_>,
        at: l::Location,
        mut token: [u8; 32],
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let vector = self.vector(layouts);
        let h = vector.check_loan(self, allocator)?;
        if self.kind != o::LoanKind::Exclusive {
            return Err(Error::BorrowConflict);
        }
        let value = vector.element(h, self.index)?;
        value.read_token(at, allocator)?;
        let o::Type::Constructed(id) = at.ty() else {
            return Err(l::Error::InvalidType(at.ty()).into());
        };
        let encoded = token;
        match layouts.types().table().constructed[id.0 as usize].constructor {
            o::Constructor::Builtin(o::Builtin::Bytes) => {
                crate::buffer::Bytes::from_token(&mut token, allocator)
                    .map_err(|_| Error::State)?;
            }
            o::Constructor::Builtin(o::Builtin::Text) => {
                crate::buffer::Text::from_token(&mut token, allocator).map_err(|_| Error::State)?;
            }
            _ => return Err(l::Error::InvalidType(at.ty()).into()),
        }
        let start = at.offset() as usize;
        value.data_mut(allocator)?[start..start + 32].copy_from_slice(&encoded);
        Ok(())
    }
    /// Mutate through the registered exclusive lease without recreating an
    /// owning vector token. The displaced obligation remains in `input`.
    pub fn replace_from<'l, 't, P: a::Provider>(
        &self,
        layouts: &'l l::Layouts<'t>,
        target: l::Location,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        self.vector(layouts)
            .replace_from(self, target, input, span, allocator)
    }
    pub fn replace_copy<'l, 't, P: a::Provider>(
        &self,
        layouts: &'l l::Layouts<'t>,
        target: l::Location,
        input: &l::Reservation<'l, 't>,
        from: l::Location,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.vector(layouts)
            .replace_copy(self, target, input, from, span, allocator)
    }
    fn vector<'l, 't>(&self, layouts: &'l l::Layouts<'t>) -> Vector<'l, 't> {
        Vector {
            layouts,
            ty: self.ty,
            header: Some(self.owner),
        }
    }
    pub fn view<'a, 'l, 't, P: a::Provider>(
        &'a self,
        layouts: &'l l::Layouts<'t>,
        allocator: &a::Allocator<P>,
    ) -> Result<Borrowed<'a, 'l, 't>, Error> {
        let vector = self.vector(layouts);
        let h = vector.check_loan(self, allocator)?;
        Ok(Borrowed {
            loan: self,
            value: vector.element(h, self.index)?,
        })
    }
    pub fn end<P: a::Provider>(
        &mut self,
        layouts: &l::Layouts<'_>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let vector = self.vector(layouts);
        let h = vector.check_loan(self, allocator)?;
        let owner = self.owner;
        let at = (HEADER + self.slot * LOAN_ENTRY) as usize;
        self.active = false;
        let bytes = allocator.bytes_mut(owner)?;
        bytes[at..at + LOAN_ENTRY as usize].fill(0);
        put(bytes, 96, h.active - 1);
        if self.kind == o::LoanKind::Exclusive {
            put(bytes, 104, 0);
        }
        Ok(())
    }
}
impl Borrowed<'_, '_, '_> {
    fn check<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<(), Error> {
        self.loan
            .vector(self.value.layouts())
            .check_loan(self.loan, allocator)?;
        Ok(())
    }
    pub fn root(&self) -> l::Location {
        self.value.root()
    }
    pub fn child<P: a::Provider>(
        &self,
        at: l::Location,
        index: u32,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Location, Error> {
        self.check(allocator)?;
        Ok(self.value.child(at, index, allocator)?)
    }
    pub fn case<P: a::Provider>(
        &self,
        at: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<u32, Error> {
        self.check(allocator)?;
        Ok(self.value.case(at, allocator)?)
    }
    pub fn read_immediate<P: a::Provider>(
        &self,
        at: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Immediate, Error> {
        self.check(allocator)?;
        Ok(self.value.read_immediate(at, allocator)?)
    }
    /// Borrow-only bytes: the element keeps its cleanup obligation.
    pub fn read_token<P: a::Provider>(
        &self,
        at: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Token, Error> {
        self.check(allocator)?;
        Ok(self.value.read_token(at, allocator)?)
    }
    pub fn copy_into<P: a::Provider>(
        &self,
        from: l::Location,
        destination: &l::Reservation<'_, '_>,
        target: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.check(allocator)?;
        Ok(destination.copy_from(target, &self.value, from, allocator)?)
    }
}
impl<'l, 't> Vector<'l, 't> {
    fn index(&self, h: Header, index: i64) -> Result<u64, Error> {
        u64::try_from(index)
            .ok()
            .filter(|i| *i < h.len)
            .ok_or(Error::Bounds(crate::buffer::BoundsError::OutOfBounds))
    }
    /// Reserve no storage: all lease rows were included in the constructor.
    pub fn borrow<P: a::Provider>(
        &mut self,
        index: i64,
        kind: o::LoanKind,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Loan, Error> {
        let h = self.inspect(allocator)?;
        let index = self.index(h, index)?;
        if h.exclusive != 0 || (kind == o::LoanKind::Exclusive && h.active != 0) {
            return Err(Error::BorrowConflict);
        }
        if h.serial == 0 {
            return Err(a::Error::Overflow.into());
        }
        let owner = self.header.unwrap();
        let bytes = allocator.bytes(owner)?;
        let slot = (0..h.loans)
            .find(|slot| word(bytes, (HEADER + slot * LOAN_ENTRY) as usize) == Ok(0))
            .ok_or(Error::LoanLimit)?;
        let bytes = allocator.bytes_mut(owner)?;
        let at = (HEADER + slot * LOAN_ENTRY) as usize;
        put(bytes, at, h.serial);
        put(bytes, at + 8, index);
        put(
            bytes,
            at + 16,
            if kind == o::LoanKind::Shared { 1 } else { 2 },
        );
        put(bytes, 112, h.serial.checked_add(1).unwrap_or(0));
        put(bytes, 96, h.active + 1);
        if kind == o::LoanKind::Exclusive {
            put(bytes, 104, h.serial);
        }
        Ok(Loan {
            owner,
            ty: self.ty,
            slot,
            serial: h.serial,
            index,
            kind,
            active: true,
        })
    }
    fn check_loan<P: a::Provider>(
        &self,
        loan: &Loan,
        allocator: &a::Allocator<P>,
    ) -> Result<Header, Error> {
        let h = self.inspect(allocator)?;
        if !loan.active
            || Some(loan.owner) != self.header
            || loan.ty != self.ty
            || loan.slot >= h.loans
            || loan.index >= h.len
        {
            return Err(Error::Unavailable);
        }
        let bytes = allocator.bytes(loan.owner)?;
        let at = (HEADER + loan.slot * LOAN_ENTRY) as usize;
        if word(bytes, at)? != loan.serial
            || word(bytes, at + 8)? != loan.index
            || word(bytes, at + 16)?
                != if loan.kind == o::LoanKind::Shared {
                    1
                } else {
                    2
                }
            || h.active == 0
            || (loan.kind == o::LoanKind::Exclusive && h.exclusive != loan.serial)
            || (loan.kind == o::LoanKind::Shared && h.exclusive != 0)
        {
            return Err(Error::Unavailable);
        }
        Ok(h)
    }
    fn order_at(&self, h: Header, index: u64) -> Result<usize, Error> {
        if index >= h.capacity {
            return Err(Error::State);
        }
        let p = plan(self.layout(), h.capacity)?;
        Ok((mul(index, p.metadata_stride)? + self.layout().metadata_bytes) as usize)
    }
    pub(super) fn check_tail<P: a::Provider>(
        &self,
        h: Header,
        allocator: &a::Allocator<P>,
    ) -> Result<(), Error> {
        if h.tail != NONE {
            let at = self.order_at(h, h.tail)?;
            if word(allocator.bytes(h.metadata.unwrap())?, at + 24)? != NONE {
                return Err(Error::State);
            }
        }
        Ok(())
    }
    pub(super) fn append_order<P: a::Provider>(
        &self,
        h: Header,
        index: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let at = self.order_at(h, index)?;
        let bytes = allocator.bytes_mut(h.metadata.unwrap())?;
        put(bytes, at + 16, h.tail);
        put(bytes, at + 24, NONE);
        if h.tail != NONE {
            let old = self.order_at(h, h.tail)?;
            put(allocator.bytes_mut(h.metadata.unwrap())?, old + 24, index);
        }
        put(allocator.bytes_mut(self.header.unwrap())?, 120, index);
        Ok(())
    }
    fn links<P: a::Provider>(
        &self,
        h: Header,
        index: u64,
        allocator: &a::Allocator<P>,
    ) -> Result<(u64, u64), Error> {
        let at = self.order_at(h, index)?;
        let bytes = allocator.bytes(h.metadata.unwrap())?;
        let previous = word(bytes, at + 16)?;
        let next = word(bytes, at + 24)?;
        if previous == index
            || next == index
            || (next == NONE && h.tail != index)
            || (next != NONE && h.tail == index)
        {
            return Err(Error::State);
        }
        if previous != NONE
            && (previous >= h.len || word(bytes, self.order_at(h, previous)? + 24)? != index)
        {
            return Err(Error::State);
        }
        if next != NONE && (next >= h.len || word(bytes, self.order_at(h, next)? + 16)? != index) {
            return Err(Error::State);
        }
        Ok((previous, next))
    }
    fn unlink<P: a::Provider>(
        &self,
        h: Header,
        index: u64,
        links: (u64, u64),
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let (previous, next) = links;
        if previous != NONE {
            let at = self.order_at(h, previous)?;
            put(allocator.bytes_mut(h.metadata.unwrap())?, at + 24, next);
        }
        if next != NONE {
            let at = self.order_at(h, next)?;
            put(allocator.bytes_mut(h.metadata.unwrap())?, at + 16, previous);
        } else {
            put(allocator.bytes_mut(self.header.unwrap())?, 120, previous);
        }
        let at = self.order_at(h, index)?;
        allocator.bytes_mut(h.metadata.unwrap())?[at..at + 32].fill(0);
        Ok(())
    }
    /// Move the last logical element into an admitted empty destination, which
    /// may be an Option payload. Empty leaves the destination untouched.
    pub fn pop_into<P: a::Provider>(
        &mut self,
        destination: &l::Reservation<'l, 't>,
        target: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<bool, Error> {
        let h = self.inspect(allocator)?;
        if h.active != 0 {
            return Err(Error::BorrowConflict);
        }
        if h.len == 0 {
            return Ok(false);
        }
        let index = h.len - 1;
        let links = self.links(h, index, allocator)?;
        let mut source = self.element(h, index)?;
        source.move_root_into(destination, target, allocator)?;
        self.unlink(h, index, links, allocator)?;
        put(allocator.bytes_mut(self.header.unwrap())?, 16, index);
        Ok(true)
    }
    fn span<P: a::Provider>(
        &self,
        h: Header,
        index: u64,
        allocator: &a::Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        let at = self.order_at(h, index)?;
        let bytes = &allocator.bytes(h.metadata.unwrap())?[at..at + 16];
        let get = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        match get(12) {
            0 => Ok(None),
            1 => {
                let span = m::Span {
                    source: m::SourceId(get(0)),
                    start: get(4),
                    end: get(8),
                };
                if !self.layouts.span_valid(span) {
                    return Err(l::Error::InvalidSpan.into());
                }
                Ok(Some(span))
            }
            _ => Err(Error::State),
        }
    }
    fn root_replaced<P: a::Provider>(
        &self,
        h: Header,
        index: u64,
        links: (u64, u64),
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let tail = if h.tail == index { links.0 } else { h.tail };
        self.unlink(h, index, links, allocator)?;
        self.append_order(Header { tail, ..h }, index, allocator)?;
        let at = self.order_at(h, index)?;
        let bytes = &mut allocator.bytes_mut(h.metadata.unwrap())?[at..at + 16];
        bytes[..4].copy_from_slice(&span.source.0.to_le_bytes());
        bytes[4..8].copy_from_slice(&span.start.to_le_bytes());
        bytes[8..12].copy_from_slice(&span.end.to_le_bytes());
        bytes[12..].copy_from_slice(&1u32.to_le_bytes());
        Ok(())
    }
    /// Publish first, returning the displaced value in `input`. The adapter
    /// retires it afterwards with the returned original initializer span.
    pub fn replace_from<P: a::Provider>(
        &mut self,
        loan: &Loan,
        target: l::Location,
        input: &mut l::Reservation<'l, 't>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<m::Span>, Error> {
        let h = self.check_loan(loan, allocator)?;
        if loan.kind != o::LoanKind::Exclusive {
            return Err(Error::BorrowConflict);
        }
        let links = self.links(h, loan.index, allocator)?;
        let old = self.span(h, loan.index, allocator)?;
        let mut value = self.element(h, loan.index)?;
        let root = target == value.root();
        let replaced = value.exchange_root_at(target, input, span, allocator)?;
        if root {
            self.root_replaced(h, loan.index, links, span, allocator)?;
            Ok(old)
        } else {
            Ok(replaced.or(old))
        }
    }
    pub fn replace_copy<P: a::Provider>(
        &mut self,
        loan: &Loan,
        target: l::Location,
        input: &l::Reservation<'l, 't>,
        from: l::Location,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let h = self.check_loan(loan, allocator)?;
        if loan.kind != o::LoanKind::Exclusive {
            return Err(Error::BorrowConflict);
        }
        if !self.layouts.span_valid(span) {
            return Err(l::Error::InvalidSpan.into());
        }
        let links = self.links(h, loan.index, allocator)?;
        let value = self.element(h, loan.index)?;
        value.copy_from(target, input, from, allocator)?;
        if target == value.root() {
            self.root_replaced(h, loan.index, links, span, allocator)?;
        }
        Ok(())
    }
    /// Host teardown can revoke forgotten trusted-host leases before draining.
    pub fn abort_cleanup<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        self.inspect(allocator)?;
        let bytes = allocator.bytes_mut(self.header.unwrap())?;
        put(bytes, 96, 0);
        put(bytes, 104, 0);
        bytes[HEADER as usize..].fill(0);
        self.cleanup(allocator, context, cleaner)
    }
}
