//! Portable schema-1 register programs. Encoding maps fields explicitly; no
//! Rust representation, registry address, or native padding is persisted.

mod codec;

pub use codec::{CodecError, CodecErrorKind, CodecLimits, Table, Tag, decode, encode};
use lang_mir::unit::{Limits, Unit, VerifiedUnit};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    verified: VerifiedUnit,
}
impl Program {
    pub const fn from_verified(verified: VerifiedUnit) -> Self {
        Self { verified }
    }
    pub fn lower(unit: Unit, limits: Limits) -> Result<Self, lang_mir::unit::Error> {
        Ok(Self {
            verified: unit.verify(limits)?,
        })
    }
    pub fn verified(&self) -> &VerifiedUnit {
        &self.verified
    }
    pub fn into_verified(self) -> VerifiedUnit {
        self.verified
    }
}
