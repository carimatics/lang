//! Executable ownership experiment for shared mutable services.
//!
//! This deliberately strict model isolates mutable service state to one owner
//! task. Shared handles may enqueue sendable payloads, but they cannot mutate
//! service state directly. Sending moves a payload into the mailbox. Service
//! cancellation waits for an active mutation to end, then discards queued
//! messages and drops their payloads exactly once. These choices are design
//! hypotheses, not accepted language semantics.

use std::collections::{BTreeMap, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::SourceSpan;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ServiceId(u32);

impl ServiceId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

impl Display for ServiceId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "s{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TaskId(u32);

impl TaskId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

impl Display for TaskId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "t{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MessageId(u32);

impl MessageId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

impl Display for MessageId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "m{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PayloadId(u32);

impl PayloadId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

impl Display for PayloadId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "p{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadCapability {
    Sendable,
    LocalOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceStatus {
    Running,
    CancellationPending,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceOperationKind {
    CreateService,
    ShareHandle,
    CreatePayload,
    Send,
    BeginMutation,
    EndMutation,
    ProcessNext,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceOperation {
    CreateService {
        service: ServiceId,
        owner: TaskId,
        span: SourceSpan,
    },
    ShareHandle {
        service: ServiceId,
        from: TaskId,
        to: TaskId,
        span: SourceSpan,
    },
    CreatePayload {
        payload: PayloadId,
        owner: TaskId,
        capability: PayloadCapability,
        span: SourceSpan,
    },
    Send {
        service: ServiceId,
        sender: TaskId,
        message: MessageId,
        payload: PayloadId,
        span: SourceSpan,
    },
    BeginMutation {
        service: ServiceId,
        task: TaskId,
        span: SourceSpan,
    },
    EndMutation {
        service: ServiceId,
        task: TaskId,
        span: SourceSpan,
    },
    ProcessNext {
        service: ServiceId,
        task: TaskId,
        span: SourceSpan,
    },
    Cancel {
        service: ServiceId,
        requester: TaskId,
        span: SourceSpan,
    },
}

impl ServiceOperation {
    pub const fn kind(self) -> ServiceOperationKind {
        match self {
            Self::CreateService { .. } => ServiceOperationKind::CreateService,
            Self::ShareHandle { .. } => ServiceOperationKind::ShareHandle,
            Self::CreatePayload { .. } => ServiceOperationKind::CreatePayload,
            Self::Send { .. } => ServiceOperationKind::Send,
            Self::BeginMutation { .. } => ServiceOperationKind::BeginMutation,
            Self::EndMutation { .. } => ServiceOperationKind::EndMutation,
            Self::ProcessNext { .. } => ServiceOperationKind::ProcessNext,
            Self::Cancel { .. } => ServiceOperationKind::Cancel,
        }
    }

    pub const fn span(self) -> SourceSpan {
        match self {
            Self::CreateService { span, .. }
            | Self::ShareHandle { span, .. }
            | Self::CreatePayload { span, .. }
            | Self::Send { span, .. }
            | Self::BeginMutation { span, .. }
            | Self::EndMutation { span, .. }
            | Self::ProcessNext { span, .. }
            | Self::Cancel { span, .. } => span,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceEvent {
    ServiceCreated {
        service: ServiceId,
        owner: TaskId,
    },
    HandleShared {
        service: ServiceId,
        from: TaskId,
        to: TaskId,
    },
    PayloadCreated {
        payload: PayloadId,
        owner: TaskId,
        capability: PayloadCapability,
    },
    PayloadMoved {
        payload: PayloadId,
        from: TaskId,
        service: ServiceId,
        message: MessageId,
    },
    MessageQueued {
        service: ServiceId,
        message: MessageId,
    },
    MutationStarted {
        service: ServiceId,
        task: TaskId,
    },
    MutationEnded {
        service: ServiceId,
        task: TaskId,
    },
    MessageProcessed {
        service: ServiceId,
        message: MessageId,
    },
    MessageDiscarded {
        service: ServiceId,
        message: MessageId,
    },
    PayloadDropped(PayloadId),
    CancellationRequested {
        service: ServiceId,
        requester: TaskId,
    },
    ServiceStateDropped(ServiceId),
    ServiceCancelled(ServiceId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ServiceTrace {
    events: Vec<ServiceEvent>,
}

impl ServiceTrace {
    pub fn events(&self) -> &[ServiceEvent] {
        &self.events
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceDiagnosticCode {
    DuplicateService,
    UnknownService,
    ServiceUnavailable,
    DuplicatePayload,
    UnknownPayload,
    PayloadUnavailable,
    PayloadNotSendable,
    MissingHandle,
    DuplicateHandle,
    IsolationViolation,
    MutationConflict,
    NoActiveMutation,
    DuplicateMessage,
    EmptyMailbox,
}

impl ServiceDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateService => "SEM0022",
            Self::UnknownService => "SEM0023",
            Self::ServiceUnavailable => "SEM0024",
            Self::DuplicatePayload => "SEM0025",
            Self::UnknownPayload => "SEM0026",
            Self::PayloadUnavailable => "SEM0027",
            Self::PayloadNotSendable => "SEM0028",
            Self::MissingHandle => "SEM0029",
            Self::DuplicateHandle => "SEM0030",
            Self::IsolationViolation => "SEM0031",
            Self::MutationConflict => "SEM0032",
            Self::NoActiveMutation => "SEM0033",
            Self::DuplicateMessage => "SEM0034",
            Self::EmptyMailbox => "SEM0035",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceDiagnosticKind {
    DuplicateService(ServiceId),
    UnknownService(ServiceId),
    ServiceUnavailable {
        service: ServiceId,
        status: ServiceStatus,
    },
    DuplicatePayload(PayloadId),
    UnknownPayload(PayloadId),
    PayloadUnavailable(PayloadId),
    PayloadNotSendable(PayloadId),
    MissingHandle {
        service: ServiceId,
        task: TaskId,
    },
    DuplicateHandle {
        service: ServiceId,
        task: TaskId,
    },
    IsolationViolation {
        service: ServiceId,
        task: TaskId,
        owner: TaskId,
    },
    MutationConflict {
        service: ServiceId,
        task: TaskId,
    },
    NoActiveMutation(ServiceId),
    DuplicateMessage(MessageId),
    EmptyMailbox(ServiceId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceDiagnostic {
    pub code: ServiceDiagnosticCode,
    pub primary: SourceSpan,
    pub secondary: Option<SourceSpan>,
    pub kind: ServiceDiagnosticKind,
}

impl Display for ServiceDiagnostic {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: ", self.code.as_str())?;
        match self.kind {
            ServiceDiagnosticKind::DuplicateService(service) => {
                write!(formatter, "service {service} is created more than once")
            }
            ServiceDiagnosticKind::UnknownService(service) => {
                write!(formatter, "service {service} does not exist")
            }
            ServiceDiagnosticKind::ServiceUnavailable { service, status } => {
                write!(
                    formatter,
                    "service {service} is unavailable while {status:?}"
                )
            }
            ServiceDiagnosticKind::DuplicatePayload(payload) => {
                write!(formatter, "payload {payload} is created more than once")
            }
            ServiceDiagnosticKind::UnknownPayload(payload) => {
                write!(formatter, "payload {payload} does not exist")
            }
            ServiceDiagnosticKind::PayloadUnavailable(payload) => {
                write!(
                    formatter,
                    "payload {payload} has already been moved or dropped"
                )
            }
            ServiceDiagnosticKind::PayloadNotSendable(payload) => {
                write!(formatter, "payload {payload} cannot cross a task boundary")
            }
            ServiceDiagnosticKind::MissingHandle { service, task } => {
                write!(formatter, "task {task} has no handle for service {service}")
            }
            ServiceDiagnosticKind::DuplicateHandle { service, task } => {
                write!(
                    formatter,
                    "task {task} already has a handle for service {service}"
                )
            }
            ServiceDiagnosticKind::IsolationViolation {
                service,
                task,
                owner,
            } => write!(
                formatter,
                "task {task} cannot mutate service {service}; its owner is {owner}"
            ),
            ServiceDiagnosticKind::MutationConflict { service, task } => write!(
                formatter,
                "task {task} cannot access service {service} during an active mutation"
            ),
            ServiceDiagnosticKind::NoActiveMutation(service) => {
                write!(formatter, "service {service} has no active mutation")
            }
            ServiceDiagnosticKind::DuplicateMessage(message) => {
                write!(formatter, "message {message} is sent more than once")
            }
            ServiceDiagnosticKind::EmptyMailbox(service) => {
                write!(formatter, "service {service} has no queued messages")
            }
        }
    }
}

impl Error for ServiceDiagnostic {}

#[derive(Clone, Copy, Debug)]
enum PayloadLocation {
    Task(TaskId),
    Mailbox {
        service: ServiceId,
        message: MessageId,
    },
    Dropped,
}

#[derive(Clone, Copy, Debug)]
struct PayloadState {
    capability: PayloadCapability,
    location: PayloadLocation,
    created_at: SourceSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MessageStatus {
    Queued,
    Processed,
    Discarded,
}

#[derive(Clone, Copy, Debug)]
struct MessageState {
    service: ServiceId,
    payload: PayloadId,
    created_at: SourceSpan,
    status: MessageStatus,
}

#[derive(Clone, Debug)]
struct ServiceState {
    owner: TaskId,
    status: ServiceStatus,
    handles: BTreeMap<TaskId, SourceSpan>,
    mailbox: VecDeque<MessageId>,
    active_mutation: Option<(TaskId, SourceSpan)>,
    created_at: SourceSpan,
    state_dropped: bool,
}

#[derive(Default)]
pub struct ServiceModel {
    services: BTreeMap<ServiceId, ServiceState>,
    payloads: BTreeMap<PayloadId, PayloadState>,
    messages: BTreeMap<MessageId, MessageState>,
    trace: ServiceTrace,
}

impl ServiceModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn trace(&self) -> &ServiceTrace {
        &self.trace
    }

    pub fn status(&self, service: ServiceId) -> Option<ServiceStatus> {
        self.services.get(&service).map(|state| state.status)
    }

    pub fn evaluate(operations: &[ServiceOperation]) -> Result<ServiceTrace, ServiceDiagnostic> {
        let mut model = Self::new();
        for operation in operations {
            model.apply(*operation)?;
        }
        Ok(model.trace)
    }

    pub fn apply(&mut self, operation: ServiceOperation) -> Result<(), ServiceDiagnostic> {
        match operation {
            ServiceOperation::CreateService {
                service,
                owner,
                span,
            } => self.create_service(service, owner, span),
            ServiceOperation::ShareHandle {
                service,
                from,
                to,
                span,
            } => self.share_handle(service, from, to, span),
            ServiceOperation::CreatePayload {
                payload,
                owner,
                capability,
                span,
            } => self.create_payload(payload, owner, capability, span),
            ServiceOperation::Send {
                service,
                sender,
                message,
                payload,
                span,
            } => self.send(service, sender, message, payload, span),
            ServiceOperation::BeginMutation {
                service,
                task,
                span,
            } => self.begin_mutation(service, task, span),
            ServiceOperation::EndMutation {
                service,
                task,
                span,
            } => self.end_mutation(service, task, span),
            ServiceOperation::ProcessNext {
                service,
                task,
                span,
            } => self.process_next(service, task, span),
            ServiceOperation::Cancel {
                service,
                requester,
                span,
            } => self.cancel(service, requester, span),
        }
    }

    fn create_service(
        &mut self,
        service: ServiceId,
        owner: TaskId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        if let Some(existing) = self.services.get(&service) {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::DuplicateService,
                primary: span,
                secondary: Some(existing.created_at),
                kind: ServiceDiagnosticKind::DuplicateService(service),
            });
        }

        self.services.insert(
            service,
            ServiceState {
                owner,
                status: ServiceStatus::Running,
                handles: BTreeMap::from([(owner, span)]),
                mailbox: VecDeque::new(),
                active_mutation: None,
                created_at: span,
                state_dropped: false,
            },
        );
        self.trace
            .events
            .push(ServiceEvent::ServiceCreated { service, owner });
        Ok(())
    }

    fn share_handle(
        &mut self,
        service: ServiceId,
        from: TaskId,
        to: TaskId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        self.require_running_service(service, span)?;
        self.require_handle(service, from, span)?;
        if let Some(existing) = self
            .services
            .get(&service)
            .expect("service availability was checked above")
            .handles
            .get(&to)
        {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::DuplicateHandle,
                primary: span,
                secondary: Some(*existing),
                kind: ServiceDiagnosticKind::DuplicateHandle { service, task: to },
            });
        }

        self.services
            .get_mut(&service)
            .expect("service availability was checked above")
            .handles
            .insert(to, span);
        self.trace
            .events
            .push(ServiceEvent::HandleShared { service, from, to });
        Ok(())
    }

    fn create_payload(
        &mut self,
        payload: PayloadId,
        owner: TaskId,
        capability: PayloadCapability,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        if let Some(existing) = self.payloads.get(&payload) {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::DuplicatePayload,
                primary: span,
                secondary: Some(existing.created_at),
                kind: ServiceDiagnosticKind::DuplicatePayload(payload),
            });
        }

        self.payloads.insert(
            payload,
            PayloadState {
                capability,
                location: PayloadLocation::Task(owner),
                created_at: span,
            },
        );
        self.trace.events.push(ServiceEvent::PayloadCreated {
            payload,
            owner,
            capability,
        });
        Ok(())
    }

    fn send(
        &mut self,
        service: ServiceId,
        sender: TaskId,
        message: MessageId,
        payload: PayloadId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        self.require_running_service(service, span)?;
        self.require_handle(service, sender, span)?;
        if let Some(existing) = self.messages.get(&message) {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::DuplicateMessage,
                primary: span,
                secondary: Some(existing.created_at),
                kind: ServiceDiagnosticKind::DuplicateMessage(message),
            });
        }

        let payload_state = self
            .payloads
            .get(&payload)
            .copied()
            .ok_or(ServiceDiagnostic {
                code: ServiceDiagnosticCode::UnknownPayload,
                primary: span,
                secondary: None,
                kind: ServiceDiagnosticKind::UnknownPayload(payload),
            })?;
        if !matches!(payload_state.location, PayloadLocation::Task(owner) if owner == sender) {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::PayloadUnavailable,
                primary: span,
                secondary: Some(payload_state.created_at),
                kind: ServiceDiagnosticKind::PayloadUnavailable(payload),
            });
        }
        if payload_state.capability != PayloadCapability::Sendable {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::PayloadNotSendable,
                primary: span,
                secondary: Some(payload_state.created_at),
                kind: ServiceDiagnosticKind::PayloadNotSendable(payload),
            });
        }

        self.payloads
            .get_mut(&payload)
            .expect("payload existence was checked above")
            .location = PayloadLocation::Mailbox { service, message };
        self.messages.insert(
            message,
            MessageState {
                service,
                payload,
                created_at: span,
                status: MessageStatus::Queued,
            },
        );
        self.services
            .get_mut(&service)
            .expect("service availability was checked above")
            .mailbox
            .push_back(message);
        self.trace.events.push(ServiceEvent::PayloadMoved {
            payload,
            from: sender,
            service,
            message,
        });
        self.trace
            .events
            .push(ServiceEvent::MessageQueued { service, message });
        Ok(())
    }

    fn begin_mutation(
        &mut self,
        service: ServiceId,
        task: TaskId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        self.require_running_service(service, span)?;
        self.require_owner(service, task, span)?;
        self.require_no_mutation(service, task, span)?;

        self.services
            .get_mut(&service)
            .expect("service availability was checked above")
            .active_mutation = Some((task, span));
        self.trace
            .events
            .push(ServiceEvent::MutationStarted { service, task });
        Ok(())
    }

    fn end_mutation(
        &mut self,
        service: ServiceId,
        task: TaskId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        let service_state = self.require_service(service, span)?;
        if service_state.status == ServiceStatus::Cancelled {
            return Err(self.unavailable_service(service, service_state, span));
        }
        self.require_owner(service, task, span)?;
        let active = self
            .services
            .get(&service)
            .expect("service existence was checked above")
            .active_mutation;
        let Some((mutator, mutation_span)) = active else {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::NoActiveMutation,
                primary: span,
                secondary: None,
                kind: ServiceDiagnosticKind::NoActiveMutation(service),
            });
        };
        if mutator != task {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::MutationConflict,
                primary: span,
                secondary: Some(mutation_span),
                kind: ServiceDiagnosticKind::MutationConflict { service, task },
            });
        }

        let cancellation_pending = {
            let state = self
                .services
                .get_mut(&service)
                .expect("service existence was checked above");
            state.active_mutation = None;
            state.status == ServiceStatus::CancellationPending
        };
        self.trace
            .events
            .push(ServiceEvent::MutationEnded { service, task });
        if cancellation_pending {
            self.finish_cancellation(service);
        }
        Ok(())
    }

    fn process_next(
        &mut self,
        service: ServiceId,
        task: TaskId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        self.require_running_service(service, span)?;
        self.require_owner(service, task, span)?;
        self.require_no_mutation(service, task, span)?;
        let message = self
            .services
            .get_mut(&service)
            .expect("service availability was checked above")
            .mailbox
            .pop_front()
            .ok_or(ServiceDiagnostic {
                code: ServiceDiagnosticCode::EmptyMailbox,
                primary: span,
                secondary: None,
                kind: ServiceDiagnosticKind::EmptyMailbox(service),
            })?;

        let payload = {
            let message_state = self
                .messages
                .get_mut(&message)
                .expect("mailbox only contains existing messages");
            debug_assert_eq!(message_state.service, service);
            debug_assert_eq!(message_state.status, MessageStatus::Queued);
            message_state.status = MessageStatus::Processed;
            message_state.payload
        };
        self.trace
            .events
            .push(ServiceEvent::MessageProcessed { service, message });
        self.drop_payload(payload);
        Ok(())
    }

    fn cancel(
        &mut self,
        service: ServiceId,
        requester: TaskId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        self.require_running_service(service, span)?;
        self.require_handle(service, requester, span)?;
        let active_mutation = {
            let state = self
                .services
                .get_mut(&service)
                .expect("service availability was checked above");
            state.status = ServiceStatus::CancellationPending;
            state.active_mutation.is_some()
        };
        self.trace
            .events
            .push(ServiceEvent::CancellationRequested { service, requester });
        if !active_mutation {
            self.finish_cancellation(service);
        }
        Ok(())
    }

    fn require_service(
        &self,
        service: ServiceId,
        span: SourceSpan,
    ) -> Result<&ServiceState, ServiceDiagnostic> {
        self.services.get(&service).ok_or(ServiceDiagnostic {
            code: ServiceDiagnosticCode::UnknownService,
            primary: span,
            secondary: None,
            kind: ServiceDiagnosticKind::UnknownService(service),
        })
    }

    fn require_running_service(
        &self,
        service: ServiceId,
        span: SourceSpan,
    ) -> Result<&ServiceState, ServiceDiagnostic> {
        let state = self.require_service(service, span)?;
        if state.status != ServiceStatus::Running {
            return Err(self.unavailable_service(service, state, span));
        }
        Ok(state)
    }

    fn unavailable_service(
        &self,
        service: ServiceId,
        state: &ServiceState,
        span: SourceSpan,
    ) -> ServiceDiagnostic {
        ServiceDiagnostic {
            code: ServiceDiagnosticCode::ServiceUnavailable,
            primary: span,
            secondary: Some(state.created_at),
            kind: ServiceDiagnosticKind::ServiceUnavailable {
                service,
                status: state.status,
            },
        }
    }

    fn require_handle(
        &self,
        service: ServiceId,
        task: TaskId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        let state = self.require_service(service, span)?;
        if !state.handles.contains_key(&task) {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::MissingHandle,
                primary: span,
                secondary: Some(state.created_at),
                kind: ServiceDiagnosticKind::MissingHandle { service, task },
            });
        }
        Ok(())
    }

    fn require_owner(
        &self,
        service: ServiceId,
        task: TaskId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        let state = self.require_service(service, span)?;
        if state.owner != task {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::IsolationViolation,
                primary: span,
                secondary: Some(state.created_at),
                kind: ServiceDiagnosticKind::IsolationViolation {
                    service,
                    task,
                    owner: state.owner,
                },
            });
        }
        Ok(())
    }

    fn require_no_mutation(
        &self,
        service: ServiceId,
        task: TaskId,
        span: SourceSpan,
    ) -> Result<(), ServiceDiagnostic> {
        let state = self.require_service(service, span)?;
        if let Some((_, mutation_span)) = state.active_mutation {
            return Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::MutationConflict,
                primary: span,
                secondary: Some(mutation_span),
                kind: ServiceDiagnosticKind::MutationConflict { service, task },
            });
        }
        Ok(())
    }

    fn finish_cancellation(&mut self, service: ServiceId) {
        let queued = self
            .services
            .get_mut(&service)
            .expect("only existing services can finish cancellation")
            .mailbox
            .drain(..)
            .collect::<Vec<_>>();
        for message in queued {
            let payload = {
                let message_state = self
                    .messages
                    .get_mut(&message)
                    .expect("mailbox only contains existing messages");
                debug_assert_eq!(message_state.service, service);
                debug_assert_eq!(message_state.status, MessageStatus::Queued);
                message_state.status = MessageStatus::Discarded;
                message_state.payload
            };
            self.trace
                .events
                .push(ServiceEvent::MessageDiscarded { service, message });
            self.drop_payload(payload);
        }

        let state = self
            .services
            .get_mut(&service)
            .expect("only existing services can finish cancellation");
        if !state.state_dropped {
            state.state_dropped = true;
            self.trace
                .events
                .push(ServiceEvent::ServiceStateDropped(service));
        }
        state.status = ServiceStatus::Cancelled;
        self.trace
            .events
            .push(ServiceEvent::ServiceCancelled(service));
    }

    fn drop_payload(&mut self, payload: PayloadId) {
        let state = self
            .payloads
            .get_mut(&payload)
            .expect("only existing payloads can be dropped");
        if !matches!(state.location, PayloadLocation::Dropped) {
            if let PayloadLocation::Mailbox { service, message } = state.location {
                debug_assert!(self.messages.get(&message).is_some_and(|message_state| {
                    message_state.service == service && message_state.payload == payload
                }));
            }
            state.location = PayloadLocation::Dropped;
            self.trace
                .events
                .push(ServiceEvent::PayloadDropped(payload));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MessageId, PayloadCapability, PayloadId, ServiceDiagnostic, ServiceDiagnosticCode,
        ServiceDiagnosticKind, ServiceEvent, ServiceId, ServiceModel, ServiceOperation,
        ServiceStatus, TaskId,
    };
    use crate::SourceSpan;

    const fn span(start: u32) -> SourceSpan {
        SourceSpan::new(start, start + 1)
    }

    #[test]
    fn shared_handles_can_send_but_cannot_mutate_service_state() {
        let service = ServiceId::new(0);
        let owner = TaskId::new(0);
        let client = TaskId::new(1);
        let mut model = ServiceModel::new();
        model
            .apply(ServiceOperation::CreateService {
                service,
                owner,
                span: span(0),
            })
            .unwrap();
        model
            .apply(ServiceOperation::ShareHandle {
                service,
                from: owner,
                to: client,
                span: span(1),
            })
            .unwrap();

        assert_eq!(
            model.apply(ServiceOperation::BeginMutation {
                service,
                task: client,
                span: span(2),
            }),
            Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::IsolationViolation,
                primary: span(2),
                secondary: Some(span(0)),
                kind: ServiceDiagnosticKind::IsolationViolation {
                    service,
                    task: client,
                    owner,
                },
            })
        );
    }

    #[test]
    fn send_moves_a_payload_and_owner_processes_messages_in_fifo_order() {
        let service = ServiceId::new(0);
        let owner = TaskId::new(0);
        let client = TaskId::new(1);
        let first_message = MessageId::new(0);
        let second_message = MessageId::new(1);
        let first_payload = PayloadId::new(0);
        let second_payload = PayloadId::new(1);
        let trace = ServiceModel::evaluate(&[
            ServiceOperation::CreateService {
                service,
                owner,
                span: span(0),
            },
            ServiceOperation::ShareHandle {
                service,
                from: owner,
                to: client,
                span: span(1),
            },
            ServiceOperation::CreatePayload {
                payload: first_payload,
                owner: client,
                capability: PayloadCapability::Sendable,
                span: span(2),
            },
            ServiceOperation::CreatePayload {
                payload: second_payload,
                owner: client,
                capability: PayloadCapability::Sendable,
                span: span(3),
            },
            ServiceOperation::Send {
                service,
                sender: client,
                message: first_message,
                payload: first_payload,
                span: span(4),
            },
            ServiceOperation::Send {
                service,
                sender: client,
                message: second_message,
                payload: second_payload,
                span: span(5),
            },
            ServiceOperation::ProcessNext {
                service,
                task: owner,
                span: span(6),
            },
            ServiceOperation::ProcessNext {
                service,
                task: owner,
                span: span(7),
            },
        ])
        .unwrap();

        let processed = trace
            .events()
            .iter()
            .filter_map(|event| match event {
                ServiceEvent::MessageProcessed { message, .. } => Some(*message),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(processed, vec![first_message, second_message]);
        assert_eq!(
            trace
                .events()
                .iter()
                .filter(|event| matches!(event, ServiceEvent::PayloadDropped(_)))
                .count(),
            2
        );
    }

    #[test]
    fn rejects_non_sendable_payloads_at_the_task_boundary() {
        let service = ServiceId::new(0);
        let owner = TaskId::new(0);
        let payload = PayloadId::new(0);
        let mut model = ServiceModel::new();
        model
            .apply(ServiceOperation::CreateService {
                service,
                owner,
                span: span(0),
            })
            .unwrap();
        model
            .apply(ServiceOperation::CreatePayload {
                payload,
                owner,
                capability: PayloadCapability::LocalOnly,
                span: span(1),
            })
            .unwrap();

        assert_eq!(
            model.apply(ServiceOperation::Send {
                service,
                sender: owner,
                message: MessageId::new(0),
                payload,
                span: span(2),
            }),
            Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::PayloadNotSendable,
                primary: span(2),
                secondary: Some(span(1)),
                kind: ServiceDiagnosticKind::PayloadNotSendable(payload),
            })
        );
    }

    #[test]
    fn rejects_reuse_after_a_payload_moves_into_the_mailbox() {
        let service = ServiceId::new(0);
        let owner = TaskId::new(0);
        let payload = PayloadId::new(0);
        let mut model = ServiceModel::new();
        for operation in [
            ServiceOperation::CreateService {
                service,
                owner,
                span: span(0),
            },
            ServiceOperation::CreatePayload {
                payload,
                owner,
                capability: PayloadCapability::Sendable,
                span: span(1),
            },
            ServiceOperation::Send {
                service,
                sender: owner,
                message: MessageId::new(0),
                payload,
                span: span(2),
            },
        ] {
            model.apply(operation).unwrap();
        }

        assert_eq!(
            model.apply(ServiceOperation::Send {
                service,
                sender: owner,
                message: MessageId::new(1),
                payload,
                span: span(3),
            }),
            Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::PayloadUnavailable,
                primary: span(3),
                secondary: Some(span(1)),
                kind: ServiceDiagnosticKind::PayloadUnavailable(payload),
            })
        );
    }

    #[test]
    fn cancellation_discards_queued_payloads_once_in_fifo_order() {
        let service = ServiceId::new(0);
        let owner = TaskId::new(0);
        let first_message = MessageId::new(0);
        let second_message = MessageId::new(1);
        let first_payload = PayloadId::new(0);
        let second_payload = PayloadId::new(1);
        let trace = ServiceModel::evaluate(&[
            ServiceOperation::CreateService {
                service,
                owner,
                span: span(0),
            },
            ServiceOperation::CreatePayload {
                payload: first_payload,
                owner,
                capability: PayloadCapability::Sendable,
                span: span(1),
            },
            ServiceOperation::CreatePayload {
                payload: second_payload,
                owner,
                capability: PayloadCapability::Sendable,
                span: span(2),
            },
            ServiceOperation::Send {
                service,
                sender: owner,
                message: first_message,
                payload: first_payload,
                span: span(3),
            },
            ServiceOperation::Send {
                service,
                sender: owner,
                message: second_message,
                payload: second_payload,
                span: span(4),
            },
            ServiceOperation::Cancel {
                service,
                requester: owner,
                span: span(5),
            },
        ])
        .unwrap();

        assert!(trace.events().ends_with(&[
            ServiceEvent::CancellationRequested {
                service,
                requester: owner,
            },
            ServiceEvent::MessageDiscarded {
                service,
                message: first_message,
            },
            ServiceEvent::PayloadDropped(first_payload),
            ServiceEvent::MessageDiscarded {
                service,
                message: second_message,
            },
            ServiceEvent::PayloadDropped(second_payload),
            ServiceEvent::ServiceStateDropped(service),
            ServiceEvent::ServiceCancelled(service),
        ]));
    }

    #[test]
    fn cancellation_waits_for_an_active_mutation_boundary() {
        let service = ServiceId::new(0);
        let owner = TaskId::new(0);
        let client = TaskId::new(1);
        let mut model = ServiceModel::new();
        for operation in [
            ServiceOperation::CreateService {
                service,
                owner,
                span: span(0),
            },
            ServiceOperation::ShareHandle {
                service,
                from: owner,
                to: client,
                span: span(1),
            },
            ServiceOperation::BeginMutation {
                service,
                task: owner,
                span: span(2),
            },
            ServiceOperation::Cancel {
                service,
                requester: client,
                span: span(3),
            },
        ] {
            model.apply(operation).unwrap();
        }

        assert_eq!(
            model.status(service),
            Some(ServiceStatus::CancellationPending)
        );
        assert!(
            !model
                .trace()
                .events()
                .contains(&ServiceEvent::ServiceCancelled(service))
        );

        model
            .apply(ServiceOperation::EndMutation {
                service,
                task: owner,
                span: span(4),
            })
            .unwrap();

        assert_eq!(model.status(service), Some(ServiceStatus::Cancelled));
        assert!(model.trace().events().ends_with(&[
            ServiceEvent::MutationEnded {
                service,
                task: owner,
            },
            ServiceEvent::ServiceStateDropped(service),
            ServiceEvent::ServiceCancelled(service),
        ]));
    }

    #[test]
    fn cancelled_services_reject_new_messages() {
        let service = ServiceId::new(0);
        let owner = TaskId::new(0);
        let payload = PayloadId::new(0);
        let mut model = ServiceModel::new();
        for operation in [
            ServiceOperation::CreateService {
                service,
                owner,
                span: span(0),
            },
            ServiceOperation::CreatePayload {
                payload,
                owner,
                capability: PayloadCapability::Sendable,
                span: span(1),
            },
            ServiceOperation::Cancel {
                service,
                requester: owner,
                span: span(2),
            },
        ] {
            model.apply(operation).unwrap();
        }

        assert_eq!(
            model.apply(ServiceOperation::Send {
                service,
                sender: owner,
                message: MessageId::new(0),
                payload,
                span: span(3),
            }),
            Err(ServiceDiagnostic {
                code: ServiceDiagnosticCode::ServiceUnavailable,
                primary: span(3),
                secondary: Some(span(0)),
                kind: ServiceDiagnosticKind::ServiceUnavailable {
                    service,
                    status: ServiceStatus::Cancelled,
                },
            })
        );
    }
}
