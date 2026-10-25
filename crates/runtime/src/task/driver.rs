//! Scheduling adapter for owned tasks and a synchronous call on a borrowed task.
use super::*;
use std::ops::{Deref, DerefMut};

enum ScopeState {
    Empty,
    Ready,
    Running,
    CleanupReady(Outcome),
    Cleaning(Outcome),
    Complete(Outcome),
}
enum Kind<'a> {
    Owned(Scheduler),
    Scope {
        current: CurrentTask<'a>,
        state: ScopeState,
        cancelled: bool,
    },
}
pub struct Driver<'a>(Kind<'a>);
impl<'a> Driver<'a> {
    pub fn owned(limits: Limits) -> Result<Self, Error> {
        Scheduler::new(limits).map(|s| Self(Kind::Owned(s)))
    }
    pub fn scope(current: CurrentTask<'a>) -> Result<Self, Error> {
        current.scheduler.running(current.id())?;
        Ok(Self(Kind::Scope {
            current,
            state: ScopeState::Empty,
            cancelled: false,
        }))
    }
    pub fn record_index(&self, task: &TaskId) -> Result<usize, Error> {
        match &self.0 {
            Kind::Owned(s) => s.index(task),
            Kind::Scope { current, .. } if current.id() == task => Ok(0),
            Kind::Scope { .. } => Err(Error::ForeignTask),
        }
    }
    pub fn start(&mut self, origin: Option<Span>) -> Result<TaskId, Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.start(origin),
            Kind::Scope { current, state, .. } => {
                if !matches!(state, ScopeState::Empty) {
                    return Err(Error::RootExists);
                }
                current.scheduler.running(current.id())?;
                *state = ScopeState::Ready;
                Ok(current.id().clone())
            }
        }
    }
    pub fn dispatch(&mut self) -> Result<Option<Dispatch>, Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.dispatch(),
            Kind::Scope { current, state, .. } => {
                let task = current.id().clone();
                Ok(match state {
                    ScopeState::Ready => {
                        *state = ScopeState::Running;
                        Some(Dispatch::Resume(task))
                    }
                    ScopeState::CleanupReady(outcome) => {
                        let outcome = *outcome;
                        *state = ScopeState::Cleaning(outcome);
                        Some(Dispatch::Cleanup { task, outcome })
                    }
                    ScopeState::Running | ScopeState::Cleaning(_) => return Err(Error::Busy),
                    ScopeState::Empty | ScopeState::Complete(_) => None,
                })
            }
        }
    }
    pub fn is_finished(&self) -> bool {
        match &self.0 {
            Kind::Owned(s) => s.is_finished(),
            Kind::Scope { state, .. } => matches!(state, ScopeState::Complete(_)),
        }
    }
    pub fn status(&self, task: &TaskId) -> Result<Status, Error> {
        match &self.0 {
            Kind::Owned(s) => s.status(task),
            Kind::Scope { current, state, .. } => {
                if current.id() != task {
                    return Err(Error::ForeignTask);
                }
                Ok(match state {
                    ScopeState::Empty => return Err(Error::InvalidTask),
                    ScopeState::Ready => Status::Ready,
                    ScopeState::Running => Status::Running,
                    ScopeState::CleanupReady(outcome) => Status::CleanupReady(*outcome),
                    ScopeState::Cleaning(outcome) => Status::Cleaning(*outcome),
                    ScopeState::Complete(outcome) => Status::Complete(*outcome),
                })
            }
        }
    }
    pub fn finish(&mut self, task: &TaskId, outcome: Outcome) -> Result<(), Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.finish(task, outcome),
            Kind::Scope {
                current,
                state,
                cancelled,
            } => {
                if current.id() != task {
                    return Err(Error::ForeignTask);
                }
                if !matches!(state, ScopeState::Running) {
                    return Err(Error::WrongPhase);
                }
                let closing = *cancelled || current.closing_requested()?;
                *state = ScopeState::CleanupReady(if outcome == Outcome::Success && closing {
                    Outcome::Cancelled
                } else {
                    outcome
                });
                Ok(())
            }
        }
    }
    pub fn finish_cleanup(
        &mut self,
        task: &TaskId,
        failure: Option<FailureId>,
    ) -> Result<Outcome, Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.finish_cleanup(task, failure),
            Kind::Scope { current, state, .. } => {
                if current.id() != task {
                    return Err(Error::ForeignTask);
                }
                let ScopeState::Cleaning(outcome) = *state else {
                    return Err(Error::WrongPhase);
                };
                let outcome = match (outcome, failure) {
                    (Outcome::Success, Some(failure)) => Outcome::Failed(failure),
                    (outcome, _) => outcome,
                };
                *state = ScopeState::Complete(outcome);
                Ok(outcome)
            }
        }
    }
    pub fn cancellation_point(&mut self, task: &TaskId) -> Result<Boundary, Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.cancellation_point(task),
            Kind::Scope {
                current,
                state,
                cancelled,
            } => {
                if current.id() != task {
                    return Err(Error::ForeignTask);
                }
                if !matches!(state, ScopeState::Running) {
                    return Err(Error::WrongPhase);
                }
                if *cancelled || current.closing_requested()? {
                    *state = ScopeState::CleanupReady(Outcome::Cancelled);
                    Ok(Boundary::Closing)
                } else {
                    Ok(Boundary::Continue)
                }
            }
        }
    }
    pub fn request_cancel(&mut self, task: &TaskId) -> Result<(), Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.request_cancel(task),
            Kind::Scope {
                current, cancelled, ..
            } => {
                if current.id() != task {
                    return Err(Error::ForeignTask);
                }
                *cancelled = true;
                Ok(())
            }
        }
    }
    pub fn current(&mut self, task: &TaskId) -> Result<CurrentTask<'_>, Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.current(task),
            Kind::Scope { current, state, .. } => {
                if current.id() != task {
                    return Err(Error::ForeignTask);
                }
                if !matches!(state, ScopeState::Running) {
                    return Err(Error::WrongPhase);
                }
                current.reborrow()
            }
        }
    }
    pub fn check_spawn(&self, task: &TaskId) -> Result<(), Error> {
        match &self.0 {
            Kind::Owned(s) => s.check_spawn(task),
            Kind::Scope { .. } => Err(Error::WrongPhase),
        }
    }
    pub fn spawn(&mut self, task: &TaskId, origin: Option<Span>) -> Result<TaskId, Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.spawn(task, origin),
            Kind::Scope { .. } => Err(Error::WrongPhase),
        }
    }
    pub fn join(&mut self, task: &TaskId, child: &TaskId) -> Result<(), Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.join(task, child),
            Kind::Scope { .. } => Err(Error::WrongPhase),
        }
    }
    pub fn yield_now(&mut self, task: &TaskId) -> Result<(), Error> {
        match &mut self.0 {
            Kind::Owned(s) => s.yield_now(task),
            Kind::Scope { .. } => Err(Error::WrongPhase),
        }
    }
}
impl Deref for Driver<'_> {
    type Target = Scheduler;
    fn deref(&self) -> &Scheduler {
        match &self.0 {
            Kind::Owned(s) => s,
            Kind::Scope { current, .. } => current.scheduler,
        }
    }
}
impl DerefMut for Driver<'_> {
    fn deref_mut(&mut self) -> &mut Scheduler {
        match &mut self.0 {
            Kind::Owned(s) => s,
            Kind::Scope { current, .. } => current.scheduler,
        }
    }
}
