use std::path::{Path, PathBuf};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UriError {
    Invalid,
    NonLocal,
    NonUtf8,
}
fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
pub fn path(uri: &str, limit: usize) -> Result<PathBuf, UriError> {
    if uri.len() > limit || uri.contains(['?', '#']) {
        return Err(UriError::Invalid);
    }
    let rest = uri.strip_prefix("file://").ok_or(UriError::Invalid)?;
    let slash = rest.find('/').ok_or(UriError::Invalid)?;
    let authority = &rest[..slash];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return Err(UriError::NonLocal);
    }
    let encoded = &rest.as_bytes()[slash..];
    let mut decoded = Vec::new();
    decoded
        .try_reserve_exact(encoded.len())
        .map_err(|_| UriError::Invalid)?;
    let mut i = 0;
    while i < encoded.len() {
        let byte = if encoded[i] == b'%' {
            let a = hex(*encoded.get(i + 1).ok_or(UriError::Invalid)?).ok_or(UriError::Invalid)?;
            let b = hex(*encoded.get(i + 2).ok_or(UriError::Invalid)?).ok_or(UriError::Invalid)?;
            i += 3;
            a * 16 + b
        } else {
            let b = encoded[i];
            i += 1;
            b
        };
        if byte == 0 || byte == b'\\' {
            return Err(UriError::Invalid);
        }
        decoded.push(byte);
    }
    let text = std::str::from_utf8(&decoded).map_err(|_| UriError::NonUtf8)?;
    if text.starts_with("//") {
        return Err(UriError::NonLocal);
    }
    let text = if cfg!(windows)
        && text.as_bytes().get(2) == Some(&b':')
        && text.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic)
    {
        &text[1..]
    } else {
        text
    };
    let path = PathBuf::from(text);
    if !path.is_absolute()
        || path.components().any(|p| {
            matches!(
                p,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(UriError::Invalid);
    }
    Ok(path)
}
pub fn file(path: &Path) -> Result<String, UriError> {
    if !path.is_absolute() {
        return Err(UriError::Invalid);
    }
    let text = path.to_str().ok_or(UriError::NonUtf8)?;
    let text = if cfg!(windows) {
        text.strip_prefix("\\\\?\\")
            .unwrap_or(text)
            .replace('\\', "/")
    } else {
        text.into()
    };
    if text.starts_with("UNC/") || text.starts_with("//") {
        return Err(UriError::NonLocal);
    }
    let mut uri = String::from("file://");
    if !text.starts_with('/') {
        uri.push('/');
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b':' | b'-' | b'_' | b'.' | b'~') {
            uri.push(b as char);
        } else {
            uri.push('%');
            uri.push(HEX[(b >> 4) as usize] as char);
            uri.push(HEX[(b & 15) as usize] as char);
        }
    }
    Ok(uri)
}
