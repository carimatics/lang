//! Scoped RFC 0020/0031 consumption of original native code and bounded records.
//! No source analysis, backend emission, executable allocation or interpretation.
#![allow(
    clippy::result_large_err,
    clippy::large_enum_variant,
    reason = "Keep the original runtime/cleanup report and source input without allocating after guest/provider failure."
)]
use crate::{
    native_record::{self, Kind, Record},
    owned, program,
};
use lang_mir::unit as m;
use lang_runtime::{allocation as a, cleanup as c, execution as e, layout::Layouts};
use std::{ffi::c_void, fmt, ptr::NonNull};

#[derive(Clone, Copy)]
pub struct Input<'a> {
    pub target: &'a str,
    pub build_id: &'a str,
    pub semantic: &'a [u8],
    pub native: &'a [u8],
}
impl fmt::Debug for Input<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Input")
            .field("target", &format_args!("{:.128}", self.target))
            .field("build_id", &format_args!("{:.128}", self.build_id))
            .field("semantic_bytes", &self.semantic.len())
            .field("native_bytes", &self.native.len())
            .finish()
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub copy: lang_bytecode::unit::CodecLimits,
    pub owned: lang_bytecode::ownership::CodecLimits,
    pub tasks: lang_bytecode::tasks::CodecLimits,
    pub record: native_record::Limits,
    pub plans: m::Limits,
    pub cells: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            copy: Default::default(),
            owned: Default::default(),
            tasks: Default::default(),
            record: Default::default(),
            plans: Default::default(),
            cells: 4096,
        }
    }
}
#[derive(Debug)]
pub enum ErrorKind {
    Target {
        host: crate::Triple,
    },
    RecordTarget {
        expected: crate::Triple,
        recorded: crate::Triple,
    },
    BuildId {
        expected: String,
    },
    Profile,
    Entry,
    UnsupportedCopyControls,
    CopyCodec(lang_bytecode::unit::CodecError),
    OwnedCodec(lang_bytecode::ownership::CodecError),
    TaskCodec(lang_bytecode::ownership::CodecError),
    TaskPlan(crate::tasks::Error),
    TaskBinding(crate::tasks::Error),
    TaskRun(lang_runtime::task::runner::Failure),
    Record(native_record::Error),
    CopyBinding(program::BindError),
    CopyInvoke(program::InvokeError),
    Layout(lang_runtime::layout::Error),
    OwnedPlan(owned::RecordedError),
    OwnedBinding(owned::Error),
    OwnedEntry(owned::runtime::EntryError),
    CMetadata(lang_runtime::c_abi::Error),
    CPlan(owned::CObjectError),
    CContext(lang_runtime::c_abi::context::Fault),
    CBinding(crate::c_abi::object::Error),
}
/// Keeps the exact semantic input for source mapping after the local scope ends;
/// runtime errors retain their original usage, report, stack and poison cause.
#[derive(Debug)]
pub struct Failure<'a> {
    pub input: Input<'a>,
    pub kind: ErrorKind,
}
impl fmt::Display for Failure<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let ErrorKind::BuildId { expected } = &self.kind {
            write!(
                f,
                "SDK build mismatch: expected {expected}, got {:.128}",
                self.input.build_id
            )
        } else {
            write!(f, "native SDK: {:?}", self.kind)
        }
    }
}
impl std::error::Error for Failure<'_> {}
impl<'a> Input<'a> {
    fn error(self, kind: ErrorKind) -> Failure<'a> {
        Failure { input: self, kind }
    }
    fn check(self) -> Result<crate::Triple, Failure<'a>> {
        let host = crate::Triple::host();
        if !matches!(
            self.target,
            "aarch64-apple-darwin" | "x86_64-pc-windows-msvc"
        ) || self.target != host.to_string()
        {
            return Err(self.error(ErrorKind::Target { host }));
        }
        let expected = super::build_id(&host);
        if self.build_id != expected {
            return Err(self.error(ErrorKind::BuildId { expected }));
        }
        Ok(host)
    }
    fn record(self, target: &crate::Triple, limits: Limits) -> Result<Record, Failure<'a>> {
        let record = Record::decode(self.native, self.semantic, limits.record)
            .map_err(|e| self.error(ErrorKind::Record(e)))?;
        if record.target() != target {
            return Err(self.error(ErrorKind::RecordTarget {
                expected: target.clone(),
                recorded: record.target().clone(),
            }));
        }
        Ok(record)
    }
}

/// Execute the original checked Copy entry with its synchronous unmetered ABI.
/// Any configured control, including an unfired cancellation flag, is refused.
/// # Safety
/// The borrowed image/resolver must satisfy PreparedObject::bind for this exact
/// original object and SDK, retaining every symbol/dependency for the whole call.
pub unsafe fn execute_copy<'a, Image>(
    input: Input<'a>,
    image: &Image,
    mut resolve: impl FnMut(&Image, &str) -> Option<NonNull<c_void>>,
    control: e::Control<'_>,
    limits: Limits,
) -> Result<i64, Failure<'a>> {
    let target = input.check()?;
    if !control.same_configuration(Default::default()) {
        return Err(input.error(ErrorKind::UnsupportedCopyControls));
    }
    let record = input.record(&target, limits)?;
    if record.kind() != Kind::Copy {
        return Err(input.error(ErrorKind::Profile));
    }
    let p = lang_bytecode::unit::decode(input.semantic, limits.copy)
        .map_err(|e| input.error(ErrorKind::CopyCodec(e)))?;
    let unit = p.verified().unit();
    let entry = unit.entry.ok_or_else(|| input.error(ErrorKind::Entry))?;
    let signature = &unit.functions[entry.0 as usize].signature;
    if !signature.parameters.is_empty() || signature.result != m::Type::I64 {
        return Err(input.error(ErrorKind::Entry));
    }
    let object = record
        .prepare_copy(p.verified(), &[], limits.plans)
        .map_err(|e| input.error(ErrorKind::CopyBinding(e)))?;
    // SAFETY: delegated matching-image contract; the borrowed owner lives through invocation.
    let code = unsafe { object.bind(image, |owner, name| resolve(*owner, name), limits.plans) }
        .map_err(|e| input.error(ErrorKind::CopyBinding(e)))?;
    code.invoke(entry, &[])
        .map_err(|e| input.error(ErrorKind::CopyInvoke(e)))?
        .cells
        .first()
        .copied()
        .ok_or_else(|| input.error(ErrorKind::Entry))
}

/// Execute the recorded ordinary ownership entry with the actual caller state.
/// # Safety
/// The borrowed image/resolver must satisfy ObjectArtifact::bind for the exact
/// original object/SDK and retain its code and relocation dependencies throughout.
pub unsafe fn execute_owned<'a, Image, P: a::Provider>(
    input: Input<'a>,
    image: &Image,
    mut resolve: impl FnMut(&Image, &str) -> Option<NonNull<c_void>>,
    instance: &mut c::Instance,
    allocator: &mut a::Allocator<P>,
    config: owned::runtime::Config<'_>,
    limits: Limits,
) -> Result<owned::runtime::Outcome, Failure<'a>> {
    let target = input.check()?;
    let record = input.record(&target, limits)?;
    if record.kind() != Kind::OwnershipUnit {
        return Err(input.error(ErrorKind::Profile));
    }
    let p = lang_bytecode::ownership::decode(input.semantic, limits.owned)
        .map_err(|e| input.error(ErrorKind::OwnedCodec(e)))?;
    let entry = p.entry().ok_or_else(|| input.error(ErrorKind::Entry))?;
    let flow = p.flow();
    let decl = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    if decl
        .imports()
        .iter()
        .any(|i| i.abi == m::ownership::declarations::ImportAbi::C)
        || decl
            .functions()
            .iter()
            .any(|f| f.abi == m::ownership::declarations::FunctionAbi::CExport)
    {
        return Err(input.error(ErrorKind::Profile));
    }
    let signature = &decl.functions()[entry.0 as usize];
    if !signature.parameters.is_empty() || signature.result.ty != m::ownership::Type::I64 {
        return Err(input.error(ErrorKind::Entry));
    }
    let layouts = Layouts::compile(decl.types(), limits.plans)
        .map_err(|e| input.error(ErrorKind::Layout(e)))?;
    let object = owned::RecordedObject::from_record(record)
        .map_err(|e| input.error(ErrorKind::Record(e)))?
        .prepare(flow, &layouts, limits.plans, limits.cells)
        .map_err(|e| input.error(ErrorKind::OwnedPlan(e)))?;
    // SAFETY: delegated exact-image contract; no address is fabricated from data.
    let code = unsafe { object.bind(image, |owner, name| resolve(*owner, name), limits.plans) }
        .map_err(|e| input.error(ErrorKind::OwnedBinding(e)))?;
    owned::runtime::execute_entry(&code, entry, instance, allocator, config)
        .map_err(|e| input.error(ErrorKind::OwnedEntry(e)))
}

/// Execute an original task entry with the caller's allocator and controls.
/// # Safety
/// Image/resolver must retain the exact original task object and its matching
/// compiler/SDK ABI for this host through the complete call and cleanup.
pub unsafe fn execute_tasks<'a, Image, P: a::Provider>(
    input: Input<'a>,
    image: &Image,
    mut resolve: impl FnMut(&Image, &str) -> Option<NonNull<c_void>>,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    execution: lang_runtime::task::runner::Limits,
    limits: Limits,
) -> Result<lang_runtime::task::runner::Outcome, Failure<'a>> {
    let target = input.check()?;
    let record = input.record(&target, limits)?;
    let Kind::Task(recorded_entry) = record.kind() else {
        return Err(input.error(ErrorKind::Profile));
    };
    let program = lang_bytecode::tasks::decode(input.semantic, limits.tasks)
        .map_err(|e| input.error(ErrorKind::TaskCodec(e)))?;
    if program.entry() != Some(recorded_entry) {
        return Err(input.error(ErrorKind::Entry));
    }
    let object = crate::tasks::RecordedObject::from_record(record)
        .map_err(|e| input.error(ErrorKind::Record(e)))?
        .prepare(program.proof(), limits.plans)
        .map_err(|e| input.error(ErrorKind::TaskPlan(e)))?;
    // SAFETY: the caller supplies this exact original statically linked image.
    let code = unsafe {
        object.bind(
            image,
            |owner, name| resolve(*owner, name).map(|p| p.as_ptr().cast_const().cast()),
            limits.plans,
        )
    }
    .map_err(|e| input.error(ErrorKind::TaskBinding(e)))?;
    code.execute(allocator, control, execution)
        .map_err(|e| input.error(ErrorKind::TaskRun(e)))
}

/// Bind original C exports only while the explicit host closure runs.
/// The closure supplies real C configuration/providers; metadata grants none.
/// # Safety
/// Image/resolver must satisfy CObjectArtifact::bind and ObjectExports::new.
/// Run synchronous calls on the owner thread. Every C call must return and
/// every context must be destroyed before the closure exits, including unwind.
/// No context, raw client, or registration may escape or be used after return.
pub unsafe fn with_c<'a, Image, R>(
    input: Input<'a>,
    image: &Image,
    mut resolve: impl FnMut(&Image, &str) -> Option<NonNull<c_void>>,
    capacities: lang_runtime::c_abi::context::config::Capacities,
    limits: Limits,
    body: impl for<'scope> FnOnce(&'scope lang_runtime::c_abi::Artifact) -> R,
) -> Result<R, Failure<'a>> {
    use lang_runtime::c_abi::{self as abi, context as gate};
    let target = input.check()?;
    let record = input.record(&target, limits)?;
    if !matches!(record.kind(), Kind::CObject(_)) {
        return Err(input.error(ErrorKind::Profile));
    }
    let p = lang_bytecode::ownership::decode(input.semantic, limits.owned)
        .map_err(|e| input.error(ErrorKind::OwnedCodec(e)))?;
    if p.entry().is_some() {
        return Err(input.error(ErrorKind::Entry));
    }
    let flow = p.flow();
    let decl = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    let c = decl
        .imports()
        .iter()
        .any(|i| i.abi == m::ownership::declarations::ImportAbi::C)
        || decl
            .functions()
            .iter()
            .any(|f| f.abi == m::ownership::declarations::FunctionAbi::CExport);
    if !c {
        return Err(input.error(ErrorKind::Profile));
    }
    let layouts = Layouts::compile(decl.types(), limits.plans)
        .map_err(|e| input.error(ErrorKind::Layout(e)))?;
    let abi_target = if target
        == "aarch64-apple-darwin"
            .parse::<crate::Triple>()
            .expect("fixed target")
    {
        abi::Target::MacosAarch64
    } else {
        abi::Target::WindowsX86_64
    };
    let artifact = abi::Artifact::compile(decl, abi_target, Default::default())
        .map_err(|e| input.error(ErrorKind::CMetadata(e)))?;
    let object = owned::RecordedObject::from_record(record)
        .map_err(|e| input.error(ErrorKind::Record(e)))?
        .prepare_c(flow, &layouts, &artifact, limits.plans, limits.cells)
        .map_err(|e| input.error(ErrorKind::CPlan(e)))?;
    // SAFETY: exact matching image is retained by the caller's borrow.
    let code = unsafe { object.bind(image, |owner, name| resolve(*owner, name), limits.plans) }
        .map_err(|e| input.error(ErrorKind::CPlan(e)))?;
    let program = gate::Program::new(&artifact, &code, capacities)
        .map_err(|e| input.error(ErrorKind::CContext(e)))?;
    let factory = crate::c_abi::runtime::NativeFactory::new(&code, &artifact, limits.plans)
        .map_err(|e| input.error(ErrorKind::CContext(e)))?;
    // SAFETY: caller promises all raw C activity/contexts end before this scope,
    // on the owner thread; local code/program/factory outlive the registration.
    let exports = unsafe { crate::c_abi::object::ObjectExports::new(&program, &code, &factory) }
        .map_err(|e| input.error(ErrorKind::CBinding(e)))?;
    let result = body(&artifact);
    drop(exports);
    Ok(result)
}
