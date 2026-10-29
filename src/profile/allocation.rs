//! Complete logical allocator ledgers reconciled at original entry return.
mod command;
pub use command::run;

use super::{
    MAX_REPORT_BYTES,
    runtime::{Engine, Profile, Source, Usage},
};
use crate::package::Diagnostic;
use lang_codegen_clif::Triple;
use lang_runtime::allocation as a;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{self, Write},
};

fn error(message: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("PRF0003", message.to_string())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    AllocationTrace,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Measurement {
    LogicalRequestedStorage,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Boundary {
    EntryReturn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub bytes: u64,
    pub alignment: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Statistics {
    pub live_bytes: u64,
    pub cumulative_bytes: u64,
    pub peak_live_bytes: u64,
    pub live_allocations: u64,
    pub peak_live_allocations: u64,
    pub requests: u64,
    pub allocations: u64,
    pub deallocations: u64,
}
impl From<a::Stats> for Statistics {
    fn from(s: a::Stats) -> Self {
        Self {
            live_bytes: s.live_bytes,
            cumulative_bytes: s.cumulative_bytes,
            peak_live_bytes: s.peak_live_bytes,
            live_allocations: s.live_allocations as u64,
            peak_live_allocations: s.peak_live_allocations as u64,
            requests: s.requests,
            allocations: s.allocations,
            deallocations: s.deallocations,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    OutOfMemory,
    InvalidStorage,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Allocated { generation: u64 },
    AllocationFailed { reason: Failure },
    Retired { generation: u64 },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub request: Request,
    pub operation: Operation,
    pub stats: Statistics,
}
impl From<a::Event> for Event {
    fn from(e: a::Event) -> Self {
        let operation = match e.kind {
            a::EventKind::Allocated { generation } => Operation::Allocated { generation },
            a::EventKind::Retired { generation } => Operation::Retired { generation },
            a::EventKind::AllocationFailed(reason) => Operation::AllocationFailed {
                reason: match reason {
                    a::FailureKind::OutOfMemory => Failure::OutOfMemory,
                    a::FailureKind::InvalidStorage => Failure::InvalidStorage,
                },
            },
        };
        Self {
            request: Request {
                bytes: e.request.bytes(),
                alignment: e.request.alignment(),
            },
            operation,
            stats: e.stats.into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    pub source: u32,
    pub start: u32,
    pub end: u32,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Completed {
        result: i64,
    },
    Failed {
        message: String,
        fault: Option<Fault>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema: u32,
    pub kind: Kind,
    pub compiler_id: String,
    pub target: String,
    pub input_digest: String,
    pub engine: Engine,
    pub profile: Profile,
    pub measurement: Measurement,
    pub boundary: Boundary,
    pub allocator: u64,
    pub sources: Vec<Source>,
    pub usage: Usage,
    pub outcome: Outcome,
    pub events: Vec<Event>,
    pub stats: Statistics,
}

fn add(a: u64, b: u64) -> Result<u64, Diagnostic> {
    a.checked_add(b)
        .ok_or_else(|| error("allocation trace counter overflow"))
}
impl Report {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.schema != 1
            || !super::digest(&self.compiler_id)
            || !super::digest(&self.input_digest)
            || self.target.len() > 256
            || self
                .target
                .parse::<Triple>()
                .ok()
                .is_none_or(|t| t.to_string() != self.target)
            || self.profile != Profile::Ownership
            || self.allocator == 0
            || self.usage.host_calls != 0
            || self.sources.len() > 4096
            || self.events.len() > a::MAX_TRACE_EVENTS
        {
            return Err(error(
                "unsupported allocation trace identity, mode or counts",
            ));
        }
        let mut sources = BTreeMap::new();
        let mut source_bytes = 0_u64;
        let mut previous = None;
        for source in &self.sources {
            if source.id >= 4096
                || previous.is_some_and(|id| id >= source.id)
                || !super::runtime::portable_path(&source.path)
                || !super::digest(&source.digest)
            {
                return Err(error("invalid allocation trace source"));
            }
            source_bytes = add(source_bytes, source.bytes)?;
            if source_bytes > 32 * 1024 * 1024 {
                return Err(error("allocation trace source limit exceeded"));
            }
            sources.insert(source.id, source);
            previous = Some(source.id);
        }
        if let Outcome::Failed { message, fault } = &self.outcome {
            if message.is_empty() || message.len() > 8192 || message.contains('\0') {
                return Err(error("invalid allocation trace failure message"));
            }
            if let Some(fault) = fault {
                let source = sources
                    .get(&fault.source)
                    .ok_or_else(|| error("unknown allocation trace failure source"))?;
                if fault.start > fault.end || u64::from(fault.end) > source.bytes {
                    return Err(error("invalid allocation trace failure range"));
                }
            }
        }
        let mut stats = Statistics::default();
        let mut live = BTreeMap::new();
        let mut generation = 0;
        for event in &self.events {
            a::Request::new(event.request.bytes, event.request.alignment).map_err(error)?;
            match event.operation {
                Operation::Allocated { generation: next } => {
                    if next <= generation {
                        return Err(error("reused or unordered allocation generation"));
                    }
                    generation = next;
                    live.insert(next, event.request);
                    stats.requests = add(stats.requests, 1)?;
                    stats.cumulative_bytes = add(stats.cumulative_bytes, event.request.bytes)?;
                    stats.live_bytes = add(stats.live_bytes, event.request.bytes)?;
                    stats.live_allocations = add(stats.live_allocations, 1)?;
                    stats.allocations = add(stats.allocations, 1)?;
                    stats.peak_live_bytes = stats.peak_live_bytes.max(stats.live_bytes);
                    stats.peak_live_allocations =
                        stats.peak_live_allocations.max(stats.live_allocations);
                }
                Operation::AllocationFailed { .. } => {
                    if event.request.bytes == 0 {
                        return Err(error("zero-size allocation cannot fail in the provider"));
                    }
                    stats.requests = add(stats.requests, 1)?;
                    stats.cumulative_bytes = add(stats.cumulative_bytes, event.request.bytes)?;
                }
                Operation::Retired { generation } => {
                    if live.remove(&generation) != Some(event.request) {
                        return Err(error("unknown retirement or changed allocation request"));
                    }
                    stats.live_bytes = stats
                        .live_bytes
                        .checked_sub(event.request.bytes)
                        .ok_or_else(|| error("allocation trace live-byte underflow"))?;
                    stats.live_allocations = stats
                        .live_allocations
                        .checked_sub(1)
                        .ok_or_else(|| error("allocation trace live-count underflow"))?;
                    stats.deallocations = add(stats.deallocations, 1)?;
                }
            }
            if event.stats != stats {
                return Err(error("allocation event counters do not reconcile"));
            }
        }
        if self.stats != stats {
            return Err(error("final allocation counters do not reconcile"));
        }
        Ok(())
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Diagnostic> {
        if bytes.len() > MAX_REPORT_BYTES {
            return Err(error("allocation report input limit exceeded"));
        }
        let report: Self =
            serde_json::from_slice(bytes).map_err(|_| error("invalid allocation trace JSON"))?;
        report.validate()?;
        Ok(report)
    }
    pub fn encode(&self) -> Result<Vec<u8>, Diagnostic> {
        self.validate()?;
        struct Buffer(Vec<u8>);
        impl Write for Buffer {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.0.len().saturating_add(bytes.len()) > MAX_REPORT_BYTES {
                    return Err(io::Error::other("allocation report output limit"));
                }
                self.0
                    .try_reserve_exact(bytes.len())
                    .map_err(io::Error::other)?;
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut buffer = Buffer(Vec::new());
        serde_json::to_writer(&mut buffer, self)
            .map_err(|_| error("cannot encode bounded allocation report"))?;
        Ok(buffer.0)
    }
}
