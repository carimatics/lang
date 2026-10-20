use std::collections::HashMap;

use gimli::write::{
    Address, AttributeValue, Dwarf, DwarfUnit, EndianVec, LineProgram, LineString, RelocateWriter,
    Relocation as DwarfRelocation, RelocationTarget, Sections, Unit,
};
use gimli::{Encoding, Format, LineEncoding, LittleEndian, SectionId as DwarfSectionId, constants};
use lang_mir::{Signature, Type};
use object::write::{Object, Relocation, SymbolId};
use object::{BinaryFormat, RelocationEncoding, RelocationFlags, RelocationKind, SectionKind};

use super::CompiledSourceMap;

mod source_coff;
pub(super) use source_coff::add_coff_units;

const CODEVIEW_SIGNATURE_C13: u32 = 4;
const DEBUG_S_SYMBOLS: u32 = 0xf1;
const DEBUG_S_LINES: u32 = 0xf2;
const DEBUG_S_STRING_TABLE: u32 = 0xf3;
const DEBUG_S_FILE_CHECKSUMS: u32 = 0xf4;
const S_OBJNAME: u16 = 0x1101;
const S_FRAMEPROC: u16 = 0x1012;
const S_COMPILE3: u16 = 0x113c;
const S_GPROC32_ID: u16 = 0x1147;
const S_BUILDINFO: u16 = 0x114c;
const S_PROC_ID_END: u16 = 0x114f;
const LF_PROCEDURE: u16 = 0x1008;
const LF_ARGLIST: u16 = 0x1201;
const LF_BUILDINFO: u16 = 0x1603;
const LF_FUNC_ID: u16 = 0x1601;
const LF_STRING_ID: u16 = 0x1605;
const TYPE_INDEX_I64: u32 = 0x13;
const TYPE_INDEX_U32: u32 = 0x75;
const TYPE_INDEX_BOOL8: u32 = 0x30;
const FIRST_NON_SIMPLE_TYPE_INDEX: u32 = 0x1000;
const CPU_TYPE_X64: u16 = 0xd0;
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
    signature: &Signature,
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
        BinaryFormat::Coff => add_coff_codeview(
            object,
            function_symbol,
            function_name,
            signature,
            code_size,
            source_map,
        ),
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

    let sections = build_dwarf(function_name, code_size, source_map)?;
    add_macho_sections(object, sections, &[function_symbol])
}

fn add_macho_sections(
    object: &mut Object<'_>,
    mut sections: Sections<DwarfSection>,
    symbols: &[SymbolId],
) -> Result<(), String> {
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
            let (symbol, addend) = match relocation.target {
                RelocationTarget::Symbol(index) => {
                    let symbol = *symbols
                        .get(index)
                        .ok_or_else(|| format!("unknown DWARF symbol index {index}"))?;
                    let definition = object.symbol(symbol);
                    let object::write::SymbolSection::Section(section) = definition.section else {
                        return Err("DWARF address requires a defined code symbol".into());
                    };
                    // Mach-O debug maps use the original object address. Keep
                    // that offset in the implicit addend of a local section
                    // relocation so dsymutil can map each linked function.
                    let addend = i64::try_from(definition.value)
                        .ok()
                        .and_then(|value| value.checked_add(relocation.addend))
                        .ok_or("DWARF object address exceeds the relocation range")?;
                    (object.section_symbol(section), addend)
                }
                RelocationTarget::Section(target) => {
                    let target = *object_sections
                        .get(&target)
                        .ok_or_else(|| format!("missing DWARF target section {}", target.name()))?;
                    (object.section_symbol(target), relocation.addend)
                }
            };
            object
                .add_relocation(
                    object_id,
                    Relocation {
                        offset: relocation.offset as u64,
                        symbol,
                        addend,
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
    function_name: &str,
    signature: &Signature,
    code_size: usize,
    source_map: &CompiledSourceMap,
) -> Result<(), String> {
    if source_map.ranges().is_empty() {
        return Ok(());
    }

    if object.architecture() != object::Architecture::X86_64 {
        return Err("CodeView emission currently supports only x86-64 COFF".to_owned());
    }
    let codeview = build_codeview(function_name, signature, code_size, source_map)?;
    let section = object.add_section(Vec::new(), b".debug$S".to_vec(), SectionKind::Debug);
    object.append_section_data(section, &codeview.symbols, 4);
    for &(offset, kind, size) in &[
        (codeview.procedure_offset, RelocationKind::SectionOffset, 32),
        (codeview.procedure_segment, RelocationKind::SectionIndex, 16),
        (codeview.lines_offset, RelocationKind::SectionOffset, 32),
        (codeview.lines_segment, RelocationKind::SectionIndex, 16),
    ] {
        object
            .add_relocation(
                section,
                Relocation {
                    offset: offset as u64,
                    symbol: function_symbol,
                    addend: 0,
                    flags: RelocationFlags::Generic {
                        kind,
                        encoding: RelocationEncoding::Generic,
                        size,
                    },
                },
            )
            .map_err(|error| error.to_string())?;
    }
    let types = object.add_section(Vec::new(), b".debug$T".to_vec(), SectionKind::Debug);
    object.append_section_data(types, &codeview.types, 4);
    Ok(())
}

struct CodeViewData {
    symbols: Vec<u8>,
    types: Vec<u8>,
    procedure_offset: usize,
    procedure_segment: usize,
    lines_offset: usize,
    lines_segment: usize,
}

fn build_codeview(
    function_name: &str,
    signature: &Signature,
    code_size: usize,
    source_map: &CompiledSourceMap,
) -> Result<CodeViewData, String> {
    let code_size = u32::try_from(code_size)
        .map_err(|_| "CodeView function code size exceeds 32 bits".to_owned())?;
    let parameter_count = u16::try_from(signature.parameters().len())
        .map_err(|_| "CodeView function parameter count exceeds 16 bits".to_owned())?;
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
        let offset = if rows.is_empty() { 0 } else { range.start() };
        rows.push((offset, position.line(), column, end_column));
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

    let mut procedure = Vec::new();
    push_u32(&mut procedure, 0);
    push_u32(&mut procedure, 0);
    push_u32(&mut procedure, 0);
    push_u32(&mut procedure, code_size);
    push_u32(&mut procedure, 0);
    push_u32(&mut procedure, 0);
    push_u32(&mut procedure, FIRST_NON_SIMPLE_TYPE_INDEX + 2);
    let procedure_offset = procedure.len();
    push_u32(&mut procedure, 0);
    let procedure_segment = procedure.len();
    push_u16(&mut procedure, 0);
    procedure.push(0);
    procedure.extend_from_slice(function_name.as_bytes());
    procedure.push(0);

    let mut symbol_records = Vec::new();
    let mut object_name = Vec::new();
    push_u32(&mut object_name, 0);
    object_name.extend_from_slice(function_name.as_bytes());
    object_name.extend_from_slice(b".obj\0");
    append_codeview_symbol(&mut symbol_records, S_OBJNAME, &mut object_name)?;

    let mut compile = Vec::new();
    push_u32(&mut compile, 0);
    push_u16(&mut compile, CPU_TYPE_X64);
    for _ in 0..8 {
        push_u16(&mut compile, 0);
    }
    compile.extend_from_slice(b"lang prototype compiler\0");
    append_codeview_symbol(&mut symbol_records, S_COMPILE3, &mut compile)?;

    let procedure_record_start =
        append_codeview_symbol(&mut symbol_records, S_GPROC32_ID, &mut procedure)?;
    let mut frame_procedure = vec![0; 26];
    append_codeview_symbol(&mut symbol_records, S_FRAMEPROC, &mut frame_procedure)?;
    let mut procedure_end = Vec::new();
    append_codeview_symbol(&mut symbol_records, S_PROC_ID_END, &mut procedure_end)?;
    let mut build_information = Vec::new();
    push_u32(&mut build_information, FIRST_NON_SIMPLE_TYPE_INDEX + 8);
    append_codeview_symbol(&mut symbol_records, S_BUILDINFO, &mut build_information)?;

    let mut symbols = Vec::new();
    push_u32(&mut symbols, CODEVIEW_SIGNATURE_C13);
    let symbol_payload =
        append_codeview_subsection(&mut symbols, DEBUG_S_SYMBOLS, &symbol_records)?;
    let lines_start = append_codeview_subsection(&mut symbols, DEBUG_S_LINES, &lines)?;
    append_codeview_subsection(&mut symbols, DEBUG_S_FILE_CHECKSUMS, &checksums)?;
    append_codeview_subsection(&mut symbols, DEBUG_S_STRING_TABLE, &strings)?;

    let mut types = Vec::new();
    push_u32(&mut types, CODEVIEW_SIGNATURE_C13);
    let mut arguments = Vec::new();
    push_u32(&mut arguments, u32::from(parameter_count));
    for &parameter in signature.parameters() {
        push_u32(&mut arguments, codeview_type(parameter));
    }
    append_codeview_type(&mut types, LF_ARGLIST, &arguments)?;
    let mut procedure_type = Vec::new();
    push_u32(&mut procedure_type, codeview_type(signature.result()));
    procedure_type.push(0);
    procedure_type.push(0);
    push_u16(&mut procedure_type, parameter_count);
    push_u32(&mut procedure_type, FIRST_NON_SIMPLE_TYPE_INDEX);
    append_codeview_type(&mut types, LF_PROCEDURE, &procedure_type)?;
    let mut function_id = Vec::new();
    push_u32(&mut function_id, 0);
    push_u32(&mut function_id, FIRST_NON_SIMPLE_TYPE_INDEX + 1);
    function_id.extend_from_slice(function_name.as_bytes());
    function_id.push(0);
    append_codeview_type(&mut types, LF_FUNC_ID, &function_id)?;
    for value in [
        ".",
        source_map.source_path(),
        "",
        "lang prototype compiler",
        "",
    ] {
        let mut string_id = Vec::new();
        push_u32(&mut string_id, 0);
        string_id.extend_from_slice(value.as_bytes());
        string_id.push(0);
        append_codeview_type(&mut types, LF_STRING_ID, &string_id)?;
    }
    let mut build_information = Vec::new();
    push_u16(&mut build_information, 5);
    for index in [3, 6, 4, 5, 7] {
        push_u32(&mut build_information, FIRST_NON_SIMPLE_TYPE_INDEX + index);
    }
    append_codeview_type(&mut types, LF_BUILDINFO, &build_information)?;

    Ok(CodeViewData {
        symbols,
        types,
        procedure_offset: symbol_payload + procedure_record_start + 4 + procedure_offset,
        procedure_segment: symbol_payload + procedure_record_start + 4 + procedure_segment,
        lines_offset: lines_start,
        lines_segment: lines_start + 4,
    })
}

fn codeview_type(value_type: Type) -> u32 {
    match value_type {
        Type::I64 => TYPE_INDEX_I64,
        Type::Bool => TYPE_INDEX_BOOL8,
        Type::Own(_) => TYPE_INDEX_U32,
    }
}

/// A source-level name and location map for a private native function. No
/// source parameter locations are invented for the backend's pointer ABI.
pub(super) struct NativeDebugFunction<'a> {
    pub symbol: SymbolId,
    pub name: &'a str,
    pub linkage_name: &'a str,
    pub path: &'a str,
    pub span: lang_mir::SourceSpan,
    pub code_size: usize,
    pub ranges: Vec<NativeDebugRange<'a>>,
}
pub(super) struct NativeDebugRange<'a> {
    pub start: u32,
    pub end: u32,
    pub path: &'a str,
    pub span: lang_mir::SourceSpan,
}

pub(super) fn add_macho_units(
    object: &mut Object<'_>,
    functions: &[NativeDebugFunction<'_>],
) -> Result<(), String> {
    let encoding = Encoding {
        format: Format::Dwarf32,
        version: 4,
        address_size: 8,
    };
    let mut dwarf = Dwarf::default();
    let mut symbols = Vec::with_capacity(functions.len());
    for (index, function) in functions.iter().enumerate() {
        let mut lines = LineProgram::new(
            encoding,
            LineEncoding::default(),
            LineString::String(b".".to_vec()),
            None,
            LineString::String(function.path.as_bytes().to_vec()),
            None,
        );
        let primary = lines.add_file(
            LineString::String(function.path.as_bytes().to_vec()),
            lines.default_directory(),
            None,
        );
        let mut files = HashMap::new();
        files.insert(function.path, primary);
        lines.begin_sequence(Some(Address::Symbol {
            symbol: index,
            addend: 0,
        }));
        let mut previous = None;
        for range in &function.ranges {
            if range.start == range.end || previous == Some(range.start) {
                continue;
            }
            let file = *files.entry(range.path).or_insert_with(|| {
                lines.add_file(
                    LineString::String(range.path.as_bytes().to_vec()),
                    lines.default_directory(),
                    None,
                )
            });
            let row = lines.row();
            row.address_offset = u64::from(range.start);
            row.file = file;
            row.line = u64::from(range.span.start().line());
            row.column = u64::from(range.span.start().column());
            row.is_statement = true;
            lines.generate_row();
            previous = Some(range.start);
        }
        lines.end_sequence(function.code_size as u64);
        let mut unit = Unit::new(encoding, lines);
        let root = unit.root();
        {
            let entry = unit.get_mut(root);
            entry.set(
                constants::DW_AT_producer,
                AttributeValue::String(b"lang native source".to_vec()),
            );
            entry.set(constants::DW_AT_use_UTF8, AttributeValue::FlagPresent);
            entry.set(
                constants::DW_AT_name,
                AttributeValue::String(function.path.as_bytes().to_vec()),
            );
            entry.set(
                constants::DW_AT_comp_dir,
                AttributeValue::String(b".".to_vec()),
            );
            entry.set(constants::DW_AT_stmt_list, AttributeValue::LineProgramRef);
            entry.set(
                constants::DW_AT_low_pc,
                AttributeValue::Address(Address::Symbol {
                    symbol: index,
                    addend: 0,
                }),
            );
            entry.set(
                constants::DW_AT_high_pc,
                AttributeValue::Udata(function.code_size as u64),
            );
        }
        let procedure = unit.add(root, constants::DW_TAG_subprogram);
        {
            let entry = unit.get_mut(procedure);
            entry.set(
                constants::DW_AT_name,
                AttributeValue::String(function.name.as_bytes().to_vec()),
            );
            entry.set(
                constants::DW_AT_linkage_name,
                AttributeValue::String(function.linkage_name.as_bytes().to_vec()),
            );
            entry.set(
                constants::DW_AT_decl_file,
                AttributeValue::FileIndex(Some(primary)),
            );
            entry.set(
                constants::DW_AT_decl_line,
                AttributeValue::Udata(u64::from(function.span.start().line())),
            );
            entry.set(
                constants::DW_AT_decl_column,
                AttributeValue::Udata(u64::from(function.span.start().column())),
            );
            entry.set(constants::DW_AT_external, AttributeValue::FlagPresent);
            entry.set(
                constants::DW_AT_calling_convention,
                AttributeValue::Data1(constants::DW_CC_nocall.0),
            );
            entry.set(
                constants::DW_AT_low_pc,
                AttributeValue::Address(Address::Symbol {
                    symbol: index,
                    addend: 0,
                }),
            );
            entry.set(
                constants::DW_AT_high_pc,
                AttributeValue::Udata(function.code_size as u64),
            );
        }
        dwarf.units.add(unit);
        symbols.push(function.symbol);
    }
    let mut sections = Sections::new(DwarfSection::new());
    dwarf
        .write(&mut sections)
        .map_err(|error| error.to_string())?;
    add_macho_sections(object, sections, &symbols)
}

fn append_codeview_symbol(
    records: &mut Vec<u8>,
    kind: u16,
    body: &mut Vec<u8>,
) -> Result<usize, String> {
    while !(body.len() + 4).is_multiple_of(4) {
        body.push(0);
    }
    let record_start = records.len();
    let record_length = body
        .len()
        .checked_add(2)
        .ok_or_else(|| "CodeView symbol record size overflowed".to_owned())?;
    push_u16(
        records,
        u16::try_from(record_length)
            .map_err(|_| "CodeView symbol record exceeds 16 bits".to_owned())?,
    );
    push_u16(records, kind);
    records.extend_from_slice(body);
    Ok(record_start)
}

fn append_codeview_type(records: &mut Vec<u8>, kind: u16, body: &[u8]) -> Result<(), String> {
    let record_start = records.len();
    push_u16(records, 0);
    push_u16(records, kind);
    records.extend_from_slice(body);
    let padding = (4 - (records.len() - record_start) % 4) % 4;
    for value in (1..=padding).rev() {
        records.push(0xf0 + value as u8);
    }
    let record_length = records
        .len()
        .checked_sub(record_start + 2)
        .ok_or_else(|| "CodeView type record size overflowed".to_owned())?;
    records[record_start..record_start + 2].copy_from_slice(
        &u16::try_from(record_length)
            .map_err(|_| "CodeView type record exceeds 16 bits".to_owned())?
            .to_le_bytes(),
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use lang_mir::{
        BlockSourceMap, Function, FunctionSourceMap, ResourceTypeId, SourcePosition, SourceSpan,
        Terminator, ValueId,
    };
    use object::{Object as _, ObjectSection as _};

    #[test]
    fn codeview_records_the_actual_machine_signature() {
        let span = SourceSpan::new(
            SourcePosition::new(1, 1).unwrap(),
            SourcePosition::new(1, 9).unwrap(),
        )
        .unwrap();
        let signature = Signature::new(
            vec![Type::I64, Type::Own(ResourceTypeId::new(7))],
            Type::Own(ResourceTypeId::new(7)),
        );
        let function = Function::new(
            "typed_owner",
            signature,
            vec![],
            Terminator::Return(ValueId::new(1)),
        )
        .with_source_map(
            FunctionSourceMap::new(
                "resource.lang",
                span,
                vec![BlockSourceMap::new(vec![], span)],
            )
            .unwrap(),
        )
        .unwrap();
        let artifact =
            crate::emit_object_for_target(&function, "x86_64-pc-windows-msvc".parse().unwrap())
                .unwrap();
        let object = object::File::parse(artifact.bytes()).unwrap();
        let types = object.section_by_name(".debug$T").unwrap().data().unwrap();
        let u16_at = |offset| u16::from_le_bytes(types[offset..offset + 2].try_into().unwrap());
        let u32_at = |offset| u32::from_le_bytes(types[offset..offset + 4].try_into().unwrap());
        // Decode the public CodeView record layout independently of the writer.
        assert_eq!(u16_at(6), 0x1201); // LF_ARGLIST
        assert_eq!(u32_at(8), 2);
        assert_eq!(u32_at(12), 0x13); // signed 64-bit
        assert_eq!(u32_at(16), 0x75); // unsigned 32-bit
        let procedure = 4 + 2 + usize::from(u16_at(4));
        assert_eq!(u16_at(procedure + 2), 0x1008); // LF_PROCEDURE
        assert_eq!(u32_at(procedure + 4), 0x75);
        assert_eq!(u16_at(procedure + 10), 2);
    }
}
