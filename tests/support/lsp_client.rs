//! Independent bounded LSP consumer for the editor interoperability fixture.
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Client {
    child: Child,
    input: Option<ChildStdin>,
    messages: Receiver<Result<Value, String>>,
    reader: Option<JoinHandle<()>>,
    next: i64,
    pub events: Vec<Value>,
}
impl Client {
    pub fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lang"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let (tx, messages) = mpsc::channel();
        let reader = thread::spawn(move || {
            loop {
                match read_frame(&mut output) {
                    Ok(Some(value)) => {
                        if tx.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = tx.send(Err(error.to_string()));
                        break;
                    }
                }
            }
        });
        Self {
            child,
            input,
            messages,
            reader: Some(reader),
            next: 0,
            events: Vec::new(),
        }
    }
    fn send(&mut self, value: Value) {
        let body = serde_json::to_vec(&value).unwrap();
        assert!(body.len() <= 4 * 1024 * 1024);
        let input = self.input.as_mut().unwrap();
        write!(input, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        input.write_all(&body).unwrap();
        input.flush().unwrap();
    }
    pub fn note(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}));
    }
    pub fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let value = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("bounded LSP response deadline")
                .expect("valid independent LSP frame");
            assert_eq!(value["jsonrpc"], "2.0");
            if value["method"] == "workspace/semanticTokens/refresh" {
                self.send(json!({"jsonrpc":"2.0","id":value["id"],"result":null}));
            } else if value.get("method").is_none() {
                assert_eq!(value["id"], id, "unexpected response: {value:?}");
                return value;
            }
            assert!(self.events.len() < 256, "bounded fixture notifications");
            self.events.push(value);
        }
    }
    pub fn finish(&mut self) {
        let result = self.call("shutdown", Value::Null);
        assert!(result.get("error").is_none());
        assert!(result["result"].is_null());
        self.note("exit", Value::Null);
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "bounded LSP exit deadline");
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(0));
        let mut errors = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut errors)
            .unwrap();
        assert!(errors.is_empty(), "{errors}");
        self.reader.take().unwrap().join().unwrap();
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.input.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn read_frame(input: &mut impl Read) -> io::Result<Option<Value>> {
    let mut total = 0usize;
    let mut length = None;
    loop {
        let mut line = Vec::new();
        loop {
            let mut byte = [0];
            if input.read(&mut byte)? == 0 {
                return if total == 0 {
                    Ok(None)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "partial LSP header",
                    ))
                };
            }
            total += 1;
            if total > 8192 {
                return Err(io::Error::other("LSP header limit"));
            }
            line.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        let text = std::str::from_utf8(&line).map_err(io::Error::other)?;
        if !text.is_ascii() || !text.ends_with("\r\n") {
            return Err(io::Error::other("nonstandard LSP header"));
        }
        if text == "\r\n" {
            break;
        }
        let (key, value) = text
            .trim_end()
            .split_once(':')
            .ok_or_else(|| io::Error::other("invalid LSP header"))?;
        if key.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() {
                return Err(io::Error::other("duplicate LSP length"));
            }
            let value = value.trim();
            if !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(io::Error::other("invalid LSP length"));
            }
            length = Some(value.parse::<usize>().map_err(io::Error::other)?);
        }
    }
    let length = length
        .filter(|n| *n <= 4 * 1024 * 1024)
        .ok_or_else(|| io::Error::other("missing or oversized LSP length"))?;
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(io::Error::other)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    pub version: i32,
    pub text: String,
}
#[derive(Debug, Eq, PartialEq)]
pub enum ApplyError {
    Stale,
    Invalid,
}
pub fn position(text: &str, offset: usize) -> Value {
    let before = &text[..offset];
    json!({"line":before.bytes().filter(|b|*b==b'\n').count(),"character":before.rsplit('\n').next().unwrap().encode_utf16().count()})
}
// Deliberately independent of lang_lsp::text_offset; this consumer accepts LF/CRLF.
fn offset(text: &str, point: &Value) -> Result<usize, ApplyError> {
    let line = point["line"].as_u64().ok_or(ApplyError::Invalid)?;
    let column = point["character"].as_u64().ok_or(ApplyError::Invalid)?;
    let mut start = 0;
    for (index, body) in text.split('\n').enumerate() {
        if index as u64 == line {
            let body = body.strip_suffix('\r').unwrap_or(body);
            let mut utf16 = 0;
            for (byte, ch) in body.char_indices() {
                if utf16 == column {
                    return Ok(start + byte);
                }
                utf16 += ch.len_utf16() as u64;
                if utf16 > column {
                    return Err(ApplyError::Invalid);
                }
            }
            return if utf16 == column {
                Ok(start + body.len())
            } else {
                Err(ApplyError::Invalid)
            };
        }
        start += body.len() + 1;
    }
    Err(ApplyError::Invalid)
}
pub fn apply(documents: &mut BTreeMap<String, Document>, edit: &Value) -> Result<(), ApplyError> {
    let changes = edit["documentChanges"]
        .as_array()
        .ok_or(ApplyError::Invalid)?;
    let mut candidate = documents.clone();
    let mut seen = BTreeSet::new();
    for change in changes {
        let uri = change["textDocument"]["uri"]
            .as_str()
            .ok_or(ApplyError::Invalid)?;
        if !seen.insert(uri) {
            return Err(ApplyError::Invalid);
        }
        let original = documents.get(uri).ok_or(ApplyError::Invalid)?;
        if change["textDocument"]["version"].as_i64() != Some(i64::from(original.version)) {
            return Err(ApplyError::Stale);
        }
        let mut edits = change["edits"]
            .as_array()
            .ok_or(ApplyError::Invalid)?
            .iter()
            .map(|e| {
                Ok((
                    offset(&original.text, &e["range"]["start"])?,
                    offset(&original.text, &e["range"]["end"])?,
                    e["newText"].as_str().ok_or(ApplyError::Invalid)?,
                ))
            })
            .collect::<Result<Vec<_>, ApplyError>>()?;
        edits.sort_by_key(|e| (e.0, e.1));
        let mut end = 0;
        let mut text = String::new();
        for (start, finish, replacement) in edits {
            if start < end || start > finish {
                return Err(ApplyError::Invalid);
            }
            text.push_str(&original.text[end..start]);
            text.push_str(replacement);
            end = finish;
        }
        text.push_str(&original.text[end..]);
        candidate.insert(
            uri.into(),
            Document {
                version: original.version + 1,
                text,
            },
        );
    }
    *documents = candidate;
    Ok(())
}
