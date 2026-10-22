use super::super::negotiation::{BuildIdentifier, PrereleaseIdentifier, SemanticVersion};
use super::*;

pub(super) fn parse(text: &str, span: m::Span, c: &mut Context) -> Result<SemanticVersion, Error> {
    c.charge(text.len(), span)?;
    let bad = || Error {
        code: Code::Version,
        reason: Reason::InvalidVersion,
        span,
        related: None,
    };
    let (head, build) = text
        .split_once('+')
        .map_or((text, None), |(h, b)| (h, Some(b)));
    let (core, pre) = head
        .split_once('-')
        .map_or((head, None), |(h, p)| (h, Some(p)));
    let mut parts = core.split('.');
    fn number(text: &str) -> Option<u64> {
        if text.is_empty() || (text.len() > 1 && text.starts_with('0')) {
            return None;
        }
        text.bytes().try_fold(0u64, |n, b| {
            if b.is_ascii_digit() {
                n.checked_mul(10)?.checked_add(u64::from(b - b'0'))
            } else {
                None
            }
        })
    }
    let major = number(parts.next().ok_or_else(bad)?).ok_or_else(bad)?;
    let minor = number(parts.next().ok_or_else(bad)?).ok_or_else(bad)?;
    let patch = number(parts.next().ok_or_else(bad)?).ok_or_else(bad)?;
    if parts.next().is_some() {
        return Err(bad());
    }
    let mut version = SemanticVersion::new(major, minor, patch);
    fn valid(text: &str) -> bool {
        !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }
    if let Some(pre) = pre {
        let mut values = c.vector(pre.split('.').count(), span)?;
        for part in pre.split('.') {
            if !valid(part) {
                return Err(bad());
            }
            let numeric = part.bytes().all(|b| b.is_ascii_digit());
            if numeric && part.len() > 1 && part.starts_with('0') {
                return Err(bad());
            }
            let text = c.text(part, span)?;
            let id = if numeric {
                PrereleaseIdentifier::numeric(text)
            } else {
                PrereleaseIdentifier::alphanumeric(text)
            }
            .map_err(|_| bad())?;
            values.push(id);
        }
        version = version.with_prerelease(values);
    }
    if let Some(build) = build {
        let mut values = c.vector(build.split('.').count(), span)?;
        for part in build.split('.') {
            if !valid(part) {
                return Err(bad());
            }
            values.push(BuildIdentifier::new(c.text(part, span)?).map_err(|_| bad())?);
        }
        version = version.with_build(values);
    }
    Ok(version)
}
