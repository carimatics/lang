use super::*;

#[derive(Clone, Copy)]
enum DataWork {
    Framing,
    Payload,
    Utf8Payload,
}
impl DataWork {
    fn charge(self, session: &mut Session, n: usize, at: usize) -> Result<(), Error> {
        match self {
            Self::Framing => session.charge(n, at),
            Self::Payload => session.value_work(n, at),
            Self::Utf8Payload => session.value_work(Session::add(n, n, at)?, at),
        }
    }
}

pub(super) struct Writer<'a> {
    pub session: &'a mut Session,
    pub bytes: Vec<u8>,
    length: usize,
    measuring: bool,
}
impl<'a> Writer<'a> {
    pub fn new(session: &'a mut Session) -> Self {
        Self {
            session,
            bytes: Vec::new(),
            length: 0,
            measuring: false,
        }
    }
    pub fn measuring(session: &'a mut Session) -> Self {
        let mut value = Self::new(session);
        value.measuring = true;
        value
    }
    pub fn at(&self) -> usize {
        self.length
    }
    pub fn raw(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.raw_work(bytes, DataWork::Framing)
    }
    pub fn payload(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.raw_work(bytes, DataWork::Payload)
    }
    fn raw_work(&mut self, bytes: &[u8], work: DataWork) -> Result<(), Error> {
        let at = self.at();
        let end = Session::add(at, bytes.len(), at)?;
        self.session
            .cap(end, self.session.limits.bytes, LimitKind::Bytes, at)?;
        work.charge(self.session, bytes.len(), at)?;
        self.session.serialized(bytes.len(), at)?;
        if self.measuring {
            self.length = end;
            return Ok(());
        }
        if end > self.bytes.capacity() {
            let capacity = end.max(self.bytes.capacity().saturating_mul(2)).max(4);
            self.session
                .budget
                .allocate::<u8>(capacity)
                .map_err(|e| Session::budget_error(at, e))?;
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| {
                    self.session
                        .error(at, ErrorKind::Budget(m::ErrorKind::AllocationFailed))
                })?;
        }
        self.bytes.extend_from_slice(bytes);
        self.length = end;
        Ok(())
    }
    pub fn u8(&mut self, n: u8) -> Result<(), Error> {
        self.raw(&[n])
    }
    pub fn u32(&mut self, n: u32) -> Result<(), Error> {
        self.raw(&n.to_le_bytes())
    }
    pub fn u64(&mut self, n: u64) -> Result<(), Error> {
        self.raw(&n.to_le_bytes())
    }
    pub fn count(&mut self, n: usize) -> Result<(), Error> {
        let at = self.at();
        let n32 = u32::try_from(n).map_err(|_| self.session.error(at, ErrorKind::InvalidSchema))?;
        self.session.count(n, at)?;
        self.u32(n32)
    }
    pub fn string(&mut self, s: &str) -> Result<(), Error> {
        self.string_work(s, DataWork::Framing)
    }
    pub fn value_string(&mut self, s: &str) -> Result<(), Error> {
        self.string_work(s, DataWork::Payload)
    }
    fn string_work(&mut self, s: &str, work: DataWork) -> Result<(), Error> {
        let at = self.at();
        self.session.cap(
            s.len(),
            self.session.limits.string_bytes,
            LimitKind::StringBytes,
            at,
        )?;
        let len =
            u32::try_from(s.len()).map_err(|_| self.session.error(at, ErrorKind::InvalidSchema))?;
        self.u32(len)?;
        self.raw_work(s.as_bytes(), work)
    }
    pub fn header(&mut self, magic: &[u8; 4], version: u32, kind: u8) -> Result<(), Error> {
        self.raw(magic)?;
        self.u32(version)?;
        self.u8(kind)
    }
}
pub(super) struct Reader<'b, 's> {
    pub bytes: &'b [u8],
    pub at: usize,
    pub session: &'s mut Session,
}
impl<'b, 's> Reader<'b, 's> {
    pub fn new(bytes: &'b [u8], session: &'s mut Session) -> Result<Self, Error> {
        session.cap(bytes.len(), session.limits.bytes, LimitKind::Bytes, 0)?;
        session.serialized(bytes.len(), 0)?;
        Ok(Self {
            bytes,
            at: 0,
            session,
        })
    }
    pub fn raw(&mut self, n: usize) -> Result<&'b [u8], Error> {
        self.raw_work(n, DataWork::Framing)
    }
    pub fn payload(&mut self, n: usize) -> Result<&'b [u8], Error> {
        self.raw_work(n, DataWork::Payload)
    }
    fn raw_work(&mut self, n: usize, work: DataWork) -> Result<&'b [u8], Error> {
        let end = Session::add(self.at, n, self.at)?;
        let bytes = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| self.session.error(self.at, ErrorKind::Truncated))?;
        work.charge(self.session, n, self.at)?;
        self.at = end;
        Ok(bytes)
    }
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        Ok(self.raw(N)?.try_into().expect("exact length"))
    }
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.raw(1)?[0])
    }
    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    pub fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    pub fn boolean(&mut self) -> Result<bool, Error> {
        let at = self.at;
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(self.session.error(at, ErrorKind::Tag)),
        }
    }
    pub fn count(&mut self, minimum_bytes: usize) -> Result<usize, Error> {
        let at = self.at;
        let n = self.u32()? as usize;
        let min = n
            .checked_mul(minimum_bytes)
            .ok_or_else(|| self.session.error(at, ErrorKind::InvalidSchema))?;
        if min > self.bytes.len() - self.at {
            return Err(self.session.error(at, ErrorKind::Truncated));
        }
        self.session.count(n, at)?;
        Ok(n)
    }
    pub fn string(&mut self) -> Result<String, Error> {
        self.string_work(DataWork::Framing)
    }
    pub fn value_string(&mut self) -> Result<String, Error> {
        if self.session.service_work.is_some() {
            self.string_work(DataWork::Utf8Payload)
        } else {
            self.string()
        }
    }
    fn string_work(&mut self, work: DataWork) -> Result<String, Error> {
        let at = self.at;
        let n = self.u32()? as usize;
        self.session.cap(
            n,
            self.session.limits.string_bytes,
            LimitKind::StringBytes,
            at,
        )?;
        let raw = self.raw_work(n, work)?;
        if matches!(work, DataWork::Framing) {
            self.session.charge(n, at)?;
        }
        let s = std::str::from_utf8(raw).map_err(|_| self.session.error(at, ErrorKind::Utf8))?;
        self.session
            .budget
            .allocate::<u8>(n)
            .map_err(|e| Session::budget_error(at, e))?;
        let mut text = String::new();
        text.try_reserve_exact(n).map_err(|_| {
            self.session
                .error(at, ErrorKind::Budget(m::ErrorKind::AllocationFailed))
        })?;
        text.push_str(s);
        Ok(text)
    }
    pub fn header(&mut self, magic: &[u8; 4], version: u32, kind: u8) -> Result<(), Error> {
        if &self.array::<4>()? != magic {
            return Err(self.session.error(0, ErrorKind::Magic));
        }
        if self.u32()? != version {
            return Err(self.session.error(4, ErrorKind::Version));
        }
        if self.u8()? != kind {
            return Err(self.session.error(8, ErrorKind::Kind));
        }
        Ok(())
    }
    pub fn finish(&self) -> Result<(), Error> {
        if self.at != self.bytes.len() {
            Err(self.session.error(self.at, ErrorKind::Trailing))
        } else {
            Ok(())
        }
    }
}
