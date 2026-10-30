#![no_main]
use ed25519_dalek::{Signer, SigningKey};
use lang::toolchain::{self as t, Channel, Request, Target, TrustedRoot};
use libfuzzer_sys::fuzz_target;
use serde_json::json;

const NOW: u64 = 1_800_000_000;
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fuzz_target!(|bytes: &[u8]| {
    let _ = TrustedRoot::decode(bytes, NOW);
    if bytes.len() > t::MAX_METADATA_BYTES {
        return;
    }
    // Test-only signing authority derived from fuzz input, never a release
    // credential. Signing arbitrary payloads reaches validation behind the
    // real signature boundary without weakening the production verifier.
    let hash = t::digest(bytes);
    let seed = std::array::from_fn(|i| u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).unwrap());
    let key = SigningKey::from_bytes(&seed);
    let public = key.verifying_key().to_bytes();
    let id = t::digest(&public);
    let root = serde_json::to_vec(
        &json!({"schema":1,"repository":"test.lang","expires":NOW+86400,
        "threshold":1,"keys":[{"id":id,"public_key":hex(&public)}]}),
    )
    .unwrap();
    let root = TrustedRoot::decode(&root, NOW).unwrap();
    let request = || Request {
        channel: Channel::Stable,
        target: Target::Macos,
        now: NOW,
        previous: None,
    };
    let _ = t::verify(&root, bytes, request());
    let Ok(payload) = std::str::from_utf8(bytes) else {
        return;
    };
    let mut message = t::SIGNING_DOMAIN.to_vec();
    message.extend_from_slice(bytes);
    let envelope = serde_json::to_vec(&json!({"schema":1,"payload":payload,
        "signatures":[{"key_id":id,"signature":hex(&key.sign(&message).to_bytes())}]}))
    .unwrap();
    if let Ok(admitted) = t::verify(&root, &envelope, request()) {
        let repeated = t::verify(
            &root,
            &envelope,
            Request {
                previous: Some(admitted.admission()),
                ..request()
            },
        )
        .unwrap();
        assert_eq!(admitted.admission(), repeated.admission());
        assert_eq!(admitted.payload(), repeated.payload());
        for release in &admitted.payload().releases {
            let _ = release.identity().unwrap();
            for file in &release.files {
                let _ = t::verify_file(file, bytes);
            }
        }
    }
});
