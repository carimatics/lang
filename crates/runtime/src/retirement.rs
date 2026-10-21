//! Iterative cross-service destruction. Each suspended frame keeps its own
//! live guest header/control block until all descendants finish. Admission
//! therefore reserves one frame for every possible live guest allocation;
//! cleanup of valid unique owners cannot exhaust this pool. Publication and
//! graph/brand admission remain the execution adapter's responsibility.
use crate::{
    allocation as a, channel::managed as ch, cleanup as c, layout as l, map, shared::managed as g,
    task, vector,
};
use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};

const FAULT: u32 = 0x4452_0001;
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    Layout(l::Error),
    Vector(vector::Error),
    Map(map::Error),
    Shared(g::Error),
    Channel(ch::Error),
    ForeignAllocator,
    State,
}
impl From<l::Error> for Error {
    fn from(e: l::Error) -> Self {
        Self::Layout(e)
    }
}
impl From<vector::Error> for Error {
    fn from(e: vector::Error) -> Self {
        Self::Vector(e)
    }
}
impl From<map::Error> for Error {
    fn from(e: map::Error) -> Self {
        Self::Map(e)
    }
}
impl From<g::Error> for Error {
    fn from(e: g::Error) -> Self {
        Self::Shared(e)
    }
}
impl From<ch::Error> for Error {
    fn from(error: ch::Error) -> Self {
        Self::Channel(error)
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "service retirement failed: {self:?}")
    }
}
impl std::error::Error for Error {}

enum Frame<'l, 't> {
    Vector {
        value: vector::Retired<'l, 't>,
        customer: u64,
        span: Option<m::Span>,
    },
    Map {
        value: map::Retired<'l, 't>,
        customer: u64,
        span: Option<m::Span>,
    },
    Shared {
        level: usize,
        span: Option<m::Span>,
    },
    Channel {
        value: ch::Retired<'l, 't>,
        span: Option<m::Span>,
    },
}
/// Borrow the existing invocation services for one common iterative drain.
pub(crate) struct Services<'a, 'l, 't> {
    pub heap: Option<&'a mut g::Heap<'l, 't>>,
    pub channels: Option<(&'a mut ch::Channels<'l, 't>, &'a mut task::Scheduler)>,
}
pub struct Drain<'l, 't> {
    layouts: &'l l::Layouts<'t>,
    allocator: a::Identity,
    capacity: usize,
    frames: Vec<Frame<'l, 't>>,
    usage: m::Usage,
}
impl<'l, 't> Drain<'l, 't> {
    pub fn prepare<P: a::Provider>(
        layouts: &'l l::Layouts<'t>,
        allocator: &a::Allocator<P>,
        limits: m::Limits,
        initial: m::Usage,
    ) -> Result<Self, Error> {
        let base = layouts.usage();
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(Error::State);
        }
        let mut budget = m::Budget::with_usage(limits, initial).map_err(l::Error::from)?;
        let capacity = allocator.limits().live_allocations;
        let frames = budget.vector(capacity).map_err(l::Error::from)?;
        Ok(Self {
            layouts,
            allocator: allocator.identity(),
            capacity,
            frames,
            usage: m::Usage {
                work: budget.work_used(),
                allocation_bytes: budget.allocation_bytes(),
                elements: budget.elements_used(),
            },
        })
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    /// End a retained read through the same iterative drain when it releases
    /// the final strong count. A heap step completes before child dispatch.
    pub fn end_read<P: a::Provider>(
        &mut self,
        read: g::Read,
        ty: o::Type,
        heap: &mut g::Heap<'l, 't>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        self.end_read_with_services(
            read,
            ty,
            Services {
                heap: Some(heap),
                channels: None,
            },
            allocator,
            context,
            cleaner,
        )
    }
    pub(crate) fn end_read_with_services<P: a::Provider>(
        &mut self,
        read: g::Read,
        ty: o::Type,
        mut services: Services<'_, 'l, 't>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        self.check(allocator)?;
        self.check_services(&services)?;
        self.vacancy()?;
        if let Some(level) = services
            .heap
            .as_deref_mut()
            .ok_or(Error::State)?
            .retire_read(read, ty, allocator)?
        {
            self.frames.push(Frame::Shared {
                level,
                span: context.span(),
            });
        }
        self.run(&mut services, allocator, context, &mut cleaner)
    }
    fn check_services(&self, services: &Services<'_, 'l, 't>) -> Result<(), Error> {
        if services
            .heap
            .as_ref()
            .is_some_and(|heap| !heap.uses_layouts(self.layouts))
            || services
                .channels
                .as_ref()
                .is_some_and(|(channels, _)| !channels.uses_layouts(self.layouts))
        {
            return Err(Error::State);
        }
        Ok(())
    }
    fn check<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<(), Error> {
        if self.allocator != allocator.identity() {
            return Err(Error::ForeignAllocator);
        }
        if !self.frames.is_empty() {
            return Err(Error::State);
        }
        Ok(())
    }
    fn vacancy(&self) -> Result<(), Error> {
        if self.frames.len() == self.capacity {
            return Err(Error::State);
        }
        Ok(())
    }
    fn dispatch<P: a::Provider, F>(
        &mut self,
        token: &mut l::Token,
        customer: u64,
        services: &mut Services<'_, 'l, 't>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: &mut F,
    ) -> Result<(), u32>
    where
        F: FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    {
        let constructor = match self.layouts.get(token.leaf.ty).map(|l| &l.kind) {
            Some(l::Kind::Token(constructor)) => *constructor,
            _ => return Err(FAULT),
        };
        match constructor {
            o::Constructor::Vec(ty) => {
                self.vacancy().map_err(|_| FAULT)?;
                let mut value =
                    vector::Vector::from_token(self.layouts, ty, &mut token.bytes, allocator)
                        .map_err(|_| FAULT)?;
                let value = value.retire(allocator).map_err(|_| FAULT)?;
                self.frames.push(Frame::Vector {
                    value,
                    customer,
                    span: context.span(),
                });
                Ok(())
            }
            o::Constructor::Map(key, value) => {
                self.vacancy().map_err(|_| FAULT)?;
                let mut value =
                    map::Map::from_token(self.layouts, key, value, &mut token.bytes, allocator)
                        .map_err(|_| FAULT)?;
                let value = value.retire(allocator).map_err(|_| FAULT)?;
                self.frames.push(Frame::Map {
                    value,
                    customer,
                    span: context.span(),
                });
                Ok(())
            }
            o::Constructor::Shared(_) | o::Constructor::Weak(_) => {
                self.vacancy().map_err(|_| FAULT)?;
                if let Some(level) = services
                    .heap
                    .as_deref_mut()
                    .ok_or(FAULT)?
                    .retire_obligation(token, customer, allocator)
                    .map_err(|_| FAULT)?
                {
                    self.frames.push(Frame::Shared {
                        level,
                        span: context.span(),
                    });
                }
                Ok(())
            }
            o::Constructor::ChannelSetup(_)
            | o::Constructor::Sender(_)
            | o::Constructor::Receiver(_)
            | o::Constructor::ServiceSetup(..)
            | o::Constructor::ServiceOwner(..)
            | o::Constructor::ServiceHandle(_) => {
                self.vacancy().map_err(|_| FAULT)?;
                let (channels, scheduler) = services.channels.as_mut().ok_or(FAULT)?;
                if let Some(value) = channels
                    .begin_retirement(token, scheduler, allocator)
                    .map_err(|_| FAULT)?
                {
                    self.frames.push(Frame::Channel {
                        value,
                        span: context.span(),
                    });
                }
                Ok(())
            }
            _ => cleaner(token, allocator, context.reason(), context),
        }
    }
    fn run<P: a::Provider, F>(
        &mut self,
        services: &mut Services<'_, 'l, 't>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: &mut F,
    ) -> Result<(), Error>
    where
        F: FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    {
        while let Some(frame) = self.frames.last_mut() {
            let next = match frame {
                Frame::Vector {
                    value,
                    customer,
                    span,
                } => value.next(allocator)?.map(|t| (t, *customer, *span)),
                Frame::Map {
                    value,
                    customer,
                    span,
                } => value.next(allocator)?.map(|t| (t, *customer, *span)),
                Frame::Shared { level, span } => services
                    .heap
                    .as_deref_mut()
                    .ok_or(Error::State)?
                    .retirement_step(*level, allocator)?
                    .map(|(t, customer)| (t, customer, *span)),
                Frame::Channel { value, span } => value
                    .next(services.channels.as_mut().ok_or(Error::State)?.0, allocator)?
                    .map(|token| (token, 0, *span)),
            };
            if let Some((token, customer, parent_span)) = next {
                let span = token.leaf.span.or(parent_span);
                context.field(
                    token,
                    span,
                    &mut c::Contextual::new(
                        |token: &mut l::Token, _, context: &mut c::Context<'_>| {
                            self.dispatch(token, customer, services, allocator, context, cleaner)
                        },
                    ),
                );
            } else {
                self.frames.pop();
            }
        }
        Ok(())
    }
    /// Called within the owner's common cleanup context. Every yielded child
    /// runs through that same report, including failure/panic containment. The
    /// leaf callback must consume its obligation before failing or panicking.
    pub fn token<P: a::Provider>(
        &mut self,
        token: &mut l::Token,
        heap: Option<&mut g::Heap<'l, 't>>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), u32> {
        self.token_with_services(
            token,
            Services {
                heap,
                channels: None,
            },
            allocator,
            context,
            cleaner,
        )
    }
    pub(crate) fn token_with_services<P: a::Provider>(
        &mut self,
        token: &mut l::Token,
        mut services: Services<'_, 'l, 't>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), u32> {
        self.check(allocator).map_err(|_| FAULT)?;
        self.check_services(&services).map_err(|_| FAULT)?;
        self.dispatch(token, 0, &mut services, allocator, context, &mut cleaner)?;
        self.run(&mut services, allocator, context, &mut cleaner)
            .map_err(|_| FAULT)
    }
    pub(crate) fn message<P: a::Provider>(
        &mut self,
        message: &mut ch::Message<'l, 't>,
        mut services: Services<'_, 'l, 't>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), u32> {
        self.check(allocator).map_err(|_| FAULT)?;
        self.check_services(&services).map_err(|_| FAULT)?;
        if services.channels.is_none() {
            return Err(FAULT);
        }
        self.vacancy().map_err(|_| FAULT)?;
        let value = ch::Retired::message(message, allocator).map_err(|_| FAULT)?;
        self.frames.push(Frame::Channel {
            value,
            span: context.span(),
        });
        self.run(&mut services, allocator, context, &mut cleaner)
            .map_err(|_| FAULT)
    }
}
