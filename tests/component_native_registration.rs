use lang_mir::unit as m;
use lang_runtime::component::{
    idl, native,
    serialization::{DigestAlgorithm, VersionedDigest},
    wire,
};
use std::{cell::Cell, ffi::c_void, ptr};

extern "C" fn callback(
    _: *mut c_void,
    _: *const native::Context,
    _: native::Bytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    0
}

#[test]
fn registration_retains_real_callback_and_owner_without_calling_it() {
    let doc = idl::check(
        m::SourceId(71),
        include_bytes!("fixtures/component_generated_script.idl.json"),
        Default::default(),
    )
    .unwrap();
    let mut s = wire::Session::new(Default::default());
    let contract = wire::Contract::new(doc, vec![], &mut s).unwrap();
    let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
    let owner = Cell::new(0);
    let methods = [native::Method {
        interface: 0,
        member: 0,
        invoke: callback,
        user: ptr::from_ref(&owner).cast_mut().cast(),
        flags: 1,
    }];
    let provider = native::Provider::new(
        wire::Provider {
            contract: &contract,
            digest: &digest,
        },
        &methods,
        &owner,
        &mut s,
    )
    .unwrap();
    assert!(provider.retains_code(&owner));
    assert!(std::ptr::fn_addr_eq(
        provider.methods()[0].invoke,
        callback as native::Invoke
    ));
    assert_eq!(
        provider.methods()[0].user,
        ptr::from_ref(&owner).cast_mut().cast()
    );
    assert_eq!(owner.get(), 0);
    assert_eq!(size_of::<native::Header>(), 8);
    assert_eq!(size_of::<native::Bytes>(), 16);
    assert_eq!(size_of::<native::Context>(), 48);
    assert_eq!(align_of::<native::Context>(), 8);
    assert_eq!(std::mem::offset_of!(native::Context, engine), 8);
    assert_eq!(std::mem::offset_of!(native::Context, invocation), 16);
    assert_eq!(std::mem::offset_of!(native::Context, scope), 24);
    assert_eq!(std::mem::offset_of!(native::Context, reserved), 32);
}

#[test]
fn all_export_implementations_are_required_and_registration_is_bounded() {
    use native::{RegistrationError as E, RegistrationErrorKind as K};
    let doc = idl::check(
        m::SourceId(71),
        include_bytes!("fixtures/component_generated_script.idl.json"),
        Default::default(),
    )
    .unwrap();
    let contract =
        wire::Contract::new(doc, vec![], &mut wire::Session::new(Default::default())).unwrap();
    let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
    let metadata = wire::Provider {
        contract: &contract,
        digest: &digest,
    };
    let owner = Cell::new(0);
    let good = native::Method {
        interface: 0,
        member: 0,
        invoke: callback,
        user: ptr::null_mut(),
        flags: 0,
    };
    for (methods, expected) in [
        (vec![], K::MissingMember),
        (vec![good, good], K::DuplicateMember),
        (
            vec![native::Method {
                interface: 1,
                ..good
            }],
            K::UnknownMember,
        ),
        (vec![native::Method { member: 1, ..good }], K::UnknownMember),
        (vec![native::Method { flags: 2, ..good }], K::Flags),
    ] {
        let error = native::Provider::new(
            metadata,
            &methods,
            &owner,
            &mut wire::Session::new(Default::default()),
        )
        .err()
        .unwrap();
        assert!(matches!(error, E::Method { kind, .. } if kind == expected));
        assert_eq!(owner.get(), 0);
    }
    let methods = [good];
    let mut s = wire::Session::new(Default::default());
    native::Provider::new(metadata, &methods, &owner, &mut s).unwrap();
    let usage = s.usage();
    let exact = wire::Limits {
        work: usage.work,
        allocation_bytes: usage.allocation_bytes,
        elements: usage.elements,
        serialization_bytes: usage.serialization_bytes,
        ..Default::default()
    };
    native::Provider::new(metadata, &methods, &owner, &mut wire::Session::new(exact)).unwrap();
    for limits in [
        wire::Limits {
            work: usage.work - 1,
            ..exact
        },
        wire::Limits {
            allocation_bytes: usage.allocation_bytes - 1,
            ..exact
        },
        wire::Limits {
            elements: usage.elements - 1,
            ..exact
        },
        wire::Limits {
            serialization_bytes: usage.serialization_bytes - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            native::Provider::new(metadata, &methods, &owner, &mut wire::Session::new(limits)),
            Err(E::Wire(_))
        ));
    }
    // A non-exported declaration cannot be registered as an implementation.
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "fixtures/component_generated_script.idl.json"
    ))
    .unwrap();
    value["exports"] = serde_json::json!([]);
    let checked = idl::check(
        m::SourceId(71),
        &serde_json::to_vec(&value).unwrap(),
        Default::default(),
    )
    .unwrap();
    let hidden =
        wire::Contract::new(checked, vec![], &mut wire::Session::new(Default::default())).unwrap();
    assert!(matches!(
        native::Provider::new(
            wire::Provider {
                contract: &hidden,
                digest: &digest
            },
            &methods,
            &owner,
            &mut wire::Session::new(Default::default())
        ),
        Err(E::Method {
            kind: K::UnknownMember,
            ..
        })
    ));
}

extern "C" fn destroy(
    _: *mut c_void,
    _: *const native::Context,
    _: native::MutBytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    panic!("registration invoked resource destruction")
}

#[test]
fn registered_resource_kinds_retain_real_destructors_and_seal_provider_identity() {
    use native::{RegistrationError as E, RegistrationErrorKind as K};
    let doc = idl::check(
        m::SourceId(71),
        include_bytes!("fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap();
    let contract =
        wire::Contract::new(doc, vec![], &mut wire::Session::new(Default::default())).unwrap();
    let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
    let metadata = wire::Provider {
        contract: &contract,
        digest: &digest,
    };
    let owner = Cell::new(0);
    let methods = [native::Method {
        interface: 0,
        member: 0,
        invoke: callback,
        user: ptr::null_mut(),
        flags: 0,
    }];
    let good = native::ResourceType {
        interface: 0,
        resource: 0,
        destroy,
        user: ptr::from_ref(&owner).cast_mut().cast(),
    };
    for (resources, expected) in [
        (vec![], K::MissingResource),
        (vec![good, good], K::DuplicateResource),
        (
            vec![native::ResourceType {
                interface: 1,
                ..good
            }],
            K::UnknownResource,
        ),
        (
            vec![native::ResourceType {
                resource: 1,
                ..good
            }],
            K::UnknownResource,
        ),
    ] {
        assert!(
            matches!(native::Provider::with_resources(metadata, &methods, &resources, &owner, &mut wire::Session::new(Default::default())), Err(E::Resource { kind, .. }) if kind == expected)
        );
    }
    assert!(matches!(
        native::Provider::new(
            metadata,
            &methods,
            &owner,
            &mut wire::Session::new(Default::default())
        ),
        Err(E::Resource {
            kind: K::MissingResource,
            ..
        })
    ));
    let resources = [good];
    let mut s = wire::Session::new(Default::default());
    let first =
        native::Provider::with_resources(metadata, &methods, &resources, &owner, &mut s).unwrap();
    let second =
        native::Provider::with_resources(metadata, &methods, &resources, &owner, &mut s).unwrap();
    let key = &contract.document().interfaces[0].resources[0].value;
    let a = first.resource_kind(key, &mut s).unwrap();
    let b = first.resource_kind(key, &mut s).unwrap();
    let c = second.resource_kind(key, &mut s).unwrap();
    assert!(a.same_implementation(b));
    assert!(!a.same_implementation(c));
    assert_eq!(a.key(), c.key());
    assert!(a.provider().retains_code(&owner));
    assert!(std::ptr::fn_addr_eq(
        a.registration().destroy,
        destroy as native::Destroy
    ));
    assert_eq!(
        a.registration().user,
        ptr::from_ref(&owner).cast_mut().cast()
    );
    assert_eq!(owner.get(), 0);
    assert_eq!(size_of::<native::MutBytes>(), 16);
    assert_eq!(std::mem::offset_of!(native::MutBytes, length), 8);
    let mut s = wire::Session::new(Default::default());
    native::Provider::with_resources(metadata, &methods, &resources, &owner, &mut s).unwrap();
    let usage = s.usage();
    let exact = wire::Limits {
        work: usage.work,
        elements: usage.elements,
        allocation_bytes: usage.allocation_bytes,
        serialization_bytes: usage.serialization_bytes,
        ..Default::default()
    };
    native::Provider::with_resources(
        metadata,
        &methods,
        &resources,
        &owner,
        &mut wire::Session::new(exact),
    )
    .unwrap();
    assert!(
        native::Provider::with_resources(
            metadata,
            &methods,
            &resources,
            &owner,
            &mut wire::Session::new(wire::Limits {
                work: usage.work - 1,
                ..exact
            })
        )
        .is_err()
    );
}

#[test]
fn physical_storage_checks_tickets_holders_allocator_and_implementation_before_access() {
    use lang_runtime::{allocation as a, component::storage as s};
    let doc = idl::check(
        m::SourceId(71),
        include_bytes!("fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap();
    let mut session = wire::Session::new(Default::default());
    let contract = wire::Contract::new(doc, vec![], &mut session).unwrap();
    let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
    let owner = Cell::new(0);
    let methods = [native::Method {
        interface: 0,
        member: 0,
        invoke: callback,
        user: ptr::null_mut(),
        flags: 0,
    }];
    let types = [native::ResourceType {
        interface: 0,
        resource: 0,
        destroy,
        user: ptr::null_mut(),
    }];
    let metadata = wire::Provider {
        contract: &contract,
        digest: &digest,
    };
    let first =
        native::Provider::with_resources(metadata, &methods, &types, &owner, &mut session).unwrap();
    let second =
        native::Provider::with_resources(metadata, &methods, &types, &owner, &mut session).unwrap();
    let key = &contract.document().interfaces[0].resources[0].value;
    let kind = first.resource_kind(key, &mut session).unwrap();
    let other_kind = second.resource_kind(key, &mut session).unwrap();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut budget = m::Budget::new(Default::default());
    let mut values = s::Resources::prepare(&allocator, 2, 2, &mut budget).unwrap();
    let host = values.host();
    let guest = values.new_holder().unwrap();
    let original = values
        .publish(kind, host, &41_i64.to_le_bytes(), &mut allocator)
        .unwrap();
    assert_eq!(allocator.stats().live_allocations, 2);
    assert_eq!(allocator.stats().live_bytes, 56);
    let before = allocator.stats();
    let prepared = values
        .preflight(&[original], host, guest, &mut allocator)
        .unwrap();
    let abandoned = prepared.claim(0).unwrap();
    drop(prepared);
    values.validate(original, host, key, &allocator).unwrap();
    assert!(values.validate(abandoned, host, key, &allocator).is_err());
    assert!(matches!(
        values.preflight(&[original, original], host, guest, &mut allocator),
        Err(s::Error::AliasedOwner)
    ));
    assert_eq!(allocator.stats(), before);
    let prepared = values
        .preflight(&[original], host, guest, &mut allocator)
        .unwrap();
    let received = prepared.claim(0).unwrap();
    prepared.commit();
    assert_eq!(allocator.stats(), before);
    assert!(values.validate(original, host, key, &allocator).is_err());
    assert!(matches!(
        values.validate(received, host, key, &allocator),
        Err(s::Error::WrongHolder)
    ));
    assert_eq!(s::Claim::from_token(received.token()).unwrap(), received);
    assert_eq!(
        values.bytes(received, guest, kind, &allocator).unwrap(),
        &41_i64.to_le_bytes()
    );
    assert!(matches!(
        values.bytes(received, guest, other_kind, &allocator),
        Err(s::Error::Implementation)
    ));
    let mut foreign = a::Allocator::new(a::System, Default::default()).unwrap();
    assert!(matches!(
        values.validate(received, guest, key, &foreign),
        Err(s::Error::ForeignAllocator)
    ));
    let empty = values
        .prepare_transfer(&[], host, guest, &allocator)
        .unwrap();
    assert!(matches!(
        values.commit_transfer(empty, &mut foreign),
        Err(s::Error::ForeignAllocator)
    ));
    values.abort_transfer(empty).unwrap();
    let mut malformed = received.token();
    malformed[12] = 1;
    assert!(s::Claim::from_token(malformed).is_err());
    let mut detached = values.detach(received, guest, &allocator).unwrap();
    assert!(values.is_empty());
    assert!(values.validate(received, guest, key, &allocator).is_err());
    assert!(detached.kind().unwrap().same_implementation(kind));
    // Storage detachment does not invoke native destruction; the execution
    // adapter must call its real descriptor under the original live context.
    assert_eq!(owner.get(), 0);
    detached.release(&mut allocator).unwrap();
    assert!(detached.release(&mut allocator).is_err());
    assert_eq!(allocator.stats().deallocations, 2);
    assert_eq!(allocator.stats().live_allocations, 0);
}
