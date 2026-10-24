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
    WorkContext,
    Execution(crate::execution::Failure),
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
    service_work: Option<crate::execution::ServiceWork>,
}
/// One synchronous loan of the original preparation ledger. The Session keeps
/// its local cumulative allowance; the caller ledger keeps global history.
pub struct PreparationScope<'s> {
    session: &'s mut Session,
    original: &'s mut m::Budget,
    original_limits: m::Limits,
    initial: m::Usage,
    local_budget: Option<m::Budget>,
    local_elements: usize,
    local_limits: Limits,
}
impl std::ops::Deref for PreparationScope<'_> {
    type Target = Session;
    fn deref(&self) -> &Self::Target {
        self.session
    }
}
impl std::ops::DerefMut for PreparationScope<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.session
    }
}
impl Drop for PreparationScope<'_> {
    fn drop(&mut self) {
        let usage = self.session.preparation_usage();
        let difference = m::Usage {
            work: usage
                .work
                .checked_sub(self.initial.work)
                .expect("preparation history is monotonic"),
            allocation_bytes: usage
                .allocation_bytes
                .checked_sub(self.initial.allocation_bytes)
                .expect("preparation history is monotonic"),
            elements: usage
                .elements
                .checked_sub(self.initial.elements)
                .expect("preparation history is monotonic"),
        };
        let local = self
            .local_budget
            .take()
            .expect("one preparation scope exit");
        let local_usage = m::Usage {
            work: local
                .work_used()
                .checked_add(difference.work)
                .expect("scope admitted local allowance"),
            allocation_bytes: local
                .allocation_bytes()
                .checked_add(difference.allocation_bytes)
                .expect("scope admitted local allowance"),
            elements: local
                .elements_used()
                .checked_add(difference.elements)
                .expect("scope admitted local allowance"),
        };
        self.session.elements = self
            .local_elements
            .checked_add(
                self.session
                    .elements
                    .checked_sub(self.initial.elements)
                    .expect("data history is monotonic"),
            )
            .expect("scope admitted element allowance");
        self.session.budget =
            m::Budget::with_usage(local.limits(), local_usage).expect("scope retains local limits");
        self.session.limits = self.local_limits;
        *self.original = m::Budget::with_usage(self.original_limits, usage)
            .expect("scope retains original limits");
    }
}
/// Restore the previous association on every exit, including Rust unwinding.
pub(crate) struct WorkScope<'s> {
    session: &'s mut Session,
    previous: Option<crate::execution::ServiceWork>,
}
impl std::ops::Deref for WorkScope<'_> {
    type Target = Session;
    fn deref(&self) -> &Self::Target {
        self.session
    }
}
impl std::ops::DerefMut for WorkScope<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.session
    }
}
impl Drop for WorkScope<'_> {
    fn drop(&mut self) {
        self.session.service_work = self.previous.take();
    }
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
            service_work: None,
        }
    }
    /// Internal metadata prepared during the active canonical phase uses the
    /// same ledger checkpoint as its wire operations.
    pub(super) fn preparation_budget(&mut self) -> &mut m::Budget {
        &mut self.budget
    }
    /// Lend the original preparation ledger while retaining this Session's
    /// cumulative allowance. Charges survive every exit, including unwind.
    pub fn scope_preparation<'s>(
        &'s mut self,
        original: &'s mut m::Budget,
    ) -> Result<PreparationScope<'s>, Error> {
        let initial = m::Usage {
            work: original.work_used(),
            allocation_bytes: original.allocation_bytes(),
            elements: original.elements_used(),
        };
        let original_limits = original.limits();
        let local_limits = self.limits;
        let local_elements = self.elements;
        let local = self.budget.limits();
        let ceiling = |used: usize, maximum: usize, spent: usize| {
            used.saturating_add(maximum.saturating_sub(spent))
        };
        let mut limits = original_limits;
        limits.work = limits
            .work
            .min(ceiling(initial.work, local.work, self.budget.work_used()));
        limits.allocations = limits.allocations.min(ceiling(
            initial.allocation_bytes,
            local.allocations,
            self.budget.allocation_bytes(),
        ));
        limits.elements = limits
            .elements
            .min(ceiling(
                initial.elements,
                local.elements,
                self.budget.elements_used(),
            ))
            .min(ceiling(
                initial.elements,
                local_limits.elements,
                local_elements,
            ));
        let borrowed =
            m::Budget::with_usage(limits, initial).map_err(|error| Self::budget_error(0, error))?;
        let local_budget = std::mem::replace(&mut self.budget, borrowed);
        self.limits.elements = limits.elements;
        self.elements = initial.elements;
        Ok(PreparationScope {
            session: self,
            original,
            original_limits,
            initial,
            local_budget: Some(local_budget),
            local_elements,
            local_limits,
        })
    }
    pub(crate) fn scope_work<'s>(
        &'s mut self,
        work: &crate::execution::ServiceWork,
    ) -> Result<WorkScope<'s>, Error> {
        if self
            .service_work
            .as_ref()
            .is_some_and(|old| !old.same_invocation(work))
        {
            return Err(self.error(0, ErrorKind::WorkContext));
        }
        let previous = self.service_work.replace(work.clone());
        Ok(WorkScope {
            session: self,
            previous,
        })
    }
    /// One value observation/copy consumes both applicable quotas atomically.
    pub(super) fn value_work(&mut self, n: usize, at: usize) -> Result<(), Error> {
        self.value_work_cost(n, n, at)
    }
    pub(super) fn text_work(&mut self, n: usize, at: usize) -> Result<(), Error> {
        self.value_work_cost(n, Self::add(n, n, at)?, at)
    }
    fn value_work_cost(
        &mut self,
        preparation: usize,
        service: usize,
        at: usize,
    ) -> Result<(), Error> {
        let mut budget = m::Budget::with_usage(self.budget.limits(), self.budget_usage())
            .map_err(|e| Self::budget_error(at, e))?;
        budget
            .charge(preparation)
            .map_err(|e| Self::budget_error(at, e))?;
        self.payload_work(service, at)?;
        self.budget = budget;
        Ok(())
    }
    pub(super) fn payload_work(&self, n: usize, at: usize) -> Result<(), Error> {
        if let Some(work) = &self.service_work {
            let n = u64::try_from(n).map_err(|_| {
                self.error(
                    at,
                    ErrorKind::Execution(crate::execution::Failure::CounterOverflow),
                )
            })?;
            work.charge(n)
                .map_err(|e| self.error(at, ErrorKind::Execution(e)))?;
        }
        Ok(())
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
    /// Charge cumulative preparation work; a refused operation does not refund it.
    pub fn charge(&mut self, n: usize, at: usize) -> Result<(), Error> {
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
    /// Admit cumulative canonical elements before traversing a native array.
    pub fn count(&mut self, n: usize, at: usize) -> Result<(), Error> {
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
    /// Admit retained metadata storage before its allocation or ownership transfer.
    pub fn retained<T>(&mut self, n: usize, at: usize) -> Result<(), Error> {
        self.budget
            .allocate::<T>(n)
            .map_err(|e| Self::budget_error(at, e))
    }
    /// Admit payload/reference-count storage before invoking the owner factory.
    pub fn shared<T>(
        &mut self,
        value: impl FnOnce() -> T,
        at: usize,
    ) -> Result<std::rc::Rc<T>, Error> {
        // Preparation storage model: retained payload plus two reference counts.
        // All fallible admission precedes ownership acquisition in the factory.
        self.retained::<T>(1, at)?;
        self.retained::<usize>(2, at)?;
        Ok(std::rc::Rc::new(value()))
    }
    /// Admit and reserve a bounded metadata vector before populating it.
    pub fn vector<T>(&mut self, n: usize, at: usize) -> Result<Vec<T>, Error> {
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

#[cfg(test)]
mod work_tests {
    use super::*;
    use crate::{
        component::{
            idl,
            negotiation::{InterfaceKey, PackageKey},
        },
        execution as e,
    };

    fn contract() -> Contract {
        let document = idl::check(
            m::SourceId(0),
            br#"{
            "idl":1,"package":"demo:work","version":"1.0.0",
            "interfaces":[{"key":"demo:work/api","resources":[],"types":[],
            "functions":[{"name":"echo","effect":"sync","params":[{"name":"text","type":"text"}],
            "result":"text","capability":"demo.work.echo"}]}],
            "imports":[],"exports":["demo:work/api"]
        }"#,
            Default::default(),
        )
        .unwrap();
        Contract::new(document, vec![], &mut Session::new(Default::default())).unwrap()
    }
    fn signature(contract: &Contract) -> Signature<'_> {
        Signature::new(
            contract,
            &InterfaceKey::new(PackageKey::new("demo", "work"), "api"),
            "echo",
            &mut Session::new(Default::default()),
        )
        .unwrap()
    }
    fn values() -> Values {
        Values {
            roots: vec![ValueId(0)],
            nodes: vec![Value::Text("寿司".into())],
        }
    }
    #[test]
    fn canonical_text_shares_observation_copy_and_utf8_work_without_changing_bytes() {
        let contract = contract();
        let signature = signature(&contract);
        let expected = encode_values(
            &signature,
            FrameKind::Parameters,
            &values(),
            &mut Session::new(Default::default()),
        )
        .unwrap();
        for limit in [20, 19] {
            let work = e::ServiceWork::new(Some(limit));
            let mut session = Session::new(Default::default());
            {
                let mut scope = session.scope_work(&work).unwrap();
                let bytes = encode_values(&signature, FrameKind::Parameters, &values(), &mut scope)
                    .unwrap();
                assert_eq!(bytes, expected);
                assert_eq!(work.used(), 7); // One value and six copied UTF-8 bytes.
                let decoded = decode_values(&signature, FrameKind::Parameters, &bytes, &mut scope);
                if limit == 20 {
                    assert_eq!(decoded.unwrap(), values());
                    assert_eq!(work.used(), 20); // One value, six copied and six validated bytes.
                } else {
                    assert!(matches!(
                        decoded.err().unwrap().kind,
                        ErrorKind::Execution(e::Failure::ServiceWorkLimit {
                            limit: 19,
                            used: 8,
                            requested: 12
                        })
                    ));
                    assert_eq!(work.used(), 8);
                }
            }
            assert!(session.service_work.is_none());
        }
    }
    #[test]
    fn local_quota_precedes_global_refusal_and_overflow_keeps_original_control_identity() {
        let contract = contract();
        let signature = signature(&contract);
        let mut session = Session::new(Limits {
            work: 0,
            ..Default::default()
        });
        let work = e::ServiceWork::new(Some(0));
        let error = encode_values(
            &signature,
            FrameKind::Parameters,
            &values(),
            &mut session.scope_work(&work).unwrap(),
        )
        .err()
        .unwrap();
        assert!(matches!(error.kind, ErrorKind::Budget(_)));
        assert_eq!(work.used(), 0);
        let work = e::ServiceWork::new(None);
        work.charge(u64::MAX).unwrap();
        let mut session = Session::new(Default::default());
        let error = encode_values(
            &signature,
            FrameKind::Parameters,
            &values(),
            &mut session.scope_work(&work).unwrap(),
        )
        .err()
        .unwrap();
        assert!(matches!(
            crate::task::runner::Reason::from(error),
            crate::task::runner::Reason::Control(e::Failure::CounterOverflow)
        ));
        assert_eq!(work.used(), u64::MAX);
    }
    #[test]
    fn session_scope_refuses_foreign_counters_and_restores_association_on_unwind() {
        let a = e::ServiceWork::new(None);
        let b = e::ServiceWork::new(None);
        let mut session = Session::new(Default::default());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut root = session.scope_work(&a).unwrap();
            assert!(matches!(
                root.scope_work(&b).err().unwrap().kind,
                ErrorKind::WorkContext
            ));
            let child = root.scope_work(&a).unwrap();
            child.payload_work(1, 0).unwrap();
            panic!("test unwind after a shared nested scope");
        }));
        assert!(result.is_err());
        assert_eq!(a.used(), 1);
        assert_eq!(b.used(), 0);
        assert!(session.service_work.is_none());
        assert!(session.scope_work(&b).is_ok());
    }

    #[test]
    fn zero_byte_unit_elements_still_consume_each_value_observation() {
        let document = idl::check(m::SourceId(0), br#"{
            "idl":1,"package":"demo:work","version":"1.0.0",
            "interfaces":[{"key":"demo:work/api","resources":[],"types":[],
            "functions":[{"name":"echo","effect":"sync","params":[{"name":"values","type":{"list":"unit"}}],
            "result":"unit","capability":"demo.work.echo"}]}],
            "imports":[],"exports":["demo:work/api"]
        }"#, Default::default()).unwrap();
        let contract =
            Contract::new(document, vec![], &mut Session::new(Default::default())).unwrap();
        let signature = signature(&contract);
        let mut nodes = vec![Value::Unit; 5];
        nodes.push(Value::List((0..5).map(ValueId).collect()));
        let values = Values {
            roots: vec![ValueId(5)],
            nodes,
        };
        let work = e::ServiceWork::new(Some(12));
        let mut session = Session::new(Default::default());
        let mut scope = session.scope_work(&work).unwrap();
        let bytes = encode_values(&signature, FrameKind::Parameters, &values, &mut scope).unwrap();
        assert_eq!(bytes.len(), 49);
        assert_eq!(work.used(), 6);
        assert_eq!(
            decode_values(&signature, FrameKind::Parameters, &bytes, &mut scope).unwrap(),
            values
        );
        assert_eq!(work.used(), 12);
    }
}
#[cfg(test)]
mod preparation_scope_tests {
    use super::*;
    fn usage(budget: &m::Budget) -> m::Usage {
        m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        }
    }
    #[test]
    fn original_and_local_allowances_survive_repeated_refusal() {
        for original_first in [false, true] {
            let mut session = Session::new(Limits {
                work: if original_first { 100 } else { 5 },
                ..Default::default()
            });
            session.value_work(3, 0).unwrap();
            let mut original = m::Budget::with_usage(
                m::Limits {
                    work: if original_first { 9 } else { 100 },
                    ..Default::default()
                },
                m::Usage {
                    work: 7,
                    ..Default::default()
                },
            )
            .unwrap();
            let limits = original.limits();
            {
                let mut scope = session.scope_preparation(&mut original).unwrap();
                scope.value_work(2, 0).unwrap();
                assert!(scope.value_work(1, 0).is_err());
            }
            assert_eq!(original.work_used(), 9);
            assert_eq!(session.usage().work, 5);
            assert_eq!(original.limits(), limits);
            {
                let mut scope = session.scope_preparation(&mut original).unwrap();
                assert!(scope.value_work(1, 0).is_err());
            }
            assert_eq!(original.work_used(), 9);
            assert_eq!(session.usage().work, 5);
        }
    }
    #[test]
    fn allocation_elements_and_data_history_obey_both_caps() {
        let mut session = Session::new(Limits {
            allocation_bytes: 12,
            elements: 5,
            ..Default::default()
        });
        session.vector::<u32>(1, 0).unwrap();
        session.elements = 1;
        session.serialized = 7;
        let mut original = m::Budget::with_usage(
            m::Limits {
                allocations: 28,
                elements: 9,
                ..Default::default()
            },
            m::Usage {
                allocation_bytes: 20,
                elements: 7,
                ..Default::default()
            },
        )
        .unwrap();
        {
            let mut scope = session.scope_preparation(&mut original).unwrap();
            scope.vector::<u32>(2, 0).unwrap();
            assert!(scope.vector::<u8>(1, 0).is_err());
            scope.serialized += 3;
        }
        assert_eq!(original.allocation_bytes(), 28);
        assert_eq!(original.elements_used(), 9);
        assert_eq!(session.usage().allocation_bytes, 12);
        assert_eq!(session.budget.elements_used(), 3);
        assert_eq!(session.elements, 1);
        assert_eq!(session.serialized, 10);
        assert!(
            session
                .scope_preparation(&mut original)
                .unwrap()
                .vector::<()>(1, 0)
                .is_err()
        );
        assert_eq!(original.elements_used(), 9);
    }
    #[test]
    fn nested_scopes_preserve_each_original_history() {
        let mut session = Session::new(Default::default());
        let mut outer = m::Budget::new(Default::default());
        let mut inner = m::Budget::new(Default::default());
        outer.charge(11).unwrap();
        inner.charge(17).unwrap();
        {
            let mut scope = session.scope_preparation(&mut outer).unwrap();
            scope.value_work(2, 0).unwrap();
            {
                let mut nested = scope.scope_preparation(&mut inner).unwrap();
                nested.value_work(3, 0).unwrap();
            }
            scope.value_work(1, 0).unwrap();
        }
        assert_eq!(outer.work_used(), 17);
        assert_eq!(inner.work_used(), 20);
        assert_eq!(session.usage().work, 6);
    }
    #[test]
    fn accepted_error_and_unwind_prefixes_keep_limits_and_original_work() {
        for unwind in [false, true] {
            let mut session = Session::new(Default::default());
            let mut original = m::Budget::new(Default::default());
            original.charge(19).unwrap();
            let limits = original.limits();
            let work = crate::execution::ServiceWork::new(Some(20));
            {
                let mut accounted = session.scope_work(&work).unwrap();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut scope = accounted.scope_preparation(&mut original).unwrap();
                    scope.value_work(3, 0).unwrap();
                    scope.vector::<u8>(4, 0).unwrap();
                    if unwind {
                        let _: Result<(), Error> = scope.metadata(|session, _| {
                            session.value_work(2, 0)?;
                            panic!("contained preparation scope");
                        });
                    }
                    Err::<(), _>(scope.error(7, ErrorKind::Tag))
                }));
                if unwind {
                    assert!(result.is_err());
                } else {
                    assert!(result.unwrap().is_err());
                }
            }
            assert_eq!(original.work_used(), if unwind { 24 } else { 22 });
            assert_eq!(session.usage().work, if unwind { 5 } else { 3 });
            assert_eq!(original.allocation_bytes(), 4);
            assert_eq!(original.elements_used(), 4);
            assert_eq!(original.limits(), limits);
            assert_eq!(session.limits.work, Limits::default().work);
            assert!(session.service_work.is_none());
            assert_eq!(work.used(), if unwind { 5 } else { 3 });
        }
    }
    #[test]
    fn repeated_allocation_and_metadata_element_scopes_keep_each_ceiling() {
        for elements in [false, true] {
            for original_first in [false, true] {
                let limits = Limits {
                    allocation_bytes: if !elements && !original_first { 5 } else { 100 },
                    elements: if elements && !original_first { 5 } else { 100 },
                    ..Default::default()
                };
                let mut session = Session::with_usage(
                    limits,
                    m::Usage {
                        allocation_bytes: 2,
                        elements: 2,
                        ..Default::default()
                    },
                )
                .unwrap();
                let original_limits = m::Limits {
                    allocations: if !elements && original_first { 10 } else { 100 },
                    elements: if elements && original_first { 10 } else { 100 },
                    ..Default::default()
                };
                let mut original = m::Budget::with_usage(
                    original_limits,
                    m::Usage {
                        allocation_bytes: if elements { 0 } else { 7 },
                        elements: if elements { 7 } else { 0 },
                        ..Default::default()
                    },
                )
                .unwrap();
                {
                    let mut scope = session.scope_preparation(&mut original).unwrap();
                    if elements {
                        scope.vector::<()>(3, 0).unwrap();
                        assert!(scope.vector::<()>(1, 0).is_err());
                    } else {
                        scope.vector::<u8>(3, 0).unwrap();
                        assert!(scope.vector::<u8>(1, 0).is_err());
                    }
                }
                assert_eq!(
                    if elements {
                        original.elements_used()
                    } else {
                        original.allocation_bytes()
                    },
                    10
                );
                assert_eq!(
                    if elements {
                        session.budget.elements_used()
                    } else {
                        session.budget.allocation_bytes()
                    },
                    5
                );
                {
                    let mut scope = session.scope_preparation(&mut original).unwrap();
                    if elements {
                        assert!(scope.vector::<()>(1, 0).is_err());
                    } else {
                        assert!(scope.vector::<u8>(1, 0).is_err());
                    }
                }
                assert_eq!(original.limits(), original_limits);
                assert_eq!(
                    if elements {
                        original.elements_used()
                    } else {
                        original.allocation_bytes()
                    },
                    10
                );
            }
        }
    }
    #[test]
    fn overflow_refusal_never_wraps_or_refunds() {
        for dimension in 0..3 {
            let initial = m::Usage {
                work: if dimension == 0 { usize::MAX - 1 } else { 0 },
                allocation_bytes: if dimension == 1 { usize::MAX - 1 } else { 0 },
                elements: if dimension == 2 { usize::MAX - 1 } else { 0 },
            };
            let limits = m::Limits {
                work: usize::MAX,
                allocations: usize::MAX,
                elements: usize::MAX,
                ..Default::default()
            };
            let mut original = m::Budget::with_usage(limits, initial).unwrap();
            let mut session = Session::new(Default::default());
            {
                let mut scope = session.scope_preparation(&mut original).unwrap();
                if dimension == 0 {
                    scope.value_work(1, 0).unwrap();
                    assert!(scope.value_work(1, 0).is_err());
                } else if dimension == 1 {
                    scope.vector::<u8>(1, 0).unwrap();
                    assert!(scope.vector::<u8>(1, 0).is_err());
                } else {
                    scope.vector::<()>(1, 0).unwrap();
                    assert!(scope.vector::<()>(1, 0).is_err());
                }
            }
            let after = usage(&original);
            assert_eq!(
                match dimension {
                    0 => after.work,
                    1 => after.allocation_bytes,
                    _ => after.elements,
                },
                usize::MAX
            );
        }
    }
}
