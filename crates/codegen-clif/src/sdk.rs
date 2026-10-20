//! Shared identity of the compiler and runtime SDK, not native authentication.
pub mod runtime;
use sha2::{Digest, Sha256};
pub const RUSTC_RELEASE: &str = env!("LANG_SDK_RUSTC_RELEASE");
pub const RUSTC_COMMIT: &str = env!("LANG_SDK_RUSTC_COMMIT");
pub const BUILD_PROFILE: &str = env!("LANG_SDK_PROFILE");
pub const FEATURE_POLICY: &str = "workspace-fixed";
pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn build_id(target: &crate::Triple) -> String {
    let mut input = env!("LANG_SDK_SEED").as_bytes().to_vec();
    input.extend_from_slice(target.to_string().as_bytes());
    digest(&input)
}
