//! Bounded declaration-only component IDL v1 (RFC 0027).
//!
//! Checking metadata grants no allocator, import, resource or execution authority.
use lang_mir::unit as m;
use std::fmt;

mod json;
mod model;
mod schema;
mod validate;
mod versions;
pub use model::*;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub source_bytes: usize,
    pub depth: usize,
    pub declarations: usize,
    pub elements: usize,
    pub string_bytes: usize,
    pub work: usize,
    pub allocation_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            source_bytes: 1024 * 1024,
            depth: 64,
            declarations: 4096,
            elements: 65_536,
            string_bytes: 64 * 1024,
            work: 1_000_000,
            allocation_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Code {
    Syntax,
    Field,
    Name,
    Type,
    Projection,
    Resource,
    Version,
    Unsupported,
}
impl Code {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "IDL0001",
            Self::Field => "IDL0002",
            Self::Name => "IDL0003",
            Self::Type => "IDL0004",
            Self::Projection => "IDL0005",
            Self::Resource => "IDL0006",
            Self::Version => "IDL0007",
            Self::Unsupported => "IDL0008",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    SourceBytes,
    Depth,
    Declarations,
    Elements,
    StringBytes,
    Arity,
    Capacity,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reason {
    Syntax,
    Expected(&'static str),
    MissingField(&'static str),
    UnknownField,
    Duplicate,
    InvalidName,
    Unresolved,
    CyclicType,
    InvalidBorrow,
    InvalidEffect,
    InvalidProjection,
    InvalidVersion,
    Limit {
        kind: LimitKind,
        limit: usize,
        requested: usize,
    },
    Budget(m::ErrorKind),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub code: Code,
    pub reason: Reason,
    pub span: m::Span,
    pub related: Option<m::Span>,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}..{}: {:?}",
            self.code.as_str(),
            self.span.start,
            self.span.end,
            self.reason
        )
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: m::Span,
}

pub fn check(source: m::SourceId, input: &[u8], limits: Limits) -> Result<CheckedDocument, Error> {
    let mut c = Context::new(source, limits);
    let parsed = json::parse(input, &mut c)?;
    let mut document = schema::parse(&parsed, &mut c)?;
    drop(parsed);
    validate::check(&mut document, &mut c)?;
    Ok(CheckedDocument {
        document,
        usage: Usage {
            budget: m::Usage {
                work: c.budget.work_used(),
                allocation_bytes: c.budget.allocation_bytes(),
                elements: c.budget.elements_used(),
            },
            declarations: c.declarations,
            elements: c.elements,
            data_depth: c.data_depth,
        },
    })
}

struct Context {
    source: m::SourceId,
    limits: Limits,
    budget: m::Budget,
    declarations: usize,
    elements: usize,
    data_depth: usize,
}
// Binary metadata has already checked its closed fields, names and counts.
// Reuse the complete independent graph/projection proof and cumulative budget.
pub(super) fn check_decoded(
    mut document: Document,
    limits: Limits,
    budget: m::Budget,
    declarations: usize,
    elements: usize,
) -> (Result<CheckedDocument, Error>, m::Budget) {
    let mut c = Context {
        source: document.source,
        limits,
        budget,
        declarations,
        elements,
        data_depth: 0,
    };
    let result = validate::check(&mut document, &mut c).map(|()| CheckedDocument {
        document,
        usage: Usage {
            budget: m::Usage {
                work: c.budget.work_used(),
                allocation_bytes: c.budget.allocation_bytes(),
                elements: c.budget.elements_used(),
            },
            declarations,
            elements,
            data_depth: c.data_depth,
        },
    });
    (result, c.budget)
}
impl Context {
    fn new(source: m::SourceId, limits: Limits) -> Self {
        Self {
            source,
            limits,
            declarations: 0,
            elements: 0,
            data_depth: 0,
            budget: m::Budget::new(m::Limits {
                work: limits.work,
                allocations: limits.allocation_bytes,
                // Domain elements have their own cap. Byte/work/source bounds
                // constrain temporary JSON buffers without an unrelated default.
                elements: usize::MAX,
                ..Default::default()
            }),
        }
    }
    fn span(&self, start: usize, end: usize) -> m::Span {
        m::Span {
            source: self.source,
            start: start as u32,
            end: end as u32,
        }
    }
    fn error(&self, code: Code, reason: Reason, span: m::Span) -> Error {
        Error {
            code,
            reason,
            span,
            related: None,
        }
    }
    fn charge(&mut self, n: usize, span: m::Span) -> Result<(), Error> {
        self.budget.charge(n).map_err(|e| Error {
            code: Code::Resource,
            reason: Reason::Budget(e.kind),
            span,
            related: None,
        })
    }
    fn cap(&self, n: usize, max: usize, kind: LimitKind, span: m::Span) -> Result<(), Error> {
        if n > max {
            Err(self.error(
                Code::Resource,
                Reason::Limit {
                    kind,
                    limit: max,
                    requested: n,
                },
                span,
            ))
        } else {
            Ok(())
        }
    }
    fn push<T>(&mut self, values: &mut Vec<T>, value: T, span: m::Span) -> Result<(), Error> {
        if values.len() == values.capacity() {
            let capacity = values.capacity().saturating_mul(2).max(4);
            self.budget
                .allocate::<T>(capacity)
                .map_err(|e| self.error(Code::Resource, Reason::Budget(e.kind), span))?;
            values
                .try_reserve_exact(capacity - values.len())
                .map_err(|_| {
                    self.error(
                        Code::Resource,
                        Reason::Budget(m::ErrorKind::AllocationFailed),
                        span,
                    )
                })?;
        }
        values.push(value);
        Ok(())
    }
    fn vector<T>(&mut self, n: usize, span: m::Span) -> Result<Vec<T>, Error> {
        self.budget
            .vector(n)
            .map_err(|e| self.error(Code::Resource, Reason::Budget(e.kind), span))
    }
    fn text(&mut self, value: &str, span: m::Span) -> Result<String, Error> {
        self.cap(
            value.len(),
            self.limits.string_bytes,
            LimitKind::StringBytes,
            span,
        )?;
        self.charge(value.len(), span)?;
        self.budget
            .allocate::<u8>(value.len())
            .map_err(|e| self.error(Code::Resource, Reason::Budget(e.kind), span))?;
        let mut out = String::new();
        out.try_reserve_exact(value.len()).map_err(|_| {
            self.error(
                Code::Resource,
                Reason::Budget(m::ErrorKind::AllocationFailed),
                span,
            )
        })?;
        out.push_str(value);
        Ok(out)
    }
    fn count(&mut self, n: usize, kind: LimitKind, span: m::Span) -> Result<(), Error> {
        let (old, max) = match kind {
            LimitKind::Declarations => (self.declarations, self.limits.declarations),
            LimitKind::Elements => (self.elements, self.limits.elements),
            _ => unreachable!(),
        };
        let total = old.checked_add(n).ok_or_else(|| {
            self.error(
                Code::Resource,
                Reason::Budget(m::ErrorKind::ArithmeticOverflow),
                span,
            )
        })?;
        self.cap(total, max, kind, span)?;
        match kind {
            LimitKind::Declarations => self.declarations = total,
            LimitKind::Elements => self.elements = total,
            _ => unreachable!(),
        };
        Ok(())
    }
    fn sorting(&mut self, n: usize, key_bytes: usize, span: m::Span) -> Result<(), Error> {
        let comparisons = (usize::BITS - n.saturating_sub(1).leading_zeros()) as usize + 1;
        let work = n
            .checked_mul(comparisons)
            .and_then(|n| n.checked_mul(key_bytes.saturating_add(1)))
            .ok_or_else(|| {
                self.error(
                    Code::Resource,
                    Reason::Budget(m::ErrorKind::ArithmeticOverflow),
                    span,
                )
            })?;
        self.charge(work, span)
    }
    fn lookup(&mut self, n: usize, key_bytes: usize, span: m::Span) -> Result<(), Error> {
        let comparisons = (usize::BITS - n.saturating_sub(1).leading_zeros()) as usize + 1;
        let work = comparisons
            .checked_mul(key_bytes.saturating_add(1))
            .ok_or_else(|| {
                self.error(
                    Code::Resource,
                    Reason::Budget(m::ErrorKind::ArithmeticOverflow),
                    span,
                )
            })?;
        self.charge(work, span)
    }
}
