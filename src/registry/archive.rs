//! RFC 0046's finite, canonical USTAR source profile, without extraction authority.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::io::{self, Write};
use std::marker::PhantomData;
use std::ops::Range;

use serde::{Deserialize, Deserializer, Serialize, de};
use sha2::{Digest, Sha256};

use crate::package::compatibility::SourceEdition;
use crate::package::manifest::{TARGETS, valid_name, valid_version};

pub const MAX_ARCHIVE_BYTES: usize = 18 * 1024 * 1024;
pub const MAX_DESCRIPTOR_BYTES: usize = 128 * 1024;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_TOTAL_SOURCE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FILES: usize = 256;
pub const MAX_DEPENDENCIES: usize = 128;
pub const MAX_TESTS: usize = 128;
pub const MAX_PATH_BYTES: usize = 100;
pub const MAX_JSON_DEPTH: usize = 16;
const BLOCK: usize = 512;
const DESCRIPTOR: &str = ".lang-package.json";
const SEQUENCE_LIMIT: &str = "registry sequence limit exceeded";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Malformed,
    Limit,
    Integrity,
}

/// Bounded diagnostics never echo hostile metadata or source bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub offset: Option<usize>,
    pub message: &'static str,
}

impl Error {
    pub const fn code(&self) -> &'static str {
        match self.kind {
            ErrorKind::Malformed => "REG0001",
            ErrorKind::Limit => "REG0002",
            ErrorKind::Integrity => "REG0006",
        }
    }

    fn at(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message)?;
        if let Some(offset) = self.offset {
            write!(f, " (archive byte {offset})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

fn error(kind: ErrorKind, message: &'static str) -> Error {
    Error {
        kind,
        offset: None,
        message,
    }
}

fn malformed(message: &'static str) -> Error {
    error(ErrorKind::Malformed, message)
}

fn limit(message: &'static str) -> Error {
    error(ErrorKind::Limit, message)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    name: String,
    version: String,
    edition: String,
    entry: String,
}

impl Package {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn edition(&self) -> &str {
        &self.edition
    }
    pub fn entry(&self) -> &str {
        &self.entry
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    alias: String,
    registry: String,
    name: String,
    version: String,
    archive_sha256: String,
    #[serde(deserialize_with = "targets")]
    targets: Vec<String>,
}

impl Dependency {
    pub fn alias(&self) -> &str {
        &self.alias
    }
    pub fn registry(&self) -> &str {
        &self.registry
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }
    pub fn targets(&self) -> &[String] {
        &self.targets
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    path: String,
    bytes: u64,
    sha256: String,
}

impl File {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Metadata is inspectable; only `AdmittedArchive` carries admission evidence.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    format: String,
    schema: u32,
    package: Package,
    #[serde(deserialize_with = "tests")]
    tests: Vec<String>,
    #[serde(deserialize_with = "dependencies")]
    dependencies: Vec<Dependency>,
    #[serde(deserialize_with = "files")]
    files: Vec<File>,
}

impl Descriptor {
    pub fn package(&self) -> &Package {
        &self.package
    }
    pub fn tests(&self) -> &[String] {
        &self.tests
    }
    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }
    pub fn files(&self) -> &[File] {
        &self.files
    }
}

fn bounded_sequence<'de, D, T, const N: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T, const N: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> de::Visitor<'de> for Visitor<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "an array with at most {N} records")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut values = Vec::new();
            while values.len() < N {
                let Some(value) = seq.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            // Depth and descriptor bytes are bounded before serde. Inspect one
            // excess value without allocating a domain record for it.
            if seq.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(SEQUENCE_LIMIT));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor::<T, N>(PhantomData))
}

fn files<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<File>, D::Error> {
    bounded_sequence::<D, File, MAX_FILES>(d)
}
fn tests<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    bounded_sequence::<D, String, MAX_TESTS>(d)
}
fn dependencies<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Dependency>, D::Error> {
    bounded_sequence::<D, Dependency, MAX_DEPENDENCIES>(d)
}
fn targets<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    bounded_sequence::<D, String, 2>(d)
}

/// An owned frozen archive: callers cannot substitute bytes after validation.
/// This type grants no filesystem, network, publication or execution authority.
#[derive(Debug)]
pub struct AdmittedArchive {
    bytes: Vec<u8>,
    descriptor: Descriptor,
    sources: Vec<Range<usize>>,
    sha256: String,
    descriptor_sha256: String,
}

impl AdmittedArchive {
    pub fn admit(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_ARCHIVE_BYTES {
            return Err(limit("archive byte limit exceeded"));
        }
        let (metadata, mut offset) = entry(bytes, 0, DESCRIPTOR, MAX_DESCRIPTOR_BYTES)?;
        let descriptor_bytes = &bytes[metadata];
        json_depth(descriptor_bytes)?;
        let descriptor: Descriptor = serde_json::from_slice(descriptor_bytes).map_err(|e| {
            if e.to_string().starts_with(SEQUENCE_LIMIT) {
                limit("descriptor record limit exceeded")
            } else {
                malformed("invalid closed descriptor JSON")
            }
        })?;
        validate_descriptor(&descriptor)?;
        let mut canonical = BoundedWriter(Vec::new());
        serde_json::to_writer_pretty(&mut canonical, &descriptor)
            .map_err(|_| limit("canonical descriptor byte limit exceeded"))?;
        canonical
            .write_all(b"\n")
            .map_err(|_| limit("canonical descriptor byte limit exceeded"))?;
        if canonical.0 != descriptor_bytes {
            return Err(malformed("descriptor JSON is not canonical"));
        }
        let mut sources = Vec::new();
        for file in &descriptor.files {
            let (range, next) = entry(bytes, offset, &file.path, MAX_SOURCE_BYTES)?;
            let source = &bytes[range.clone()];
            if source.len() as u64 != file.bytes || digest(source) != file.sha256 {
                return Err(
                    error(ErrorKind::Integrity, "source length or digest mismatch").at(offset),
                );
            }
            if std::str::from_utf8(source).is_err() {
                return Err(malformed("source is not UTF-8").at(range.start));
            }
            sources.push(range);
            offset = next;
        }
        let trailer = bytes
            .get(offset..)
            .ok_or_else(|| malformed("missing archive terminator"))?;
        if trailer.len() != BLOCK * 2 || trailer.iter().any(|b| *b != 0) {
            return Err(malformed("expected exactly two zero terminator blocks").at(offset));
        }
        let mut frozen = Vec::new();
        frozen
            .try_reserve_exact(bytes.len())
            .map_err(|_| limit("archive allocation refused"))?;
        frozen.extend_from_slice(bytes);
        Ok(Self {
            bytes: frozen,
            descriptor,
            sources,
            sha256: digest(bytes),
            descriptor_sha256: digest(descriptor_bytes),
        })
    }

    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn descriptor_sha256(&self) -> &str {
        &self.descriptor_sha256
    }

    /// Original UTF-8 bytes, with comments, offsets and line endings unchanged.
    pub fn source(&self, path: &str) -> Option<&[u8]> {
        let i = self
            .descriptor
            .files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .ok()?;
        self.bytes.get(self.sources.get(i)?.clone())
    }
}

fn digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 15)]));
    }
    text
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_descriptor(d: &Descriptor) -> Result<(), Error> {
    if d.format != "lang-registry-source" || d.schema != 1 {
        return Err(malformed("unsupported registry archive format or schema"));
    }
    if !valid_name(&d.package.name)
        || d.package.version.len() > 32
        || !valid_version(&d.package.version)
        || SourceEdition::parse(&d.package.edition).is_err()
    {
        return Err(malformed("invalid package identity or edition"));
    }
    let mut paths = BTreeMap::<String, (&str, bool)>::new();
    let mut file_names = BTreeSet::new();
    let mut total = 0_u64;
    let mut previous = None;
    for f in &d.files {
        validate_path(&f.path)?;
        if previous.is_some_and(|p| p >= f.path.as_str()) || !valid_hash(&f.sha256) {
            return Err(malformed(
                "file records must have unique sorted paths and valid digests",
            ));
        }
        previous = Some(f.path.as_str());
        if f.bytes > MAX_SOURCE_BYTES as u64 {
            return Err(limit("source byte limit exceeded"));
        }
        total = total
            .checked_add(f.bytes)
            .ok_or_else(|| limit("source byte sum overflow"))?;
        if total > MAX_TOTAL_SOURCE_BYTES as u64 {
            return Err(limit("aggregate source byte limit exceeded"));
        }
        file_names.insert(f.path.as_str());
        for end in f
            .path
            .match_indices('/')
            .map(|(i, _)| i)
            .chain(std::iter::once(f.path.len()))
        {
            let prefix = &f.path[..end];
            let is_file = end == f.path.len();
            match paths.entry(prefix.to_ascii_lowercase()) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert((prefix, is_file));
                }
                std::collections::btree_map::Entry::Occupied(slot) => {
                    let (prior, prior_file) = *slot.get();
                    if prior != prefix || prior_file || is_file {
                        return Err(malformed("case alias or file/directory prefix collision"));
                    }
                }
            }
        }
    }
    if !file_names.contains(d.package.entry.as_str()) {
        return Err(malformed("package entry is not a listed source"));
    }
    let mut tests = BTreeSet::new();
    for test in &d.tests {
        if !file_names.contains(test.as_str()) || !tests.insert(test) {
            return Err(malformed("test must name a unique listed source"));
        }
    }
    let mut previous = None;
    let mut identities = BTreeMap::new();
    for dependency in &d.dependencies {
        if !valid_name(&dependency.alias)
            || !valid_name(&dependency.registry)
            || !valid_name(&dependency.name)
            || dependency.version.len() > 32
            || !valid_version(&dependency.version)
            || !valid_hash(&dependency.archive_sha256)
            || previous.is_some_and(|p| p >= dependency.alias.as_str())
            || dependency.targets.is_empty()
            || dependency
                .targets
                .iter()
                .any(|t| !TARGETS.contains(&t.as_str()))
            || dependency.targets.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(malformed(
                "invalid or unordered dependency identity, alias or targets",
            ));
        }
        previous = Some(dependency.alias.as_str());
        let key = (&dependency.registry, &dependency.name, &dependency.version);
        if identities
            .insert(key, &dependency.archive_sha256)
            .is_some_and(|prior| prior != &dependency.archive_sha256)
        {
            return Err(error(
                ErrorKind::Integrity,
                "one dependency version has conflicting digests",
            ));
        }
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), Error> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES || !path.ends_with(".lang") {
        return Err(malformed("invalid source path length or extension"));
    }
    for (i, part) in path.split('/').enumerate() {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with('.')
            || !part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err(malformed("nonportable source path component"));
        }
        let lower = part.to_ascii_lowercase();
        if i == 0 && [DESCRIPTOR, ".git", ".codex", "target"].contains(&lower.as_str()) {
            return Err(malformed("reserved archive path"));
        }
        let stem = lower.split('.').next().unwrap_or_default();
        if ["con", "prn", "aux", "nul"].contains(&stem)
            || (stem.len() == 4
                && (stem.starts_with("com") || stem.starts_with("lpt"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(malformed("reserved device path component"));
        }
    }
    Ok(())
}

fn json_depth(bytes: &[u8]) -> Result<(), Error> {
    let (mut depth, mut string, mut escaped) = (0_usize, false, false);
    for byte in bytes {
        if string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > MAX_JSON_DEPTH {
                        return Err(limit("descriptor nesting limit exceeded"));
                    }
                }
                b'}' | b']' => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| malformed("invalid JSON nesting"))?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

struct BoundedWriter(Vec<u8>);
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_DESCRIPTOR_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("descriptor limit"));
        }
        self.0.try_reserve(bytes.len()).map_err(io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn entry(
    bytes: &[u8],
    offset: usize,
    name: &str,
    max: usize,
) -> Result<(Range<usize>, usize), Error> {
    let body = offset
        .checked_add(BLOCK)
        .ok_or_else(|| limit("archive offset overflow"))?;
    let header = bytes
        .get(offset..body)
        .ok_or_else(|| malformed("truncated archive header").at(offset))?;
    let mut size = 0_usize;
    for digit in &header[124..135] {
        if !matches!(digit, b'0'..=b'7') {
            return Err(malformed("noncanonical TAR size").at(offset + 124));
        }
        size = size
            .checked_mul(8)
            .and_then(|n| n.checked_add(usize::from(*digit - b'0')))
            .ok_or_else(|| limit("TAR size overflow"))?;
    }
    if header[135] != 0 {
        return Err(malformed("noncanonical TAR size terminator").at(offset + 135));
    }
    if size > max {
        return Err(limit("archive entry byte limit exceeded").at(offset));
    }
    if header != canonical_header(name, size) {
        return Err(malformed("unsupported or noncanonical TAR header").at(offset));
    }
    let end = body
        .checked_add(size)
        .ok_or_else(|| limit("archive entry offset overflow"))?;
    let next = end
        .checked_add((BLOCK - size % BLOCK) % BLOCK)
        .ok_or_else(|| limit("archive padding offset overflow"))?;
    let padding = bytes
        .get(end..next)
        .ok_or_else(|| malformed("truncated archive payload or padding").at(offset))?;
    if padding.iter().any(|b| *b != 0) {
        return Err(malformed("nonzero archive padding").at(end));
    }
    Ok((body..end, next))
}

fn canonical_header(name: &str, size: usize) -> [u8; BLOCK] {
    // Names are the fixed descriptor name or validated <=100-byte ASCII paths;
    // sizes have already passed the <=1 MiB per-entry bound.
    let mut header = [0; BLOCK];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    header[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[329..337].copy_from_slice(b"0000000\0");
    header[337..345].copy_from_slice(b"0000000\0");
    let checksum: usize = header.iter().map(|b| usize::from(*b)).sum();
    header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
    header
}
