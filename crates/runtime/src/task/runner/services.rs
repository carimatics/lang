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
pub struct Services<'l, 'ty> {
    pub layouts: &'l l::Layouts<'ty>,
    pub drain: Option<r::Drain<'l, 'ty>>,
    pub heap: Option<sh::Heap<'l, 'ty>>,
    pub opened: bool,
}
impl<'l, 'ty> Services<'l, 'ty> {
    pub fn prepare<P: a::Provider>(
        proof: &o::tasks::VerifiedTasks,
        plans: &[Option<l::FramePlan<'l, 'ty>>],
        layouts: &'l l::Layouts<'ty>,
        allocator: &a::Allocator<P>,
        budget: &mut m::Budget,
    ) -> Result<Self, Reason> {
        let mut has_indirect = false;
        let mut has_shared = false;
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
                if let b::Op::CoreCall(call) = &instruction.op {
                    has_indirect |= indirect(call.operation);
                    has_shared |= shared(call.operation);
                }
            }
        }
        let heap = if has_shared {
            let heap =
                sh::Heap::prepare(layouts, Default::default(), budget.limits(), usage(budget))
                    .map_err(preparation)?;
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
        Ok(Self {
            layouts,
            heap,
            drain,
            opened: false,
        })
    }
    pub fn retire<P: a::Provider>(
        &mut self,
        types: &o::VerifiedTypes,
        token: &mut l::Token,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        if let Some(drain) = &mut self.drain {
            drain.token(
                token,
                self.heap.as_mut(),
                allocator,
                context,
                |token, allocator, _, _| {
                    crate::buffer::cleanup_token(types, token, allocator).map_err(|_| 3u32)
                },
            )
        } else {
            crate::buffer::cleanup_token(types, token, allocator).map_err(|_| 3u32)
        }
    }
    pub fn close<P: a::Provider>(&mut self, allocator: &mut a::Allocator<P>) -> Result<(), Reason> {
        if self.opened {
            self.heap
                .as_mut()
                .ok_or(Reason::Invariant)?
                .close(allocator)
                .map_err(preparation)?;
            self.opened = false;
        }
        Ok(())
    }
}
