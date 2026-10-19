//! C imports reuse generated ownership continuations and trusted typed storage
//! primitives. This wrapper alone enables the private C compilation mode; the
//! ordinary JIT/object APIs keep their complete C-unit refusal.
use super::*;
use lang_runtime::c_abi::{
    self as abi,
    context::{
        self as gate,
        scalar::{ContextProvider, Invoker, Scalar},
    },
};
use lang_runtime::{allocation as a, execution as e};

#[derive(Debug)]
pub enum CError {
    Codegen(Error),
    Metadata(abi::Error),
    Profile { span: m::Span },
}
impl std::fmt::Display for CError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native C program preparation failed: {self:?}")
    }
}
impl std::error::Error for CError {}

/// Verified preparation requirements, not a serialized format or a capability
/// constructible by source code. Runtime implementations must use this exact
/// allocator instance/control/work/cleanup configuration in their primitives.
#[derive(Clone)]
pub struct CEnvironment<'a> {
    context: gate::AllocatorIdentity,
    allocator: a::Identity,
    control: e::Control<'a>,
    work: e::ServiceWork,
    cleanup_records: usize,
}
impl<'a> CEnvironment<'a> {
    pub const fn allocator(&self) -> a::Identity {
        self.allocator
    }
    pub const fn control(&self) -> e::Control<'a> {
        self.control
    }
    pub fn work(&self) -> &e::ServiceWork {
        &self.work
    }
    pub const fn cleanup_records(&self) -> usize {
        self.cleanup_records
    }
}
pub struct CCompiledProgram<'a> {
    program: CompiledProgram<'a>,
    artifact: abi::Digest,
    functions: Vec<Option<usize>>,
}
impl<'a> CCompiledProgram<'a> {
    pub fn compile_scalar_imports(
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        artifact: &abi::Artifact,
        limits: m::Limits,
        cells: usize,
    ) -> Result<Self, CError> {
        let declarations = typed(flow).places().declarations();
        let checked = abi::Artifact::compile(
            declarations,
            artifact.target(),
            abi::Limits {
                budget: limits,
                ..Default::default()
            },
        )
        .map_err(CError::Metadata)?;
        if checked.digest() != artifact.digest() {
            return Err(CError::Profile {
                span: declarations
                    .functions()
                    .first()
                    .ok_or(CError::Codegen(Error::InvalidFunction(m::FunctionId(0))))?
                    .signature_span,
            });
        }
        for function in checked.functions() {
            if function.direction == abi::Direction::Import
                && function
                    .parameters
                    .iter()
                    .chain(std::iter::once(&function.result))
                    .any(|ty| {
                        !matches!(
                            checked.types()[ty.0].kind,
                            abi::Kind::I64 | abi::Kind::Bool | abi::Kind::Unit
                        )
                    })
            {
                return Err(CError::Profile {
                    span: function.span,
                });
            }
        }
        let mut budget = m::Budget::new(limits);
        let mut functions = budget
            .vector(declarations.imports().len())
            .map_err(|e| CError::Codegen(Error::Resource(e)))?;
        for import in declarations.imports() {
            functions.push(if import.abi == o::declarations::ImportAbi::C {
                Some(
                    checked
                        .functions()
                        .iter()
                        .position(|f| {
                            f.direction == abi::Direction::Import && f.name == import.name
                        })
                        .ok_or(CError::Profile { span: import.span })?,
                )
            } else {
                None
            });
        }
        let program =
            CompiledProgram::compile_scope(flow, layouts, limits, cells, CompileScope::CUnit)
                .map_err(CError::Codegen)?;
        Ok(Self {
            program,
            artifact: checked.digest(),
            functions,
        })
    }
    pub fn layout(&self, function: m::FunctionId) -> Option<&FunctionLayout<'a>> {
        self.program.layout(function)
    }
    pub fn metadata(&self, function: m::FunctionId) -> Option<&crate::program::FunctionCode> {
        self.program.metadata(function)
    }
    pub const fn usage(&self) -> m::Usage {
        self.program.usage
    }
    /// Perform this admission BEFORE reserving the runtime's guest-controlled
    /// frames/metadata. The returned requirements retain the invocation counter
    /// and cannot relax the context's allocation/execution bounds.
    pub fn prepare<'context, P: ContextProvider>(
        &self,
        session: &gate::Session<'context>,
        invoker: &dyn Invoker,
        allocator: &a::Allocator<P>,
    ) -> Result<CEnvironment<'context>, gate::Fault> {
        if !session.retains_code(self)
            || session.artifact().digest() != self.artifact
            || invoker.artifact() != self.artifact
            || invoker.allocator_identity() != session.allocator_identity()
            || session.bound_allocator() != Some(allocator.identity())
            || !allocator
                .provider()
                .is_some_and(|p| p.context_identity() == session.allocator_identity())
        {
            return Err(gate::Fault::new(gate::Kind::WrongProgram));
        }
        let actual = allocator.limits();
        let allowed = session.allocation_limits();
        if actual.request_bytes > allowed.request_bytes
            || actual.live_bytes > allowed.live_bytes
            || actual.cumulative_bytes > allowed.cumulative_bytes
            || actual.live_allocations > allowed.live_allocations
            || actual.alignment > allowed.alignment
        {
            return Err(gate::Fault::new(gate::Kind::Capacity));
        }
        Ok(CEnvironment {
            context: session.allocator_identity(),
            allocator: allocator.identity(),
            control: session.control(),
            work: session.work().clone(),
            cleanup_records: session.cleanup_records(),
        })
    }
    pub fn invoke<R: Runtime>(
        &self,
        function: m::FunctionId,
        session: &mut gate::Session<'_>,
        invoker: &mut dyn Invoker,
        instance: &mut RuntimeInstance<R>,
    ) -> Result<(), Failure> {
        let compiled = self.program.compiled(function).ok_or(Failure {
            function,
            reason: Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::Invariant)),
            span: self.program.functions[0].function.nodes[0].span,
            panic_code: None,
        })?;
        let reject = |fault| Failure {
            function,
            reason: Reason::Foreign(fault),
            span: compiled.function.nodes[0].span,
            panic_code: None,
        };
        let actual = instance
            .runtime
            .c_environment()
            .ok_or_else(|| reject(gate::Fault::new(gate::Kind::WrongProgram)))?;
        if !session.retains_code(self)
            || session.artifact().digest() != self.artifact
            || invoker.artifact() != self.artifact
            || invoker.allocator_identity() != session.allocator_identity()
            || actual.context != session.allocator_identity()
            || Some(actual.allocator) != session.bound_allocator()
            || !actual.control.same_configuration(session.control())
            || !actual.work.same_invocation(session.work())
            || actual.cleanup_records != session.cleanup_records()
            || instance.runtime.c_usage().is_none()
        {
            return Err(reject(gate::Fault::new(gate::Kind::WrongProgram)));
        }
        if typed(compiled.function.layout.source)
            .places()
            .declarations()
            .functions()[function.0 as usize]
            .abi
            != o::declarations::FunctionAbi::Guest
        {
            return Err(reject(gate::Fault::new(gate::Kind::Function(
                function.0 as usize,
            ))));
        }
        let mut imports = Dispatcher {
            session,
            invoker,
            functions: &self.functions,
        };
        let result = run(
            compiled.entry(),
            Some(ProgramRef::Jit(&self.program)),
            instance,
            Some(&mut imports),
        );
        if let Some(usage) = instance.runtime.c_usage() {
            *imports.session.usage() = usage;
        }
        result.map_err(|mut failure| {
            if let Some(fault) = imports.session.failure() {
                failure.reason = Reason::Foreign(fault);
                failure.span = fault.span.unwrap_or(failure.span);
            }
            failure
        })
    }
}

pub(super) trait ImportDispatcher<R> {
    fn invoke(&mut self, runtime: &mut R, site: &ImportSite) -> ResultStep;
}
struct Dispatcher<'a, 'context> {
    session: &'a mut gate::Session<'context>,
    invoker: &'a mut dyn Invoker,
    functions: &'a [Option<usize>],
}
impl<R: Runtime> ImportDispatcher<R> for Dispatcher<'_, '_> {
    fn invoke(&mut self, runtime: &mut R, site: &ImportSite) -> ResultStep {
        let invariant = c::Cause::Execution(c::ExecutionFailure::Invariant);
        let function = self
            .functions
            .get(site.import.0 as usize)
            .copied()
            .flatten()
            .ok_or(invariant)?;
        let metadata = &self.session.artifact().functions()[function];
        if site.arguments.len() > 256 || site.arguments.len() != metadata.parameters.len() {
            return Err(invariant);
        }
        let mut arguments = [Scalar::Unit; 256];
        for (index, (slot, ty)) in site.arguments.iter().zip(&metadata.parameters).enumerate() {
            let value = runtime.read_scalar(*slot)?;
            arguments[index] = match self.session.artifact().types()[ty.0].kind {
                abi::Kind::I64 => Scalar::I64(value),
                abi::Kind::Bool if (0..=1).contains(&value) => Scalar::Bool(value as u8),
                abi::Kind::Unit if value == 0 => Scalar::Unit,
                _ => return Err(invariant),
            };
        }
        *self.session.usage() = runtime.c_usage().ok_or(invariant)?;
        let result = self.session.invoke_scalars(
            function,
            Some(site.span),
            &arguments[..site.arguments.len()],
            self.invoker,
        );
        runtime.set_c_usage(*self.session.usage())?;
        let result = result.map_err(|fault| {
            c::Cause::Execution(match fault.kind {
                gate::Kind::Control(error)
                | gate::Kind::Value(abi::validation::Error::Control(error))
                | gate::Kind::ForeignValue(abi::validation::Error::Control(error)) => error.into(),
                gate::Kind::HostPanic => c::ExecutionFailure::HostPanic,
                _ => c::ExecutionFailure::HostCall,
            })
        })?;
        let bits = match result {
            Scalar::I64(value) => value,
            Scalar::Bool(value @ 0..=1) => i64::from(value),
            Scalar::Unit => 0,
            _ => return Err(invariant),
        };
        runtime.write_scalar(site.result, bits, site.span)
    }
}
