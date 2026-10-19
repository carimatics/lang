//! Explicit, bounded source-to-artifact compilation and version dispatch.
//! Embedded snapshots are diagnostics, never filesystem or execution authority.
#![allow(
    clippy::result_large_err,
    reason = "Keep structured failures inline after resource exhaustion."
)]

use crate::frontend::{self, CompileLimits, ScriptError};
use lang_bytecode::{ownership as owned, unit as copy};
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_semantics::analysis::Program as SourceProgram;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub copy: m::Limits,
    pub ownership: owned::VerificationLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024,
            copy: m::Limits::default(),
            ownership: owned::VerificationLimits::default(),
        }
    }
}
#[allow(
    clippy::large_enum_variant,
    reason = "Keep verified results inline; boxing would add an unbudgeted allocation after decoding."
)]
pub enum Program {
    Copy(copy::Program),
    Ownership(owned::Program),
}
#[derive(Debug)]
pub enum CompileError {
    Script(ScriptError),
    Copy(copy::CodecError),
    Ownership(owned::CodecError),
    MissingEntry,
}
impl Display for CompileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Script(e) => Display::fmt(e, f),
            Self::Copy(e) => Display::fmt(e, f),
            Self::Ownership(e) => Display::fmt(e, f),
            Self::MissingEntry => f.write_str("missing ownership MIR script entry"),
        }
    }
}
impl std::error::Error for CompileError {}
#[derive(Debug)]
pub enum DecodeError {
    Envelope(copy::CodecError),
    Copy(copy::CodecError),
    Ownership(owned::CodecError),
}
impl Display for DecodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Envelope(e) | Self::Copy(e) => Display::fmt(e, f),
            Self::Ownership(e) => Display::fmt(e, f),
        }
    }
}
impl std::error::Error for DecodeError {}

/// Compile a source script to its explicit profile version. The source entry
/// diagnostic is shared with ordinary source execution; no output is published
/// until the complete format verification and encoding have succeeded.
pub fn compile(
    program: SourceProgram,
    sources: &SourceMap,
    compile: CompileLimits,
    limits: Limits,
) -> Result<Vec<u8>, CompileError> {
    frontend::check_script_entry(program.source(), sources).map_err(CompileError::Script)?;
    match program {
        SourceProgram::Copy(program) => {
            let program = frontend::compile_script(&program, sources, compile)
                .map_err(CompileError::Script)?;
            copy::encode(
                &program,
                copy::CodecLimits {
                    bytes: limits.bytes,
                    validation: limits.copy,
                },
            )
            .map_err(CompileError::Copy)
        }
        SourceProgram::Ownership(program) => {
            let table = program
                .into_failure_flow()
                .into_source()
                .into_activity()
                .into_moved()
                .into_typed()
                .into_table();
            let entry = table
                .places
                .declarations
                .functions
                .iter()
                .position(|f| f.name == "main")
                .map(|i| m::FunctionId(i as u32))
                .ok_or(CompileError::MissingEntry)?;
            owned::encode(
                table,
                Some(entry),
                owned::CodecLimits {
                    bytes: limits.bytes,
                    verification: limits.ownership,
                },
            )
            .map_err(CompileError::Ownership)
        }
    }
}

/// Decode exactly the declared profile. There is no source fallback, snapshot
/// reparse, native loading, implicit import binding or allocator authority.
pub fn decode(bytes: &[u8], limits: Limits) -> Result<Program, DecodeError> {
    let envelope = |offset, kind| DecodeError::Envelope(copy::CodecError { offset, kind });
    if bytes.len() > limits.bytes {
        return Err(envelope(
            0,
            copy::CodecErrorKind::TooLarge {
                limit: limits.bytes,
                requested: bytes.len(),
            },
        ));
    }
    if bytes.len() < 16 {
        return Err(envelope(bytes.len(), copy::CodecErrorKind::Truncated));
    }
    if &bytes[..8] != b"LANGBC\0\0" {
        return Err(envelope(0, copy::CodecErrorKind::BadMagic));
    }
    let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    match version {
        1 => copy::decode(
            bytes,
            copy::CodecLimits {
                bytes: limits.bytes,
                validation: limits.copy,
            },
        )
        .map(Program::Copy)
        .map_err(DecodeError::Copy),
        2 | 3 => owned::decode(
            bytes,
            owned::CodecLimits {
                bytes: limits.bytes,
                verification: limits.ownership,
            },
        )
        .map(Program::Ownership)
        .map_err(DecodeError::Ownership),
        value => Err(envelope(8, copy::CodecErrorKind::Version(value))),
    }
}
