//! Finite RFC 0007/0009 core-call types and result provenance. No runtime operation,
//! allocator callback, source ABI, or serialized opcode is defined here.

use super::bodies::{LoanId, Site};
use super::{ArenaOwner, Builtin, LoanKind, TypeUse};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypePattern {
    I64,
    Bool,
    Builtin(Builtin),
    Argument(u8),
    ServiceSetup(&'static Self, &'static Self),
    ServiceOwner(&'static Self, &'static Self),
    ServiceHandle(&'static Self),
    ChannelSetup(&'static Self),
    Sender(&'static Self),
    Receiver(&'static Self),
    SendResult(&'static Self),
    Option(&'static Self),
    Result(&'static Self, &'static Self),
    Vec(&'static Self),
    Map(&'static Self, &'static Self),
    Shared(&'static Self),
    Weak(&'static Self),
    Arena(&'static Self),
    Loan(LoanKind, &'static Self),
    ArenaOwner,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeArgumentRule {
    Storable,
    /// Independently derived logical-task message eligibility.
    Send,
    MapKey,
    SharedPayload,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultBinding {
    Value,
    InstanceAllocator,
    ProvisionalLoan {
        parameter: usize,
        kind: LoanKind,
        success: u32,
    },
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Call {
    pub operation: Operation,
    pub type_arguments: Vec<TypeUse>,
    pub arguments: Vec<super::super::Register>,
    pub result_loan: Option<LoanId>,
}
impl Call {
    pub fn loan_kind(&self) -> Option<LoanKind> {
        match self.operation.signature().result_binding {
            ResultBinding::InstanceAllocator => Some(LoanKind::Shared),
            ResultBinding::ProvisionalLoan { kind, .. } => Some(kind),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Position {
    Argument(usize),
    Result,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckedCall {
    pub result_binding: ResultBinding,
    pub arena_owner: Option<ArenaOwner>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fact {
    pub site: Site,
    pub operation: Operation,
    pub checked: CheckedCall,
}

mod validation;
pub use validation::check;

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
    OptionUnwrap,
    ResultUnwrap,
    ResultUnwrapErr,
    ServiceNew,
    ServiceHandle,
    ServiceClone,
    ServiceOwner,
    ServiceRead,
    ServiceReplace,
    ServiceFinish,
    ServiceCancel,
    ChannelNew,
    ChannelSender,
    ChannelClone,
    ChannelReceiver,
    ChannelSeal,
    ChannelUnsent,
}

/// A checked whole-sum extraction: the other valid case raises this guest panic.
/// Neither this contract nor an operation's enum ordinal defines a wire tag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Extraction {
    pub case: u32,
    pub panic_code: i64,
}

const T: TypePattern = TypePattern::Argument(0);
const V: TypePattern = TypePattern::Argument(1);
const UNIT: TypePattern = TypePattern::Builtin(Builtin::Unit);
const TEXT: TypePattern = TypePattern::Builtin(Builtin::Text);
const BYTES: TypePattern = TypePattern::Builtin(Builtin::Bytes);
const ALLOCATOR: TypePattern = TypePattern::Builtin(Builtin::Allocator);
const ALLOC_ERROR: TypePattern = TypePattern::Builtin(Builtin::AllocError);
const BOUNDS_ERROR: TypePattern = TypePattern::Builtin(Builtin::BoundsError);
const BUFFER_ERROR: TypePattern = TypePattern::Builtin(Builtin::BufferError);
const UTF8_ERROR: TypePattern = TypePattern::Builtin(Builtin::Utf8Error);
const OWNERSHIP_ERROR: TypePattern = TypePattern::Builtin(Builtin::OwnershipError);
const ARENA_ERROR: TypePattern = TypePattern::Builtin(Builtin::ArenaError);
const VEC: TypePattern = TypePattern::Vec(&T);
const MAP: TypePattern = TypePattern::Map(&T, &V);
const SHARED: TypePattern = TypePattern::Shared(&T);
const WEAK: TypePattern = TypePattern::Weak(&T);
const ARENA: TypePattern = TypePattern::Arena(&T);
const OWNER: TypePattern = TypePattern::ArenaOwner;
const CHANNEL_ERROR: TypePattern = TypePattern::Builtin(Builtin::ChannelError);
const SETUP: TypePattern = TypePattern::ChannelSetup(&T);
const SENDER: TypePattern = TypePattern::Sender(&T);
const RECEIVER: TypePattern = TypePattern::Receiver(&T);
const SEND_RESULT: TypePattern = TypePattern::SendResult(&T);
const SERVICE_SETUP: TypePattern = TypePattern::ServiceSetup(&T, &V);
const SERVICE_OWNER: TypePattern = TypePattern::ServiceOwner(&T, &V);
const SERVICE_HANDLE: TypePattern = TypePattern::ServiceHandle(&T);
const SERVICE_ERROR: TypePattern = TypePattern::Builtin(Builtin::ServiceError);
const SEND_TWO: &[TypeArgumentRule] = &[TypeArgumentRule::Send; 2];
const SEND_ONE: &[TypeArgumentRule] = &[TypeArgumentRule::Send];
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
    pub const ALL: [Self; 51] = [
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
        Self::ServiceNew,
        Self::ServiceHandle,
        Self::ServiceClone,
        Self::ServiceOwner,
        Self::ServiceRead,
        Self::ServiceReplace,
        Self::ServiceFinish,
        Self::ServiceCancel,
        Self::ChannelNew,
        Self::ChannelSender,
        Self::ChannelClone,
        Self::ChannelReceiver,
        Self::ChannelSeal,
        Self::ChannelUnsent,
    ];

    pub const fn is_service(self) -> bool {
        matches!(
            self,
            Self::ServiceNew
                | Self::ServiceHandle
                | Self::ServiceClone
                | Self::ServiceOwner
                | Self::ServiceRead
                | Self::ServiceReplace
                | Self::ServiceFinish
                | Self::ServiceCancel
        )
    }
    pub const fn is_channel(self) -> bool {
        matches!(
            self,
            Self::ChannelNew
                | Self::ChannelSender
                | Self::ChannelClone
                | Self::ChannelReceiver
                | Self::ChannelSeal
                | Self::ChannelUnsent
        )
    }
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
            Self::OptionUnwrap => "option_unwrap",
            Self::ResultUnwrap => "result_unwrap",
            Self::ResultUnwrapErr => "result_unwrap_err",
            Self::ServiceNew => "service_new",
            Self::ServiceHandle => "service_handle",
            Self::ServiceClone => "service_clone",
            Self::ServiceOwner => "service_owner",
            Self::ServiceRead => "service_read",
            Self::ServiceReplace => "service_replace",
            Self::ServiceFinish => "service_finish",
            Self::ServiceCancel => "service_cancel",
            Self::ChannelNew => "channel_new",
            Self::ChannelSender => "channel_sender",
            Self::ChannelClone => "channel_clone",
            Self::ChannelReceiver => "channel_receiver",
            Self::ChannelSeal => "channel_seal",
            Self::ChannelUnsent => "channel_unsent",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.name() == name)
    }

    pub const fn extraction(self) -> Option<Extraction> {
        match self {
            Self::OptionUnwrap => Some(Extraction {
                case: 1,
                panic_code: -1,
            }),
            Self::ResultUnwrap => Some(Extraction {
                case: 0,
                panic_code: -2,
            }),
            Self::ResultUnwrapErr => Some(Extraction {
                case: 1,
                panic_code: -3,
            }),
            _ => None,
        }
    }

    pub fn signature(self) -> Signature {
        use LoanKind::{Exclusive as M, Shared as S};
        use TypePattern as P;
        let mut sig = match self {
            Self::ServiceNew => signature(
                SEND_TWO,
                &[P::Loan(S, &ALLOCATOR), P::I64, T],
                P::Result(&SERVICE_SETUP, &SERVICE_ERROR),
            ),
            Self::ServiceHandle => signature(
                SEND_TWO,
                &[P::Loan(S, &SERVICE_SETUP)],
                P::ServiceHandle(&V),
            ),
            Self::ServiceClone => {
                signature(SEND_ONE, &[P::Loan(S, &SERVICE_HANDLE)], SERVICE_HANDLE)
            }
            Self::ServiceOwner => signature(SEND_TWO, &[SERVICE_SETUP], SERVICE_OWNER),
            Self::ServiceRead => signature(
                SEND_TWO,
                &[P::Loan(S, &SERVICE_OWNER)],
                P::Result(&P::Loan(S, &T), &SERVICE_ERROR),
            ),
            Self::ServiceReplace => signature(
                SEND_TWO,
                &[P::Loan(M, &SERVICE_OWNER), T],
                P::Result(&UNIT, &SERVICE_ERROR),
            ),
            Self::ServiceFinish => signature(SEND_TWO, &[P::Loan(M, &SERVICE_OWNER)], UNIT),
            Self::ServiceCancel => signature(SEND_ONE, &[P::Loan(S, &SERVICE_HANDLE)], UNIT),

            Self::ChannelNew => signature(
                SEND_ONE,
                &[P::Loan(S, &ALLOCATOR), P::I64],
                P::Result(&SETUP, &CHANNEL_ERROR),
            ),
            Self::ChannelSender => signature(SEND_ONE, &[P::Loan(S, &SETUP)], SENDER),
            Self::ChannelClone => signature(SEND_ONE, &[P::Loan(S, &SENDER)], SENDER),
            Self::ChannelReceiver => signature(SEND_ONE, &[SETUP], RECEIVER),
            Self::ChannelSeal => signature(SEND_ONE, &[P::Loan(M, &RECEIVER)], UNIT),
            Self::ChannelUnsent => signature(ONE, &[SEND_RESULT], T),
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
            Self::VecGet | Self::SharedRead | Self::ArenaRead | Self::ServiceRead => {
                ResultBinding::ProvisionalLoan {
                    parameter: 0,
                    kind: S,
                    success: 0,
                }
            }
            Self::VecGetMut => ResultBinding::ProvisionalLoan {
                parameter: 0,
                kind: M,
                success: 0,
            },
            Self::MapGet => ResultBinding::ProvisionalLoan {
                parameter: 0,
                kind: S,
                success: 1,
            },
            Self::ArenaNew => ResultBinding::ArenaReference { owner: 0 },
            _ => ResultBinding::Value,
        };
        sig
    }
}
