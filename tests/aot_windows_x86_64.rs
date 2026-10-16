use lang::linker::LinkTarget;
use lang::{
    prototype_add_function, prototype_fibonacci_function, prototype_host_allocation_burst_function,
    prototype_host_allocation_churn_function, prototype_host_allocation_variable_function,
    prototype_host_callback_function, prototype_min_function, prototype_sum_below_function,
    prototype_sum_to_function,
};
use lang_codegen_clif::{Triple, emit_object_for_target};
use object::{
    Architecture, BinaryFormat, Object, ObjectSection, ObjectSymbol, RelocationKind,
    RelocationTarget,
};

#[test]
fn emits_windows_x86_64_coff_with_expected_symbols() {
    let target = LinkTarget::WindowsX86_64Msvc
        .triple()
        .parse::<Triple>()
        .expect("the Windows x86-64 target triple is valid");
    let add = emit_object_for_target(&prototype_add_function(), target.clone())
        .expect("emits the add function for Windows x86-64");
    let fibonacci = emit_object_for_target(&prototype_fibonacci_function(), target.clone())
        .expect("emits the fibonacci function for Windows x86-64");
    let callback = emit_object_for_target(&prototype_host_callback_function(), target.clone())
        .expect("emits the host-callback function for Windows x86-64");
    let minimum = emit_object_for_target(&prototype_min_function(), target.clone())
        .expect("emits the minimum function for Windows x86-64");
    let allocation_burst =
        emit_object_for_target(&prototype_host_allocation_burst_function(), target.clone())
            .expect("emits the host-allocation burst function for Windows x86-64");
    let allocation_churn =
        emit_object_for_target(&prototype_host_allocation_churn_function(), target.clone())
            .expect("emits the host-allocation churn function for Windows x86-64");
    let allocation_variable = emit_object_for_target(
        &prototype_host_allocation_variable_function(),
        target.clone(),
    )
    .expect("emits the variable host-allocation function for Windows x86-64");
    let sum_below = emit_object_for_target(&prototype_sum_below_function(), target.clone())
        .expect("emits the conditional sum function for Windows x86-64");
    let sum = emit_object_for_target(&prototype_sum_to_function(), target)
        .expect("emits the sum function for Windows x86-64");

    let add_object = object::File::parse(add.bytes()).expect("parses the add COFF object");
    assert_eq!(add_object.format(), BinaryFormat::Coff);
    assert_eq!(add_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(&add_object, add.function_symbol()));
    assert_codeview_source_map(&add_object, add.function_symbol(), &[(2, 3), (3, 3)]);

    let fibonacci_object =
        object::File::parse(fibonacci.bytes()).expect("parses the fibonacci COFF object");
    assert_eq!(fibonacci_object.format(), BinaryFormat::Coff);
    assert_eq!(fibonacci_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &fibonacci_object,
        fibonacci.function_symbol()
    ));

    let callback_object =
        object::File::parse(callback.bytes()).expect("parses the callback COFF object");
    assert_eq!(callback_object.format(), BinaryFormat::Coff);
    assert_eq!(callback_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &callback_object,
        callback.function_symbol()
    ));
    assert!(has_undefined_symbol(&callback_object, "lang_host_i64_0"));
    assert_codeview_source_map(
        &callback_object,
        callback.function_symbol(),
        &[(17, 3), (18, 3)],
    );

    let minimum_object =
        object::File::parse(minimum.bytes()).expect("parses the minimum COFF object");
    assert_eq!(minimum_object.format(), BinaryFormat::Coff);
    assert_eq!(minimum_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &minimum_object,
        minimum.function_symbol()
    ));
    assert_codeview_source_map(
        &minimum_object,
        minimum.function_symbol(),
        &[(9, 3), (11, 3), (13, 3)],
    );

    let allocation_burst_object = object::File::parse(allocation_burst.bytes())
        .expect("parses the host-allocation burst COFF object");
    assert_eq!(allocation_burst_object.format(), BinaryFormat::Coff);
    assert_eq!(allocation_burst_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &allocation_burst_object,
        allocation_burst.function_symbol()
    ));
    assert!(has_undefined_symbol(
        &allocation_burst_object,
        "lang_host_i64_2"
    ));

    let allocation_churn_object = object::File::parse(allocation_churn.bytes())
        .expect("parses the host-allocation churn COFF object");
    assert_eq!(allocation_churn_object.format(), BinaryFormat::Coff);
    assert_eq!(allocation_churn_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &allocation_churn_object,
        allocation_churn.function_symbol()
    ));
    assert!(has_undefined_symbol(
        &allocation_churn_object,
        "lang_host_i64_3"
    ));

    let allocation_variable_object = object::File::parse(allocation_variable.bytes())
        .expect("parses the variable host-allocation COFF object");
    assert_eq!(allocation_variable_object.format(), BinaryFormat::Coff);
    assert_eq!(
        allocation_variable_object.architecture(),
        Architecture::X86_64
    );
    assert!(has_defined_symbol(
        &allocation_variable_object,
        allocation_variable.function_symbol()
    ));
    assert!(has_undefined_symbol(
        &allocation_variable_object,
        "lang_host_i64_4"
    ));

    let sum_below_object =
        object::File::parse(sum_below.bytes()).expect("parses the conditional sum COFF object");
    assert_eq!(sum_below_object.format(), BinaryFormat::Coff);
    assert_eq!(sum_below_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(
        &sum_below_object,
        sum_below.function_symbol()
    ));

    let sum_object = object::File::parse(sum.bytes()).expect("parses the sum COFF object");
    assert_eq!(sum_object.format(), BinaryFormat::Coff);
    assert_eq!(sum_object.architecture(), Architecture::X86_64);
    assert!(has_defined_symbol(&sum_object, sum.function_symbol()));
}

fn has_defined_symbol(object: &object::File<'_>, name: &str) -> bool {
    object
        .symbols()
        .any(|symbol| symbol.name() == Ok(name) && !symbol.is_undefined())
}

fn has_undefined_symbol(object: &object::File<'_>, name: &str) -> bool {
    object
        .symbols()
        .any(|symbol| symbol.name() == Ok(name) && symbol.is_undefined())
}

fn assert_codeview_source_map(
    object: &object::File<'_>,
    function: &str,
    expected_positions: &[(u32, u16)],
) {
    let section = object
        .section_by_name(".debug$S")
        .expect("source-mapped COFF object contains .debug$S");
    let data = section.data().expect("reads .debug$S data");
    assert_eq!(read_u32(data, 0), 4, "uses the CodeView C13 signature");

    let mut cursor = 4;
    let mut lines = None;
    let mut lines_start = None;
    let mut strings = None;
    let mut checksums = None;
    while cursor < data.len() {
        let kind = read_u32(data, cursor);
        let size = read_u32(data, cursor + 4) as usize;
        let start = cursor + 8;
        let end = start
            .checked_add(size)
            .expect("subsection size does not overflow");
        assert!(end <= data.len(), "CodeView subsection stays in bounds");
        match kind {
            0xf2 => {
                lines = Some(&data[start..end]);
                lines_start = Some(start as u64);
            }
            0xf3 => strings = Some(&data[start..end]),
            0xf4 => checksums = Some(&data[start..end]),
            _ => {}
        }
        cursor = end.next_multiple_of(4);
    }
    assert_eq!(
        cursor,
        data.len(),
        "CodeView subsections are 4-byte aligned"
    );

    let strings = strings.expect("contains a CodeView string table");
    let checksums = checksums.expect("contains a CodeView checksum table");
    assert_eq!(read_u32(checksums, 0), 1, "source path starts at offset 1");
    assert_eq!(checksums[4], 0, "records no unavailable source checksum");
    assert_eq!(checksums[5], 0, "uses the CodeView None checksum kind");
    assert_eq!(strings[0], 0, "reserves string-table offset zero");
    let source_end = strings[1..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|offset| offset + 1)
        .expect("source path is null terminated");
    assert_eq!(
        std::str::from_utf8(&strings[1..source_end]).expect("source path is UTF-8"),
        "benchmarks/fixtures/native-debug.mir"
    );

    let lines = lines.expect("contains a CodeView line table");
    assert_eq!(read_u32(lines, 0), 0, "relocated function offset is zero");
    assert_eq!(read_u16(lines, 4), 0, "relocated function section is zero");
    assert_eq!(read_u16(lines, 6), 1, "line table contains columns");
    assert!(read_u32(lines, 8) > 0, "line table records the code size");
    assert_eq!(
        read_u32(lines, 12),
        0,
        "line block uses checksum entry zero"
    );
    let row_count = read_u32(lines, 16) as usize;
    assert_eq!(row_count, expected_positions.len());
    assert_eq!(read_u32(lines, 20) as usize, 12 + row_count * 12);
    let line_start = 24;
    let column_start = line_start + row_count * 8;
    let positions = (0..row_count)
        .map(|index| {
            let flags = read_u32(lines, line_start + index * 8 + 4);
            assert_ne!(flags & (1 << 31), 0, "line row is a statement");
            (
                flags & 0x00ff_ffff,
                read_u16(lines, column_start + index * 4),
            )
        })
        .collect::<Vec<_>>();
    for expected in expected_positions {
        assert!(
            positions.contains(expected),
            "missing CodeView line/column {expected:?}: {positions:?}"
        );
    }

    let lines_start = lines_start.expect("records the line-subsection offset");
    let mut relocations = section.relocations().collect::<Vec<_>>();
    relocations.sort_by_key(|(offset, _)| *offset);
    assert_eq!(relocations.len(), 2);
    assert_eq!(relocations[0].0, lines_start);
    assert_eq!(relocations[0].1.kind(), RelocationKind::SectionOffset);
    assert_eq!(relocations[0].1.size(), 32);
    assert_eq!(relocations[1].0, lines_start + 4);
    assert_eq!(relocations[1].1.kind(), RelocationKind::SectionIndex);
    assert_eq!(relocations[1].1.size(), 16);
    for (_, relocation) in relocations {
        let RelocationTarget::Symbol(symbol) = relocation.target() else {
            panic!("CodeView relocation does not target a function symbol");
        };
        assert_eq!(
            object
                .symbol_by_index(symbol)
                .expect("CodeView relocation target exists")
                .name(),
            Ok(function)
        );
    }
}

fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        data[offset..offset + 2]
            .try_into()
            .expect("reads an in-bounds u16"),
    )
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .expect("reads an in-bounds u32"),
    )
}
