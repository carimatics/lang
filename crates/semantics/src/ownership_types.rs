//! Checked source ownership HIR to the MIR's independently verified type graph.
//! Type/place adapters and automatic body lowering remain independent from
//! executable layouts, runtime authority, and serialization.

use lang_diagnostics::{SourceError, SourceId, SourceMap, Span};
use lang_hir::{regions::RegionCheckedBodies, resolved as r};
use lang_mir::unit::{self as m, ownership as o};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

pub mod bodies;
pub mod places;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Profile {
    Ownership,
    Tasks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedBody {
    Match,
    ProvisionalResult,
    SourceIntrinsic,
    Task,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Source(SourceError),
    Mir(m::Error),
    Types(o::Error),
    Tasks(o::tasks::Error),
    MissingSource,
    MissingLocal {
        function: usize,
        local: usize,
    },
    MissingExpression {
        function: usize,
        expression: usize,
    },
    MissingField {
        function: usize,
        expression: usize,
    },
    MissingPlace {
        function: usize,
        expression: usize,
    },
    UnsupportedBody {
        feature: UnsupportedBody,
        span: Span,
    },
    Fallthrough {
        function: usize,
    },
    Capacity,
}
impl From<SourceError> for Error {
    fn from(e: SourceError) -> Self {
        Self::Source(e)
    }
}
impl From<m::Error> for Error {
    fn from(e: m::Error) -> Self {
        Self::Mir(e)
    }
}
impl From<o::Error> for Error {
    fn from(e: o::Error) -> Self {
        Self::Types(e)
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "source ownership MIR lowering failed: {self:?}")
    }
}
impl StdError for Error {}
type Result<T> = std::result::Result<T, Error>;

fn id(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| Error::Capacity)
}
fn ty(ty: r::Type) -> Result<o::Type> {
    Ok(match ty {
        r::Type::I64 => o::Type::I64,
        r::Type::Bool => o::Type::Bool,
        r::Type::Nominal(d) => o::Type::Nominal(m::DefinitionId(id(d.0)?)),
        r::Type::Constructed(c) => o::Type::Constructed(o::ConstructedId(id(c.0)?)),
    })
}
fn constructor(c: r::TypeConstructor, span: Span, profile: Profile) -> Result<o::Constructor> {
    use r::{BuiltinType as B, TypeConstructor as C};
    Ok(match c {
        C::ChannelSetup(t) => o::Constructor::ChannelSetup(ty(t)?),
        C::Sender(t) => o::Constructor::Sender(ty(t)?),
        C::Receiver(t) => o::Constructor::Receiver(ty(t)?),
        C::Task(t) if profile == Profile::Tasks => o::Constructor::Task(ty(t)?),
        C::Task(_) => {
            return Err(Error::UnsupportedBody {
                feature: UnsupportedBody::Task,
                span,
            });
        }
        C::Builtin(b) => o::Constructor::Builtin(match b {
            B::Unit => o::Builtin::Unit,
            B::ChannelError => o::Builtin::ChannelError,
            B::Text => o::Builtin::Text,
            B::Bytes => o::Builtin::Bytes,
            B::Allocator => o::Builtin::Allocator,
            B::AllocError => o::Builtin::AllocError,
            B::BoundsError => o::Builtin::BoundsError,
            B::BufferError => o::Builtin::BufferError,
            B::Utf8Error => o::Builtin::Utf8Error,
            B::OwnershipError => o::Builtin::OwnershipError,
            B::ArenaError => o::Builtin::ArenaError,
            B::TaskError if profile == Profile::Tasks => o::Builtin::TaskError,
            B::TaskError => {
                return Err(Error::UnsupportedBody {
                    feature: UnsupportedBody::Task,
                    span,
                });
            }
        }),
        C::Option(t) => o::Constructor::Option(ty(t)?),
        C::SendResult(t) => o::Constructor::SendResult(ty(t)?),
        C::Result(t, e) => o::Constructor::Result(ty(t)?, ty(e)?),
        C::Vec(t) => o::Constructor::Vec(ty(t)?),
        C::Map(k, v) => o::Constructor::Map(ty(k)?, ty(v)?),
        C::Shared(t) => o::Constructor::Shared(ty(t)?),
        C::Weak(t) => o::Constructor::Weak(ty(t)?),
        C::Arena(t) => o::Constructor::Arena(ty(t)?),
        C::Loan(k, t) => o::Constructor::Loan(
            match k {
                r::LoanKind::Shared => o::LoanKind::Shared,
                r::LoanKind::Exclusive => o::LoanKind::Exclusive,
            },
            ty(t)?,
        ),
        C::ArenaOwner(owner) => o::Constructor::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(id(owner.function.0)?),
            local: id(owner.local.0)?,
        }),
    })
}
struct Context<'a> {
    sources: &'a SourceMap,
    source_ids: Vec<(SourceId, m::SourceId)>,
    budget: m::Budget,
}
impl Context<'_> {
    fn text(&mut self, text: &str) -> Result<String> {
        self.budget.charge(text.len().div_ceil(1024) + 1)?;
        self.budget.bytes(text.len())?;
        let mut out = String::new();
        out.try_reserve_exact(text.len())
            .map_err(|_| Error::Capacity)?;
        out.push_str(text);
        Ok(out)
    }
    fn span(&mut self, span: Span) -> Result<m::Span> {
        self.budget.charge(self.source_ids.len() + 1)?;
        self.sources.slice(span)?;
        let source = self
            .source_ids
            .iter()
            .find(|(id, _)| *id == span.source())
            .ok_or(Error::MissingSource)?
            .1;
        Ok(m::Span {
            source,
            start: id(span.range().start)?,
            end: id(span.range().end)?,
        })
    }
    fn type_use(&mut self, t: r::TypeUse) -> Result<o::TypeUse> {
        Ok(o::TypeUse {
            ty: ty(t.value_type)?,
            span: self.span(t.span)?,
        })
    }
}
/// Definition/constructed indices and owner symbols preserve the checked HIR
/// identity. Sources are remapped from immutable snapshots, never read by path.
/// Type verification alone cannot construct the copy-profile `VerifiedUnit`.
pub fn lower(
    checked: &RegionCheckedBodies,
    sources: &SourceMap,
    limits: o::Limits,
) -> Result<o::VerifiedTypes> {
    let (mut c, table) = construct(
        checked.loaned().moved().bodies(),
        sources,
        limits,
        Profile::Ownership,
    )?;
    Ok(table.verify_with_budget(&mut c.budget, limits.constructed)?)
}

fn construct<'a>(
    bodies: &lang_hir::program::BodyTypes,
    sources: &'a SourceMap,
    limits: o::Limits,
    profile: Profile,
) -> Result<(Context<'a>, o::TypeTable)> {
    let input = bodies.source();
    let mut c = Context {
        sources,
        source_ids: Vec::new(),
        budget: m::Budget::new(limits.unit),
    };
    // Reject oversized caller-owned graphs before reserving output buffers.
    if bodies.types().len() > limits.constructed {
        return Err(o::Error {
            kind: o::ErrorKind::ConstructedLimit {
                requested: bodies.types().len(),
                limit: limits.constructed,
            },
            span: None,
        }
        .into());
    }
    if input.definitions.len() > limits.unit.definitions {
        return Err(m::Error::new(m::ErrorKind::Limit {
            kind: m::LimitKind::Definitions,
            requested: input.definitions.len(),
            limit: limits.unit.definitions,
        })
        .into());
    }
    // One reservation covers the maximum distinct module source count.
    // Each insertion is still capped by the configured source limit.
    c.source_ids = c.budget.vector(input.modules.len())?;
    let mut table = o::TypeTable {
        sources: c.budget.vector(input.modules.len())?,
        definitions: c.budget.vector(input.definitions.len())?,
        constructed: c.budget.vector(bodies.types().len())?,
    };
    let mut source_bytes = 0usize;
    for module in &input.modules {
        c.budget.charge(c.source_ids.len() + 1)?;
        let source = module.span.source();
        if c.source_ids.iter().any(|(s, _)| *s == source) {
            continue;
        }
        let requested = table.sources.len() + 1;
        if requested > limits.unit.sources {
            return Err(m::Error::new(m::ErrorKind::Limit {
                kind: m::LimitKind::Sources,
                requested,
                limit: limits.unit.sources,
            })
            .into());
        }
        let file = sources.file(source)?;
        source_bytes = source_bytes
            .checked_add(file.text().len())
            .ok_or(Error::Capacity)?;
        for (requested, limit, kind) in [
            (
                file.text().len(),
                limits.unit.source_bytes,
                m::LimitKind::SourceBytes,
            ),
            (
                source_bytes,
                limits.unit.total_source_bytes,
                m::LimitKind::TotalSourceBytes,
            ),
        ] {
            if requested > limit {
                return Err(m::Error::new(m::ErrorKind::Limit {
                    kind,
                    requested,
                    limit,
                })
                .into());
            }
        }
        c.source_ids
            .push((source, m::SourceId(id(table.sources.len())?)));
        table.sources.push(m::Source {
            path: c.text(file.name())?,
            text: c.text(file.text())?,
        });
    }
    for d in &input.definitions {
        c.budget.charge(1)?;
        let kind = match &d.kind {
            r::DefinitionKind::Record(fields) => {
                let mut out = c.budget.vector(fields.len())?;
                for f in fields {
                    c.budget.charge(1)?;
                    out.push(o::Field {
                        name: c.text(&f.name.text)?,
                        span: c.span(f.name.span)?,
                        ty: c.type_use(f.value_type)?,
                    });
                }
                o::DefinitionKind::Record(out)
            }
            r::DefinitionKind::Enum(cases) => {
                let mut out = c.budget.vector(cases.len())?;
                for case in cases {
                    c.budget.charge(case.payload.len() + 1)?;
                    let mut payload = c.budget.vector(case.payload.len())?;
                    for t in &case.payload {
                        payload.push(c.type_use(*t)?);
                    }
                    out.push(o::Case {
                        name: c.text(&case.name.text)?,
                        span: c.span(case.name.span)?,
                        payload,
                    });
                }
                o::DefinitionKind::Enum(out)
            }
        };
        table.definitions.push(o::Definition {
            name: c.text(&d.qualified_name)?,
            public: d.public,
            span: c.span(d.span)?,
            kind,
        });
    }
    for t in bodies.types() {
        c.budget.charge(1)?;
        table.constructed.push(o::Constructed {
            constructor: constructor(t.constructor, t.span, profile)?,
            span: c.span(t.span)?,
        });
    }
    Ok((c, table))
}

fn cap(n: usize, limit: usize, kind: m::LimitKind) -> Result<()> {
    if n > limit {
        Err(m::Error::new(m::ErrorKind::Limit {
            kind,
            requested: n,
            limit,
        })
        .into())
    } else {
        Ok(())
    }
}

/// Preserve source function/local identities and immutable parameter kinds.
/// The verified result contains no instructions and cannot authorize execution.
pub fn lower_declarations(
    checked: &RegionCheckedBodies,
    sources: &SourceMap,
    limits: o::Limits,
) -> Result<o::declarations::VerifiedDeclarations> {
    let (mut c, out) = construct_declarations(
        checked.loaned().moved().bodies(),
        sources,
        limits,
        Profile::Ownership,
    )?;
    Ok(out.verify_with_budget(&mut c.budget, limits.constructed)?)
}

fn construct_declarations<'a>(
    bodies: &lang_hir::program::BodyTypes,
    sources: &'a SourceMap,
    limits: o::Limits,
    profile: Profile,
) -> Result<(Context<'a>, o::declarations::Declarations)> {
    use o::declarations as d;
    if let Some(span) = bodies.task_feature_span()
        && profile == Profile::Ownership
    {
        return Err(Error::UnsupportedBody {
            feature: UnsupportedBody::Task,
            span,
        });
    }
    let input = bodies.source();
    cap(
        input.functions.len(),
        limits.unit.functions,
        m::LimitKind::Functions,
    )?;
    cap(
        input.imports.len(),
        limits.unit.imports,
        m::LimitKind::Imports,
    )?;
    let (mut c, table) = construct(bodies, sources, limits, profile)?;
    let mut out = d::Declarations {
        types: table,
        imports: c.budget.vector(input.imports.len())?,
        functions: c.budget.vector(input.functions.len())?,
    };
    for import in &input.imports {
        c.budget.charge(1)?;
        let mut parameters = c.budget.vector(import.parameters.len())?;
        for parameter in &import.parameters {
            parameters.push(o::TypeUse {
                ty: ty(parameter.ty.value_type)?,
                span: c.span(parameter.ty.span)?,
            });
        }
        out.imports.push(d::Import {
            abi: match import.abi {
                r::ImportAbi::Host => d::ImportAbi::Host,
                r::ImportAbi::C => d::ImportAbi::C,
            },
            name: c.text(&import.qualified_name)?,
            span: c.span(import.span)?,
            parameters,
            result: o::TypeUse {
                ty: ty(import.result.value_type)?,
                span: c.span(import.result.span)?,
            },
        });
    }
    let mut locals = 0usize;
    for (i, f) in input.functions.iter().enumerate() {
        c.budget.charge(1)?;
        cap(f.parameters.len(), limits.unit.fields, m::LimitKind::Fields)?;
        locals = locals.checked_add(f.locals.len()).ok_or(Error::Capacity)?;
        cap(locals, limits.unit.registers, m::LimitKind::Registers)?;
        let mut parameters = c.budget.vector(f.parameters.len())?;
        for local in &f.parameters {
            c.budget.charge(1)?;
            parameters.push(d::LocalId(id(local.0)?));
        }
        let mut typed = c.budget.vector(f.locals.len())?;
        for (local_index, local) in f.locals.iter().enumerate() {
            c.budget.charge(1)?;
            let t = bodies
                .functions()
                .get(i)
                .and_then(|f| f.locals().get(local_index))
                .copied()
                .ok_or(Error::MissingLocal {
                    function: i,
                    local: local_index,
                })?;
            let kind = match local.kind {
                r::BindingKind::Parameter => d::LocalKind::Parameter,
                r::BindingKind::Mutable => d::LocalKind::Mutable,
                r::BindingKind::Pattern => d::LocalKind::Pattern,
                r::BindingKind::ArenaOwner => d::LocalKind::ArenaOwner,
            };
            typed.push(d::Local {
                name: c.text(&local.name.text)?,
                span: c.span(local.name.span)?,
                kind,
                ty: o::TypeUse {
                    ty: ty(t)?,
                    span: c.span(local.annotation.map_or(local.name.span, |t| t.span))?,
                },
            });
        }
        out.functions.push(d::Function {
            effect: match f.effect {
                r::FunctionEffect::Synchronous => d::FunctionEffect::Synchronous,
                r::FunctionEffect::Asynchronous { span } => d::FunctionEffect::Asynchronous {
                    span: c.span(span)?,
                },
            },
            abi: match f.abi {
                r::FunctionAbi::Guest => d::FunctionAbi::Guest,
                r::FunctionAbi::CExport => d::FunctionAbi::CExport,
            },
            name: c.text(&f.qualified_name)?,
            public: f.public,
            span: c.span(f.span)?,
            signature_span: c.span(f.signature_span)?,
            parameters,
            result: c.type_use(f.result)?,
            locals: typed,
        });
    }
    Ok((c, out))
}

/// Map resolved source identity explicitly; display names and Rust discriminants
/// are not the ownership MIR operation contract.
/// Source-only intrinsics return None and must lower to existing MIR primitives.
pub const fn core_operation(operation: lang_hir::core::Operation) -> Option<o::core::Operation> {
    Some(match operation {
        lang_hir::core::Operation::Drop | lang_hir::core::Operation::Panic => return None,
        lang_hir::core::Operation::Allocator => o::core::Operation::Allocator,
        lang_hir::core::Operation::TextFromUtf8 => o::core::Operation::TextFromUtf8,
        lang_hir::core::Operation::TextClone => o::core::Operation::TextClone,
        lang_hir::core::Operation::TextLen => o::core::Operation::TextLen,
        lang_hir::core::Operation::TextBytes => o::core::Operation::TextBytes,
        lang_hir::core::Operation::TextEqual => o::core::Operation::TextEqual,
        lang_hir::core::Operation::TextConcat => o::core::Operation::TextConcat,
        lang_hir::core::Operation::BytesNew => o::core::Operation::BytesNew,
        lang_hir::core::Operation::BytesClone => o::core::Operation::BytesClone,
        lang_hir::core::Operation::BytesLen => o::core::Operation::BytesLen,
        lang_hir::core::Operation::BytesGet => o::core::Operation::BytesGet,
        lang_hir::core::Operation::BytesPush => o::core::Operation::BytesPush,
        lang_hir::core::Operation::BytesSet => o::core::Operation::BytesSet,
        lang_hir::core::Operation::VecNew => o::core::Operation::VecNew,
        lang_hir::core::Operation::VecLen => o::core::Operation::VecLen,
        lang_hir::core::Operation::VecPush => o::core::Operation::VecPush,
        lang_hir::core::Operation::VecPop => o::core::Operation::VecPop,
        lang_hir::core::Operation::VecGet => o::core::Operation::VecGet,
        lang_hir::core::Operation::VecGetMut => o::core::Operation::VecGetMut,
        lang_hir::core::Operation::MapNew => o::core::Operation::MapNew,
        lang_hir::core::Operation::MapLen => o::core::Operation::MapLen,
        lang_hir::core::Operation::MapInsert => o::core::Operation::MapInsert,
        lang_hir::core::Operation::MapGet => o::core::Operation::MapGet,
        lang_hir::core::Operation::MapRemove => o::core::Operation::MapRemove,
        lang_hir::core::Operation::SharedNew => o::core::Operation::SharedNew,
        lang_hir::core::Operation::SharedClone => o::core::Operation::SharedClone,
        lang_hir::core::Operation::SharedRead => o::core::Operation::SharedRead,
        lang_hir::core::Operation::SharedReplace => o::core::Operation::SharedReplace,
        lang_hir::core::Operation::WeakNew => o::core::Operation::WeakNew,
        lang_hir::core::Operation::WeakClone => o::core::Operation::WeakClone,
        lang_hir::core::Operation::WeakUpgrade => o::core::Operation::WeakUpgrade,
        lang_hir::core::Operation::ArenaNew => o::core::Operation::ArenaNew,
        lang_hir::core::Operation::ArenaRead => o::core::Operation::ArenaRead,
        lang_hir::core::Operation::ArenaReplace => o::core::Operation::ArenaReplace,
        lang_hir::core::Operation::OptionUnwrap => o::core::Operation::OptionUnwrap,
        lang_hir::core::Operation::ResultUnwrap => o::core::Operation::ResultUnwrap,
        lang_hir::core::Operation::ResultUnwrapErr => o::core::Operation::ResultUnwrapErr,
        lang_hir::core::Operation::ChannelNew => o::core::Operation::ChannelNew,
        lang_hir::core::Operation::ChannelSender => o::core::Operation::ChannelSender,
        lang_hir::core::Operation::ChannelClone => o::core::Operation::ChannelClone,
        lang_hir::core::Operation::ChannelReceiver => o::core::Operation::ChannelReceiver,
        lang_hir::core::Operation::ChannelSeal => o::core::Operation::ChannelSeal,
        lang_hir::core::Operation::ChannelUnsent => o::core::Operation::ChannelUnsent,
        lang_hir::core::Operation::ChannelSend | lang_hir::core::Operation::ChannelReceive => {
            return None;
        }
    })
}
