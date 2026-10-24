//! Read-only traversal of inline values and nested Vec/Map storage. It neither
//! retains nor consumes tokens. Callers must validate returned shared/weak/arena
//! identities and buffer contents before publication; traversal is not a graph,
//! brand, uniqueness or ownership proof. Private physical Map order is not a
//! source iteration contract. All scratch storage is admitted before execution.
use crate::{allocation as a, layout as l, map, vector};
use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    ServiceWork(crate::execution::Failure),
    Layout(l::Error),
    Vector(vector::Error),
    Map(map::Error),
    Cells,
    Work,
}
impl From<l::Error> for Error {
    fn from(error: l::Error) -> Self {
        Self::Layout(error)
    }
}
impl From<vector::Error> for Error {
    fn from(error: vector::Error) -> Self {
        Self::Vector(error)
    }
}
impl From<map::Error> for Error {
    fn from(error: map::Error) -> Self {
        Self::Map(error)
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "managed inspection failed: {self:?}")
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ServiceWork(error) => Some(error),
            Self::Layout(error) => Some(error),
            Self::Vector(error) => Some(error),
            Self::Map(error) => Some(error),
            _ => None,
        }
    }
}

/// One cumulative publication budget, including caller-specific leaf checks.
pub struct Work {
    remaining: usize,
    service_work: Option<crate::execution::ServiceWork>,
}
impl Work {
    pub const fn new(remaining: usize) -> Self {
        Self {
            remaining,
            service_work: None,
        }
    }
    pub fn with_service_work(mut self, meter: crate::execution::ServiceWork) -> Self {
        self.service_work = Some(meter);
        self
    }
    pub const fn remaining(&self) -> usize {
        self.remaining
    }
    pub fn charge(&mut self, amount: usize) -> Result<(), Error> {
        self.remaining = self.remaining.checked_sub(amount).ok_or(Error::Work)?;
        Ok(())
    }
    /// Admit both quotas before committing either; metadata-only local work
    /// continues to use `charge` and contributes no service event.
    pub fn charge_event(&mut self, local: usize, service: u64) -> Result<(), Error> {
        let remaining = self.remaining.checked_sub(local).ok_or(Error::Work)?;
        self.service(service).map_err(Error::ServiceWork)?;
        self.remaining = remaining;
        Ok(())
    }
    pub fn service(&self, amount: u64) -> Result<(), crate::execution::Failure> {
        if let Some(meter) = &self.service_work {
            meter.charge(amount)?;
        }
        Ok(())
    }
}
/// Borrowed token bytes. This observation creates no cleanup obligation and
/// must never be released or used to duplicate an owning value.
#[derive(Debug, Eq, PartialEq)]
pub struct Observation {
    pub ty: o::Type,
    pub bytes: [u8; 32],
    pub span: Option<m::Span>,
}
enum Visit<'l, 't> {
    Value {
        value: l::ReadView<'l, 't>,
        at: l::Location,
        span: Option<m::Span>,
    },
    Token {
        ty: o::Type,
        bytes: [u8; 32],
        span: Option<m::Span>,
    },
    Vector {
        ty: o::Type,
        bytes: [u8; 32],
        index: u64,
        len: u64,
        span: Option<m::Span>,
    },
    Map {
        key: o::Type,
        value: o::Type,
        bytes: [u8; 32],
        index: u64,
        len: u64,
        part: map::Part,
        span: Option<m::Span>,
    },
}
pub struct Inspector<'l, 't> {
    layouts: &'l l::Layouts<'t>,
    cells: usize,
    pending: Vec<Visit<'l, 't>>,
    usage: m::Usage,
}
/// The immutable allocator borrow prevents mutation, release, guest execution
/// or provider callbacks between steps. Dropping a partial read is harmless.
pub struct Read<'a, 'l, 't, P: a::Provider> {
    inspector: &'a mut Inspector<'l, 't>,
    allocator: &'a a::Allocator<P>,
}
impl<'l, 't> Inspector<'l, 't> {
    pub fn preparation_usage(
        layouts: &'l l::Layouts<'t>,
        cells: usize,
        limits: m::Limits,
        initial: m::Usage,
    ) -> Result<m::Usage, Error> {
        let base = layouts.usage();
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(l::Error::State.into());
        }
        let mut budget = m::Budget::with_usage(limits, initial).map_err(l::Error::from)?;
        budget
            .allocate::<Visit<'l, 't>>(cells)
            .map_err(l::Error::from)?;
        Ok(m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        })
    }
    pub fn prepare(
        layouts: &'l l::Layouts<'t>,
        cells: usize,
        limits: m::Limits,
        initial: m::Usage,
    ) -> Result<Self, Error> {
        let base = layouts.usage();
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(l::Error::State.into());
        }
        let mut budget = m::Budget::with_usage(limits, initial).map_err(l::Error::from)?;
        Self::prepare_with_budget(layouts, cells, &mut budget)
    }
    /// Prepare with the original cumulative budget; accepted prefixes survive failure.
    pub fn prepare_with_budget(
        layouts: &'l l::Layouts<'t>,
        cells: usize,
        budget: &mut m::Budget,
    ) -> Result<Self, Error> {
        let initial = m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        };
        let base = layouts.usage();
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(l::Error::State.into());
        }
        let pending = budget.vector(cells).map_err(l::Error::from)?;
        Ok(Self {
            layouts,
            cells,
            pending,
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
    fn push(&mut self, visit: Visit<'l, 't>) -> Result<(), Error> {
        if self.pending.len() == self.cells {
            return Err(Error::Cells);
        }
        self.pending.push(visit);
        Ok(())
    }
    fn span(&self, span: Option<m::Span>) -> Result<(), Error> {
        if span.is_some_and(|s| !self.layouts.span_valid(s)) {
            return Err(l::Error::InvalidSpan.into());
        }
        Ok(())
    }
    pub fn read<'a, P: a::Provider>(
        &'a mut self,
        value: &'a l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &'a a::Allocator<P>,
    ) -> Result<Read<'a, 'l, 't, P>, Error> {
        self.pending.clear();
        if !std::ptr::eq(value.layouts(), self.layouts) {
            return Err(l::Error::State.into());
        }
        self.span(span)?;
        let value = value.observe()?;
        self.push(Visit::Value {
            value,
            at: value.root(),
            span,
        })?;
        Ok(Read {
            inspector: self,
            allocator,
        })
    }
    /// Start at a borrowed collection/leaf token, such as an arena's callback
    /// boundary. The caller still owns the original token and its policy proof.
    pub fn token<'a, P: a::Provider>(
        &'a mut self,
        token: &'a l::Token,
        allocator: &'a a::Allocator<P>,
    ) -> Result<Read<'a, 'l, 't, P>, Error> {
        self.pending.clear();
        self.span(token.leaf.span)?;
        self.push(Visit::Token {
            ty: token.leaf.ty,
            bytes: token.bytes,
            span: token.leaf.span,
        })?;
        Ok(Read {
            inspector: self,
            allocator,
        })
    }
    fn next<P: a::Provider>(
        &mut self,
        allocator: &a::Allocator<P>,
        work: &mut Work,
    ) -> Result<Option<Observation>, Error> {
        while let Some(visit) = self.pending.pop() {
            let observation = match &visit {
                Visit::Value { at, .. } => !matches!(
                    self.layouts
                        .get(at.ty())
                        .ok_or(l::Error::InvalidType(at.ty()))?
                        .kind,
                    l::Kind::Token(_)
                ),
                Visit::Token { .. } => true,
                Visit::Vector { .. } | Visit::Map { .. } => false,
            };
            work.charge_event(1, u64::from(observation))?;
            match visit {
                Visit::Value { value, at, span } => {
                    let layout = self
                        .layouts
                        .get(at.ty())
                        .ok_or(l::Error::InvalidType(at.ty()))?;
                    let fields = match &layout.kind {
                        l::Kind::TaskClaim => return Err(l::Error::InvalidType(at.ty()).into()),
                        l::Kind::Scalar | l::Kind::Error { .. } => {
                            value.immediate(at, allocator)?;
                            None
                        }
                        l::Kind::Token(_) => {
                            let token = value.token(at, allocator)?;
                            self.push(Visit::Token {
                                ty: token.leaf.ty,
                                bytes: token.bytes,
                                span,
                            })?;
                            None
                        }
                        l::Kind::Record(fields) => Some(fields),
                        l::Kind::Enum { cases, .. } => Some(
                            cases
                                .get(value.case(at, allocator)? as usize)
                                .ok_or(l::Error::InvalidValue)?,
                        ),
                    };
                    if let Some(fields) = fields {
                        value.initialized_aggregate(at, allocator)?;
                        // Prove exact initialized-field coverage, including
                        // duplicate ledger entries, before exposing children.
                        work.charge(fields.fields.len())?;
                        if self
                            .pending
                            .len()
                            .checked_add(fields.fields.len())
                            .is_none_or(|n| n > self.cells)
                        {
                            return Err(Error::Cells);
                        }
                        for rank in 0..fields.fields.len() {
                            let (field, child_span) = if layout.metadata_bytes == 0 {
                                (rank as u32, span)
                            } else {
                                value.initialized(at, rank as u32, allocator)?
                            };
                            if layout.metadata_bytes != 0 {
                                for previous in 0..rank {
                                    work.charge(1)?;
                                    if value.initialized(at, previous as u32, allocator)?.0 == field
                                    {
                                        return Err(l::Error::InvalidOrder.into());
                                    }
                                }
                            }
                            self.push(Visit::Value {
                                value,
                                at: value.child(at, field, allocator)?,
                                span: child_span.or(span),
                            })?;
                        }
                    }
                }
                Visit::Token {
                    ty,
                    mut bytes,
                    span,
                } => {
                    let l::Kind::Token(constructor) =
                        &self.layouts.get(ty).ok_or(l::Error::InvalidType(ty))?.kind
                    else {
                        return Err(l::Error::InvalidType(ty).into());
                    };
                    match *constructor {
                        o::Constructor::Vec(ty) => {
                            let original = bytes;
                            let vector = vector::Vector::from_token(
                                self.layouts,
                                ty,
                                &mut bytes,
                                allocator,
                            )?;
                            let len = vector.len(allocator)? as u64;
                            self.push(Visit::Vector {
                                ty,
                                bytes: original,
                                index: 0,
                                len,
                                span,
                            })?;
                        }
                        o::Constructor::Map(key, value) => {
                            let original = bytes;
                            let map = map::Map::from_token(
                                self.layouts,
                                key,
                                value,
                                &mut bytes,
                                allocator,
                            )?;
                            let len = map.len(allocator)? as u64;
                            self.push(Visit::Map {
                                key,
                                value,
                                bytes: original,
                                index: 0,
                                len,
                                part: map::Part::Key,
                                span,
                            })?;
                        }
                        _ => return Ok(Some(Observation { ty, bytes, span })),
                    }
                }
                Visit::Vector {
                    ty,
                    bytes,
                    index,
                    len,
                    span,
                } => {
                    if index == len {
                        continue;
                    }
                    let mut token = bytes;
                    let vector =
                        vector::Vector::from_token(self.layouts, ty, &mut token, allocator)?;
                    let (value, child_span) = vector.observation(index, allocator)?;
                    self.push(Visit::Vector {
                        ty,
                        bytes,
                        index: index + 1,
                        len,
                        span,
                    })?;
                    self.push(Visit::Value {
                        value,
                        at: value.root(),
                        span: child_span.or(span),
                    })?;
                }
                Visit::Map {
                    key,
                    value,
                    bytes,
                    index,
                    len,
                    part,
                    span,
                } => {
                    if index == len {
                        continue;
                    }
                    let mut token = bytes;
                    let map =
                        map::Map::from_token(self.layouts, key, value, &mut token, allocator)?;
                    let (observed, child_span) = map.observation(index, part, allocator)?;
                    self.push(Visit::Map {
                        key,
                        value,
                        bytes,
                        index: index + u64::from(matches!(part, map::Part::Value)),
                        len,
                        part: match part {
                            map::Part::Key => map::Part::Value,
                            map::Part::Value => map::Part::Key,
                        },
                        span,
                    })?;
                    self.push(Visit::Value {
                        value: observed,
                        at: observed.root(),
                        span: child_span.or(span),
                    })?;
                }
            }
        }
        Ok(None)
    }
}
impl<P: a::Provider> Read<'_, '_, '_, P> {
    pub fn next(&mut self, work: &mut Work) -> Result<Option<Observation>, Error> {
        let result = self.inspector.next(self.allocator, work);
        if result.is_err() {
            self.inspector.pending.clear();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_collection_initializer_spans_refuse_read_only_and_restore_for_retry() {
        let span = m::Span {
            source: m::SourceId(0),
            start: 0,
            end: 1,
        };
        let ty = |id| o::Type::Constructed(o::ConstructedId(id));
        let types = o::TypeTable {
            sources: vec![m::Source {
                path: "spans.lang".into(),
                text: "x".into(),
            }],
            constructed: [
                o::Constructor::Vec(o::Type::I64),
                o::Constructor::Map(o::Type::I64, o::Type::I64),
            ]
            .into_iter()
            .map(|constructor| o::Constructed { constructor, span })
            .collect(),
            ..Default::default()
        }
        .verify(Default::default())
        .unwrap();
        let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut key = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
        let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
        let mut vector = vector::Vector::new(&layouts, o::Type::I64, &mut allocator).unwrap();
        vector
            .push_copy(&mut value, Some(span), &mut allocator)
            .unwrap();
        let mut map = map::Map::new(&layouts, o::Type::I64, o::Type::I64, &mut allocator).unwrap();
        map.publish_if_absent(
            &mut key,
            &mut value,
            map::Spans {
                key: Some(span),
                value: Some(span),
            },
            &mut allocator,
        )
        .unwrap();
        key.release_storage(&mut allocator).unwrap();
        value.release_storage(&mut allocator).unwrap();
        let mut inspector =
            Inspector::prepare(&layouts, 2, Default::default(), layouts.usage()).unwrap();
        for (ty, bytes, offset) in [
            (ty(0), vector.take_token(&allocator).unwrap(), 56),
            (ty(1), map.take_token(&allocator).unwrap(), 96),
        ] {
            let root = l::Token {
                leaf: l::Leaf {
                    ty,
                    offset: 0,
                    span: Some(span),
                },
                bytes,
            };
            let owner = a::Handle::from_token(root.bytes[..24].try_into().unwrap()).unwrap();
            let metadata = a::Handle::from_token(
                allocator.bytes(owner).unwrap()[offset..offset + 24]
                    .try_into()
                    .unwrap(),
            )
            .unwrap();
            let original = allocator.bytes(metadata).unwrap().to_vec();
            for (at, word) in [(12, 2u32), (8, 99u32)] {
                allocator.bytes_mut(metadata).unwrap()[at..at + 4]
                    .copy_from_slice(&word.to_le_bytes());
                let invalid = allocator.bytes(metadata).unwrap().to_vec();
                let before = allocator.stats();
                assert!(
                    inspector
                        .token(&root, &allocator)
                        .unwrap()
                        .next(&mut Work::new(100))
                        .is_err()
                );
                assert_eq!(allocator.bytes(metadata).unwrap(), invalid);
                assert_eq!(allocator.stats(), before);
                allocator
                    .bytes_mut(metadata)
                    .unwrap()
                    .copy_from_slice(&original);
                assert_eq!(
                    inspector
                        .token(&root, &allocator)
                        .unwrap()
                        .next(&mut Work::new(100)),
                    Ok(None)
                );
            }
            // Copy-only storage owns no leaves; return the observed wrapper to
            // its actual owner for ordinary retirement after the probe.
            let mut token = root.bytes;
            if matches!(
                layouts.get(ty).unwrap().kind,
                l::Kind::Token(o::Constructor::Vec(_))
            ) {
                vector = vector::Vector::from_token(&layouts, o::Type::I64, &mut token, &allocator)
                    .unwrap();
            } else {
                map = map::Map::from_token(
                    &layouts,
                    o::Type::I64,
                    o::Type::I64,
                    &mut token,
                    &allocator,
                )
                .unwrap();
            }
        }
        let mut instance = crate::cleanup::Instance::default();
        let mut frame = instance
            .enter_with_context(
                crate::cleanup::Limits {
                    slots: 1,
                    ..Default::default()
                },
                |_: &mut (), _, ctx: &mut crate::cleanup::Context<'_>| {
                    vector
                        .cleanup(&mut allocator, ctx, |_, _, _, _| panic!("Copy leaf"))
                        .unwrap();
                    map.cleanup(&mut allocator, ctx, |_, _, _, _| panic!("Copy leaf"))
                        .unwrap();
                    Ok(())
                },
            )
            .unwrap();
        frame
            .declare(crate::cleanup::Slot(0), crate::cleanup::Kind::Owner)
            .unwrap();
        frame
            .initialize(crate::cleanup::Slot(0), (), Some(span))
            .unwrap();
        assert!(
            frame
                .finish(crate::cleanup::Exit::Return(None))
                .report
                .primary()
                .is_none()
        );
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}
