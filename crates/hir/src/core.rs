//! The finite RFC 0007/0008/0009 core operation signatures. These backend-independent
//! templates describe source types and result provenance; they do not grant
//! allocator authority, implement an operation, or establish body validity.

use crate::resolved::{BuiltinType, LoanKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypePattern {
    I64,
    Bool,
    Builtin(BuiltinType),
    Argument(u8),
    Option(&'static Self),
    Result(&'static Self, &'static Self),
    Vec(&'static Self),
    Map(&'static Self, &'static Self),
    Shared(&'static Self),
    Weak(&'static Self),
    Arena(&'static Self),
    Loan(LoanKind, &'static Self),
    /// Internal compiler-created lexical owner, never a public source type.
    ArenaOwner,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeArgumentRule {
    /// No ordinary loan or allocator authority; arena brands remain tracked.
    Storable,
    /// Any valid result type; divergence never creates a value of it.
    ReturnType,
    /// Exactly i64 or core::Text.
    MapKey,
    /// Storable, and contains no arena brands through any referent.
    SharedPayload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultBinding {
    Value,
    /// Direct authority loan supplied by execution-instance preflight.
    InstanceAllocator,
    /// Borrow-bearing Option/Result: only immediate matching is permitted.
    /// A successful binding borrows this argument's owner; unused/error arms
    /// end the provisional loan. This is not a storable user sum type.
    ProvisionalLoan {
        parameter: usize,
    },
    /// An arena token branded by the compiler-created owner argument.
    ArenaReference {
        owner: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Signature {
    pub type_arguments: &'static [TypeArgumentRule],
    pub parameters: &'static [TypePattern],
    pub result: TypePattern,
    pub result_binding: ResultBinding,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Family {
    Option,
    Result,
    AllocError,
    BoundsError,
    BufferError,
    Utf8Error,
    OwnershipError,
    ArenaError,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Case {
    None,
    Some,
    Ok,
    Err,
    AllocLimit,
    AllocOutOfMemory,
    AllocOverflow,
    OutOfBounds,
    BufferBounds,
    InvalidByte,
    BufferAllocation,
    InvalidUtf8,
    OwnershipAllocation,
    StrongCycle,
    ExpiredWeak,
    BorrowConflict,
    Unavailable,
    CountOverflow,
    ArenaAllocation,
    CrossRegion,
    ArenaUnavailable,
    ArenaBorrowConflict,
}

impl Family {
    pub const ALL: [Self; 8] = [
        Self::Option,
        Self::Result,
        Self::AllocError,
        Self::BoundsError,
        Self::BufferError,
        Self::Utf8Error,
        Self::OwnershipError,
        Self::ArenaError,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Option => "Option",
            Self::Result => "Result",
            Self::AllocError => "AllocError",
            Self::BoundsError => "BoundsError",
            Self::BufferError => "BufferError",
            Self::Utf8Error => "Utf8Error",
            Self::OwnershipError => "OwnershipError",
            Self::ArenaError => "ArenaError",
        }
    }
    pub const fn type_arguments(self) -> usize {
        match self {
            Self::Option => 1,
            Self::Result => 2,
            _ => 0,
        }
    }
    pub fn cases(self) -> &'static [Case] {
        use Case as C;
        match self {
            Self::Option => &[C::None, C::Some],
            Self::Result => &[C::Ok, C::Err],
            Self::AllocError => &[C::AllocLimit, C::AllocOutOfMemory, C::AllocOverflow],
            Self::BoundsError => &[C::OutOfBounds],
            Self::BufferError => &[C::BufferBounds, C::InvalidByte, C::BufferAllocation],
            Self::Utf8Error => &[C::InvalidUtf8],
            Self::OwnershipError => &[
                C::OwnershipAllocation,
                C::StrongCycle,
                C::ExpiredWeak,
                C::BorrowConflict,
                C::Unavailable,
                C::CountOverflow,
            ],
            Self::ArenaError => &[
                C::ArenaAllocation,
                C::CrossRegion,
                C::ArenaUnavailable,
                C::ArenaBorrowConflict,
            ],
        }
    }
}

impl Case {
    pub const ALL: [Self; 22] = [
        Self::None,
        Self::Some,
        Self::Ok,
        Self::Err,
        Self::AllocLimit,
        Self::AllocOutOfMemory,
        Self::AllocOverflow,
        Self::OutOfBounds,
        Self::BufferBounds,
        Self::InvalidByte,
        Self::BufferAllocation,
        Self::InvalidUtf8,
        Self::OwnershipAllocation,
        Self::StrongCycle,
        Self::ExpiredWeak,
        Self::BorrowConflict,
        Self::Unavailable,
        Self::CountOverflow,
        Self::ArenaAllocation,
        Self::CrossRegion,
        Self::ArenaUnavailable,
        Self::ArenaBorrowConflict,
    ];
    pub const fn family(self) -> Family {
        match self {
            Self::None | Self::Some => Family::Option,
            Self::Ok | Self::Err => Family::Result,
            Self::AllocLimit | Self::AllocOutOfMemory | Self::AllocOverflow => Family::AllocError,
            Self::OutOfBounds => Family::BoundsError,
            Self::BufferBounds | Self::InvalidByte | Self::BufferAllocation => Family::BufferError,
            Self::InvalidUtf8 => Family::Utf8Error,
            Self::OwnershipAllocation
            | Self::StrongCycle
            | Self::ExpiredWeak
            | Self::BorrowConflict
            | Self::Unavailable
            | Self::CountOverflow => Family::OwnershipError,
            Self::ArenaAllocation
            | Self::CrossRegion
            | Self::ArenaUnavailable
            | Self::ArenaBorrowConflict => Family::ArenaError,
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Some => "Some",
            Self::Ok => "Ok",
            Self::Err => "Err",
            Self::AllocLimit => "Limit",
            Self::AllocOutOfMemory => "OutOfMemory",
            Self::AllocOverflow => "Overflow",
            Self::OutOfBounds => "OutOfBounds",
            Self::BufferBounds => "Bounds",
            Self::InvalidByte => "InvalidByte",
            Self::BufferAllocation | Self::OwnershipAllocation | Self::ArenaAllocation => {
                "Allocation"
            }
            Self::InvalidUtf8 => "Invalid",
            Self::StrongCycle => "StrongCycle",
            Self::ExpiredWeak => "ExpiredWeak",
            Self::BorrowConflict | Self::ArenaBorrowConflict => "BorrowConflict",
            Self::Unavailable | Self::ArenaUnavailable => "Unavailable",
            Self::CountOverflow => "CountOverflow",
            Self::CrossRegion => "CrossRegion",
        }
    }
    pub fn from_path(family: &str, name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|case| case.family().name() == family && case.name() == name)
    }
    /// Option/Result payload arguments are inferred from expected source
    /// context or the match scrutinee, never from explicit case type arguments.
    pub fn payload(self) -> &'static [TypePattern] {
        match self {
            Self::Some | Self::Ok => &[T],
            Self::Err => &[V],
            Self::BufferAllocation | Self::OwnershipAllocation | Self::ArenaAllocation => {
                &[ALLOC_ERROR]
            }
            Self::InvalidUtf8 => &[TypePattern::I64],
            _ => &[],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Operation {
    /// Source-only RFC 0008 intrinsic, lowered to ordinary MIR cleanup.
    Drop,
    /// Source-only terminal intrinsic with a dynamic signed code.
    Panic,
    Allocator,
    TextFromUtf8,
    TextClone,
    TextLen,
    TextBytes,
    TextEqual,
    TextConcat,
    BytesNew,
    BytesClone,
    BytesLen,
    BytesGet,
    BytesPush,
    BytesSet,
    VecNew,
    VecLen,
    VecPush,
    VecPop,
    VecGet,
    VecGetMut,
    MapNew,
    MapLen,
    MapInsert,
    MapGet,
    MapRemove,
    SharedNew,
    SharedClone,
    SharedRead,
    SharedReplace,
    WeakNew,
    WeakClone,
    WeakUpgrade,
    ArenaNew,
    ArenaRead,
    ArenaReplace,
    OptionUnwrap,
    ResultUnwrap,
    ResultUnwrapErr,
}

const T: TypePattern = TypePattern::Argument(0);
const V: TypePattern = TypePattern::Argument(1);
const UNIT: TypePattern = TypePattern::Builtin(BuiltinType::Unit);
const TEXT: TypePattern = TypePattern::Builtin(BuiltinType::Text);
const BYTES: TypePattern = TypePattern::Builtin(BuiltinType::Bytes);
const ALLOCATOR: TypePattern = TypePattern::Builtin(BuiltinType::Allocator);
const ALLOC_ERROR: TypePattern = TypePattern::Builtin(BuiltinType::AllocError);
const BOUNDS_ERROR: TypePattern = TypePattern::Builtin(BuiltinType::BoundsError);
const BUFFER_ERROR: TypePattern = TypePattern::Builtin(BuiltinType::BufferError);
const UTF8_ERROR: TypePattern = TypePattern::Builtin(BuiltinType::Utf8Error);
const OWNERSHIP_ERROR: TypePattern = TypePattern::Builtin(BuiltinType::OwnershipError);
const ARENA_ERROR: TypePattern = TypePattern::Builtin(BuiltinType::ArenaError);
const VEC: TypePattern = TypePattern::Vec(&T);
const MAP: TypePattern = TypePattern::Map(&T, &V);
const SHARED: TypePattern = TypePattern::Shared(&T);
const WEAK: TypePattern = TypePattern::Weak(&T);
const ARENA: TypePattern = TypePattern::Arena(&T);
const OWNER: TypePattern = TypePattern::ArenaOwner;
const ONE: &[TypeArgumentRule] = &[TypeArgumentRule::Storable];
const TWO: &[TypeArgumentRule] = &[TypeArgumentRule::Storable; 2];
const SHARED_ONE: &[TypeArgumentRule] = &[TypeArgumentRule::SharedPayload];
const MAP_TWO: &[TypeArgumentRule] = &[TypeArgumentRule::MapKey, TypeArgumentRule::Storable];

const fn signature(
    type_arguments: &'static [TypeArgumentRule],
    parameters: &'static [TypePattern],
    result: TypePattern,
) -> Signature {
    Signature {
        type_arguments,
        parameters,
        result,
        result_binding: ResultBinding::Value,
    }
}

impl Operation {
    pub const ALL: [Self; 39] = [
        Self::Drop,
        Self::Panic,
        Self::Allocator,
        Self::TextFromUtf8,
        Self::TextClone,
        Self::TextLen,
        Self::TextBytes,
        Self::TextEqual,
        Self::TextConcat,
        Self::BytesNew,
        Self::BytesClone,
        Self::BytesLen,
        Self::BytesGet,
        Self::BytesPush,
        Self::BytesSet,
        Self::VecNew,
        Self::VecLen,
        Self::VecPush,
        Self::VecPop,
        Self::VecGet,
        Self::VecGetMut,
        Self::MapNew,
        Self::MapLen,
        Self::MapInsert,
        Self::MapGet,
        Self::MapRemove,
        Self::SharedNew,
        Self::SharedClone,
        Self::SharedRead,
        Self::SharedReplace,
        Self::WeakNew,
        Self::WeakClone,
        Self::WeakUpgrade,
        Self::ArenaNew,
        Self::ArenaRead,
        Self::ArenaReplace,
        Self::OptionUnwrap,
        Self::ResultUnwrap,
        Self::ResultUnwrapErr,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Drop => "drop",
            Self::Panic => "panic",
            Self::Allocator => "allocator",
            Self::TextFromUtf8 => "text_from_utf8",
            Self::TextClone => "text_clone",
            Self::TextLen => "text_len",
            Self::TextBytes => "text_bytes",
            Self::TextEqual => "text_equal",
            Self::TextConcat => "text_concat",
            Self::BytesNew => "bytes_new",
            Self::BytesClone => "bytes_clone",
            Self::BytesLen => "bytes_len",
            Self::BytesGet => "bytes_get",
            Self::BytesPush => "bytes_push",
            Self::BytesSet => "bytes_set",
            Self::VecNew => "vec_new",
            Self::VecLen => "vec_len",
            Self::VecPush => "vec_push",
            Self::VecPop => "vec_pop",
            Self::VecGet => "vec_get",
            Self::VecGetMut => "vec_get_mut",
            Self::MapNew => "map_new",
            Self::MapLen => "map_len",
            Self::MapInsert => "map_insert",
            Self::MapGet => "map_get",
            Self::MapRemove => "map_remove",
            Self::SharedNew => "shared_new",
            Self::SharedClone => "shared_clone",
            Self::SharedRead => "shared_read",
            Self::SharedReplace => "shared_replace",
            Self::WeakNew => "weak_new",
            Self::WeakClone => "weak_clone",
            Self::WeakUpgrade => "weak_upgrade",
            Self::ArenaNew => "arena_new",
            Self::ArenaRead => "arena_read",
            Self::ArenaReplace => "arena_replace",
            Self::OptionUnwrap => "option_unwrap",
            Self::ResultUnwrap => "result_unwrap",
            Self::ResultUnwrapErr => "result_unwrap_err",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.name() == name)
    }

    pub fn signature(self) -> Signature {
        use LoanKind::{Exclusive as M, Shared as S};
        use TypePattern as P;
        let mut sig = match self {
            Self::Drop => signature(ONE, &[T], UNIT),
            Self::Panic => signature(&[TypeArgumentRule::ReturnType], &[TypePattern::I64], T),
            Self::OptionUnwrap => signature(ONE, &[P::Option(&T)], T),
            Self::ResultUnwrap => signature(TWO, &[P::Result(&T, &V)], T),
            Self::ResultUnwrapErr => signature(TWO, &[P::Result(&T, &V)], V),
            Self::Allocator => signature(&[], &[], P::Loan(S, &ALLOCATOR)),
            Self::TextFromUtf8 => signature(&[], &[BYTES], P::Result(&TEXT, &UTF8_ERROR)),
            Self::TextClone => signature(
                &[],
                &[P::Loan(S, &ALLOCATOR), P::Loan(S, &TEXT)],
                P::Result(&TEXT, &ALLOC_ERROR),
            ),
            Self::TextLen => signature(&[], &[P::Loan(S, &TEXT)], P::I64),
            Self::TextBytes => signature(
                &[],
                &[P::Loan(S, &ALLOCATOR), P::Loan(S, &TEXT)],
                P::Result(&BYTES, &ALLOC_ERROR),
            ),
            Self::TextEqual => signature(&[], &[P::Loan(S, &TEXT), P::Loan(S, &TEXT)], P::Bool),
            Self::TextConcat => signature(
                &[],
                &[P::Loan(S, &ALLOCATOR), P::Loan(S, &TEXT), P::Loan(S, &TEXT)],
                P::Result(&TEXT, &ALLOC_ERROR),
            ),
            Self::BytesNew => signature(
                &[],
                &[P::Loan(S, &ALLOCATOR)],
                P::Result(&BYTES, &ALLOC_ERROR),
            ),
            Self::BytesClone => signature(
                &[],
                &[P::Loan(S, &ALLOCATOR), P::Loan(S, &BYTES)],
                P::Result(&BYTES, &ALLOC_ERROR),
            ),
            Self::BytesLen => signature(&[], &[P::Loan(S, &BYTES)], P::I64),
            Self::BytesGet => signature(
                &[],
                &[P::Loan(S, &BYTES), P::I64],
                P::Result(&P::I64, &BOUNDS_ERROR),
            ),
            Self::BytesPush => signature(
                &[],
                &[P::Loan(M, &BYTES), P::I64],
                P::Result(&UNIT, &BUFFER_ERROR),
            ),
            Self::BytesSet => signature(
                &[],
                &[P::Loan(M, &BYTES), P::I64, P::I64],
                P::Result(&UNIT, &BUFFER_ERROR),
            ),
            Self::VecNew => signature(
                ONE,
                &[P::Loan(S, &ALLOCATOR)],
                P::Result(&VEC, &ALLOC_ERROR),
            ),
            Self::VecLen => signature(ONE, &[P::Loan(S, &VEC)], P::I64),
            Self::VecPush => signature(ONE, &[P::Loan(M, &VEC), T], P::Result(&UNIT, &ALLOC_ERROR)),
            Self::VecPop => signature(ONE, &[P::Loan(M, &VEC)], P::Option(&T)),
            Self::VecGet => signature(
                ONE,
                &[P::Loan(S, &VEC), P::I64],
                P::Result(&P::Loan(S, &T), &BOUNDS_ERROR),
            ),
            Self::VecGetMut => signature(
                ONE,
                &[P::Loan(M, &VEC), P::I64],
                P::Result(&P::Loan(M, &T), &BOUNDS_ERROR),
            ),
            Self::MapNew => signature(
                MAP_TWO,
                &[P::Loan(S, &ALLOCATOR)],
                P::Result(&MAP, &ALLOC_ERROR),
            ),
            Self::MapLen => signature(MAP_TWO, &[P::Loan(S, &MAP)], P::I64),
            Self::MapInsert => signature(
                MAP_TWO,
                &[P::Loan(M, &MAP), T, V],
                P::Result(&P::Option(&V), &ALLOC_ERROR),
            ),
            Self::MapGet => signature(
                MAP_TWO,
                &[P::Loan(S, &MAP), P::Loan(S, &T)],
                P::Option(&P::Loan(S, &V)),
            ),
            Self::MapRemove => {
                signature(MAP_TWO, &[P::Loan(M, &MAP), P::Loan(S, &T)], P::Option(&V))
            }
            Self::SharedNew => signature(
                SHARED_ONE,
                &[P::Loan(S, &ALLOCATOR), T],
                P::Result(&SHARED, &OWNERSHIP_ERROR),
            ),
            Self::SharedClone => signature(
                SHARED_ONE,
                &[P::Loan(S, &SHARED)],
                P::Result(&SHARED, &OWNERSHIP_ERROR),
            ),
            Self::SharedRead => signature(
                SHARED_ONE,
                &[P::Loan(S, &SHARED)],
                P::Result(&P::Loan(S, &T), &OWNERSHIP_ERROR),
            ),
            Self::SharedReplace => signature(
                SHARED_ONE,
                &[P::Loan(M, &SHARED), T],
                P::Result(&UNIT, &OWNERSHIP_ERROR),
            ),
            Self::WeakNew => signature(
                SHARED_ONE,
                &[P::Loan(S, &SHARED)],
                P::Result(&WEAK, &OWNERSHIP_ERROR),
            ),
            Self::WeakClone => signature(
                SHARED_ONE,
                &[P::Loan(S, &WEAK)],
                P::Result(&WEAK, &OWNERSHIP_ERROR),
            ),
            Self::WeakUpgrade => signature(
                SHARED_ONE,
                &[P::Loan(S, &WEAK)],
                P::Result(&SHARED, &OWNERSHIP_ERROR),
            ),
            Self::ArenaNew => signature(
                ONE,
                &[P::Loan(M, &OWNER), T],
                P::Result(&ARENA, &ARENA_ERROR),
            ),
            Self::ArenaRead => signature(
                ONE,
                &[P::Loan(S, &OWNER), ARENA],
                P::Result(&P::Loan(S, &T), &ARENA_ERROR),
            ),
            Self::ArenaReplace => signature(
                ONE,
                &[P::Loan(M, &OWNER), ARENA, T],
                P::Result(&UNIT, &ARENA_ERROR),
            ),
        };
        sig.result_binding = match self {
            Self::Allocator => ResultBinding::InstanceAllocator,
            Self::VecGet | Self::VecGetMut | Self::MapGet | Self::SharedRead | Self::ArenaRead => {
                ResultBinding::ProvisionalLoan { parameter: 0 }
            }
            Self::ArenaNew => ResultBinding::ArenaReference { owner: 0 },
            _ => ResultBinding::Value,
        };
        sig
    }
}
