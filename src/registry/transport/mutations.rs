use super::*;
use crate::registry::{archive::AdmittedArchive, authority::Bearer};
use serde::{Deserialize, Serialize};
use std::io::Write;
use zeroize::Zeroizing;

#[cfg(test)]
mod tests;

/// Describes this transmission attempt, never the fate of an earlier same-ID request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Delivery {
    NotSent,
    Refused,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MutationError {
    pub error: Error,
    pub delivery: Delivery,
}
impl Display for MutationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({:?})", self.error, self.delivery)
    }
}
impl std::error::Error for MutationError {}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MutationOutcome {
    Published,
    Yanked,
    Unyanked,
    OwnerAdded,
    OwnerRemoved,
}

/// Informational server result. It is never accepted as publication authority.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MutationReceipt {
    schema: u32,
    registry: String,
    request_id: String,
    revision: u64,
    audit_sequence: u64,
    outcome: MutationOutcome,
}
impl MutationReceipt {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn audit_sequence(&self) -> u64 {
        self.audit_sequence
    }
    pub fn outcome(&self) -> MutationOutcome {
        self.outcome
    }
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub(super) fn from_host(
        identity: &ServiceIdentity,
        id: &str,
        receipt: &store::Receipt,
    ) -> Result<Self, Error> {
        let (revision, outcome) = receipt
            .package_result()
            .ok_or_else(|| error(ErrorKind::State))?;
        let outcome = match outcome {
            "published" => MutationOutcome::Published,
            "yanked" => MutationOutcome::Yanked,
            "unyanked" => MutationOutcome::Unyanked,
            "owner_added" => MutationOutcome::OwnerAdded,
            "owner_removed" => MutationOutcome::OwnerRemoved,
            _ => return Err(error(ErrorKind::State)),
        };
        Ok(Self {
            schema: 1,
            registry: identity.registry().into(),
            request_id: id.into(),
            revision,
            audit_sequence: receipt.state_revision(),
            outcome,
        })
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, Error> {
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|_| malformed())?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    fn admit(
        bytes: &[u8],
        identity: &ServiceIdentity,
        id: &str,
        revision: u64,
        outcome: MutationOutcome,
    ) -> Result<Self, Error> {
        let receipt: Self = serde_json::from_slice(bytes).map_err(|_| malformed())?;
        if receipt.schema != 1
            || receipt.registry != identity.registry()
            || receipt.request_id != id
            || receipt.revision != revision
            || receipt.audit_sequence <= revision
            || receipt.outcome != outcome
            || receipt.encode()?.as_slice() != bytes
        {
            return Err(error(ErrorKind::Integrity));
        }
        Ok(receipt)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Publication {
    pub receipt: MutationReceipt,
    pub created: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CurrentOwners {
    schema: u32,
    registry: String,
    name: String,
    revision: u64,
    #[serde(deserialize_with = "owner_ids")]
    owners: Vec<String>,
}
fn owner_ids<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    crate::registry::authority::codec::bounded::<D, _, 128>(d)
}
impl CurrentOwners {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn owners(&self) -> &[String] {
        &self.owners
    }

    fn admit(bytes: &[u8], identity: &ServiceIdentity, name: &str) -> Result<Self, Error> {
        let value: Self = serde_json::from_slice(bytes).map_err(|_| malformed())?;
        let mut canonical = serde_json::to_vec_pretty(&value).map_err(|_| malformed())?;
        canonical.push(b'\n');
        if value.schema != 1
            || value.registry != identity.registry()
            || value.name != name
            || value.revision == 0
            || value.owners.is_empty()
            || value.owners.iter().any(|id| !wire::valid_id(id))
            || value.owners.windows(2).any(|pair| pair[0] >= pair[1])
            || canonical != bytes
        {
            return Err(error(ErrorKind::Integrity));
        }
        Ok(value)
    }
}

struct ActionRequest<'a> {
    name: &'a str,
    bytes: &'a [u8],
    id: &'a str,
    revision: u64,
    outcome: MutationOutcome,
}

impl Client {
    /// Publish with explicitly supplied secret input. Never retry implicitly;
    /// an unknown result must be reconciled using the same request identity.
    pub fn publish(
        &self,
        bearer: &Bearer,
        archive: &AdmittedArchive,
        id: &str,
        expected: store::ExpectedRevision,
    ) -> Result<Publication, MutationError> {
        let mut delivery = Delivery::NotSent;
        let result = (|| {
            if !wire::valid_id(id) {
                return Err(malformed());
            }
            let revision = match expected {
                store::ExpectedRevision::Absent => 1,
                store::ExpectedRevision::Revision(0) => return Err(malformed()),
                store::ExpectedRevision::Revision(n) => n.checked_add(1).ok_or_else(limit)?,
            };
            let package = archive.descriptor().package();
            let mut header =
                Zeroizing::new(String::with_capacity(MAX_LINE_BYTES + MAX_HEADER_BYTES));
            header.push_str(&format!("PUT /v1/packages/{}/{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/x-tar\r\nContent-Length: {}\r\nIdempotency-Key: {id}\r\n", package.name(), package.version(), authority(self.identity()), archive.as_bytes().len()));
            match expected {
                store::ExpectedRevision::Absent => header.push_str("If-None-Match: *\r\n"),
                store::ExpectedRevision::Revision(n) => {
                    header.push_str(&format!("If-Match: \"{n}\"\r\n"))
                }
            }
            bearer.append_http(&mut header);
            header.push_str("\r\n");
            self.transmit(
                header.as_bytes(),
                archive.as_bytes(),
                (id, revision, MutationOutcome::Published),
                &mut delivery,
            )
        })();
        result.map_err(|error| MutationError { error, delivery })
    }

    pub fn yank(
        &self,
        bearer: &Bearer,
        action: &store::YankAction,
    ) -> Result<MutationReceipt, MutationError> {
        self.action(
            bearer,
            ActionRequest {
                name: action.name(),
                bytes: action.as_bytes(),
                id: action.request_id(),
                revision: action.expected_revision(),
                outcome: match action.kind() {
                    store::YankActionKind::Yank => MutationOutcome::Yanked,
                    store::YankActionKind::Unyank => MutationOutcome::Unyanked,
                },
            },
        )
    }

    pub fn change_owner(
        &self,
        bearer: &Bearer,
        action: &store::OwnerAction,
    ) -> Result<MutationReceipt, MutationError> {
        self.action(
            bearer,
            ActionRequest {
                name: action.name(),
                bytes: action.as_bytes(),
                id: action.request_id(),
                revision: action.expected_revision(),
                outcome: match action.kind() {
                    store::OwnerActionKind::AddOwner => MutationOutcome::OwnerAdded,
                    store::OwnerActionKind::RemoveOwner => MutationOutcome::OwnerRemoved,
                },
            },
        )
    }

    fn action(
        &self,
        bearer: &Bearer,
        action: ActionRequest<'_>,
    ) -> Result<MutationReceipt, MutationError> {
        let mut delivery = Delivery::NotSent;
        let result = (|| {
            let revision = action.revision.checked_add(1).ok_or_else(limit)?;
            let mut header =
                Zeroizing::new(String::with_capacity(MAX_LINE_BYTES + MAX_HEADER_BYTES));
            header.push_str(&format!("POST /v1/packages/{}/actions HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", action.name, authority(self.identity()), action.bytes.len()));
            bearer.append_http(&mut header);
            header.push_str("\r\n");
            self.transmit(
                header.as_bytes(),
                action.bytes,
                (action.id, revision, action.outcome),
                &mut delivery,
            )
            .map(|p| p.receipt)
        })();
        result.map_err(|error| MutationError { error, delivery })
    }

    pub fn owners(&self, bearer: &Bearer, name: &str) -> Result<CurrentOwners, Error> {
        if !crate::package::manifest::valid_name(name) {
            return Err(malformed());
        }
        let mut header = Zeroizing::new(String::with_capacity(MAX_LINE_BYTES + MAX_HEADER_BYTES));
        header.push_str(&format!("GET /v1/packages/{name}/owners HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: 0\r\n", authority(self.identity())));
        bearer.append_http(&mut header);
        header.push_str("\r\n");
        let deadlines = Deadlines::start();
        let mut stream = self.connect(deadlines)?;
        stream.write_all(header.as_bytes())?;
        stream.flush()?;
        let head = wire::head(&mut stream, deadlines.request)?;
        let response = wire::response(&head, "application/json")?;
        let maximum = if response.status == 200 {
            store::MAX_OWNER_BYTES
        } else {
            MAX_ERROR_BYTES
        };
        let bytes = wire::body(&mut stream, response.length, maximum, deadlines.request)?;
        wire::eof(&mut stream, deadlines.request)?;
        remaining(deadlines.request)?;
        if response.status != 200 {
            return Err(wire::error_body(&bytes, response.status)?);
        }
        CurrentOwners::admit(&bytes, self.identity(), name)
    }

    fn transmit(
        &self,
        header: &[u8],
        body: &[u8],
        expected: (&str, u64, MutationOutcome),
        delivery: &mut Delivery,
    ) -> Result<Publication, Error> {
        let (id, revision, outcome) = expected;
        let deadlines = Deadlines::start();
        let mut stream = self.connect(deadlines)?;
        *delivery = Delivery::Unknown;
        stream.write_all(header)?;
        for bytes in body.chunks(64 * 1024) {
            remaining(deadlines.request)?;
            stream.write_all(bytes)?;
        }
        stream.flush()?;
        let head = wire::head(&mut stream, deadlines.request)?;
        let response = wire::response(&head, "application/json")?;
        let bytes = wire::body(
            &mut stream,
            response.length,
            MAX_ERROR_BYTES,
            deadlines.request,
        )?;
        wire::eof(&mut stream, deadlines.request)?;
        remaining(deadlines.request)?;
        if !matches!(response.status, 200 | 201) {
            let e = wire::error_body_for(&bytes, response.status, Some(id))?;
            if !matches!(
                e.kind,
                ErrorKind::State | ErrorKind::Transport | ErrorKind::Timeout
            ) {
                *delivery = Delivery::Refused;
            }
            return Err(e);
        }
        if response.status == 201 && outcome != MutationOutcome::Published {
            return Err(malformed());
        }
        let receipt = MutationReceipt::admit(&bytes, self.identity(), id, revision, outcome)?;
        Ok(Publication {
            receipt,
            created: response.status == 201,
        })
    }
}
