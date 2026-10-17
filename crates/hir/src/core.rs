//! The finite RFC 0007 core operation signatures. These backend-independent
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
pub enum Operation {
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
    pub const ALL: [Self; 34] = [
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
    ];

    pub const fn name(self) -> &'static str {
        match self {
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
