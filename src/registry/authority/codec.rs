use super::*;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::marker::PhantomData;
use std::net::Ipv4Addr;

const SEQUENCE_LIMIT: &str = "authority collection limit";

#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Records {
    format: String,
    schema: u32,
    pub registry: String,
    pub origin: String,
    pub time_floor: u64,
    #[serde(deserialize_with = "principals")]
    pub principals: Vec<Principal>,
    #[serde(deserialize_with = "tokens")]
    pub tokens: Vec<Token>,
    #[serde(deserialize_with = "packages")]
    pub packages: Vec<Package>,
}
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Principal {
    pub id: String,
    pub active: bool,
}
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Token {
    pub id: String,
    pub principal: String,
    registry: String,
    origin: String,
    pub authenticator_sha256: String,
    pub issued: u64,
    pub expires: u64,
    pub revoked: bool,
    #[serde(deserialize_with = "grants")]
    pub grants: Vec<Grant>,
}
#[derive(Clone, Copy, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Action {
    Admin,
    Claim,
    Owners,
    Publish,
    Yank,
}
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Grant {
    pub action: Action,
    pub name: String,
}
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Package {
    pub name: String,
    #[serde(deserialize_with = "owners")]
    pub owners: Vec<String>,
}

pub(crate) fn bounded<'de, D, T, const N: usize>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T, const N: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> de::Visitor<'de> for Visitor<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("bounded authority collection")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while values.len() < N {
                let Some(value) = seq.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if seq.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(SEQUENCE_LIMIT));
            }
            Ok(values)
        }
    }
    d.deserialize_seq(Visitor::<T, N>(PhantomData))
}
fn principals<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Principal>, D::Error> {
    bounded::<D, _, MAX_PRINCIPALS>(d)
}
fn tokens<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Token>, D::Error> {
    bounded::<D, _, MAX_TOKENS>(d)
}
fn packages<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Package>, D::Error> {
    bounded::<D, _, MAX_PACKAGES>(d)
}
fn grants<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Grant>, D::Error> {
    bounded::<D, _, MAX_GRANTS>(d)
}
fn owners<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    bounded::<D, _, MAX_OWNERS>(d)
}

pub(crate) fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
pub(crate) fn unhex<const N: usize>(s: &str) -> Result<[u8; N], Error> {
    if s.len() != N * 2 {
        return Err(malformed());
    }
    let mut bytes = [0; N];
    for (pair, byte) in s.as_bytes().as_chunks::<2>().0.iter().zip(&mut bytes) {
        *byte =
            nibble(pair[0]).ok_or_else(malformed)? * 16 + nibble(pair[1]).ok_or_else(malformed)?;
    }
    Ok(bytes)
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| {
            [
                char::from(DIGITS[usize::from(b >> 4)]),
                char::from(DIGITS[usize::from(b & 15)]),
            ]
        })
        .collect()
}

pub(crate) fn valid_origin(s: &str) -> bool {
    let Some(authority) = s.strip_prefix("https://") else {
        return false;
    };
    if authority.is_empty() || authority.len() > 259 {
        return false;
    }
    let host = if let Some((host, port)) = authority.split_once(':') {
        if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) || port.starts_with('0') {
            return false;
        }
        let Ok(port) = port.parse::<u16>() else {
            return false;
        };
        if port == 443 {
            return false;
        }
        host
    } else {
        authority
    };
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    // Numeric host spellings must be the canonical four-octet IPv4 form.
    if host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return host
            .parse::<Ipv4Addr>()
            .is_ok_and(|ip| ip.to_string() == host);
    }
    // A final numeric label could be interpreted as an alternative IPv4 form
    // by URL stacks. Reject it rather than normalizing after authorization.
    if host.rsplit('.').next().is_some_and(|last| {
        last.bytes().all(|b| b.is_ascii_digit())
            || last
                .strip_prefix("0x")
                .is_some_and(|digits| digits.bytes().all(|b| b.is_ascii_hexdigit()))
    }) {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && !label.starts_with("xn--")
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

fn depth(bytes: &[u8]) -> Result<(), Error> {
    let (mut count, mut string, mut escape) = (0usize, false, false);
    for &b in bytes {
        if string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                string = false;
            }
        } else {
            match b {
                b'"' => string = true,
                b'{' | b'[' => {
                    count += 1;
                    if count > MAX_DEPTH {
                        return Err(error(ErrorKind::Limit, "authority nesting limit exceeded"));
                    }
                }
                b'}' | b']' => count = count.checked_sub(1).ok_or_else(malformed)?,
                _ => {}
            }
        }
    }
    Ok(())
}
struct BoundedWriter(Vec<u8>);
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("authority limit"));
        }
        self.0.try_reserve(bytes.len()).map_err(io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Records, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(error(ErrorKind::Limit, "authority byte limit exceeded"));
    }
    depth(bytes)?;
    let records: Records = serde_json::from_slice(bytes).map_err(|e| {
        if e.to_string().starts_with(SEQUENCE_LIMIT) {
            error(ErrorKind::Limit, "authority collection limit exceeded")
        } else {
            malformed()
        }
    })?;
    validate(&records)?;
    let mut canonical = BoundedWriter(Vec::new());
    serde_json::to_writer_pretty(&mut canonical, &records)
        .map_err(|_| error(ErrorKind::Limit, "canonical authority byte limit exceeded"))?;
    canonical
        .write_all(b"\n")
        .map_err(|_| error(ErrorKind::Limit, "canonical authority byte limit exceeded"))?;
    if bytes != canonical.0 {
        return Err(malformed());
    }
    Ok(records)
}

pub(crate) fn validate(r: &Records) -> Result<(), Error> {
    if r.principals.len() > MAX_PRINCIPALS
        || r.tokens.len() > MAX_TOKENS
        || r.packages.len() > MAX_PACKAGES
        || r.tokens.iter().any(|t| t.grants.len() > MAX_GRANTS)
        || r.packages.iter().any(|p| p.owners.len() > MAX_OWNERS)
    {
        return Err(error(
            ErrorKind::Limit,
            "authority collection limit exceeded",
        ));
    }
    if r.format != "lang-registry-authority"
        || r.schema != 1
        || !valid_name(&r.registry)
        || !valid_origin(&r.origin)
        || r.time_floor > MAX_TIME
    {
        return Err(malformed());
    }
    if !r.principals.windows(2).all(|p| p[0].id < p[1].id)
        || !r.tokens.windows(2).all(|p| p[0].id < p[1].id)
        || !r.packages.windows(2).all(|p| p[0].name < p[1].name)
    {
        return Err(malformed());
    }
    let mut principals = BTreeMap::new();
    for p in &r.principals {
        unhex::<16>(&p.id)?;
        principals.insert(p.id.as_str(), p.active);
    }
    let mut hashes = BTreeSet::new();
    for t in &r.tokens {
        unhex::<16>(&t.id)?;
        let hash = unhex::<32>(&t.authenticator_sha256)?;
        if !principals.contains_key(t.principal.as_str())
            || !hashes.insert(hash)
            || t.registry != r.registry
            || t.origin != r.origin
            || t.issued >= t.expires
            || t.expires > MAX_TIME
            || t.expires - t.issued > MAX_LIFETIME
            || t.grants.is_empty()
            || !t
                .grants
                .windows(2)
                .all(|p| (p[0].action, &p[0].name) < (p[1].action, &p[1].name))
            || t.grants.iter().any(|g| !valid_name(&g.name))
        {
            return Err(malformed());
        }
    }
    for p in &r.packages {
        if !valid_name(&p.name)
            || p.owners.is_empty()
            || !p.owners.windows(2).all(|p| p[0] < p[1])
            || p.owners
                .iter()
                .any(|o| !principals.contains_key(o.as_str()))
            || !p
                .owners
                .iter()
                .any(|o| principals.get(o.as_str()) == Some(&true))
        {
            return Err(malformed());
        }
    }
    Ok(())
}
