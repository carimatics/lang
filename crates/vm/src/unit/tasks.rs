//! RFC 0023 task adapter over independently verified ownership MIR.
//! Guest frames/finite values use managed storage; scheduler handles remain
//! invocation-local claims. Existing synchronous APIs keep their own profile.
#![allow(
    clippy::result_large_err,
    reason = "Keep task failures and cleanup evidence inline without allocating during failure handling."
)]
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, failure as f},
};
use lang_runtime::{allocation as a, execution as e, layout as l, task as t};

mod instructions;
mod machine;
use frame::Pending;

use lang_runtime::task::runner::frame;
pub use lang_runtime::task::runner::{Event, EventKind, Failure, Limits, Outcome, Reason};

pub fn execute_entry<P: a::Provider>(
    proof: &o::tasks::VerifiedTasks,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    limits: Limits,
) -> Result<Outcome, Failure> {
    lang_runtime::task::runner::execute_entry(
        &mut machine::Interpreter,
        proof,
        allocator,
        control,
        limits,
    )
}
pub fn execute_function<P: a::Provider>(
    proof: &o::tasks::VerifiedTasks,
    function: m::FunctionId,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    limits: Limits,
) -> Result<Outcome, Failure> {
    lang_runtime::task::runner::execute_function(
        &mut machine::Interpreter,
        proof,
        function,
        allocator,
        control,
        limits,
    )
}
