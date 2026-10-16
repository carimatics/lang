use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str;

use super::{
    COMPONENT_METADATA_SCHEMA_VERSION, ComponentExport, ComponentFunctionType, ComponentImport,
    ComponentMetadata, ComponentScalarType, ComponentVersion,
};

/// Magic bytes for the experimental schema-1 component metadata encoding.
pub const COMPONENT_METADATA_MAGIC: [u8; 4] = *b"LCMP";

/// Resource bounds applied before or during component metadata encoding and decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentCodecLimits {
    max_bytes: usize,
    max_items_per_section: usize,
    max_nesting_depth: usize,
    max_string_bytes: usize,
}

impl ComponentCodecLimits {
    pub const fn new(
        max_bytes: usize,
        max_items_per_section: usize,
        max_nesting_depth: usize,
        max_string_bytes: usize,
    ) -> Self {
        Self {
            max_bytes,
            max_items_per_section,
            max_nesting_depth,
            max_string_bytes,
        }
    }

    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    pub const fn max_items_per_section(self) -> usize {
        self.max_items_per_section
    }

    pub const fn max_nesting_depth(self) -> usize {
        self.max_nesting_depth
    }

    pub const fn max_string_bytes(self) -> usize {
        self.max_string_bytes
    }
}

impl Default for ComponentCodecLimits {
    fn default() -> Self {
        Self::new(1024 * 1024, 1024, 5, 4096)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentMetadataSection {
    Imports,
    Exports,
}

impl Display for ComponentMetadataSection {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Imports => formatter.write_str("imports"),
            Self::Exports => formatter.write_str("exports"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentMetadataStringField {
    PackageNamespace,
    PackageName,
    ImportName,
    ImportCapability,
    ExportName,
}

impl Display for ComponentMetadataStringField {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PackageNamespace => formatter.write_str("package namespace"),
            Self::PackageName => formatter.write_str("package name"),
            Self::ImportName => formatter.write_str("import name"),
            Self::ImportCapability => formatter.write_str("import capability"),
            Self::ExportName => formatter.write_str("export name"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentCodecErrorKind {
    ByteLimitExceeded {
        limit: usize,
        actual: usize,
    },
    ItemCountExceeded {
        section: ComponentMetadataSection,
        limit: usize,
        actual: usize,
    },
    NestingDepthExceeded {
        limit: usize,
        required: usize,
    },
    StringLengthExceeded {
        field: ComponentMetadataStringField,
        limit: usize,
        actual: usize,
    },
    StringLengthDoesNotFitU32 {
        field: ComponentMetadataStringField,
        actual: usize,
    },
    UnexpectedEnd {
        needed: usize,
        remaining: usize,
    },
    InvalidMagic {
        actual: [u8; 4],
    },
    UnsupportedSchemaVersion {
        expected: u32,
        actual: u32,
    },
    InvalidUtf8 {
        field: ComponentMetadataStringField,
    },
    UnknownScalarType {
        tag: u8,
    },
    TrailingBytes {
        remaining: usize,
    },
}

impl Display for ComponentCodecErrorKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ByteLimitExceeded { limit, actual } => write!(
                formatter,
                "component metadata uses {actual} bytes, exceeding the {limit}-byte limit"
            ),
            Self::ItemCountExceeded {
                section,
                limit,
                actual,
            } => write!(
                formatter,
                "component metadata declares {actual} {section}, exceeding the limit of {limit}"
            ),
            Self::NestingDepthExceeded { limit, required } => write!(
                formatter,
                "component metadata requires nesting depth {required}, exceeding the limit of {limit}"
            ),
            Self::StringLengthExceeded {
                field,
                limit,
                actual,
            } => write!(
                formatter,
                "component metadata {field} uses {actual} bytes, exceeding the {limit}-byte string limit"
            ),
            Self::StringLengthDoesNotFitU32 { field, actual } => write!(
                formatter,
                "component metadata {field} uses {actual} bytes, which cannot be represented by schema 1"
            ),
            Self::UnexpectedEnd { needed, remaining } => write!(
                formatter,
                "component metadata ended with {remaining} bytes available while {needed} were required"
            ),
            Self::InvalidMagic { actual } => write!(
                formatter,
                "component metadata magic {actual:?} does not match {COMPONENT_METADATA_MAGIC:?}"
            ),
            Self::UnsupportedSchemaVersion { expected, actual } => write!(
                formatter,
                "component metadata schema {actual} is unsupported; expected {expected}"
            ),
            Self::InvalidUtf8 { field } => {
                write!(formatter, "component metadata {field} is not valid UTF-8")
            }
            Self::UnknownScalarType { tag } => {
                write!(
                    formatter,
                    "component metadata scalar type tag {tag} is unknown"
                )
            }
            Self::TrailingBytes { remaining } => write!(
                formatter,
                "component metadata has {remaining} trailing bytes after the schema-1 payload"
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentCodecError {
    offset: usize,
    kind: ComponentCodecErrorKind,
}

impl ComponentCodecError {
    const fn new(offset: usize, kind: ComponentCodecErrorKind) -> Self {
        Self { offset, kind }
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }

    pub const fn kind(&self) -> &ComponentCodecErrorKind {
        &self.kind
    }
}

impl Display for ComponentCodecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} at byte offset {}", self.kind, self.offset)
    }
}

impl Error for ComponentCodecError {}

/// Encodes schema-1 scalar component metadata in a deterministic prototype format.
///
/// The format is `LCMP`, a little-endian schema version, two length-prefixed UTF-8
/// package strings, a three-part package version, then length-prefixed import and
/// export sections. Scalar tags are `0` for `i64` and `1` for `bool`.
pub fn encode_component_metadata(
    metadata: &ComponentMetadata,
    limits: ComponentCodecLimits,
) -> Result<Vec<u8>, ComponentCodecError> {
    let mut writer = ComponentWriter::new(limits);
    writer.require_depth(1)?;
    writer.write_bytes(&COMPONENT_METADATA_MAGIC)?;
    let schema_offset = writer.offset();
    if metadata.schema_version() != COMPONENT_METADATA_SCHEMA_VERSION {
        return Err(ComponentCodecError::new(
            schema_offset,
            ComponentCodecErrorKind::UnsupportedSchemaVersion {
                expected: COMPONENT_METADATA_SCHEMA_VERSION,
                actual: metadata.schema_version(),
            },
        ));
    }
    writer.write_u32(metadata.schema_version())?;
    writer.write_string(
        ComponentMetadataStringField::PackageNamespace,
        metadata.package_namespace(),
    )?;
    writer.write_string(
        ComponentMetadataStringField::PackageName,
        metadata.package_name(),
    )?;
    writer.write_u32(metadata.version().major())?;
    writer.write_u32(metadata.version().minor())?;
    writer.write_u32(metadata.version().patch())?;

    writer.require_depth(2)?;
    writer.write_count(ComponentMetadataSection::Imports, metadata.imports().len())?;
    for component_import in metadata.imports() {
        writer.require_depth(3)?;
        writer.write_string(
            ComponentMetadataStringField::ImportName,
            component_import.name(),
        )?;
        writer.write_scalar_type(component_import.function_type().parameter())?;
        writer.write_scalar_type(component_import.function_type().result())?;
        writer.write_string(
            ComponentMetadataStringField::ImportCapability,
            component_import.capability(),
        )?;
    }

    writer.require_depth(2)?;
    writer.write_count(ComponentMetadataSection::Exports, metadata.exports().len())?;
    for component_export in metadata.exports() {
        writer.require_depth(3)?;
        writer.write_string(
            ComponentMetadataStringField::ExportName,
            component_export.name(),
        )?;
        writer.write_scalar_type(component_export.function_type().parameter())?;
        writer.write_scalar_type(component_export.function_type().result())?;
    }

    Ok(writer.finish())
}

/// Decodes untrusted schema-1 scalar component metadata under explicit bounds.
pub fn decode_component_metadata(
    bytes: &[u8],
    limits: ComponentCodecLimits,
) -> Result<ComponentMetadata, ComponentCodecError> {
    if bytes.len() > limits.max_bytes() {
        return Err(ComponentCodecError::new(
            0,
            ComponentCodecErrorKind::ByteLimitExceeded {
                limit: limits.max_bytes(),
                actual: bytes.len(),
            },
        ));
    }

    let mut reader = ComponentReader::new(bytes, limits);
    reader.require_depth(1)?;
    let magic_offset = reader.offset();
    let mut magic = [0_u8; 4];
    magic.copy_from_slice(reader.read_exact(4)?);
    if magic != COMPONENT_METADATA_MAGIC {
        return Err(ComponentCodecError::new(
            magic_offset,
            ComponentCodecErrorKind::InvalidMagic { actual: magic },
        ));
    }

    let schema_offset = reader.offset();
    let schema_version = reader.read_u32()?;
    if schema_version != COMPONENT_METADATA_SCHEMA_VERSION {
        return Err(ComponentCodecError::new(
            schema_offset,
            ComponentCodecErrorKind::UnsupportedSchemaVersion {
                expected: COMPONENT_METADATA_SCHEMA_VERSION,
                actual: schema_version,
            },
        ));
    }

    let package_namespace = reader.read_string(ComponentMetadataStringField::PackageNamespace)?;
    let package_name = reader.read_string(ComponentMetadataStringField::PackageName)?;
    let version = ComponentVersion::new(reader.read_u32()?, reader.read_u32()?, reader.read_u32()?);

    reader.require_depth(2)?;
    let import_count = reader.read_count(ComponentMetadataSection::Imports)?;
    let mut imports = Vec::with_capacity(import_count);
    for _ in 0..import_count {
        reader.require_depth(3)?;
        let name = reader.read_string(ComponentMetadataStringField::ImportName)?;
        let parameter = reader.read_scalar_type()?;
        let result = reader.read_scalar_type()?;
        let capability = reader.read_string(ComponentMetadataStringField::ImportCapability)?;
        imports.push(ComponentImport::new(
            name,
            ComponentFunctionType::new(parameter, result),
            capability,
        ));
    }

    reader.require_depth(2)?;
    let export_count = reader.read_count(ComponentMetadataSection::Exports)?;
    let mut exports = Vec::with_capacity(export_count);
    for _ in 0..export_count {
        reader.require_depth(3)?;
        let name = reader.read_string(ComponentMetadataStringField::ExportName)?;
        let parameter = reader.read_scalar_type()?;
        let result = reader.read_scalar_type()?;
        exports.push(ComponentExport::new(
            name,
            ComponentFunctionType::new(parameter, result),
        ));
    }

    reader.finish()?;
    Ok(ComponentMetadata::new(
        schema_version,
        package_namespace,
        package_name,
        version,
        imports,
        exports,
    ))
}

struct ComponentWriter {
    bytes: Vec<u8>,
    limits: ComponentCodecLimits,
}

impl ComponentWriter {
    fn new(limits: ComponentCodecLimits) -> Self {
        Self {
            bytes: Vec::new(),
            limits,
        }
    }

    fn offset(&self) -> usize {
        self.bytes.len()
    }

    fn require_depth(&self, required: usize) -> Result<(), ComponentCodecError> {
        if required > self.limits.max_nesting_depth() {
            return Err(ComponentCodecError::new(
                self.offset(),
                ComponentCodecErrorKind::NestingDepthExceeded {
                    limit: self.limits.max_nesting_depth(),
                    required,
                },
            ));
        }
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), ComponentCodecError> {
        let actual = self.bytes.len().saturating_add(bytes.len());
        if actual > self.limits.max_bytes() {
            return Err(ComponentCodecError::new(
                self.offset(),
                ComponentCodecErrorKind::ByteLimitExceeded {
                    limit: self.limits.max_bytes(),
                    actual,
                },
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn write_u32(&mut self, value: u32) -> Result<(), ComponentCodecError> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn write_count(
        &mut self,
        section: ComponentMetadataSection,
        count: usize,
    ) -> Result<(), ComponentCodecError> {
        let offset = self.offset();
        if count > self.limits.max_items_per_section() {
            return Err(ComponentCodecError::new(
                offset,
                ComponentCodecErrorKind::ItemCountExceeded {
                    section,
                    limit: self.limits.max_items_per_section(),
                    actual: count,
                },
            ));
        }
        let count = u32::try_from(count).map_err(|_| {
            ComponentCodecError::new(
                offset,
                ComponentCodecErrorKind::ItemCountExceeded {
                    section,
                    limit: u32::MAX as usize,
                    actual: count,
                },
            )
        })?;
        self.write_u32(count)
    }

    fn write_string(
        &mut self,
        field: ComponentMetadataStringField,
        value: &str,
    ) -> Result<(), ComponentCodecError> {
        let offset = self.offset();
        if value.len() > self.limits.max_string_bytes() {
            return Err(ComponentCodecError::new(
                offset,
                ComponentCodecErrorKind::StringLengthExceeded {
                    field,
                    limit: self.limits.max_string_bytes(),
                    actual: value.len(),
                },
            ));
        }
        let length = u32::try_from(value.len()).map_err(|_| {
            ComponentCodecError::new(
                offset,
                ComponentCodecErrorKind::StringLengthDoesNotFitU32 {
                    field,
                    actual: value.len(),
                },
            )
        })?;
        self.write_u32(length)?;
        self.write_bytes(value.as_bytes())
    }

    fn write_scalar_type(
        &mut self,
        scalar_type: ComponentScalarType,
    ) -> Result<(), ComponentCodecError> {
        let tag = match scalar_type {
            ComponentScalarType::I64 => 0,
            ComponentScalarType::Bool => 1,
        };
        self.write_bytes(&[tag])
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

struct ComponentReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    limits: ComponentCodecLimits,
}

impl<'a> ComponentReader<'a> {
    const fn new(bytes: &'a [u8], limits: ComponentCodecLimits) -> Self {
        Self {
            bytes,
            offset: 0,
            limits,
        }
    }

    const fn offset(&self) -> usize {
        self.offset
    }

    fn require_depth(&self, required: usize) -> Result<(), ComponentCodecError> {
        if required > self.limits.max_nesting_depth() {
            return Err(ComponentCodecError::new(
                self.offset(),
                ComponentCodecErrorKind::NestingDepthExceeded {
                    limit: self.limits.max_nesting_depth(),
                    required,
                },
            ));
        }
        Ok(())
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8], ComponentCodecError> {
        let remaining = self.bytes.len().saturating_sub(self.offset);
        if length > remaining {
            return Err(ComponentCodecError::new(
                self.offset,
                ComponentCodecErrorKind::UnexpectedEnd {
                    needed: length,
                    remaining,
                },
            ));
        }
        let start = self.offset;
        self.offset += length;
        Ok(&self.bytes[start..self.offset])
    }

    fn read_u32(&mut self) -> Result<u32, ComponentCodecError> {
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(self.read_exact(4)?);
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_count(
        &mut self,
        section: ComponentMetadataSection,
    ) -> Result<usize, ComponentCodecError> {
        let offset = self.offset();
        let count = self.read_u32()? as usize;
        if count > self.limits.max_items_per_section() {
            return Err(ComponentCodecError::new(
                offset,
                ComponentCodecErrorKind::ItemCountExceeded {
                    section,
                    limit: self.limits.max_items_per_section(),
                    actual: count,
                },
            ));
        }
        // Even permissive caller limits must not let a tiny payload reserve an
        // attacker-controlled capacity before its entries have been decoded.
        let minimum_entry_bytes = match section {
            ComponentMetadataSection::Imports => 10, // two strings and two type tags
            ComponentMetadataSection::Exports => 6,  // one string and two type tags
        };
        let needed = count.saturating_mul(minimum_entry_bytes);
        let remaining = self.bytes.len() - self.offset;
        if needed > remaining {
            return Err(ComponentCodecError::new(
                offset,
                ComponentCodecErrorKind::UnexpectedEnd { needed, remaining },
            ));
        }
        Ok(count)
    }

    fn read_string(
        &mut self,
        field: ComponentMetadataStringField,
    ) -> Result<String, ComponentCodecError> {
        let length_offset = self.offset();
        let length = self.read_u32()? as usize;
        if length > self.limits.max_string_bytes() {
            return Err(ComponentCodecError::new(
                length_offset,
                ComponentCodecErrorKind::StringLengthExceeded {
                    field,
                    limit: self.limits.max_string_bytes(),
                    actual: length,
                },
            ));
        }
        let string_offset = self.offset();
        let bytes = self.read_exact(length)?;
        str::from_utf8(bytes).map(str::to_owned).map_err(|error| {
            ComponentCodecError::new(
                string_offset + error.valid_up_to(),
                ComponentCodecErrorKind::InvalidUtf8 { field },
            )
        })
    }

    fn read_scalar_type(&mut self) -> Result<ComponentScalarType, ComponentCodecError> {
        let offset = self.offset();
        let tag = self.read_exact(1)?[0];
        match tag {
            0 => Ok(ComponentScalarType::I64),
            1 => Ok(ComponentScalarType::Bool),
            _ => Err(ComponentCodecError::new(
                offset,
                ComponentCodecErrorKind::UnknownScalarType { tag },
            )),
        }
    }

    fn finish(self) -> Result<(), ComponentCodecError> {
        let remaining = self.bytes.len() - self.offset;
        if remaining != 0 {
            return Err(ComponentCodecError::new(
                self.offset,
                ComponentCodecErrorKind::TrailingBytes { remaining },
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMPONENT_METADATA_MAGIC, ComponentCodecError, ComponentCodecErrorKind,
        ComponentCodecLimits, ComponentMetadataSection, ComponentMetadataStringField,
        decode_component_metadata, encode_component_metadata,
    };
    use crate::component::{
        COMPONENT_METADATA_SCHEMA_VERSION, ComponentExport, ComponentFunctionType, ComponentImport,
        ComponentMetadata, ComponentScalarType, ComponentVersion,
    };

    fn metadata() -> ComponentMetadata {
        ComponentMetadata::new(
            COMPONENT_METADATA_SCHEMA_VERSION,
            "example",
            "counter",
            ComponentVersion::new(1, 2, 3),
            vec![ComponentImport::new(
                "read-clock",
                ComponentFunctionType::new(ComponentScalarType::I64, ComponentScalarType::Bool),
                "clock.monotonic",
            )],
            vec![ComponentExport::new(
                "run",
                ComponentFunctionType::new(ComponentScalarType::Bool, ComponentScalarType::I64),
            )],
        )
    }

    fn limits(
        max_bytes: usize,
        max_items: usize,
        max_depth: usize,
        max_string_bytes: usize,
    ) -> ComponentCodecLimits {
        ComponentCodecLimits::new(max_bytes, max_items, max_depth, max_string_bytes)
    }

    #[test]
    fn rejects_impossible_counts_before_reserving_entries() {
        let mut encoded =
            encode_component_metadata(&metadata(), ComponentCodecLimits::default()).unwrap();
        encoded[42..46].copy_from_slice(&u32::MAX.to_le_bytes());
        let error = decode_component_metadata(&encoded, limits(encoded.len(), usize::MAX, 3, 64))
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            ComponentCodecErrorKind::UnexpectedEnd { .. }
        ));
    }

    #[test]
    fn round_trips_schema_1_deterministically() {
        let metadata = metadata();
        let encoded =
            encode_component_metadata(&metadata, ComponentCodecLimits::default()).unwrap();

        assert_eq!(&encoded[..4], &COMPONENT_METADATA_MAGIC);
        assert_eq!(
            encode_component_metadata(&metadata, ComponentCodecLimits::default()).unwrap(),
            encoded
        );
        assert_eq!(
            decode_component_metadata(&encoded, ComponentCodecLimits::default()).unwrap(),
            metadata
        );
    }

    #[test]
    fn enforces_byte_count_depth_and_string_limits() {
        let metadata = metadata();
        let encoded =
            encode_component_metadata(&metadata, ComponentCodecLimits::default()).unwrap();

        assert_eq!(
            encode_component_metadata(&metadata, limits(4, 8, 3, 64)),
            Err(ComponentCodecError::new(
                4,
                ComponentCodecErrorKind::ByteLimitExceeded {
                    limit: 4,
                    actual: 8,
                }
            ))
        );
        assert_eq!(
            encode_component_metadata(&metadata, limits(encoded.len(), 0, 3, 64)),
            Err(ComponentCodecError::new(
                42,
                ComponentCodecErrorKind::ItemCountExceeded {
                    section: ComponentMetadataSection::Imports,
                    limit: 0,
                    actual: 1,
                }
            ))
        );
        assert_eq!(
            encode_component_metadata(&metadata, limits(encoded.len(), 8, 2, 64)),
            Err(ComponentCodecError::new(
                46,
                ComponentCodecErrorKind::NestingDepthExceeded {
                    limit: 2,
                    required: 3,
                }
            ))
        );
        assert_eq!(
            encode_component_metadata(&metadata, limits(encoded.len(), 8, 3, 3)),
            Err(ComponentCodecError::new(
                8,
                ComponentCodecErrorKind::StringLengthExceeded {
                    field: ComponentMetadataStringField::PackageNamespace,
                    limit: 3,
                    actual: 7,
                }
            ))
        );

        assert_eq!(
            decode_component_metadata(&encoded, limits(encoded.len() - 1, 8, 3, 64)),
            Err(ComponentCodecError::new(
                0,
                ComponentCodecErrorKind::ByteLimitExceeded {
                    limit: encoded.len() - 1,
                    actual: encoded.len(),
                }
            ))
        );
        assert_eq!(
            decode_component_metadata(&encoded, limits(encoded.len(), 0, 3, 64)),
            Err(ComponentCodecError::new(
                42,
                ComponentCodecErrorKind::ItemCountExceeded {
                    section: ComponentMetadataSection::Imports,
                    limit: 0,
                    actual: 1,
                }
            ))
        );
        assert_eq!(
            decode_component_metadata(&encoded, limits(encoded.len(), 8, 2, 64)),
            Err(ComponentCodecError::new(
                46,
                ComponentCodecErrorKind::NestingDepthExceeded {
                    limit: 2,
                    required: 3,
                }
            ))
        );
        assert_eq!(
            decode_component_metadata(&encoded, limits(encoded.len(), 8, 3, 3)),
            Err(ComponentCodecError::new(
                8,
                ComponentCodecErrorKind::StringLengthExceeded {
                    field: ComponentMetadataStringField::PackageNamespace,
                    limit: 3,
                    actual: 7,
                }
            ))
        );
    }

    #[test]
    fn reports_malformed_fixture_offsets() {
        let metadata = metadata();
        let encoded =
            encode_component_metadata(&metadata, ComponentCodecLimits::default()).unwrap();

        assert_eq!(
            decode_component_metadata(&[], ComponentCodecLimits::default()),
            Err(ComponentCodecError::new(
                0,
                ComponentCodecErrorKind::UnexpectedEnd {
                    needed: 4,
                    remaining: 0,
                }
            ))
        );

        let mut bad_magic = encoded.clone();
        bad_magic[0] = b'X';
        assert_eq!(
            decode_component_metadata(&bad_magic, ComponentCodecLimits::default()),
            Err(ComponentCodecError::new(
                0,
                ComponentCodecErrorKind::InvalidMagic { actual: *b"XCMP" }
            ))
        );

        let mut unsupported_schema = COMPONENT_METADATA_MAGIC.to_vec();
        unsupported_schema
            .extend_from_slice(&(COMPONENT_METADATA_SCHEMA_VERSION + 1).to_le_bytes());
        assert_eq!(
            decode_component_metadata(&unsupported_schema, ComponentCodecLimits::default()),
            Err(ComponentCodecError::new(
                4,
                ComponentCodecErrorKind::UnsupportedSchemaVersion {
                    expected: COMPONENT_METADATA_SCHEMA_VERSION,
                    actual: COMPONENT_METADATA_SCHEMA_VERSION + 1,
                }
            ))
        );

        let truncated_string = &encoded[..13];
        assert_eq!(
            decode_component_metadata(truncated_string, ComponentCodecLimits::default()),
            Err(ComponentCodecError::new(
                12,
                ComponentCodecErrorKind::UnexpectedEnd {
                    needed: 7,
                    remaining: 1,
                }
            ))
        );

        let mut invalid_utf8 = encoded.clone();
        invalid_utf8[12] = 0xff;
        assert_eq!(
            decode_component_metadata(&invalid_utf8, ComponentCodecLimits::default()),
            Err(ComponentCodecError::new(
                12,
                ComponentCodecErrorKind::InvalidUtf8 {
                    field: ComponentMetadataStringField::PackageNamespace,
                }
            ))
        );

        let mut unknown_scalar = encoded.clone();
        unknown_scalar[60] = 9;
        assert_eq!(
            decode_component_metadata(&unknown_scalar, ComponentCodecLimits::default()),
            Err(ComponentCodecError::new(
                60,
                ComponentCodecErrorKind::UnknownScalarType { tag: 9 }
            ))
        );

        let mut trailing = encoded;
        let offset = trailing.len();
        trailing.push(0);
        assert_eq!(
            decode_component_metadata(&trailing, ComponentCodecLimits::default()),
            Err(ComponentCodecError::new(
                offset,
                ComponentCodecErrorKind::TrailingBytes { remaining: 1 }
            ))
        );
    }

    #[test]
    fn arbitrary_byte_corpus_never_panics_or_exceeds_input_bounds() {
        let limits = limits(256, 16, 3, 32);
        let mut state = 0x9e37_79b9_u32;
        for length in 0..=256 {
            let mut bytes = vec![0_u8; length];
            for byte in &mut bytes {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *byte = state.to_le_bytes()[3];
            }
            let _ = decode_component_metadata(&bytes, limits);
        }
    }
}
