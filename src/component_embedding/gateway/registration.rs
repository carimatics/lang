//! Copy and validate native registration inputs before consuming their owner.
use super::{bytes, ffi, header};
use lang_runtime::component::serialization::{DigestAlgorithm, VersionedDigest};
use lang_runtime::{
    c_abi::context::CodeOwner,
    component::{native, wire},
};
use sha2::{Digest, Sha256};
use std::{cell::Cell, ptr, rc::Rc};

struct Owner {
    user: *mut std::ffi::c_void,
    release: ffi::ReleaseFn,
    acquired: Cell<bool>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        if self.acquired.get() {
            // SAFETY: successful registration acquired this exact native
            // owner. Its non-unwinding code/user survives the final return.
            unsafe { (self.release)(self.user) };
        }
    }
}
fn key_matches(input: &[u8], parts: &[&[u8]], session: &mut wire::Session) -> Result<bool, u32> {
    let mut suffix = Some(input);
    for part in parts {
        session
            .charge(part.len().saturating_add(1), 0)
            .map_err(|_| 2u32)?;
        suffix = suffix.and_then(|tail| tail.strip_prefix(*part));
    }
    Ok(suffix == Some(&[][..]))
}
unsafe fn descriptors<'a, T>(
    pointer: *const T,
    count: u64,
    session: &mut wire::Session,
) -> Result<&'a [T], u32> {
    let count = usize::try_from(count).map_err(|_| 2u32)?;
    session.count(count, 0).map_err(|_| 2u32)?;
    let size = count.checked_mul(std::mem::size_of::<T>()).ok_or(2u32)?;
    if size > isize::MAX as usize || (count != 0 && (pointer.is_null() || !pointer.is_aligned())) {
        return Err(2);
    }
    session.charge(size, 0).map_err(|_| 2u32)?;
    if count == 0 {
        return Ok(&[]);
    }
    // SAFETY: caller retains this entire initialized aligned descriptor array
    // through admission. Nothing returned by prepare borrows these elements.
    Ok(unsafe { std::slice::from_raw_parts(pointer, count) })
}
/// # Safety
/// All views and descriptor arrays remain valid through return. Native code
/// and user storage survive the owner release callback's final return. The
/// caller pre-admits publication storage so no refusal follows acquisition.
pub(super) unsafe fn prepare(
    config: &ffi::ProviderConfig,
    session: &mut wire::Session,
) -> Result<native::Provider<'static>, u32> {
    if !unsafe { header(ptr::from_ref(config)) } || config.reserved != [0; 2] {
        return Err(2);
    }
    let release = config.release_owner.ok_or(2u32)?;
    let contract = unsafe { bytes(config.contract) }?;
    session.charge(contract.len(), 0).map_err(|_| 2u32)?;
    let contract =
        wire::decode_contract(lang_mir::unit::SourceId(0), contract, session).map_err(|_| 2u32)?;
    let input_methods = unsafe { descriptors(config.imports, config.import_count, session) }?;
    let input_resources =
        unsafe { descriptors(config.resource_types, config.resource_type_count, session) }?;
    let mut methods = session.vector(input_methods.len(), 0).map_err(|_| 2u32)?;
    let mut resources = session.vector(input_resources.len(), 0).map_err(|_| 2u32)?;
    for method in input_methods {
        if !unsafe { header(ptr::from_ref(method)) }
            || method.reserved0 != 0
            || method.reserved != [0; 2]
            || method.flags & !1 != 0
        {
            return Err(2);
        }
        let invoke = method.invoke.ok_or(2u32)?;
        let key = unsafe { bytes(method.function_key) }?;
        session.charge(key.len(), 0).map_err(|_| 2u32)?;
        std::str::from_utf8(key).map_err(|_| 2u32)?;
        let mut found = None;
        session
            .charge(contract.document().interfaces.len(), 0)
            .map_err(|_| 2u32)?;
        for (interface, d) in contract.document().interfaces.iter().enumerate() {
            for (member, f) in d.functions.iter().enumerate() {
                let k = &d.key.value;
                let parts = [
                    k.package().namespace().as_bytes(),
                    b":",
                    k.package().package().as_bytes(),
                    b"/",
                    k.interface().as_bytes(),
                    b".",
                    f.name.value.as_bytes(),
                ];
                if key_matches(key, &parts, session)? {
                    let signature = wire::Signature::new(&contract, k, &f.name.value, session)
                        .map_err(|_| 2u32)?;
                    session.charge(32, 0).map_err(|_| 2u32)?;
                    if signature.digest() != method.signature {
                        return Err(2);
                    }
                    found = Some(native::Method {
                        interface,
                        member,
                        invoke,
                        user: method.user,
                        flags: method.flags,
                    });
                }
            }
        }
        methods.push(found.ok_or(2u32)?);
    }
    for resource in input_resources {
        if !unsafe { header(ptr::from_ref(resource)) } || resource.reserved != [0; 2] {
            return Err(2);
        }
        let destroy = resource.destroy.ok_or(2u32)?;
        let key = unsafe { bytes(resource.resource_key) }?;
        session.charge(key.len(), 0).map_err(|_| 2u32)?;
        std::str::from_utf8(key).map_err(|_| 2u32)?;
        let mut found = None;
        session
            .charge(contract.document().interfaces.len(), 0)
            .map_err(|_| 2u32)?;
        for (interface, d) in contract.document().interfaces.iter().enumerate() {
            for (index, r) in d.resources.iter().enumerate() {
                let k = &r.value;
                let i = k.interface();
                let parts = [
                    i.package().namespace().as_bytes(),
                    b":",
                    i.package().package().as_bytes(),
                    b"/",
                    i.interface().as_bytes(),
                    b".",
                    k.resource().as_bytes(),
                ];
                if key_matches(key, &parts, session)? {
                    found = Some(native::ResourceType {
                        interface,
                        resource: index,
                        destroy,
                        user: resource.user,
                    });
                }
            }
        }
        resources.push(found.ok_or(2u32)?);
    }
    let encoded = wire::encode_contract(&contract, session).map_err(|_| 2u32)?;
    session.charge(encoded.len(), 0).map_err(|_| 2u32)?;
    session.retained::<u8>(32, 0).map_err(|_| 2u32)?;
    let digest = VersionedDigest::new(DigestAlgorithm::Sha256, Sha256::digest(&encoded).to_vec())
        .map_err(|_| 2u32)?;
    let owner = session
        .shared(
            || Owner {
                user: config.owner,
                release,
                acquired: Cell::new(false),
            },
            0,
        )
        .map_err(|_| 2u32)?;
    let mut acquire = || -> Rc<dyn CodeOwner> {
        owner.acquired.set(true);
        owner.clone()
    };
    native::Provider::new_owned(contract, digest, methods, resources, &mut acquire, session)
        .map_err(|_| 2u32)
}

pub(super) struct Grant {
    pub member: lang_runtime::component::bindings::MemberKey,
    pub capability: String,
    pub signature: Vec<u8>,
}
impl Grant {
    pub fn view(&self) -> lang_runtime::component::grants::Grant<'_> {
        lang_runtime::component::grants::Grant {
            member: &self.member,
            capability: &self.capability,
            signature: &self.signature,
        }
    }
}
/// # Safety
/// Caller retains every initialized grant view and its bytes through return.
pub(super) unsafe fn grants(
    pointer: *const ffi::Bytes,
    count: u64,
    session: &mut wire::Session,
) -> Result<Vec<Grant>, u32> {
    // SAFETY: the caller retains the exact complete view array through return.
    let inputs = unsafe { descriptors(pointer, count, session) }?;
    let mut output = session.vector(inputs.len(), 0).map_err(|_| 2u32)?;
    for input in inputs {
        // SAFETY: each complete readable byte range is the caller contract.
        let data = unsafe { bytes(*input) }?;
        session.charge(data.len(), 0).map_err(|_| 2u32)?;
        let contract =
            wire::decode_contract(lang_mir::unit::SourceId(0), data, session).map_err(|_| 2u32)?;
        let document = contract.document();
        if document.imports.len() != 1 || document.imports[0].members.len() != 1 {
            return Err(2);
        }
        let requirement = &document.imports[0];
        let key = &requirement.interface.value;
        let key_bytes = key
            .package()
            .namespace()
            .len()
            .saturating_add(key.package().package().len())
            .saturating_add(key.interface().len())
            .saturating_add(1);
        session
            .charge(document.interfaces.len().saturating_mul(key_bytes), 0)
            .map_err(|_| 2u32)?;
        let interface = document
            .interfaces
            .iter()
            .find(|i| i.key.value == requirement.interface.value)
            .ok_or(2u32)?;
        session
            .charge(
                interface
                    .functions
                    .len()
                    .saturating_mul(requirement.members[0].value.len().saturating_add(1)),
                0,
            )
            .map_err(|_| 2u32)?;
        let function = interface
            .functions
            .iter()
            .find(|f| f.name.value == requirement.members[0].value)
            .ok_or(2u32)?;
        let signature = wire::Signature::new(
            &contract,
            &interface.key.value,
            &function.name.value,
            session,
        )
        .map_err(|_| 2u32)?
        .preimage(session)
        .map_err(|_| 2u32)?;
        let key = &interface.key.value;
        let bytes = key
            .package()
            .namespace()
            .len()
            .saturating_add(key.package().package().len())
            .saturating_add(key.interface().len())
            .saturating_add(function.name.value.len())
            .saturating_add(function.capability.value.len());
        session.charge(bytes, 0).map_err(|_| 2u32)?;
        session.retained::<u8>(bytes, 0).map_err(|_| 2u32)?;
        output.push(Grant {
            member: lang_runtime::component::bindings::MemberKey {
                interface: key.clone(),
                member: function.name.value.clone(),
            },
            capability: function.capability.value.clone(),
            signature,
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_mir::unit as m;
    use lang_runtime::component::idl;
    unsafe extern "C" fn release(owner: *mut std::ffi::c_void) {
        // SAFETY: this test retains the exact counter until all leases drop.
        let counter = unsafe { &*owner.cast::<Cell<u32>>() };
        counter.set(counter.get() + 1);
    }
    #[test]
    fn original_native_owner_is_unconsumed_on_refusal_and_released_after_the_last_lease() {
        let mut session = wire::Session::new(Default::default());
        let document = idl::check(m::SourceId(0), br#"{"idl":1,"package":"demo:host","version":"1.0.0","interfaces":[],"imports":[],"exports":[]}"#, Default::default()).unwrap();
        let contract = wire::Contract::new(document, vec![], &mut session).unwrap();
        let mut encoded = wire::encode_contract(&contract, &mut session).unwrap();
        let counter = Cell::new(0);
        let mut config = ffi::ProviderConfig {
            header: ffi::Header {
                size: std::mem::size_of::<ffi::ProviderConfig>() as u32,
                version: 1,
            },
            contract: ffi::Bytes {
                data: encoded.as_ptr(),
                length: encoded.len() as u64,
            },
            imports: ptr::null(),
            import_count: 0,
            resource_types: ptr::null(),
            resource_type_count: 0,
            owner: ptr::from_ref(&counter).cast_mut().cast(),
            release_owner: Some(release),
            reserved: [0; 2],
        };
        let limits = wire::Limits {
            work: 0,
            ..Default::default()
        };
        assert!(unsafe { prepare(&config, &mut wire::Session::new(limits)) }.is_err());
        assert_eq!(counter.get(), 0);
        config.reserved[0] = 1;
        assert!(unsafe { prepare(&config, &mut wire::Session::new(Default::default())) }.is_err());
        assert_eq!(counter.get(), 0);
        config.reserved[0] = 0;
        let provider =
            unsafe { prepare(&config, &mut wire::Session::new(Default::default())) }.unwrap();
        encoded.fill(0);
        let retained = provider.clone();
        drop(provider);
        assert_eq!(counter.get(), 0);
        assert_eq!(
            retained
                .metadata()
                .contract
                .document()
                .package
                .value
                .package(),
            "host"
        );
        drop(retained);
        assert_eq!(counter.get(), 1);
    }
}
