//! Iterative cross-service destruction. Each suspended frame keeps its own
//! live guest header/control block until all descendants finish. Admission
//! therefore reserves one frame for every possible live guest allocation;
//! cleanup of valid unique owners cannot exhaust this pool. Publication and
//! graph/brand admission remain the execution adapter's responsibility.
use crate::{allocation as a, cleanup as c, layout as l, map, shared::managed as g, vector};
use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};

const FAULT: u32 = 0x4452_0001;
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    Layout(l::Error),
    Vector(vector::Error),
    Map(map::Error),
    Shared(g::Error),
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
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        self.check(allocator)?;
        if !heap.uses_layouts(self.layouts) {
            return Err(Error::State);
        }
        self.vacancy()?;
        if let Some(level) = heap.retire_read(read, ty, allocator)? {
            self.frames.push(Frame::Shared {
                level,
                span: context.span(),
            });
        }
        self.run(&mut Some(heap), allocator, context, &mut cleaner)
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
        heap: &mut Option<&mut g::Heap<'l, 't>>,
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
                if let Some(level) = heap
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
            _ => cleaner(token, allocator, context.reason(), context),
        }
    }
    fn run<P: a::Provider, F>(
        &mut self,
        heap: &mut Option<&mut g::Heap<'l, 't>>,
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
                Frame::Shared { level, span } => heap
                    .as_deref_mut()
                    .ok_or(Error::State)?
                    .retirement_step(*level, allocator)?
                    .map(|(t, customer)| (t, customer, *span)),
            };
            if let Some((token, customer, parent_span)) = next {
                let span = token.leaf.span.or(parent_span);
                context.field(
                    token,
                    span,
                    &mut c::Contextual::new(
                        |token: &mut l::Token, _, context: &mut c::Context<'_>| {
                            self.dispatch(token, customer, heap, allocator, context, cleaner)
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
        mut heap: Option<&mut g::Heap<'l, 't>>,
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
        if heap
            .as_ref()
            .is_some_and(|heap| !heap.uses_layouts(self.layouts))
        {
            return Err(FAULT);
        }
        self.dispatch(token, 0, &mut heap, allocator, context, &mut cleaner)?;
        self.run(&mut heap, allocator, context, &mut cleaner)
            .map_err(|_| FAULT)
    }
}
