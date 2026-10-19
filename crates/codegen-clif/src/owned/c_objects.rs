//! Matched native object metadata for RFC 0015. This is a trusted compiler /
//! linker boundary, not a serialized package or an untrusted native loader.
use super::c_imports::{self, CExecutable, CExecution, CInterface, NativeProgram};
use super::*;
use crate::c_abi::{object as bridge, scalar::LinkedThunk};
use lang_runtime::{
    allocation as a,
    c_abi::{
        self as abi,
        context::{
            self as gate,
            scalar::{ContextProvider, Invoker},
        },
    },
};
use std::ptr::NonNull;

#[derive(Debug)]
pub enum CObjectError {
    Preparation(CError),
    Codegen(Error),
    Bridge(bridge::Error),
    Recorded(RecordedError),
    RecordScope,
    ArtifactDigest {
        expected: abi::Digest,
        recorded: abi::Digest,
    },
    Target,
    MissingSymbol(String),
    SlotAlignment,
}
impl std::fmt::Display for CObjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "C object preparation failed: {self:?}")
    }
}
impl std::error::Error for CObjectError {}

pub struct CObjectArtifact<'a> {
    object: ObjectArtifact<'a>,
    interface: CInterface,
    symbols: bridge::Symbols,
}
fn c_object_target(artifact: &abi::Artifact) -> crate::Triple {
    match artifact.target() {
        abi::Target::MacosAarch64 => "aarch64-apple-darwin",
        abi::Target::WindowsX86_64 => "x86_64-pc-windows-msvc",
    }
    .parse::<crate::Triple>()
    .expect("known tier-1 target")
}
pub fn emit_c_object_for_target<'a>(
    flow: &'a f::Elaborated,
    layouts: &'a Layouts<'a>,
    artifact: &abi::Artifact,
    target: &crate::Triple,
    limits: m::Limits,
    cells: usize,
) -> Result<CObjectArtifact<'a>, CObjectError> {
    let expected = c_object_target(artifact);
    if *target != expected {
        return Err(CObjectError::Target);
    }
    let interface = CInterface::check(flow, artifact, limits).map_err(CObjectError::Preparation)?;
    let mut prepared = prepare_object(
        flow,
        layouts,
        target,
        limits,
        cells,
        CompileScope::CObject(artifact.digest()),
    )
    .map_err(CObjectError::Codegen)?;
    let mut budget = m::Budget::with_usage(limits, prepared.usage)
        .map_err(|e| CObjectError::Codegen(Error::Resource(e)))?;
    let symbols =
        bridge::emit(&mut prepared.module, artifact, &mut budget).map_err(CObjectError::Bridge)?;
    prepared.usage = m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    };
    let object = prepared.finish().map_err(CObjectError::Codegen)?;
    Ok(CObjectArtifact {
        object,
        interface,
        symbols,
    })
}
impl<'a> CObjectArtifact<'a> {
    /// Detach the RFC 0018 C-object record; no live bridge/context is retained.
    pub fn into_recorded(self, limits: m::Limits) -> Result<RecordedObject, Error> {
        self.object.into_recorded(limits)
    }
    pub fn bytes(&self) -> &[u8] {
        &self.object.bytes
    }
    pub fn slot_symbol(&self) -> &str {
        &self.symbols.slot
    }
    pub fn support_symbols(&self) -> &[String; 3] {
        &self.symbols.support
    }
    pub fn export_symbols(&self) -> &[(usize, String)] {
        &self.symbols.exports
    }
    pub fn thunk_symbols(&self) -> &[(usize, String)] {
        &self.symbols.thunks
    }
    pub fn function(&self, id: m::FunctionId) -> Option<&ObjectFunction<'a>> {
        self.object.function(id)
    }
    pub const fn usage(&self) -> m::Usage {
        self.object.usage()
    }
    /// # Safety
    /// Every resolved address must be the matching symbol from this exact
    /// artifact/toolchain/target. The image owns all code, data and relocation
    /// dependencies through its lifetime. The slot is an aligned writable
    /// pointer object initialized by the linker. No fabricated addresses,
    /// concurrent mutation, unload or untrusted native metadata are permitted.
    pub unsafe fn bind<Image>(
        self,
        image: Image,
        mut resolve: impl FnMut(&Image, &str) -> Option<NonNull<c_void>>,
        limits: m::Limits,
    ) -> Result<CLinkedProgram<'a, Image>, CObjectError> {
        if self.object.target != crate::Triple::host() {
            return Err(CObjectError::Target);
        }
        let mut budget = m::Budget::with_usage(limits, self.object.usage)
            .map_err(|e| CObjectError::Codegen(Error::Resource(e)))?;
        budget
            .charge(1 + self.symbols.exports.len() + self.symbols.thunks.len())
            .map_err(|e| CObjectError::Codegen(Error::Resource(e)))?;
        let mut symbol = |name: &str| {
            resolve(&image, name).ok_or_else(|| CObjectError::MissingSymbol(name.to_owned()))
        };
        let slot = symbol(&self.symbols.slot)?.cast::<*const bridge::Header>();
        if slot.as_ptr().addr() % std::mem::align_of::<*const bridge::Header>() != 0 {
            return Err(CObjectError::SlotAlignment);
        }
        let mut support = [std::ptr::null(); 3];
        for (i, name) in self.symbols.support.iter().enumerate() {
            budget
                .charge(1)
                .map_err(|e| CObjectError::Codegen(Error::Resource(e)))?;
            support[i] = symbol(name)?.as_ptr().cast_const().cast();
        }
        let mut exports = budget
            .vector(self.symbols.exports.len())
            .map_err(|e| CObjectError::Codegen(Error::Resource(e)))?;
        for (i, name) in self.symbols.exports {
            let code = symbol(&name)?.as_ptr().cast_const().cast();
            exports.push((i, name, code));
        }
        let mut thunks = budget
            .vector(self.symbols.thunks.len())
            .map_err(|e| CObjectError::Codegen(Error::Resource(e)))?;
        for (function, name) in self.symbols.thunks {
            thunks.push(LinkedThunk {
                function,
                code: symbol(&name)?.as_ptr().cast_const().cast(),
            });
        }
        let usage = m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        };
        // SAFETY: this method's matched-image and lifetime contract includes all
        // private continuation entries, in addition to the public C wrappers.
        let mut object = self.object;
        object.usage = usage;
        let program =
            unsafe { object.bind(image, resolve, limits) }.map_err(CObjectError::Codegen)?;
        let usage = program.usage();
        Ok(CLinkedProgram {
            program,
            interface: self.interface,
            slot,
            support,
            exports,
            thunks,
            usage,
        })
    }
}
impl RecordedObject {
    /// Restore checked C interface/bridge metadata from decoded semantics.
    /// No source analysis, ISA lookup, code emission, JIT or symbol resolution
    /// occurs here. The result's unsafe binder still requires the exact
    /// original object, matching toolchain/target and retained trusted image.
    pub fn prepare_c<'a>(
        self,
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        artifact: &abi::Artifact,
        limits: m::Limits,
        cells: usize,
    ) -> Result<CObjectArtifact<'a>, CObjectError> {
        let CompileScope::CObject(recorded) = self.compilation_scope() else {
            return Err(CObjectError::RecordScope);
        };
        if recorded != artifact.digest() {
            return Err(CObjectError::ArtifactDigest {
                expected: artifact.digest(),
                recorded,
            });
        }
        if *self.compilation_target() != c_object_target(artifact) {
            return Err(CObjectError::Target);
        }
        let interface =
            CInterface::check(flow, artifact, limits).map_err(CObjectError::Preparation)?;
        let mut object = self
            .prepare(flow, layouts, limits, cells)
            .map_err(CObjectError::Recorded)?;
        let mut budget = m::Budget::with_usage(limits, object.usage)
            .map_err(|e| CObjectError::Codegen(Error::Resource(e)))?;
        let symbols =
            bridge::Symbols::prepare(artifact, &mut budget).map_err(CObjectError::Bridge)?;
        object.usage = m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        };
        Ok(CObjectArtifact {
            object,
            interface,
            symbols,
        })
    }
}
pub struct CLinkedProgram<'a, Image> {
    program: LinkedProgram<'a, Image>,
    interface: CInterface,
    pub(crate) slot: NonNull<*const bridge::Header>,
    pub(crate) support: [*const u8; 3],
    exports: Vec<(usize, String, *const u8)>,
    thunks: Vec<LinkedThunk>,
    usage: m::Usage,
}
impl<Image> CLinkedProgram<'_, Image> {
    pub fn symbol(&self, function: usize) -> Option<(&str, *const u8)> {
        self.exports
            .iter()
            .find(|e| e.0 == function)
            .map(|e| (e.1.as_str(), e.2))
    }
}
impl<Image> c_imports::sealed::Sealed for CLinkedProgram<'_, Image> {}
impl<'a, Image> NativeProgram<'a> for CLinkedProgram<'a, Image> {
    fn artifact_digest(&self) -> abi::Digest {
        self.interface.artifact
    }
    fn exported_function(&self, index: usize) -> Option<m::FunctionId> {
        self.interface.exports.get(index).copied().flatten()
    }
    fn layout(&self, function: m::FunctionId) -> Option<&FunctionLayout<'a>> {
        self.program.layout(function)
    }
    fn usage(&self) -> m::Usage {
        self.usage
    }
}
impl<'a, Image> CExecutable<'a> for CLinkedProgram<'a, Image> {
    fn prepare_c<'context, P: ContextProvider>(
        &self,
        session: &gate::Session<'context>,
        invoker: &dyn Invoker,
        allocator: &a::Allocator<P>,
    ) -> Result<CEnvironment<'context>, gate::Fault> {
        self.interface.prepare(self, session, invoker, allocator)
    }
    fn execute_export<R: Runtime>(
        &self,
        function: m::FunctionId,
        session: &mut gate::Session<'_>,
        invoker: &mut dyn Invoker,
        instance: &mut RuntimeInstance<R>,
    ) -> Result<(), Failure> {
        let entry = self.program.entry(function).ok_or(Failure {
            function,
            reason: Reason::Runtime(c::Cause::Execution(c::ExecutionFailure::Invariant)),
            span: self.program.artifact.functions[0].nodes[0].span,
            panic_code: None,
        })?;
        self.interface.invoke(
            CExecution {
                owner: self,
                entry,
                program: ProgramRef::Linked(&self.program),
            },
            session,
            invoker,
            instance,
            o::declarations::FunctionAbi::CExport,
        )
    }
    fn linked_thunks(&self) -> Option<&[LinkedThunk]> {
        Some(&self.thunks)
    }
}
