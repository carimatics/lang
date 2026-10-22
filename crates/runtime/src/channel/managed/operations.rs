use super::*;
use crate::inspection;

fn inspection_error(error: inspection::Error) -> super::super::Error {
    match error {
        inspection::Error::ServiceWork(error) => super::super::Error::Work(error),
        inspection::Error::Cells | inspection::Error::Work => super::super::Error::InspectionLimit,
        _ => super::super::Error::InvalidPayload,
    }
}
impl<'l, 'ty> Channels<'l, 'ty> {
    pub fn send_readiness<P: a::Provider>(
        &self,
        scheduler: &mut t::Scheduler,
        task: &t::TaskId,
        sender: &l::Token,
        allocator: &a::Allocator<P>,
    ) -> Result<SendReadiness, Error> {
        let index = self.index(sender, allocator)?;
        if !matches!(sender.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::Sender(_)|o::Constructor::ServiceHandle(_)))
        {
            return Err(Error::MessageType);
        }
        self.hub.running(scheduler, task)?;
        if self
            .hub
            .pending
            .iter()
            .flatten()
            .any(|p| p.operation.task() == task)
        {
            return Err(super::super::Error::Busy.into());
        }
        if matches!(
            scheduler
                .cancellation_point(task)
                .map_err(super::super::Error::from)?,
            t::Boundary::Closing
        ) {
            return Ok(SendReadiness::Closing);
        }
        if self.hub.channels[index].sealed || self.hub.channels[index].role == Role::Retired {
            self.hub.work.charge(1).map_err(super::super::Error::from)?;
            Ok(SendReadiness::Closed)
        } else {
            Ok(SendReadiness::Open)
        }
    }
    fn receiver_key<P: a::Provider>(
        layouts: &l::Layouts<'ty>,
        backing: &[Backing<'l, 'ty>],
        identity: &Rc<()>,
        observation: &inspection::Observation,
        allocator: &a::Allocator<P>,
    ) -> Result<Receiver, super::super::Error> {
        let index = usize::try_from(u64::from_le_bytes(
            observation.bytes[24..].try_into().unwrap(),
        ))
        .map_err(|_| super::super::Error::InvalidPayload)?;
        let header = a::Handle::from_token(observation.bytes[..24].try_into().unwrap())
            .map_err(|_| super::super::Error::InvalidPayload)?;
        let value = backing
            .get(index)
            // A closed service owner is still transferable receive authority.
            // Validate its live header/count/role below, not disposed storage.
            .filter(|b| b.header == Some(header))
            .ok_or(super::super::Error::ForeignEndpoint)?;
        let bytes = allocator
            .bytes(header)
            .map_err(|_| super::super::Error::InvalidPayload)?;
        let word = |i: usize| u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
        if bytes.len() != HEADER_BYTES as usize
            || word(0) != HEADER
            || word(3) != index as u64
            || word(4) == 0
            || word(5) != 1
        {
            return Err(super::super::Error::InvalidEndpoint);
        }
        let o::Type::Constructed(id) = observation.ty else {
            return Err(super::super::Error::InvalidPayload);
        };
        let valid = match (
            layouts.types().table().constructed[id.0 as usize].constructor,
            value.service.as_ref(),
        ) {
            (o::Constructor::Receiver(t), None) => {
                layouts.types().equivalent(t, value.message) == Some(true)
            }
            (o::Constructor::ServiceOwner(state, t), Some(service)) => {
                layouts.types().equivalent(t, value.message) == Some(true)
                    && layouts.types().equivalent(state, service.ty) == Some(true)
            }
            _ => false,
        };
        if !valid {
            return Err(super::super::Error::WrongRole);
        }
        if word(1)
            != Self::layout_type_index(layouts, value.message)
                .map_err(|_| super::super::Error::InvalidPayload)?
        {
            return Err(super::super::Error::InvalidEndpoint);
        }
        Ok(Receiver(Key {
            identity: Rc::clone(identity),
            index,
        }))
    }
    fn inspect_sender<P: a::Provider>(
        layouts: &l::Layouts<'ty>,
        backing: &[Backing<'l, 'ty>],
        tickets: &[Option<a::Handle>],
        observation: &inspection::Observation,
        allocator: &a::Allocator<P>,
    ) -> Result<(), super::super::Error> {
        use super::super::Error as E;
        let number = usize::try_from(u64::from_le_bytes(
            observation.bytes[24..].try_into().unwrap(),
        ))
        .map_err(|_| E::InvalidPayload)?;
        let handle = a::Handle::from_token(observation.bytes[..24].try_into().unwrap())
            .map_err(|_| E::InvalidPayload)?;
        if tickets.get(number).copied().flatten() != Some(handle) {
            return Err(E::ForeignEndpoint);
        }
        let bytes = allocator.bytes(handle).map_err(|_| E::InvalidPayload)?;
        if bytes.len() != 40 {
            return Err(E::InvalidEndpoint);
        }
        let word = |i: usize| u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
        let index = usize::try_from(word(2)).map_err(|_| E::InvalidEndpoint)?;
        let value = backing.get(index).ok_or(E::ForeignEndpoint)?;
        let o::Type::Constructed(id) = observation.ty else {
            return Err(E::WrongRole);
        };
        if !matches!((layouts.types().table().constructed[id.0 as usize].constructor,value.service.is_some()),(o::Constructor::Sender(t),false)|(o::Constructor::ServiceHandle(t),true) if layouts.types().equivalent(t,value.message)==Some(true))
        {
            return Err(E::WrongRole);
        }
        let canonical =
            Self::layout_type_index(layouts, value.message).map_err(|_| E::InvalidPayload)?;
        if word(0) != TICKET || word(1) != canonical || word(3) != number as u64 || word(4) == 0 {
            return Err(E::InvalidEndpoint);
        }
        let header = value.header.ok_or(E::InvalidEndpoint)?;
        let bytes = allocator.bytes(header).map_err(|_| E::InvalidPayload)?;
        if bytes.len() != HEADER_BYTES as usize {
            return Err(E::InvalidEndpoint);
        }
        let word = |i: usize| u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
        if word(0) != HEADER || word(1) != canonical || word(3) != index as u64 {
            return Err(E::InvalidEndpoint);
        }
        Ok(())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Inspection borrows the invocation identity, live backing, bounded walker and allocator without adding an owner registry."
    )]
    pub(super) fn inspect_publication<P: a::Provider>(
        layouts: &'l l::Layouts<'ty>,
        backing: &[Backing<'l, 'ty>],
        tickets: &[Option<a::Handle>],
        identity: &Rc<()>,
        inspector: &mut inspection::Inspector<'l, 'ty>,
        heap: Option<&crate::shared::managed::Heap<'l, 'ty>>,
        root: PublicationRoot<'_, Message<'l, 'ty>>,
        allocator: &a::Allocator<P>,
        work: &e::ServiceWork,
        visit: &mut dyn FnMut(&Receiver) -> Result<(), super::super::Error>,
    ) -> Result<(), super::super::Error> {
        let (storage, span) = match root {
            PublicationRoot::Message(message) => (
                message
                    .storage
                    .as_ref()
                    .ok_or(super::super::Error::InvalidPayload)?,
                message.span,
            ),
            PublicationRoot::State(index) => {
                let backing = backing
                    .get(index)
                    .ok_or(super::super::Error::InvalidEndpoint)?;
                let Some(state) = backing.service.as_ref() else {
                    return Ok(());
                };
                let Some(storage) = state.storage.as_ref() else {
                    return Ok(());
                };
                (storage, state.span)
            }
        };
        Self::inspect_value(
            layouts, backing, tickets, identity, inspector, heap, storage, span, allocator, work,
            visit,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Inspection borrows the existing invocation graph, storage and bounded work context without duplicating owners."
    )]
    pub(super) fn inspect_value<P: a::Provider>(
        layouts: &'l l::Layouts<'ty>,
        backing: &[Backing<'l, 'ty>],
        tickets: &[Option<a::Handle>],
        identity: &Rc<()>,
        inspector: &mut inspection::Inspector<'l, 'ty>,
        heap: Option<&crate::shared::managed::Heap<'l, 'ty>>,
        storage: &l::Reservation<'l, 'ty>,
        span: Option<Span>,
        allocator: &a::Allocator<P>,
        work: &e::ServiceWork,
        visit: &mut dyn FnMut(&Receiver) -> Result<(), super::super::Error>,
    ) -> Result<(), super::super::Error> {
        let mut read = inspector
            .read(storage, span, allocator)
            .map_err(inspection_error)?;
        let mut quota = inspection::Work::new(usize::MAX).with_service_work(work.clone());
        while let Some(observation) = read.next(&mut quota).map_err(inspection_error)? {
            let o::Type::Constructed(id) = observation.ty else {
                continue;
            };
            match layouts.types().table().constructed[id.0 as usize].constructor {
                o::Constructor::Receiver(_) | o::Constructor::ServiceOwner(..) => visit(
                    &Self::receiver_key(layouts, backing, identity, &observation, allocator)?,
                )?,
                o::Constructor::Sender(_) | o::Constructor::ServiceHandle(_) => {
                    Self::inspect_sender(layouts, backing, tickets, &observation, allocator)?
                }
                o::Constructor::Shared(target) | o::Constructor::Weak(target) => {
                    // Independently verified Share forbids physical Receiver
                    // ownership here. Sender's message parameter is phantom.
                    if !layouts
                        .types()
                        .properties(target)
                        .is_some_and(|p| p.is_share())
                    {
                        return Err(super::super::Error::InvalidPayload);
                    }
                    let token = l::Token {
                        leaf: l::Leaf {
                            ty: observation.ty,
                            offset: 0,
                            span: observation.span,
                        },
                        bytes: observation.bytes,
                    };
                    heap.ok_or(super::super::Error::InspectionContextRequired)?
                        .preview_observation(&token, 0, allocator)
                        .map_err(|_| super::super::Error::InvalidPayload)?;
                }
                o::Constructor::Builtin(o::Builtin::Bytes) => {
                    let mut bytes = observation.bytes;
                    crate::buffer::Bytes::from_token(&mut bytes, allocator)
                        .and_then(|value| value.len(allocator))
                        .map_err(|_| super::super::Error::InvalidPayload)?;
                }
                o::Constructor::Builtin(o::Builtin::Text) => {
                    let mut bytes = observation.bytes;
                    let text =
                        crate::buffer::Text::from_token_metered(&mut bytes, allocator, &mut |n| {
                            quota.service(n)
                        })
                        .map_err(|error| match error {
                            crate::buffer::Fault::ServiceWork(error) => {
                                super::super::Error::Work(error)
                            }
                            _ => super::super::Error::InvalidPayload,
                        })?;
                    let length = usize::try_from(
                        text.len(allocator)
                            .map_err(|_| super::super::Error::InvalidPayload)?,
                    )
                    .map_err(|_| super::super::Error::InvalidPayload)?;
                    quota
                        .charge_event(length, length as u64)
                        .map_err(inspection_error)?;
                    text.as_str(allocator)
                        .map_err(|_| super::super::Error::InvalidPayload)?;
                }
                _ => return Err(super::super::Error::InvalidPayload),
            }
        }
        Ok(())
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep the unique payload, checked endpoint, scheduler and read-only guest inspection context explicit."
    )]
    #[expect(
        clippy::result_large_err,
        reason = "A failed send returns the unique staged payload without allocating an error box at an allocation-failure boundary."
    )]
    pub fn send<P: a::Provider>(
        &mut self,
        scheduler: &mut t::Scheduler,
        task: &t::TaskId,
        sender: &l::Token,
        message: Message<'l, 'ty>,
        span: Option<Span>,
        inspector: &mut inspection::Inspector<'l, 'ty>,
        heap: Option<&crate::shared::managed::Heap<'l, 'ty>>,
        allocator: &a::Allocator<P>,
    ) -> Result<Send<Message<'l, 'ty>>, SendFailure<Message<'l, 'ty>>> {
        let channel = match self.index(sender, allocator) {
            Ok(index) => index,
            Err(error) => {
                return Err(SendFailure {
                    error,
                    value: message,
                });
            }
        };
        if !matches!(sender.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::Sender(_)|o::Constructor::ServiceHandle(_)))
            || message.channel != channel
        {
            return Err(SendFailure {
                error: Error::MessageType,
                value: message,
            });
        }
        let ticket = u64::from_le_bytes(sender.bytes[24..].try_into().unwrap()) as usize;
        let key = Sender {
            key: Key {
                identity: Rc::clone(&self.hub.identity),
                index: channel,
            },
            ticket,
        };
        let identity = Rc::clone(&self.hub.identity);
        let backing = &self.backing;
        let tickets = &self.tickets;
        let layouts = self.layouts;
        let work = self.hub.work.clone();
        self.hub
            .send_with_publication(
                scheduler,
                task,
                &key,
                message,
                span,
                &mut |message, visit| {
                    Self::inspect_publication(
                        layouts, backing, tickets, &identity, inspector, heap, message, allocator,
                        &work, visit,
                    )
                },
            )
            .map_err(|f| SendFailure {
                error: Error::Protocol(f.error),
                value: f.value,
            })
    }
    pub fn resume_send<P: a::Provider>(
        &mut self,
        scheduler: &mut t::Scheduler,
        operation: &Operation,
        inspector: &mut inspection::Inspector<'l, 'ty>,
        heap: Option<&crate::shared::managed::Heap<'l, 'ty>>,
        allocator: &a::Allocator<P>,
    ) -> Result<Send<Message<'l, 'ty>>, Error> {
        self.check(allocator)?;
        let identity = Rc::clone(&self.hub.identity);
        let backing = &self.backing;
        let tickets = &self.tickets;
        let layouts = self.layouts;
        let work = self.hub.work.clone();
        Ok(self
            .hub
            .resume_send_with_publication(scheduler, operation, &mut |message, visit| {
                Self::inspect_publication(
                    layouts, backing, tickets, &identity, inspector, heap, message, allocator,
                    &work, visit,
                )
            })?)
    }
    pub fn receive<P: a::Provider>(
        &mut self,
        scheduler: &mut t::Scheduler,
        task: &t::TaskId,
        receiver: &l::Token,
        span: Option<Span>,
        allocator: &a::Allocator<P>,
    ) -> Result<Receive<Message<'l, 'ty>>, Error> {
        let index = self.index(receiver, allocator)?;
        if !matches!(receiver.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::Receiver(_)|o::Constructor::ServiceOwner(..)))
        {
            return Err(Error::MessageType);
        }
        self.service_boundary(index)?;
        let received = self.hub.receive(
            scheduler,
            task,
            &mut Receiver(Key {
                identity: Rc::clone(&self.hub.identity),
                index,
            }),
            span,
        )?;
        self.service_received(index, matches!(&received, Receive::Value { .. }))?;
        Ok(received)
    }
    pub fn resume_receive(
        &mut self,
        scheduler: &mut t::Scheduler,
        operation: &Operation,
    ) -> Result<Receive<Message<'l, 'ty>>, Error> {
        let slot = self.hub.operation(operation)?;
        let index = self.hub.pending[slot].as_ref().ok_or(Error::State)?.channel;
        let received = self.hub.resume_receive(scheduler, operation)?;
        self.service_received(index, matches!(&received, Receive::Value { .. }))?;
        Ok(received)
    }
    pub fn cancel(
        &mut self,
        scheduler: &mut t::Scheduler,
        operation: &Operation,
    ) -> Result<Cancelled<Message<'l, 'ty>>, Error> {
        Ok(self.hub.cancel(scheduler, operation)?)
    }
    pub fn deadlock(&self, scheduler: &t::Scheduler) -> Result<Option<Deadlock>, Error> {
        Ok(self.hub.deadlock(scheduler)?)
    }
    pub fn seal<P: a::Provider>(
        &mut self,
        scheduler: &mut t::Scheduler,
        receiver: &l::Token,
        allocator: &a::Allocator<P>,
    ) -> Result<(), Error> {
        let index = self.index(receiver, allocator)?;
        if !matches!(receiver.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::Receiver(_)))
        {
            return Err(Error::MessageType);
        }
        self.hub.seal(
            scheduler,
            &Receiver(Key {
                identity: Rc::clone(&self.hub.identity),
                index,
            }),
        )?;
        Ok(())
    }
}
