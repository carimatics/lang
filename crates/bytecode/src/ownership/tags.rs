use super::*;
use o::{
    Builtin as B, LoanKind as L, bodies::EndMode as E, core::Operation as C,
    declarations::LocalKind as K,
};

// Explicit RFC 0011 assignments, never Rust enum discriminants. Adding an
// accepted MIR variant requires a deliberate wire/version decision here.
macro_rules! tags {
    ($write:ident, $read:ident, $ty:ty, $tag:expr, {$($number:literal => $variant:path),+ $(,)?}) => {
        pub(super) fn $write(value: $ty) -> u8 { match value { $($variant => $number),+ } }
        pub(super) fn $read(value: u8, offset: usize) -> Result<$ty, CodecError> {
            match value { $($number => Ok($variant)),+, value => Err(error(offset, CodecErrorKind::InvalidTag { tag: $tag, value })) }
        }
    };
}
tags!(builtin, read_builtin, B, Tag::Builtin, {
    0=>B::Unit, 1=>B::Text, 2=>B::Bytes, 3=>B::Allocator, 4=>B::AllocError,
    5=>B::BoundsError, 6=>B::BufferError, 7=>B::Utf8Error, 8=>B::OwnershipError, 9=>B::ArenaError
});
tags!(loan, read_loan, L, Tag::LoanKind, {0=>L::Shared, 1=>L::Exclusive});
tags!(end, read_end, E, Tag::EndMode, {0=>E::Required, 1=>E::IfActive});
tags!(local, read_local, K, Tag::LocalKind, {0=>K::Parameter, 1=>K::Mutable, 2=>K::Pattern, 3=>K::ArenaOwner});
tags!(core, read_core, C, Tag::CoreOperation, {
    0=>C::Allocator, 1=>C::TextFromUtf8, 2=>C::TextClone, 3=>C::TextLen,
    4=>C::TextBytes, 5=>C::TextEqual, 6=>C::TextConcat, 7=>C::BytesNew,
    8=>C::BytesClone, 9=>C::BytesLen, 10=>C::BytesGet, 11=>C::BytesPush,
    12=>C::BytesSet, 13=>C::VecNew, 14=>C::VecLen, 15=>C::VecPush,
    16=>C::VecPop, 17=>C::VecGet, 18=>C::VecGetMut, 19=>C::MapNew,
    20=>C::MapLen, 21=>C::MapInsert, 22=>C::MapGet, 23=>C::MapRemove,
    24=>C::SharedNew, 25=>C::SharedClone, 26=>C::SharedRead, 27=>C::SharedReplace,
    28=>C::WeakNew, 29=>C::WeakClone, 30=>C::WeakUpgrade, 31=>C::ArenaNew,
    32=>C::ArenaRead, 33=>C::ArenaReplace, 34=>C::OptionUnwrap, 35=>C::ResultUnwrap,
    36=>C::ResultUnwrapErr
});
