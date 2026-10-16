use std::collections::HashMap;

use gimli::write::{
    Address, AttributeValue, DwarfUnit, EndianVec, LineProgram, LineString, RelocateWriter,
    Relocation as DwarfRelocation, RelocationTarget, Sections,
};
use gimli::{Encoding, Format, LineEncoding, LittleEndian, SectionId as DwarfSectionId, constants};
use object::write::{Object, Relocation, SymbolId};
use object::{BinaryFormat, RelocationEncoding, RelocationFlags, RelocationKind, SectionKind};

use super::CompiledSourceMap;

const CODEVIEW_SIGNATURE_C13: u32 = 4;
const DEBUG_S_LINES: u32 = 0xf2;
const DEBUG_S_STRING_TABLE: u32 = 0xf3;
const DEBUG_S_FILE_CHECKSUMS: u32 = 0xf4;
const CODEVIEW_LINES_HAVE_COLUMNS: u16 = 1;
const CODEVIEW_LINE_IS_STATEMENT: u32 = 1 << 31;
const CODEVIEW_MAX_LINE: u32 = (1 << 24) - 1;

const DWARF_SECTIONS: [DwarfSectionId; 11] = [
    DwarfSectionId::DebugAbbrev,
    DwarfSectionId::DebugStr,
    DwarfSectionId::DebugLineStr,
    DwarfSectionId::DebugLine,
    DwarfSectionId::DebugRanges,
    DwarfSectionId::DebugRngLists,
    DwarfSectionId::DebugLoc,
    DwarfSectionId::DebugLocLists,
    DwarfSectionId::DebugInfo,
    DwarfSectionId::DebugFrame,
    DwarfSectionId::EhFrame,
];

#[derive(Clone, Debug)]
struct DwarfSection {
    writer: EndianVec<LittleEndian>,
    relocations: Vec<DwarfRelocation>,
}

impl DwarfSection {
    fn new() -> Self {
        Self {
            writer: EndianVec::new(LittleEndian),
            relocations: Vec::new(),
        }
    }
}

impl RelocateWriter for DwarfSection {
    type Writer = EndianVec<LittleEndian>;

    fn writer(&self) -> &Self::Writer {
        &self.writer
    }

    fn writer_mut(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }

    fn relocate(&mut self, relocation: DwarfRelocation) {
        self.relocations.push(relocation);
    }
}

pub(super) fn add_native_debug(
    object: &mut Object<'_>,
    function_symbol: SymbolId,
    function_name: &str,
    code_size: usize,
    source_map: &CompiledSourceMap,
) -> Result<(), String> {
    match object.format() {
        BinaryFormat::MachO => add_macho_dwarf(
            object,
            function_symbol,
            function_name,
            code_size,
            source_map,
        ),
        BinaryFormat::Coff => add_coff_codeview(object, function_symbol, code_size, source_map),
        _ => Ok(()),
    }
}

fn add_macho_dwarf(
    object: &mut Object<'_>,
    function_symbol: SymbolId,
    function_name: &str,
    code_size: usize,
    source_map: &CompiledSourceMap,
) -> Result<(), String> {
    if source_map.ranges().is_empty() {
        return Ok(());
    }

    let mut sections = build_dwarf(function_name, code_size, source_map)?;
    let mut object_sections = HashMap::new();

    for dwarf_id in DWARF_SECTIONS {
        let section = sections
            .get(dwarf_id)
            .expect("the fixed DWARF section list contains writable sections");
        if section.writer.slice().is_empty() {
            continue;
        }
        let object_id = object.add_section(
            b"__DWARF".to_vec(),
            macho_section_name(dwarf_id),
            SectionKind::Debug,
        );
        object.append_section_data(object_id, section.writer.slice(), 1);
        object_sections.insert(dwarf_id, object_id);
    }

    for dwarf_id in DWARF_SECTIONS {
        let Some(&object_id) = object_sections.get(&dwarf_id) else {
            continue;
        };
        let section = sections
            .get_mut(dwarf_id)
            .expect("the fixed DWARF section list contains writable sections");
        for relocation in section.relocations.drain(..) {
            let symbol = match relocation.target {
                RelocationTarget::Symbol(0) => function_symbol,
                RelocationTarget::Symbol(symbol) => {
                    return Err(format!("unknown DWARF symbol index {symbol}"));
                }
                RelocationTarget::Section(target) => {
                    let target = *object_sections
                        .get(&target)
                        .ok_or_else(|| format!("missing DWARF target section {}", target.name()))?;
                    object.section_symbol(target)
                }
            };
            object
                .add_relocation(
                    object_id,
                    Relocation {
                        offset: relocation.offset as u64,
                        symbol,
                        addend: relocation.addend,
                        flags: RelocationFlags::Generic {
                            kind: RelocationKind::Absolute,
                            encoding: RelocationEncoding::Generic,
                            size: relocation.size * 8,
                        },
                    },
                )
                .map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

fn add_coff_codeview(
    object: &mut Object<'_>,
    function_symbol: SymbolId,
    code_size: usize,
    source_map: &CompiledSourceMap,
) -> Result<(), String> {
    if source_map.ranges().is_empty() {
        return Ok(());
    }

    let (section_data, relocation_offset, relocation_segment) =
        build_codeview(code_size, source_map)?;
    let section = object.add_section(Vec::new(), b".debug$S".to_vec(), SectionKind::Debug);
    object.append_section_data(section, &section_data, 4);
    object
        .add_relocation(
            section,
            Relocation {
                offset: relocation_offset as u64,
                symbol: function_symbol,
                addend: 0,
                flags: RelocationFlags::Generic {
                    kind: RelocationKind::SectionOffset,
                    encoding: RelocationEncoding::Generic,
                    size: 32,
                },
            },
        )
        .map_err(|error| error.to_string())?;
    object
        .add_relocation(
            section,
            Relocation {
                offset: relocation_segment as u64,
                symbol: function_symbol,
                addend: 0,
                flags: RelocationFlags::Generic {
                    kind: RelocationKind::SectionIndex,
                    encoding: RelocationEncoding::Generic,
                    size: 16,
                },
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn build_codeview(
    code_size: usize,
    source_map: &CompiledSourceMap,
) -> Result<(Vec<u8>, usize, usize), String> {
    let code_size = u32::try_from(code_size)
        .map_err(|_| "CodeView function code size exceeds 32 bits".to_owned())?;
    let mut rows = Vec::new();
    let mut previous_offset = None;
    for range in source_map.ranges() {
        if range.start() == range.end() || previous_offset == Some(range.start()) {
            continue;
        }
        let position = range.span().start();
        if position.line() > CODEVIEW_MAX_LINE {
            return Err(format!(
                "CodeView source line {} exceeds 24 bits",
                position.line()
            ));
        }
        let column = u16::try_from(position.column()).map_err(|_| {
            format!(
                "CodeView source column {} exceeds 16 bits",
                position.column()
            )
        })?;
        let end_column = if range.span().end().line() == position.line() {
            u16::try_from(range.span().end().column()).map_err(|_| {
                format!(
                    "CodeView source column {} exceeds 16 bits",
                    range.span().end().column()
                )
            })?
        } else {
            column
        };
        rows.push((range.start(), position.line(), column, end_column));
        previous_offset = Some(range.start());
    }
    if rows.is_empty() {
        return Err("CodeView source map contains no emitted code ranges".to_owned());
    }

    let row_count = u32::try_from(rows.len())
        .map_err(|_| "CodeView line table contains too many rows".to_owned())?;
    let block_size = 12_u32
        .checked_add(
            row_count
                .checked_mul(12)
                .ok_or_else(|| "CodeView line block size overflowed".to_owned())?,
        )
        .ok_or_else(|| "CodeView line block size overflowed".to_owned())?;

    let mut lines = Vec::new();
    push_u32(&mut lines, 0);
    push_u16(&mut lines, 0);
    push_u16(&mut lines, CODEVIEW_LINES_HAVE_COLUMNS);
    push_u32(&mut lines, code_size);
    push_u32(&mut lines, 0);
    push_u32(&mut lines, row_count);
    push_u32(&mut lines, block_size);
    for &(offset, line, _, _) in &rows {
        push_u32(&mut lines, offset);
        push_u32(&mut lines, line | CODEVIEW_LINE_IS_STATEMENT);
    }
    for &(_, _, start, end) in &rows {
        push_u16(&mut lines, start);
        push_u16(&mut lines, end);
    }

    let mut strings = vec![0];
    strings.extend_from_slice(source_map.source_path().as_bytes());
    strings.push(0);

    let mut checksums = Vec::new();
    push_u32(&mut checksums, 1);
    checksums.push(0);
    checksums.push(0);
    pad_to_four(&mut checksums);

    let mut section = Vec::new();
    push_u32(&mut section, CODEVIEW_SIGNATURE_C13);
    let lines_start = append_codeview_subsection(&mut section, DEBUG_S_LINES, &lines)?;
    append_codeview_subsection(&mut section, DEBUG_S_STRING_TABLE, &strings)?;
    append_codeview_subsection(&mut section, DEBUG_S_FILE_CHECKSUMS, &checksums)?;
    Ok((section, lines_start, lines_start + 4))
}

fn append_codeview_subsection(
    section: &mut Vec<u8>,
    kind: u32,
    payload: &[u8],
) -> Result<usize, String> {
    push_u32(section, kind);
    push_u32(
        section,
        u32::try_from(payload.len())
            .map_err(|_| "CodeView subsection exceeds 32 bits".to_owned())?,
    );
    let payload_start = section.len();
    section.extend_from_slice(payload);
    pad_to_four(section);
    Ok(payload_start)
}

fn push_u16(data: &mut Vec<u8>, value: u16) {
    data.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(data: &mut Vec<u8>, value: u32) {
    data.extend_from_slice(&value.to_le_bytes());
}

fn pad_to_four(data: &mut Vec<u8>) {
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
}

fn build_dwarf(
    function_name: &str,
    code_size: usize,
    source_map: &CompiledSourceMap,
) -> Result<Sections<DwarfSection>, String> {
    let encoding = Encoding {
        format: Format::Dwarf32,
        version: 4,
        address_size: 8,
    };
    let source_path = source_map.source_path().as_bytes().to_vec();
    let mut line_program = LineProgram::new(
        encoding,
        LineEncoding::default(),
        LineString::String(b".".to_vec()),
        None,
        LineString::String(source_path.clone()),
        None,
    );
    let source_file = line_program.add_file(
        LineString::String(source_path.clone()),
        line_program.default_directory(),
        None,
    );
    line_program.begin_sequence(Some(Address::Symbol {
        symbol: 0,
        addend: 0,
    }));
    let mut previous_offset = None;
    for range in source_map.ranges() {
        if range.start() == range.end() || previous_offset == Some(range.start()) {
            continue;
        }
        let position = range.span().start();
        let row = line_program.row();
        row.address_offset = u64::from(range.start());
        row.file = source_file;
        row.line = u64::from(position.line());
        row.column = u64::from(position.column());
        row.is_statement = true;
        line_program.generate_row();
        previous_offset = Some(range.start());
    }
    line_program.end_sequence(code_size as u64);

    let mut dwarf = DwarfUnit::new(encoding);
    dwarf.unit.line_program = line_program;
    let root = dwarf.unit.root();
    {
        let entry = dwarf.unit.get_mut(root);
        entry.set(
            constants::DW_AT_producer,
            AttributeValue::String(b"lang Cranelift prototype".to_vec()),
        );
        entry.set(
            constants::DW_AT_name,
            AttributeValue::String(source_path.clone()),
        );
        entry.set(
            constants::DW_AT_comp_dir,
            AttributeValue::String(b".".to_vec()),
        );
        entry.set(constants::DW_AT_stmt_list, AttributeValue::LineProgramRef);
        entry.set(
            constants::DW_AT_low_pc,
            AttributeValue::Address(Address::Symbol {
                symbol: 0,
                addend: 0,
            }),
        );
        entry.set(
            constants::DW_AT_high_pc,
            AttributeValue::Udata(code_size as u64),
        );
    }

    let subprogram = dwarf.unit.add(root, constants::DW_TAG_subprogram);
    let function_position = source_map.function_span().start();
    {
        let entry = dwarf.unit.get_mut(subprogram);
        entry.set(
            constants::DW_AT_name,
            AttributeValue::String(function_name.as_bytes().to_vec()),
        );
        entry.set(
            constants::DW_AT_linkage_name,
            AttributeValue::String(function_name.as_bytes().to_vec()),
        );
        entry.set(
            constants::DW_AT_decl_file,
            AttributeValue::FileIndex(Some(source_file)),
        );
        entry.set(
            constants::DW_AT_decl_line,
            AttributeValue::Udata(u64::from(function_position.line())),
        );
        entry.set(
            constants::DW_AT_decl_column,
            AttributeValue::Udata(u64::from(function_position.column())),
        );
        entry.set(constants::DW_AT_external, AttributeValue::FlagPresent);
        entry.set(
            constants::DW_AT_low_pc,
            AttributeValue::Address(Address::Symbol {
                symbol: 0,
                addend: 0,
            }),
        );
        entry.set(
            constants::DW_AT_high_pc,
            AttributeValue::Udata(code_size as u64),
        );
    }

    let mut sections = Sections::new(DwarfSection::new());
    dwarf
        .write(&mut sections)
        .map_err(|error| error.to_string())?;
    Ok(sections)
}

fn macho_section_name(section: DwarfSectionId) -> Vec<u8> {
    let name = section
        .name()
        .strip_prefix('.')
        .expect("DWARF section names start with a dot");
    format!("__{name}").into_bytes()
}
