//! Trusted native registrations for the fixed component-v1 callback ABI.
//!
//! Preparation never invokes a callback or dereferences a native pointer. The
//! caller must keep the code and user storage valid through the borrowed owner;
//! the eventual native adapter must enforce the context and buffer contracts.
use super::wire;
use crate::c_abi::context::CodeOwner;
use std::{ffi::c_void, fmt};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub size: u32,
    pub version: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Bytes {
    pub data: *const u8,
    pub length: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MutBytes {
    pub data: *mut u8,
    pub length: u64,
}
#[repr(C)]
pub struct Engine {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Buffer {
    _private: [u8; 0],
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub header: Header,
    pub engine: *mut Engine,
    pub invocation: u64,
    pub scope: u64,
    pub reserved: [u64; 2],
}
/// The signature in docs/component-abi-v1.h. Calling it requires a live,
/// owner-thread component context and valid canonical input/output storage.
pub type Invoke = unsafe extern "C" fn(*mut c_void, *const Context, Bytes, *mut *mut Buffer) -> u32;
pub type Destroy =
    unsafe extern "C" fn(*mut c_void, *const Context, MutBytes, *mut *mut Buffer) -> u32;

#[derive(Clone, Copy, Debug)]
pub struct Method {
    pub interface: usize,
    pub member: usize,
    pub invoke: Invoke,
    pub user: *mut c_void,
    /// Bit 0 permits nested synchronous guest calls; all other bits are zero.
    pub flags: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct ResourceType {
    pub interface: usize,
    pub resource: usize,
    pub destroy: Destroy,
    pub user: *mut c_void,
}

/// A live implementation identity. Equal static keys from separately registered
/// providers cannot authorize access to one another's backing storage.
#[derive(Clone, Copy)]
pub struct ResourceKind<'a> {
    provider: &'a Provider<'a>,
    registration: usize,
}
impl<'a> ResourceKind<'a> {
    pub fn key(&self) -> &'a super::negotiation::ResourceTypeKey {
        let r = self.registration();
        &self.provider.metadata.contract.document().interfaces[r.interface].resources[r.resource]
            .value
    }
    pub const fn provider(&self) -> &'a Provider<'a> {
        self.provider
    }
    pub fn registration(&self) -> &'a ResourceType {
        &self.provider.resources[self.registration]
    }
    pub fn same_implementation(&self, other: ResourceKind<'_>) -> bool {
        std::ptr::eq(self.provider, other.provider) && self.registration == other.registration
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationErrorKind {
    UnknownMember,
    DuplicateMember,
    MissingMember,
    UnknownResource,
    DuplicateResource,
    MissingResource,
    Flags,
}
#[derive(Debug)]
pub enum RegistrationError {
    Wire(wire::Error),
    Method {
        kind: RegistrationErrorKind,
        index: usize,
    },
    Resource {
        kind: RegistrationErrorKind,
        index: usize,
    },
}
impl From<wire::Error> for RegistrationError {
    fn from(e: wire::Error) -> Self {
        Self::Wire(e)
    }
}
impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native component registration: {self:?}")
    }
}
impl std::error::Error for RegistrationError {}

/// An immutable registration retaining the actual code/user owner. Metadata
/// identity alone cannot construct one. This is trusted native code, not a sandbox.
pub struct Provider<'a> {
    metadata: wire::Provider<'a>,
    methods: &'a [Method],
    resources: &'a [ResourceType],
    owner: &'a dyn CodeOwner,
}
impl<'a> Provider<'a> {
    pub fn new(
        metadata: wire::Provider<'a>,
        methods: &'a [Method],
        owner: &'a dyn CodeOwner,
        session: &mut wire::Session,
    ) -> Result<Self, RegistrationError> {
        Self::with_resources(metadata, methods, &[], owner, session)
    }
    pub fn with_resources(
        metadata: wire::Provider<'a>,
        methods: &'a [Method],
        resources: &'a [ResourceType],
        owner: &'a dyn CodeOwner,
        session: &mut wire::Session,
    ) -> Result<Self, RegistrationError> {
        wire::encode_contract(metadata.contract, session)?;
        session.count(methods.len(), 0)?;
        session.count(resources.len(), 0)?;
        let d = metadata.contract.document();
        for (index, method) in methods.iter().enumerate() {
            let error = |kind| RegistrationError::Method { kind, index };
            if method.flags & !1 != 0 {
                return Err(error(RegistrationErrorKind::Flags));
            }
            let interface = d
                .interfaces
                .get(method.interface)
                .filter(|i| method.member < i.functions.len())
                .ok_or_else(|| error(RegistrationErrorKind::UnknownMember))?;
            let mut exported = false;
            for key in &d.exports {
                session.charge(
                    key.value.package().namespace().len()
                        + key.value.package().package().len()
                        + key.value.interface().len()
                        + 1,
                    0,
                )?;
                exported |= key.value == interface.key.value;
            }
            if !exported {
                return Err(error(RegistrationErrorKind::UnknownMember));
            }
            session.charge(index, 0)?;
            if methods[..index]
                .iter()
                .any(|m| m.interface == method.interface && m.member == method.member)
            {
                return Err(error(RegistrationErrorKind::DuplicateMember));
            }
        }
        for (index, resource) in resources.iter().enumerate() {
            let error = |kind| RegistrationError::Resource { kind, index };
            let interface = d
                .interfaces
                .get(resource.interface)
                .filter(|i| resource.resource < i.resources.len())
                .ok_or_else(|| error(RegistrationErrorKind::UnknownResource))?;
            let mut exported = false;
            for key in &d.exports {
                session.charge(
                    key.value.package().namespace().len()
                        + key.value.package().package().len()
                        + key.value.interface().len()
                        + 1,
                    0,
                )?;
                exported |= key.value == interface.key.value;
            }
            if !exported {
                return Err(error(RegistrationErrorKind::UnknownResource));
            }
            session.charge(index, 0)?;
            if resources[..index]
                .iter()
                .any(|r| r.interface == resource.interface && r.resource == resource.resource)
            {
                return Err(error(RegistrationErrorKind::DuplicateResource));
            }
        }
        // Every declared export must have a live implementation, including unused ones.
        for (interface, i) in d.interfaces.iter().enumerate() {
            session.charge(1, 0)?;
            let mut exported = false;
            for key in &d.exports {
                session.charge(
                    key.value.package().namespace().len()
                        + key.value.package().package().len()
                        + key.value.interface().len()
                        + 1,
                    0,
                )?;
                exported |= key.value == i.key.value;
            }
            if !exported {
                continue;
            }
            for resource in 0..i.resources.len() {
                session.charge(resources.len().saturating_add(1), 0)?;
                if !resources
                    .iter()
                    .any(|r| r.interface == interface && r.resource == resource)
                {
                    return Err(RegistrationError::Resource {
                        kind: RegistrationErrorKind::MissingResource,
                        index: resources.len(),
                    });
                }
            }
            for member in 0..i.functions.len() {
                session.charge(methods.len().saturating_add(1), 0)?;
                if !methods
                    .iter()
                    .any(|m| m.interface == interface && m.member == member)
                {
                    return Err(RegistrationError::Method {
                        kind: RegistrationErrorKind::MissingMember,
                        index: methods.len(),
                    });
                }
            }
        }
        Ok(Self {
            metadata,
            methods,
            resources,
            owner,
        })
    }
    pub const fn metadata(&self) -> wire::Provider<'a> {
        self.metadata
    }
    pub fn methods(&self) -> &'a [Method] {
        self.methods
    }
    pub fn resource_kind(
        &'a self,
        key: &super::negotiation::ResourceTypeKey,
        session: &mut wire::Session,
    ) -> Result<ResourceKind<'a>, RegistrationError> {
        session.charge(1, 0)?;
        let bytes = key
            .interface()
            .package()
            .namespace()
            .len()
            .saturating_add(key.interface().package().package().len())
            .saturating_add(key.interface().interface().len())
            .saturating_add(key.resource().len())
            .saturating_add(1);
        for registration in 0..self.resources.len() {
            session.charge(bytes, 0)?;
            let kind = ResourceKind {
                provider: self,
                registration,
            };
            if kind.key() == key {
                return Ok(kind);
            }
        }
        Err(RegistrationError::Resource {
            kind: RegistrationErrorKind::UnknownResource,
            index: self.resources.len(),
        })
    }
    pub(crate) fn resource_kind_bytes(
        &'a self,
        key: &[u8],
        budget: &mut lang_mir::unit::Budget,
    ) -> Result<ResourceKind<'a>, u32> {
        budget.charge(key.len()).map_err(|_| 12u32)?;
        for registration in 0..self.resources.len() {
            let kind = ResourceKind {
                provider: self,
                registration,
            };
            let typed = kind.key();
            let interface = typed.interface();
            let package = interface.package();
            let parts = [
                package.namespace().as_bytes(),
                b":",
                package.package().as_bytes(),
                b"/",
                interface.interface().as_bytes(),
                b".",
                typed.resource().as_bytes(),
            ];
            let mut remaining = Some(key);
            for part in parts {
                budget.charge(part.len()).map_err(|_| 12u32)?;
                remaining = remaining.and_then(|bytes| bytes.strip_prefix(part));
            }
            if remaining == Some(&[][..]) {
                return Ok(kind);
            }
        }
        Err(7)
    }
    pub fn retains_code<T>(&self, owner: &T) -> bool {
        std::ptr::addr_eq(self.owner as *const dyn CodeOwner, owner as *const T)
    }
}
