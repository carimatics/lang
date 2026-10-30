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
    let methods = [
        /* SAFETY: retained code/user outlive these descriptors and all synchronous uses. */
        unsafe { native::Method::new(0, 0, callback, ptr::from_ref(&owner).cast_mut().cast(), 1) },
    ];
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
        provider.methods()[0].invoke_fn(),
        callback as native::Invoke
    ));
    assert_eq!(
        provider.methods()[0].user_data(),
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
    // SAFETY: this inert callback has static code and never accesses user data.
    let good = unsafe { native::Method::new(0, 0, callback, ptr::null_mut(), 0) };
    for (methods, expected) in [
        (vec![], K::MissingMember),
        (vec![good, good], K::DuplicateMember),
        (
            vec![{
                let mut value = good;
                value.interface = 1;
                value
            }],
            K::UnknownMember,
        ),
        (
            vec![{
                let mut value = good;
                value.member = 1;
                value
            }],
            K::UnknownMember,
        ),
        (
            // SAFETY: the callback and null user storage have the same lifetime
            // as good; invalid flags are rejected before any invocation.
            vec![unsafe { native::Method::new(0, 0, callback, ptr::null_mut(), 2) }],
            K::Flags,
        ),
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
    let methods = [
        /* SAFETY: retained code/user outlive these descriptors and all synchronous uses. */
        unsafe { native::Method::new(0, 0, callback, ptr::null_mut(), 0) },
    ];
    // SAFETY: retained code/user outlive these descriptors and synchronous uses.
    let good = unsafe {
        native::ResourceType::new(0, 0, destroy, ptr::from_ref(&owner).cast_mut().cast())
    };
    for (resources, expected) in [
        (vec![], K::MissingResource),
        (vec![good, good], K::DuplicateResource),
        (
            vec![{
                let mut value = good;
                value.interface = 1;
                value
            }],
            K::UnknownResource,
        ),
        (
            vec![{
                let mut value = good;
                value.resource = 1;
                value
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
    assert!(a.same_implementation(&b));
    assert!(!a.same_implementation(&c));
    assert_eq!(a.key(), c.key());
    assert!(a.provider().retains_code(&owner));
    assert!(std::ptr::fn_addr_eq(
        a.registration().destroy_fn(),
        destroy as native::Destroy
    ));
    assert_eq!(
        a.registration().user_data(),
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
    let methods = [
        /* SAFETY: retained code/user outlive these descriptors and all synchronous uses. */
        unsafe { native::Method::new(0, 0, callback, ptr::null_mut(), 0) },
    ];
    let types = [
        /* SAFETY: retained code/user outlive these descriptors and all synchronous uses. */
        unsafe { native::ResourceType::new(0, 0, destroy, ptr::null_mut()) },
    ];
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
        .publish(kind.clone(), host, &41_i64.to_le_bytes(), &mut allocator)
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
        values
            .bytes(received, guest, kind.clone(), &allocator)
            .unwrap(),
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
    assert!(detached.kind().unwrap().same_implementation(&kind));
    // Storage detachment does not invoke native destruction; the execution
    // adapter must call its real descriptor under the original live context.
    assert_eq!(owner.get(), 0);
    detached.release(&mut allocator).unwrap();
    assert!(detached.release(&mut allocator).is_err());
    assert_eq!(allocator.stats().deallocations, 2);
    assert_eq!(allocator.stats().live_allocations, 0);
}

fn owning_inputs() -> (
    wire::Contract,
    VersionedDigest,
    Vec<native::Method>,
    Vec<native::ResourceType>,
) {
    let doc = idl::check(
        m::SourceId(71),
        include_bytes!("fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap();
    let contract =
        wire::Contract::new(doc, vec![], &mut wire::Session::new(Default::default())).unwrap();
    (
        contract,
        VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap(),
        vec![
            /* SAFETY: retained code/user outlive these descriptors and all synchronous uses. */
            unsafe { native::Method::new(0, 0, callback, ptr::null_mut(), 0) },
        ],
        vec![
            /* SAFETY: retained code/user outlive these descriptors and all synchronous uses. */
            unsafe { native::ResourceType::new(0, 0, destroy, ptr::null_mut()) },
        ],
    )
}

#[test]
fn owning_registration_acquires_owner_only_after_complete_budget_admission() {
    use std::rc::Rc;
    let (contract, digest, methods, resources) = owning_inputs();
    let acquisitions = Cell::new(0);
    let mut session = wire::Session::new(Default::default());
    let provider = native::Provider::new_owned(
        contract,
        digest,
        methods,
        resources,
        &mut || {
            acquisitions.set(acquisitions.get() + 1);
            Rc::new(())
        },
        &mut session,
    )
    .unwrap();
    assert_eq!(acquisitions.get(), 1);
    drop(provider);
    let used = session.usage();
    let exact = wire::Limits {
        work: used.work,
        allocation_bytes: used.allocation_bytes,
        elements: used.elements,
        serialization_bytes: used.serialization_bytes,
        ..Default::default()
    };
    for (limits, accepted) in [
        (exact, true),
        (
            wire::Limits {
                work: used.work - 1,
                ..exact
            },
            false,
        ),
        (
            wire::Limits {
                allocation_bytes: used.allocation_bytes - 1,
                ..exact
            },
            false,
        ),
        (
            wire::Limits {
                elements: used.elements - 1,
                ..exact
            },
            false,
        ),
        (
            wire::Limits {
                serialization_bytes: used.serialization_bytes - 1,
                ..exact
            },
            false,
        ),
    ] {
        acquisitions.set(0);
        let (contract, digest, methods, resources) = owning_inputs();
        let result = native::Provider::new_owned(
            contract,
            digest,
            methods,
            resources,
            &mut || {
                acquisitions.set(acquisitions.get() + 1);
                Rc::new(())
            },
            &mut wire::Session::new(limits),
        );
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(acquisitions.get(), usize::from(accepted));
    }
    for invalid_resource in [false, true] {
        acquisitions.set(0);
        let (contract, digest, mut methods, mut resources) = owning_inputs();
        if invalid_resource {
            resources.clear();
        } else {
            methods[0] = invalid_flags(methods[0]);
        }
        assert!(
            native::Provider::new_owned(
                contract,
                digest,
                methods,
                resources,
                &mut || {
                    acquisitions.set(acquisitions.get() + 1);
                    Rc::new(())
                },
                &mut wire::Session::new(Default::default())
            )
            .is_err()
        );
        assert_eq!(acquisitions.get(), 0);
    }
}

#[test]
fn retained_cleanup_record_owns_the_actual_registration_without_an_instance() {
    use lang_runtime::{
        allocation as a, cleanup as c,
        component::{execution, storage},
        execution as e,
    };
    use std::{num::NonZeroUsize, rc::Rc};
    struct Owner(Rc<Cell<usize>>);
    impl Drop for Owner {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    unsafe extern "C" fn fail(
        _: *mut std::ffi::c_void,
        _: *const native::Context,
        bytes: native::MutBytes,
        _: *mut *mut native::Buffer,
    ) -> u32 {
        assert_eq!(bytes.length, 8);
        1
    }
    let drops = Rc::new(Cell::new(0));
    let actual = Rc::new(Owner(drops.clone()));
    let (contract, digest, methods, mut descriptors) = owning_inputs();
    // SAFETY: fail ignores its user pointer and is permanently linked.
    descriptors[0] = unsafe {
        native::ResourceType::new(
            descriptors[0].interface,
            descriptors[0].resource,
            fail,
            descriptors[0].user_data(),
        )
    };
    let provider = native::Provider::new_owned(
        contract,
        digest,
        methods,
        descriptors,
        &mut || actual.clone(),
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    let key = provider.metadata().contract.document().interfaces[0].resources[0]
        .value
        .clone();
    let kind = provider
        .resource_kind(&key, &mut wire::Session::new(Default::default()))
        .unwrap();
    let mut memory = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut budget = m::Budget::new(Default::default());
    let mut resources = storage::Resources::prepare(&memory, 1, 1, &mut budget).unwrap();
    let holder = resources.new_holder().unwrap();
    let claim = resources
        .publish(kind.clone(), holder, &41i64.to_le_bytes(), &mut memory)
        .unwrap();
    let mut backing = resources.detach(claim, holder, &memory).unwrap();
    let mut cleanup = c::Session::prepare(NonZeroUsize::new(2).unwrap(), &mut budget).unwrap();
    let mut bridge = lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
    {
        let mut invocation = bridge
            .begin(e::Control::with_limits(e::Limits::unbounded()))
            .unwrap();
        let mut cleaner = c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
            execution::Invoker::destroy(
                &mut invocation,
                kind.clone(),
                &mut backing,
                &mut memory,
                context,
            )
        });
        assert!(
            cleanup
                .retire(c::Slot(0), (), None, c::Reason::ScopeExit, &mut cleaner)
                .is_err()
        );
    }
    backing.release(&mut memory).unwrap();
    assert_eq!(a::Memory::stats(&memory).live_allocations, 0);
    drop(backing);
    drop(resources);
    drop(kind);
    drop(provider);
    drop(actual);
    assert_eq!(drops.get(), 0);
    let report = cleanup.into_report();
    let record = report.primary().unwrap().clone();
    assert!(record.code_owner.is_some());
    drop(report);
    assert_eq!(drops.get(), 0);
    drop(record);
    assert_eq!(drops.get(), 1);
}

fn invalid_flags(method: native::Method) -> native::Method {
    // SAFETY: preserve this fixture's live code/user pointers. Invalid flags are
    // rejected during registration, before the callback can run.
    unsafe {
        native::Method::new(
            method.interface,
            method.member,
            method.invoke_fn(),
            method.user_data(),
            2,
        )
    }
}

#[test]
fn refused_registration_leaves_the_owner_factory_with_its_caller() {
    use std::rc::Rc;
    let owner = Rc::new(());
    let weak = Rc::downgrade(&owner);
    let mut factory = move || -> Rc<dyn lang_runtime::c_abi::context::CodeOwner> { owner.clone() };
    let (contract, digest, mut methods, resources) = owning_inputs();
    methods[0] = invalid_flags(methods[0]);
    assert!(
        native::Provider::new_owned(
            contract,
            digest,
            methods,
            resources,
            &mut factory,
            &mut wire::Session::new(Default::default()),
        )
        .is_err()
    );
    assert!(weak.upgrade().is_some());
    drop(factory);
    assert!(weak.upgrade().is_none());
}

struct RetainedOwner {
    released: std::rc::Rc<Cell<usize>>,
    destroyed: std::rc::Rc<Cell<usize>>,
}
impl Drop for RetainedOwner {
    fn drop(&mut self) {
        self.released.set(self.released.get() + 1);
    }
}
unsafe extern "C" fn retained_destroy(
    user: *mut c_void,
    _: *const native::Context,
    backing: native::MutBytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    // SAFETY: the test's retained owner lease keeps user live through return;
    // the detached original allocation supplies all backing.length bytes.
    let owner = unsafe { &*user.cast::<RetainedOwner>() };
    let bytes = unsafe { std::slice::from_raw_parts_mut(backing.data, backing.length as usize) };
    assert_eq!(bytes, &41_i64.to_le_bytes());
    owner.destroyed.set(owner.destroyed.get() + 1);
    0
}

#[test]
fn owning_kind_retains_original_descriptor_and_backing_after_registration_release() {
    use lang_runtime::{allocation as a, component::storage as s};
    use std::rc::Rc;
    let released = Rc::new(Cell::new(0));
    let destroyed = Rc::new(Cell::new(0));
    let owner = Rc::new(RetainedOwner {
        released: released.clone(),
        destroyed: destroyed.clone(),
    });
    let weak = Rc::downgrade(&owner);
    let (contract, digest, methods, mut resources) = owning_inputs();
    let key = contract.document().interfaces[0].resources[0].value.clone();
    // SAFETY: the registration retains this owner through final destruction.
    resources[0] = unsafe {
        native::ResourceType::new(
            resources[0].interface,
            resources[0].resource,
            retained_destroy,
            Rc::as_ptr(&owner).cast_mut().cast(),
        )
    };
    let mut session = wire::Session::new(Default::default());
    let provider = native::Provider::new_owned(
        contract,
        digest,
        methods,
        resources,
        &mut || owner.clone(),
        &mut session,
    )
    .unwrap();
    assert!(provider.retains_code(owner.as_ref()));
    let kind = provider
        .resource_kind(&key, &mut session)
        .unwrap()
        .into_owned()
        .ok()
        .unwrap();
    let clone = provider.clone();
    let clone_kind = clone.resource_kind(&key, &mut session).unwrap();
    assert!(kind.same_implementation(&clone_kind));
    let (contract, digest, methods, resources) = owning_inputs();
    let separate = native::Provider::new_owned(
        contract,
        digest,
        methods,
        resources,
        &mut || owner.clone(),
        &mut session,
    )
    .unwrap();
    let other = separate.resource_kind(&key, &mut session).unwrap();
    assert_eq!(kind.key(), other.key());
    assert!(!kind.same_implementation(&other));
    drop(other);
    drop(separate);
    drop(owner);
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut values =
        s::Resources::prepare(&allocator, 1, 1, &mut m::Budget::new(Default::default())).unwrap();
    let host = values.host();
    let claim = values
        .publish(kind.clone(), host, &41_i64.to_le_bytes(), &mut allocator)
        .unwrap();
    drop(kind);
    drop(clone_kind);
    drop(clone);
    drop(provider);
    assert!(weak.upgrade().is_some());
    assert_eq!(released.get(), 0);
    let mut detached = values.detach(claim, host, &allocator).unwrap();
    let descriptor = detached.kind().unwrap();
    let bytes = detached.bytes_mut(&mut allocator).unwrap();
    let mut failure = ptr::null_mut();
    // Isolated descriptor-lifetime probe; stored-source tests use live contexts.
    // SAFETY: the retained lease keeps code/user live, and the registered test
    // callback ignores context/failure and reads only the original live backing.
    let status = unsafe {
        (descriptor.registration().destroy_fn())(
            descriptor.registration().user_data(),
            ptr::null(),
            native::MutBytes {
                data: bytes.as_mut_ptr(),
                length: bytes.len() as u64,
            },
            &mut failure,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(destroyed.get(), 1);
    detached.release(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(released.get(), 0);
    drop(descriptor);
    assert_eq!(released.get(), 1);
    assert!(weak.upgrade().is_none());
    assert!(detached.release(&mut allocator).is_err());
    assert_eq!(destroyed.get(), 1);
}

#[test]
fn borrowed_kind_conversion_refusal_preserves_its_original_identity() {
    let (contract, digest, methods, resources) = owning_inputs();
    let mut session = wire::Session::new(Default::default());
    let owner = ();
    let provider = native::Provider::with_resources(
        wire::Provider {
            contract: &contract,
            digest: &digest,
        },
        &methods,
        &resources,
        &owner,
        &mut session,
    )
    .unwrap();
    let key = &contract.document().interfaces[0].resources[0].value;
    let kind = provider.resource_kind(key, &mut session).unwrap();
    let original = kind.clone();
    let refused = kind.into_owned().err().unwrap();
    assert!(original.same_implementation(&refused));
    assert_eq!(refused.key(), key);
}
