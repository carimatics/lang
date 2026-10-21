//! Private managed-execution layouts and iterative drop descriptors. These are
//! neither a source/C ABI nor a serialized value format or ownership proof.
//! Indirect tokens require their service's liveness/region/ownership validation.
use crate::allocation;
use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_LAYOUT: AtomicU64 = AtomicU64::new(1);

mod storage;
pub(crate) use storage::ReadView;
pub use storage::{
    ArenaError, ChannelError, Immediate, Leaf, Location, OwnershipError, Preview, PreviewError,
    Reservation, Token,
};
mod frame;
pub use frame::{FramePlan, FrameStorage};

const ORDER_HEADER: u64 = 8;
const ORDER_ENTRY: u64 = 24;
pub(crate) const WORK_ENTRY: u64 = 40;

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    ServiceWork(crate::execution::Failure),
    Resource {
        error: m::Error,
        span: Option<m::Span>,
    },
    Allocation(allocation::Error),
    Overflow,
    InvalidType(o::Type),
    UnsupportedChannelProfile {
        span: m::Span,
    },
    UnsupportedTaskProfile {
        span: m::Span,
    },
    InvalidFunction(m::FunctionId),
    InvalidCell(usize),
    CellLimit {
        cells: usize,
        limit: usize,
    },
    FrameCapacity {
        cells: usize,
        capacity: usize,
    },
    InvalidCase(u32),
    InvalidValue,
    InvalidField(u32),
    InvalidOrder,
    InvalidSpan,
    ForeignLocation,
    Uninitialized,
    State,
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid managed value layout: {self:?}")
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ServiceWork(error) => Some(error),
            _ => None,
        }
    }
}
impl From<m::Error> for Error {
    fn from(value: m::Error) -> Self {
        Self::Resource {
            error: value,
            span: None,
        }
    }
}
impl From<allocation::Error> for Error {
    fn from(value: allocation::Error) -> Self {
        Self::Allocation(value)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Field {
    pub ty: o::Type,
    pub offset: u64,
    pub metadata_offset: u64,
    pub span: m::Span,
}
#[derive(Debug, Eq, PartialEq)]
pub struct Aggregate {
    pub fields: Vec<Field>,
}
#[derive(Debug, Eq, PartialEq)]
pub enum Kind {
    /// Payload-free marker; the task adapter owns an unforgeable claim sidecar.
    TaskClaim,
    Scalar,
    Error {
        builtin: o::Builtin,
        cases: u32,
        payload_offset: u64,
    },
    /// A private 32-byte logical token, not a raw pointer or source authority.
    Token(o::Constructor),
    Record(Aggregate),
    Enum {
        payload_offset: u64,
        cases: Vec<Aggregate>,
    },
}
#[derive(Debug, Eq, PartialEq)]
pub struct Layout {
    pub size: u64,
    pub alignment: u64,
    /// Aggregate order/span state; excludes the separately reserved work area.
    pub metadata_bytes: u64,
    pub work_slots: u64,
    pub kind: Kind,
}
impl Layout {
    /// Profile-1 cost of one typed core-service transfer. A standalone token
    /// transfers a fixed handle; inline typed storage copies its full extent.
    /// Separate initialization/retirement metadata is never payload data.
    pub fn service_copy_work(&self) -> u64 {
        if matches!(&self.kind, Kind::Token(_)) {
            0
        } else {
            self.size
        }
    }
    pub fn storage_requests(&self) -> Result<[allocation::Request; 2], Error> {
        let metadata = add(self.metadata_bytes, mul(self.work_slots, WORK_ENTRY)?)?;
        Ok([
            allocation::Request::new(metadata, 8)?,
            allocation::Request::new(self.size, self.alignment)?,
        ])
    }
}

pub struct Layouts<'types> {
    identity: u64,
    types: &'types o::VerifiedTypes,
    tasks: bool,
    entries: Vec<Option<Layout>>,
    usage: m::Usage,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Profile {
    Ownership,
    Tasks,
    Channels,
}
impl<'types> Layouts<'types> {
    pub fn compile(types: &'types o::VerifiedTypes, limits: m::Limits) -> Result<Self, Error> {
        Self::compile_inner(types, limits, Profile::Ownership)
    }
    /// Explicit task proof is required; ordinary layout entry points still refuse it.
    pub fn compile_tasks(
        proof: &'types o::tasks::VerifiedTasks,
        limits: m::Limits,
    ) -> Result<Self, Error> {
        Self::compile_inner(
            proof.typed().places().declarations().types(),
            limits,
            Profile::Tasks,
        )
    }
    /// Private channel service storage, requiring independent task/channel proof.
    /// This grants neither endpoint authority nor wire/C/native-profile admission.
    #[doc(hidden)]
    pub fn compile_channel_tasks(
        proof: &'types o::tasks::VerifiedTasks,
        limits: m::Limits,
    ) -> Result<Self, Error> {
        Self::compile_inner(
            proof.typed().places().declarations().types(),
            limits,
            Profile::Channels,
        )
    }
    fn compile_inner(
        types: &'types o::VerifiedTypes,
        limits: m::Limits,
        profile: Profile,
    ) -> Result<Self, Error> {
        let mut budget = m::Budget::with_usage(
            limits,
            m::Usage {
                work: types.work_used(),
                allocation_bytes: types.requested_bytes(),
                elements: types.elements_used(),
            },
        )?;
        budget.charge(types.table().constructed.len())?;
        let tasks = profile != Profile::Ownership;
        if let Some(span) = types.channel_feature_span()
            && profile != Profile::Channels
        {
            return Err(Error::UnsupportedChannelProfile { span });
        }
        if let Some(span) = types.task_feature_span()
            && !tasks
        {
            return Err(Error::UnsupportedTaskProfile { span });
        }
        let n = types
            .table()
            .definitions
            .len()
            .checked_add(types.table().constructed.len())
            .and_then(|n| n.checked_add(2))
            .ok_or(Error::Overflow)?;
        let mut entries = budget.vector(n)?;
        entries.resize_with(n, || None);
        let mut states = budget.vector::<u8>(n)?;
        states.resize(n, 0);
        let mut edges = 0usize;
        for index in 0..n {
            budget.charge(1)?;
            let ty = type_at(types, index).ok_or(Error::Overflow)?;
            for _ in dependencies(types, ty) {
                budget.charge(1)?;
                edges = edges.checked_add(1).ok_or(Error::Overflow)?;
            }
        }
        let mut stack =
            budget.vector::<(usize, bool)>(n.checked_add(edges).ok_or(Error::Overflow)?)?;
        for root in 0..n {
            if states[root] == 2 {
                continue;
            }
            stack.push((root, false));
            while let Some((index, expanded)) = stack.pop() {
                budget.charge(1)?;
                let ty = type_at(types, index).ok_or(Error::Overflow)?;
                if expanded {
                    let result = build(types, &entries, ty, &mut budget).map_err(|mut error| {
                        if let Error::Resource { span, .. } = &mut error {
                            *span = span.or(type_span(types, ty));
                        }
                        error
                    })?;
                    // Check all host/request arithmetic before a guest callback.
                    result.storage_requests()?;
                    entries[index] = Some(result);
                    states[index] = 2;
                } else if states[index] == 0 {
                    states[index] = 1;
                    stack.push((index, true));
                    for child in dependencies(types, ty).rev() {
                        let child = type_index(types, child).ok_or(Error::InvalidType(child))?;
                        stack.push((child, false));
                    }
                } else if states[index] == 1 {
                    return Err(Error::InvalidType(ty));
                }
            }
        }
        Ok(Self {
            identity: NEXT_LAYOUT
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .map_err(|_| Error::Overflow)?,
            types,
            tasks,
            entries,
            usage: m::Usage {
                work: budget.work_used(),
                allocation_bytes: budget.allocation_bytes(),
                elements: budget.elements_used(),
            },
        })
    }
    pub fn get(&self, ty: o::Type) -> Option<&Layout> {
        self.entries.get(type_index(self.types, ty)?)?.as_ref()
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub const fn types(&self) -> &'types o::VerifiedTypes {
        self.types
    }
    /// Private execution identity; never serialized or exposed as source/ABI data.
    pub(crate) const fn identity(&self) -> u64 {
        self.identity
    }
    pub(crate) fn span_valid(&self, span: m::Span) -> bool {
        self.types
            .table()
            .sources
            .get(span.source.0 as usize)
            .is_some_and(|source| span.start <= span.end && span.end as usize <= source.text.len())
    }
}

fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_mul(b).ok_or(Error::Overflow)
}
fn aligned(size: u64, alignment: u64) -> Result<u64, Error> {
    Ok(add(size, alignment - 1)? & !(alignment - 1))
}
fn type_index(types: &o::VerifiedTypes, ty: o::Type) -> Option<usize> {
    types.properties(ty)?;
    Some(match ty {
        o::Type::I64 => 0,
        o::Type::Bool => 1,
        o::Type::Nominal(id) => 2 + id.0 as usize,
        o::Type::Constructed(id) => 2 + types.table().definitions.len() + id.0 as usize,
    })
}
fn type_at(types: &o::VerifiedTypes, index: usize) -> Option<o::Type> {
    let definitions = types.table().definitions.len();
    Some(match index {
        0 => o::Type::I64,
        1 => o::Type::Bool,
        i if i - 2 < definitions => o::Type::Nominal(m::DefinitionId(u32::try_from(i - 2).ok()?)),
        i => o::Type::Constructed(o::ConstructedId(u32::try_from(i - 2 - definitions).ok()?)),
    })
}
fn type_span(types: &o::VerifiedTypes, ty: o::Type) -> Option<m::Span> {
    match ty {
        o::Type::Nominal(id) => Some(types.table().definitions[id.0 as usize].span),
        o::Type::Constructed(id) => Some(types.table().constructed[id.0 as usize].span),
        _ => None,
    }
}
fn dependencies(types: &o::VerifiedTypes, ty: o::Type) -> impl DoubleEndedIterator<Item = o::Type> {
    let (mut fixed, mut fields, mut cases): ([Option<o::Type>; 2], &[o::Field], &[o::Case]) =
        ([None, None], &[], &[]);
    match ty {
        o::Type::Nominal(id) => match &types.table().definitions[id.0 as usize].kind {
            o::DefinitionKind::Record(f) => fields = f,
            o::DefinitionKind::Enum(c) => cases = c,
        },
        o::Type::Constructed(id) => match types.table().constructed[id.0 as usize].constructor {
            o::Constructor::Option(t) | o::Constructor::SendResult(t) => fixed[0] = Some(t),
            o::Constructor::Result(t, e) => fixed = [Some(t), Some(e)],
            _ => {}
        },
        _ => {}
    }
    fixed
        .into_iter()
        .flatten()
        .chain(fields.iter().map(|f| f.ty.ty))
        .chain(
            cases
                .iter()
                .flat_map(|case| case.payload.iter().map(|t| t.ty)),
        )
}

struct AggregateLayout {
    aggregate: Aggregate,
    size: u64,
    alignment: u64,
    metadata: u64,
    work: u64,
}
fn aggregate(
    types: &o::VerifiedTypes,
    entries: &[Option<Layout>],
    fields: impl ExactSizeIterator<Item = o::TypeUse>,
    dropping: bool,
    budget: &mut m::Budget,
) -> Result<AggregateLayout, Error> {
    let count = fields.len();
    let mut result = AggregateLayout {
        aggregate: Aggregate {
            fields: budget.vector(count)?,
        },
        size: 0,
        alignment: 1,
        metadata: if dropping {
            add(ORDER_HEADER, mul(count as u64, ORDER_ENTRY)?)?
        } else {
            0
        },
        work: u64::from(dropping),
    };
    for field in fields {
        budget.charge(1)?;
        let child = entries[type_index(types, field.ty).ok_or(Error::InvalidType(field.ty))?]
            .as_ref()
            .ok_or(Error::InvalidType(field.ty))?;
        let offset = aligned(result.size, child.alignment)?;
        result.aggregate.fields.push(Field {
            ty: field.ty,
            offset,
            metadata_offset: result.metadata,
            span: field.span,
        });
        result.size = add(offset, child.size)?;
        result.alignment = result.alignment.max(child.alignment);
        result.metadata = add(result.metadata, child.metadata_bytes)?;
        result.work = add(result.work, child.work_slots)?;
    }
    result.size = aligned(result.size, result.alignment)?;
    Ok(result)
}
fn build(
    types: &o::VerifiedTypes,
    entries: &[Option<Layout>],
    ty: o::Type,
    budget: &mut m::Budget,
) -> Result<Layout, Error> {
    let dropping = types
        .properties(ty)
        .ok_or(Error::InvalidType(ty))?
        .requires_drop;
    let simple = |size, alignment, kind| Layout {
        size,
        alignment,
        kind,
        metadata_bytes: 0,
        work_slots: u64::from(dropping),
    };
    let from_record = |a: AggregateLayout| Layout {
        size: a.size,
        alignment: a.alignment,
        metadata_bytes: a.metadata,
        work_slots: a.work,
        kind: Kind::Record(a.aggregate),
    };
    let mut variants;
    match ty {
        o::Type::I64 => return Ok(simple(8, 8, Kind::Scalar)),
        o::Type::Bool => return Ok(simple(1, 1, Kind::Scalar)),
        o::Type::Nominal(id) => match &types.table().definitions[id.0 as usize].kind {
            o::DefinitionKind::Record(fields) => {
                return Ok(from_record(aggregate(
                    types,
                    entries,
                    fields.iter().map(|f| f.ty),
                    dropping,
                    budget,
                )?));
            }
            o::DefinitionKind::Enum(cases) => {
                variants = budget.vector(cases.len())?;
                for case in cases {
                    budget.charge(1)?;
                    variants.push(aggregate(
                        types,
                        entries,
                        case.payload.iter().copied(),
                        dropping,
                        budget,
                    )?);
                }
            }
        },
        o::Type::Constructed(id) => {
            let entry = types.table().constructed[id.0 as usize];
            let use_type = |ty| o::TypeUse {
                ty,
                span: entry.span,
            };
            match entry.constructor {
                o::Constructor::Option(t) | o::Constructor::SendResult(t) => {
                    let cases = if matches!(entry.constructor, o::Constructor::SendResult(_)) {
                        3
                    } else {
                        2
                    };
                    variants = budget.vector(cases)?;
                    variants.push(aggregate(types, entries, [].into_iter(), dropping, budget)?);
                    for _ in 1..cases {
                        variants.push(aggregate(
                            types,
                            entries,
                            [use_type(t)].into_iter(),
                            dropping,
                            budget,
                        )?);
                    }
                }
                o::Constructor::Result(t, e) => {
                    variants = budget.vector(2)?;
                    variants.push(aggregate(
                        types,
                        entries,
                        [use_type(t)].into_iter(),
                        dropping,
                        budget,
                    )?);
                    variants.push(aggregate(
                        types,
                        entries,
                        [use_type(e)].into_iter(),
                        dropping,
                        budget,
                    )?);
                }
                o::Constructor::Builtin(o::Builtin::Unit) => return Ok(simple(0, 1, Kind::Scalar)),
                o::Constructor::ChannelSetup(_)
                | o::Constructor::Sender(_)
                | o::Constructor::Receiver(_) => {
                    return Ok(simple(32, 8, Kind::Token(entry.constructor)));
                }
                o::Constructor::Task(_) => {
                    return Ok(Layout {
                        work_slots: 0,
                        ..simple(0, 1, Kind::TaskClaim)
                    });
                }
                o::Constructor::Builtin(
                    builtin @ (o::Builtin::AllocError
                    | o::Builtin::BoundsError
                    | o::Builtin::BufferError
                    | o::Builtin::Utf8Error
                    | o::Builtin::OwnershipError
                    | o::Builtin::ArenaError
                    | o::Builtin::ChannelError
                    | o::Builtin::TaskError),
                ) => {
                    let (size, alignment, cases, payload_offset) = match builtin {
                        o::Builtin::AllocError => (4, 4, 3, 4),
                        o::Builtin::BoundsError => (4, 4, 1, 4),
                        o::Builtin::TaskError => (4, 4, 1, 4),
                        o::Builtin::ChannelError => (8, 4, 2, 4),
                        o::Builtin::Utf8Error => (16, 8, 1, 8),
                        o::Builtin::BufferError => (8, 4, 3, 4),
                        o::Builtin::OwnershipError => (8, 4, 6, 4),
                        o::Builtin::ArenaError => (8, 4, 4, 4),
                        _ => unreachable!(),
                    };
                    return Ok(simple(
                        size,
                        alignment,
                        Kind::Error {
                            builtin,
                            cases,
                            payload_offset,
                        },
                    ));
                }
                constructor => return Ok(simple(32, 8, Kind::Token(constructor))),
            }
        }
    }
    let mut cases = budget.vector(variants.len())?;
    let (mut size, mut alignment, mut metadata, mut work) = (0, 4, 0, 0);
    for variant in variants {
        size = size.max(variant.size);
        alignment = alignment.max(variant.alignment);
        metadata = metadata.max(variant.metadata);
        work = work.max(variant.work);
        cases.push(variant.aggregate);
    }
    let payload_offset = aligned(4, alignment)?;
    Ok(Layout {
        size: aligned(add(payload_offset, size)?, alignment)?,
        alignment,
        metadata_bytes: metadata,
        work_slots: work,
        kind: Kind::Enum {
            payload_offset,
            cases,
        },
    })
}
