//! RFC 0007 byte/text storage under explicit instance allocation. Headers and
//! capacity are requested storage; flat leaves need no new cleanup work area.
//! Tokens are private in-memory execution data, never a host/source ABI. Like
//! managed MIR registers, these owner wrappers have explicit ledger cleanup;
//! Rust wrapper disposal alone does not release the instance's allocation.
use crate::allocation::{self as a, Allocator, Handle, Provider, Request};
use std::fmt::{Display, Formatter};

const HEADER: usize = 24;
/// Validate token/header provenance without scanning text. Callers must charge
/// their byte-work budget before UTF-8 validation or comparison of this view.
pub(crate) fn text_token_bytes<P: Provider>(
    token: [u8; 32],
    allocator: &Allocator<P>,
) -> Result<&[u8], Fault> {
    if word(&token, 24)? != Kind::Text as u64 {
        return Err(Fault::WrongKind);
    }
    Buffer {
        handle: Some(Handle::from_token(
            token[..24].try_into().expect("fixed token"),
        )?),
        kind: Kind::Text,
    }
    .view(allocator)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Bytes = 0,
    Text = 1,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllocError {
    Limit,
    OutOfMemory,
    Overflow,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundsError {
    OutOfBounds,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferError {
    Bounds,
    InvalidByte,
    Allocation(AllocError),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Utf8Error {
    Invalid(i64),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fault {
    Allocator(a::Error),
    Consumed,
    InvalidStorage,
    WrongKind,
    UnsupportedType,
}
impl Display for Fault {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "buffer runtime fault: {self:?}")
    }
}
impl std::error::Error for Fault {}
impl From<a::Error> for Fault {
    fn from(error: a::Error) -> Self {
        Self::Allocator(error)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure<E> {
    Error(E),
    Fault(Fault),
}
pub type Outcome<T, E> = Result<T, Failure<E>>;
impl<E> Failure<E> {
    fn map<F>(self, convert: impl FnOnce(E) -> F) -> Failure<F> {
        match self {
            Self::Error(error) => Failure::Error(convert(error)),
            Self::Fault(fault) => Failure::Fault(fault),
        }
    }
}
fn allocation(error: a::Error) -> Failure<AllocError> {
    match error {
        a::Error::Limit(_) => Failure::Error(AllocError::Limit),
        a::Error::OutOfMemory => Failure::Error(AllocError::OutOfMemory),
        a::Error::Overflow => Failure::Error(AllocError::Overflow),
        error => Failure::Fault(Fault::Allocator(error)),
    }
}
#[derive(Clone, Copy)]
struct Header {
    len: usize,
    capacity: usize,
}
#[derive(Debug)]
struct Buffer {
    handle: Option<Handle>,
    kind: Kind,
}
#[derive(Debug)]
pub struct Bytes(Buffer);
#[derive(Debug)]
pub struct Text(Buffer);

fn word(bytes: &[u8], offset: usize) -> Result<u64, Fault> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or(Fault::InvalidStorage)?
            .try_into()
            .map_err(|_| Fault::InvalidStorage)?,
    ))
}
fn write_word(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
impl Buffer {
    fn handle(&self) -> Result<Handle, Fault> {
        self.handle.ok_or(Fault::Consumed)
    }
    fn inspect<P: Provider>(&self, allocator: &Allocator<P>) -> Result<Header, Fault> {
        let handle = self.handle()?;
        let request = allocator.request(handle)?;
        let bytes = allocator.bytes(handle)?;
        if bytes.len() < HEADER || request.alignment() != 8 {
            return Err(Fault::InvalidStorage);
        }
        let len = usize::try_from(word(bytes, 0)?).map_err(|_| Fault::InvalidStorage)?;
        let capacity = usize::try_from(word(bytes, 8)?).map_err(|_| Fault::InvalidStorage)?;
        if word(bytes, 16)? != self.kind as u64 {
            return Err(Fault::WrongKind);
        }
        if len > capacity
            || capacity > i64::MAX as usize
            || capacity.checked_add(HEADER) != Some(bytes.len())
        {
            return Err(Fault::InvalidStorage);
        }
        Ok(Header { len, capacity })
    }
    fn view<'a, P: Provider>(&self, allocator: &'a Allocator<P>) -> Result<&'a [u8], Fault> {
        let header = self.inspect(allocator)?;
        Ok(&allocator.bytes(self.handle()?)?[HEADER..HEADER + header.len])
    }
    fn allocate<P: Provider>(
        allocator: &mut Allocator<P>,
        kind: Kind,
        capacity: usize,
        len: usize,
    ) -> Outcome<Self, AllocError> {
        if len > capacity || capacity > i64::MAX as usize {
            return Err(Failure::Error(AllocError::Overflow));
        }
        let size = capacity
            .checked_add(HEADER)
            .ok_or(Failure::Error(AllocError::Overflow))?;
        let request = Request::new(size as u64, 8).map_err(allocation)?;
        let handle = allocator.allocate(request).map_err(allocation)?;
        let bytes = allocator
            .bytes_mut(handle)
            .map_err(|e| Failure::Fault(e.into()))?;
        // Providers need not zero storage. Initialize the header and all live data.
        bytes[..HEADER + len].fill(0);
        write_word(bytes, 0, len as u64);
        write_word(bytes, 8, capacity as u64);
        write_word(bytes, 16, kind as u64);
        Ok(Self {
            handle: Some(handle),
            kind,
        })
    }
    fn release<P: Provider>(&mut self, allocator: &mut Allocator<P>) -> Result<(), Fault> {
        let Some(handle) = self.handle else {
            return Ok(());
        };
        allocator.request(handle)?;
        self.handle = None; // Consume before trusted deallocation, including panic.
        allocator.release(handle)?;
        Ok(())
    }
    fn token<P: Provider>(&mut self, allocator: &Allocator<P>) -> Result<[u8; 32], Fault> {
        self.inspect(allocator)?;
        let handle = self.handle.take().ok_or(Fault::Consumed)?;
        let mut bytes = [0; 32];
        bytes[..24].copy_from_slice(&handle.token());
        bytes[24..].copy_from_slice(&(self.kind as u64).to_le_bytes());
        Ok(bytes)
    }
    fn take_token<P: Provider>(
        bytes: &mut [u8; 32],
        kind: Kind,
        allocator: &Allocator<P>,
    ) -> Result<Self, Fault> {
        if word(bytes, 24)? != kind as u64 {
            return Err(Fault::WrongKind);
        }
        let handle = Handle::from_token(bytes[..24].try_into().expect("fixed token"))?;
        let result = Self {
            handle: Some(handle),
            kind,
        };
        let data = result.view(allocator)?;
        if kind == Kind::Text && std::str::from_utf8(data).is_err() {
            return Err(Fault::InvalidStorage);
        }
        bytes.fill(0);
        Ok(result)
    }
    fn copy<P: Provider>(
        &self,
        allocator: &mut Allocator<P>,
        kind: Kind,
    ) -> Outcome<Self, AllocError> {
        let header = self.inspect(allocator).map_err(Failure::Fault)?;
        let mut result = Self::allocate(allocator, kind, header.len, header.len)?;
        if let Err(error) = allocator.copy_bytes(
            self.handle().map_err(Failure::Fault)?,
            HEADER,
            result.handle().map_err(Failure::Fault)?,
            HEADER,
            header.len,
        ) {
            result.release(allocator).map_err(Failure::Fault)?;
            return Err(Failure::Fault(error.into()));
        }
        Ok(result)
    }
    fn copy_in<P: Provider, Q: Provider>(
        &self,
        source: &Allocator<P>,
        target: &mut Allocator<Q>,
        kind: Kind,
    ) -> Outcome<Self, AllocError> {
        let data = self.view(source).map_err(Failure::Fault)?;
        let result = Self::allocate(target, kind, data.len(), data.len())?;
        target
            .bytes_mut(result.handle().map_err(Failure::Fault)?)
            .map_err(|e| Failure::Fault(e.into()))?[HEADER..HEADER + data.len()]
            .copy_from_slice(data);
        Ok(result)
    }
}
impl Bytes {
    pub fn is_empty<P: Provider>(&self, allocator: &Allocator<P>) -> Result<bool, Fault> {
        Ok(self.0.inspect(allocator)?.len == 0)
    }
    pub fn new<P: Provider>(allocator: &mut Allocator<P>) -> Outcome<Self, AllocError> {
        Ok(Self(Buffer::allocate(allocator, Kind::Bytes, 0, 0)?))
    }
    /// Explicit trusted host input; source byte literals are a separate design.
    pub fn from_slice<P: Provider>(
        allocator: &mut Allocator<P>,
        bytes: &[u8],
    ) -> Outcome<Self, AllocError> {
        let result = Self(Buffer::allocate(
            allocator,
            Kind::Bytes,
            bytes.len(),
            bytes.len(),
        )?);
        allocator
            .bytes_mut(result.0.handle().map_err(Failure::Fault)?)
            .map_err(|e| Failure::Fault(e.into()))?[HEADER..]
            .copy_from_slice(bytes);
        Ok(result)
    }
    pub fn len<P: Provider>(&self, allocator: &Allocator<P>) -> Result<i64, Fault> {
        Ok(self.0.inspect(allocator)?.len as i64)
    }
    pub fn as_slice<'a, P: Provider>(
        &self,
        allocator: &'a Allocator<P>,
    ) -> Result<&'a [u8], Fault> {
        self.0.view(allocator)
    }
    pub fn get<P: Provider>(
        &self,
        allocator: &Allocator<P>,
        index: i64,
    ) -> Outcome<i64, BoundsError> {
        let bytes = self.0.view(allocator).map_err(Failure::Fault)?;
        let index = usize::try_from(index).map_err(|_| Failure::Error(BoundsError::OutOfBounds))?;
        Ok(i64::from(
            *bytes
                .get(index)
                .ok_or(Failure::Error(BoundsError::OutOfBounds))?,
        ))
    }
    pub fn set<P: Provider>(
        &mut self,
        allocator: &mut Allocator<P>,
        index: i64,
        value: i64,
    ) -> Outcome<(), BufferError> {
        let header = self.0.inspect(allocator).map_err(Failure::Fault)?;
        let index = usize::try_from(index)
            .ok()
            .filter(|&i| i < header.len)
            .ok_or(Failure::Error(BufferError::Bounds))?;
        let value = u8::try_from(value).map_err(|_| Failure::Error(BufferError::InvalidByte))?;
        allocator
            .bytes_mut(self.0.handle().map_err(Failure::Fault)?)
            .map_err(|e| Failure::Fault(e.into()))?[HEADER + index] = value;
        Ok(())
    }
    pub fn push<P: Provider>(
        &mut self,
        allocator: &mut Allocator<P>,
        value: i64,
    ) -> Outcome<(), BufferError> {
        let header = self.0.inspect(allocator).map_err(Failure::Fault)?;
        let value = u8::try_from(value).map_err(|_| Failure::Error(BufferError::InvalidByte))?;
        let len = header
            .len
            .checked_add(1)
            .filter(|&n| n <= i64::MAX as usize)
            .ok_or(Failure::Error(BufferError::Allocation(
                AllocError::Overflow,
            )))?;
        if len > header.capacity {
            let capacity = header
                .capacity
                .checked_mul(2)
                .filter(|&n| n <= i64::MAX as usize)
                .unwrap_or(i64::MAX as usize)
                .max(len);
            let size =
                capacity
                    .checked_add(HEADER)
                    .ok_or(Failure::Error(BufferError::Allocation(
                        AllocError::Overflow,
                    )))?;
            let mut handle = self.0.handle().map_err(Failure::Fault)?;
            allocator
                .resize(&mut handle, size as u64)
                .map_err(|e| allocation(e).map(BufferError::Allocation))?;
            self.0.handle = Some(handle);
            write_word(
                allocator
                    .bytes_mut(handle)
                    .map_err(|e| Failure::Fault(e.into()))?,
                8,
                capacity as u64,
            );
        }
        let bytes = allocator
            .bytes_mut(self.0.handle().map_err(Failure::Fault)?)
            .map_err(|e| Failure::Fault(e.into()))?;
        bytes[HEADER + header.len] = value;
        write_word(bytes, 0, len as u64);
        Ok(())
    }
    pub fn try_clone<P: Provider>(
        &self,
        allocator: &mut Allocator<P>,
    ) -> Outcome<Self, AllocError> {
        Ok(Self(self.0.copy(allocator, Kind::Bytes)?))
    }
    pub fn try_clone_in<P: Provider, Q: Provider>(
        &self,
        source: &Allocator<P>,
        target: &mut Allocator<Q>,
    ) -> Outcome<Self, AllocError> {
        Ok(Self(self.0.copy_in(source, target, Kind::Bytes)?))
    }
    /// Consume on either domain outcome, with no allocation. Runtime preflight
    /// faults preserve the owner so the enclosing failure cleanup can retire it.
    pub fn take_text<P: Provider>(
        &mut self,
        allocator: &mut Allocator<P>,
    ) -> Outcome<Text, Utf8Error> {
        let bytes = self.0.view(allocator).map_err(Failure::Fault)?;
        if let Err(error) = std::str::from_utf8(bytes) {
            let offset = error.valid_up_to() as i64;
            self.0.release(allocator).map_err(Failure::Fault)?;
            return Err(Failure::Error(Utf8Error::Invalid(offset)));
        }
        let handle = self.0.handle().map_err(Failure::Fault)?;
        write_word(
            allocator
                .bytes_mut(handle)
                .map_err(|e| Failure::Fault(e.into()))?,
            16,
            Kind::Text as u64,
        );
        self.0.handle = None;
        Ok(Text(Buffer {
            handle: Some(handle),
            kind: Kind::Text,
        }))
    }
    pub fn release<P: Provider>(&mut self, allocator: &mut Allocator<P>) -> Result<(), Fault> {
        self.0.release(allocator)
    }
    pub fn take_token<P: Provider>(&mut self, allocator: &Allocator<P>) -> Result<[u8; 32], Fault> {
        self.0.token(allocator)
    }
    pub fn from_token<P: Provider>(
        bytes: &mut [u8; 32],
        allocator: &Allocator<P>,
    ) -> Result<Self, Fault> {
        Ok(Self(Buffer::take_token(bytes, Kind::Bytes, allocator)?))
    }
}
impl Text {
    pub fn is_empty<P: Provider>(&self, allocator: &Allocator<P>) -> Result<bool, Fault> {
        Ok(self.0.inspect(allocator)?.len == 0)
    }
    pub fn len<P: Provider>(&self, allocator: &Allocator<P>) -> Result<i64, Fault> {
        Ok(self.0.inspect(allocator)?.len as i64)
    }
    pub fn as_str<'a, P: Provider>(&self, allocator: &'a Allocator<P>) -> Result<&'a str, Fault> {
        std::str::from_utf8(self.0.view(allocator)?).map_err(|_| Fault::InvalidStorage)
    }
    pub fn equal<P: Provider>(
        &self,
        other: &Self,
        allocator: &Allocator<P>,
    ) -> Result<bool, Fault> {
        Ok(self.0.view(allocator)? == other.0.view(allocator)?)
    }
    pub fn try_clone<P: Provider>(
        &self,
        allocator: &mut Allocator<P>,
    ) -> Outcome<Self, AllocError> {
        Ok(Self(self.0.copy(allocator, Kind::Text)?))
    }
    pub fn try_clone_in<P: Provider, Q: Provider>(
        &self,
        source: &Allocator<P>,
        target: &mut Allocator<Q>,
    ) -> Outcome<Self, AllocError> {
        Ok(Self(self.0.copy_in(source, target, Kind::Text)?))
    }
    pub fn to_bytes<P: Provider>(
        &self,
        allocator: &mut Allocator<P>,
    ) -> Outcome<Bytes, AllocError> {
        Ok(Bytes(self.0.copy(allocator, Kind::Bytes)?))
    }
    pub fn to_bytes_in<P: Provider, Q: Provider>(
        &self,
        source: &Allocator<P>,
        target: &mut Allocator<Q>,
    ) -> Outcome<Bytes, AllocError> {
        Ok(Bytes(self.0.copy_in(source, target, Kind::Bytes)?))
    }
    pub fn concat<P: Provider>(
        &self,
        other: &Self,
        allocator: &mut Allocator<P>,
    ) -> Outcome<Self, AllocError> {
        let left = self.0.inspect(allocator).map_err(Failure::Fault)?;
        let right = other.0.inspect(allocator).map_err(Failure::Fault)?;
        let len = left
            .len
            .checked_add(right.len)
            .ok_or(Failure::Error(AllocError::Overflow))?;
        let mut result = Buffer::allocate(allocator, Kind::Text, len, len)?;
        let copy = allocator
            .copy_bytes(
                self.0.handle().map_err(Failure::Fault)?,
                HEADER,
                result.handle().map_err(Failure::Fault)?,
                HEADER,
                left.len,
            )
            .and_then(|()| {
                allocator.copy_bytes(
                    other.0.handle().expect("validated owner"),
                    HEADER,
                    result.handle().expect("new owner"),
                    HEADER + left.len,
                    right.len,
                )
            });
        if let Err(error) = copy {
            result.release(allocator).map_err(Failure::Fault)?;
            return Err(Failure::Fault(error.into()));
        }
        Ok(Self(result))
    }
    pub fn concat_in<P: Provider, Q: Provider, R: Provider>(
        &self,
        source: &Allocator<P>,
        other: &Self,
        other_source: &Allocator<Q>,
        target: &mut Allocator<R>,
    ) -> Outcome<Self, AllocError> {
        let left = self.0.view(source).map_err(Failure::Fault)?;
        let right = other.0.view(other_source).map_err(Failure::Fault)?;
        let len = left
            .len()
            .checked_add(right.len())
            .ok_or(Failure::Error(AllocError::Overflow))?;
        let result = Buffer::allocate(target, Kind::Text, len, len)?;
        let data = target
            .bytes_mut(result.handle().map_err(Failure::Fault)?)
            .map_err(|e| Failure::Fault(e.into()))?;
        data[HEADER..HEADER + left.len()].copy_from_slice(left);
        data[HEADER + left.len()..HEADER + len].copy_from_slice(right);
        Ok(Self(result))
    }
    pub fn release<P: Provider>(&mut self, allocator: &mut Allocator<P>) -> Result<(), Fault> {
        self.0.release(allocator)
    }
    pub fn take_token<P: Provider>(&mut self, allocator: &Allocator<P>) -> Result<[u8; 32], Fault> {
        self.0.token(allocator)
    }
    pub fn from_token<P: Provider>(
        bytes: &mut [u8; 32],
        allocator: &Allocator<P>,
    ) -> Result<Self, Fault> {
        Ok(Self(Buffer::take_token(bytes, Kind::Text, allocator)?))
    }
}

/// Resolve a checked layout's Bytes/Text leaf directly, without a host owner
/// registry. Copies of raw token bytes do not create source ownership authority.
pub fn cleanup_token<P: Provider>(
    types: &lang_mir::unit::ownership::VerifiedTypes,
    token: &mut crate::layout::Token,
    allocator: &mut Allocator<P>,
) -> Result<(), Fault> {
    use lang_mir::unit::ownership::{Builtin, Constructor, Type};
    let Type::Constructed(id) = token.leaf.ty else {
        return Err(Fault::UnsupportedType);
    };
    let kind = match types
        .table()
        .constructed
        .get(id.0 as usize)
        .map(|t| t.constructor)
    {
        Some(Constructor::Builtin(Builtin::Bytes)) => Kind::Bytes,
        Some(Constructor::Builtin(Builtin::Text)) => Kind::Text,
        _ => return Err(Fault::UnsupportedType),
    };
    if token.bytes == [0; 32] {
        return Ok(());
    }
    let mut buffer = Buffer::take_token(&mut token.bytes, kind, allocator)?;
    buffer.release(allocator)
}
