//! Whole-unit typed grant admission, followed by immutable live native bindings.
//! A decoded contract, digest, or passive selection is never a callable grant.
use super::{bindings, native, wire};
use lang_mir::unit as m;
use std::fmt;

/// Native host authorization for one exact function projection. Signature is
/// the complete LCSG preimage, including effect, capability and nominal types.
#[derive(Clone, Copy, Debug)]
pub struct Grant<'a> {
    pub member: &'a bindings::MemberKey,
    pub capability: &'a str,
    pub signature: &'a [u8],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Snapshot,
    UnknownGrant,
    DuplicateGrant,
    MissingGrant,
    Capability,
    Signature,
    MissingImplementation,
}
#[derive(Debug)]
pub enum Error {
    Wire(wire::Error),
    Selection(wire::SelectionError),
    Admission {
        kind: ErrorKind,
        import: Option<m::ImportId>,
        source: Option<m::Span>,
        contract: Option<m::Span>,
        grant: Option<usize>,
    },
}
impl From<wire::Error> for Error {
    fn from(e: wire::Error) -> Self {
        Self::Wire(e)
    }
}
impl From<wire::SelectionError> for Error {
    fn from(e: wire::SelectionError) -> Self {
        Self::Selection(e)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "component import admission: {self:?}")
    }
}
impl std::error::Error for Error {}

pub struct BoundImport<'a> {
    source: m::ImportId,
    provider: &'a native::Provider<'a>,
    member: bindings::Function,
    method: &'a native::Method,
    signature: [u8; 32],
}
impl<'a> BoundImport<'a> {
    pub const fn source(&self) -> m::ImportId {
        self.source
    }
    pub const fn provider(&self) -> &'a native::Provider<'a> {
        self.provider
    }
    pub const fn member(&self) -> bindings::Function {
        self.member
    }
    pub const fn method(&self) -> &'a native::Method {
        self.method
    }
    pub const fn signature(&self) -> [u8; 32] {
        self.signature
    }
}
/// Published only after the entire grant/selection/implementation batch passes.
/// The source unit, exact metadata snapshot and actual provider owners all live
/// at least as long as these bindings. No callback executes during preparation.
pub struct Bindings<'a> {
    unit: &'a bindings::Unit,
    contract: &'a wire::Contract,
    imports: Vec<BoundImport<'a>>,
}
impl<'a> Bindings<'a> {
    pub const fn unit(&self) -> &'a bindings::Unit {
        self.unit
    }
    pub const fn contract(&self) -> &'a wire::Contract {
        self.contract
    }
    pub fn imports(&self) -> &[BoundImport<'a>] {
        &self.imports
    }
    pub fn resolve(&self, id: m::ImportId) -> Option<&BoundImport<'a>> {
        self.imports.get(id.0 as usize)
    }
}
fn error(
    unit: &bindings::Unit,
    kind: ErrorKind,
    index: Option<usize>,
    grant: Option<usize>,
) -> Error {
    Error::Admission {
        kind,
        import: index.map(|i| m::ImportId(i as u32)),
        source: index.map(|i| unit.code().declarations().imports()[i].span),
        contract: index.map(|i| {
            let f = unit.imports()[i];
            unit.document().document().interfaces[f.interface].functions[f.member].span
        }),
        grant,
    }
}
fn key_bytes(key: &super::negotiation::InterfaceKey) -> usize {
    key.package()
        .namespace()
        .len()
        .saturating_add(key.package().package().len())
        .saturating_add(key.interface().len())
        .saturating_add(1)
}

pub fn prepare<'a>(
    unit: &'a bindings::Unit,
    contract: &'a wire::Contract,
    grants: &[Grant<'_>],
    inventory: impl FnOnce() -> &'a [native::Provider<'a>],
    s: &mut wire::Session,
) -> Result<Bindings<'a>, Error> {
    // Both graphs are already independently checked and immutable. Their retained
    // preparation work plus requested storage bounds a full, nonrecursive Eq of
    // the flat graph (including every span, source identity and unused TypeId).
    for doc in [unit.document(), contract.checked()] {
        let usage = doc.usage().budget;
        s.charge(
            usage
                .work
                .saturating_add(usage.allocation_bytes)
                .saturating_add(1),
            0,
        )?;
    }
    if unit.document().document() != contract.document() {
        return Err(error(unit, ErrorKind::Snapshot, None, None));
    }
    // Re-admit the entire contract at this session's stricter limits.
    wire::encode_contract(contract, s)?;
    s.count(grants.len(), 0)?;
    s.count(unit.imports().len(), 0)?;
    let mut seen = s.vector(unit.imports().len(), 0)?;
    seen.resize(unit.imports().len(), false);
    let mut signatures = s.vector(unit.imports().len(), 0)?;
    signatures.resize(unit.imports().len(), [0; 32]);
    for (g, grant) in grants.iter().enumerate() {
        let mut found = None;
        for (index, f) in unit.imports().iter().enumerate() {
            let i = &contract.document().interfaces[f.interface];
            let f = &i.functions[f.member];
            s.charge(
                key_bytes(&grant.member.interface)
                    .saturating_add(key_bytes(&i.key.value))
                    .saturating_add(grant.member.member.len())
                    .saturating_add(f.name.value.len())
                    .saturating_add(1),
                0,
            )?;
            if grant.member.interface == i.key.value && grant.member.member == f.name.value {
                found = Some(index);
                break;
            }
        }
        let index = found.ok_or_else(|| error(unit, ErrorKind::UnknownGrant, None, Some(g)))?;
        if seen[index] {
            return Err(error(unit, ErrorKind::DuplicateGrant, Some(index), Some(g)));
        }
        let f = unit.imports()[index];
        let i = &contract.document().interfaces[f.interface];
        let f = &i.functions[f.member];
        s.charge(
            grant
                .capability
                .len()
                .saturating_add(f.capability.value.len())
                .saturating_add(1),
            0,
        )?;
        if grant.capability != f.capability.value {
            return Err(error(unit, ErrorKind::Capability, Some(index), Some(g)));
        }
        let sig = wire::Signature::new(contract, &i.key.value, &f.name.value, s)?;
        let bytes = sig.preimage(s)?;
        s.charge(
            grant
                .signature
                .len()
                .saturating_add(bytes.len())
                .saturating_add(1),
            0,
        )?;
        if grant.signature != bytes {
            return Err(error(unit, ErrorKind::Signature, Some(index), Some(g)));
        }
        seen[index] = true;
        signatures[index] = sig.digest();
    }
    for (index, seen) in seen.into_iter().enumerate() {
        s.charge(1, 0)?;
        if !seen {
            return Err(error(unit, ErrorKind::MissingGrant, Some(index), None));
        }
    }
    // This is the first inventory access. Even an unused import or a later
    // interface's invalid grant must refuse before it.
    let providers = inventory();
    s.count(providers.len(), 0)?;
    let mut metadata = s.vector(providers.len(), 0)?;
    for provider in providers {
        metadata.push(provider.metadata());
    }
    let mut selections = s.vector(contract.document().imports.len(), 0)?;
    for import in 0..contract.document().imports.len() {
        s.charge(1, 0)?;
        selections.push(wire::select_granted_provider(
            contract, import, &metadata, s,
        )?);
    }
    let mut imports = s.vector(unit.imports().len(), 0)?;
    for (index, f) in unit.imports().iter().enumerate() {
        let mut selected = None;
        for selection in &selections {
            s.charge(1, 0)?;
            if selection.consumer_interface() != f.interface {
                continue;
            }
            for member in selection.members() {
                s.charge(1, 0)?;
                if member.expected == f.member {
                    selected = Some((selection, member.actual));
                    break;
                }
            }
        }
        let (selection, member) = selected
            .ok_or_else(|| error(unit, ErrorKind::MissingImplementation, Some(index), None))?;
        let provider = &providers[selection.provider_index()];
        let mut implementation = None;
        for method in provider.methods() {
            s.charge(1, 0)?;
            if method.interface == selection.provider_interface() && method.member == member {
                implementation = Some(method);
                break;
            }
        }
        let method = implementation
            .ok_or_else(|| error(unit, ErrorKind::MissingImplementation, Some(index), None))?;
        imports.push(BoundImport {
            source: m::ImportId(index as u32),
            provider,
            member: bindings::Function {
                interface: selection.provider_interface(),
                member,
            },
            method,
            signature: signatures[index],
        });
    }
    Ok(Bindings {
        unit,
        contract,
        imports,
    })
}
