//! RFC 0047's pure policy over a trusted, current registry-state view.
//!
//! Structural admission does not establish the view's provenance or freshness.
//! A mutation adapter must reload it under its registry lock and pass the actual
//! credential and fresh host time. Returned audit identities are not capabilities.

pub(crate) mod codec;

use std::fmt::{self, Debug, Display, Formatter};
use std::io::{self, Read};

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::package::manifest::valid_name;
use codec::{Action, Records};

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DEPTH: usize = 16;
pub const MAX_PRINCIPALS: usize = 4096;
pub const MAX_TOKENS: usize = 4096;
pub const MAX_PACKAGES: usize = 1024;
pub const MAX_GRANTS: usize = 128;
pub const MAX_OWNERS: usize = 128;
pub const MAX_TIME: u64 = 253_402_300_799;
pub const MAX_LIFETIME: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Malformed,
    Limit,
    Authentication,
    Authorization,
    Identity,
    State,
    Io,
}

/// Static diagnostics never include input material or token authenticators.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: &'static str,
}
impl Error {
    pub const fn code(&self) -> &'static str {
        match self.kind {
            ErrorKind::Malformed => "REG0001",
            ErrorKind::Limit => "REG0002",
            ErrorKind::Authentication => "REG0003",
            ErrorKind::Authorization => "REG0004",
            ErrorKind::Identity => "REG0006",
            ErrorKind::State => "REG0007",
            ErrorKind::Io => "REG0008",
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.message)
    }
}
impl std::error::Error for Error {}

fn error(kind: ErrorKind, message: &'static str) -> Error {
    Error { kind, message }
}
fn malformed() -> Error {
    error(ErrorKind::Malformed, "invalid closed authority record")
}
fn authentication() -> Error {
    error(ErrorKind::Authentication, "credential not authenticated")
}
fn denied() -> Error {
    error(ErrorKind::Authorization, "operation not authorized")
}

/// Independently configured service identity; never derived from an upload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceIdentity {
    registry: String,
    origin: String,
}
impl ServiceIdentity {
    pub fn new(registry: &str, origin: &str) -> Result<Self, Error> {
        if !valid_name(registry) || !codec::valid_origin(origin) {
            return Err(malformed());
        }
        Ok(Self {
            registry: registry.into(),
            origin: origin.into(),
        })
    }
    pub fn registry(&self) -> &str {
        &self.registry
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
}

/// An explicitly provided secret. Caller-owned input is not cleared by this API.
pub struct Bearer(Zeroizing<[u8; 32]>);
impl Debug for Bearer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Bearer([REDACTED])")
    }
}
impl Bearer {
    pub fn from_input(input: &[u8]) -> Result<Self, Error> {
        let input = match input {
            bytes if bytes.len() == 64 => bytes,
            bytes if bytes.len() == 65 && bytes[64] == b'\n' => &bytes[..64],
            _ => return Err(authentication()),
        };
        let mut secret = Zeroizing::new([0; 32]);
        // Decode into the clearing buffer directly, including on an early error.
        for (pair, byte) in input.as_chunks::<2>().0.iter().zip(secret.iter_mut()) {
            let high = codec::nibble(pair[0]).ok_or_else(authentication)?;
            let low = codec::nibble(pair[1]).ok_or_else(authentication)?;
            *byte = high * 16 + low;
        }
        Ok(Self(secret))
    }

    /// Consume at most 66 bytes. The host supplies the stream and its deadline.
    pub fn read_input(mut reader: impl Read) -> Result<Self, Error> {
        let mut input = Zeroizing::new([0; 66]);
        let mut used = 0;
        while used < input.len() {
            match reader.read(&mut input[used..]) {
                Ok(0) => break,
                Ok(n) => used += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(error(ErrorKind::Io, "credential input unavailable")),
            }
        }
        Self::from_input(&input[..used])
    }

    fn authenticator(&self) -> [u8; 32] {
        let digest = Sha256::new()
            .chain_update(b"LANGREGTOKEN1\0")
            .chain_update(self.0.as_ref())
            .finalize();
        let mut bytes = [0; 32];
        bytes.copy_from_slice(&digest);
        bytes
    }

    /// Explicit enrollment material for a trusted administrator, not a bearer.
    pub fn authenticator_sha256(&self) -> String {
        codec::hex(&self.authenticator())
    }

    // The caller owns the explicit secret and a clearing header buffer. Never
    // create an ordinary temporary String containing the bearer.
    pub(crate) fn append_http(&self, header: &mut Zeroizing<String>) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        header.push_str("Authorization: Bearer ");
        for byte in self.0.iter() {
            header.push(HEX[(byte >> 4) as usize] as char);
            header.push(HEX[(byte & 15) as usize] as char);
        }
        header.push_str("\r\n");
    }
}

/// A request adapter derives this from the admitted mutation, never caller claims.
#[derive(Clone, Copy, Debug)]
pub enum Operation<'a> {
    ReadOwners(&'a str),
    Claim(&'a str),
    Publish(&'a str),
    Yank(&'a str),
    ChangeOwners(&'a str),
    Admin(&'a str),
}
impl<'a> Operation<'a> {
    fn name(self) -> &'a str {
        match self {
            Self::ReadOwners(n)
            | Self::Claim(n)
            | Self::Publish(n)
            | Self::Yank(n)
            | Self::ChangeOwners(n)
            | Self::Admin(n) => n,
        }
    }
}

/// Informational audit fields only. No mutation API accepts this as authority.
#[derive(Debug, Eq, PartialEq)]
pub struct Actor<'a> {
    principal: &'a str,
    credential_id: &'a str,
}
impl<'a> Actor<'a> {
    pub fn principal(&self) -> &'a str {
        self.principal
    }
    pub fn credential_id(&self) -> &'a str {
        self.credential_id
    }
}

pub struct Authority {
    records: Records,
    authenticators: Vec<[u8; 32]>,
}
impl Debug for Authority {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Authority")
            .field("principals", &self.records.principals.len())
            .field("tokens", &self.records.tokens.len())
            .field("packages", &self.records.packages.len())
            .finish_non_exhaustive()
    }
}
impl Authority {
    /// Admit structural data from the trusted committed-state loader only.
    pub fn admit(bytes: &[u8], expected: &ServiceIdentity) -> Result<Self, Error> {
        let records = codec::decode(bytes)?;
        Self::from_records(records, expected)
    }

    pub(crate) fn into_records(self) -> codec::Records {
        self.records
    }

    /// Internal state builders still validate every bound and reference.
    pub(crate) fn from_records(
        records: codec::Records,
        expected: &ServiceIdentity,
    ) -> Result<Self, Error> {
        codec::validate(&records)?;
        if records.registry != expected.registry || records.origin != expected.origin {
            return Err(error(
                ErrorKind::Identity,
                "authority service identity mismatch",
            ));
        }
        let authenticators = records
            .tokens
            .iter()
            .map(|t| codec::unhex::<32>(&t.authenticator_sha256))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            records,
            authenticators,
        })
    }

    /// Reevaluate against this view. The caller must establish freshness under
    /// its lock and supply a new trusted host time at each commit or replay.
    pub fn authorize(
        &self,
        bearer: &Bearer,
        now: u64,
        operation: Operation<'_>,
    ) -> Result<Actor<'_>, Error> {
        if now > MAX_TIME || now < self.records.time_floor {
            return Err(error(
                ErrorKind::State,
                "invalid or regressed registry time",
            ));
        }
        let authenticator = bearer.authenticator();
        let mut selected = None;
        for (index, candidate) in self.authenticators.iter().enumerate() {
            // Equality is provided by subtle; there is no prefix shortcut and
            // no early return for the matching record. Whole-policy timing is
            // deliberately not a constant-time claim.
            if bool::from(candidate.ct_eq(&authenticator)) {
                selected = Some(index);
            }
        }
        self.authorize_index(selected.ok_or_else(authentication)?, now, operation)
    }

    /// Checks an audit's recorded policy claim, not possession of a bearer.
    /// This is not a current-operation authorization entry point.
    pub(crate) fn validate_recorded_actor(
        &self,
        principal: &str,
        credential_id: &str,
        time: u64,
        operation: Operation<'_>,
    ) -> Result<(), Error> {
        let index = self
            .records
            .tokens
            .binary_search_by(|t| t.id.as_str().cmp(credential_id))
            .map_err(|_| authentication())?;
        let actor = self.authorize_index(index, time, operation)?;
        if actor.principal != principal {
            return Err(authentication());
        }
        Ok(())
    }

    fn authorize_index(
        &self,
        index: usize,
        now: u64,
        operation: Operation<'_>,
    ) -> Result<Actor<'_>, Error> {
        if now > MAX_TIME || now < self.records.time_floor {
            return Err(error(
                ErrorKind::State,
                "invalid or regressed registry time",
            ));
        }
        let token = self.records.tokens.get(index).ok_or_else(authentication)?;
        let principal = self
            .records
            .principals
            .binary_search_by(|p| p.id.cmp(&token.principal))
            .ok()
            .map(|i| &self.records.principals[i])
            .ok_or_else(authentication)?;
        if token.revoked || !principal.active || now < token.issued || now >= token.expires {
            return Err(authentication());
        }
        let name = operation.name();
        if !valid_name(name) {
            return Err(malformed());
        }
        let package = self
            .records
            .packages
            .binary_search_by(|p| p.name.as_str().cmp(name))
            .ok()
            .map(|i| &self.records.packages[i]);
        let owner = package.is_some_and(|p| p.owners.binary_search(&principal.id).is_ok());
        let grant = |action| {
            token
                .grants
                .iter()
                .any(|g| g.action == action && g.name == name)
        };
        let allowed = match operation {
            Operation::Claim(_) => {
                package.is_none() && grant(Action::Claim) && grant(Action::Publish)
            }
            Operation::ReadOwners(_) => package.is_some() && (owner || grant(Action::Admin)),
            Operation::Admin(_) => package.is_some() && grant(Action::Admin),
            Operation::Publish(_) => owner && grant(Action::Publish),
            Operation::Yank(_) => owner && grant(Action::Yank),
            Operation::ChangeOwners(_) => owner && grant(Action::Owners),
        };
        if !allowed {
            return Err(denied());
        }
        Ok(Actor {
            principal: &principal.id,
            credential_id: &token.id,
        })
    }
}
