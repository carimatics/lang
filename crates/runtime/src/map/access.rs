use super::*;

/// One read registration, ended explicitly. Disposing this wrapper does not
/// unregister it. Host abort invalidates abandoned registrations.
pub struct Loan {
    service_work: Option<crate::execution::ServiceWork>,
    entry: Entry,
    key: o::Type,
    value: o::Type,
    slot: u64,
    serial: u64,
    active: bool,
}

/// A structural projection with a checked registration on every access.
pub struct Borrowed<'a, 'l, 't> {
    loan: &'a Loan,
    value: l::Reservation<'l, 't>,
}

impl Loan {
    fn map<'l, 't>(&self, layouts: &'l l::Layouts<'t>) -> Map<'l, 't> {
        Map {
            layouts,
            key: self.key,
            value: self.value,
            owner: Some(self.entry.owner),
            service_work: self.service_work.clone(),
        }
    }
    pub fn view<'a, 'l, 't, P: a::Provider>(
        &'a self,
        layouts: &'l l::Layouts<'t>,
        allocator: &a::Allocator<P>,
    ) -> Result<Borrowed<'a, 'l, 't>, Error> {
        let map = self.map(layouts);
        let h = map.check_loan(self, allocator)?;
        Ok(Borrowed {
            loan: self,
            value: map.element(h, self.entry.index, Part::Value)?,
        })
    }
    pub fn end<P: a::Provider>(
        &mut self,
        layouts: &l::Layouts<'_>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let h = self.map(layouts).check_loan(self, allocator)?;
        let at = (HEADER + self.slot * LOAN_ENTRY) as usize;
        let bytes = allocator.bytes_mut(self.entry.owner)?;
        bytes[at..at + LOAN_ENTRY as usize].fill(0);
        put(bytes, 136, h.active - 1);
        self.active = false;
        Ok(())
    }
}

impl Borrowed<'_, '_, '_> {
    fn check<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<(), Error> {
        self.loan
            .map(self.value.layouts())
            .check_loan(self.loan, allocator)?;
        Ok(())
    }
    pub fn root(&self) -> l::Location {
        self.value.root()
    }
    pub fn child<P: a::Provider>(
        &self,
        at: l::Location,
        field: u32,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Location, Error> {
        self.check(allocator)?;
        Ok(self.value.child(at, field, allocator)?)
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
        Ok(self.value.read_immediate_with_service_work(
            at,
            allocator,
            self.loan.service_work.as_ref(),
        )?)
    }
    /// The element retains the cleanup obligation for the observed token.
    pub fn read_token<P: a::Provider>(
        &self,
        at: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Token, Error> {
        self.check(allocator)?;
        Ok(self.value.read_token_with_service_work(
            at,
            allocator,
            self.loan.service_work.as_ref(),
        )?)
    }
    pub fn copy_into<P: a::Provider>(
        &self,
        from: l::Location,
        destination: &l::Reservation<'_, '_>,
        target: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.check(allocator)?;
        Ok(destination.copy_from_with_service_work(
            target,
            &self.value,
            from,
            allocator,
            self.loan.service_work.as_ref(),
        )?)
    }
}

impl<'l, 't> Map<'l, 't> {
    /// Missing keys create no registration. All rows were admitted by new().
    pub fn borrow<P: a::Provider>(
        &mut self,
        key: Key,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<Loan>, Error> {
        let Some(entry) = self.lookup(key, allocator)? else {
            return Ok(None);
        };
        let h = self.check_entry(entry, allocator)?;
        if h.serial == 0 {
            return Err(a::Error::Overflow.into());
        }
        let bytes = allocator.bytes(entry.owner)?;
        let slot = (0..h.loans)
            .find(|slot| word(bytes, (HEADER + slot * LOAN_ENTRY) as usize) == Ok(0))
            .ok_or(Error::LoanLimit)?;
        let at = (HEADER + slot * LOAN_ENTRY) as usize;
        let bytes = allocator.bytes_mut(entry.owner)?;
        put(bytes, at, h.serial);
        put(bytes, at + 8, entry.index);
        put(bytes, 136, h.active + 1);
        put(bytes, 144, h.serial.checked_add(1).unwrap_or(0));
        Ok(Some(Loan {
            entry,
            key: self.key,
            value: self.value,
            slot,
            serial: h.serial,
            active: true,
            service_work: self.service_work.clone(),
        }))
    }
    fn check_loan<P: a::Provider>(
        &self,
        loan: &Loan,
        allocator: &a::Allocator<P>,
    ) -> Result<Header, Error> {
        let h = self.check_entry(loan.entry, allocator)?;
        if !loan.active
            || loan.serial == 0
            || loan.key != self.key
            || loan.value != self.value
            || loan.slot >= h.loans
            || h.active == 0
        {
            return Err(Error::Unavailable);
        }
        let at = (HEADER + loan.slot * LOAN_ENTRY) as usize;
        let bytes = allocator.bytes(loan.entry.owner)?;
        if word(bytes, at)? != loan.serial || word(bytes, at + 8)? != loan.entry.index {
            return Err(Error::Unavailable);
        }
        Ok(h)
    }
    /// Host abort revokes every read before draining its owner. Old views must
    /// validate the released allocator generation even after locator reuse.
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
        let bytes = allocator.bytes_mut(self.owner.unwrap())?;
        bytes[HEADER as usize..].fill(0);
        put(bytes, 136, 0);
        self.cleanup(allocator, context, cleaner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(loan: &Loan) -> Loan {
        Loan {
            entry: loan.entry,
            key: loan.key,
            value: loan.value,
            slot: loan.slot,
            serial: loan.serial,
            active: loan.active,
            service_work: loan.service_work.clone(),
        }
    }
    #[test]
    fn stale_or_forged_lease_cannot_consume_a_reused_registration() {
        let types = o::TypeTable::default().verify(Default::default()).unwrap();
        let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
        let mut a = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut map = Map::new_with_limits(
            &layouts,
            o::Type::I64,
            o::Type::I64,
            Limits {
                loans: 1,
                ..Default::default()
            },
            &mut a,
        )
        .unwrap();
        let mut k = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
        let mut v = l::Reservation::allocate(&layouts, o::Type::I64, &mut a).unwrap();
        map.publish_if_absent(&mut k, &mut v, Default::default(), &mut a)
            .unwrap();
        let mut first = map.borrow(Key::I64(0), &mut a).unwrap().unwrap();
        let mut stale = snapshot(&first);
        first.end(&layouts, &mut a).unwrap();
        let mut live = map.borrow(Key::I64(0), &mut a).unwrap().unwrap();
        let before = a.stats();
        assert_eq!(stale.end(&layouts, &mut a), Err(Error::Unavailable));
        assert!(stale.view(&layouts, &a).is_err());
        for which in 0..6 {
            let mut fake = snapshot(&live);
            match which {
                0 => fake.serial = 0,
                1 => fake.slot = u64::MAX,
                2 => fake.entry.version += 1,
                3 => fake.entry.index += 1,
                4 => fake.key = o::Type::Bool,
                _ => fake.active = false,
            };
            assert!(fake.end(&layouts, &mut a).is_err());
        }
        assert_eq!(a.stats(), before);
        assert!(live.view(&layouts, &a).is_ok());
        live.end(&layouts, &mut a).unwrap();
        k.release_storage(&mut a).unwrap();
        v.release_storage(&mut a).unwrap();
        super::super::tests::drain(&mut map, &mut a);
        assert_eq!(a.stats().live_allocations, 0);
    }
}
