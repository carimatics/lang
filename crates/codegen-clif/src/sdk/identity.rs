//! Pinned-Cargo compiled identity, deliberately separate from source hashing.
struct BuildIdentity;

pub(crate) fn compiled_identity() -> String {
    // TypeId's representation is opaque and compiler-specific. The SDK already
    // includes the exact rustc commit; this value is never a stable wire field.
    // Cargo's crate disambiguator includes effective profile/dependency inputs.
    format!("{:?}", std::any::TypeId::of::<BuildIdentity>())
}
