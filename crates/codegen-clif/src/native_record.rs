//! RFC 0018/0030 native compiler records. Data never authorizes native execution.
use crate::{
    Triple,
    program::{FunctionCode, SourceRange},
};
use lang_mir::unit as m;
use lang_runtime::c_abi::Digest;
use sha2::{Digest as _, Sha256};
use std::fmt::{Display, Formatter};

const MAGIC: &[u8; 8] = b"LANGNAT\0";
const HEADER: usize = 16;
const SYMBOL_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub validation: m::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024,
            validation: m::Limits::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Copy,
    OwnershipFunction,
    OwnershipUnit,
    CUnit,
    CObject(Digest),
    Task(m::FunctionId),
    Component(m::FunctionId),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Bytes,
    Truncated,
    Magic,
    Version(u16),
    Flags(u16),
    Length,
    Target,
    Kind(u8),
    Profile,
    SemanticDigest,
    ObjectDigest,
    Count,
    Symbol,
    FunctionOrder,
    CodeRange,
    SourceRange,
    Capacity,
    Resource(m::Error),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub offset: usize,
    pub kind: ErrorKind,
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "native record failed at byte {}: {:?}",
            self.offset, self.kind
        )
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ErrorKind::Resource(e) => Some(e),
            _ => None,
        }
    }
}
fn error(offset: usize, kind: ErrorKind) -> Error {
    Error { offset, kind }
}
fn charged(budget: &mut m::Budget, count: usize, offset: usize) -> Result<(), Error> {
    budget
        .charge(count)
        .map_err(|e| error(offset, ErrorKind::Resource(e)))
}
fn hash(bytes: &[u8], budget: &mut m::Budget, offset: usize) -> Result<[u8; 32], Error> {
    charged(budget, bytes.len() / 1024 + 1, offset)?;
    Ok(Sha256::digest(bytes).into())
}
fn target_tag(target: &Triple) -> Result<u8, Error> {
    if target
        == &"aarch64-apple-darwin"
            .parse::<Triple>()
            .expect("known target")
    {
        Ok(0)
    } else if target
        == &"x86_64-pc-windows-msvc"
            .parse::<Triple>()
            .expect("known target")
    {
        Ok(1)
    } else {
        Err(error(HEADER, ErrorKind::Target))
    }
}
fn symbol_valid(symbol: &str) -> bool {
    !symbol.is_empty() && symbol.len() <= SYMBOL_BYTES && !symbol.contains('\0')
}
fn range_valid(range: &SourceRange, size: usize) -> bool {
    range.start < range.end && range.end as usize <= size
}
fn usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}

/// Compiler output data, never a proof of native safety or an executable owner.
/// Link/bind still requires the exact compiler/toolchain/image and explicit authority.
#[derive(Debug)]
pub struct Record {
    pub(crate) target: Triple,
    pub(crate) kind: Kind,
    pub(crate) bytes: Vec<u8>,
    pub(crate) functions: Vec<FunctionCode>,
    usage: m::Usage,
}
impl Record {
    pub(crate) fn new(
        target: Triple,
        kind: Kind,
        bytes: Vec<u8>,
        functions: Vec<FunctionCode>,
    ) -> Self {
        Self {
            target,
            kind,
            bytes,
            functions,
            usage: m::Usage::default(),
        }
    }
    pub fn target(&self) -> &Triple {
        &self.target
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn object(&self) -> &[u8] {
        &self.bytes
    }
    /// Recover original compiler object data without cloning it or granting
    /// any native execution authority.
    pub fn into_object_bytes(self) -> Vec<u8> {
        self.bytes
    }
    pub fn functions(&self) -> &[FunctionCode] {
        &self.functions
    }
    pub fn usage(&self) -> m::Usage {
        self.usage
    }
    pub fn into_copy(self) -> Result<crate::program::ObjectArtifact, Error> {
        if self.kind != Kind::Copy {
            return Err(error(HEADER + 1, ErrorKind::Profile));
        }
        Ok(crate::program::ObjectArtifact {
            bytes: self.bytes,
            functions: self.functions,
        })
    }

    /// Encode a record paired with these exact portable artifact bytes.
    /// The compiler must supply semantics from the same checked program; hashes
    /// detect mix-ups, not authenticity or semantic equivalence of native code.
    pub fn encode(&self, semantic: &[u8], limits: Limits) -> Result<Vec<u8>, Error> {
        if semantic.len() > limits.bytes || self.bytes.len() > limits.bytes {
            return Err(error(0, ErrorKind::Bytes));
        }
        let target = target_tag(&self.target)?;
        if self.functions.len() > limits.validation.functions {
            return Err(error(0, ErrorKind::Count));
        }
        let mut budget = m::Budget::new(limits.validation);
        budget
            .allocate::<u8>(self.bytes.capacity())
            .map_err(|e| error(0, ErrorKind::Resource(e)))?;
        budget
            .allocate::<FunctionCode>(self.functions.capacity())
            .map_err(|e| error(0, ErrorKind::Resource(e)))?;
        charged(&mut budget, self.functions.len(), 0)?;
        for f in &self.functions {
            if f.source_ranges.len() > limits.validation.instructions {
                return Err(error(0, ErrorKind::Count));
            }
            budget
                .allocate::<u8>(f.symbol.capacity())
                .map_err(|e| error(0, ErrorKind::Resource(e)))?;
            budget
                .allocate::<SourceRange>(f.source_ranges.capacity())
                .map_err(|e| error(0, ErrorKind::Resource(e)))?;
        }
        let semantic = hash(semantic, &mut budget, 0)?;
        let object = hash(&self.bytes, &mut budget, 0)?;
        let size = {
            let mut writer = Writer {
                output: None,
                length: 0,
                maximum: limits.bytes,
                budget: &mut budget,
            };
            writer.record(self, target, semantic, object, 0)?;
            writer.length
        };
        let payload = u32::try_from(size - HEADER).map_err(|_| error(12, ErrorKind::Capacity))?;
        let output = budget
            .vector(size)
            .map_err(|e| error(0, ErrorKind::Resource(e)))?;
        let mut writer = Writer {
            output: Some(output),
            length: 0,
            maximum: size,
            budget: &mut budget,
        };
        writer.record(self, target, semantic, object, payload)?;
        Ok(writer.output.unwrap())
    }

    /// Validate framing, checksums and bounded metadata only. This does not
    /// load code, resolve symbols, create contexts, or authorize execution.
    pub fn decode(bytes: &[u8], semantic: &[u8], limits: Limits) -> Result<Self, Error> {
        Self::decode_with_budget(
            bytes,
            semantic,
            limits.bytes,
            &mut m::Budget::new(limits.validation),
        )
    }

    /// Decode on the caller's original preparation ledger. Intrinsic usage is
    /// reported on success; every admitted prefix remains charged on refusal.
    /// This operation creates no code, context or execution authority.
    pub fn decode_with_budget(
        bytes: &[u8],
        semantic: &[u8],
        maximum_bytes: usize,
        budget: &mut m::Budget,
    ) -> Result<Self, Error> {
        if bytes.len() > maximum_bytes || semantic.len() > maximum_bytes {
            return Err(error(0, ErrorKind::Bytes));
        }
        let mut intrinsic = budget.scope_usage();
        let mut reader = Reader {
            bytes,
            offset: 0,
            budget: &mut intrinsic,
        };
        if reader.raw(8)? != MAGIC {
            return Err(error(0, ErrorKind::Magic));
        }
        let version = u16::from_le_bytes(reader.raw(2)?.try_into().unwrap());
        if version != 1 && version != 2 {
            return Err(error(8, ErrorKind::Version(version)));
        }
        let flags = u16::from_le_bytes(reader.raw(2)?.try_into().unwrap());
        if flags != 0 {
            return Err(error(10, ErrorKind::Flags(flags)));
        }
        if reader.word()? as usize != bytes.len() - HEADER {
            return Err(error(12, ErrorKind::Length));
        }
        let target = match reader.byte()? {
            0 => "aarch64-apple-darwin",
            1 => "x86_64-pc-windows-msvc",
            _ => return Err(error(HEADER, ErrorKind::Target)),
        }
        .parse()
        .expect("known target");
        let kind = match (version, reader.byte()?) {
            (1, 0) => Kind::Copy,
            (1, 1) => Kind::OwnershipFunction,
            (1, 2) => Kind::OwnershipUnit,
            (1, 3) => Kind::CUnit,
            (1, 4) => Kind::CObject(Digest(reader.raw(32)?.try_into().unwrap())),
            (2, 0) => Kind::Task(m::FunctionId(reader.word()?)),
            (2, 1) => Kind::Component(m::FunctionId(reader.word()?)),
            (_, tag) => return Err(error(HEADER + 1, ErrorKind::Kind(tag))),
        };
        let offset = reader.offset;
        let expected: [u8; 32] = reader.raw(32)?.try_into().unwrap();
        if hash(semantic, reader.budget, offset)? != expected {
            return Err(error(offset, ErrorKind::SemanticDigest));
        }
        let digest_offset = reader.offset;
        let expected: [u8; 32] = reader.raw(32)?.try_into().unwrap();
        let length = reader.word()? as usize;
        let object = reader.raw(length)?;
        if hash(object, reader.budget, digest_offset)? != expected {
            return Err(error(digest_offset, ErrorKind::ObjectDigest));
        }
        let mut object_bytes = reader.vector::<u8>(length)?;
        object_bytes.extend_from_slice(object);
        let count = reader.count(reader.budget.limits().functions, 17)?;
        let mut functions: Vec<FunctionCode> = reader.vector(count)?;
        for _ in 0..count {
            let offset = reader.offset;
            let function = m::FunctionId(reader.word()?);
            if functions.last().is_some_and(|f| f.function.0 >= function.0) {
                return Err(error(offset, ErrorKind::FunctionOrder));
            }
            let symbol = reader.symbol()?;
            let size_offset = reader.offset;
            let code_size = reader.word()? as usize;
            if code_size == 0 {
                return Err(error(size_offset, ErrorKind::CodeRange));
            }
            let count = reader.count(reader.budget.limits().instructions, 20)?;
            let mut source_ranges = reader.vector(count)?;
            for _ in 0..count {
                let offset = reader.offset;
                let range = SourceRange {
                    start: reader.word()?,
                    end: reader.word()?,
                    span: m::Span {
                        source: m::SourceId(reader.word()?),
                        start: reader.word()?,
                        end: reader.word()?,
                    },
                };
                if !range_valid(&range, code_size) {
                    return Err(error(offset, ErrorKind::CodeRange));
                }
                if range.span.start > range.span.end {
                    return Err(error(offset + 12, ErrorKind::SourceRange));
                }
                source_ranges.push(range);
            }
            functions.push(FunctionCode {
                function,
                symbol,
                code_size,
                source_ranges,
            });
        }
        if reader.offset != bytes.len() {
            return Err(error(reader.offset, ErrorKind::Length));
        }
        Ok(Self {
            target,
            kind,
            bytes: object_bytes,
            functions,
            usage: usage(reader.budget),
        })
    }
}
struct Writer<'a> {
    output: Option<Vec<u8>>,
    length: usize,
    maximum: usize,
    budget: &'a mut m::Budget,
}
impl Writer<'_> {
    fn raw(&mut self, bytes: &[u8]) -> Result<(), Error> {
        charged(self.budget, bytes.len() / 1024 + 1, self.length)?;
        let end = self
            .length
            .checked_add(bytes.len())
            .ok_or_else(|| error(self.length, ErrorKind::Capacity))?;
        if end > self.maximum {
            return Err(error(self.length, ErrorKind::Bytes));
        }
        if let Some(output) = &mut self.output {
            output.extend_from_slice(bytes);
        }
        self.length = end;
        Ok(())
    }
    fn word(&mut self, value: usize) -> Result<(), Error> {
        let value = u32::try_from(value).map_err(|_| error(self.length, ErrorKind::Capacity))?;
        self.raw(&value.to_le_bytes())
    }
    fn record(
        &mut self,
        record: &Record,
        target: u8,
        semantic: [u8; 32],
        object: [u8; 32],
        payload: u32,
    ) -> Result<(), Error> {
        let (version, kind, entry) = match record.kind {
            Kind::Copy => (1u16, 0, None),
            Kind::OwnershipFunction => (1, 1, None),
            Kind::OwnershipUnit => (1, 2, None),
            Kind::CUnit => (1, 3, None),
            Kind::CObject(_) => (1, 4, None),
            Kind::Task(entry) => (2, 0, Some(entry)),
            Kind::Component(entry) => (2, 1, Some(entry)),
        };
        self.raw(MAGIC)?;
        self.raw(&version.to_le_bytes())?;
        self.raw(&0u16.to_le_bytes())?;
        self.word(payload as usize)?;
        self.raw(&[target])?;
        self.raw(&[kind])?;
        if let Some(entry) = entry {
            self.word(entry.0 as usize)?;
        }
        if let Kind::CObject(digest) = record.kind {
            self.raw(&digest.0)?;
        }
        self.raw(&semantic)?;
        self.raw(&object)?;
        self.word(record.bytes.len())?;
        self.raw(&record.bytes)?;
        if record.functions.len() > self.budget.limits().functions {
            return Err(error(self.length, ErrorKind::Count));
        }
        self.word(record.functions.len())?;
        let mut previous = None;
        for f in &record.functions {
            if previous.is_some_and(|id| id >= f.function.0) {
                return Err(error(self.length, ErrorKind::FunctionOrder));
            }
            previous = Some(f.function.0);
            self.word(f.function.0 as usize)?;
            if !symbol_valid(&f.symbol) {
                return Err(error(self.length, ErrorKind::Symbol));
            }
            self.word(f.symbol.len())?;
            self.raw(f.symbol.as_bytes())?;
            if f.code_size == 0 {
                return Err(error(self.length, ErrorKind::CodeRange));
            }
            self.word(f.code_size)?;
            if f.source_ranges.len() > self.budget.limits().instructions {
                return Err(error(self.length, ErrorKind::Count));
            }
            self.word(f.source_ranges.len())?;
            for range in &f.source_ranges {
                if !range_valid(range, f.code_size) {
                    return Err(error(self.length, ErrorKind::CodeRange));
                }
                if range.span.start > range.span.end {
                    return Err(error(self.length + 12, ErrorKind::SourceRange));
                }
                for value in [
                    range.start,
                    range.end,
                    range.span.source.0,
                    range.span.start,
                    range.span.end,
                ] {
                    self.word(value as usize)?;
                }
            }
        }
        Ok(())
    }
}
struct Reader<'a, 'b> {
    bytes: &'a [u8],
    offset: usize,
    budget: &'b mut m::Budget,
}
impl<'a> Reader<'a, '_> {
    fn raw(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| error(self.offset, ErrorKind::Capacity))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| error(self.offset, ErrorKind::Truncated))?;
        charged(self.budget, length / 1024 + 1, self.offset)?;
        self.offset = end;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.raw(1)?[0])
    }
    fn word(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.raw(4)?.try_into().unwrap()))
    }
    fn count(&mut self, maximum: usize, minimum: usize) -> Result<usize, Error> {
        let offset = self.offset;
        let count = self.word()? as usize;
        if count > maximum || count > (self.bytes.len() - self.offset) / minimum {
            return Err(error(offset, ErrorKind::Count));
        }
        Ok(count)
    }
    fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, Error> {
        self.budget
            .vector(count)
            .map_err(|e| error(self.offset, ErrorKind::Resource(e)))
    }
    fn symbol(&mut self) -> Result<String, Error> {
        let offset = self.offset;
        let count = self.word()? as usize;
        if count > SYMBOL_BYTES {
            return Err(error(offset, ErrorKind::Symbol));
        }
        let bytes = self.raw(count)?;
        let text = std::str::from_utf8(bytes).map_err(|_| error(offset, ErrorKind::Symbol))?;
        if !symbol_valid(text) {
            return Err(error(offset, ErrorKind::Symbol));
        }
        let mut owned = self.vector(count)?;
        owned.extend_from_slice(bytes);
        Ok(String::from_utf8(owned).expect("validated UTF-8"))
    }
}
#[cfg(test)]
mod tests;
