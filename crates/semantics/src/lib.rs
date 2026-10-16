#![forbid(unsafe_code)]

//! Compiler semantics and executable language-semantics experiments.
//!
//! `lowering` converts successful RFC 0002 HIR to validated typed MIR.
//!
//! RFCs 0004/0005 accept the strict policies; this crate models only selected
//! traces, not a production ownership checker or structured-task runtime.
//! The permissive alternatives remain unaccepted experiments.
//! The first model compares a deliberately strict async baseline with an
//! alternative that permits shared task-frame borrows across suspension.
//! Exclusive borrows never cross suspension, and cancellation ends permitted
//! borrows before dropping task-owned values exactly once in reverse creation
//! order.
//! The graph model compares explicit shared/weak ownership with arena-scoped
//! storage: shared strong-reference cycles are rejected, while arena-local
//! cycles are released in bulk.

pub mod graph;
pub mod lowering;
pub mod service;

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub start: u32,
    pub end: u32,
}

impl SourceSpan {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ValueId(u32);

impl ValueId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

impl Display for ValueId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "v{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BorrowId(u32);

impl BorrowId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

impl Display for BorrowId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "b{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorrowKind {
    Shared,
    Exclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuspensionBorrowPolicy {
    RejectAll,
    AllowSharedFrameBorrows,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Running,
    Suspended,
    Cancelled,
    Completed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    CreateOwned,
    Borrow,
    EndBorrow,
    Suspend,
    Resume,
    Cancel,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    CreateOwned {
        value: ValueId,
        span: SourceSpan,
    },
    Borrow {
        borrow: BorrowId,
        value: ValueId,
        kind: BorrowKind,
        span: SourceSpan,
    },
    EndBorrow {
        borrow: BorrowId,
        span: SourceSpan,
    },
    Suspend {
        span: SourceSpan,
    },
    Resume {
        span: SourceSpan,
    },
    Cancel {
        span: SourceSpan,
    },
    Complete {
        span: SourceSpan,
    },
}

impl Operation {
    pub const fn kind(self) -> OperationKind {
        match self {
            Self::CreateOwned { .. } => OperationKind::CreateOwned,
            Self::Borrow { .. } => OperationKind::Borrow,
            Self::EndBorrow { .. } => OperationKind::EndBorrow,
            Self::Suspend { .. } => OperationKind::Suspend,
            Self::Resume { .. } => OperationKind::Resume,
            Self::Cancel { .. } => OperationKind::Cancel,
            Self::Complete { .. } => OperationKind::Complete,
        }
    }

    pub const fn span(self) -> SourceSpan {
        match self {
            Self::CreateOwned { span, .. }
            | Self::Borrow { span, .. }
            | Self::EndBorrow { span, .. }
            | Self::Suspend { span }
            | Self::Resume { span }
            | Self::Cancel { span }
            | Self::Complete { span } => span,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    ValueCreated(ValueId),
    BorrowStarted {
        borrow: BorrowId,
        value: ValueId,
        kind: BorrowKind,
    },
    BorrowEnded(BorrowId),
    Suspended,
    Resumed,
    CancellationRequested,
    ValueDropped(ValueId),
    Cancelled,
    Completed,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Trace {
    events: Vec<Event>,
}

impl Trace {
    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticCode {
    DuplicateValue,
    UnknownValue,
    DuplicateBorrow,
    UnknownBorrow,
    InactiveBorrow,
    BorrowConflict,
    BorrowAcrossSuspension,
    BorrowAtCompletion,
    InvalidTaskState,
}

impl DiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateValue => "SEM0001",
            Self::UnknownValue => "SEM0002",
            Self::DuplicateBorrow => "SEM0003",
            Self::UnknownBorrow => "SEM0004",
            Self::InactiveBorrow => "SEM0005",
            Self::BorrowConflict => "SEM0006",
            Self::BorrowAcrossSuspension => "SEM0007",
            Self::BorrowAtCompletion => "SEM0008",
            Self::InvalidTaskState => "SEM0009",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticKind {
    DuplicateValue(ValueId),
    UnknownValue(ValueId),
    DuplicateBorrow(BorrowId),
    UnknownBorrow(BorrowId),
    InactiveBorrow(BorrowId),
    BorrowConflict {
        borrow: BorrowId,
        value: ValueId,
        existing: BorrowId,
    },
    BorrowAcrossSuspension {
        borrow: BorrowId,
        value: ValueId,
    },
    BorrowAtCompletion {
        borrow: BorrowId,
        value: ValueId,
    },
    InvalidTaskState {
        operation: OperationKind,
        state: TaskState,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub primary: SourceSpan,
    pub secondary: Option<SourceSpan>,
    pub kind: DiagnosticKind,
}

impl Display for Diagnostic {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: ", self.code.as_str())?;
        match self.kind {
            DiagnosticKind::DuplicateValue(value) => {
                write!(formatter, "owned value {value} is created more than once")
            }
            DiagnosticKind::UnknownValue(value) => {
                write!(formatter, "owned value {value} does not exist")
            }
            DiagnosticKind::DuplicateBorrow(borrow) => {
                write!(formatter, "borrow {borrow} is created more than once")
            }
            DiagnosticKind::UnknownBorrow(borrow) => {
                write!(formatter, "borrow {borrow} does not exist")
            }
            DiagnosticKind::InactiveBorrow(borrow) => {
                write!(formatter, "borrow {borrow} has already ended")
            }
            DiagnosticKind::BorrowConflict {
                borrow,
                value,
                existing,
            } => write!(
                formatter,
                "borrow {borrow} of {value} conflicts with active borrow {existing}"
            ),
            DiagnosticKind::BorrowAcrossSuspension { borrow, value } => write!(
                formatter,
                "borrow {borrow} of {value} remains active at a suspension point"
            ),
            DiagnosticKind::BorrowAtCompletion { borrow, value } => write!(
                formatter,
                "borrow {borrow} of {value} remains active when the task completes"
            ),
            DiagnosticKind::InvalidTaskState { operation, state } => {
                write!(
                    formatter,
                    "operation {operation:?} is invalid while {state:?}"
                )
            }
        }
    }
}

impl Error for Diagnostic {}

#[derive(Clone, Copy, Debug)]
struct BorrowState {
    value: ValueId,
    kind: BorrowKind,
    span: SourceSpan,
    active: bool,
}

#[derive(Clone, Debug, Default)]
struct ValueState {
    shared_borrows: BTreeSet<BorrowId>,
    exclusive_borrow: Option<BorrowId>,
    dropped: bool,
}

pub struct AsyncTaskModel {
    state: TaskState,
    suspension_borrow_policy: SuspensionBorrowPolicy,
    values: BTreeMap<ValueId, ValueState>,
    borrows: BTreeMap<BorrowId, BorrowState>,
    creation_order: Vec<ValueId>,
    trace: Trace,
}

impl Default for AsyncTaskModel {
    fn default() -> Self {
        Self {
            state: TaskState::Running,
            suspension_borrow_policy: SuspensionBorrowPolicy::RejectAll,
            values: BTreeMap::new(),
            borrows: BTreeMap::new(),
            creation_order: Vec::new(),
            trace: Trace::default(),
        }
    }
}

impl AsyncTaskModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_suspension_borrow_policy(policy: SuspensionBorrowPolicy) -> Self {
        Self {
            suspension_borrow_policy: policy,
            ..Self::default()
        }
    }

    pub const fn state(&self) -> TaskState {
        self.state
    }

    pub const fn trace(&self) -> &Trace {
        &self.trace
    }

    pub fn evaluate(operations: &[Operation]) -> Result<Trace, Diagnostic> {
        let mut model = Self::new();
        for operation in operations {
            model.apply(*operation)?;
        }
        Ok(model.trace)
    }

    pub fn apply(&mut self, operation: Operation) -> Result<(), Diagnostic> {
        match operation {
            Operation::CreateOwned { value, span } => self.create_owned(value, span),
            Operation::Borrow {
                borrow,
                value,
                kind,
                span,
            } => self.borrow(borrow, value, kind, span),
            Operation::EndBorrow { borrow, span } => self.end_borrow(borrow, span),
            Operation::Suspend { span } => self.suspend(span),
            Operation::Resume { span } => self.resume(span),
            Operation::Cancel { span } => self.cancel(span),
            Operation::Complete { span } => self.complete(span),
        }
    }

    fn require_state(
        &self,
        operation: OperationKind,
        span: SourceSpan,
        required: TaskState,
    ) -> Result<(), Diagnostic> {
        if self.state == required {
            return Ok(());
        }

        Err(Diagnostic {
            code: DiagnosticCode::InvalidTaskState,
            primary: span,
            secondary: None,
            kind: DiagnosticKind::InvalidTaskState {
                operation,
                state: self.state,
            },
        })
    }

    fn create_owned(&mut self, value: ValueId, span: SourceSpan) -> Result<(), Diagnostic> {
        self.require_state(OperationKind::CreateOwned, span, TaskState::Running)?;
        if self.values.contains_key(&value) {
            return Err(Diagnostic {
                code: DiagnosticCode::DuplicateValue,
                primary: span,
                secondary: None,
                kind: DiagnosticKind::DuplicateValue(value),
            });
        }

        self.values.insert(value, ValueState::default());
        self.creation_order.push(value);
        self.trace.events.push(Event::ValueCreated(value));
        Ok(())
    }

    fn borrow(
        &mut self,
        borrow: BorrowId,
        value: ValueId,
        kind: BorrowKind,
        span: SourceSpan,
    ) -> Result<(), Diagnostic> {
        self.require_state(OperationKind::Borrow, span, TaskState::Running)?;
        if self.borrows.contains_key(&borrow) {
            return Err(Diagnostic {
                code: DiagnosticCode::DuplicateBorrow,
                primary: span,
                secondary: None,
                kind: DiagnosticKind::DuplicateBorrow(borrow),
            });
        }

        let value_state = self.values.get(&value).ok_or(Diagnostic {
            code: DiagnosticCode::UnknownValue,
            primary: span,
            secondary: None,
            kind: DiagnosticKind::UnknownValue(value),
        })?;
        let existing = match kind {
            BorrowKind::Shared => value_state.exclusive_borrow,
            BorrowKind::Exclusive => value_state
                .exclusive_borrow
                .or_else(|| value_state.shared_borrows.first().copied()),
        };

        if let Some(existing) = existing {
            return Err(Diagnostic {
                code: DiagnosticCode::BorrowConflict,
                primary: span,
                secondary: self.borrows.get(&existing).map(|state| state.span),
                kind: DiagnosticKind::BorrowConflict {
                    borrow,
                    value,
                    existing,
                },
            });
        }

        let value_state = self
            .values
            .get_mut(&value)
            .expect("value existence was checked above");
        match kind {
            BorrowKind::Shared => {
                value_state.shared_borrows.insert(borrow);
            }
            BorrowKind::Exclusive => value_state.exclusive_borrow = Some(borrow),
        }
        self.borrows.insert(
            borrow,
            BorrowState {
                value,
                kind,
                span,
                active: true,
            },
        );
        self.trace.events.push(Event::BorrowStarted {
            borrow,
            value,
            kind,
        });
        Ok(())
    }

    fn end_borrow(&mut self, borrow: BorrowId, span: SourceSpan) -> Result<(), Diagnostic> {
        self.require_state(OperationKind::EndBorrow, span, TaskState::Running)?;
        let borrow_state = self.borrows.get(&borrow).copied().ok_or(Diagnostic {
            code: DiagnosticCode::UnknownBorrow,
            primary: span,
            secondary: None,
            kind: DiagnosticKind::UnknownBorrow(borrow),
        })?;
        if !borrow_state.active {
            return Err(Diagnostic {
                code: DiagnosticCode::InactiveBorrow,
                primary: span,
                secondary: Some(borrow_state.span),
                kind: DiagnosticKind::InactiveBorrow(borrow),
            });
        }

        let value_state = self
            .values
            .get_mut(&borrow_state.value)
            .expect("active borrows always reference an existing value");
        match borrow_state.kind {
            BorrowKind::Shared => {
                value_state.shared_borrows.remove(&borrow);
            }
            BorrowKind::Exclusive => value_state.exclusive_borrow = None,
        }
        self.borrows
            .get_mut(&borrow)
            .expect("borrow existence was checked above")
            .active = false;
        self.trace.events.push(Event::BorrowEnded(borrow));
        Ok(())
    }

    fn suspend(&mut self, span: SourceSpan) -> Result<(), Diagnostic> {
        self.require_state(OperationKind::Suspend, span, TaskState::Running)?;
        if let Some((borrow, borrow_state)) = self.borrow_blocking_suspension() {
            return Err(Diagnostic {
                code: DiagnosticCode::BorrowAcrossSuspension,
                primary: span,
                secondary: Some(borrow_state.span),
                kind: DiagnosticKind::BorrowAcrossSuspension {
                    borrow,
                    value: borrow_state.value,
                },
            });
        }

        self.state = TaskState::Suspended;
        self.trace.events.push(Event::Suspended);
        Ok(())
    }

    fn resume(&mut self, span: SourceSpan) -> Result<(), Diagnostic> {
        self.require_state(OperationKind::Resume, span, TaskState::Suspended)?;
        self.state = TaskState::Running;
        self.trace.events.push(Event::Resumed);
        Ok(())
    }

    fn cancel(&mut self, span: SourceSpan) -> Result<(), Diagnostic> {
        self.require_state(OperationKind::Cancel, span, TaskState::Suspended)?;
        self.trace.events.push(Event::CancellationRequested);
        self.end_active_borrows_for_cancellation();
        self.drop_owned_values();
        self.state = TaskState::Cancelled;
        self.trace.events.push(Event::Cancelled);
        Ok(())
    }

    fn complete(&mut self, span: SourceSpan) -> Result<(), Diagnostic> {
        self.require_state(OperationKind::Complete, span, TaskState::Running)?;
        if let Some((borrow, borrow_state)) = self.active_borrow() {
            return Err(Diagnostic {
                code: DiagnosticCode::BorrowAtCompletion,
                primary: span,
                secondary: Some(borrow_state.span),
                kind: DiagnosticKind::BorrowAtCompletion {
                    borrow,
                    value: borrow_state.value,
                },
            });
        }

        self.drop_owned_values();
        self.state = TaskState::Completed;
        self.trace.events.push(Event::Completed);
        Ok(())
    }

    fn active_borrow(&self) -> Option<(BorrowId, BorrowState)> {
        self.borrows
            .iter()
            .find_map(|(borrow, state)| state.active.then_some((*borrow, *state)))
    }

    fn borrow_blocking_suspension(&self) -> Option<(BorrowId, BorrowState)> {
        self.borrows.iter().find_map(|(borrow, state)| {
            let blocks = state.active
                && match self.suspension_borrow_policy {
                    SuspensionBorrowPolicy::RejectAll => true,
                    SuspensionBorrowPolicy::AllowSharedFrameBorrows => {
                        state.kind == BorrowKind::Exclusive
                    }
                };
            blocks.then_some((*borrow, *state))
        })
    }

    fn end_active_borrows_for_cancellation(&mut self) {
        let active_borrows = self
            .borrows
            .iter()
            .filter_map(|(borrow, state)| state.active.then_some((*borrow, *state)))
            .collect::<Vec<_>>();

        for (borrow, borrow_state) in active_borrows {
            let value_state = self
                .values
                .get_mut(&borrow_state.value)
                .expect("active borrows always reference an existing value");
            match borrow_state.kind {
                BorrowKind::Shared => {
                    value_state.shared_borrows.remove(&borrow);
                }
                BorrowKind::Exclusive => value_state.exclusive_borrow = None,
            }
            self.borrows
                .get_mut(&borrow)
                .expect("active borrow existence was checked above")
                .active = false;
            self.trace.events.push(Event::BorrowEnded(borrow));
        }
    }

    fn drop_owned_values(&mut self) {
        for value in self.creation_order.iter().rev().copied() {
            let value_state = self
                .values
                .get_mut(&value)
                .expect("creation order only contains existing values");
            if !value_state.dropped {
                value_state.dropped = true;
                self.trace.events.push(Event::ValueDropped(value));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AsyncTaskModel, BorrowId, BorrowKind, Diagnostic, DiagnosticCode, DiagnosticKind, Event,
        Operation, SourceSpan, SuspensionBorrowPolicy, TaskState, ValueId,
    };

    const fn span(start: u32) -> SourceSpan {
        SourceSpan::new(start, start + 1)
    }

    #[test]
    fn owned_values_survive_suspension_and_drop_on_completion() {
        let value = ValueId::new(0);
        let trace = AsyncTaskModel::evaluate(&[
            Operation::CreateOwned {
                value,
                span: span(0),
            },
            Operation::Suspend { span: span(1) },
            Operation::Resume { span: span(2) },
            Operation::Complete { span: span(3) },
        ])
        .unwrap();

        assert_eq!(
            trace.events(),
            &[
                Event::ValueCreated(value),
                Event::Suspended,
                Event::Resumed,
                Event::ValueDropped(value),
                Event::Completed,
            ]
        );
    }

    #[test]
    fn allows_borrows_that_end_before_suspension() {
        let value = ValueId::new(0);
        let borrow = BorrowId::new(0);
        let result = AsyncTaskModel::evaluate(&[
            Operation::CreateOwned {
                value,
                span: span(0),
            },
            Operation::Borrow {
                borrow,
                value,
                kind: BorrowKind::Shared,
                span: span(1),
            },
            Operation::EndBorrow {
                borrow,
                span: span(2),
            },
            Operation::Suspend { span: span(3) },
        ]);

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_borrows_across_suspension_with_a_stable_diagnostic() {
        let value = ValueId::new(0);
        let borrow = BorrowId::new(0);
        let result = AsyncTaskModel::evaluate(&[
            Operation::CreateOwned {
                value,
                span: span(0),
            },
            Operation::Borrow {
                borrow,
                value,
                kind: BorrowKind::Shared,
                span: span(1),
            },
            Operation::Suspend { span: span(2) },
        ]);

        assert_eq!(
            result,
            Err(Diagnostic {
                code: DiagnosticCode::BorrowAcrossSuspension,
                primary: span(2),
                secondary: Some(span(1)),
                kind: DiagnosticKind::BorrowAcrossSuspension { borrow, value },
            })
        );
        assert_eq!(DiagnosticCode::BorrowAcrossSuspension.as_str(), "SEM0007");
    }

    #[test]
    fn alternative_policy_allows_shared_frame_borrows_across_suspension() {
        let value = ValueId::new(0);
        let borrow = BorrowId::new(0);
        let mut model = AsyncTaskModel::with_suspension_borrow_policy(
            SuspensionBorrowPolicy::AllowSharedFrameBorrows,
        );
        for operation in [
            Operation::CreateOwned {
                value,
                span: span(0),
            },
            Operation::Borrow {
                borrow,
                value,
                kind: BorrowKind::Shared,
                span: span(1),
            },
            Operation::Suspend { span: span(2) },
            Operation::Resume { span: span(3) },
            Operation::EndBorrow {
                borrow,
                span: span(4),
            },
            Operation::Complete { span: span(5) },
        ] {
            model.apply(operation).unwrap();
        }

        assert_eq!(
            model.trace().events(),
            &[
                Event::ValueCreated(value),
                Event::BorrowStarted {
                    borrow,
                    value,
                    kind: BorrowKind::Shared,
                },
                Event::Suspended,
                Event::Resumed,
                Event::BorrowEnded(borrow),
                Event::ValueDropped(value),
                Event::Completed,
            ]
        );
    }

    #[test]
    fn alternative_policy_still_rejects_exclusive_borrows_across_suspension() {
        let value = ValueId::new(0);
        let borrow = BorrowId::new(0);
        let mut model = AsyncTaskModel::with_suspension_borrow_policy(
            SuspensionBorrowPolicy::AllowSharedFrameBorrows,
        );
        model
            .apply(Operation::CreateOwned {
                value,
                span: span(0),
            })
            .unwrap();
        model
            .apply(Operation::Borrow {
                borrow,
                value,
                kind: BorrowKind::Exclusive,
                span: span(1),
            })
            .unwrap();

        assert_eq!(
            model.apply(Operation::Suspend { span: span(2) }),
            Err(Diagnostic {
                code: DiagnosticCode::BorrowAcrossSuspension,
                primary: span(2),
                secondary: Some(span(1)),
                kind: DiagnosticKind::BorrowAcrossSuspension { borrow, value },
            })
        );
        assert_eq!(model.state(), TaskState::Running);
    }

    #[test]
    fn cancellation_ends_shared_frame_borrows_before_dropping_values() {
        let value = ValueId::new(0);
        let borrow = BorrowId::new(0);
        let mut model = AsyncTaskModel::with_suspension_borrow_policy(
            SuspensionBorrowPolicy::AllowSharedFrameBorrows,
        );
        for operation in [
            Operation::CreateOwned {
                value,
                span: span(0),
            },
            Operation::Borrow {
                borrow,
                value,
                kind: BorrowKind::Shared,
                span: span(1),
            },
            Operation::Suspend { span: span(2) },
            Operation::Cancel { span: span(3) },
        ] {
            model.apply(operation).unwrap();
        }

        assert_eq!(
            model.trace().events(),
            &[
                Event::ValueCreated(value),
                Event::BorrowStarted {
                    borrow,
                    value,
                    kind: BorrowKind::Shared,
                },
                Event::Suspended,
                Event::CancellationRequested,
                Event::BorrowEnded(borrow),
                Event::ValueDropped(value),
                Event::Cancelled,
            ]
        );
    }

    #[test]
    fn cancellation_drops_values_once_in_reverse_creation_order() {
        let first = ValueId::new(0);
        let second = ValueId::new(1);
        let mut model = AsyncTaskModel::new();
        for operation in [
            Operation::CreateOwned {
                value: first,
                span: span(0),
            },
            Operation::CreateOwned {
                value: second,
                span: span(1),
            },
            Operation::Suspend { span: span(2) },
            Operation::Cancel { span: span(3) },
        ] {
            model.apply(operation).unwrap();
        }

        assert_eq!(
            model.trace().events(),
            &[
                Event::ValueCreated(first),
                Event::ValueCreated(second),
                Event::Suspended,
                Event::CancellationRequested,
                Event::ValueDropped(second),
                Event::ValueDropped(first),
                Event::Cancelled,
            ]
        );
        let completed_trace = model.trace().clone();
        assert!(model.apply(Operation::Cancel { span: span(4) }).is_err());
        assert_eq!(model.trace(), &completed_trace);
        assert_eq!(
            model
                .trace()
                .events()
                .iter()
                .filter(|event| matches!(event, Event::ValueDropped(_)))
                .count(),
            2
        );
    }

    #[test]
    fn completion_rejects_active_borrows_before_dropping_values() {
        let value = ValueId::new(0);
        let borrow = BorrowId::new(0);
        let mut model = AsyncTaskModel::new();
        model
            .apply(Operation::CreateOwned {
                value,
                span: span(0),
            })
            .unwrap();
        model
            .apply(Operation::Borrow {
                borrow,
                value,
                kind: BorrowKind::Shared,
                span: span(1),
            })
            .unwrap();

        assert_eq!(
            model.apply(Operation::Complete { span: span(2) }),
            Err(Diagnostic {
                code: DiagnosticCode::BorrowAtCompletion,
                primary: span(2),
                secondary: Some(span(1)),
                kind: DiagnosticKind::BorrowAtCompletion { borrow, value },
            })
        );
        assert!(
            !model
                .trace()
                .events()
                .iter()
                .any(|event| matches!(event, Event::ValueDropped(_)))
        );
    }

    #[test]
    fn rejects_exclusive_borrow_while_shared_borrow_is_active() {
        let value = ValueId::new(0);
        let shared = BorrowId::new(0);
        let exclusive = BorrowId::new(1);
        let result = AsyncTaskModel::evaluate(&[
            Operation::CreateOwned {
                value,
                span: span(0),
            },
            Operation::Borrow {
                borrow: shared,
                value,
                kind: BorrowKind::Shared,
                span: span(1),
            },
            Operation::Borrow {
                borrow: exclusive,
                value,
                kind: BorrowKind::Exclusive,
                span: span(2),
            },
        ]);

        assert_eq!(
            result,
            Err(Diagnostic {
                code: DiagnosticCode::BorrowConflict,
                primary: span(2),
                secondary: Some(span(1)),
                kind: DiagnosticKind::BorrowConflict {
                    borrow: exclusive,
                    value,
                    existing: shared,
                },
            })
        );
    }

    #[test]
    fn cancellation_is_observed_only_at_a_suspension_point() {
        let mut model = AsyncTaskModel::new();

        assert_eq!(model.state(), TaskState::Running);
        assert_eq!(
            model.apply(Operation::Cancel { span: span(0) }),
            Err(Diagnostic {
                code: DiagnosticCode::InvalidTaskState,
                primary: span(0),
                secondary: None,
                kind: DiagnosticKind::InvalidTaskState {
                    operation: super::OperationKind::Cancel,
                    state: TaskState::Running,
                },
            })
        );
    }
}
