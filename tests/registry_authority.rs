use std::io::{self, Read};

use lang::registry::authority::{
    Actor, Authority, Bearer, Error, ErrorKind, MAX_BYTES, MAX_DEPTH, MAX_GRANTS, MAX_LIFETIME,
    MAX_OWNERS, MAX_PACKAGES, MAX_PRINCIPALS, MAX_TIME, MAX_TOKENS, Operation, ServiceIdentity,
};
use serde::Serialize;

const PRINCIPAL: &str = "00000000000000000000000000000001";
const SECOND: &str = "00000000000000000000000000000002";
const HASH: &str = "3dec5dc6d20c715eada8190369186e6884d3b159da462c13cc112a2a76eff05e";

#[derive(Clone, Serialize)]
struct Principal {
    id: String,
    active: bool,
}
#[derive(Clone, Serialize)]
struct Grant {
    action: String,
    name: String,
}
#[derive(Clone, Serialize)]
struct Token {
    id: String,
    principal: String,
    registry: String,
    origin: String,
    authenticator_sha256: String,
    issued: u64,
    expires: u64,
    revoked: bool,
    grants: Vec<Grant>,
}
#[derive(Clone, Serialize)]
struct Package {
    name: String,
    owners: Vec<String>,
}
#[derive(Clone, Serialize)]
struct Fixture {
    format: String,
    schema: u32,
    registry: String,
    origin: String,
    time_floor: u64,
    principals: Vec<Principal>,
    tokens: Vec<Token>,
    packages: Vec<Package>,
}
fn identity() -> ServiceIdentity {
    ServiceIdentity::new("example", "https://registry.example").unwrap()
}
fn bearer() -> Bearer {
    Bearer::from_input("11".repeat(32).as_bytes()).unwrap()
}
fn grant(action: &str, name: &str) -> Grant {
    Grant {
        action: action.into(),
        name: name.into(),
    }
}
fn fixture() -> Fixture {
    Fixture {
        format: "lang-registry-authority".into(),
        schema: 1,
        registry: "example".into(),
        origin: "https://registry.example".into(),
        time_floor: 100,
        principals: vec![
            Principal {
                id: PRINCIPAL.into(),
                active: true,
            },
            Principal {
                id: SECOND.into(),
                active: true,
            },
        ],
        tokens: vec![Token {
            id: "01".repeat(16),
            principal: PRINCIPAL.into(),
            registry: "example".into(),
            origin: "https://registry.example".into(),
            authenticator_sha256: HASH.into(),
            issued: 100,
            expires: 200,
            revoked: false,
            grants: vec![grant("publish", "parser")],
        }],
        packages: vec![Package {
            name: "parser".into(),
            owners: vec![PRINCIPAL.into(), SECOND.into()],
        }],
    }
}
fn encode(f: &Fixture) -> Vec<u8> {
    let mut b = serde_json::to_vec_pretty(f).unwrap();
    b.push(b'\n');
    b
}
fn admit(f: &Fixture) -> Authority {
    Authority::admit(&encode(f), &identity()).unwrap()
}
fn assert_kind<T: std::fmt::Debug>(r: Result<T, Error>, kind: ErrorKind) {
    assert_eq!(r.unwrap_err().kind, kind);
}
fn policy(f: &Fixture, now: u64, op: Operation<'_>) -> Result<String, Error> {
    admit(f)
        .authorize(&bearer(), now, op)
        .map(|a| a.principal().to_owned())
}
fn actor_ids(actor: Actor<'_>) {
    assert_eq!(actor.principal(), PRINCIPAL);
    assert_eq!(actor.credential_id(), "01".repeat(16));
}

#[test]
fn exact_secret_input_independent_hash_and_owned_inventory() {
    let secret = bearer();
    // Independently computed with Node's SHA-256, not the implementation helper.
    assert_eq!(secret.authenticator_sha256(), HASH);
    assert_eq!(
        Bearer::from_input(format!("{}\n", "11".repeat(32)).as_bytes())
            .unwrap()
            .authenticator_sha256(),
        HASH
    );
    let mut bytes = encode(&fixture());
    let authority = Authority::admit(&bytes, &identity()).unwrap();
    bytes.fill(0);
    actor_ids(
        authority
            .authorize(&secret, 150, Operation::Publish("parser"))
            .unwrap(),
    );
    assert!(!format!("{secret:?}{authority:?}").contains(HASH));
    assert!(!format!("{secret:?}{authority:?}").contains(&"11".repeat(32)));
    for input in [
        "1".repeat(63),
        "1".repeat(65),
        "1".repeat(66),
        "AA".repeat(32),
        format!("{}\r\n", "11".repeat(32)),
        format!(" {}", "11".repeat(32)),
        format!("{}\n\n", "11".repeat(32)),
        "é".repeat(32),
    ] {
        let e = Bearer::from_input(input.as_bytes()).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Authentication);
        assert!(!format!("{e}{e:?}").contains(&input));
    }
}

#[test]
fn bounded_stream_and_io_errors_never_echo_secret_material() {
    struct Endless(usize);
    impl Read for Endless {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            b.fill(b'1');
            self.0 += b.len();
            Ok(b.len())
        }
    }
    let mut source = Endless(0);
    assert_kind(Bearer::read_input(&mut source), ErrorKind::Authentication);
    assert_eq!(source.0, 66);
    assert_eq!(
        Bearer::read_input(format!("{}\n", "11".repeat(32)).as_bytes())
            .unwrap()
            .authenticator_sha256(),
        HASH
    );
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("SECRET should not escape"))
        }
    }
    let error = Bearer::read_input(Broken).unwrap_err();
    assert_eq!(error.code(), "REG0008");
    assert!(!format!("{error:?}{error}").contains("SECRET"));
}

#[test]
fn service_identity_is_independent_and_canonical() {
    for origin in [
        "https://registry.example",
        "https://registry.example:8443",
        "https://registry.0xyz",
        "https://127.0.0.1",
        "https://127.0.0.1:1",
        "https://r:65535",
    ] {
        ServiceIdentity::new("example", origin).unwrap();
    }
    for origin in [
        "http://registry.example",
        "https://REGISTRY.example",
        "https://registry.example/",
        "https://registry.example:443",
        "https://registry.example:0444",
        "https://registry.example:0",
        "https://registry.example:65536",
        "https://registry.example.",
        "https://user@registry.example",
        "https://registry.example?x",
        "https://registry.example#x",
        "https://[::1]",
        "https://127.1",
        "https://2130706433",
        "https://0177.0.0.1",
        "https://0x7f000001",
        "https://127.0.0.01",
        "https://é.example",
        "https://xn--test.example",
        "https://a..example",
        "https://-bad.example",
        "https://bad-.example",
    ] {
        assert!(ServiceIdentity::new("example", origin).is_err(), "{origin}");
    }
    assert!(ServiceIdentity::new("Example", "https://registry.example").is_err());
    assert_kind(
        Authority::admit(
            &encode(&fixture()),
            &ServiceIdentity::new("other", "https://registry.example").unwrap(),
        ),
        ErrorKind::Identity,
    );
    assert_kind(
        Authority::admit(
            &encode(&fixture()),
            &ServiceIdentity::new("example", "https://other.example").unwrap(),
        ),
        ErrorKind::Identity,
    );
    let mut f = fixture();
    f.tokens[0].origin = "https://other.example".into();
    assert_kind(
        Authority::admit(&encode(&f), &identity()),
        ErrorKind::Malformed,
    );
}

#[test]
fn current_authentication_and_exact_time_boundaries_precede_policy() {
    for now in [100, 199] {
        assert!(policy(&fixture(), now, Operation::Publish("parser")).is_ok());
    }
    for now in [200, MAX_TIME] {
        assert_kind(
            policy(&fixture(), now, Operation::Publish("parser")),
            ErrorKind::Authentication,
        );
    }
    for now in [99, MAX_TIME + 1] {
        assert_kind(
            policy(&fixture(), now, Operation::Publish("parser")),
            ErrorKind::State,
        );
    }
    let mut f = fixture();
    f.time_floor = 0;
    assert_kind(
        policy(&f, 99, Operation::Publish("parser")),
        ErrorKind::Authentication,
    );
    f.tokens[0].revoked = true;
    assert_kind(
        policy(&f, 150, Operation::Publish("parser")),
        ErrorKind::Authentication,
    );
    f.tokens[0].revoked = false;
    f.principals[0].active = false;
    assert_kind(
        policy(&f, 150, Operation::ReadOwners("parser")),
        ErrorKind::Authentication,
    );
    assert_kind(
        admit(&fixture()).authorize(
            &Bearer::from_input("22".repeat(32).as_bytes()).unwrap(),
            150,
            Operation::Publish("unknown"),
        ),
        ErrorKind::Authentication,
    );
    let mut f = fixture();
    f.tokens[0].expires = f.tokens[0].issued + MAX_LIFETIME;
    admit(&f);
    f.tokens[0].expires += 1;
    assert_kind(
        Authority::admit(&encode(&f), &identity()),
        ErrorKind::Malformed,
    );
    f.tokens[0].expires = f.tokens[0].issued;
    assert_kind(
        Authority::admit(&encode(&f), &identity()),
        ErrorKind::Malformed,
    );
}

#[test]
fn each_operation_requires_its_own_scope_and_current_owner() {
    for (action, op) in [
        ("publish", Operation::Publish("parser")),
        ("yank", Operation::Yank("parser")),
        ("owners", Operation::ChangeOwners("parser")),
    ] {
        let mut f = fixture();
        f.tokens[0].grants = vec![grant(action, "parser")];
        assert!(policy(&f, 150, op).is_ok());
        f.packages[0].owners = vec![SECOND.into()];
        assert_kind(policy(&f, 150, op), ErrorKind::Authorization);
        f.packages[0].owners = vec![PRINCIPAL.into()];
        f.tokens[0].grants = vec![grant("admin", "parser")];
        assert_kind(policy(&f, 150, op), ErrorKind::Authorization);
    }
    let mut f = fixture();
    f.packages[0].owners = vec![SECOND.into()];
    f.tokens[0].grants = vec![grant("admin", "parser")];
    assert!(policy(&f, 150, Operation::Admin("parser")).is_ok());
    assert!(policy(&f, 150, Operation::ReadOwners("parser")).is_ok());
    assert_kind(
        policy(&fixture(), 150, Operation::Admin("parser")),
        ErrorKind::Authorization,
    );
    assert!(policy(&fixture(), 150, Operation::ReadOwners("parser")).is_ok());
    f.tokens[0].grants = vec![grant("publish", "parser")];
    assert_kind(
        policy(&f, 150, Operation::ReadOwners("parser")),
        ErrorKind::Authorization,
    );
    for op in [
        Operation::Publish("other"),
        Operation::ReadOwners("other"),
        Operation::Admin("other"),
    ] {
        assert_kind(policy(&fixture(), 150, op), ErrorKind::Authorization);
    }
    assert_kind(
        policy(&fixture(), 150, Operation::Publish("../parser")),
        ErrorKind::Malformed,
    );
}

#[test]
fn claiming_requires_both_exact_grants_and_is_not_existing_publication() {
    let mut f = fixture();
    f.packages.clear();
    assert_kind(
        policy(&f, 150, Operation::Claim("parser")),
        ErrorKind::Authorization,
    );
    f.tokens[0].grants = vec![grant("claim", "parser")];
    assert_kind(
        policy(&f, 150, Operation::Claim("parser")),
        ErrorKind::Authorization,
    );
    f.tokens[0].grants.push(grant("publish", "parser"));
    assert!(policy(&f, 150, Operation::Claim("parser")).is_ok());
    assert_kind(
        policy(&f, 150, Operation::Publish("parser")),
        ErrorKind::Authorization,
    );
    assert_kind(
        policy(&f, 150, Operation::Claim("other")),
        ErrorKind::Authorization,
    );
    f.packages = fixture().packages;
    assert_kind(
        policy(&f, 150, Operation::Claim("parser")),
        ErrorKind::Authorization,
    );
}

#[test]
fn fresh_views_refuse_revoked_removed_or_expired_preflight_identity() {
    let mut f = fixture();
    assert!(policy(&f, 150, Operation::Publish("parser")).is_ok());
    f.tokens[0].revoked = true;
    assert_kind(
        policy(&f, 150, Operation::Publish("parser")),
        ErrorKind::Authentication,
    );
    f.tokens[0].revoked = false;
    f.packages[0].owners = vec![SECOND.into()];
    assert_kind(
        policy(&f, 150, Operation::Publish("parser")),
        ErrorKind::Authorization,
    );
    f.packages[0].owners = vec![PRINCIPAL.into()];
    assert_kind(
        policy(&f, 200, Operation::Publish("parser")),
        ErrorKind::Authentication,
    );
    // This exercises pure reevaluation; storage locking and replay are D02.
}

#[test]
fn closed_canonical_records_and_cross_references_are_required() {
    let bytes = encode(&fixture());
    for text in [
        String::from_utf8(bytes.clone())
            .unwrap()
            .replace("\"schema\": 1", "\"schema\": 1.0"),
        String::from_utf8(bytes.clone())
            .unwrap()
            .replace("\"schema\": 1", "\"schema\": 1,\n  \"schema\": 1"),
        String::from_utf8(bytes.clone())
            .unwrap()
            .replace("\"schema\": 1", "\"schema\": 1,\n  \"surprise\": 1"),
        String::from_utf8(bytes.clone())
            .unwrap()
            .trim_end()
            .to_owned(),
        serde_json::to_string(&fixture()).unwrap(),
        String::from_utf8(bytes)
            .unwrap()
            .replace("example", "\\u0065xample"),
    ] {
        assert_kind(
            Authority::admit(text.as_bytes(), &identity()),
            ErrorKind::Malformed,
        );
    }
    let mutations: [fn(&mut Fixture); 15] = [
        |f| f.schema = 2,
        |f| f.format = "other".into(),
        |f| f.principals.reverse(),
        |f| f.principals[1] = f.principals[0].clone(),
        |f| f.tokens[0].principal = "ff".repeat(16),
        |f| f.tokens[0].authenticator_sha256 = "AA".repeat(32),
        |f| f.tokens[0].grants.clear(),
        |f| {
            let duplicate = f.tokens[0].grants[0].clone();
            f.tokens[0].grants.push(duplicate);
        },
        |f| f.tokens[0].grants[0].action = "*".into(),
        |f| f.packages[0].owners.clear(),
        |f| f.packages[0].owners.reverse(),
        |f| f.packages[0].owners[0] = "ff".repeat(16),
        |f| f.principals.iter_mut().for_each(|p| p.active = false),
        |f| f.time_floor = MAX_TIME + 1,
        |f| {
            let mut t = f.tokens[0].clone();
            t.id = "02".repeat(16);
            f.tokens.push(t);
        },
    ];
    for change in mutations {
        let mut f = fixture();
        change(&mut f);
        assert_kind(
            Authority::admit(&encode(&f), &identity()),
            ErrorKind::Malformed,
        );
    }
}

#[test]
fn collection_limits_are_checked_at_minus_one_equal_and_plus_one() {
    for count in [MAX_PRINCIPALS - 1, MAX_PRINCIPALS, MAX_PRINCIPALS + 1] {
        let mut f = fixture();
        f.principals = (1..=count)
            .map(|i| Principal {
                id: format!("{i:032x}"),
                active: true,
            })
            .collect();
        let r = Authority::admit(&encode(&f), &identity());
        if count > MAX_PRINCIPALS {
            assert_kind(r, ErrorKind::Limit)
        } else {
            r.unwrap();
        }
    }
    for count in [MAX_TOKENS - 1, MAX_TOKENS, MAX_TOKENS + 1] {
        let mut f = fixture();
        let token = f.tokens[0].clone();
        f.tokens = (0..count)
            .map(|i| {
                let mut t = token.clone();
                t.id = format!("{i:032x}");
                t.authenticator_sha256 = format!("{i:064x}");
                t
            })
            .collect();
        f.tokens.last_mut().unwrap().authenticator_sha256 = HASH.into();
        let r = Authority::admit(&encode(&f), &identity());
        if count > MAX_TOKENS {
            assert_kind(r, ErrorKind::Limit)
        } else {
            let view = r.unwrap();
            let secret = bearer();
            assert_eq!(
                view.authorize(&secret, 150, Operation::Publish("parser"))
                    .unwrap()
                    .credential_id(),
                format!("{:032x}", count - 1)
            );
        }
    }
    for count in [MAX_PACKAGES - 1, MAX_PACKAGES, MAX_PACKAGES + 1] {
        let mut f = fixture();
        f.packages = (0..count)
            .map(|i| Package {
                name: format!("pkg_{i:04}"),
                owners: vec![PRINCIPAL.into()],
            })
            .collect();
        let r = Authority::admit(&encode(&f), &identity());
        if count > MAX_PACKAGES {
            assert_kind(r, ErrorKind::Limit)
        } else {
            r.unwrap();
        }
    }
    for count in [MAX_GRANTS - 1, MAX_GRANTS, MAX_GRANTS + 1] {
        let mut f = fixture();
        f.tokens[0].grants = (0..count)
            .map(|i| grant("publish", &format!("pkg_{i:04}")))
            .collect();
        let r = Authority::admit(&encode(&f), &identity());
        if count > MAX_GRANTS {
            assert_kind(r, ErrorKind::Limit)
        } else {
            r.unwrap();
        }
    }
    for count in [MAX_OWNERS - 1, MAX_OWNERS, MAX_OWNERS + 1] {
        let mut f = fixture();
        f.principals = (1..=count)
            .map(|i| Principal {
                id: format!("{i:032x}"),
                active: true,
            })
            .collect();
        f.packages[0].owners = f.principals.iter().map(|p| p.id.clone()).collect();
        let r = Authority::admit(&encode(&f), &identity());
        if count > MAX_OWNERS {
            assert_kind(r, ErrorKind::Limit)
        } else {
            r.unwrap();
        }
    }
}

#[test]
fn total_bytes_nesting_and_deterministic_mutations_are_bounded() {
    for length in [MAX_BYTES - 1, MAX_BYTES, MAX_BYTES + 1] {
        let mut b = encode(&fixture());
        b.resize(length, b' ');
        assert_kind(
            Authority::admit(&b, &identity()),
            if length > MAX_BYTES {
                ErrorKind::Limit
            } else {
                ErrorKind::Malformed
            },
        );
    }
    for depth in [MAX_DEPTH - 1, MAX_DEPTH, MAX_DEPTH + 1] {
        let text = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        assert_kind(
            Authority::admit(text.as_bytes(), &identity()),
            if depth > MAX_DEPTH {
                ErrorKind::Limit
            } else {
                ErrorKind::Malformed
            },
        );
    }
    let original = encode(&fixture());
    for seed in 0..512 {
        let mut bytes = original.clone();
        let at = (seed * 47) % bytes.len();
        bytes[at] ^= (seed % 255 + 1) as u8;
        let before = bytes.clone();
        let _ = Authority::admit(&bytes, &identity());
        assert_eq!(bytes, before);
    }
}
