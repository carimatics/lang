//! Iterative JSON arena: configured depth never becomes native stack depth.
use super::*;

pub(super) type Id = usize;
#[derive(Debug)]
pub(super) struct Member {
    pub key: Spanned<String>,
    pub value: Id,
}
#[derive(Debug)]
pub(super) enum Kind {
    Object(Vec<Member>),
    Array(Vec<Id>),
    String(String),
    Number,
    Bool(bool),
    Null,
}
#[derive(Debug)]
pub(super) struct Node {
    pub kind: Kind,
    pub span: m::Span,
}
pub(super) struct Json<'a> {
    pub nodes: Vec<Node>,
    pub input: &'a [u8],
}
#[derive(Clone, Copy)]
enum Phase {
    ObjectFirst,
    ObjectKey,
    Colon,
    ObjectValue,
    ObjectAfter,
    ArrayFirst,
    ArrayValue,
    ArrayAfter,
}
struct Frame {
    node: Id,
    phase: Phase,
    key: Option<Spanned<String>>,
}

pub(super) fn parse<'a>(input: &'a [u8], c: &mut Context) -> Result<Json<'a>, Error> {
    let span = c.span(0, 0);
    c.cap(
        input.len(),
        c.limits.source_bytes,
        LimitKind::SourceBytes,
        span,
    )?;
    c.cap(input.len(), u32::MAX as usize, LimitKind::Capacity, span)?;
    c.charge(input.len(), span)?;
    std::str::from_utf8(input).map_err(|e| {
        c.error(
            Code::Syntax,
            Reason::Syntax,
            c.span(
                e.valid_up_to(),
                e.valid_up_to() + e.error_len().unwrap_or(0),
            ),
        )
    })?;
    let mut p = Parser {
        input,
        at: 0,
        nodes: Vec::new(),
        c,
    };
    let root = p.value(1)?;
    debug_assert_eq!(root, 0);
    let mut stack = Vec::new();
    if let Some(frame) = p.frame(root) {
        p.c.push(&mut stack, frame, span)?;
    }
    while let Some(mut frame) = stack.pop() {
        p.ws()?;
        match frame.phase {
            Phase::ObjectFirst | Phase::ObjectKey => {
                if matches!(frame.phase, Phase::ObjectFirst) && p.peek() == Some(b'}') {
                    p.close(frame.node)?;
                    continue;
                }
                let key = p.string()?;
                let Kind::Object(members) = &p.nodes[frame.node].kind else {
                    unreachable!()
                };
                for member in members {
                    p.c.charge(member.key.value.len().min(key.value.len()) + 1, key.span)?;
                    if member.key.value == key.value {
                        let mut e = p.c.error(Code::Field, Reason::Duplicate, key.span);
                        e.related = Some(member.key.span);
                        return Err(e);
                    }
                }
                frame.key = Some(key);
                frame.phase = Phase::Colon;
                p.c.push(&mut stack, frame, span)?;
            }
            Phase::Colon => {
                p.expect(b':')?;
                frame.phase = Phase::ObjectValue;
                p.c.push(&mut stack, frame, span)?;
            }
            Phase::ObjectValue | Phase::ArrayFirst | Phase::ArrayValue => {
                if matches!(frame.phase, Phase::ArrayFirst) && p.peek() == Some(b']') {
                    p.close(frame.node)?;
                    continue;
                }
                let value = p.value(stack.len() + 2)?;
                let child = p.frame(value);
                let location = p.nodes[value].span;
                match &mut p.nodes[frame.node].kind {
                    Kind::Object(members) => {
                        p.c.push(
                            members,
                            Member {
                                key: frame.key.take().expect("object key state"),
                                value,
                            },
                            location,
                        )?;
                        frame.phase = Phase::ObjectAfter;
                    }
                    Kind::Array(values) => {
                        p.c.push(values, value, location)?;
                        frame.phase = Phase::ArrayAfter;
                    }
                    _ => unreachable!(),
                }
                p.c.push(&mut stack, frame, location)?;
                if let Some(child) = child {
                    p.c.push(&mut stack, child, location)?;
                }
            }
            Phase::ObjectAfter | Phase::ArrayAfter => {
                let object = matches!(frame.phase, Phase::ObjectAfter);
                if p.peek() == Some(if object { b'}' } else { b']' }) {
                    p.close(frame.node)?;
                } else {
                    p.expect(b',')?;
                    frame.phase = if object {
                        Phase::ObjectKey
                    } else {
                        Phase::ArrayValue
                    };
                    p.c.push(&mut stack, frame, span)?;
                }
            }
        }
    }
    p.ws()?;
    if p.at != input.len() {
        return Err(p.syntax());
    }
    Ok(Json {
        nodes: p.nodes,
        input,
    })
}
struct Parser<'a, 'c> {
    input: &'a [u8],
    at: usize,
    nodes: Vec<Node>,
    c: &'c mut Context,
}
impl Parser<'_, '_> {
    fn here(&self) -> m::Span {
        let mut start = self.at;
        let mut end = self.at.saturating_add(1).min(self.input.len());
        while start > 0 && self.input.get(start).is_some_and(|b| b & 0xc0 == 0x80) {
            start -= 1;
        }
        while self.input.get(end).is_some_and(|b| b & 0xc0 == 0x80) {
            end += 1;
        }
        self.c.span(start, end)
    }
    fn syntax(&self) -> Error {
        self.c.error(Code::Syntax, Reason::Syntax, self.here())
    }
    fn peek(&self) -> Option<u8> {
        self.input.get(self.at).copied()
    }
    fn take(&mut self) -> Result<u8, Error> {
        self.c.charge(1, self.here())?;
        let byte = self.peek().ok_or_else(|| self.syntax())?;
        self.at += 1;
        Ok(byte)
    }
    fn expect(&mut self, byte: u8) -> Result<(), Error> {
        if self.peek() != Some(byte) {
            return Err(self.syntax());
        }
        self.take()?;
        Ok(())
    }
    fn ws(&mut self) -> Result<(), Error> {
        while self
            .peek()
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.take()?;
        }
        Ok(())
    }
    fn frame(&self, id: Id) -> Option<Frame> {
        let phase = match self.nodes[id].kind {
            Kind::Object(_) => Phase::ObjectFirst,
            Kind::Array(_) => Phase::ArrayFirst,
            _ => return None,
        };
        Some(Frame {
            node: id,
            phase,
            key: None,
        })
    }
    fn close(&mut self, id: Id) -> Result<(), Error> {
        self.take()?;
        self.nodes[id].span.end = self.at as u32;
        Ok(())
    }
    fn value(&mut self, depth: usize) -> Result<Id, Error> {
        self.ws()?;
        self.c
            .cap(depth, self.c.limits.depth, LimitKind::Depth, self.here())?;
        let start = self.at;
        let kind = match self.peek() {
            Some(b'{') => {
                self.take()?;
                Kind::Object(Vec::new())
            }
            Some(b'[') => {
                self.take()?;
                Kind::Array(Vec::new())
            }
            Some(b'"') => Kind::String(self.string()?.value),
            Some(b't') => {
                self.word(b"true")?;
                Kind::Bool(true)
            }
            Some(b'f') => {
                self.word(b"false")?;
                Kind::Bool(false)
            }
            Some(b'n') => {
                self.word(b"null")?;
                Kind::Null
            }
            Some(b'-' | b'0'..=b'9') => {
                self.number()?;
                Kind::Number
            }
            _ => return Err(self.syntax()),
        };
        let span = self.c.span(start, self.at);
        let id = self.nodes.len();
        self.c.push(&mut self.nodes, Node { kind, span }, span)?;
        Ok(id)
    }
    fn word(&mut self, word: &[u8]) -> Result<(), Error> {
        for b in word {
            self.expect(*b)?;
        }
        Ok(())
    }
    fn number(&mut self) -> Result<(), Error> {
        if self.peek() == Some(b'-') {
            self.take()?;
        }
        match self.peek() {
            Some(b'0') => {
                self.take()?;
            }
            Some(b'1'..=b'9') => {
                self.take()?;
                while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.take()?;
                }
            }
            _ => return Err(self.syntax()),
        }
        if self.peek() == Some(b'.') {
            self.take()?;
            self.digits()?;
        }
        if self.peek().is_some_and(|b| matches!(b, b'e' | b'E')) {
            self.take()?;
            if self.peek().is_some_and(|b| matches!(b, b'+' | b'-')) {
                self.take()?;
            }
            self.digits()?;
        }
        Ok(())
    }
    fn digits(&mut self) -> Result<(), Error> {
        if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
            return Err(self.syntax());
        }
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.take()?;
        }
        Ok(())
    }
    fn hex(&mut self) -> Result<u16, Error> {
        let mut value = 0;
        for _ in 0..4 {
            let n = match self.peek() {
                Some(b'0'..=b'9') => self.take()? - b'0',
                Some(b'a'..=b'f') => self.take()? - b'a' + 10,
                Some(b'A'..=b'F') => self.take()? - b'A' + 10,
                _ => return Err(self.syntax()),
            };
            value = value * 16 + u16::from(n);
        }
        Ok(value)
    }
    fn string(&mut self) -> Result<Spanned<String>, Error> {
        let start = self.at;
        self.expect(b'"')?;
        let mut bytes = Vec::new();
        loop {
            let b = self.take()?;
            let mut encoded = [0; 4];
            let chunk: &[u8] = match b {
                b'"' => break,
                0..=31 => return Err(self.syntax()),
                b'\\' => match self.take()? {
                    b'"' => b"\"",
                    b'\\' => b"\\",
                    b'/' => b"/",
                    b'b' => b"\x08",
                    b'f' => b"\x0c",
                    b'n' => b"\n",
                    b'r' => b"\r",
                    b't' => b"\t",
                    b'u' => {
                        let first = self.hex()?;
                        let cp = if (0xd800..=0xdbff).contains(&first) {
                            self.word(b"\\u")?;
                            let low = self.hex()?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return Err(self.syntax());
                            }
                            0x10000
                                + ((u32::from(first) - 0xd800) << 10)
                                + (u32::from(low) - 0xdc00)
                        } else {
                            u32::from(first)
                        };
                        char::from_u32(cp)
                            .ok_or_else(|| self.syntax())?
                            .encode_utf8(&mut encoded)
                            .as_bytes()
                    }
                    _ => return Err(self.syntax()),
                },
                _ => {
                    encoded[0] = b;
                    &encoded[..1]
                }
            };
            let span = self.c.span(start, self.at);
            self.c.cap(
                bytes.len() + chunk.len(),
                self.c.limits.string_bytes,
                LimitKind::StringBytes,
                span,
            )?;
            for b in chunk {
                self.c.push(&mut bytes, *b, span)?;
            }
        }
        let value = String::from_utf8(bytes).map_err(|_| self.syntax())?;
        Ok(Spanned {
            value,
            span: self.c.span(start, self.at),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counter_json_keeps_token_ranges_and_rejects_each_truncated_prefix() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/component_counter.idl.json"
        ))
        .trim_end();
        let json = parse(
            source.as_bytes(),
            &mut Context::new(m::SourceId(7), Limits::default()),
        )
        .unwrap();
        let Kind::Object(fields) = &json.nodes[0].kind else {
            panic!("object")
        };
        let package = fields.iter().find(|f| f.key.value == "package").unwrap();
        assert_eq!(
            &source[package.key.span.start as usize..package.key.span.end as usize],
            "\"package\""
        );
        assert_eq!(package.key.span.source, m::SourceId(7));
        for end in 0..source.len() {
            assert!(
                parse(
                    &source.as_bytes()[..end],
                    &mut Context::new(m::SourceId(7), Limits::default())
                )
                .is_err(),
                "prefix {end}"
            );
        }
    }
    #[test]
    fn decoded_duplicate_keys_retain_both_locations() {
        let bytes = br#"{"idl":1,"\u0069dl":1}"#;
        let error = parse(bytes, &mut Context::new(m::SourceId(7), Limits::default()))
            .err()
            .unwrap();
        assert_eq!(error.code, Code::Field);
        assert_eq!(error.reason, Reason::Duplicate);
        assert!(error.span.start > error.related.unwrap().start);
    }
    #[test]
    fn large_configured_depth_uses_an_explicit_stack() {
        let depth = 8192;
        let mut source = "[".repeat(depth);
        source.push('0');
        source.push_str(&"]".repeat(depth));
        let limits = Limits {
            depth: depth + 1,
            ..Default::default()
        };
        let json = parse(source.as_bytes(), &mut Context::new(m::SourceId(0), limits)).unwrap();
        assert_eq!(json.nodes.len(), depth + 1);
    }
}
