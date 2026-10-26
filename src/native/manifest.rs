//! Bounded RFC 0020 data validation; no native execution authority is created.
use serde::{
    Deserialize,
    de::{self, SeqAccess, Visitor},
};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hex(pub [u8; 32]);
impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Hex;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("64 lowercase hexadecimal digits")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Hex, E> {
                if s.len() != 64
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(E::custom("invalid SHA-256/build identifier"));
                }
                let mut bytes = [0; 32];
                for (i, out) in bytes.iter_mut().enumerate() {
                    *out = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(E::custom)?;
                }
                Ok(Hex(bytes))
            }
        }
        d.deserialize_str(V)
    }
}
#[derive(Debug)]
struct Table<T, const N: usize>(Vec<T>);
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Table<T, N> {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T, const N: usize>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for V<T, N> {
            type Value = Table<T, N>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "at most {N} files")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                loop {
                    if values.len() == N {
                        if seq.next_element::<de::IgnoredAny>()?.is_some() {
                            return Err(de::Error::custom(format_args!(
                                "file table limit {N}; observed at least {}",
                                N + 1
                            )));
                        }
                        break;
                    }
                    let Some(value) = seq.next_element()? else {
                        break;
                    };
                    values.try_reserve(1).map_err(de::Error::custom)?;
                    values.push(value);
                }
                Ok(Table(values))
            }
        }
        d.deserialize_seq(V::<T, N>(std::marker::PhantomData))
    }
}

#[derive(Debug)]
pub struct Error {
    pub path: Box<std::path::Path>,
    pub kind: ErrorKind,
}
#[derive(Debug)]
pub enum ErrorKind {
    Io(std::io::Error),
    Json(serde_json::Error),
    Limit {
        resource: &'static str,
        maximum: u64,
        actual: u64,
    },
    Invalid {
        field: &'static str,
    },
    Mismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    Capacity,
    CopyCodec(lang_bytecode::unit::CodecError),
    OwnedCodec(lang_bytecode::ownership::CodecError),
    TaskCodec(Box<lang_bytecode::ownership::CodecError>),
    ComponentCodec(Box<crate::artifact::component::Error>),
    TaskPlan(Box<lang_codegen_clif::tasks::Error>),
    Record(lang_codegen_clif::native_record::Error),
    CopyPlan(lang_codegen_clif::program::BindError),
    OwnedPlan(lang_codegen_clif::owned::RecordedError),
    Layout(lang_runtime::layout::Error),
    CMetadata(lang_runtime::c_abi::Error),
    CPlan(lang_codegen_clif::owned::CObjectError),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "native package data at {}: {:?}",
            self.path.display(),
            self.kind
        )
    }
}
impl std::error::Error for Error {}
fn error(path: impl Into<std::path::PathBuf>, kind: ErrorKind) -> Error {
    Error {
        path: path.into().into_boxed_path(),
        kind,
    }
}
fn at(mut failure: Error, path: impl Into<std::path::PathBuf>) -> Error {
    if failure.path.as_os_str().is_empty() {
        failure.path = path.into().into_boxed_path();
    }
    failure
}
fn invalid(field: &'static str) -> Error {
    error("", ErrorKind::Invalid { field })
}
fn mismatch(field: &'static str, expected: impl Into<String>, actual: impl Into<String>) -> Error {
    error(
        "",
        ErrorKind::Mismatch {
            field,
            expected: expected.into(),
            actual: actual.into(),
        },
    )
}
fn bound(resource: &'static str, actual: u64, maximum: u64) -> Result<(), Error> {
    if actual > maximum {
        Err(error(
            "",
            ErrorKind::Limit {
                resource,
                maximum,
                actual,
            },
        ))
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub sdk_manifest: usize,
    pub program_manifest: usize,
    pub depth: usize,
    pub sdk_file: u64,
    pub sdk_total: u64,
    pub program_total: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sdk_manifest: 1024 * 1024,
            program_manifest: 64 * 1024,
            depth: 32,
            sdk_file: 2 * 1024 * 1024 * 1024,
            sdk_total: 8 * 1024 * 1024 * 1024,
            program_total: 256 * 1024 * 1024,
        }
    }
}
fn parse<'de, T: Deserialize<'de>>(
    bytes: &'de [u8],
    maximum: usize,
    depth: usize,
) -> Result<T, Error> {
    bound("manifest bytes", bytes.len() as u64, maximum as u64)?;
    let (mut level, mut string, mut escaped) = (0usize, false, false);
    for &b in bytes {
        if string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                string = false;
            }
        } else {
            match b {
                b'"' => string = true,
                b'{' | b'[' => {
                    level += 1;
                    bound("JSON depth", level as u64, depth as u64)?;
                }
                b'}' | b']' => level = level.saturating_sub(1),
                _ => {}
            }
        }
    }
    serde_json::from_slice(bytes).map_err(|e| error("", ErrorKind::Json(e)))
}
#[derive(Debug)]
struct Text<const N: usize>(String);
impl<'de, const N: usize> Deserialize<'de> for Text<N> {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<const N: usize>;
        impl<const N: usize> Visitor<'_> for V<N> {
            type Value = Text<N>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a string of at most {N} UTF-8 bytes")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
                if s.len() > N {
                    return Err(E::custom(format_args!(
                        "string byte limit {N}; observed {}",
                        s.len()
                    )));
                }
                let mut text = String::new();
                text.try_reserve(s.len()).map_err(E::custom)?;
                text.push_str(s);
                Ok(Text(text))
            }
        }
        d.deserialize_str(V::<N>)
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum Target {
    #[serde(rename = "aarch64-apple-darwin")]
    MacosAarch64,
    #[serde(rename = "x86_64-pc-windows-msvc")]
    WindowsX86_64,
}
impl Target {
    pub fn text(self) -> &'static str {
        match self {
            Self::MacosAarch64 => "aarch64-apple-darwin",
            Self::WindowsX86_64 => "x86_64-pc-windows-msvc",
        }
    }
    pub fn triple(self) -> lang_codegen_clif::Triple {
        self.text().parse().expect("fixed target triple")
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Copy,
    Owned,
    C,
    Task,
    Component,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Compiler,
    Runtime,
    Dependency,
    Header,
    Startup,
    Documentation,
    Semantic,
    NativeRecord,
    Object,
    Bindings,
    Main,
    CHeader,
}
impl Role {
    fn sdk(self) -> bool {
        matches!(
            self,
            Self::Compiler
                | Self::Runtime
                | Self::Dependency
                | Self::Header
                | Self::Startup
                | Self::Documentation
        )
    }
    fn singleton(self) -> bool {
        !matches!(
            self,
            Self::Dependency | Self::Header | Self::Startup | Self::Documentation
        )
    }
    fn program_path(self, target: Target) -> Option<&'static str> {
        Some(match self {
            Self::Semantic => "program.lbc",
            Self::NativeRecord => "program.native",
            Self::Object => {
                if target == Target::MacosAarch64 {
                    "program.o"
                } else {
                    "program.obj"
                }
            }
            Self::Bindings => "bindings.rs",
            Self::Main => "main.rs",
            Self::CHeader => "program.h",
            _ => return None,
        })
    }
    fn program_limit(self) -> u64 {
        match self {
            Self::Bindings | Self::Main => 8 * 1024 * 1024,
            Self::CHeader => 16 * 1024 * 1024,
            _ => 64 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    role: Role,
    path: Text<4096>,
    bytes: u64,
    sha256: Hex,
}
impl File {
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn path(&self) -> &str {
        &self.path.0
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn hash(&self) -> &Hex {
        &self.sha256
    }
}
#[derive(Debug)]
struct Entry(Option<u32>);
impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Entry;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an explicit null or u32 entry")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Entry, E> {
                Ok(Entry(None))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Entry, E> {
                Ok(Entry(Some(u32::try_from(v).map_err(E::custom)?)))
            }
        }
        d.deserialize_any(V)
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProgram {
    format: Text<32>,
    version: u32,
    build_id: Hex,
    target: Target,
    profile: Profile,
    entry: Entry,
    files: Table<File, 8>,
}
#[derive(Debug)]
pub struct Program(RawProgram);
impl Program {
    pub fn version(&self) -> u32 {
        self.0.version
    }
    pub fn target(&self) -> Target {
        self.0.target
    }
    pub fn profile(&self) -> Profile {
        self.0.profile
    }
    pub fn entry(&self) -> Option<u32> {
        self.0.entry.0
    }
    pub fn build_id(&self) -> &Hex {
        &self.0.build_id
    }
    pub fn files(&self) -> &[File] {
        &self.0.files.0
    }
    pub fn file(&self, role: Role) -> Option<&File> {
        self.files().iter().find(|f| f.role == role)
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rustc {
    release: Text<128>,
    commit: Text<128>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSdk {
    format: Text<32>,
    version: u32,
    build_id: Hex,
    target: Target,
    rustc: Rustc,
    build_profile: Text<16>,
    feature_policy: Text<128>,
    files: Table<File, 4096>,
}
#[derive(Debug)]
pub struct Sdk(RawSdk);
impl Sdk {
    pub fn version(&self) -> u32 {
        self.0.version
    }
    pub fn target(&self) -> Target {
        self.0.target
    }
    pub fn build_id(&self) -> &Hex {
        &self.0.build_id
    }
    pub fn rustc_release(&self) -> &str {
        &self.0.rustc.release.0
    }
    pub fn rustc_commit(&self) -> &str {
        &self.0.rustc.commit.0
    }
    pub fn build_profile(&self) -> &str {
        &self.0.build_profile.0
    }
    pub fn feature_policy(&self) -> &str {
        &self.0.feature_policy.0
    }
    pub fn files(&self) -> &[File] {
        &self.0.files.0
    }
}
fn path(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.contains(['\\', ':', '<', '>', '"', '|', '?', '*'])
        || value.chars().any(|c| c < ' ')
    {
        return Err(invalid("relative file path"));
    }
    for part in value.split('/') {
        if part.is_empty() || matches!(part, "." | "..") || part.ends_with(['.', ' ']) {
            return Err(invalid("path component"));
        }
        let stem = part
            .split('.')
            .next()
            .unwrap()
            .trim_end_matches([' ', '.'])
            .to_ascii_uppercase();
        if matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
        ) || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|n| {
                matches!(
                    n,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        }) {
            return Err(invalid("Windows device path"));
        }
    }
    Ok(())
}
fn common(
    files: &[File],
    sdk: bool,
    target: Target,
    limits: Limits,
    manifest_bytes: usize,
) -> Result<std::collections::BTreeSet<Role>, Error> {
    use std::collections::BTreeSet;
    let (mut roles, mut names, mut total) = (
        BTreeSet::new(),
        BTreeSet::<String>::new(),
        manifest_bytes as u64,
    );
    for f in files {
        path(f.path())?;
        let name = f.path().to_lowercase();
        if name.split('/').next() == Some(if sdk { "sdk.json" } else { "package.json" }) {
            return Err(invalid("manifest as component"));
        }
        if !names.insert(name) {
            return Err(invalid("duplicate path"));
        }
        if f.role.sdk() != sdk {
            return Err(invalid("file role"));
        }
        if f.role.singleton() && !roles.insert(f.role) {
            return Err(invalid("duplicate singleton role"));
        }
        if !sdk {
            if f.role.program_path(target) != Some(f.path()) {
                return Err(invalid("program role path"));
            }
            bound("program component bytes", f.bytes, f.role.program_limit())?;
        } else {
            bound("SDK file bytes", f.bytes, limits.sdk_file)?;
        }
        total = total
            .checked_add(f.bytes)
            .ok_or_else(|| invalid("total file size"))?;
        bound(
            "total file bytes",
            total,
            if sdk {
                limits.sdk_total
            } else {
                limits.program_total
            },
        )?;
    }
    for name in &names {
        let mut parent = name.as_str();
        while let Some((p, _)) = parent.rsplit_once('/') {
            if names.contains(p) {
                return Err(invalid("file/directory path conflict"));
            }
            parent = p;
        }
    }
    Ok(roles)
}
pub fn parse_program(bytes: &[u8], limits: Limits) -> Result<Program, Error> {
    let p: RawProgram = parse(bytes, limits.program_manifest, limits.depth)?;
    if p.format.0 != "lang-native-program" || !matches!(p.version, 1..=3) {
        return Err(invalid("program format/version"));
    }
    if !matches!(
        (p.version, p.profile),
        (1, Profile::Copy | Profile::Owned | Profile::C)
            | (2, Profile::Task)
            | (3, Profile::Component)
    ) {
        return Err(invalid("program version/profile"));
    }
    let roles = common(&p.files.0, false, p.target, limits, bytes.len())?;
    let required = [
        Role::Semantic,
        Role::NativeRecord,
        Role::Object,
        Role::Bindings,
    ];
    let extra = match p.profile {
        Profile::C => Some(Role::CHeader),
        Profile::Component => None,
        _ => Some(Role::Main),
    };
    if roles.len() != required.len() + usize::from(extra.is_some())
        || required.iter().any(|r| !roles.contains(r))
        || extra.is_some_and(|r| !roles.contains(&r))
    {
        return Err(invalid("program roles/profile"));
    }
    if (p.profile == Profile::C) != p.entry.0.is_none() {
        return Err(invalid("entry/profile"));
    }
    Ok(Program(p))
}
pub fn parse_sdk(bytes: &[u8], limits: Limits) -> Result<Sdk, Error> {
    let s: RawSdk = parse(bytes, limits.sdk_manifest, limits.depth)?;
    if s.format.0 != "lang-native-sdk" || !matches!(s.version, 1..=3) {
        return Err(invalid("SDK format/version"));
    }
    if !matches!(s.build_profile.0.as_str(), "debug" | "release") {
        return Err(invalid("build profile"));
    }
    for (name, value) in [
        ("rustc release", &s.rustc.release.0),
        ("rustc commit", &s.rustc.commit.0),
    ] {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(invalid(name));
        }
    }
    if s.feature_policy.0.is_empty()
        || !s
            .feature_policy
            .0
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(invalid("feature policy"));
    }
    let roles = common(&s.files.0, true, s.target, limits, bytes.len())?;
    if !roles.contains(&Role::Compiler) || !roles.contains(&Role::Runtime) {
        return Err(invalid("required SDK role"));
    }
    Ok(Sdk(s))
}

impl fmt::Display for Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
mod snapshot;
pub use snapshot::{ProgramSnapshot, SdkSnapshot};
#[cfg(test)]
mod limits_tests;
