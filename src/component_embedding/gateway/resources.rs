//! C locators resolve only through the original holder/table/allocator.
use super::*;
use lang_runtime::cleanup as c;
use std::num::NonZeroUsize;

fn resource_kind(
    provider: &native::Provider<'static>,
    key: &[u8],
    session: &mut wire::Session,
) -> Result<native::ResourceKind<'static>, u32> {
    session.charge(key.len(), 0).map_err(|_| INVALID)?;
    std::str::from_utf8(key).map_err(|_| INVALID)?;
    let document = provider.metadata().contract.document();
    session
        .charge(document.interfaces.len(), 0)
        .map_err(|_| INVALID)?;
    for interface in &document.interfaces {
        for resource in &interface.resources {
            let kind = &resource.value;
            let i = kind.interface();
            let parts = [
                i.package().namespace().as_bytes(),
                b":",
                i.package().package().as_bytes(),
                b"/",
                i.interface().as_bytes(),
                b".",
                kind.resource().as_bytes(),
            ];
            let mut suffix = Some(key);
            for part in parts {
                session
                    .charge(part.len().saturating_add(1), 0)
                    .map_err(|_| INVALID)?;
                suffix = suffix.and_then(|bytes| bytes.strip_prefix(part));
            }
            if suffix == Some(&[][..]) {
                return provider.resource_kind(kind, session).map_err(|_| INVALID);
            }
        }
    }
    Err(INVALID)
}

/// # Safety
/// Exact live instance/provider and initialized views/output/error remain
/// valid through return. No caller may release the engine's backing storage.
pub unsafe extern "C" fn resource_publish(
    instance: *mut ffi::Instance,
    provider: *mut ffi::Provider,
    key: ffi::Bytes,
    input: ffi::Bytes,
    alignment: u64,
    output: *mut u32,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: caller retains initialized error and writable output objects.
    if instance.is_null()
        || provider.is_null()
        || output.is_null()
        || !unsafe { error_ready(error) }
    {
        return INVALID;
    }
    // SAFETY: caller supplies the exact still-live opaque handles.
    let (instance, provider) = unsafe {
        (
            &(*instance.cast::<InstanceHandle>()).0,
            &*provider.cast::<ProviderHandle>(),
        )
    };
    if instance.engine != provider.engine || instance.closed.get() {
        return INVALID;
    }
    // SAFETY: dependent handles retain this original engine metadata.
    let owner = match unsafe { owner(&instance.engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if owner.active.get() {
        return BUSY;
    }
    if instance.state.borrow().poisoned() {
        return INVALID;
    }
    let mut indices = instance.indices.borrow_mut();
    if indices.len() >= instance.ceiling.canonical_indices as usize {
        let failure = admission::Failure::limit(
            wire::ReportLimitKind::CanonicalIndices,
            instance.ceiling.canonical_indices as usize,
            indices.len().saturating_add(1),
        );
        // SAFETY: initialized fixed error output remains retained.
        unsafe { error.write(failure.error) };
        return INVALID;
    }
    let index = match u32::try_from(indices.len())
        .ok()
        .and_then(|n| n.checked_add(1))
    {
        Some(v) => v,
        None => return INVALID,
    };
    // SAFETY: both complete input byte views are the native caller contract.
    let (key, input) = match (unsafe { bytes(key) }, unsafe { bytes(input) }) {
        (Ok(k), Ok(i)) => (k, i),
        _ => return INVALID,
    };
    // SAFETY: no root/callback borrows this mutable execution storage.
    let data = unsafe { &mut *owner.data.as_ptr() };
    let mut session = wire::Session::new(data.limits.metadata);
    if session.charge(input.len(), 0).is_err() {
        return INVALID;
    }
    let Some(native) = provider.registration.provider.as_ref() else {
        return INVALID;
    };
    let kind = match resource_kind(native, key, &mut session) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let holder = instance.state.borrow().holder();
    owner.active.set(true);
    let _active = Active(&owner.active);
    let claim =
        match data
            .values
            .publish_aligned(kind, holder, input, alignment, &mut data.allocator)
        {
            Ok(v) => v,
            Err(_) => return INVALID,
        };
    // Instance admission reserved every index slot before any resource effect.
    debug_assert!(indices.len() < indices.capacity());
    indices.push(claim);
    // SAFETY: publication follows the original table's completed transaction.
    unsafe { output.write(index) };
    OK
}

fn cleanup_projection(
    report: &c::Report,
    allocations: a::Stats,
    canonical_indices: u64,
    maximum: u64,
) -> (wire::Report, Vec<u8>) {
    let record = |(category, diagnostic)| wire::Record {
        category,
        phase: wire::Phase::Cleanup,
        diagnostic,
        message: None,
        limit: None,
        origin: None,
    };
    let primary = report
        .primary()
        .and_then(|r| cleanup_description(r.kind))
        .unwrap_or((wire::Category::HostContract, wire::Diagnostic::HostContract));
    let mut suppressed = Vec::new();
    let mut omitted = report.truncated() as u64;
    for failure in report.secondary() {
        if let Some(description) = cleanup_description(failure.kind)
            && suppressed.len() < maximum.min(64) as usize
        {
            suppressed.push(record(description));
        } else {
            omitted = omitted.saturating_add(1);
        }
    }
    let output = wire::Report {
        policy: wire::MessagePolicy::Redacted,
        primary: record(primary),
        suppressed,
        poisoned: true,
        omitted,
        usage: wire::ReportUsage {
            peak_heap_bytes: allocations.peak_live_bytes,
            peak_live_allocations: allocations.peak_live_allocations as u64,
            canonical_indices,
            ..Default::default()
        },
        frames: Vec::new(),
    };
    // At most 65 fixed cleanup records; no guest-provided text or trace is
    // copied. This mandatory fixed-bound projection cannot reset guest work.
    let mut session = wire::Session::new(wire::Limits {
        bytes: 16384,
        serialization_bytes: 16384,
        allocation_bytes: 65536,
        work: 65536,
        elements: 1024,
        ..Default::default()
    });
    let bytes =
        wire::encode_report(&output, &mut session).expect("bounded redacted cleanup projection");
    (output, bytes)
}

/// # Safety
/// Exact live instance and initialized output/error remain valid through this
/// complete owner-thread operation. A successful drop invalidates the index.
pub unsafe extern "C" fn resource_drop(
    instance: *mut ffi::Instance,
    index: u32,
    output: *mut *mut ffi::Result,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: caller retains initialized error and writable result storage.
    if instance.is_null() || output.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: instance is this gateway's exact live handle.
    let instance = unsafe { &(*instance.cast::<InstanceHandle>()).0 };
    // SAFETY: dependent handle retains original metadata.
    let owner = match unsafe { owner(&instance.engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if owner.active.get() {
        return BUSY;
    }
    if instance.closed.get() {
        return INVALID;
    }
    let Some(position) = index.checked_sub(1) else {
        return INVALID;
    };
    let Some(claim) = instance.indices.borrow().get(position as usize).copied() else {
        return INVALID;
    };
    let Some(next_handles) = owner.handles.get().checked_add(1) else {
        return OOM;
    };
    // SAFETY: owner-thread admission rejects active mutable execution borrows.
    let data = unsafe { &mut *owner.data.as_ptr() };
    let mut budget = m::Budget::new(data.limits.preparation);
    owner.active.set(true);
    let _active = Active(&owner.active);
    let claims = [claim];
    let retired = match retire(instance, data, &claims, &mut budget) {
        Ok(v) => v,
        Err(s) => return s,
    };
    if let Some(value) = retired {
        owner.handles.set(next_handles);
        // SAFETY: publish the exact retained immutable cleanup result.
        unsafe { output.write(Box::into_raw(Box::new(value)).cast()) };
        1
    } else {
        // SAFETY: success publishes no failure result.
        unsafe { output.write(ptr::null_mut()) };
        OK
    }
}

fn retire(
    instance: &Rc<InstanceData>,
    data: &mut Data,
    claims: &[storage::Claim],
    budget: &mut m::Budget,
) -> Result<Option<CallResult>, u32> {
    if claims.is_empty() {
        return Ok(None);
    }
    let holder = instance.state.borrow().holder();
    budget.allocate::<CallResult>(1).map_err(|_| OOM)?;
    budget.allocate::<c::Report>(1).map_err(|_| OOM)?;
    let mut retained = budget.vector(claims.len()).map_err(|_| OOM)?;
    budget.charge(claims.len()).map_err(|_| OOM)?;
    for claim in claims {
        retained.push(
            data.values
                .retirement_kind(*claim, holder, &data.allocator)
                .map_err(|error| match error {
                    storage::Error::Table(
                        lang_runtime::component::resource::ResourceTableError::ResourceInUse {
                            ..
                        },
                    )
                    | storage::Error::State => BUSY,
                    _ => 5u32,
                })?,
        );
    }
    let records = usize::try_from(instance.ceiling.suppressed_records)
        .unwrap_or(usize::MAX)
        .saturating_add(1)
        .min(claims.len().saturating_mul(2).max(1));
    let mut cleanup = c::Session::prepare(NonZeroUsize::new(records).expect("nonzero"), budget)
        .map_err(|_| OOM)?;
    let mut invoker = data.bridge.begin(e::Control::default())?;
    for (slot, (claim, kind)) in claims.iter().zip(&retained).enumerate() {
        // All target releases were preflighted before any destructor. Native
        // cleanup cannot mutate this original table through an active root.
        let mut backing = data
            .values
            .detach(*claim, holder, &data.allocator)
            .expect("complete original-table preflight and active guard preserve retirement");
        let mut cleaner =
            c::Contextual::new(|_: &mut (), _: c::Reason, context: &mut c::Context<'_>| {
                let result = execution::Invoker::destroy(
                    &mut invoker,
                    kind.clone(),
                    &mut backing,
                    &mut data.allocator,
                    context,
                );
                if backing.release(&mut data.allocator).is_err() {
                    context.fail(17);
                }
                result
            });
        let _ = cleanup.retire(c::Slot(slot), (), None, c::Reason::Explicit, &mut cleaner);
    }
    if !cleanup.is_poisoned() {
        return Ok(None);
    }
    instance.state.borrow_mut().poison();
    let original = cleanup.into_report();
    let projection = cleanup_projection(
        &original,
        data.allocator.stats(),
        instance.indices.borrow().len() as u64,
        instance.ceiling.suppressed_records,
    );
    Ok(Some(CallResult {
        instance: instance.clone(),
        body: ResultBody::Cleanup {
            report: Box::new(original),
            _code: retained,
        },
        usage: Default::default(),
        committed: true,
        poisoned: true,
        allocations: data.allocator.stats(),
        canonical_indices: instance.indices.borrow().len() as u64,
        tasks: Default::default(),
        report: Some(projection),
    }))
}

/// # Safety
/// Parent operation validates the exact live instance/output/error objects.
pub(super) unsafe fn close(
    instance: *mut ffi::Instance,
    output: *mut *mut ffi::Result,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: native caller retains initialized fixed output objects.
    if instance.is_null() || output.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: caller supplies this gateway's exact live instance.
    let value = unsafe { &(*instance.cast::<InstanceHandle>()).0 };
    // SAFETY: dependent handle keeps original metadata alive.
    let owner = match unsafe { owner(&value.engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if owner.active.get() {
        return BUSY;
    }
    let Some(after_close) = owner.handles.get().checked_sub(1) else {
        return INVALID;
    };
    // SAFETY: no active scope borrows mutable execution data.
    let data = unsafe { &mut *owner.data.as_ptr() };
    let mut budget = m::Budget::new(data.limits.preparation);
    let claims =
        match data
            .values
            .held_claims(value.state.borrow().holder(), &data.allocator, &mut budget)
        {
            Ok(v) => v,
            Err(_) => return BUSY,
        };
    owner.active.set(true);
    let _active = Active(&owner.active);
    let result = match retire(value, data, &claims, &mut budget) {
        Ok(v) => v,
        Err(s) => return s,
    };
    value.closed.set(true);
    value.leases.borrow_mut().clear();
    let status = if let Some(result) = result {
        // The consumed instance handle supplies the failure result's slot.
        owner.handles.set(after_close + 1);
        // SAFETY: result owns all immutable metadata/code through release.
        unsafe { output.write(Box::into_raw(Box::new(result)).cast()) };
        1
    } else {
        owner.handles.set(after_close);
        // SAFETY: successful close has no failure result.
        unsafe { output.write(ptr::null_mut()) };
        OK
    };
    // SAFETY: admitted close consumes the exact unique instance handle once.
    unsafe { drop(Box::from_raw(instance.cast::<InstanceHandle>())) };
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_projection_preserves_ordered_counts_usage_and_bounded_omission() {
        let mut budget = m::Budget::new(Default::default());
        let mut cleanup = c::Session::prepare(NonZeroUsize::new(80).unwrap(), &mut budget).unwrap();
        let mut cleaner = c::Contextual::new(|_: &mut (), _, _: &mut c::Context<'_>| Err(19));
        for slot in 0..80 {
            assert!(
                cleanup
                    .retire(c::Slot(slot), (), None, c::Reason::Explicit, &mut cleaner)
                    .is_err()
            );
        }
        let raw = cleanup.into_report();
        assert_eq!(raw.secondary().len(), 79);
        for maximum in [0, 3, 64] {
            let (projected, bytes) = cleanup_projection(&raw, Default::default(), 23, maximum);
            assert_eq!(projected.primary.category, wire::Category::Destructor);
            assert_eq!(projected.primary.phase, wire::Phase::Cleanup);
            assert!(projected.poisoned);
            assert_eq!(projected.suppressed.len(), maximum as usize);
            assert_eq!(projected.omitted, 79 - maximum);
            assert_eq!(projected.usage.canonical_indices, 23);
            let mut session = wire::Session::new(Default::default());
            assert_eq!(
                wire::decode_report(&bytes, &mut session).unwrap(),
                projected
            );
        }
        assert_eq!(raw.secondary().len(), 79);
    }

    #[test]
    fn cleanup_projection_preserves_host_contract_and_destructor_order() {
        let mut budget = m::Budget::new(Default::default());
        let mut cleanup = c::Session::prepare(NonZeroUsize::new(4).unwrap(), &mut budget).unwrap();
        cleanup.execution(c::ExecutionFailure::Invariant, None);
        let mut destructor = c::Contextual::new(|_: &mut (), _, _: &mut c::Context<'_>| Err(19));
        assert!(
            cleanup
                .retire(c::Slot(0), (), None, c::Reason::Explicit, &mut destructor)
                .is_err()
        );
        let mut panicking =
            c::Contextual::new(|_: &mut (), _, _: &mut c::Context<'_>| -> Result<(), u32> {
                panic!("contained cleanup panic")
            });
        assert!(
            cleanup
                .retire(c::Slot(1), (), None, c::Reason::Explicit, &mut panicking)
                .is_err()
        );
        let (report, bytes) = cleanup_projection(&cleanup.into_report(), Default::default(), 3, 3);
        assert_eq!(report.primary.category, wire::Category::HostContract);
        assert_eq!(report.primary.phase, wire::Phase::Cleanup);
        assert_eq!(
            report
                .suppressed
                .iter()
                .map(|r| r.category)
                .collect::<Vec<_>>(),
            [wire::Category::Destructor, wire::Category::HostContract]
        );
        assert_eq!(report.omitted, 0);
        assert_eq!(
            wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
            report
        );
    }
}
