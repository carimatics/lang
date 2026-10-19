//! RFC 0010 previews read checked managed values without taking their owners.
//! Output and the iterative depth stack are admitted before guest execution.
use super::*;
use crate::{buffer, map, shared::managed as shared, vector};
mod indirect;
use std::fmt::{self, Write};

#[derive(Debug, Eq, PartialEq)]
pub enum PreviewError {
    Limit,
    Value(Error),
    Buffer(buffer::Fault),
    Vector(vector::Error),
    Map(map::Error),
    Shared(shared::Error),
}
impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "managed preview failed: {self:?}")
    }
}
impl std::error::Error for PreviewError {}
impl From<Error> for PreviewError {
    fn from(error: Error) -> Self {
        Self::Value(error)
    }
}
impl From<buffer::Fault> for PreviewError {
    fn from(error: buffer::Fault) -> Self {
        Self::Buffer(error)
    }
}
#[derive(Clone, Copy)]
struct Site<'l, 't> {
    value: ReadView<'l, 't>,
    location: Location,
    customer: u64,
}
impl<'l, 't> Site<'l, 't> {
    fn root(value: ReadView<'l, 't>, customer: u64) -> Self {
        Self {
            location: value.root(),
            value,
            customer,
        }
    }
    fn child<P: Provider>(self, field: u32, allocator: &Allocator<P>) -> Result<Self, Error> {
        Ok(Self {
            location: self.value.child(self.location, field, allocator)?,
            ..self
        })
    }
}
enum Print<'l, 't> {
    Value(Site<'l, 't>),
    Fields {
        site: Site<'l, 't>,
        next: usize,
        case: Option<u32>,
    },
    Vector {
        ty: o::Type,
        token: [u8; 32],
        customer: u64,
        next: u64,
        len: u64,
    },
    Map(indirect::MapWalk),
    Text(&'static str),
    Immediate(Immediate),
}
#[derive(Clone, Copy)]
struct OrderedKey {
    key: map::Key,
    index: u64,
}
struct Output {
    text: String,
    limit: usize,
}
impl Write for Output {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self
            .text
            .len()
            .checked_add(text.len())
            .is_none_or(|n| n > self.limit)
        {
            return Err(fmt::Error);
        }
        self.text.push_str(text);
        Ok(())
    }
}
pub struct Preview<'l, 't> {
    layouts: &'l Layouts<'t>,
    root: o::Type,
    output: Output,
    pending: Vec<Print<'l, 't>>,
    order: Vec<OrderedKey>,
    usage: m::Usage,
}
impl<'l, 't> Preview<'l, 't> {
    /// Reject unsupported/inactive payloads and reserve bounded host storage.
    /// Preparation shares the caller's cumulative compiler/host budget.
    pub fn prepare(
        layouts: &'l Layouts<'t>,
        root: o::Type,
        limit: usize,
        limits: m::Limits,
        initial: m::Usage,
    ) -> Result<Self, Error> {
        let base = layouts.usage();
        if initial.work < base.work
            || initial.allocation_bytes < base.allocation_bytes
            || initial.elements < base.elements
        {
            return Err(Error::State);
        }
        let mut budget = m::Budget::with_usage(limits, initial)?;
        let types = layouts.types;
        let count = types
            .table()
            .definitions
            .len()
            .checked_add(types.table().constructed.len())
            .and_then(|n| n.checked_add(2))
            .ok_or(Error::Overflow)?;
        let mut seen = budget.vector(count)?;
        seen.resize(count, false);
        let mut visit = budget.vector(count)?;
        let mut reached = 0usize;
        let mut indirect = false;
        let mut maps = false;
        let enqueue = |ty,
                       seen: &mut Vec<bool>,
                       visit: &mut Vec<o::Type>,
                       budget: &mut m::Budget|
         -> Result<(), Error> {
            budget.charge(1)?;
            let index = type_index(types, ty).ok_or(Error::InvalidType(ty))?;
            if !seen[index] {
                seen[index] = true;
                visit.push(ty);
            }
            Ok(())
        };
        enqueue(root, &mut seen, &mut visit, &mut budget)?;
        while let Some(ty) = visit.pop() {
            reached += 1;
            match ty {
                o::Type::I64 | o::Type::Bool => {}
                o::Type::Nominal(id) => match &types.table().definitions[id.0 as usize].kind {
                    o::DefinitionKind::Record(fields) => {
                        for field in fields {
                            enqueue(field.ty.ty, &mut seen, &mut visit, &mut budget)?;
                        }
                    }
                    o::DefinitionKind::Enum(cases) => {
                        for case in cases {
                            for field in &case.payload {
                                enqueue(field.ty, &mut seen, &mut visit, &mut budget)?;
                            }
                        }
                    }
                },
                o::Type::Constructed(id) => match types.table().constructed[id.0 as usize]
                    .constructor
                {
                    o::Constructor::Builtin(
                        o::Builtin::Unit
                        | o::Builtin::Bytes
                        | o::Builtin::Text
                        | o::Builtin::AllocError
                        | o::Builtin::BoundsError
                        | o::Builtin::BufferError
                        | o::Builtin::Utf8Error
                        | o::Builtin::OwnershipError
                        | o::Builtin::ArenaError,
                    ) => {}
                    o::Constructor::Vec(t) | o::Constructor::Shared(t) => {
                        indirect = true;
                        enqueue(t, &mut seen, &mut visit, &mut budget)?;
                    }
                    o::Constructor::Map(k, v) => {
                        indirect = true;
                        maps = true;
                        enqueue(k, &mut seen, &mut visit, &mut budget)?;
                        enqueue(v, &mut seen, &mut visit, &mut budget)?;
                    }
                    o::Constructor::Weak(_) => {
                        indirect = true;
                    }
                    o::Constructor::Option(t) => enqueue(t, &mut seen, &mut visit, &mut budget)?,
                    o::Constructor::Result(a, b) => {
                        enqueue(a, &mut seen, &mut visit, &mut budget)?;
                        enqueue(b, &mut seen, &mut visit, &mut budget)?;
                    }
                    _ => return Err(Error::InvalidType(ty)),
                },
            }
        }
        let capacity = reached
            .checked_add(4)
            .and_then(|n| n.checked_mul(3))
            .ok_or(Error::Overflow)?;
        let capacity = capacity
            .checked_add(if indirect { limits.value_cells } else { 0 })
            .ok_or(Error::Overflow)?;
        let pending = budget.vector(capacity)?;
        let order = budget.vector(if maps {
            (limit / 4).min(limits.value_cells)
        } else {
            0
        })?;
        let bytes: Vec<u8> = budget.vector(limit)?;
        let text = String::from_utf8(bytes).map_err(|_| Error::InvalidValue)?;
        Ok(Self {
            layouts,
            root,
            output: Output { text, limit },
            pending,
            order,
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
    fn push(&mut self, item: Print<'l, 't>) -> Result<(), PreviewError> {
        if self.pending.len() == self.pending.capacity() {
            return Err(PreviewError::Limit);
        }
        self.pending.push(item);
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<(), PreviewError> {
        self.output.write_str(text).map_err(|_| PreviewError::Limit)
    }
    fn number(&mut self, n: i64) -> Result<(), PreviewError> {
        write!(&mut self.output, "{n}").map_err(|_| PreviewError::Limit)
    }
    fn immediate(&mut self, value: Immediate) -> Result<(), PreviewError> {
        let (name, payload) = match value {
            Immediate::I64(n) => return self.number(n),
            Immediate::Bool(b) => return self.text(if b { "true" } else { "false" }),
            Immediate::Unit => return self.text("core::Unit {}"),
            Immediate::Allocation(v) => (
                match v {
                    buffer::AllocError::Limit => "core::AllocError::Limit",
                    buffer::AllocError::OutOfMemory => "core::AllocError::OutOfMemory",
                    buffer::AllocError::Overflow => "core::AllocError::Overflow",
                },
                None,
            ),
            Immediate::Bounds(_) => ("core::BoundsError::OutOfBounds", None),
            Immediate::Buffer(v) => (
                match v {
                    buffer::BufferError::Bounds => "core::BufferError::Bounds",
                    buffer::BufferError::InvalidByte => "core::BufferError::InvalidByte",
                    buffer::BufferError::Allocation(_) => "core::BufferError::Allocation",
                },
                value.payload(),
            ),
            Immediate::Utf8(_) => ("core::Utf8Error::Invalid", value.payload()),
            Immediate::Ownership(v) => (
                match v {
                    OwnershipError::Allocation(_) => "core::OwnershipError::Allocation",
                    OwnershipError::StrongCycle => "core::OwnershipError::StrongCycle",
                    OwnershipError::ExpiredWeak => "core::OwnershipError::ExpiredWeak",
                    OwnershipError::BorrowConflict => "core::OwnershipError::BorrowConflict",
                    OwnershipError::Unavailable => "core::OwnershipError::Unavailable",
                    OwnershipError::CountOverflow => "core::OwnershipError::CountOverflow",
                },
                value.payload(),
            ),
            Immediate::Arena(v) => (
                match v {
                    ArenaError::Allocation(_) => "core::ArenaError::Allocation",
                    ArenaError::CrossRegion => "core::ArenaError::CrossRegion",
                    ArenaError::Unavailable => "core::ArenaError::Unavailable",
                    ArenaError::BorrowConflict => "core::ArenaError::BorrowConflict",
                },
                value.payload(),
            ),
        };
        self.text(name)?;
        if let Some(payload) = payload {
            self.text("(")?;
            self.push(Print::Text(")"))?;
            self.push(Print::Immediate(payload))?;
        }
        Ok(())
    }
    fn aggregate(
        &mut self,
        site: Site<'l, 't>,
        case: Option<u32>,
        fields: usize,
        record: bool,
    ) -> Result<(), PreviewError> {
        if record {
            self.text(if fields == 0 { " {}" } else { " { " })?;
            if fields == 0 {
                return Ok(());
            }
        } else {
            if fields == 0 {
                return Ok(());
            }
            self.text("(")?;
        }
        self.push(Print::Text(if record { " }" } else { ")" }))?;
        self.push(Print::Fields {
            site,
            next: 0,
            case,
        })
    }
    /// Read-only rendering; success/failure never transfers or retires a token.
    /// The adapter must consume the result and check cleanup before publication.
    pub fn render<P: Provider>(
        self,
        cell: &Reservation<'l, 't>,
        allocator: &Allocator<P>,
    ) -> Result<String, PreviewError> {
        self.render_with_shared(cell, allocator, None)
    }
    /// The registry is immutably borrowed with the allocator and protected result.
    /// Formatting never creates guest read tickets or changes reference counts.
    pub fn render_with_shared<P: Provider>(
        mut self,
        cell: &Reservation<'l, 't>,
        allocator: &Allocator<P>,
        shared: Option<&shared::Heap<'l, 't>>,
    ) -> Result<String, PreviewError> {
        if !std::ptr::eq(self.layouts, cell.layouts)
            || self.layouts.types.equivalent(self.root, cell.ty) != Some(true)
        {
            return Err(Error::ForeignLocation.into());
        }
        let layouts = self.layouts;
        self.push(Print::Value(Site::root(cell.observe()?, 0)))?;
        while let Some(item) = self.pending.pop() {
            match item {
                Print::Text(text) => self.text(text)?,
                Print::Immediate(value) => self.immediate(value)?,
                Print::Value(site) => {
                    let location = site.location;
                    match location.ty {
                        o::Type::I64 | o::Type::Bool => {
                            self.immediate(site.value.immediate(location, allocator)?)?
                        }
                        o::Type::Nominal(id) => {
                            let definition = &layouts.types.table().definitions[id.0 as usize];
                            self.text(&definition.name)?;
                            match &definition.kind {
                                o::DefinitionKind::Record(fields) => {
                                    self.aggregate(site, None, fields.len(), true)?
                                }
                                o::DefinitionKind::Enum(cases) => {
                                    let case = site.value.case(location, allocator)?;
                                    let definition =
                                        cases.get(case as usize).ok_or(Error::InvalidCase(case))?;
                                    self.text("::")?;
                                    self.text(&definition.name)?;
                                    self.aggregate(
                                        site,
                                        Some(case),
                                        definition.payload.len(),
                                        false,
                                    )?;
                                }
                            }
                        }
                        o::Type::Constructed(id) => {
                            match layouts.types.table().constructed[id.0 as usize].constructor {
                                o::Constructor::Builtin(o::Builtin::Bytes | o::Builtin::Text) => {
                                    let builtin = layouts.types.table().constructed[id.0 as usize]
                                        .constructor;
                                    let mut token = site.value.token(location, allocator)?.bytes;
                                    if builtin == o::Constructor::Builtin(o::Builtin::Bytes) {
                                        let bytes =
                                            buffer::Bytes::from_token(&mut token, allocator)?;
                                        self.text("core::Bytes [")?;
                                        if bytes.len(allocator)? as usize
                                            > self.output.limit - self.output.text.len()
                                        {
                                            return Err(PreviewError::Limit);
                                        }
                                        for (i, byte) in
                                            bytes.as_slice(allocator)?.iter().enumerate()
                                        {
                                            if i != 0 {
                                                self.text(", ")?;
                                            }
                                            self.number(i64::from(*byte))?;
                                        }
                                        self.text("]")?;
                                    } else {
                                        let text = buffer::Text::from_token(&mut token, allocator)?;
                                        self.text("core::Text \"")?;
                                        if text.len(allocator)? as usize
                                            > self.output.limit - self.output.text.len()
                                        {
                                            return Err(PreviewError::Limit);
                                        }
                                        for character in text.as_str(allocator)?.chars() {
                                            match character {
                                                '"' => self.text("\\\"")?,
                                                '\\' => self.text("\\\\")?,
                                                '\n' => self.text("\\n")?,
                                                '\r' => self.text("\\r")?,
                                                '\t' => self.text("\\t")?,
                                                ' '..='~' => self
                                                    .output
                                                    .write_char(character)
                                                    .map_err(|_| PreviewError::Limit)?,
                                                other => write!(
                                                    &mut self.output,
                                                    "\\u{{{:x}}}",
                                                    u32::from(other)
                                                )
                                                .map_err(|_| PreviewError::Limit)?,
                                            }
                                        }
                                        self.text("\"")?;
                                    }
                                }
                                o::Constructor::Builtin(_) => {
                                    self.immediate(site.value.immediate(location, allocator)?)?
                                }
                                o::Constructor::Option(_) | o::Constructor::Result(..) => {
                                    let case = site.value.case(location, allocator)?;
                                    let option = matches!(
                                        layouts.types.table().constructed[id.0 as usize]
                                            .constructor,
                                        o::Constructor::Option(_)
                                    );
                                    self.text(if option {
                                        "core::Option::"
                                    } else {
                                        "core::Result::"
                                    })?;
                                    self.text(if option {
                                        if case == 0 { "None" } else { "Some" }
                                    } else if case == 0 {
                                        "Ok"
                                    } else {
                                        "Err"
                                    })?;
                                    let Kind::Enum { cases, .. } = &layouts
                                        .get(location.ty)
                                        .ok_or(Error::InvalidType(location.ty))?
                                        .kind
                                    else {
                                        return Err(Error::InvalidType(location.ty).into());
                                    };
                                    self.aggregate(
                                        site,
                                        Some(case),
                                        cases[case as usize].fields.len(),
                                        false,
                                    )?;
                                }
                                o::Constructor::Vec(ty) => self.vector(site, ty, allocator)?,
                                o::Constructor::Map(key, value) => {
                                    self.map(site, key, value, allocator)?
                                }
                                o::Constructor::Shared(_) | o::Constructor::Weak(_) => {
                                    self.shared(site, shared.ok_or(Error::State)?, allocator)?;
                                }
                                _ => return Err(Error::InvalidType(location.ty).into()),
                            }
                        }
                    }
                }
                Print::Vector {
                    ty,
                    token,
                    customer,
                    next,
                    len,
                } => {
                    self.vector_next(ty, token, customer, next, len, allocator)?;
                }
                Print::Map(walk) => self.map_next(walk, allocator)?,
                Print::Fields { site, next, case } => {
                    let location = site.location;
                    let Kind::Record(record) = &layouts
                        .get(location.ty)
                        .ok_or(Error::InvalidType(location.ty))?
                        .kind
                    else {
                        let Kind::Enum { cases, .. } = &layouts.get(location.ty).unwrap().kind
                        else {
                            return Err(Error::InvalidType(location.ty).into());
                        };
                        let count = cases[case.ok_or(Error::State)? as usize].fields.len();
                        if next < count {
                            if next != 0 {
                                self.text(", ")?;
                            }
                            self.push(Print::Fields {
                                site,
                                next: next + 1,
                                case,
                            })?;
                            self.push(Print::Value(site.child(next as u32, allocator)?))?;
                        }
                        continue;
                    };
                    if next < record.fields.len() {
                        if next != 0 {
                            self.text(", ")?;
                        }
                        let o::Type::Nominal(id) = location.ty else {
                            return Err(Error::InvalidType(location.ty).into());
                        };
                        let o::DefinitionKind::Record(fields) =
                            &layouts.types.table().definitions[id.0 as usize].kind
                        else {
                            return Err(Error::InvalidType(location.ty).into());
                        };
                        self.text(&fields[next].name)?;
                        self.text(": ")?;
                        self.push(Print::Fields {
                            site,
                            next: next + 1,
                            case,
                        })?;
                        self.push(Print::Value(site.child(next as u32, allocator)?))?;
                    }
                }
            }
        }
        Ok(self.output.text)
    }
}
