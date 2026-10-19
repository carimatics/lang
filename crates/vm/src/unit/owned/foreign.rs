//! Explicit C entry into the existing ownership interpreter. Ordinary entry
//! keeps its complete C-unit refusal; this path requires a live context Entry,
//! the same actual allocator and explicitly prepared C invoker before effects.
use super::*;
use lang_runtime::c_abi::{
    self as abi,
    context::{
        self as gate,
        scalar::{ContextProvider, Invoker, Scalar},
    },
};

impl Failure {
    /// Propagate the execution/cleanup report to Context::run. Its Session
    /// already retains a foreign primary; the report contributes later cleanup
    /// records without reclassifying a foreign status as a source guest panic.
    pub fn context_fault(&self) -> gate::Fault {
        if let Some(fault) = self
            .report
            .as_ref()
            .and_then(|report| gate::Fault::from_report(report, self.panic_code.unwrap_or(0)))
        {
            return fault;
        }
        let mut fault = match &self.reason {
            Reason::Foreign(fault) => *fault,
            Reason::Poisoned => gate::Fault::new(gate::Kind::Poisoned),
            Reason::Control(error) => {
                gate::Fault::new(gate::Kind::Execution(execution_reason(error.clone())))
            }
            Reason::Preparation(l::Error::Allocation(_))
            | Reason::MissingAllocator
            | Reason::Enter(_) => {
                gate::Fault::new(gate::Kind::Execution(c::ExecutionFailure::AllocationLimit))
            }
            Reason::Entry | Reason::UnsupportedCInterface => {
                gate::Fault::new(gate::Kind::WrongProgram)
            }
            _ => gate::Fault::new(gate::Kind::Execution(c::ExecutionFailure::Fault)),
        };
        fault.span = self.fault.or(fault.span);
        fault
    }
}

pub(super) struct Imports<'a, 'context> {
    pub session: &'a mut gate::Session<'context>,
    invoker: &'a mut dyn Invoker,
    functions: Vec<Option<usize>>,
}
impl<'a, 'context> Imports<'a, 'context> {
    #[allow(
        clippy::result_large_err,
        reason = "Retain inline structured failure after allocation failure."
    )]
    fn new<P: ContextProvider>(
        flow: &f::Elaborated,
        allocator: &a::Allocator<P>,
        session: &'a mut gate::Session<'context>,
        invoker: &'a mut dyn Invoker,
    ) -> Result<Self, Failure> {
        let reject = |fault| rejected(Reason::Foreign(fault), None, None);
        if !session.retains_code(flow)
            || session.bound_allocator() != Some(allocator.identity())
            || !allocator
                .provider()
                .is_some_and(|p| p.context_identity() == session.allocator_identity())
            || invoker.allocator_identity() != session.allocator_identity()
            || invoker.artifact() != session.artifact().digest()
        {
            return Err(reject(gate::Fault::new(gate::Kind::WrongProgram)));
        }
        let actual = allocator.limits();
        let allowed = session.allocation_limits();
        if actual.request_bytes > allowed.request_bytes
            || actual.live_bytes > allowed.live_bytes
            || actual.cumulative_bytes > allowed.cumulative_bytes
            || actual.live_allocations > allowed.live_allocations
            || actual.alignment > allowed.alignment
        {
            return Err(reject(gate::Fault::new(gate::Kind::Capacity)));
        }
        let declarations = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations();
        let artifact = abi::Artifact::compile(
            declarations,
            session.artifact().target(),
            abi::Limits::default(),
        )
        .map_err(|_| reject(gate::Fault::new(gate::Kind::Capacity)))?;
        if artifact.digest() != session.artifact().digest() {
            return Err(reject(gate::Fault::new(gate::Kind::Config(
                gate::config::InvalidConfig::Artifact,
            ))));
        }
        for (index, function) in artifact.functions().iter().enumerate() {
            if function.direction == abi::Direction::Import
                && function
                    .parameters
                    .iter()
                    .chain(std::iter::once(&function.result))
                    .any(|ty| {
                        !matches!(
                            artifact.types()[ty.0].kind,
                            abi::Kind::I64 | abi::Kind::Bool | abi::Kind::Unit
                        )
                    })
            {
                return Err(reject(
                    gate::Fault::new(gate::Kind::Function(index)).at(function.span),
                ));
            }
        }
        let mut budget = m::Budget::new(m::Limits::default());
        let mut functions = budget
            .vector(declarations.imports().len())
            .map_err(|_| reject(gate::Fault::new(gate::Kind::Allocation)))?;
        for import in declarations.imports() {
            functions.push(if import.abi == o::declarations::ImportAbi::C {
                Some(
                    artifact
                        .functions()
                        .iter()
                        .position(|f| {
                            f.direction == abi::Direction::Import && f.name == import.name
                        })
                        .ok_or_else(|| reject(gate::Fault::new(gate::Kind::WrongProgram)))?,
                )
            } else {
                None
            });
        }
        Ok(Self {
            session,
            invoker,
            functions,
        })
    }
    pub fn supports(&self, import: m::ImportId) -> bool {
        self.functions
            .get(import.0 as usize)
            .is_some_and(Option::is_some)
    }
    pub fn fault(&self) -> Option<gate::Fault> {
        self.session.failure()
    }
    pub fn invoke<P: a::Provider>(
        &mut self,
        import: m::ImportId,
        instruction: &b::Instruction,
        values: usize,
        shared: &Rc<RefCell<State<'_, '_, '_, P>>>,
        usage: &mut Usage,
    ) -> Step<()> {
        let b::Op::Import { arguments, .. } = &instruction.op else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let function = self
            .functions
            .get(import.0 as usize)
            .copied()
            .flatten()
            .ok_or(c::ExecutionFailure::Invariant)?;
        if arguments.len() > 256 || instruction.results.len() != 1 {
            return Err(c::ExecutionFailure::Invariant);
        }
        let mut scalars = [Scalar::Unit; 256];
        {
            let state = shared.borrow();
            for (to, from) in scalars.iter_mut().zip(arguments) {
                *to = match state.read(state.root(values + from.0 as usize)?)? {
                    l::Immediate::I64(value) => Scalar::I64(value),
                    l::Immediate::Bool(value) => Scalar::Bool(u8::from(value)),
                    l::Immediate::Unit => Scalar::Unit,
                    _ => return Err(c::ExecutionFailure::Invariant),
                };
            }
        } // Release all state/topology borrows before calling native code.
        *self.session.usage() = *usage;
        let result = self.session.invoke_scalars(
            function,
            Some(instruction.span),
            &scalars[..arguments.len()],
            self.invoker,
        );
        *usage = *self.session.usage();
        let result = result.map_err(|fault| match fault.kind {
            gate::Kind::Control(error)
            | gate::Kind::Value(abi::validation::Error::Control(error))
            | gate::Kind::ForeignValue(abi::validation::Error::Control(error)) => error.into(),
            gate::Kind::HostPanic => c::ExecutionFailure::HostPanic,
            _ => c::ExecutionFailure::HostCall,
        })?;
        let result = match result {
            Scalar::I64(value) => l::Immediate::I64(value),
            Scalar::Bool(0) => l::Immediate::Bool(false),
            Scalar::Bool(1) => l::Immediate::Bool(true),
            Scalar::Unit => l::Immediate::Unit,
            _ => return Err(c::ExecutionFailure::Invariant),
        };
        shared
            .borrow_mut()
            .write(values + instruction.results[0].0 as usize, result)
    }
}

/// Execute source main using only the context's checked scalar/Unit imports.
/// The context Program must retain this exact failure-flow owner, and its
/// actual allocator cannot be replaced or relaxed. No provider/guest effect
/// occurs before complete C admission. Result publication still follows cleanup.
#[allow(
    clippy::result_large_err,
    reason = "Keep the completed cleanup report on failure."
)]
pub fn execute_entry_c<P: ContextProvider>(
    flow: &f::Elaborated,
    entry: &mut gate::Entry<'_, '_>,
    allocator: &mut a::Allocator<P>,
    invoker: &mut dyn Invoker,
) -> Result<Outcome, Failure> {
    let declarations = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    let function = declarations
        .functions()
        .iter()
        .position(|f| f.name == "main" && f.abi == o::declarations::FunctionAbi::Guest)
        .map(|i| m::FunctionId(i as u32))
        .ok_or_else(|| rejected(Reason::Entry, None, None))?;
    let (instance, session) = entry.parts();
    let control = session.control();
    let mut imports = Imports::new(flow, allocator, session, invoker)?;
    let result = calls::execute_c(flow, function, instance, allocator, control, &mut imports);
    match &result {
        Ok(outcome) => *imports.session.usage() = outcome.usage,
        Err(failure) if failure.report.is_some() => *imports.session.usage() = failure.usage,
        _ => {}
    }
    result
}
