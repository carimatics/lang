use super::*;

#[test]
fn local_action_admission_bounds_utf8_grants_and_hostile_inputs() {
    let id = "a".repeat(32);
    let reason = "é".repeat(128);
    let e = Enrollment::new(B, &id, 2, &reason).unwrap();
    let p = CredentialProvision::new(&large_token(), &id, 2, &reason).unwrap();
    let d = Deactivation::new(B, &id, 2, &reason).unwrap();
    let r = OwnerRecovery::new("answer", B, &id, 2, 1, &reason).unwrap();
    assert!(Enrollment::new(B, &id, 2, &(reason.clone() + "x")).is_err());
    assert!(CredentialProvision::new(&large_token(), &id, 2, &(reason.clone() + "x")).is_err());
    assert!(Deactivation::new(B, &id, 2, &(reason.clone() + "x")).is_err());
    assert!(OwnerRecovery::new("answer", B, &id, 2, 1, &(reason + "x")).is_err());
    let mut too_many: authority::codec::Token = records::decode(&large_token(), 64 * 1024).unwrap();
    too_many.grants.push(authority::codec::Grant {
        action: authority::codec::Action::Publish,
        name: "z".into(),
    });
    assert!(
        CredentialProvision::new(
            &records::encode(&too_many, 64 * 1024).unwrap(),
            &id,
            2,
            "too many grants"
        )
        .is_err()
    );
    let long_lived = changed_token(|t| t["expires"] = (100 + authority::MAX_LIFETIME).into());
    assert!(CredentialProvision::new(&long_lived, &id, 2, "exact lifetime").is_ok());
    assert_eq!(
        Enrollment::admit(&vec![b' '; 2049]).unwrap_err().kind,
        ErrorKind::Limit
    );
    assert_eq!(
        Deactivation::admit(&vec![b' '; 2049]).unwrap_err().kind,
        ErrorKind::Limit
    );
    assert_eq!(
        OwnerRecovery::admit(&vec![b' '; 2049]).unwrap_err().kind,
        ErrorKind::Limit
    );
    assert_eq!(
        CredentialProvision::admit(&vec![b' '; 65537])
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    assert_eq!(
        CredentialProvision::admit(&vec![b' '; 65536])
            .unwrap_err()
            .kind,
        ErrorKind::Malformed
    );
    let mut random = 0x41b5_49d1u32;
    for (mode, seed) in [
        ("enroll", e.as_bytes()),
        ("provision", p.as_bytes()),
        ("deactivate", d.as_bytes()),
        ("recover", r.as_bytes()),
    ] {
        for iteration in 0..256 {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let mut bytes = seed.to_vec();
            let i = random as usize % bytes.len();
            match iteration % 4 {
                0 => bytes[i] ^= (random >> 24) as u8 | 1,
                1 => bytes.truncate(i),
                2 => bytes.insert(i, b'{'),
                _ => bytes.extend_from_slice(b"null"),
            }
            let result = match mode {
                "enroll" => Enrollment::admit(&bytes).map(|a| a.as_bytes().to_vec()),
                "provision" => CredentialProvision::admit(&bytes).map(|a| a.as_bytes().to_vec()),
                "deactivate" => Deactivation::admit(&bytes).map(|a| a.as_bytes().to_vec()),
                "recover" => OwnerRecovery::admit(&bytes).map(|a| a.as_bytes().to_vec()),
                _ => unreachable!(),
            };
            match result {
                Ok(canonical) => assert_eq!(canonical, bytes),
                Err(error) => assert!(error.to_string().len() < 128),
            }
        }
    }
}
