//! Allocation-free projection of existing module-admission failures.
use super::ffi;
use crate::{artifact, component_bindings as generated, component_embedding::Error};
use lang_mir::unit as m;
use lang_runtime::component::{idl, wire};

pub(super) struct Failure {
    pub status: u32,
    pub error: ffi::Error,
}
impl Failure {
    pub fn new(diagnostic: wire::Diagnostic) -> Self {
        Self {
            status: 2,
            error: ffi::Error {
                header: ffi::Header {
                    size: std::mem::size_of::<ffi::Error>() as u32,
                    version: 1,
                },
                diagnostic: diagnostic.c_id(),
                limit_kind: 0,
                limit: 0,
                attempted: 0,
                input_digest: [0; 32],
                source_id: 0,
                start: 0,
                end: 0,
                detail_code: 0,
                flags: 0,
                reserved0: 0,
                reserved: [0; 2],
            },
        }
    }
    pub fn limit(kind: wire::ReportLimitKind, limit: usize, attempted: usize) -> Self {
        let mut failure = Self::new(wire::Diagnostic::Limit);
        failure.error.limit_kind = kind as u32;
        failure.error.limit = limit as u64;
        failure.error.attempted = attempted as u64;
        failure
    }
    fn span(&mut self, source: u32, start: usize, end: usize) {
        if let (Ok(start), Ok(end)) = (u32::try_from(start), u32::try_from(end))
            && start <= end
        {
            self.error.source_id = source;
            self.error.start = start;
            self.error.end = end;
            self.error.flags |= 4;
        }
    }
    fn mir(&mut self, reason: &m::ErrorKind) {
        match reason {
            m::ErrorKind::Limit {
                kind,
                limit,
                requested,
            } => {
                let kind = match kind {
                    m::LimitKind::Work => wire::ReportLimitKind::PreparationWork,
                    m::LimitKind::Allocations
                    | m::LimitKind::SourceBytes
                    | m::LimitKind::TotalSourceBytes => wire::ReportLimitKind::MetadataBytes,
                    m::LimitKind::Depth => wire::ReportLimitKind::Depth,
                    _ => wire::ReportLimitKind::ValueElements,
                };
                let details = Self::limit(kind, *limit, *requested);
                self.error.diagnostic = details.error.diagnostic;
                self.error.limit_kind = details.error.limit_kind;
                self.error.limit = details.error.limit;
                self.error.attempted = details.error.attempted;
            }
            m::ErrorKind::AllocationFailed => self.status = 6,
            _ => {}
        }
    }
    fn idl(&mut self, reason: &idl::Reason) {
        match reason {
            idl::Reason::Budget(reason) => self.mir(reason),
            idl::Reason::Limit {
                kind,
                limit,
                requested,
            } => {
                let kind = match kind {
                    idl::LimitKind::Depth => wire::ReportLimitKind::Depth,
                    idl::LimitKind::SourceBytes | idl::LimitKind::StringBytes => {
                        wire::ReportLimitKind::MetadataBytes
                    }
                    _ => wire::ReportLimitKind::ValueElements,
                };
                let details = Self::limit(kind, *limit, *requested);
                self.error.diagnostic = details.error.diagnostic;
                self.error.limit_kind = details.error.limit_kind;
                self.error.limit = details.error.limit;
                self.error.attempted = details.error.attempted;
            }
            _ => {}
        }
    }
    fn wire(&mut self, reason: &wire::ErrorKind) {
        match reason {
            wire::ErrorKind::Version => {
                self.status = 3;
                self.error.diagnostic = wire::Diagnostic::UnsupportedVersion.c_id();
            }
            wire::ErrorKind::Budget(reason) => self.mir(reason),
            wire::ErrorKind::Declaration(reason) => self.idl(&reason.reason),
            wire::ErrorKind::Limit {
                kind,
                limit,
                requested,
            } => {
                let kind = match kind {
                    wire::LimitKind::Bytes | wire::LimitKind::StringBytes => {
                        wire::ReportLimitKind::MetadataBytes
                    }
                    wire::LimitKind::SerializationBytes => {
                        wire::ReportLimitKind::SerializationBytes
                    }
                    wire::LimitKind::Depth => wire::ReportLimitKind::Depth,
                    _ => wire::ReportLimitKind::ValueElements,
                };
                let details = Self::limit(kind, *limit, *requested);
                self.error.diagnostic = details.error.diagnostic;
                self.error.limit_kind = details.error.limit_kind;
                self.error.limit = details.error.limit;
                self.error.attempted = details.error.attempted;
            }
            _ => {}
        }
    }
}

pub(super) fn module(reason: &Error, digests: Option<([u8; 32], [u8; 32])>) -> Failure {
    let mut failure = Failure::new(wire::Diagnostic::InvalidMetadata);
    let kind = match reason {
        Error::Idl(_) | Error::Utf8(_) => 2,
        Error::Source(_) => 1,
        Error::Artifact(_) => 3,
        _ => 0,
    };
    if let Some((program, idl)) = digests
        && kind != 0
    {
        failure.error.flags = kind;
        failure.error.input_digest = if kind == 2 { idl } else { program };
    }
    match reason {
        Error::Format(_) | Error::ArtifactContract => {
            failure.error.diagnostic = wire::Diagnostic::InvalidRequest.c_id()
        }
        Error::InputBytes { limit, attempted } => {
            return Failure::limit(wire::ReportLimitKind::MetadataBytes, *limit, *attempted);
        }
        Error::Budget(reason) => failure.mir(&reason.kind),
        Error::Idl(reason) => {
            failure.idl(&reason.reason);
            failure.error.detail_code = reason.code.as_str()[3..].parse().unwrap_or(0);
            failure.span(
                reason.span.source.0,
                reason.span.start as usize,
                reason.span.end as usize,
            );
        }
        Error::Utf8(reason) => {
            failure.error.detail_code = 1;
            let start = reason.valid_up_to();
            failure.span(1, start, start + reason.error_len().unwrap_or(0));
        }
        Error::Source(analysis) => {
            if let Some(diagnostic) = analysis
                .diagnostics
                .iter()
                .find(|diagnostic| diagnostic.severity == lang_diagnostics::Severity::Error)
            {
                failure.error.detail_code = diagnostic
                    .code
                    .as_str()
                    .strip_prefix("SRC")
                    .and_then(|digits| digits.parse().ok())
                    .unwrap_or(0);
                let source = diagnostic.primary.span.source();
                let index = analysis
                    .code_sources
                    .iter()
                    .position(|original| *original == source)
                    .or_else(|| {
                        analysis
                            .sources
                            .iter()
                            .position(|(original, _)| original == source)
                    });
                if let Some(index) = index.and_then(|index| u32::try_from(index).ok()) {
                    let range = diagnostic.primary.span.range();
                    failure.span(index, range.start, range.end);
                }
            }
            if let Err(crate::frontend::component::Error::Bindings(reason)) = &analysis.result
                && let lang_runtime::component::bindings::ErrorKind::Budget(reason) = &reason.kind
            {
                failure.mir(reason);
            }
        }
        Error::Artifact(reason) => match reason {
            artifact::component::Error::Code(reason) => {
                use lang_bytecode::ownership::CodecErrorKind;
                failure.span(0, reason.offset, reason.offset);
                match &reason.kind {
                    CodecErrorKind::Budget(reason) => failure.mir(&reason.kind),
                    CodecErrorKind::TooLarge { limit, requested } => {
                        let details = Failure::limit(
                            wire::ReportLimitKind::MetadataBytes,
                            *limit,
                            *requested,
                        );
                        failure.error.diagnostic = wire::Diagnostic::Limit.c_id();
                        failure.error.limit_kind = details.error.limit_kind;
                        failure.error.limit = details.error.limit;
                        failure.error.attempted = details.error.attempted;
                    }
                    CodecErrorKind::Version(_) | CodecErrorKind::ProfileVersion(_) => {
                        failure.status = 3;
                        failure.error.diagnostic = wire::Diagnostic::UnsupportedVersion.c_id();
                    }
                    CodecErrorKind::AllocationFailed => failure.status = 6,
                    _ => {}
                }
            }
            artifact::component::Error::Metadata(reason) => {
                failure.wire(&reason.kind);
                failure.span(0, reason.offset, reason.offset);
            }
            _ => {}
        },
        Error::Generation(reason) => match reason {
            generated::Error::Budget(reason) => failure.mir(&reason.kind),
            generated::Error::Wire(reason) => failure.wire(&reason.kind),
            generated::Error::Allocation => failure.status = 6,
            _ => {}
        },
        Error::Wire(reason) => {
            failure.wire(&reason.kind);
            // This offset belongs to the inner wire view, not the entire
            // source/artifact input. Never publish a fabricated outer span.
        }
    }
    failure
}
