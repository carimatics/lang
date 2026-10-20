//! RFC 0021: explicit, bounded scheduling state shared by execution adapters.
//!
//! This kernel owns no guest frames and executes no guest code. A trusted driver
//! must perform actual cleanup before acknowledging a cleanup dispatch, and must
//! retain frame/result ownership until then. Source/MIR loan and capability
//! validation, instruction controls and frame allocation remain adapter duties.

use lang_mir::unit::Span;
use std::collections::VecDeque;
use std::fmt::{Debug, Display, Formatter};
use std::rc::Rc;

pub mod runner;

/// Private invocation identity prevents a handle from naming another tree.
#[derive(Clone)]
pub struct TaskId {
    invocation: Rc<()>,
    index: usize,
}
impl TaskId {
    /// Adapter storage index, never sufficient on its own to authorize access.
    pub fn index(&self) -> usize {
        self.index
    }
}
impl PartialEq for TaskId {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && Rc::ptr_eq(&self.invocation, &other.invocation)
    }
}
impl Eq for TaskId {}
impl Debug for TaskId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TaskId").field(&self.index).finish()
    }
}

/// Identity in the adapter's retained, structured failure report storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FailureId(pub u64);

/// A typed guest `Result::Err` is still scheduling success.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Success,
    Cancelled,
    Failed(FailureId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub live_tasks: usize,
    /// Bounds retained completion/ancestry metadata as well as cumulative spawn.
    pub total_tasks: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            live_tasks: 256,
            total_tasks: 4096,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    pub admitted: usize,
    pub live: usize,
    pub peak_live: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Dispatch {
    Resume(TaskId),
    Cleanup { task: TaskId, outcome: Outcome },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Ready,
    Running,
    Waiting { child: TaskId },
    Joining,
    CleanupReady(Outcome),
    Cleaning(Outcome),
    Complete(Outcome),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Boundary {
    Continue,
    Closing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Capacity,
    RootExists,
    ForeignTask,
    InvalidTask,
    Busy,
    NoActiveTask,
    NotActiveTask,
    WrongPhase,
    NotChild,
    CancellationPending,
    LiveTaskLimit { limit: usize },
    TotalTaskLimit { limit: usize },
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "structured task driver: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug)]
enum State {
    Ready,
    Running,
    Waiting(usize),
    Joining(Outcome),
    CleanupReady(Outcome),
    Cleaning(Outcome),
    Complete(Outcome),
}
impl State {
    fn complete(self) -> bool {
        matches!(self, Self::Complete(_))
    }
}

#[derive(Debug)]
struct Task {
    parent: Option<usize>,
    origin: Option<Span>,
    state: State,
    queued: bool,
    cancellation_requested: bool,
    live_children: usize,
    first_child_failure: Option<(usize, FailureId)>,
}

/// One explicitly driven invocation. No global executor, threads or wakeups.
/// Preparation reserves all task/queue storage; transitions allocate no storage.
pub struct Scheduler {
    invocation: Rc<()>,
    limits: Limits,
    tasks: Vec<Task>,
    queue: VecDeque<usize>,
    active: Option<usize>,
    usage: Usage,
}
impl Scheduler {
    pub fn new(limits: Limits) -> Result<Self, Error> {
        let mut tasks = Vec::new();
        let mut queue = VecDeque::new();
        tasks
            .try_reserve_exact(limits.total_tasks)
            .map_err(|_| Error::Capacity)?;
        queue
            .try_reserve_exact(limits.total_tasks)
            .map_err(|_| Error::Capacity)?;
        Ok(Self {
            invocation: Rc::new(()),
            limits,
            tasks,
            queue,
            active: None,
            usage: Usage::default(),
        })
    }

    pub fn usage(&self) -> Usage {
        self.usage
    }

    pub fn start(&mut self, origin: Option<Span>) -> Result<TaskId, Error> {
        if !self.tasks.is_empty() {
            return Err(Error::RootExists);
        }
        self.admit(None, origin)
    }

    pub fn spawn(&mut self, parent: &TaskId, origin: Option<Span>) -> Result<TaskId, Error> {
        self.check_spawn(parent)?;
        let parent = self.running(parent)?;
        self.admit(Some(parent), origin)
    }

    /// Check admission before an adapter allocates a frame or transfers arguments.
    pub fn check_spawn(&self, parent: &TaskId) -> Result<(), Error> {
        let parent = self.running(parent)?;
        if self.closing_requested(parent) {
            return Err(Error::CancellationPending);
        }
        if self.usage.live >= self.limits.live_tasks {
            return Err(Error::LiveTaskLimit {
                limit: self.limits.live_tasks,
            });
        }
        if self.tasks.len() >= self.limits.total_tasks {
            return Err(Error::TotalTaskLimit {
                limit: self.limits.total_tasks,
            });
        }
        Ok(())
    }

    fn admit(&mut self, parent: Option<usize>, origin: Option<Span>) -> Result<TaskId, Error> {
        if self.usage.live >= self.limits.live_tasks {
            return Err(Error::LiveTaskLimit {
                limit: self.limits.live_tasks,
            });
        }
        if self.tasks.len() >= self.limits.total_tasks {
            return Err(Error::TotalTaskLimit {
                limit: self.limits.total_tasks,
            });
        }
        let index = self.tasks.len();
        self.tasks.push(Task {
            parent,
            origin,
            state: State::Ready,
            queued: false,
            cancellation_requested: false,
            live_children: 0,
            first_child_failure: None,
        });
        if let Some(parent) = parent {
            self.tasks[parent].live_children += 1;
        }
        self.usage.admitted += 1;
        self.usage.live += 1;
        self.usage.peak_live = self.usage.peak_live.max(self.usage.live);
        self.enqueue(index);
        Ok(self.handle(index))
    }

    pub fn status(&self, task: &TaskId) -> Result<Status, Error> {
        Ok(match self.tasks[self.index(task)?].state {
            State::Ready => Status::Ready,
            State::Running => Status::Running,
            State::Waiting(child) => Status::Waiting {
                child: self.handle(child),
            },
            State::Joining(_) => Status::Joining,
            State::CleanupReady(outcome) => Status::CleanupReady(outcome),
            State::Cleaning(outcome) => Status::Cleaning(outcome),
            State::Complete(outcome) => Status::Complete(outcome),
        })
    }

    pub fn parent(&self, task: &TaskId) -> Result<Option<TaskId>, Error> {
        Ok(self.tasks[self.index(task)?]
            .parent
            .map(|index| self.handle(index)))
    }

    /// The adapter supplies a span already checked against its source snapshots.
    pub fn origin(&self, task: &TaskId) -> Result<Option<Span>, Error> {
        Ok(self.tasks[self.index(task)?].origin)
    }

    pub fn cancellation_requested(&self, task: &TaskId) -> Result<bool, Error> {
        Ok(self.tasks[self.index(task)?].cancellation_requested)
    }

    pub fn is_finished(&self) -> bool {
        self.tasks.first().is_some_and(|task| task.state.complete())
    }

    pub fn dispatch(&mut self) -> Result<Option<Dispatch>, Error> {
        if self.active.is_some() {
            return Err(Error::Busy);
        }
        let Some(index) = self.queue.pop_front() else {
            return Ok(None);
        };
        self.tasks[index].queued = false;
        self.active = Some(index);
        match self.tasks[index].state {
            State::Ready => {
                self.tasks[index].state = State::Running;
                Ok(Some(Dispatch::Resume(self.handle(index))))
            }
            State::CleanupReady(mut outcome) => {
                // Cleanup entry is itself a boundary. Its adapter must see the
                // cancellation before choosing result escrow/retirement paths.
                if outcome == Outcome::Success && self.tasks[index].cancellation_requested {
                    outcome = Outcome::Cancelled;
                }
                self.tasks[index].state = State::Cleaning(outcome);
                Ok(Some(Dispatch::Cleanup {
                    task: self.handle(index),
                    outcome,
                }))
            }
            _ => unreachable!("only ready states have a queue position"),
        }
    }

    pub fn yield_now(&mut self, task: &TaskId) -> Result<(), Error> {
        let index = self.running(task)?;
        self.active = None;
        if self.closing_requested(index) {
            self.close(index, Outcome::Success);
        } else {
            self.tasks[index].state = State::Ready;
            self.enqueue(index);
        }
        self.settle();
        Ok(())
    }

    /// Always suspends, including when the direct child has already completed.
    pub fn join(&mut self, task: &TaskId, child: &TaskId) -> Result<(), Error> {
        let index = self.running(task)?;
        let child = self.index(child)?;
        if self.tasks[child].parent != Some(index) {
            return Err(Error::NotChild);
        }
        self.active = None;
        if self.closing_requested(index) {
            self.close(index, Outcome::Success);
        } else {
            self.tasks[index].state = State::Waiting(child);
        }
        self.settle();
        Ok(())
    }

    /// An adapter cancellation check that need not yield when nothing is pending.
    pub fn cancellation_point(&mut self, task: &TaskId) -> Result<Boundary, Error> {
        let index = self.running(task)?;
        if !self.closing_requested(index) {
            return Ok(Boundary::Continue);
        }
        self.active = None;
        self.close(index, Outcome::Success);
        self.settle();
        Ok(Boundary::Closing)
    }

    pub fn finish(&mut self, task: &TaskId, outcome: Outcome) -> Result<(), Error> {
        let index = self.running(task)?;
        self.active = None;
        self.close(index, outcome);
        self.settle();
        Ok(())
    }

    pub fn request_cancel(&mut self, task: &TaskId) -> Result<(), Error> {
        let index = self.index(task)?;
        if !self.tasks[index].state.complete() {
            self.tasks[index].cancellation_requested = true;
            self.cancel_children(index);
            self.settle();
        }
        Ok(())
    }

    /// Call only after actual guest cleanup. The adapter retains secondary
    /// cleanup failures when cancellation/an earlier failure remains primary.
    pub fn finish_cleanup(
        &mut self,
        task: &TaskId,
        cleanup_failure: Option<FailureId>,
    ) -> Result<Outcome, Error> {
        let index = self.active_task(task)?;
        let State::Cleaning(mut outcome) = self.tasks[index].state else {
            return Err(Error::WrongPhase);
        };
        if outcome == Outcome::Success {
            outcome = if self.tasks[index].cancellation_requested {
                Outcome::Cancelled
            } else if let Some(failure) = cleanup_failure {
                Outcome::Failed(failure)
            } else {
                Outcome::Success
            };
        }
        self.tasks[index].state = State::Complete(outcome);
        if let Some(parent) = self.tasks[index].parent {
            self.tasks[parent].live_children -= 1;
            if let Outcome::Failed(failure) = outcome {
                if self.tasks[parent]
                    .first_child_failure
                    .is_none_or(|(first, _)| index < first)
                {
                    self.tasks[parent].first_child_failure = Some((index, failure));
                }
                self.cancel_children(parent);
            }
        }
        self.usage.live -= 1;
        self.active = None;
        self.settle();
        Ok(outcome)
    }

    fn index(&self, task: &TaskId) -> Result<usize, Error> {
        if !Rc::ptr_eq(&self.invocation, &task.invocation) {
            return Err(Error::ForeignTask);
        }
        if task.index >= self.tasks.len() {
            return Err(Error::InvalidTask);
        }
        Ok(task.index)
    }

    fn active_task(&self, task: &TaskId) -> Result<usize, Error> {
        let index = self.index(task)?;
        match self.active {
            None => Err(Error::NoActiveTask),
            Some(active) if active != index => Err(Error::NotActiveTask),
            Some(_) => Ok(index),
        }
    }

    fn running(&self, task: &TaskId) -> Result<usize, Error> {
        let index = self.active_task(task)?;
        if !matches!(self.tasks[index].state, State::Running) {
            return Err(Error::WrongPhase);
        }
        Ok(index)
    }

    fn handle(&self, index: usize) -> TaskId {
        TaskId {
            invocation: Rc::clone(&self.invocation),
            index,
        }
    }

    fn enqueue(&mut self, index: usize) {
        if !self.tasks[index].queued {
            self.tasks[index].queued = true;
            self.queue.push_back(index);
        }
    }

    fn closing_requested(&self, index: usize) -> bool {
        self.tasks[index].cancellation_requested || self.tasks[index].first_child_failure.is_some()
    }

    fn close(&mut self, index: usize, mut outcome: Outcome) {
        if outcome == Outcome::Success && self.tasks[index].cancellation_requested {
            outcome = Outcome::Cancelled;
        }
        self.tasks[index].state = State::Joining(outcome);
        if self.tasks[index].queued {
            self.queue.retain(|queued| *queued != index);
            self.tasks[index].queued = false;
        }
        if outcome != Outcome::Success || self.tasks[index].first_child_failure.is_some() {
            self.cancel_children(index);
        }
    }

    /// Parents are always admitted before children. A forward pass propagates
    /// requests without recursion, scratch allocation or user callbacks.
    fn cancel_children(&mut self, parent: usize) {
        for index in parent + 1..self.tasks.len() {
            let Some(owner) = self.tasks[index].parent else {
                continue;
            };
            if (owner == parent || self.tasks[owner].cancellation_requested)
                && !self.tasks[index].state.complete()
            {
                self.tasks[index].cancellation_requested = true;
            }
        }
    }

    /// Settle finite monotonic transitions, never executing or retiring frames.
    /// Child counters change only at admission/cleanup acknowledgement. The
    /// fixed point cannot loop on an unchanged ready/waiting task. No task tree
    /// recursion uses the host stack.
    fn settle(&mut self) {
        loop {
            let mut changed = false;
            for index in (0..self.tasks.len()).rev() {
                match self.tasks[index].state {
                    State::Ready | State::Waiting(_) if self.closing_requested(index) => {
                        self.close(index, Outcome::Success);
                        changed = true;
                    }
                    State::Waiting(child) if self.tasks[child].state.complete() => {
                        self.tasks[index].state = State::Ready;
                        self.enqueue(index);
                        changed = true;
                    }
                    State::Joining(primary) if self.tasks[index].live_children == 0 => {
                        let failure = self.tasks[index].first_child_failure.map(|(_, id)| id);
                        let outcome = match (primary, failure) {
                            (Outcome::Failed(id), _) | (_, Some(id)) => Outcome::Failed(id),
                            (Outcome::Success, _) if self.tasks[index].cancellation_requested => {
                                Outcome::Cancelled
                            }
                            _ => primary,
                        };
                        self.tasks[index].state = State::CleanupReady(outcome);
                        self.enqueue(index);
                        changed = true;
                    }
                    _ => {}
                }
            }
            if !changed {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests;
