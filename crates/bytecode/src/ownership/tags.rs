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
pub(super) fn import_abi(value: o::declarations::ImportAbi) -> Option<u8> {
    match value {
        o::declarations::ImportAbi::Host => Some(0),
        o::declarations::ImportAbi::C => Some(1),
        o::declarations::ImportAbi::Component => None,
    }
}
pub(super) fn read_import_abi(
    value: u8,
    offset: usize,
) -> Result<o::declarations::ImportAbi, CodecError> {
    match value {
        0 => Ok(o::declarations::ImportAbi::Host),
        1 => Ok(o::declarations::ImportAbi::C),
        value => Err(error(
            offset,
            CodecErrorKind::InvalidTag {
                tag: Tag::ImportAbi,
                value,
            },
        )),
    }
}
tags!(function_abi, read_function_abi, o::declarations::FunctionAbi, Tag::FunctionAbi, {
    0 => o::declarations::FunctionAbi::Guest, 1 => o::declarations::FunctionAbi::CExport
});
pub(super) fn builtin(value: B) -> Option<u8> {
    Some(match value {
        B::Unit => 0,
        B::Text => 1,
        B::Bytes => 2,
        B::Allocator => 3,
        B::AllocError => 4,
        B::BoundsError => 5,
        B::BufferError => 6,
        B::Utf8Error => 7,
        B::OwnershipError => 8,
        B::ArenaError => 9,
        B::TaskError | B::ChannelError | B::ServiceError => return None,
    })
}
pub(super) fn read_builtin(value: u8, offset: usize) -> Result<B, CodecError> {
    Ok(match value {
        0 => B::Unit,
        1 => B::Text,
        2 => B::Bytes,
        3 => B::Allocator,
        4 => B::AllocError,
        5 => B::BoundsError,
        6 => B::BufferError,
        7 => B::Utf8Error,
        8 => B::OwnershipError,
        9 => B::ArenaError,
        value => {
            return Err(error(
                offset,
                CodecErrorKind::InvalidTag {
                    tag: Tag::Builtin,
                    value,
                },
            ));
        }
    })
}
tags!(loan, read_loan, L, Tag::LoanKind, {0=>L::Shared, 1=>L::Exclusive});
tags!(end, read_end, E, Tag::EndMode, {0=>E::Required, 1=>E::IfActive});
tags!(local, read_local, K, Tag::LocalKind, {0=>K::Parameter, 1=>K::Mutable, 2=>K::Pattern, 3=>K::ArenaOwner});
pub(super) fn core(value: C) -> Option<u8> {
    Some(match value {
        C::Allocator => 0,
        C::TextFromUtf8 => 1,
        C::TextClone => 2,
        C::TextLen => 3,
        C::TextBytes => 4,
        C::TextEqual => 5,
        C::TextConcat => 6,
        C::BytesNew => 7,
        C::BytesClone => 8,
        C::BytesLen => 9,
        C::BytesGet => 10,
        C::BytesPush => 11,
        C::BytesSet => 12,
        C::VecNew => 13,
        C::VecLen => 14,
        C::VecPush => 15,
        C::VecPop => 16,
        C::VecGet => 17,
        C::VecGetMut => 18,
        C::MapNew => 19,
        C::MapLen => 20,
        C::MapInsert => 21,
        C::MapGet => 22,
        C::MapRemove => 23,
        C::SharedNew => 24,
        C::SharedClone => 25,
        C::SharedRead => 26,
        C::SharedReplace => 27,
        C::WeakNew => 28,
        C::WeakClone => 29,
        C::WeakUpgrade => 30,
        C::ArenaNew => 31,
        C::ArenaRead => 32,
        C::ArenaReplace => 33,
        C::OptionUnwrap => 34,
        C::ResultUnwrap => 35,
        C::ResultUnwrapErr => 36,
        C::ServiceNew
        | C::ServiceHandle
        | C::ServiceClone
        | C::ServiceOwner
        | C::ServiceRead
        | C::ServiceReplace
        | C::ServiceFinish
        | C::ServiceCancel
        | C::ChannelNew
        | C::ChannelSender
        | C::ChannelClone
        | C::ChannelReceiver
        | C::ChannelSeal
        | C::ChannelUnsent => return None,
    })
}
pub(super) fn read_core(value: u8, offset: usize) -> Result<C, CodecError> {
    match value {
        0 => Ok(C::Allocator),
        1 => Ok(C::TextFromUtf8),
        2 => Ok(C::TextClone),
        3 => Ok(C::TextLen),
        4 => Ok(C::TextBytes),
        5 => Ok(C::TextEqual),
        6 => Ok(C::TextConcat),
        7 => Ok(C::BytesNew),
        8 => Ok(C::BytesClone),
        9 => Ok(C::BytesLen),
        10 => Ok(C::BytesGet),
        11 => Ok(C::BytesPush),
        12 => Ok(C::BytesSet),
        13 => Ok(C::VecNew),
        14 => Ok(C::VecLen),
        15 => Ok(C::VecPush),
        16 => Ok(C::VecPop),
        17 => Ok(C::VecGet),
        18 => Ok(C::VecGetMut),
        19 => Ok(C::MapNew),
        20 => Ok(C::MapLen),
        21 => Ok(C::MapInsert),
        22 => Ok(C::MapGet),
        23 => Ok(C::MapRemove),
        24 => Ok(C::SharedNew),
        25 => Ok(C::SharedClone),
        26 => Ok(C::SharedRead),
        27 => Ok(C::SharedReplace),
        28 => Ok(C::WeakNew),
        29 => Ok(C::WeakClone),
        30 => Ok(C::WeakUpgrade),
        31 => Ok(C::ArenaNew),
        32 => Ok(C::ArenaRead),
        33 => Ok(C::ArenaReplace),
        34 => Ok(C::OptionUnwrap),
        35 => Ok(C::ResultUnwrap),
        36 => Ok(C::ResultUnwrapErr),
        value => Err(error(
            offset,
            CodecErrorKind::InvalidTag {
                tag: Tag::CoreOperation,
                value,
            },
        )),
    }
}

pub(super) fn task_builtin(value: B) -> Option<u8> {
    match value {
        B::TaskError => Some(10),
        B::ChannelError => Some(11),
        B::ServiceError => Some(12),
        _ => builtin(value),
    }
}
pub(super) fn read_task_builtin(value: u8, offset: usize) -> Result<B, CodecError> {
    match value {
        10 => Ok(B::TaskError),
        11 => Ok(B::ChannelError),
        12 => Ok(B::ServiceError),
        _ => read_builtin(value, offset),
    }
}
pub(super) fn task_core(value: C) -> Option<u8> {
    match value {
        C::ServiceNew => Some(37),
        C::ServiceHandle => Some(38),
        C::ServiceClone => Some(39),
        C::ServiceOwner => Some(40),
        C::ServiceRead => Some(41),
        C::ServiceReplace => Some(42),
        C::ServiceFinish => Some(43),
        C::ServiceCancel => Some(44),
        C::ChannelNew => Some(45),
        C::ChannelSender => Some(46),
        C::ChannelClone => Some(47),
        C::ChannelReceiver => Some(48),
        C::ChannelSeal => Some(49),
        C::ChannelUnsent => Some(50),
        _ => core(value),
    }
}
pub(super) fn read_task_core(value: u8, offset: usize) -> Result<C, CodecError> {
    match value {
        37 => Ok(C::ServiceNew),
        38 => Ok(C::ServiceHandle),
        39 => Ok(C::ServiceClone),
        40 => Ok(C::ServiceOwner),
        41 => Ok(C::ServiceRead),
        42 => Ok(C::ServiceReplace),
        43 => Ok(C::ServiceFinish),
        44 => Ok(C::ServiceCancel),
        45 => Ok(C::ChannelNew),
        46 => Ok(C::ChannelSender),
        47 => Ok(C::ChannelClone),
        48 => Ok(C::ChannelReceiver),
        49 => Ok(C::ChannelSeal),
        50 => Ok(C::ChannelUnsent),
        _ => read_core(value, offset),
    }
}
tags!(channel_role, read_channel_role, o::bodies::ChannelRole, Tag::ChannelRole, {
    0 => o::bodies::ChannelRole::Sender, 1 => o::bodies::ChannelRole::Receiver
});
tags!(service_role, read_service_role, o::bodies::ServiceRole, Tag::ServiceRole, {
    0 => o::bodies::ServiceRole::Handle, 1 => o::bodies::ServiceRole::Owner
});
