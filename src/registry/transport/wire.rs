use super::*;
use std::io::Read;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorBody {
    schema: u32,
    code: String,
    message: String,
    #[serde(deserialize_with = "nullable_id")]
    request_id: Option<String>,
}

fn nullable_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    serde::Deserialize::deserialize(d)
}

pub(super) fn error_body(bytes: &[u8], status: u16) -> Result<Error, Error> {
    error_body_for(bytes, status, None)
}

pub(super) fn error_body_for(
    bytes: &[u8],
    status: u16,
    expected_id: Option<&str>,
) -> Result<Error, Error> {
    let body: ErrorBody = serde_json::from_slice(bytes).map_err(|_| malformed())?;
    let valid_code = match status {
        400 => body.code == "REG0001",
        401 => body.code == "REG0003",
        403 => body.code == "REG0004",
        404 | 429 | 500 => body.code == "REG0008",
        409 => matches!(body.code.as_str(), "REG0005" | "REG0006" | "REG0007"),
        413 => body.code == "REG0002",
        _ => false,
    };
    if body.schema != 1
        || !valid_code
        || body.message.chars().any(char::is_control)
        || body.request_id.as_ref().is_some_and(|id| {
            id.len() != 32
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        || expected_id
            .is_some_and(|expected| body.request_id.as_deref().is_some_and(|id| id != expected))
    {
        return Err(malformed());
    }
    Ok(error(match (status, body.code.as_str()) {
        (400, _) => ErrorKind::Malformed,
        (401, _) => ErrorKind::Authentication,
        (403, _) => ErrorKind::Authorization,
        (404, _) => ErrorKind::NotFound,
        (409, "REG0006") => ErrorKind::Integrity,
        (409, "REG0007") => ErrorKind::State,
        (409, _) => ErrorKind::Conflict,
        (413, _) => ErrorKind::Limit,
        (429, _) => ErrorKind::Busy,
        _ => ErrorKind::Transport,
    }))
}

/// Read a bounded header block without consuming any body bytes. httparse does
/// the actual HTTP grammar parsing; this scanner enforces byte/CRLF limits.
pub(super) fn head(
    reader: &mut impl Read,
    end: Instant,
) -> Result<zeroize::Zeroizing<Vec<u8>>, Error> {
    let mut bytes = zeroize::Zeroizing::new(Vec::with_capacity(MAX_LINE_BYTES + MAX_HEADER_BYTES));
    let mut line_end = None;
    loop {
        remaining(end)?;
        let maximum = line_end.map_or(MAX_LINE_BYTES, |line_end| line_end + MAX_HEADER_BYTES);
        if bytes.len() == maximum {
            return Err(limit());
        }
        let mut next = [0];
        reader.read_exact(&mut next)?;
        if (next[0] == b'\n' && bytes.last() != Some(&b'\r'))
            || (bytes.last() == Some(&b'\r') && next[0] != b'\n')
        {
            return Err(malformed());
        }
        bytes.push(next[0]);
        if line_end.is_none() && bytes.ends_with(b"\r\n") {
            line_end = Some(bytes.len());
        }
        if bytes.ends_with(b"\r\n\r\n") {
            return Ok(bytes);
        }
    }
}

fn one<'a>(headers: &[httparse::Header<'a>], name: &str) -> Result<Option<&'a [u8]>, Error> {
    let mut matching = headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case(name));
    let value = matching.next().map(|header| header.value);
    if matching.next().is_some() {
        return Err(malformed());
    }
    Ok(value)
}

fn framing(headers: &[httparse::Header<'_>]) -> Result<usize, Error> {
    for name in ["transfer-encoding", "content-encoding"] {
        if one(headers, name)?.is_some() {
            return Err(malformed());
        }
    }
    for name in [
        "authorization",
        "if-match",
        "if-none-match",
        "idempotency-key",
        "host",
        "content-type",
    ] {
        one(headers, name)?;
    }
    if !one(headers, "connection")?.is_some_and(|value| value.eq_ignore_ascii_case(b"close")) {
        return Err(malformed());
    }
    let length = one(headers, "content-length")?.ok_or_else(malformed)?;
    if length.is_empty() || !length.iter().all(u8::is_ascii_digit) {
        return Err(malformed());
    }
    length.iter().try_fold(0usize, |n, digit| {
        n.checked_mul(10)
            .and_then(|n| n.checked_add((digit - b'0') as usize))
            .ok_or_else(limit)
    })
}

pub(super) enum Request<'a> {
    Index(&'a str),
    Archive(&'a str),
    Owners {
        name: &'a str,
        bearer: super::super::authority::Bearer,
    },
    Action {
        name: &'a str,
        bearer: super::super::authority::Bearer,
        length: usize,
    },
    Publish {
        name: &'a str,
        version: &'a str,
        bearer: super::super::authority::Bearer,
        request_id: &'a str,
        expected: store::ExpectedRevision,
        length: usize,
    },
}

pub(super) fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn revision(bytes: &[u8]) -> Result<u64, Error> {
    let s = std::str::from_utf8(bytes).map_err(|_| malformed())?;
    let s = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(malformed)?;
    if s.is_empty() || s.starts_with('0') || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(malformed());
    }
    s.parse().map_err(|_| malformed())
}

fn bearer(headers: &[httparse::Header<'_>]) -> Result<super::super::authority::Bearer, Error> {
    let token = one(headers, "authorization")?
        .and_then(|value| value.strip_prefix(b"Bearer "))
        .filter(|value| value.len() == 64)
        .ok_or_else(|| error(ErrorKind::Authentication))?;
    super::super::authority::Bearer::from_input(token).map_err(|_| error(ErrorKind::Authentication))
}

pub(super) fn request<'a>(bytes: &'a [u8], expected_host: &str) -> Result<Request<'a>, Error> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADER_FIELDS];
    let mut parsed = httparse::Request::new(&mut headers);
    if parsed.parse(bytes).map_err(|e| match e {
        httparse::Error::TooManyHeaders => limit(),
        _ => malformed(),
    })? != httparse::Status::Complete(bytes.len())
        || parsed.version != Some(1)
        || !matches!(parsed.method, Some("GET" | "PUT" | "POST"))
        || !(bytes.starts_with(b"GET ")
            || bytes.starts_with(b"PUT ")
            || bytes.starts_with(b"POST "))
    {
        return Err(malformed());
    }
    let length = framing(parsed.headers)?;
    if one(parsed.headers, "host")? != Some(expected_host.as_bytes()) {
        return Err(malformed());
    }
    for name in ["proxy-authorization", "expect", "upgrade", "te", "trailer"] {
        if one(parsed.headers, name)?.is_some() {
            return Err(malformed());
        }
    }
    let path = parsed.path.ok_or_else(malformed)?;
    if parsed.method == Some("PUT") {
        let (name, version) = path
            .strip_prefix("/v1/packages/")
            .and_then(|p| p.split_once('/'))
            .ok_or_else(malformed)?;
        if !crate::package::manifest::valid_name(name)
            || !crate::package::manifest::valid_version(version)
            || one(parsed.headers, "content-type")? != Some(b"application/x-tar")
        {
            return Err(malformed());
        }
        if length > super::super::archive::MAX_ARCHIVE_BYTES {
            return Err(limit());
        }
        let request_id =
            std::str::from_utf8(one(parsed.headers, "idempotency-key")?.ok_or_else(malformed)?)
                .map_err(|_| malformed())?;
        if !valid_id(request_id) {
            return Err(malformed());
        }
        let expected = match (
            one(parsed.headers, "if-match")?,
            one(parsed.headers, "if-none-match")?,
        ) {
            (Some(value), None) => store::ExpectedRevision::Revision(revision(value)?),
            (None, Some(b"*")) => store::ExpectedRevision::Absent,
            _ => return Err(malformed()),
        };
        let bearer = bearer(parsed.headers)?;
        return Ok(Request::Publish {
            name,
            version,
            bearer,
            request_id,
            expected,
            length,
        });
    }
    for name in ["if-match", "if-none-match", "idempotency-key"] {
        if one(parsed.headers, name)?.is_some() {
            return Err(malformed());
        }
    }
    if parsed.method == Some("POST") {
        let name = path
            .strip_prefix("/v1/packages/")
            .and_then(|p| p.strip_suffix("/actions"))
            .filter(|name| crate::package::manifest::valid_name(name))
            .ok_or_else(malformed)?;
        if one(parsed.headers, "content-type")? != Some(b"application/json") {
            return Err(malformed());
        }
        if length > 2048 {
            return Err(limit());
        }
        return Ok(Request::Action {
            name,
            bearer: bearer(parsed.headers)?,
            length,
        });
    }
    if length != 0 {
        return Err(malformed());
    }
    if let Some(name) = path
        .strip_prefix("/v1/packages/")
        .and_then(|p| p.strip_suffix("/owners"))
    {
        if !crate::package::manifest::valid_name(name) {
            return Err(malformed());
        }
        return Ok(Request::Owners {
            name,
            bearer: bearer(parsed.headers)?,
        });
    }
    if one(parsed.headers, "authorization")?.is_some() {
        return Err(malformed());
    }
    if let Some(name) = path.strip_prefix("/v1/packages/") {
        if !crate::package::manifest::valid_name(name) {
            return Err(malformed());
        }
        Ok(Request::Index(name))
    } else if let Some(hash) = path.strip_prefix("/v1/objects/") {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(malformed());
        }
        Ok(Request::Archive(hash))
    } else {
        Err(error(ErrorKind::NotFound))
    }
}

pub(super) struct Response {
    pub(super) status: u16,
    pub(super) length: usize,
}

pub(super) fn response(bytes: &[u8], expected_type: &str) -> Result<Response, Error> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADER_FIELDS];
    let mut parsed = httparse::Response::new(&mut headers);
    if parsed.parse(bytes).map_err(|e| match e {
        httparse::Error::TooManyHeaders => limit(),
        _ => malformed(),
    })? != httparse::Status::Complete(bytes.len())
        || parsed.version != Some(1)
        || !bytes.starts_with(b"HTTP/1.1 ")
    {
        return Err(malformed());
    }
    let length = framing(parsed.headers)?;
    let status = parsed.code.ok_or_else(malformed)?;
    let content_type = if status == 200 {
        expected_type
    } else {
        "application/json"
    };
    if one(parsed.headers, "content-type")? != Some(content_type.as_bytes()) {
        return Err(malformed());
    }
    Ok(Response { status, length })
}

pub(super) fn body(
    reader: &mut impl Read,
    length: usize,
    maximum: usize,
    end: Instant,
) -> Result<Vec<u8>, Error> {
    if length > maximum {
        return Err(limit());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| limit())?;
    bytes.resize(length, 0);
    let mut read = 0;
    while read < length {
        remaining(end)?;
        match reader.read(&mut bytes[read..]) {
            Ok(0) => return Err(error(ErrorKind::Transport)),
            Ok(n) => read += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(bytes)
}

pub(super) fn eof(reader: &mut impl Read, end: Instant) -> Result<(), Error> {
    loop {
        remaining(end)?;
        match reader.read(&mut [0]) {
            Ok(0) => return Ok(()),
            Ok(_) => return Err(malformed()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
}
