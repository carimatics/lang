//! RFC 0008 profile-1 C descriptors and layouts. These describe actual C
//! objects, independently of managed cells and their private storage layout.
//! Descriptor compilation dereferences no foreign pointer. The shared context
//! gate checks explicit grants; real VM/native C adapters remain separate.
use lang_mir::unit::{self as m, ownership as o};
use o::declarations::{FunctionAbi, ImportAbi, VerifiedDeclarations};
use sha2::{Digest as _, Sha256};
use std::fmt::{Display, Formatter};

pub mod context;
mod header;
pub mod validation;
pub use header::HeaderOptions;

pub const PROFILE: u16 = 1;
pub const MAX_DESCRIPTOR_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    MacosAarch64,
    WindowsX86_64,
}
impl Target {
    pub const fn tag(self) -> u8 {
        match self {
            Self::MacosAarch64 => 1,
            Self::WindowsX86_64 => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Import,
    Export,
}
impl Direction {
    const fn tag(self) -> u8 {
        match self {
            Self::Import => 0,
            Self::Export => 1,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Digest(pub [u8; 32]);
impl Display for Digest {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub budget: m::Limits,
    pub types: usize,
    pub functions: usize,
    pub descriptor_bytes: usize,
    pub header_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            budget: m::Limits::default(),
            types: 65_536,
            functions: 4096,
            descriptor_bytes: MAX_DESCRIPTOR_BYTES,
            header_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    DescriptorBytes,
    HeaderBytes,
    Functions,
    Parameters,
    Depth,
    Fields,
    Cases,
    Payloads,
    Types,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionKind {
    FunctionDigest,
    TypeDigest,
    Symbol,
    Alias,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Resource(m::Error),
    Limit {
        kind: LimitKind,
        requested: usize,
        limit: usize,
    },
    InvalidType(o::Type),
    InvalidBuiltin(o::Builtin),
    Overflow,
    Allocation,
    Collision(CollisionKind),
    InvalidName,
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid C ABI artifact: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<m::Error> for Error {
    fn from(error: m::Error) -> Self {
        Self::Resource(error)
    }
}
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeId(pub usize);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: TypeId,
    pub offset: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Case {
    pub name: String,
    pub payload: Vec<TypeId>,
    pub offsets: Vec<u64>,
    pub size: u64,
    pub align: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    I64,
    Bool,
    Unit,
    Record {
        name: String,
        fields: Vec<Field>,
    },
    Enum {
        name: String,
        cases: Vec<Case>,
        payload_offset: u64,
        /// Actual union extent, excluding the enclosing struct's tail padding.
        payload_size: u64,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Type {
    pub kind: Kind,
    pub size: u64,
    pub align: u64,
    pub depth: usize,
    pub digest: Digest,
    descriptor: Vec<u8>,
}
impl Type {
    pub fn descriptor(&self) -> &[u8] {
        &self.descriptor
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub name: String,
    pub direction: Direction,
    pub parameters: Vec<TypeId>,
    pub result: TypeId,
    pub digest: Digest,
    pub symbol: String,
    pub span: m::Span,
    descriptor: Vec<u8>,
}
impl Function {
    pub fn descriptor(&self) -> &[u8] {
        &self.descriptor
    }
}
#[derive(Debug)]
pub struct Artifact {
    target: Target,
    types: Vec<Type>,
    functions: Vec<Function>,
    digest: Digest,
    descriptor: Vec<u8>,
}
impl Artifact {
    pub fn compile(source: &VerifiedDeclarations, target: Target, limits: Limits) -> Result<Self> {
        Builder::new(source, target, limits).compile()
    }
    pub const fn target(&self) -> Target {
        self.target
    }
    pub fn types(&self) -> &[Type] {
        &self.types
    }
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }
    pub const fn digest(&self) -> Digest {
        self.digest
    }
    pub fn descriptor(&self) -> &[u8] {
        &self.descriptor
    }
}

fn cap(kind: LimitKind, requested: usize, limit: usize) -> Result<()> {
    if requested > limit {
        Err(Error::Limit {
            kind,
            requested,
            limit,
        })
    } else {
        Ok(())
    }
}
fn push<T>(budget: &mut m::Budget, out: &mut Vec<T>, value: T) -> Result<()> {
    budget.allocate::<T>(1)?;
    out.try_reserve(1).map_err(|_| Error::Allocation)?;
    out.push(value);
    Ok(())
}
fn text(budget: &mut m::Budget, source: &str) -> Result<String> {
    budget.charge(source.len())?;
    budget.bytes(source.len())?;
    let mut out = String::new();
    out.try_reserve_exact(source.len())
        .map_err(|_| Error::Allocation)?;
    out.push_str(source);
    Ok(out)
}
fn align(size: u64, alignment: u64) -> Result<u64> {
    size.checked_add(alignment - 1)
        .map(|n| n & !(alignment - 1))
        .ok_or(Error::Overflow)
}
fn hash(budget: &mut m::Budget, bytes: &[u8]) -> Result<Digest> {
    budget.charge(bytes.len())?;
    Ok(Digest(Sha256::digest(bytes).into()))
}
struct Writer<'a> {
    budget: &'a mut m::Budget,
    bytes: Vec<u8>,
    limit: usize,
}
impl<'a> Writer<'a> {
    fn new(budget: &'a mut m::Budget, limit: usize) -> Self {
        Self {
            budget,
            bytes: Vec::new(),
            limit: limit.min(MAX_DESCRIPTOR_BYTES),
        }
    }
    fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let count = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(Error::Overflow)?;
        cap(LimitKind::DescriptorBytes, count, self.limit)?;
        self.budget.charge(bytes.len())?;
        self.budget.allocate::<u8>(bytes.len())?;
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| Error::Allocation)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn count(&mut self, count: usize) -> Result<()> {
        self.bytes(
            &u32::try_from(count)
                .map_err(|_| Error::Overflow)?
                .to_le_bytes(),
        )
    }
    fn string(&mut self, value: &str) -> Result<()> {
        self.count(value.len())?;
        self.bytes(value.as_bytes())
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Key {
    Mir(o::Type),
    Builtin(o::Builtin),
}
struct Builder<'a> {
    source: &'a VerifiedDeclarations,
    target: Target,
    limits: Limits,
    budget: m::Budget,
    keys: Vec<(Key, TypeId)>,
    types: Vec<Type>,
    functions: Vec<Function>,
    // Prefix/profile/target/vector count; each function adds its u32 byte
    // length and complete descriptor. Reserve this shared bound before hash.
    artifact_bytes: usize,
}
impl<'a> Builder<'a> {
    fn new(source: &'a VerifiedDeclarations, target: Target, limits: Limits) -> Self {
        Self {
            source,
            target,
            limits,
            budget: m::Budget::new(limits.budget),
            keys: Vec::new(),
            types: Vec::new(),
            functions: Vec::new(),
            artifact_bytes: 9 + 2 + 1 + 4,
        }
    }
    fn compile(mut self) -> Result<Artifact> {
        for import in self.source.imports() {
            self.budget.charge(1)?;
            if import.abi == ImportAbi::C {
                self.function(
                    &import.name,
                    Direction::Import,
                    &import.parameters,
                    import.result,
                    import.span,
                )?;
            }
        }
        for function in self.source.functions() {
            self.budget.charge(1)?;
            if function.abi == FunctionAbi::CExport {
                let mut parameters = self.budget.vector(function.parameters.len())?;
                for local in &function.parameters {
                    parameters.push(function.locals[local.0 as usize].ty);
                }
                self.function(
                    &function.name,
                    Direction::Export,
                    &parameters,
                    function.result,
                    function.signature_span,
                )?;
            }
        }
        let mut order: Vec<usize> = self.budget.vector(self.functions.len())?;
        order.extend(0..self.functions.len());
        // Bound comparisons by the bounded descriptor size, including long
        // common prefixes; sorting must consume the same generation budget.
        let comparisons = order
            .len()
            .saturating_mul(order.len().next_power_of_two().ilog2() as usize + 1);
        self.budget.charge(
            comparisons
                .checked_mul(
                    self.functions
                        .iter()
                        .map(|f| f.descriptor.len())
                        .max()
                        .unwrap_or(0),
                )
                .ok_or(Error::Overflow)?,
        )?;
        order.sort_unstable_by(|a, b| {
            self.functions[*a]
                .descriptor
                .cmp(&self.functions[*b].descriptor)
        });
        let mut writer = Writer::new(&mut self.budget, self.limits.descriptor_bytes);
        writer.bytes(b"LANGCART\0")?;
        writer.bytes(&PROFILE.to_le_bytes())?;
        writer.bytes(&[self.target.tag()])?;
        writer.count(order.len())?;
        for i in order {
            writer.count(self.functions[i].descriptor.len())?;
            writer.bytes(&self.functions[i].descriptor)?;
        }
        let descriptor = writer.bytes;
        let digest = hash(&mut self.budget, &descriptor)?;
        Ok(Artifact {
            target: self.target,
            types: self.types,
            functions: self.functions,
            digest,
            descriptor,
        })
    }
    fn function(
        &mut self,
        name: &str,
        direction: Direction,
        parameters: &[o::TypeUse],
        result: o::TypeUse,
        span: m::Span,
    ) -> Result<()> {
        cap(
            LimitKind::Functions,
            self.functions.len() + 1,
            self.limits.functions.min(4096),
        )?;
        cap(LimitKind::Parameters, parameters.len(), 256)?;
        let mut types = self.budget.vector(parameters.len())?;
        for parameter in parameters {
            types.push(self.ty(Key::Mir(parameter.ty), 1)?);
        }
        let result = self.ty(Key::Mir(result.ty), 1)?;
        let limit = self.limits.descriptor_bytes.min(MAX_DESCRIPTOR_BYTES);
        let prefix = self.artifact_bytes.checked_add(4).ok_or(Error::Overflow)?;
        cap(LimitKind::DescriptorBytes, prefix, limit)?;
        let mut writer = Writer::new(&mut self.budget, limit - prefix);
        writer.bytes(b"LANGCABI\0")?;
        writer.bytes(&PROFILE.to_le_bytes())?;
        writer.bytes(&[self.target.tag(), direction.tag()])?;
        writer.string(name)?;
        writer.count(types.len())?;
        for id in &types {
            writer.bytes(&self.types[id.0].descriptor)?;
        }
        writer.bytes(&self.types[result.0].descriptor)?;
        let descriptor = writer.bytes;
        self.artifact_bytes = prefix
            .checked_add(descriptor.len())
            .ok_or(Error::Overflow)?;
        let digest = hash(&mut self.budget, &descriptor)?;
        for old in &self.functions {
            self.budget
                .charge(old.name.len().min(name.len()).saturating_add(33))?;
            if old.name == name || old.digest == digest {
                return Err(Error::Collision(CollisionKind::FunctionDigest));
            }
        }
        let root = name.rsplit("::").next().ok_or(Error::InvalidName)?;
        if root.is_empty()
            || !root
                .bytes()
                .enumerate()
                .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
        {
            return Err(Error::InvalidName);
        }
        let symbol = text(&mut self.budget, &format!("lang_{root}_{digest}"))?;
        let name = text(&mut self.budget, name)?;
        push(
            &mut self.budget,
            &mut self.functions,
            Function {
                name,
                direction,
                parameters: types,
                result,
                digest,
                symbol,
                span,
                descriptor,
            },
        )
    }
    fn ty(&mut self, mut key: Key, depth: usize) -> Result<TypeId> {
        self.budget.charge(1)?;
        cap(LimitKind::Depth, depth, 64)?;
        if let Key::Mir(o::Type::Constructed(id)) = key
            && let o::Constructor::Builtin(builtin) =
                self.source.types().table().constructed[id.0 as usize].constructor
        {
            key = Key::Builtin(builtin);
        }
        self.budget.charge(self.keys.len())?;
        if let Some((_, id)) = self.keys.iter().find(|(k, _)| *k == key) {
            cap(LimitKind::Depth, depth + self.types[id.0].depth - 1, 64)?;
            return Ok(*id);
        }
        let kind = match key {
            Key::Mir(o::Type::I64) => Kind::I64,
            Key::Mir(o::Type::Bool) => Kind::Bool,
            Key::Builtin(o::Builtin::Unit) => Kind::Unit,
            Key::Mir(o::Type::Nominal(id)) => {
                let definition = &self.source.types().table().definitions[id.0 as usize];
                let name = text(&mut self.budget, &definition.name)?;
                match &definition.kind {
                    o::DefinitionKind::Record(fields) => {
                        cap(LimitKind::Fields, fields.len(), 256)?;
                        let mut out = self.budget.vector(fields.len())?;
                        let mut offset = 0;
                        for field in fields {
                            let ty = self.ty(Key::Mir(field.ty.ty), depth + 1)?;
                            offset = align(offset, self.types[ty.0].align)?;
                            out.push(Field {
                                name: text(&mut self.budget, &field.name)?,
                                ty,
                                offset,
                            });
                            offset = offset
                                .checked_add(self.types[ty.0].size)
                                .ok_or(Error::Overflow)?;
                        }
                        Kind::Record { name, fields: out }
                    }
                    o::DefinitionKind::Enum(cases) => {
                        cap(LimitKind::Cases, cases.len(), 256)?;
                        let mut out = self.budget.vector(cases.len())?;
                        for case in cases {
                            let mut payload = self.budget.vector(case.payload.len())?;
                            for value in &case.payload {
                                payload.push(self.ty(Key::Mir(value.ty), depth + 1)?);
                            }
                            let name = text(&mut self.budget, &case.name)?;
                            out.push(self.case(name, payload)?);
                        }
                        Kind::Enum {
                            name,
                            cases: out,
                            payload_offset: 0,
                            payload_size: 0,
                        }
                    }
                }
            }
            Key::Mir(o::Type::Constructed(id)) => {
                let (name, values) = match self.source.types().table().constructed[id.0 as usize]
                    .constructor
                {
                    o::Constructor::Option(value) => ("core::Option", [None, Some(value)]),
                    o::Constructor::Result(ok, error) => ("core::Result", [Some(ok), Some(error)]),
                    _ => return Err(Error::InvalidType(o::Type::Constructed(id))),
                };
                let mut cases = self.budget.vector(2)?;
                for (index, value) in values.into_iter().enumerate() {
                    let mut payload = self.budget.vector(usize::from(value.is_some()))?;
                    if let Some(value) = value {
                        payload.push(self.ty(Key::Mir(value), depth + 1)?);
                    }
                    let case = match (name, index) {
                        ("core::Option", 0) => "None",
                        ("core::Option", _) => "Some",
                        (_, 0) => "Ok",
                        _ => "Err",
                    };
                    let case = text(&mut self.budget, case)?;
                    cases.push(self.case(case, payload)?);
                }
                Kind::Enum {
                    name: text(&mut self.budget, name)?,
                    cases,
                    payload_offset: 0,
                    payload_size: 0,
                }
            }
            Key::Builtin(builtin) => self.builtin(builtin, depth)?,
        };
        self.intern(key, kind)
    }
    fn case(&mut self, name: String, payload: Vec<TypeId>) -> Result<Case> {
        cap(LimitKind::Payloads, payload.len(), 256)?;
        let mut offsets = self.budget.vector(payload.len())?;
        let mut size: u64 = 0;
        let mut alignment = 1;
        for id in &payload {
            self.budget.charge(1)?;
            let ty = &self.types[id.0];
            size = align(size, ty.align)?;
            offsets.push(size);
            size = size.checked_add(ty.size).ok_or(Error::Overflow)?;
            alignment = alignment.max(ty.align);
        }
        Ok(Case {
            name,
            payload,
            offsets,
            size: align(size.max(1), alignment)?,
            align: alignment,
        })
    }
    fn builtin(&mut self, builtin: o::Builtin, depth: usize) -> Result<Kind> {
        use o::Builtin as B;
        let (name, names): (&str, &[&str]) = match builtin {
            B::AllocError => ("core::AllocError", &["Limit", "OutOfMemory", "Overflow"]),
            B::BoundsError => ("core::BoundsError", &["OutOfBounds"]),
            B::BufferError => (
                "core::BufferError",
                &["Bounds", "InvalidByte", "Allocation"],
            ),
            B::Utf8Error => ("core::Utf8Error", &["Invalid"]),
            B::OwnershipError => (
                "core::OwnershipError",
                &[
                    "Allocation",
                    "StrongCycle",
                    "ExpiredWeak",
                    "BorrowConflict",
                    "Unavailable",
                    "CountOverflow",
                ],
            ),
            B::ArenaError => (
                "core::ArenaError",
                &["Allocation", "CrossRegion", "Unavailable", "BorrowConflict"],
            ),
            _ => return Err(Error::InvalidBuiltin(builtin)),
        };
        let mut cases = self.budget.vector(names.len())?;
        for case in names {
            let child = if *case == "Allocation" {
                Some(Key::Builtin(B::AllocError))
            } else if builtin == B::Utf8Error {
                Some(Key::Mir(o::Type::I64))
            } else {
                None
            };
            let mut payload = self.budget.vector(usize::from(child.is_some()))?;
            if let Some(child) = child {
                payload.push(self.ty(child, depth + 1)?);
            }
            let case = text(&mut self.budget, case)?;
            cases.push(self.case(case, payload)?);
        }
        Ok(Kind::Enum {
            name: text(&mut self.budget, name)?,
            cases,
            payload_offset: 0,
            payload_size: 0,
        })
    }
    fn intern(&mut self, key: Key, mut kind: Kind) -> Result<TypeId> {
        let mut writer = Writer::new(&mut self.budget, self.limits.descriptor_bytes);
        let (size, alignment, depth) = match &mut kind {
            Kind::I64 => {
                writer.bytes(&[0])?;
                (8, 8, 1)
            }
            Kind::Bool => {
                writer.bytes(&[1])?;
                (1, 1, 1)
            }
            Kind::Unit => {
                writer.bytes(&[2])?;
                (1, 1, 1)
            }
            Kind::Record { name, fields } => {
                writer.bytes(&[3])?;
                writer.string(name)?;
                writer.count(fields.len())?;
                let mut size = 0;
                let mut alignment = 1;
                let mut depth = 1;
                for field in fields {
                    let ty = &self.types[field.ty.0];
                    writer.string(&field.name)?;
                    writer.bytes(&ty.descriptor)?;
                    size = field.offset.checked_add(ty.size).ok_or(Error::Overflow)?;
                    alignment = alignment.max(ty.align);
                    depth = depth.max(ty.depth + 1);
                }
                (align(size.max(1), alignment)?, alignment, depth)
            }
            Kind::Enum {
                name,
                cases,
                payload_offset,
                payload_size,
            } => {
                writer.bytes(&[4])?;
                writer.string(name)?;
                writer.count(cases.len())?;
                let mut size = 1;
                let mut alignment = 1;
                let mut depth = 1;
                for case in cases {
                    writer.string(&case.name)?;
                    writer.count(case.payload.len())?;
                    for id in &case.payload {
                        let ty = &self.types[id.0];
                        writer.bytes(&ty.descriptor)?;
                        depth = depth.max(ty.depth + 1);
                    }
                    size = size.max(case.size);
                    alignment = alignment.max(case.align);
                }
                *payload_offset = align(4, alignment)?;
                *payload_size = align(size, alignment)?;
                (
                    align(
                        payload_offset.checked_add(size).ok_or(Error::Overflow)?,
                        alignment.max(4),
                    )?,
                    alignment.max(4),
                    depth,
                )
            }
        };
        cap(LimitKind::Depth, depth, 64)?;
        let descriptor = writer.bytes;
        let mut digest_bytes = Writer::new(&mut self.budget, self.limits.descriptor_bytes);
        digest_bytes.bytes(b"LANGCTYPE\0")?;
        digest_bytes.bytes(&PROFILE.to_le_bytes())?;
        digest_bytes.bytes(&[self.target.tag()])?;
        digest_bytes.bytes(&descriptor)?;
        let digest_bytes = digest_bytes.bytes;
        let digest = hash(&mut self.budget, &digest_bytes)?;
        self.budget.charge(self.types.len())?;
        for (i, ty) in self.types.iter().enumerate() {
            if ty.digest == digest {
                if ty.descriptor != descriptor {
                    return Err(Error::Collision(CollisionKind::TypeDigest));
                }
                let id = TypeId(i);
                push(&mut self.budget, &mut self.keys, (key, id))?;
                return Ok(id);
            }
        }
        cap(LimitKind::Types, self.types.len() + 1, self.limits.types)?;
        let id = TypeId(self.types.len());
        push(
            &mut self.budget,
            &mut self.types,
            Type {
                kind,
                size,
                align: alignment,
                depth,
                digest,
                descriptor,
            },
        )?;
        push(&mut self.budget, &mut self.keys, (key, id))?;
        Ok(id)
    }
}
