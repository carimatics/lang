use super::*;

pub(super) fn checkpoint(stage: &'static str) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static STEP: AtomicUsize = AtomicUsize::new(0);
    if std::env::var("LANG_M5_CACHE_CHILD").as_deref() != Ok("1") {
        return;
    }
    let step = STEP.fetch_add(1, Ordering::SeqCst) + 1;
    println!("CACHE_CHECKPOINT {step} {stage}");
    let cut = std::env::var("LANG_M5_CACHE_CUT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok());
    if cut == Some(step) {
        std::process::exit(91)
    }
}

pub(super) fn after_root() -> Result<(), Error> {
    if std::env::var("LANG_M5_CACHE_CHILD").as_deref() == Ok("1")
        && std::env::var("LANG_M5_CACHE_LATE_ERROR").as_deref() == Ok("1")
    {
        return Err(Error {
            outcome: PublicationOutcome::Unknown,
            ..io()
        });
    }
    Ok(())
}

#[test]
fn physical_peak_and_checked_overflow_boundaries() {
    let mut view = storage::Inventory::default();
    view.bytes = storage::MAX_BYTES - 10;
    view.entries = storage::MAX_ENTRIES - 1;
    assert!(view.reserve(10, 1).is_ok());
    assert!(view.reserve(11, 1).is_err());
    assert!(view.reserve(10, 2).is_err());
    view.bytes = storage::MAX_BYTES;
    view.entries = storage::MAX_ENTRIES;
    assert!(view.reserve(0, 0).is_ok());
    assert!(view.reserve(u64::MAX, 0).is_err());
    assert!(view.reserve(0, usize::MAX).is_err());
}

#[test]
fn derived_identity_index_refuses_growth_but_retains_exact_reuse_at_capacity() {
    let lock = include_str!("../../../../tests/fixtures/package-lock/schema3-registry.json")
        .replace("<compiler_build_id>", &"0".repeat(64));
    let mut reference = references(lock.as_bytes()).unwrap().remove(0);
    let mut view = storage::Inventory::default();
    for i in 0..storage::MAX_ENTRIES {
        let mut locator = reference.locator.clone();
        if i != 0 {
            locator.name = format!("package{i}");
        }
        view.bindings
            .insert(locator, reference.archive_sha256.clone());
    }
    assert!(view.bind(&reference).is_ok());
    reference.locator.name = "one_more".into();
    assert_eq!(view.bind(&reference).unwrap_err().kind, ErrorKind::Limit);
    reference.locator.name = "math".into();
    reference.archive_sha256 = "ff".repeat(32);
    assert_eq!(view.bind(&reference).unwrap_err().kind, ErrorKind::Conflict);
    assert_eq!(view.bindings.len(), storage::MAX_ENTRIES);
}

#[test]
fn generation_archive_budget_charges_all_retained_identities_before_reading() {
    let lock = include_str!("../../../../tests/fixtures/package-lock/schema3-registry.json")
        .replace("<compiler_build_id>", &"0".repeat(64));
    let mut refs: Vec<_> = [18usize, 18, 18, 10]
        .into_iter()
        .map(|mib| {
            let mut reference = references(lock.as_bytes()).unwrap().remove(0);
            reference.archive_bytes = mib * 1024 * 1024;
            reference
        })
        .collect();
    assert!(check_generation_bytes(&refs).is_ok());
    refs[3].archive_bytes += 1;
    assert_eq!(
        check_generation_bytes(&refs).unwrap_err().kind,
        ErrorKind::Limit
    );
    refs[3].archive_bytes = usize::MAX;
    assert_eq!(
        check_generation_bytes(&refs).unwrap_err().kind,
        ErrorKind::Limit
    );
}

#[test]
fn independent_marker_and_generation_bytes_match() {
    assert_eq!(FORMAT.len(), 53);
    assert_eq!(
        digest(FORMAT),
        "60488014b120ab4f7b157fb97abe6af19c7efc12b80976d43d8740083f5c368f"
    );
    let lock = include_str!("../../../../tests/fixtures/package-lock/schema3-registry.json")
        .replace("<compiler_build_id>", &"0".repeat(64));
    assert_eq!(lock.len(), 3664);
    assert_eq!(
        digest(lock.as_bytes()),
        "03bf5a6f4df5180f7915ea02bab6e44693bdae4fbdf787c44ad3c97d69781360"
    );
    let refs = references(lock.as_bytes()).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].locator.name, "math");
    assert_eq!(refs[0].archive_bytes, 4608);
}
