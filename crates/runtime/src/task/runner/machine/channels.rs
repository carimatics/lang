use super::*;
use crate::channel::{self as ch, managed as g};

fn failure(error: g::Error) -> Reason {
    match error {
        g::Error::Protocol(ch::Error::Work(e)) => Reason::Control(e),
        g::Error::Layout(l::Error::ServiceWork(e)) => Reason::Control(e),
        e => Reason::Channel(e),
    }
}
fn observed(capture: &g::Capture) -> l::Token {
    let token = capture.token();
    l::Token {
        leaf: l::Leaf {
            ty: token.leaf.ty,
            offset: token.leaf.offset,
            span: token.leaf.span,
        },
        bytes: token.bytes,
    }
}
impl<'p, 'l, 'ty, 'host, 'ctrl, P: a::Provider> Machine<'p, 'l, 'ty, 'host, 'ctrl, P> {
    fn close_channel_send(
        &mut self,
        task: usize,
        payload: usize,
        destination: usize,
        span: m::Span,
    ) -> Result<(), Reason> {
        let frame = self.records[task]
            .frames
            .last_mut()
            .ok_or(Reason::Invariant)?;
        frame.available(payload)?;
        let copy = frame.is_copy(payload)?;
        let owner = frame
            .managed
            .as_ref()
            .and_then(|m| m.spans[payload])
            .unwrap_or(span);
        let (source, target) = frame
            .storage
            .as_mut()
            .ok_or(Reason::Invariant)?
            .two_cells_mut(payload, destination)?;
        let root = target.root();
        target.prepare_case(root, 1, self.allocator)?;
        let at = target.child(root, 0, self.allocator)?;
        if copy {
            target.copy_from(at, source, source.root(), self.allocator)?;
        } else {
            source.move_root_into(target, at, self.allocator)?;
            source.recycle(self.allocator)?;
        }
        target.initialize_order(root, [(0, owner)].into_iter(), self.allocator)?;
        if !copy {
            frame.forget(payload);
        }
        frame.mark(destination, span)
    }
    pub(super) fn value_cells(&self) -> Result<usize, Reason> {
        self.live_cells
            .checked_add(
                self.services
                    .channels
                    .as_ref()
                    .map_or(0, g::Channels::live_cells),
            )
            .ok_or(Reason::Invariant)
    }
    pub fn channel_usage(&mut self) -> Result<(), Reason> {
        self.local_peak_cells = self.local_peak_cells.max(self.value_cells()?);
        Ok(())
    }
    fn channel_admit(&self, additional: usize) -> Result<(), Reason> {
        self.control.check()?;
        let local = e::LiveUsage {
            frames: 0,
            cells: self
                .value_cells()?
                .checked_add(additional)
                .ok_or(e::Failure::CounterOverflow)?,
        };
        let cells = self
            .accounting
            .as_ref()
            .map_or(Ok(local), |accounting| accounting.live_with_parent(local))?
            .cells;
        if self
            .control
            .limits()
            .live_cells
            .is_some_and(|limit| cells > limit)
        {
            return Err(Reason::Execution(f::ExecutionFailure::CellLimit));
        }
        Ok(())
    }
    pub fn admit_channel_create(&self, task: usize, capacity: usize) -> Result<(), Reason> {
        let l::Immediate::I64(capacity) = self.frame(task)?.read(capacity, self.allocator)? else {
            return Err(Reason::Invariant);
        };
        if capacity > 0 {
            self.channel_admit(usize::try_from(capacity).map_err(|_| Reason::Invariant)?)?;
        }
        Ok(())
    }
    pub fn admit_service_create(&self, task: usize, capacity: usize) -> Result<(), Reason> {
        let l::Immediate::I64(capacity) = self.frame(task)?.read(capacity, self.allocator)? else {
            return Err(Reason::Invariant);
        };
        if capacity > 0 {
            self.channel_admit(
                usize::try_from(capacity)
                    .ok()
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Reason::Invariant)?,
            )?;
        }
        Ok(())
    }
    pub(super) fn cancel_channel_waits(&mut self, task: usize) -> Result<(), Reason> {
        let mut failed = false;
        for index in (0..self.records[task].frames.len()).rev() {
            let Some(wait) = self.records[task].frames[index].channel_wait.take() else {
                continue;
            };
            self.records[task].frames[index].pending = None;
            let canceled = self
                .services
                .channels
                .as_mut()
                .ok_or(Reason::Invariant)?
                .cancel(&mut self.services.scheduler, &wait.operation)
                .map_err(failure)?;
            if let ch::Cancelled::Send(message) = canceled {
                let owner_span = message.span.or(Some(wait.span));
                let types = self.proof.typed().places().declarations().types();
                let services = &mut self.services;
                let session = &mut self.records[task].frames[index]
                    .managed
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .session;
                let allocator = &mut self.allocator;
                failed |= session
                    .retire(
                        c::Slot(wait.destination),
                        message,
                        owner_span,
                        c::Reason::Failure,
                        &mut c::Contextual::new(
                            |message: &mut g::Message<'l, 'ty>, _, context: &mut c::Context<'_>| {
                                services.retire_message(types, message, allocator, context)
                            },
                        ),
                    )
                    .is_err();
            }
            let mut access =
                frame::Access::new(&mut self.records[task].frames[..=index], &mut self.services)?;
            failed |= access
                .release_channel_capture(wait.capture, wait.span, self.allocator)
                .is_err();
        }
        if failed { Err(Reason::Cleanup) } else { Ok(()) }
    }
    fn complete_channel(
        &mut self,
        task: usize,
        destination: usize,
        case: u32,
        mut message: Option<g::Message<'l, 'ty>>,
        span: m::Span,
    ) -> Result<(), Reason> {
        let frame = self.records[task]
            .frames
            .last_mut()
            .ok_or(Reason::Invariant)?;
        let cell = frame.data()?.cell(destination)?;
        let root = cell.root();
        cell.prepare_case(root, case, self.allocator)?;
        if let Some(message) = message.as_mut() {
            let at = cell.child(root, 0, self.allocator)?;
            message
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .move_root_into(cell, at, self.allocator)?;
            cell.initialize_order(
                root,
                [(0, message.span.unwrap_or(span))].into_iter(),
                self.allocator,
            )?;
        } else {
            cell.initialize_order(root, [].into_iter(), self.allocator)?;
        }
        frame.mark(destination, span)?;
        if let Some(message) = message {
            self.services
                .channels
                .as_mut()
                .ok_or(Reason::Invariant)?
                .recycle(message, self.allocator)
                .map_err(failure)?;
        }
        Ok(())
    }
    fn release_channel(
        &mut self,
        task: usize,
        capture: g::Capture,
        span: m::Span,
    ) -> Result<(), Reason> {
        frame::Access::new(&mut self.records[task].frames, &mut self.services)?
            .release_channel_capture(capture, span, self.allocator)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the verified authority/payload/destination and both continuation edges explicit."
    )]
    pub fn channel_send(
        &mut self,
        task: usize,
        authority: usize,
        payload: usize,
        destination: usize,
        next: f::NodeId,
        failed: f::NodeId,
        span: m::Span,
    ) -> Result<bool, Reason> {
        let endpoint = observed(
            self.frame(task)?
                .channel_prepared
                .get(authority)
                .and_then(Option::as_ref)
                .ok_or(Reason::Invariant)?,
        );
        match self
            .services
            .channels
            .as_ref()
            .ok_or(Reason::Invariant)?
            .send_readiness(
                &mut self.services.scheduler,
                &self.records[task].id,
                &endpoint,
                self.allocator,
            )
            .map_err(failure)?
        {
            g::SendReadiness::Closing => {
                self.active = None;
                return Ok(false);
            }
            g::SendReadiness::Closed => {
                self.close_channel_send(task, payload, destination, span)?;
                let capture = self.frame_mut(task)?.channel_prepared[authority]
                    .take()
                    .ok_or(Reason::Invariant)?;
                self.release_channel(task, capture, span)?;
                return Ok(true);
            }
            g::SendReadiness::Open => {}
        }
        let additional = self
            .services
            .channels
            .as_ref()
            .ok_or(Reason::Invariant)?
            .staging_cells(&endpoint, self.allocator)
            .map_err(failure)?;
        self.channel_admit(additional)?;
        let frame = self.records[task]
            .frames
            .last_mut()
            .ok_or(Reason::Invariant)?;
        frame.available(payload)?;
        let owner = frame
            .managed
            .as_ref()
            .and_then(|m| m.spans[payload])
            .unwrap_or(span);
        let copy = frame.is_copy(payload)?;
        let source = frame
            .storage
            .as_mut()
            .ok_or(Reason::Invariant)?
            .cell_mut(payload)?;
        let message = self
            .services
            .channels
            .as_mut()
            .ok_or(Reason::Invariant)?
            .stage(&endpoint, Some(owner), self.allocator)
            .map_err(failure)?;
        let target = message.storage.as_ref().ok_or(Reason::Invariant)?;
        if copy {
            // Staging is a typed custody copy, with the same unmetered data
            // transfer as the old scalar path. Inspection/commit charge work.
            if let Err(error) =
                target.copy_from(target.root(), source, source.root(), self.allocator)
            {
                self.services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .recycle(message, self.allocator)
                    .map_err(failure)?;
                return Err(error.into());
            }
        } else {
            source.move_root_into(target, target.root(), self.allocator)?;
            source.recycle(self.allocator)?;
            frame.forget(payload);
        }
        self.channel_usage()?;
        let outcome = self
            .services
            .channels
            .as_mut()
            .ok_or(Reason::Invariant)?
            .send(
                &mut self.services.scheduler,
                &self.records[task].id,
                &endpoint,
                message,
                Some(span),
                self.services.inspection.as_mut().ok_or(Reason::Invariant)?,
                self.services.heap.as_ref(),
                self.allocator,
            );
        match outcome {
            Ok(ch::Send::Waiting(operation)) => {
                let frame = self.frame_mut(task)?;
                let capture = frame.channel_prepared[authority]
                    .take()
                    .ok_or(Reason::Invariant)?;
                frame.channel_wait = Some(frame::ChannelWait {
                    operation,
                    capture,
                    destination,
                    send: true,
                    payload: Some(payload),
                    next,
                    failure: failed,
                    span,
                });
                frame.pending = Some(frame::Pending::Channel);
                self.active = None;
                Ok(false)
            }
            Ok(outcome) => {
                match outcome {
                    ch::Send::Sent { .. } => {
                        self.complete_channel(task, destination, 0, None, span)?
                    }
                    ch::Send::Rejected { reason, value } => self.complete_channel(
                        task,
                        destination,
                        if reason == ch::Rejection::Closed {
                            1
                        } else {
                            2
                        },
                        Some(value),
                        span,
                    )?,
                    ch::Send::Closing(value) => {
                        self.restore_channel_payload(task, payload, value, owner)?;
                        self.active = None;
                        return Ok(false);
                    }
                    ch::Send::Waiting(_) => unreachable!(),
                }
                let capture = self.frame_mut(task)?.channel_prepared[authority]
                    .take()
                    .ok_or(Reason::Invariant)?;
                self.release_channel(task, capture, span)?;
                Ok(true)
            }
            Err(error) => {
                self.restore_channel_payload(task, payload, error.value, owner)?;
                Err(failure(error.error))
            }
        }
    }
    fn restore_channel_payload(
        &mut self,
        task: usize,
        payload: usize,
        mut message: g::Message<'l, 'ty>,
        span: m::Span,
    ) -> Result<(), Reason> {
        let frame = self.records[task]
            .frames
            .last_mut()
            .ok_or(Reason::Invariant)?;
        let target = frame.data()?.cell(payload)?;
        message
            .storage
            .as_mut()
            .ok_or(Reason::Invariant)?
            .move_root_into(target, target.root(), self.allocator)?;
        frame.mark(payload, span)?;
        self.services
            .channels
            .as_mut()
            .ok_or(Reason::Invariant)?
            .recycle(message, self.allocator)
            .map_err(failure)
    }
    fn finish_service_boundary(
        &mut self,
        task: usize,
        endpoint: &l::Token,
        span: m::Span,
    ) -> Result<(), Reason> {
        let mut frame = frame::Access::new(&mut self.records[task].frames, &mut self.services)?;
        frame.finish_service_endpoint(endpoint, span, self.allocator)
    }
    pub fn channel_receive(
        &mut self,
        task: usize,
        authority: usize,
        destination: usize,
        next: f::NodeId,
        failed: f::NodeId,
        span: m::Span,
    ) -> Result<bool, Reason> {
        let endpoint = observed(
            self.frame(task)?
                .channel_prepared
                .get(authority)
                .and_then(Option::as_ref)
                .ok_or(Reason::Invariant)?,
        );
        self.finish_service_boundary(task, &endpoint, span)?;
        let outcome = self
            .services
            .channels
            .as_mut()
            .ok_or(Reason::Invariant)?
            .receive(
                &mut self.services.scheduler,
                &self.records[task].id,
                &endpoint,
                Some(span),
                self.allocator,
            )
            .map_err(failure)?;
        match outcome {
            ch::Receive::Waiting(operation) => {
                let frame = self.frame_mut(task)?;
                let capture = frame.channel_prepared[authority]
                    .take()
                    .ok_or(Reason::Invariant)?;
                frame.channel_wait = Some(frame::ChannelWait {
                    operation,
                    capture,
                    destination,
                    send: false,
                    payload: None,
                    next,
                    failure: failed,
                    span,
                });
                frame.pending = Some(frame::Pending::Channel);
                self.active = None;
                Ok(false)
            }
            ch::Receive::Closing => {
                self.active = None;
                Ok(false)
            }
            outcome => {
                match outcome {
                    ch::Receive::Value { value, .. } => {
                        self.complete_channel(task, destination, 1, Some(value), span)?
                    }
                    ch::Receive::Closed => {
                        self.complete_channel(task, destination, 0, None, span)?
                    }
                    _ => unreachable!(),
                }
                let capture = self.frame_mut(task)?.channel_prepared[authority]
                    .take()
                    .ok_or(Reason::Invariant)?;
                self.release_channel(task, capture, span)?;
                Ok(true)
            }
        }
    }
    pub(super) fn pending_channel(&mut self, task: usize) -> Result<bool, Reason> {
        enum Completion<'l, 'ty> {
            Value(u32, Option<g::Message<'l, 'ty>>),
            Closing(Option<g::Message<'l, 'ty>>),
            Unexpected,
        }
        let wait = self
            .frame_mut(task)?
            .channel_wait
            .take()
            .ok_or(Reason::Invariant)?;
        let resumed = if wait.send {
            self.services
                .channels
                .as_mut()
                .ok_or(Reason::Invariant)?
                .resume_send(
                    &mut self.services.scheduler,
                    &wait.operation,
                    self.services.inspection.as_mut().ok_or(Reason::Invariant)?,
                    self.services.heap.as_ref(),
                    self.allocator,
                )
                .map_err(failure)
                .map(|outcome| match outcome {
                    ch::Send::Sent { .. } => Completion::Value(0, None),
                    ch::Send::Rejected { reason, value } => Completion::Value(
                        if reason == ch::Rejection::Closed {
                            1
                        } else {
                            2
                        },
                        Some(value),
                    ),
                    ch::Send::Closing(value) => Completion::Closing(Some(value)),
                    ch::Send::Waiting(_) => Completion::Unexpected,
                })
        } else {
            (|| {
                let endpoint = observed(&wait.capture);
                self.finish_service_boundary(task, &endpoint, wait.span)?;
                self.services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .resume_receive(&mut self.services.scheduler, &wait.operation)
                    .map_err(failure)
                    .map(|outcome| match outcome {
                        ch::Receive::Value { value, .. } => Completion::Value(1, Some(value)),
                        ch::Receive::Closed => Completion::Value(0, None),
                        ch::Receive::Closing => Completion::Closing(None),
                        ch::Receive::Waiting(_) => Completion::Unexpected,
                    })
            })()
        };
        let completion = match resumed {
            Ok(completion) => completion,
            Err(reason) => {
                self.frame_mut(task)?.channel_wait = Some(wait);
                return Err(reason);
            }
        };
        match completion {
            Completion::Unexpected => {
                self.frame_mut(task)?.channel_wait = Some(wait);
                Err(Reason::Invariant)
            }
            Completion::Closing(message) => {
                if let Some(message) = message {
                    self.restore_channel_payload(
                        task,
                        wait.payload.ok_or(Reason::Invariant)?,
                        message,
                        wait.span,
                    )?;
                }
                self.release_channel(task, wait.capture, wait.span)?;
                self.active = None;
                Ok(false)
            }
            Completion::Value(case, message) => {
                let result =
                    self.complete_channel(task, wait.destination, case, message, wait.span);
                self.release_channel(task, wait.capture, wait.span)?;
                match result {
                    Ok(()) => self.frame_mut(task)?.node = wait.next,
                    Err(reason) => {
                        self.fault(task, reason, wait.span)?;
                        self.frame_mut(task)?.node = wait.failure;
                    }
                }
                Ok(true)
            }
        }
    }
}
