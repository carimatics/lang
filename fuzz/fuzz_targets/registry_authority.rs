#![no_main]

use lang::registry::authority::{Authority, Bearer, Operation, ServiceIdentity};
use lang::registry::store::{
    CredentialProvision, Deactivation, Enrollment, OwnerAction, OwnerRecovery, QuarantineAction,
    Revocation, YankAction,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let identity = ServiceIdentity::new("example", "https://registry.example").unwrap();
    let bearer = Bearer::from_input(&[b'1'; 64]).unwrap();
    if let Ok(authority) = Authority::admit(bytes, &identity) {
        for time in [0, 150, u64::MAX] {
            let _ = authority.authorize(&bearer, time, Operation::Publish("parser"));
        }
    }
    if let Err(error) = Bearer::from_input(bytes) {
        assert!(error.to_string().len() < 128);
    }
    match YankAction::admit("parser", bytes) {
        Ok(action) => assert_eq!(action.as_bytes(), bytes),
        Err(error) => assert!(error.to_string().len() < 128),
    }
    match OwnerAction::admit("parser", bytes) {
        Ok(action) => assert_eq!(action.as_bytes(), bytes),
        Err(error) => assert!(error.to_string().len() < 128),
    }
    match QuarantineAction::admit("parser", bytes) {
        Ok(action) => assert_eq!(action.as_bytes(), bytes),
        Err(error) => assert!(error.to_string().len() < 128),
    }
    match Revocation::admit(bytes) {
        Ok(action) => assert_eq!(action.as_bytes(), bytes),
        Err(error) => assert!(error.to_string().len() < 128),
    }
    match Enrollment::admit(bytes) {
        Ok(action) => assert_eq!(action.as_bytes(), bytes),
        Err(error) => assert!(error.to_string().len() < 128),
    }
    match CredentialProvision::admit(bytes) {
        Ok(action) => assert_eq!(action.as_bytes(), bytes),
        Err(error) => assert!(error.to_string().len() < 128),
    }
    match Deactivation::admit(bytes) {
        Ok(action) => assert_eq!(action.as_bytes(), bytes),
        Err(error) => assert!(error.to_string().len() < 128),
    }
    match OwnerRecovery::admit(bytes) {
        Ok(action) => assert_eq!(action.as_bytes(), bytes),
        Err(error) => assert!(error.to_string().len() < 128),
    }
});
