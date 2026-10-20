//! Whole-source CodeView C13 with the private bodies' physical pointer ABI.
use super::*;

const TYPE_INDEX_I32: u32 = 0x74;
const TYPE_INDEX_VOID_POINTER64: u32 = 0x603;

struct LineRow {
    offset: u32,
    line: u32,
    column: u16,
    end_column: u16,
}

struct AddressRelocation {
    offset: usize,
    symbol: SymbolId,
    kind: RelocationKind,
    bits: u8,
}

fn address(relocations: &mut Vec<AddressRelocation>, offset: usize, symbol: SymbolId) {
    relocations.push(AddressRelocation {
        offset,
        symbol,
        kind: RelocationKind::SectionOffset,
        bits: 32,
    });
    relocations.push(AddressRelocation {
        offset: offset + 4,
        symbol,
        kind: RelocationKind::SectionIndex,
        bits: 16,
    });
}

pub(crate) fn add_coff_units(
    object: &mut Object<'_>,
    functions: &[NativeDebugFunction<'_>],
) -> Result<(), String> {
    if object.architecture() != object::Architecture::X86_64 {
        return Err("source CodeView currently supports only x86-64 COFF".into());
    }
    if functions.is_empty() {
        return Ok(());
    }
    // Both program::compile and owned::emit lower private source
    // bodies to i32 (*)(void *, void *). Describe that physical convention,
    // without inventing source parameter variables or their locations.
    let mut types = Vec::new();
    push_u32(&mut types, CODEVIEW_SIGNATURE_C13);
    let mut arguments = Vec::new();
    push_u32(&mut arguments, 2);
    push_u32(&mut arguments, TYPE_INDEX_VOID_POINTER64);
    push_u32(&mut arguments, TYPE_INDEX_VOID_POINTER64);
    append_codeview_type(&mut types, LF_ARGLIST, &arguments)?;
    let mut procedure = Vec::new();
    push_u32(&mut procedure, TYPE_INDEX_I32);
    procedure.extend_from_slice(&[0, 0]);
    push_u16(&mut procedure, 2);
    push_u32(&mut procedure, FIRST_NON_SIMPLE_TYPE_INDEX);
    append_codeview_type(&mut types, LF_PROCEDURE, &procedure)?;
    let mut strings = vec![0];
    let mut checksums = Vec::new();
    let mut files = HashMap::new();
    for f in functions {
        for path in std::iter::once(f.path).chain(f.ranges.iter().map(|r| r.path)) {
            if files.contains_key(path) {
                continue;
            }
            let string = u32::try_from(strings.len())
                .map_err(|_| "CodeView string table exceeds 32 bits")?;
            let checksum = u32::try_from(checksums.len())
                .map_err(|_| "CodeView checksum table exceeds 32 bits")?;
            strings.extend_from_slice(path.as_bytes());
            strings.push(0);
            push_u32(&mut checksums, string);
            checksums.extend_from_slice(&[0, 0]); // No invented source checksum.
            pad_to_four(&mut checksums);
            files.insert(path, checksum);
        }
    }
    let mut records = Vec::new();
    let mut name = Vec::new();
    push_u32(&mut name, 0);
    name.extend_from_slice(b"lang-native-source.obj\0");
    append_codeview_symbol(&mut records, S_OBJNAME, &mut name)?;
    let mut compile = Vec::new();
    push_u32(&mut compile, 0);
    push_u16(&mut compile, CPU_TYPE_X64);
    for _ in 0..8 {
        push_u16(&mut compile, 0);
    }
    compile.extend_from_slice(b"lang native source compiler\0");
    append_codeview_symbol(&mut records, S_COMPILE3, &mut compile)?;
    let mut procedure_addresses = Vec::new();
    for (index, f) in functions.iter().enumerate() {
        let size = u32::try_from(f.code_size).map_err(|_| "CodeView function exceeds 32 bits")?;
        let type_index = u32::try_from(index)
            .ok()
            .and_then(|i| i.checked_add(FIRST_NON_SIMPLE_TYPE_INDEX + 2))
            .ok_or("CodeView function type index exceeds 32 bits")?;
        let mut function_id = Vec::new();
        push_u32(&mut function_id, 0);
        push_u32(&mut function_id, FIRST_NON_SIMPLE_TYPE_INDEX + 1);
        function_id.extend_from_slice(f.name.as_bytes());
        function_id.push(0);
        append_codeview_type(&mut types, LF_FUNC_ID, &function_id)?;
        let mut p = Vec::new();
        for word in [0, 0, 0, size, 0, 0, type_index] {
            push_u32(&mut p, word);
        }
        push_u32(&mut p, 0);
        push_u16(&mut p, 0);
        p.push(0);
        p.extend_from_slice(f.name.as_bytes());
        p.push(0);
        let start = append_codeview_symbol(&mut records, S_GPROC32_ID, &mut p)?;
        procedure_addresses.push((start + 4 + 28, f.symbol));
        append_codeview_symbol(&mut records, S_PROC_ID_END, &mut Vec::new())?;
    }
    let mut symbols = Vec::new();
    push_u32(&mut symbols, CODEVIEW_SIGNATURE_C13);
    let start = append_codeview_subsection(&mut symbols, DEBUG_S_SYMBOLS, &records)?;
    let mut relocations = Vec::new();
    for (offset, symbol) in procedure_addresses {
        address(&mut relocations, start + offset, symbol);
    }

    for f in functions {
        // Each file gets its own monotonically ordered row block. A function
        // may return to a previous file without losing either file identity.
        let mut blocks: Vec<(&str, Vec<LineRow>)> = Vec::new();
        let mut block_indices = HashMap::new();
        let mut previous = None;
        for r in &f.ranges {
            if r.start == r.end || previous == Some(r.start) {
                continue;
            }
            let p = r.span.start();
            if p.line() > CODEVIEW_MAX_LINE {
                return Err("CodeView source line exceeds 24 bits".into());
            }
            let column =
                u16::try_from(p.column()).map_err(|_| "CodeView source column exceeds 16 bits")?;
            let end = if r.span.end().line() == p.line() {
                u16::try_from(r.span.end().column())
                    .map_err(|_| "CodeView source end column exceeds 16 bits")?
            } else {
                column
            };
            let offset = if previous.is_none() { 0 } else { r.start };
            let index = *block_indices.entry(r.path).or_insert_with(|| {
                blocks.push((r.path, Vec::new()));
                blocks.len() - 1
            });
            blocks[index].1.push(LineRow {
                offset,
                line: p.line(),
                column,
                end_column: end,
            });
            previous = Some(r.start);
        }
        if blocks.is_empty() {
            return Err("CodeView source map contains no emitted code ranges".into());
        }
        let mut lines = Vec::new();
        push_u32(&mut lines, 0);
        push_u16(&mut lines, 0);
        push_u16(&mut lines, CODEVIEW_LINES_HAVE_COLUMNS);
        push_u32(
            &mut lines,
            u32::try_from(f.code_size).map_err(|_| "CodeView code size exceeds 32 bits")?,
        );
        for (path, rows) in blocks {
            let count =
                u32::try_from(rows.len()).map_err(|_| "CodeView row count exceeds 32 bits")?;
            let size = count
                .checked_mul(12)
                .and_then(|n| n.checked_add(12))
                .ok_or("CodeView line block overflow")?;
            push_u32(&mut lines, files[path]);
            push_u32(&mut lines, count);
            push_u32(&mut lines, size);
            for row in &rows {
                push_u32(&mut lines, row.offset);
                push_u32(&mut lines, row.line | CODEVIEW_LINE_IS_STATEMENT);
            }
            for row in &rows {
                push_u16(&mut lines, row.column);
                push_u16(&mut lines, row.end_column);
            }
        }
        let start = append_codeview_subsection(&mut symbols, DEBUG_S_LINES, &lines)?;
        address(&mut relocations, start, f.symbol);
    }
    append_codeview_subsection(&mut symbols, DEBUG_S_FILE_CHECKSUMS, &checksums)?;
    append_codeview_subsection(&mut symbols, DEBUG_S_STRING_TABLE, &strings)?;
    // Attach only after every bounded line/column/record has been encoded.
    let section = object.add_section(Vec::new(), b".debug$S".to_vec(), SectionKind::Debug);
    object.append_section_data(section, &symbols, 4);
    let type_section = object.add_section(Vec::new(), b".debug$T".to_vec(), SectionKind::Debug);
    object.append_section_data(type_section, &types, 4);
    for r in relocations {
        object
            .add_relocation(
                section,
                Relocation {
                    offset: r.offset as u64,
                    symbol: r.symbol,
                    addend: 0,
                    flags: RelocationFlags::Generic {
                        kind: r.kind,
                        encoding: RelocationEncoding::Generic,
                        size: r.bits,
                    },
                },
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_mir::{SourcePosition, SourceSpan};
    use object::write::{Symbol, SymbolSection};
    use object::{Object as _, ObjectSection as _, ObjectSymbol as _};

    fn span(line: u32, start: u32, end: u32) -> SourceSpan {
        SourceSpan::new(
            SourcePosition::new(line, start).unwrap(),
            SourcePosition::new(line, end).unwrap(),
        )
        .unwrap()
    }
    fn image() -> (Object<'static>, [SymbolId; 2]) {
        let mut object = Object::new(
            BinaryFormat::Coff,
            object::Architecture::X86_64,
            object::Endianness::Little,
        );
        let text = object.add_section(Vec::new(), b".text".to_vec(), SectionKind::Text);
        object.append_section_data(text, &[0; 32], 16);
        let symbols = ["native_first", "native_second"].map(|name| {
            object.add_symbol(Symbol {
                name: name.as_bytes().to_vec(),
                value: if name == "native_first" { 0 } else { 16 },
                size: 16,
                kind: object::SymbolKind::Text,
                scope: object::SymbolScope::Linkage,
                weak: false,
                section: SymbolSection::Section(text),
                flags: object::SymbolFlags::None,
            })
        });
        (object, symbols)
    }
    fn function(symbol: SymbolId) -> NativeDebugFunction<'static> {
        NativeDebugFunction {
            symbol,
            name: "first",
            linkage_name: "native_first",
            path: "root.lang",
            span: span(1, 1, 5),
            code_size: 16,
            ranges: vec![
                NativeDebugRange {
                    start: 0,
                    end: 4,
                    path: "root.lang",
                    span: span(1, 3, 5),
                },
                NativeDebugRange {
                    start: 4,
                    end: 8,
                    path: "lib/other.lang",
                    span: span(10, 2, 7),
                },
                NativeDebugRange {
                    start: 8,
                    end: 16,
                    path: "root.lang",
                    span: span(2, 1, 4),
                },
            ],
        }
    }
    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn multiple_functions_files_columns_and_physical_signature_survive_coff() {
        let (mut image, symbols) = image();
        let first = function(symbols[0]);
        let mut second = function(symbols[1]);
        second.name = "second";
        second.linkage_name = "native_second";
        second.path = "lib/other.lang";
        second.ranges.truncate(1);
        add_coff_units(&mut image, &[first, second]).unwrap();
        let bytes = image.write().unwrap();
        let parsed = object::File::parse(bytes.as_slice()).unwrap();
        let types = parsed.section_by_name(".debug$T").unwrap().data().unwrap();
        // Decode fixed CodeView types independently of writer helpers.
        assert_eq!(u32_at(types, 8), 2);
        assert_eq!(u32_at(types, 12), 0x603); // 64-bit void pointer
        assert_eq!(u32_at(types, 16), 0x603);
        assert_eq!(u32_at(types, 24), 0x74); // physical i32 status
        let debug = parsed.section_by_name(".debug$S").unwrap();
        let relocations: Vec<_> = debug
            .relocations()
            .map(|(_, r)| {
                let object::RelocationTarget::Symbol(i) = r.target() else {
                    panic!("expected function")
                };
                (
                    parsed.symbol_by_index(i).unwrap().name().unwrap(),
                    r.kind(),
                    r.size(),
                )
            })
            .collect();
        for name in ["native_first", "native_second"] {
            assert_eq!(
                relocations
                    .iter()
                    .filter(|r| **r == (name, RelocationKind::SectionOffset, 32))
                    .count(),
                2
            );
            assert_eq!(
                relocations
                    .iter()
                    .filter(|r| **r == (name, RelocationKind::SectionIndex, 16))
                    .count(),
                2
            );
        }
        let data = debug.data().unwrap();
        let mut at = 4;
        let mut lines = Vec::new();
        while at < data.len() {
            let kind = u32_at(data, at);
            let length = u32_at(data, at + 4) as usize;
            if kind == 0xf2 {
                lines.push(&data[at + 8..at + 8 + length]);
            }
            at = (at + 8 + length + 3) & !3;
        }
        assert_eq!(lines.len(), 2);
        // First file block: root.lang appears at offsets 0 and 8; the other
        // file stays distinct at offset 4. Columns are scalar positions.
        let first = lines[0];
        assert_eq!(
            [u32_at(first, 12), u32_at(first, 16), u32_at(first, 20)],
            [0, 2, 36]
        );
        assert_eq!([u32_at(first, 24), u32_at(first, 32)], [0, 8]);
        assert_eq!(&first[40..48], &[3, 0, 5, 0, 1, 0, 4, 0]);
        assert_eq!(
            [u32_at(first, 48), u32_at(first, 52), u32_at(first, 56)],
            [8, 1, 24]
        );
        assert_eq!(u32_at(first, 60), 4);
        assert_eq!(u32_at(first, 64) & 0xffffff, 10);
        assert_eq!(&first[68..72], &[2, 0, 7, 0]);
    }

    #[test]
    fn unrepresentable_codeview_positions_fail_before_sections_are_attached() {
        for bad in [
            span(1 << 24, 1, 2),
            span(1, 1 << 16, (1 << 16) + 1),
            span(1, 1, 1 << 16),
        ] {
            let (mut image, symbols) = image();
            let mut f = function(symbols[0]);
            f.ranges[0].span = bad;
            assert!(add_coff_units(&mut image, &[f]).is_err());
            let bytes = image.write().unwrap();
            let parsed = object::File::parse(bytes.as_slice()).unwrap();
            assert!(parsed.section_by_name(".debug$S").is_none());
            assert!(parsed.section_by_name(".debug$T").is_none());
        }
    }
}
