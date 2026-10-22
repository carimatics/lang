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
