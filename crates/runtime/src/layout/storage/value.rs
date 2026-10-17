//! Allocation-free access to private managed scalar/core values. Locations are
//! structural offsets, not loan or initialization authority. The execution
//! ledger must admit reads and retire old owners before reusing a destination.
use super::*;
use crate::buffer::{AllocError, BoundsError, BufferError, Utf8Error};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnershipError {
    Allocation(AllocError),
    StrongCycle,
    ExpiredWeak,
    BorrowConflict,
    Unavailable,
    CountOverflow,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArenaError {
    Allocation(AllocError),
    CrossRegion,
    Unavailable,
    BorrowConflict,
}
/// Copy leaves only; this never duplicates an owner or grants token authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Immediate {
    I64(i64),
    Bool(bool),
    Unit,
    Allocation(AllocError),
    Bounds(BoundsError),
    Buffer(BufferError),
    Utf8(Utf8Error),
    Ownership(OwnershipError),
    Arena(ArenaError),
}

fn range(start: u64, size: u64) -> Result<Range<usize>, Error> {
    Ok(usize::try_from(start).map_err(|_| Error::Overflow)?
        ..usize::try_from(add(start, size)?).map_err(|_| Error::Overflow)?)
}
fn alloc_tag(value: AllocError) -> u32 {
    match value {
        AllocError::Limit => 0,
        AllocError::OutOfMemory => 1,
        AllocError::Overflow => 2,
    }
}
fn alloc_value(tag: u32) -> Result<AllocError, Error> {
    match tag {
        0 => Ok(AllocError::Limit),
        1 => Ok(AllocError::OutOfMemory),
        2 => Ok(AllocError::Overflow),
        _ => Err(Error::InvalidCase(tag)),
    }
}
impl Immediate {
    /// Source enum case order for finite errors; scalar/Unit leaves have no case.
    pub fn case(self) -> Option<u32> {
        if matches!(self, Self::I64(_) | Self::Bool(_) | Self::Unit) {
            return None;
        }
        let mut bytes = [0; 16];
        self.encode(&mut bytes);
        Some(u32::from_le_bytes(bytes[..4].try_into().unwrap()))
    }
    /// Copy payload of a finite error, for source match/projection operations.
    pub const fn payload(self) -> Option<Self> {
        match self {
            Self::Buffer(BufferError::Allocation(error))
            | Self::Ownership(OwnershipError::Allocation(error))
            | Self::Arena(ArenaError::Allocation(error)) => Some(Self::Allocation(error)),
            Self::Utf8(Utf8Error::Invalid(offset)) => Some(Self::I64(offset)),
            _ => None,
        }
    }
    fn encode(self, bytes: &mut [u8; 16]) -> (Option<o::Builtin>, usize) {
        use o::Builtin as B;
        let (builtin, tag, allocation) = match self {
            Self::I64(value) => {
                bytes[..8].copy_from_slice(&value.to_le_bytes());
                return (None, 8);
            }
            Self::Bool(value) => {
                bytes[0] = u8::from(value);
                return (None, 1);
            }
            Self::Unit => return (Some(B::Unit), 0),
            Self::Allocation(error) => (B::AllocError, alloc_tag(error), None),
            Self::Bounds(BoundsError::OutOfBounds) => (B::BoundsError, 0, None),
            Self::Buffer(error) => match error {
                BufferError::Bounds => (B::BufferError, 0, None),
                BufferError::InvalidByte => (B::BufferError, 1, None),
                BufferError::Allocation(error) => (B::BufferError, 2, Some(error)),
            },
            Self::Utf8(Utf8Error::Invalid(offset)) => {
                bytes[8..16].copy_from_slice(&offset.to_le_bytes());
                return (Some(B::Utf8Error), 16);
            }
            Self::Ownership(error) => match error {
                OwnershipError::Allocation(error) => (B::OwnershipError, 0, Some(error)),
                OwnershipError::StrongCycle => (B::OwnershipError, 1, None),
                OwnershipError::ExpiredWeak => (B::OwnershipError, 2, None),
                OwnershipError::BorrowConflict => (B::OwnershipError, 3, None),
                OwnershipError::Unavailable => (B::OwnershipError, 4, None),
                OwnershipError::CountOverflow => (B::OwnershipError, 5, None),
            },
            Self::Arena(error) => match error {
                ArenaError::Allocation(error) => (B::ArenaError, 0, Some(error)),
                ArenaError::CrossRegion => (B::ArenaError, 1, None),
                ArenaError::Unavailable => (B::ArenaError, 2, None),
                ArenaError::BorrowConflict => (B::ArenaError, 3, None),
            },
        };
        bytes[..4].copy_from_slice(&tag.to_le_bytes());
        if let Some(error) = allocation {
            bytes[4..8].copy_from_slice(&alloc_tag(error).to_le_bytes());
        }
        let size = match builtin {
            B::AllocError | B::BoundsError => 4,
            _ => 8,
        };
        (Some(builtin), size)
    }
}

impl Reservation<'_, '_> {
    fn builtin(&self, ty: o::Type) -> Option<o::Builtin> {
        let o::Type::Constructed(id) = ty else {
            return None;
        };
        match self.layouts.types.table().constructed[id.0 as usize].constructor {
            o::Constructor::Builtin(builtin) => Some(builtin),
            _ => None,
        }
    }
    /// Validate type and full destination bounds before mutation. All scratch
    /// storage is fixed on the host stack; no guest/provider allocation occurs.
    pub fn write_immediate<P: Provider>(
        &self,
        location: Location,
        value: Immediate,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        let layout = self.validate_location(location)?;
        let mut bytes = [0; 16];
        let (builtin, size) = value.encode(&mut bytes);
        let valid_type = match value {
            Immediate::I64(_) => location.ty == o::Type::I64,
            Immediate::Bool(_) => location.ty == o::Type::Bool,
            _ => builtin.is_some() && builtin == self.builtin(location.ty),
        };
        if !valid_type || layout.size != size as u64 {
            return Err(Error::InvalidType(location.ty));
        }
        let destination = allocator
            .bytes_mut(self.data)?
            .get_mut(range(location.offset, layout.size)?)
            .ok_or(Error::State)?;
        destination.copy_from_slice(&bytes[..size]);
        Ok(())
    }
    pub fn read_immediate<P: Provider>(
        &self,
        location: Location,
        allocator: &Allocator<P>,
    ) -> Result<Immediate, Error> {
        let layout = self.validate_location(location)?;
        let bytes = allocator
            .bytes(self.data)?
            .get(range(location.offset, layout.size)?)
            .ok_or(Error::State)?;
        match location.ty {
            o::Type::I64 => return Ok(Immediate::I64(read_u64(bytes, 0)? as i64)),
            o::Type::Bool => {
                return match bytes.first() {
                    Some(0) => Ok(Immediate::Bool(false)),
                    Some(1) => Ok(Immediate::Bool(true)),
                    _ => Err(Error::InvalidValue),
                };
            }
            _ => {}
        }
        use o::Builtin as B;
        if self.builtin(location.ty) == Some(B::Unit) {
            return Ok(Immediate::Unit);
        }
        let Kind::Error { builtin, cases, .. } = layout.kind else {
            return Err(Error::InvalidType(location.ty));
        };
        let tag = read_u32(bytes, 0)?;
        if tag >= cases {
            return Err(Error::InvalidCase(tag));
        }
        Ok(match builtin {
            B::AllocError => Immediate::Allocation(alloc_value(tag)?),
            B::BoundsError => Immediate::Bounds(BoundsError::OutOfBounds),
            B::BufferError => Immediate::Buffer(match tag {
                0 => BufferError::Bounds,
                1 => BufferError::InvalidByte,
                _ => BufferError::Allocation(alloc_value(read_u32(bytes, 4)?)?),
            }),
            B::Utf8Error => Immediate::Utf8(Utf8Error::Invalid(read_u64(bytes, 8)? as i64)),
            B::OwnershipError => Immediate::Ownership(match tag {
                0 => OwnershipError::Allocation(alloc_value(read_u32(bytes, 4)?)?),
                1 => OwnershipError::StrongCycle,
                2 => OwnershipError::ExpiredWeak,
                3 => OwnershipError::BorrowConflict,
                4 => OwnershipError::Unavailable,
                _ => OwnershipError::CountOverflow,
            }),
            B::ArenaError => Immediate::Arena(match tag {
                0 => ArenaError::Allocation(alloc_value(read_u32(bytes, 4)?)?),
                1 => ArenaError::CrossRegion,
                2 => ArenaError::Unavailable,
                _ => ArenaError::BorrowConflict,
            }),
            _ => return Err(Error::InvalidType(location.ty)),
        })
    }
    /// Read a validated Option/Result/nominal/error discriminant. Payload reads
    /// and ownership admission still require the execution ledger's proofs.
    pub fn case<P: Provider>(
        &self,
        location: Location,
        allocator: &Allocator<P>,
    ) -> Result<u32, Error> {
        let layout = self.validate_location(location)?;
        if matches!(layout.kind, Kind::Error { .. }) {
            return self
                .read_immediate(location, allocator)?
                .case()
                .ok_or(Error::InvalidType(location.ty));
        }
        let Kind::Enum { cases, .. } = &layout.kind else {
            return Err(Error::InvalidType(location.ty));
        };
        let bytes = allocator
            .bytes(self.data)?
            .get(range(location.offset, layout.size)?)
            .ok_or(Error::State)?;
        let case = read_u32(bytes, 0)?;
        if case as usize >= cases.len() {
            return Err(Error::InvalidCase(case));
        }
        Ok(case)
    }
    /// Prepare an unpublished variant destination, clearing stale payload and
    /// construction metadata. Transfer payloads, then call initialize_order for
    /// dropping aggregates. This does not retire an old owner or publish a value.
    /// Wrong cases, foreign locations and bounds fail before any write.
    pub fn prepare_case<P: Provider>(
        &self,
        location: Location,
        case: u32,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        let layout = self.validate_location(location)?;
        let Kind::Enum { cases, .. } = &layout.kind else {
            return Err(Error::InvalidType(location.ty));
        };
        if case as usize >= cases.len() {
            return Err(Error::InvalidCase(case));
        }
        let data = range(location.offset, layout.size)?;
        let metadata = range(location.metadata, layout.metadata_bytes)?;
        allocator
            .bytes(self.data)?
            .get(data.clone())
            .ok_or(Error::State)?;
        allocator
            .bytes(self.metadata)?
            .get(metadata.clone())
            .ok_or(Error::State)?;
        allocator.bytes_mut(self.metadata)?[metadata].fill(0);
        let bytes = &mut allocator.bytes_mut(self.data)?[data];
        bytes.fill(0);
        write_u32(bytes, 0, case)
    }
}
