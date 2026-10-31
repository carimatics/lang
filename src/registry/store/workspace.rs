use super::*;
use crate::registry::budget::{Budget, Reservation};
use std::sync::Arc;

const PROBE_MEMORY: usize = 2 * 1024 * 1024;

fn busy() -> Error {
    error(ErrorKind::Busy, "registry request budget is busy")
}
fn recovery() -> Error {
    error(ErrorKind::State, "registry requires explicit recovery")
}

#[derive(Clone, Copy)]
enum Input {
    Read,
    Action(usize),
    Publication(usize),
}
impl Input {
    fn bytes(self) -> usize {
        match self {
            Self::Read => 0,
            Self::Action(n) | Self::Publication(n) => n,
        }
    }
}

struct Plan {
    envelope: storage::Envelope,
    workspace: usize,
    generation: usize,
    head: String,
}
impl Plan {
    fn new(probe: storage::Probe, input: Input) -> Result<Self, Error> {
        let body_limit = match input {
            Input::Publication(_) => archive::MAX_ARCHIVE_BYTES,
            Input::Read | Input::Action(_) => 2048,
        };
        if input.bytes() > body_limit {
            return Err(capacity());
        }
        let growth = if matches!(input, Input::Publication(_)) {
            2 * archive::MAX_DESCRIPTOR_BYTES + 4096
        } else {
            4096
        };
        let state = probe
            .state
            .checked_add(growth)
            .ok_or_else(capacity)?
            .min(records::MAX_STATE);
        let object = probe.object.max(if matches!(input, Input::Publication(_)) {
            input.bytes()
        } else {
            0
        });
        let entries = probe.entries.checked_add(8).ok_or_else(capacity)?;
        let workspace = state
            .checked_mul(96)
            .and_then(|n| n.checked_add(object.checked_mul(6)?))
            .and_then(|n| n.checked_add(entries.checked_mul(8192)?))
            .and_then(|n| n.checked_add(PROBE_MEMORY))
            .ok_or_else(capacity)?;
        let generation = state
            .checked_mul(2)
            .and_then(|n| n.checked_add((authority::MAX_PACKAGES + records::MAX_RELEASES) * 1024))
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(capacity)?;
        Ok(Self {
            envelope: storage::Envelope {
                state,
                object,
                entries: entries.min(storage::MAX_ENTRIES),
            },
            workspace,
            generation,
            head: probe.head,
        })
    }
}

struct LeasedGeneration {
    reads: reads::ReadGeneration,
    _reservation: Reservation,
}

// Mutable borrowing prevents a caller from reaching the ordinary Registry API
// while the scoped envelope and its actual budget reservations are active.
struct Workspace<'a> {
    registry: &'a mut Registry,
    plan: Plan,
    _workspace: Reservation,
    _body: Reservation,
    generation: Option<Reservation>,
}
impl<'a> Workspace<'a> {
    fn begin(
        registry: &'a mut Registry,
        budget: &Arc<Budget>,
        input: Input,
        clock: &impl Clock,
    ) -> Result<Self, Error> {
        if registry.storage.envelope.is_some() {
            return Err(recovery());
        }
        if PROBE_MEMORY > budget.maximum() {
            return Err(capacity());
        }
        let mut workspace = budget.reserve(PROBE_MEMORY).map_err(|_| busy())?;
        let plan = Plan::new(registry.storage.probe(clock)?, input)?;
        let required = plan
            .workspace
            .checked_add(plan.generation)
            .and_then(|n| n.checked_add(input.bytes()))
            .ok_or_else(capacity)?;
        if required > budget.maximum() {
            return Err(capacity());
        }
        // Grow the probe's real lease atomically; no uncharged gap or duplicate
        // scratch reservation distorts the exact shared-budget boundary.
        workspace.grow(plan.workspace).map_err(|_| busy())?;
        let body = budget.reserve(input.bytes()).map_err(|_| busy())?;
        let generation = budget.reserve(plan.generation).map_err(|_| busy())?;
        clock.now()?;
        if registry.storage.head()? != plan.head {
            return Err(recovery());
        }
        registry.storage.envelope = Some(plan.envelope);
        Ok(Self {
            registry,
            plan,
            _workspace: workspace,
            _body: body,
            generation: Some(generation),
        })
    }
    fn capture(mut self, clock: &impl Clock) -> Result<LeasedGeneration, Error> {
        clock.now()?;
        let reads = reads::ReadGeneration::capture(self.registry, self.plan.generation)?;
        clock.now()?;
        let mut reservation = self.generation.take().ok_or_else(recovery)?;
        reservation
            .shrink(reads.memory_bytes())
            .map_err(|_| capacity())?;
        Ok(LeasedGeneration {
            reads,
            _reservation: reservation,
        })
    }
}
impl Drop for Workspace<'_> {
    fn drop(&mut self) {
        self.registry.storage.envelope = None;
    }
}

/// A private-directory registry with RFC 0053 workspace and read-generation leases.
/// A failed committed refresh requires dropping this owner and reopening the store.
pub struct MeteredRegistry {
    registry: Registry,
    budget: Arc<Budget>,
    generation: LeasedGeneration,
    recovery_required: bool,
}
impl MeteredRegistry {
    /// Capture a fully validated initial generation under the 256 MiB budget.
    pub fn new(registry: Registry, clock: &impl Clock) -> Result<Self, Error> {
        Self::with_budget(
            registry,
            Budget::new(crate::registry::transport::MAX_REQUEST_MEMORY),
            clock,
        )
    }
    pub(crate) fn with_budget(
        mut registry: Registry,
        budget: Arc<Budget>,
        clock: &impl Clock,
    ) -> Result<Self, Error> {
        let generation =
            Workspace::begin(&mut registry, &budget, Input::Read, clock)?.capture(clock)?;
        Ok(Self {
            registry,
            budget,
            generation,
            recovery_required: false,
        })
    }
    fn ready(&self) -> Result<(), Error> {
        if self.recovery_required {
            return Err(recovery());
        }
        self.generation
            .reads
            .check_head(&self.registry)
            .map_err(|_| recovery())
    }
    fn execute<T>(
        &mut self,
        input: Input,
        clock: &impl Clock,
        operation: impl FnOnce(&mut Registry) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.ready()?;
        let result: Result<(T, Option<LeasedGeneration>), Error> = (|| {
            let workspace = Workspace::begin(&mut self.registry, &self.budget, input, clock)?;
            let value = operation(workspace.registry)?;
            // Exact retry returns its original receipt but keeps today's generation.
            if workspace.registry.storage.head()? == workspace.plan.head {
                return Ok((value, None));
            }
            let next = workspace.capture(clock)?;
            Ok((value, Some(next)))
        })();
        match result {
            Ok((value, next)) => {
                if let Some(next) = next {
                    self.generation = next;
                }
                Ok(value)
            }
            Err(e) => {
                if self.generation.reads.check_head(&self.registry).is_err() {
                    self.recovery_required = true;
                    return Err(recovery());
                }
                if e.kind == ErrorKind::State {
                    self.recovery_required = true;
                }
                Err(e)
            }
        }
    }
    /// Admit and publish source only after reserving recovery and replacement memory.
    pub fn publish(
        &mut self,
        bearer: &Bearer,
        bytes: &[u8],
        request_id: &str,
        expected: ExpectedRevision,
        clock: &impl Clock,
    ) -> Result<Receipt, Error> {
        self.execute(Input::Publication(bytes.len()), clock, |registry| {
            let archive = AdmittedArchive::admit(bytes).map_err(|e| {
                error(
                    match e.kind {
                        archive::ErrorKind::Malformed => ErrorKind::Malformed,
                        archive::ErrorKind::Limit => ErrorKind::Limit,
                        archive::ErrorKind::Integrity => ErrorKind::Integrity,
                    },
                    "invalid registry source archive",
                )
            })?;
            registry.publish(bearer, &archive, request_id, expected, clock)
        })
    }
    /// Apply one canonical yank/unyank action under the same workspace guard.
    pub fn yank(
        &mut self,
        bearer: &Bearer,
        name: &str,
        bytes: &[u8],
        clock: &impl Clock,
    ) -> Result<Receipt, Error> {
        self.execute(Input::Action(bytes.len()), clock, |registry| {
            registry.yank(bearer, &YankAction::admit(name, bytes)?, clock)
        })
    }
    /// Apply one canonical owner-membership action under the workspace guard.
    pub fn change_owner(
        &mut self,
        bearer: &Bearer,
        name: &str,
        bytes: &[u8],
        clock: &impl Clock,
    ) -> Result<Receipt, Error> {
        self.execute(Input::Action(bytes.len()), clock, |registry| {
            registry.change_owner(bearer, &OwnerAction::admit(name, bytes)?, clock)
        })
    }
    /// Borrow a public index from the latest fully validated generation.
    pub fn index(&self, name: &str) -> Result<&[u8], Error> {
        self.ready()?;
        self.generation.reads.index(&self.registry, name)
    }
    /// Keep exact object-response memory charged for the returned body's lifetime.
    pub fn archive(&self, hash: &str) -> Result<MeteredBytes, Error> {
        self.ready()?;
        let length = self.generation.reads.archive_length(&self.registry, hash)?;
        let reservation = self.budget.reserve(length).map_err(|_| busy())?;
        let bytes = self.registry.storage.captured_object(hash, length)?;
        Ok(MeteredBytes {
            bytes: bytes.into_boxed_slice(),
            _reservation: reservation,
        })
    }
    /// Freshly authorize a private owner response and retain its budget lease.
    pub fn owners(
        &mut self,
        bearer: &Bearer,
        name: &str,
        clock: &impl Clock,
    ) -> Result<MeteredBytes, Error> {
        let mut reservation = self.budget.reserve(MAX_OWNER_BYTES).map_err(|_| busy())?;
        let bytes = self.execute(Input::Read, clock, |registry| {
            registry
                .owners(bearer, name, clock)
                .map(Vec::into_boxed_slice)
        })?;
        reservation.shrink(bytes.len()).map_err(|_| capacity())?;
        Ok(MeteredBytes {
            bytes,
            _reservation: reservation,
        })
    }
}

/// A bounded response retaining its request-budget reservation until dropped.
pub struct MeteredBytes {
    bytes: Box<[u8]>,
    _reservation: Reservation,
}
impl MeteredBytes {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
