//! Bounded explicit-origin HTTPS reads and authenticated mutations for RFC 0046.
//! Transport integrity is independent
//! of package signatures/freshness and grants no filesystem or guest authority.

use super::budget;
mod client;
mod mutations;
mod server;
mod socket;
mod wire;

pub use client::Client;
pub use mutations::{
    CurrentOwners, Delivery, MutationError, MutationOutcome, MutationReceipt, Publication,
};
pub use server::Server;

use std::fmt::{Display, Formatter};
use std::io;
use std::time::{Duration, Instant};

use super::authority::ServiceIdentity;
use super::store;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
pub const MAX_CONNECTIONS: usize = 8;
pub const MAX_REQUEST_MEMORY: usize = 256 * 1024 * 1024;
pub const MAX_LINE_BYTES: usize = 2 * 1024;
pub const MAX_HEADER_BYTES: usize = 8 * 1024;
pub const MAX_HEADER_FIELDS: usize = 32;
pub const MAX_ERROR_BYTES: usize = 16 * 1024;

/// A fixed diagnostic category. Foreign reason phrases, bodies, addresses and
/// certificate details are never copied into an error message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Malformed,
    Limit,
    Authentication,
    Authorization,
    NotFound,
    Conflict,
    Integrity,
    State,
    Busy,
    Transport,
    Timeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
}

impl Error {
    pub const fn code(self) -> &'static str {
        match self.kind {
            ErrorKind::Malformed => "REG0001",
            ErrorKind::Limit => "REG0002",
            ErrorKind::Authentication => "REG0003",
            ErrorKind::Authorization => "REG0004",
            ErrorKind::Conflict => "REG0005",
            ErrorKind::Integrity => "REG0006",
            ErrorKind::State => "REG0007",
            ErrorKind::NotFound | ErrorKind::Busy | ErrorKind::Transport | ErrorKind::Timeout => {
                "REG0008"
            }
        }
    }

    pub const fn message(self) -> &'static str {
        match self.kind {
            ErrorKind::Malformed => "invalid registry HTTP message",
            ErrorKind::Limit => "registry transport limit exceeded",
            ErrorKind::Authentication => "registry credential not authenticated",
            ErrorKind::Authorization => "registry operation not authorized",
            ErrorKind::NotFound => "registry resource is unavailable",
            ErrorKind::Conflict => "registry request conflict",
            ErrorKind::Integrity => "registry identity or integrity mismatch",
            ErrorKind::State => "registry requires verified recovery",
            ErrorKind::Busy => "registry transport is busy",
            ErrorKind::Transport => "registry transport failed",
            ErrorKind::Timeout => "registry transport deadline exceeded",
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}
impl std::error::Error for Error {}

impl From<store::Error> for Error {
    fn from(value: store::Error) -> Self {
        use store::ErrorKind as S;
        error(match value.kind {
            S::Malformed => ErrorKind::Malformed,
            S::Limit => ErrorKind::Limit,
            S::Authentication => ErrorKind::Authentication,
            S::Authorization => ErrorKind::Authorization,
            S::Conflict => ErrorKind::Conflict,
            S::Integrity => ErrorKind::Integrity,
            S::State => ErrorKind::State,
            S::Busy => ErrorKind::Busy,
            S::NotFound => ErrorKind::NotFound,
            S::Io => ErrorKind::Transport,
        })
    }
}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        error(match value.kind() {
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ErrorKind::Timeout,
            _ => ErrorKind::Transport,
        })
    }
}
fn error(kind: ErrorKind) -> Error {
    Error { kind }
}
impl From<budget::Exhausted> for Error {
    fn from(_: budget::Exhausted) -> Self {
        error(ErrorKind::Busy)
    }
}
fn malformed() -> Error {
    error(ErrorKind::Malformed)
}
fn limit() -> Error {
    error(ErrorKind::Limit)
}

#[derive(Clone, Copy)]
struct Deadlines {
    connect: Instant,
    request: Instant,
}
impl Deadlines {
    fn start() -> Self {
        Self::with_limits(CONNECT_TIMEOUT, REQUEST_TIMEOUT)
    }
    fn with_limits(connect: Duration, request: Duration) -> Self {
        let now = Instant::now();
        Self {
            connect: now + connect.min(request),
            request: now + request,
        }
    }
}

fn remaining(end: Instant) -> io::Result<Duration> {
    end.checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
}

/// The identity constructor already excludes paths, IPv6 and ambiguous ports.
fn authority(identity: &ServiceIdentity) -> &str {
    identity
        .origin()
        .strip_prefix("https://")
        .expect("validated identity")
}

#[cfg(test)]
mod tests;
