//! RFC 0027 production codecs. Decoded data carries no execution authority.
use lang_mir::unit as m;
use std::fmt;

pub mod artifact;
mod contract;
mod io;
mod report;
mod selection;
mod value;
pub use contract::*;
pub use report::*;
pub use selection::*;
pub use value::*;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub metadata: super::idl::Limits,
    pub bytes: usize,
    pub serialization_bytes: usize,
    pub depth: usize,
    pub elements: usize,
    pub string_bytes: usize,
    pub work: usize,
    pub allocation_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            metadata: super::idl::Limits::default(),
            bytes: 64 * 1024 * 1024,
            serialization_bytes: 64 * 1024 * 1024,
            depth: 128,
            elements: 1_000_000,
            string_bytes: 64 * 1024 * 1024,
            work: 1_000_000,
            allocation_bytes: 128 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Bytes,
    SerializationBytes,
    Depth,
    Elements,
    StringBytes,
    Declarations,
    MetadataElements,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Truncated,
    Magic,
    Version,
    Kind,
    Tag,
    Utf8,
    Trailing,
    InvalidReport,
    InvalidSchema,
    Noncanonical,
    Unresolved,
    Signature,
    ResourceContextRequired,
    ValueType,
    Limit {
        kind: LimitKind,
        limit: usize,
        requested: usize,
    },
    Budget(m::ErrorKind),
    Declaration(super::idl::Error),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub offset: usize,
    pub kind: ErrorKind,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "component wire at {}: {:?}", self.offset, self.kind)
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    pub work: usize,
    pub allocation_bytes: usize,
    pub elements: usize,
    pub serialization_bytes: usize,
}
/// Reuse one session across codec operations; failures never refund accounting.
pub struct Session {
    limits: Limits,
    budget: m::Budget,
    elements: usize,
    serialized: usize,
}
impl Session {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            budget: m::Budget::new(m::Limits {
                work: limits.work,
                allocations: limits.allocation_bytes,
                elements: usize::MAX,
                ..Default::default()
            }),
            elements: 0,
            serialized: 0,
        }
    }
    /// Continue preparation on the MIR/metadata loader's existing ledger.
    pub fn with_usage(limits: Limits, usage: m::Usage) -> Result<Self, Error> {
        let mut value = Self::new(limits);
        value.budget = m::Budget::with_usage(
            m::Limits {
                work: limits.work,
                allocations: limits.allocation_bytes,
                elements: limits.elements,
                ..Default::default()
            },
            usage,
        )
        .map_err(|e| Self::budget_error(0, e))?;
        value.elements = usage.elements;
        Ok(value)
    }
    pub fn preparation_usage(&self) -> m::Usage {
        let mut usage = self.budget_usage();
        usage.elements = usage.elements.max(self.elements);
        usage
    }
    /// Advance the same preparation history after MIR/binding verification.
    pub fn advance(&mut self, usage: m::Usage) -> Result<(), Error> {
        let previous = self.preparation_usage();
        if usage.work < previous.work
            || usage.allocation_bytes < previous.allocation_bytes
            || usage.elements < previous.elements
        {
            return Err(self.error(0, ErrorKind::InvalidSchema));
        }
        self.budget = m::Budget::with_usage(self.budget.limits(), usage)
            .map_err(|e| Self::budget_error(0, e))?;
        self.elements = self.elements.max(usage.elements);
        Ok(())
    }
    pub fn usage(&self) -> Usage {
        Usage {
            work: self.budget.work_used(),
            allocation_bytes: self.budget.allocation_bytes(),
            elements: self.elements,
            serialization_bytes: self.serialized,
        }
    }
    fn budget_usage(&self) -> m::Usage {
        m::Usage {
            work: self.budget.work_used(),
            allocation_bytes: self.budget.allocation_bytes(),
            elements: self.budget.elements_used(),
        }
    }
    fn metadata<T>(
        &mut self,
        f: impl FnOnce(&mut Self, super::idl::Limits) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let saved = self.limits;
        let limits = saved.metadata;
        let outer = self.budget.limits();
        let usage = self.budget_usage();
        let inner = m::Limits {
            work: outer.work.min(usage.work.saturating_add(limits.work)),
            allocations: outer.allocations.min(
                usage
                    .allocation_bytes
                    .saturating_add(limits.allocation_bytes),
            ),
            ..outer
        };
        self.budget = m::Budget::with_usage(inner, usage).map_err(|e| Self::budget_error(0, e))?;
        self.limits.bytes = self.limits.bytes.min(limits.source_bytes);
        self.limits.string_bytes = self.limits.string_bytes.min(limits.string_bytes);
        self.limits.depth = self.limits.depth.min(limits.depth);
        let result = f(self, limits);
        self.budget = m::Budget::with_usage(outer, self.budget_usage())
            .expect("nested limits never exceed parent");
        self.limits = saved;
        result
    }
    fn error(&self, offset: usize, kind: ErrorKind) -> Error {
        Error { offset, kind }
    }
    pub(super) fn value_bytes(&self, count: usize, at: usize) -> Result<(), Error> {
        self.cap(count, self.limits.bytes, LimitKind::Bytes, at)
    }
    pub(super) fn text_bytes(&self, count: usize, at: usize) -> Result<(), Error> {
        self.value_bytes(count, at)?;
        self.cap(count, self.limits.string_bytes, LimitKind::StringBytes, at)
    }
    fn budget_error(offset: usize, error: m::Error) -> Error {
        Error {
            offset,
            kind: ErrorKind::Budget(error.kind),
        }
    }
    pub(super) fn charge(&mut self, n: usize, at: usize) -> Result<(), Error> {
        self.budget.charge(n).map_err(|e| Self::budget_error(at, e))
    }
    fn cap(&self, n: usize, max: usize, kind: LimitKind, at: usize) -> Result<(), Error> {
        if n > max {
            Err(self.error(
                at,
                ErrorKind::Limit {
                    kind,
                    limit: max,
                    requested: n,
                },
            ))
        } else {
            Ok(())
        }
    }
    fn add(old: usize, n: usize, at: usize) -> Result<usize, Error> {
        old.checked_add(n).ok_or(Error {
            offset: at,
            kind: ErrorKind::Budget(m::ErrorKind::ArithmeticOverflow),
        })
    }
    fn multiply(a: usize, b: usize, at: usize) -> Result<usize, Error> {
        a.checked_mul(b).ok_or(Error {
            offset: at,
            kind: ErrorKind::Budget(m::ErrorKind::ArithmeticOverflow),
        })
    }
    pub(super) fn count(&mut self, n: usize, at: usize) -> Result<(), Error> {
        let total = Self::add(self.elements, n, at)?;
        self.cap(total, self.limits.elements, LimitKind::Elements, at)?;
        self.charge(n, at)?;
        self.elements = total;
        Ok(())
    }
    fn serialized(&mut self, n: usize, at: usize) -> Result<(), Error> {
        let total = Self::add(self.serialized, n, at)?;
        self.cap(
            total,
            self.limits.serialization_bytes,
            LimitKind::SerializationBytes,
            at,
        )?;
        self.serialized = total;
        Ok(())
    }
    pub(super) fn vector<T>(&mut self, n: usize, at: usize) -> Result<Vec<T>, Error> {
        self.budget.vector(n).map_err(|e| Self::budget_error(at, e))
    }
    pub(super) fn push<T>(&mut self, v: &mut Vec<T>, value: T, at: usize) -> Result<(), Error> {
        if v.len() == v.capacity() {
            let capacity = v.capacity().saturating_mul(2).max(4);
            self.budget
                .allocate::<T>(capacity)
                .map_err(|e| Self::budget_error(at, e))?;
            v.try_reserve_exact(capacity - v.len())
                .map_err(|_| self.error(at, ErrorKind::Budget(m::ErrorKind::AllocationFailed)))?;
        }
        v.push(value);
        Ok(())
    }
}
