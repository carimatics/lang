//! Invocation-owned managed services. Their metadata is prepared before entry.
use super::*;
use crate::{retirement as r, shared::managed as sh};

pub fn indirect(operation: o::core::Operation) -> bool {
    use o::core::Operation::*;
    matches!(
        operation,
        VecNew
            | VecLen
            | VecPush
            | VecPop
            | VecGet
            | VecGetMut
            | MapNew
            | MapLen
            | MapInsert
            | MapGet
            | MapRemove
            | SharedNew
            | SharedClone
            | SharedRead
            | SharedReplace
            | WeakNew
            | WeakClone
            | WeakUpgrade
    )
}
pub fn shared(operation: o::core::Operation) -> bool {
    use o::core::Operation::*;
    matches!(
        operation,
        SharedNew | SharedClone | SharedRead | SharedReplace | WeakNew | WeakClone | WeakUpgrade
    )
}
fn preparation(error: sh::Error) -> Reason {
    match error.kind {
        sh::Kind::Layout(e) => Reason::Layout(e),
        _ => Reason::Invariant,
    }
}
fn cleanup_leaf<P: a::Provider>(
    component: &mut Option<&mut dyn crate::component::execution::Host>,
    types: &o::VerifiedTypes,
    token: &mut l::Token,
    allocator: &mut a::Allocator<P>,
    context: &mut c::Context<'_>,
) -> Result<(), u32> {
    if matches!(token.leaf.ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::ComponentResource(_)))
    {
        component
            .as_deref_mut()
            .ok_or(3u32)?
            .retire(token, allocator, context)
    } else {
        crate::buffer::cleanup_token(types, token, allocator).map_err(|_| 3u32)
    }
}
pub struct Services<'l, 'ty> {
    pub component: Option<&'l mut dyn crate::component::execution::Host>,
    pub scheduler: t::Scheduler,
    pub channels: Option<crate::channel::managed::Channels<'l, 'ty>>,
    pub work: e::ServiceWork,
    pub layouts: &'l l::Layouts<'ty>,
    pub drain: Option<r::Drain<'l, 'ty>>,
    pub heap: Option<sh::Heap<'l, 'ty>>,
    pub opened: bool,
    pub inspection: Option<crate::inspection::Inspector<'l, 'ty>>,
}
impl<'l, 'ty> Services<'l, 'ty> {
    #[expect(
        clippy::too_many_arguments,
        reason = "Invocation preparation shares the existing proof, frame, allocator, scheduler and finite service budgets."
    )]
    pub fn prepare<P: a::Provider>(
        proof: &o::tasks::VerifiedTasks,
        plans: &[Option<l::FramePlan<'l, 'ty>>],
        layouts: &'l l::Layouts<'ty>,
        allocator: &a::Allocator<P>,
        scheduler: t::Scheduler,
        root: &t::TaskId,
        channel_limits: crate::channel::Limits,
        channel_cells: usize,
        work_limit: Option<u64>,
        budget: &mut m::Budget,
    ) -> Result<Self, Reason> {
        let work = e::ServiceWork::new(work_limit);
        let mut has_indirect = false;
        let mut has_shared = false;
        let mut has_arena = false;
        let mut has_channels = false;
        for (index, plan) in plans.iter().enumerate() {
            if plan.is_none() {
                continue;
            }
            for instruction in proof.typed().functions()[index]
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
            {
                budget.charge(1)?;
                has_arena |= matches!(instruction.op, b::Op::ArenaEnter { .. });
                has_channels |= matches!(
                    instruction.op,
                    b::Op::PrepareChannel { .. }
                        | b::Op::ChannelSend { .. }
                        | b::Op::ChannelReceive { .. }
                        | b::Op::PrepareService { .. }
                        | b::Op::ServiceSend { .. }
                        | b::Op::ServiceReceive { .. }
                );
                if let b::Op::CoreCall(call) = &instruction.op {
                    has_indirect |= indirect(call.operation);
                    has_shared |= shared(call.operation);
                    has_channels |= call.operation.is_channel()
                        && call.operation != o::core::Operation::ChannelUnsent;
                    has_channels |= call.operation.is_service();
                }
            }
        }
        has_indirect |= has_arena || has_channels;
        let inspection = if has_arena || has_channels {
            let inspector = crate::inspection::Inspector::prepare(
                layouts,
                crate::arena::managed::Limits::default().value_cells,
                budget.limits(),
                usage(budget),
            )
            .map_err(|e| match e {
                crate::inspection::Error::Layout(e) => Reason::Layout(e),
                _ => Reason::Invariant,
            })?;
            *budget = m::Budget::with_usage(budget.limits(), inspector.usage())?;
            Some(inspector)
        } else {
            None
        };
        let heap = if has_shared {
            let heap =
                sh::Heap::prepare(layouts, Default::default(), budget.limits(), usage(budget))
                    .map_err(preparation)?
                    .with_service_work(work.clone());
            *budget = m::Budget::with_usage(budget.limits(), heap.usage())?;
            Some(heap)
        } else {
            None
        };
        let drain = if has_indirect {
            let drain = r::Drain::prepare(layouts, allocator, budget.limits(), usage(budget))
                .map_err(|e| match e {
                    r::Error::Layout(e) => Reason::Layout(e),
                    _ => Reason::Invariant,
                })?;
            *budget = m::Budget::with_usage(budget.limits(), drain.usage())?;
            Some(drain)
        } else {
            None
        };
        let channels = if has_channels {
            Some(
                crate::channel::managed::Channels::prepare(
                    layouts,
                    allocator,
                    &scheduler,
                    root,
                    channel_limits,
                    channel_cells,
                    work.clone(),
                    budget,
                )
                .map_err(Reason::Channel)?,
            )
        } else {
            None
        };
        Ok(Self {
            component: None,
            scheduler,
            channels,
            work,
            layouts,
            heap,
            drain,
            opened: false,
            inspection,
        })
    }
    pub fn retire<P: a::Provider>(
        &mut self,
        types: &o::VerifiedTypes,
        token: &mut l::Token,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let meter = self.work.clone();
        meter.unmetered(|| {
            let component = &mut self.component;
            if let Some(drain) = &mut self.drain {
                drain.token_with_services(
                    token,
                    r::Services {
                        heap: self.heap.as_mut(),
                        channels: self
                            .channels
                            .as_mut()
                            .map(|channels| (channels, &mut self.scheduler)),
                    },
                    allocator,
                    context,
                    |token, allocator, _, context| {
                        cleanup_leaf(component, types, token, allocator, context)
                    },
                )
            } else {
                cleanup_leaf(component, types, token, allocator, context)
            }
        })
    }
    pub fn retire_message<P: a::Provider>(
        &mut self,
        types: &o::VerifiedTypes,
        message: &mut crate::channel::managed::Message<'l, 'ty>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let meter = self.work.clone();
        meter.unmetered(|| {
            let component = &mut self.component;
            self.drain.as_mut().ok_or(3u32)?.message(
                message,
                r::Services {
                    heap: self.heap.as_mut(),
                    channels: self
                        .channels
                        .as_mut()
                        .map(|channels| (channels, &mut self.scheduler)),
                },
                allocator,
                context,
                |token, allocator, _, context| {
                    cleanup_leaf(component, types, token, allocator, context)
                },
            )
        })
    }
    pub(crate) fn retire_channel<P: a::Provider>(
        &mut self,
        types: &o::VerifiedTypes,
        value: &mut Option<crate::channel::managed::Retired<'l, 'ty>>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let meter = self.work.clone();
        meter.unmetered(|| {
            let component = &mut self.component;
            self.drain.as_mut().ok_or(3u32)?.channel(
                value,
                r::Services {
                    heap: self.heap.as_mut(),
                    channels: self
                        .channels
                        .as_mut()
                        .map(|channels| (channels, &mut self.scheduler)),
                },
                allocator,
                context,
                |token, allocator, _, context| {
                    cleanup_leaf(component, types, token, allocator, context)
                },
            )
        })
    }
    pub fn close<P: a::Provider>(&mut self, allocator: &mut a::Allocator<P>) -> Result<(), Reason> {
        let meter = self.work.clone();
        meter.unmetered(|| {
            if self.opened {
                self.heap
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .close(allocator)
                    .map_err(preparation)?;
                self.opened = false;
            }
            Ok(())
        })
    }
}
