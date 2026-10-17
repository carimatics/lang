use serde_json::Value;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::{self, BufRead, Read, Write};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub header_bytes: usize,
    pub message_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            header_bytes: 8192,
            message_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    HeaderLimit,
    MessageLimit,
    InvalidHeader,
    DuplicateHeader,
    MissingLength,
    InvalidLength,
    Truncated,
    UnsupportedEncoding,
    Allocation,
    Serialization,
}
impl Display for TransportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LSP transport error: {self:?}")
    }
}
impl Error for TransportError {}
impl From<io::Error> for TransportError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
pub fn read(input: &mut impl BufRead, limits: Limits) -> Result<Option<Vec<u8>>, TransportError> {
    let mut used = 0usize;
    let mut length = None;
    let mut content_type = false;
    let mut unsupported = false;
    loop {
        let mut line = Vec::new();
        let count = input
            .take(limits.header_bytes.saturating_sub(used).saturating_add(1) as u64)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return if used == 0 {
                Ok(None)
            } else {
                Err(TransportError::Truncated)
            };
        }
        used = used.checked_add(count).ok_or(TransportError::HeaderLimit)?;
        if used > limits.header_bytes {
            return Err(TransportError::HeaderLimit);
        }
        if !line.ends_with(b"\r\n") {
            return Err(TransportError::InvalidHeader);
        }
        let line = &line[..line.len() - 2];
        if line.is_empty() {
            break;
        }
        if !line.is_ascii() || line.iter().any(|b| (*b < 32 && *b != b'\t') || *b == 127) {
            return Err(TransportError::InvalidHeader);
        }
        let line = std::str::from_utf8(line).map_err(|_| TransportError::InvalidHeader)?;
        let (name, value) = line.split_once(':').ok_or(TransportError::InvalidHeader)?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(TransportError::InvalidHeader);
        }
        let value = value.trim_ascii();
        if name.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() {
                return Err(TransportError::DuplicateHeader);
            }
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(TransportError::InvalidLength);
            }
            length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| TransportError::InvalidLength)?,
            );
        } else if name.eq_ignore_ascii_case("Content-Type") {
            if content_type {
                return Err(TransportError::DuplicateHeader);
            }
            content_type = true;
            let mut parts = value.split(';');
            let media = parts.next().unwrap().trim_ascii();
            unsupported |= !media.eq_ignore_ascii_case("application/vscode-jsonrpc")
                && !media.eq_ignore_ascii_case("application/json");
            for part in parts {
                if let Some((name, value)) = part.split_once('=')
                    && name.trim_ascii().eq_ignore_ascii_case("charset")
                {
                    let value = value.trim_ascii();
                    let value = value
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .unwrap_or(value);
                    unsupported |=
                        !value.eq_ignore_ascii_case("utf-8") && !value.eq_ignore_ascii_case("utf8");
                }
            }
        }
    }
    let length = length.ok_or(TransportError::MissingLength)?;
    if length > limits.message_bytes {
        return Err(TransportError::MessageLimit);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| TransportError::Allocation)?;
    bytes.resize(length, 0);
    input.read_exact(&mut bytes).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            TransportError::Truncated
        } else {
            TransportError::Io(e)
        }
    })?;
    if unsupported {
        return Err(TransportError::UnsupportedEncoding);
    }
    Ok(Some(bytes))
}
struct Buffer {
    bytes: Vec<u8>,
    limit: usize,
    too_large: bool,
    allocation: bool,
}
impl Write for Buffer {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let Some(needed) = self
            .bytes
            .len()
            .checked_add(input.len())
            .filter(|n| *n <= self.limit)
        else {
            self.too_large = true;
            return Err(io::Error::other("message limit"));
        };
        if needed > self.bytes.capacity() {
            let capacity = self
                .bytes
                .capacity()
                .max(1024)
                .saturating_mul(2)
                .max(needed)
                .min(self.limit);
            if self
                .bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .is_err()
            {
                self.allocation = true;
                return Err(io::Error::other("allocation failed"));
            }
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn write(output: &mut impl Write, value: &Value, limits: Limits) -> Result<(), TransportError> {
    let mut buffer = Buffer {
        bytes: Vec::new(),
        limit: limits.message_bytes,
        too_large: false,
        allocation: false,
    };
    serde_json::to_writer(&mut buffer, value).map_err(|_| {
        if buffer.too_large {
            TransportError::MessageLimit
        } else if buffer.allocation {
            TransportError::Allocation
        } else {
            TransportError::Serialization
        }
    })?;
    output.write_all(format!("Content-Length: {}\r\n\r\n", buffer.bytes.len()).as_bytes())?;
    output.write_all(&buffer.bytes)?;
    output.flush()?;
    Ok(())
}
