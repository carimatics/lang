//! Bounded rich-contract selection. A metadata plan conveys no runtime authority.
use super::{
    Contract, Error as WireError, ErrorKind as WireErrorKind, LimitKind, Session, Signature,
    contract,
};
use crate::component::{
    CapabilityGrants, idl,
    negotiation::{InterfaceKey, PrereleaseIdentifier, SemanticVersion},
    serialization::VersionedDigest,
};
use lang_mir::unit as m;
use std::{
    cmp::Ordering,
    fmt::{self, Write},
};

/// A claimed immutable identity. Execution must separately retain/validate code.
#[derive(Clone, Copy, Debug)]
pub struct Provider<'a> {
    pub contract: &'a Contract,
    pub digest: &'a VersionedDigest,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemberSelection {
    pub expected: usize,
    pub actual: usize,
}
#[derive(Debug)]
pub struct Selection<'a> {
    consumer: &'a Contract,
    import: usize,
    provider: Provider<'a>,
    provider_index: usize,
    consumer_interface: usize,
    provider_interface: usize,
    members: Vec<MemberSelection>,
    locked: bool,
}
impl<'a> Selection<'a> {
    pub fn consumer(&self) -> &'a Contract {
        self.consumer
    }
    pub const fn import(&self) -> usize {
        self.import
    }
    pub const fn provider(&self) -> Provider<'a> {
        self.provider
    }
    pub const fn provider_index(&self) -> usize {
        self.provider_index
    }
    pub const fn consumer_interface(&self) -> usize {
        self.consumer_interface
    }
    pub const fn provider_interface(&self) -> usize {
        self.provider_interface
    }
    pub fn members(&self) -> &[MemberSelection] {
        &self.members
    }
    pub const fn is_locked(&self) -> bool {
        self.locked
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionErrorKind {
    InvalidImport,
    MissingCapability,
    LockedVersionOutsideRequirement,
    LockedProviderUnavailable,
    LockDigestMismatch,
    LockedProviderIncompatible,
    NoCompatibleVersion,
    PrereleaseRejected,
    StructuralMismatch,
    DuplicateProvider,
    AmbiguousProvider,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMismatch {
    MissingMember,
    Capability,
    Effect,
    Signature,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rejection {
    pub provider: usize,
    pub member: usize,
    pub kind: SelectionMismatch,
    pub expected: m::Span,
    pub actual: Option<m::Span>,
}
#[derive(Debug)]
pub enum SelectionError {
    Wire(WireError),
    Metadata {
        kind: SelectionErrorKind,
        import: usize,
        span: Option<m::Span>,
        candidates: Vec<usize>,
        rejections: Vec<Rejection>,
    },
}
impl From<WireError> for SelectionError {
    fn from(e: WireError) -> Self {
        Self::Wire(e)
    }
}
impl fmt::Display for SelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "component negotiation failed: {self:?}")
    }
}
impl std::error::Error for SelectionError {}
type Result<T> = std::result::Result<T, SelectionError>;
fn error(kind: SelectionErrorKind, import: usize, span: Option<m::Span>) -> SelectionError {
    SelectionError::Metadata {
        kind,
        import,
        span,
        candidates: Vec::new(),
        rejections: Vec::new(),
    }
}
fn version_capacity(v: &SemanticVersion, s: &mut Session) -> Result<usize> {
    let count = v
        .prerelease()
        .len()
        .checked_add(v.build().len())
        .ok_or_else(|| s.error(0, WireErrorKind::Budget(m::ErrorKind::ArithmeticOverflow)))?;
    s.count(count, 0)?;
    let mut n = 62usize;
    for text in v
        .prerelease()
        .iter()
        .map(|p| match p {
            PrereleaseIdentifier::Numeric(t) | PrereleaseIdentifier::AlphaNumeric(t) => t.as_str(),
        })
        .chain(v.build().iter().map(|b| b.as_str()))
    {
        s.cap(text.len(), s.limits.string_bytes, LimitKind::StringBytes, 0)?;
        s.charge(text.len().saturating_add(1), 0)?;
        n = n
            .checked_add(text.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| s.error(0, WireErrorKind::Budget(m::ErrorKind::ArithmeticOverflow)))?;
    }
    Ok(n)
}
fn requirement_cost(
    r: &crate::component::negotiation::VersionRequirement,
    s: &mut Session,
) -> Result<usize> {
    use crate::component::negotiation::VersionRequirement as R;
    let n = match r {
        R::Exact(v) => version_capacity(v, s)?,
        R::Interval {
            min_inclusive,
            max_exclusive,
            ..
        } => {
            version_capacity(min_inclusive, s)?.saturating_add(version_capacity(max_exclusive, s)?)
        }
    };
    Ok(n.saturating_mul(4))
}
fn version_text(v: &SemanticVersion, s: &mut Session) -> Result<String> {
    let n = version_capacity(v, s)?;
    s.charge(n, 0)?;
    s.budget.bytes(n).map_err(|e| Session::budget_error(0, e))?;
    let mut text = String::new();
    text.try_reserve_exact(n)
        .map_err(|_| s.error(0, WireErrorKind::Budget(m::ErrorKind::AllocationFailed)))?;
    write!(&mut text, "{v}").expect("bounded string has sufficient capacity");
    Ok(text)
}
fn interface(c: &Contract, key: &InterfaceKey, s: &mut Session) -> Result<usize> {
    contract::lookup(
        s,
        c.document().interfaces.len(),
        contract::interface_bytes(key),
        0,
    )?;
    c.document()
        .interfaces
        .binary_search_by(|i| i.key.value.cmp(key))
        .map_err(|_| s.error(0, WireErrorKind::Unresolved).into())
}
fn function(i: &idl::Interface, name: &str, s: &mut Session) -> Result<Option<usize>> {
    contract::lookup(s, i.functions.len(), name.len(), 0)?;
    Ok(i.functions
        .binary_search_by(|f| f.name.value.as_str().cmp(name))
        .ok())
}
struct Required {
    member: usize,
    bytes: Vec<u8>,
}
struct Entry {
    provider: usize,
    interface: Option<usize>,
    version: String,
}
fn comparison_cost(text: &str) -> usize {
    text.len()
        .saturating_add(1)
        .saturating_mul(4)
        .saturating_add(64)
}
fn compare(a: &Entry, b: &Entry, providers: &[Provider<'_>], s: &mut Session) -> Result<Ordering> {
    let va = &providers[a.provider].contract.document().version.value;
    let vb = &providers[b.provider].contract.document().version.value;
    s.charge(
        comparison_cost(&a.version)
            .saturating_add(comparison_cost(&b.version))
            .saturating_add(a.version.len())
            .saturating_add(b.version.len())
            .saturating_add(33),
        0,
    )?;
    Ok(vb
        .precedence_cmp(va)
        .then_with(|| a.version.cmp(&b.version))
        .then_with(|| {
            providers[a.provider]
                .digest
                .as_bytes()
                .cmp(providers[b.provider].digest.as_bytes())
        }))
}
fn project(
    consumer: &Contract,
    ci: usize,
    required: &[Required],
    provider: Provider<'_>,
    pi: usize,
    index: usize,
    s: &mut Session,
) -> Result<std::result::Result<Vec<MemberSelection>, Rejection>> {
    let expected = &consumer.document().interfaces[ci];
    let actual = &provider.contract.document().interfaces[pi];
    let mut members = s.vector(required.len(), 0)?;
    for r in required {
        s.charge(1, 0)?;
        let f = &expected.functions[r.member];
        let Some(a) = function(actual, &f.name.value, s)? else {
            return Ok(Err(Rejection {
                provider: index,
                member: r.member,
                kind: SelectionMismatch::MissingMember,
                expected: f.span,
                actual: None,
            }));
        };
        let g = &actual.functions[a];
        s.charge(
            f.capability
                .value
                .len()
                .saturating_add(g.capability.value.len())
                .saturating_add(1),
            0,
        )?;
        let mismatch = if f.capability.value != g.capability.value {
            Some(SelectionMismatch::Capability)
        } else if f.effect.value != g.effect.value {
            Some(SelectionMismatch::Effect)
        } else {
            let sig = Signature::new(provider.contract, &actual.key.value, &g.name.value, s)?;
            let bytes = sig.preimage(s)?;
            s.charge(r.bytes.len().max(bytes.len()).saturating_add(1), 0)?;
            (r.bytes != bytes).then_some(SelectionMismatch::Signature)
        };
        if let Some(kind) = mismatch {
            return Ok(Err(Rejection {
                provider: index,
                member: r.member,
                kind,
                expected: f.span,
                actual: Some(g.span),
            }));
        }
        members.push(MemberSelection {
            expected: r.member,
            actual: a,
        });
    }
    Ok(Ok(members))
}

/// Select one exported provider for one checked import. Grants are checked before
/// provider metadata inspection. All preparation consumes the caller's session.
pub fn select_provider<'a>(
    consumer: &'a Contract,
    import: usize,
    providers: &[Provider<'a>],
    grants: &CapabilityGrants,
    s: &mut Session,
) -> Result<Selection<'a>> {
    select(consumer, import, providers, Some(grants), s)
}
/// Only the whole-unit typed-grant gate may omit the legacy capability-name
/// check. This remains metadata selection and publishes no live implementation.
pub(in crate::component) fn select_granted_provider<'a>(
    consumer: &'a Contract,
    import: usize,
    providers: &[Provider<'a>],
    s: &mut Session,
) -> Result<Selection<'a>> {
    select(consumer, import, providers, None, s)
}
fn select<'a>(
    consumer: &'a Contract,
    import: usize,
    providers: &[Provider<'a>],
    grants: Option<&CapabilityGrants>,
    s: &mut Session,
) -> Result<Selection<'a>> {
    admit(consumer, s)?;
    let Some(request) = consumer.document().imports.get(import) else {
        return Err(error(SelectionErrorKind::InvalidImport, import, None));
    };
    let ci = interface(consumer, &request.interface.value, s)?;
    let iface = &consumer.document().interfaces[ci];
    s.cap(
        grants.map_or(0, |g| g.capabilities.len()),
        s.limits.metadata.declarations,
        LimitKind::Declarations,
        0,
    )?;
    let mut required = s.vector(request.members.len(), 0)?;
    for name in &request.members {
        let member = function(iface, &name.value, s)?
            .ok_or_else(|| s.error(0, WireErrorKind::Unresolved))?;
        let f = &iface.functions[member];
        let mut found = grants.is_none();
        for grant in grants.into_iter().flat_map(|g| &g.capabilities) {
            s.cap(
                grant.len(),
                s.limits.string_bytes,
                LimitKind::StringBytes,
                0,
            )?;
            s.charge(
                grant
                    .len()
                    .saturating_add(f.capability.value.len())
                    .saturating_add(1),
                0,
            )?;
            found |= grant == &f.capability.value;
        }
        if !found {
            return Err(error(
                SelectionErrorKind::MissingCapability,
                import,
                Some(f.capability.span),
            ));
        }
        let sig = Signature::new(consumer, &iface.key.value, &f.name.value, s)?;
        let bytes = sig.preimage(s)?;
        required.push(Required { member, bytes });
    }
    s.cap(
        providers.len(),
        s.limits.metadata.declarations,
        LimitKind::Declarations,
        0,
    )?;
    s.count(providers.len(), 0)?;
    let mut entries = s.vector(providers.len(), 0)?;
    let mut costs = s.vector(providers.len(), 0)?;
    for (index, p) in providers.iter().enumerate() {
        admit(p.contract, s)?;
        let doc = p.contract.document();
        contract::lookup(
            s,
            doc.exports.len(),
            contract::interface_bytes(&request.interface.value),
            0,
        )?;
        let exported = doc
            .exports
            .binary_search_by(|e| e.value.cmp(&request.interface.value))
            .is_ok();
        let pi = if exported {
            Some(interface(p.contract, &request.interface.value, s)?)
        } else {
            None
        };
        let version = version_text(&doc.version.value, s)?;
        costs.push(comparison_cost(&version));
        entries.push(Entry {
            provider: index,
            interface: pi,
            version,
        });
    }
    // Explicit metering permits early refusal during sorting, without hidden
    // uncharged comparisons or work after an exhausted comparator budget.
    for i in 1..entries.len() {
        let mut j = i;
        while j > 0 {
            if compare(&entries[j - 1], &entries[j], providers, s)? != Ordering::Greater {
                break;
            }
            entries.swap(j - 1, j);
            j -= 1;
        }
    }
    contract::lookup(
        s,
        consumer.locks().len(),
        contract::interface_bytes(&request.interface.value),
        0,
    )?;
    let lock = consumer
        .locks()
        .binary_search_by(|l| l.key().cmp(&request.interface.value))
        .ok()
        .map(|i| &consumer.locks()[i]);
    let make = |index, pi, members, locked| Selection {
        consumer,
        import,
        provider: providers[index],
        provider_index: index,
        consumer_interface: ci,
        provider_interface: pi,
        members,
        locked,
    };
    let rb = requirement_cost(&request.requirement.value, s)?;
    if let Some(lock) = lock {
        let lock_cost = version_capacity(lock.version(), s)?.saturating_mul(4);
        s.charge(rb.saturating_add(lock_cost), 0)?;
        if !request.requirement.value.matches(lock.version()) {
            return Err(error(
                SelectionErrorKind::LockedVersionOutsideRequirement,
                import,
                Some(request.requirement.span),
            ));
        }
        let mut matching = s.vector(0, 0)?;
        let mut same = false;
        for e in &entries {
            if e.interface.is_none() {
                continue;
            }
            let p = providers[e.provider];
            s.charge(
                costs[e.provider]
                    .saturating_add(lock_cost)
                    .saturating_add(33),
                0,
            )?;
            if p.contract.document().version.value == *lock.version() {
                same = true;
                if p.digest == lock.digest() {
                    s.push(&mut matching, e.provider, 0)?;
                }
            }
        }
        if matching.len() > 1 {
            return Err(SelectionError::Metadata {
                kind: SelectionErrorKind::DuplicateProvider,
                import,
                span: Some(request.interface.span),
                candidates: matching,
                rejections: Vec::new(),
            });
        }
        let Some(&index) = matching.first() else {
            return Err(error(
                if same {
                    SelectionErrorKind::LockDigestMismatch
                } else {
                    SelectionErrorKind::LockedProviderUnavailable
                },
                import,
                Some(request.interface.span),
            ));
        };
        let pi = interface(providers[index].contract, &request.interface.value, s)?;
        return match project(consumer, ci, &required, providers[index], pi, index, s)? {
            Ok(members) => Ok(make(index, pi, members, true)),
            Err(rejection) => {
                let mut rejections = s.vector(1, 0)?;
                rejections.push(rejection);
                Err(SelectionError::Metadata {
                    kind: SelectionErrorKind::LockedProviderIncompatible,
                    import,
                    span: Some(request.interface.span),
                    candidates: matching,
                    rejections,
                })
            }
        };
    }
    let mut eligible = false;
    let mut prerelease = false;
    let mut selected = None;
    let mut group = s.vector(0, 0)?;
    let mut rejections = s.vector(0, 0)?;
    for e in &entries {
        let Some(pi) = e.interface else { continue };
        let p = providers[e.provider];
        let version = &p.contract.document().version.value;
        s.charge(rb.saturating_add(costs[e.provider]).saturating_mul(2), 0)?;
        if !request.requirement.value.matches(version) {
            prerelease |= version.is_prerelease()
                && request
                    .requirement
                    .value
                    .matches_bounds_ignoring_prerelease_policy(version);
            continue;
        }
        eligible = true;
        match project(consumer, ci, &required, p, pi, e.provider, s)? {
            Err(r) => s.push(&mut rejections, r, 0)?,
            Ok(members) => {
                if let Some((first, _, _)) = &selected {
                    let top: &Provider<'_> = &providers[*first];
                    s.charge(costs[e.provider].saturating_add(costs[*first]), 0)?;
                    if version.precedence_cmp(&top.contract.document().version.value)
                        == Ordering::Equal
                    {
                        s.push(&mut group, e.provider, 0)?;
                    }
                } else {
                    s.push(&mut group, e.provider, 0)?;
                    selected = Some((e.provider, pi, members));
                }
            }
        }
    }
    let Some((index, pi, members)) = selected else {
        return Err(SelectionError::Metadata {
            kind: if !eligible {
                if prerelease {
                    SelectionErrorKind::PrereleaseRejected
                } else {
                    SelectionErrorKind::NoCompatibleVersion
                }
            } else {
                SelectionErrorKind::StructuralMismatch
            },
            import,
            span: Some(request.interface.span),
            candidates: Vec::new(),
            rejections,
        });
    };
    if group.len() > 1 {
        let mut duplicate = false;
        for pair in group.windows(2) {
            let a = providers[pair[0]];
            let b = providers[pair[1]];
            s.charge(
                costs[pair[0]]
                    .saturating_add(costs[pair[1]])
                    .saturating_add(33),
                0,
            )?;
            duplicate |= a.contract.document().version.value == b.contract.document().version.value
                && a.digest == b.digest;
        }
        return Err(SelectionError::Metadata {
            kind: if duplicate {
                SelectionErrorKind::DuplicateProvider
            } else {
                SelectionErrorKind::AmbiguousProvider
            },
            import,
            span: Some(request.interface.span),
            candidates: group,
            rejections: Vec::new(),
        });
    }
    Ok(make(index, pi, members, false))
}
fn admit(c: &Contract, s: &mut Session) -> Result<()> {
    s.cap(
        c.locks().len(),
        s.limits.metadata.declarations,
        LimitKind::Declarations,
        0,
    )?;
    // Complete canonical re-admission applies stricter limits to unused fields
    // and locks as well as the reachable signature. Scratch/work is not refunded.
    let _ = super::encode_contract(c, s)?;
    Ok(())
}
