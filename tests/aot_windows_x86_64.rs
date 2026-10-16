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

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn links_source_mapped_codeview_into_a_pdb() {
    let directory = TestDirectory::create();
    let target = LinkTarget::WindowsX86_64Msvc
        .triple()
        .parse::<Triple>()
        .expect("the Windows x86-64 target triple is valid");
    let artifacts = [
        (
            "prototype-add.obj",
            emit_object_for_target(&prototype_add_function(), target.clone())
                .expect("emits the add function for Windows x86-64"),
        ),
        (
            "prototype-min.obj",
            emit_object_for_target(&prototype_min_function(), target.clone())
                .expect("emits the minimum function for Windows x86-64"),
        ),
        (
            "prototype-host-callback.obj",
            emit_object_for_target(&prototype_host_callback_function(), target)
                .expect("emits the host-callback function for Windows x86-64"),
        ),
    ];
    for (name, artifact) in &artifacts {
        fs::write(directory.path().join(name), artifact.bytes()).expect("writes a COFF object");
    }

    let image_name = "prototype-debug.dll";
    let pdb_name = "prototype-debug.pdb";
    let mut linker = windows_debug_linker();
    let link = linker
        .current_dir(directory.path())
        .args([
            "/dll",
            "/noentry",
            "/nodefaultlib",
            "/debug:full",
            "/force:unresolved",
            "/export:prototype_add",
            "/export:prototype_min",
            "/export:prototype_host_callback",
            "/pdb:prototype-debug.pdb",
            "/out:prototype-debug.dll",
            "prototype-add.obj",
            "prototype-min.obj",
            "prototype-host-callback.obj",
        ])
        .output()
        .expect("runs the Windows-compatible linker");
    assert!(
        link.status.success(),
        "PDB link failed with status {}:\nstdout:\n{}\nstderr:\n{}",
        link.status,
        String::from_utf8_lossy(&link.stdout),
        String::from_utf8_lossy(&link.stderr)
    );

    let image_path = directory.path().join(image_name);
    let pdb_path = directory.path().join(pdb_name);
    let image_bytes = fs::read(&image_path).expect("reads the linked PE image");
    let image = object::File::parse(image_bytes.as_slice()).expect("parses the linked PE image");
    let codeview = image
        .pdb_info()
        .expect("parses the PE CodeView directory")
        .expect("linked PE image references a PDB");
    assert!(codeview.path().ends_with(pdb_name.as_bytes()));
    assert!(codeview.age() > 0);
    assert_ne!(codeview.guid(), [0; 16]);

    let pdb = fs::read(&pdb_path).expect("reads the linked PDB");
    assert!(pdb.starts_with(b"Microsoft C/C++ MSF 7.00"));
    for expected in [
        b"benchmarks/fixtures/native-debug.mir".as_slice(),
        b"prototype_add".as_slice(),
        b"prototype_min".as_slice(),
        b"prototype_host_callback".as_slice(),
    ] {
        assert!(
            contains_bytes(&pdb, expected),
            "PDB is missing {}",
            String::from_utf8_lossy(expected)
        );
    }

    #[cfg(target_os = "macos")]
    assert_lldb_resolves_pdb_lines(&image_path);

    println!(
        "CodeView/PDB: add={} B, min={} B, callback={} B, image={} B, PDB={} B",
        artifacts[0].1.bytes().len(),
        artifacts[1].1.bytes().len(),
        artifacts[2].1.bytes().len(),
        image_bytes.len(),
        pdb.len()
    );
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
    let mut symbols = None;
    let mut symbols_start = None;
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
            0xf1 => {
                symbols = Some(&data[start..end]);
                symbols_start = Some(start as u64);
            }
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

    let symbols = symbols.expect("contains CodeView symbol records");
    let symbols_start = symbols_start.expect("records the symbol-subsection offset");
    let mut symbol_cursor = 0;
    let mut procedure_offset = None;
    let mut procedure_segment = None;
    let mut symbol_kinds = Vec::new();
    while symbol_cursor < symbols.len() {
        let record_length = read_u16(symbols, symbol_cursor) as usize;
        let record_end = symbol_cursor + 2 + record_length;
        assert!(
            record_end <= symbols.len(),
            "CodeView symbol stays in bounds"
        );
        assert_eq!(record_end % 4, 0, "CodeView symbol is 4-byte aligned");
        let kind = read_u16(symbols, symbol_cursor + 2);
        symbol_kinds.push(kind);
        if kind == 0x1147 {
            assert!(
                read_u32(symbols, symbol_cursor + 16) > 0,
                "procedure records the code size"
            );
            assert_eq!(read_u32(symbols, symbol_cursor + 28), 0x1002);
            let name_start = symbol_cursor + 39;
            let name_end = symbols[name_start..record_end]
                .iter()
                .position(|byte| *byte == 0)
                .map(|offset| name_start + offset)
                .expect("procedure name is null terminated");
            assert_eq!(
                std::str::from_utf8(&symbols[name_start..name_end])
                    .expect("procedure name is UTF-8"),
                function
            );
            procedure_offset = Some(symbols_start + symbol_cursor as u64 + 32);
            procedure_segment = Some(symbols_start + symbol_cursor as u64 + 36);
        }
        symbol_cursor = record_end;
    }
    assert_eq!(symbol_cursor, symbols.len());
    for expected in [0x1101, 0x113c, 0x1147, 0x1012, 0x114f, 0x114c] {
        assert!(symbol_kinds.contains(&expected));
    }

    let types = object
        .section_by_name(".debug$T")
        .expect("source-mapped COFF object contains .debug$T")
        .data()
        .expect("reads .debug$T data");
    assert_eq!(read_u32(types, 0), 4, "uses the CodeView C13 signature");
    let mut type_cursor = 4;
    let mut type_kinds = Vec::new();
    while type_cursor < types.len() {
        let record_length = read_u16(types, type_cursor) as usize;
        let record_end = type_cursor + 2 + record_length;
        assert!(record_end <= types.len(), "CodeView type stays in bounds");
        assert_eq!(record_end % 4, 0, "CodeView type is 4-byte aligned");
        type_kinds.push(read_u16(types, type_cursor + 2));
        type_cursor = record_end;
    }
    assert_eq!(type_cursor, types.len());
    assert_eq!(&type_kinds[..3], &[0x1201, 0x1008, 0x1601]);
    assert_eq!(type_kinds.iter().filter(|kind| **kind == 0x1605).count(), 5);
    assert_eq!(type_kinds.last(), Some(&0x1603));
    assert!(
        types
            .windows(function.len())
            .any(|window| window == function.as_bytes()),
        "CodeView types contain the function name"
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
    assert_eq!(read_u32(lines, line_start), 0, "maps the function prologue");
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
    let procedure_offset = procedure_offset.expect("records the procedure offset relocation");
    let procedure_segment = procedure_segment.expect("records the procedure segment relocation");
    let mut relocations = section.relocations().collect::<Vec<_>>();
    relocations.sort_by_key(|(offset, _)| *offset);
    assert_eq!(relocations.len(), 4);
    for ((offset, relocation), (expected_offset, expected_kind, expected_size)) in
        relocations.iter().zip([
            (procedure_offset, RelocationKind::SectionOffset, 32),
            (procedure_segment, RelocationKind::SectionIndex, 16),
            (lines_start, RelocationKind::SectionOffset, 32),
            (lines_start + 4, RelocationKind::SectionIndex, 16),
        ])
    {
        assert_eq!(*offset, expected_offset);
        assert_eq!(relocation.kind(), expected_kind);
        assert_eq!(relocation.size(), expected_size);
    }
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
#[cfg(target_os = "macos")]
use std::ffi::OsString;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::fs;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct TestDirectory {
    path: PathBuf,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl TestDirectory {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lang-windows-debug-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        fs::create_dir(&path).expect("creates an isolated Windows debug test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(target_os = "windows")]
fn windows_debug_linker() -> Command {
    Command::new("link.exe")
}

#[cfg(target_os = "macos")]
fn windows_debug_linker() -> Command {
    let rustc = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .expect("queries the Rust sysroot");
    assert!(rustc.status.success());
    let sysroot = PathBuf::from(
        String::from_utf8(rustc.stdout)
            .expect("the Rust sysroot is UTF-8")
            .trim(),
    );
    let rustlib = sysroot.join("lib/rustlib");
    let linker = fs::read_dir(&rustlib)
        .expect("reads the Rust target tools")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("bin/gcc-ld/lld-link"))
        .find(|candidate| candidate.is_file())
        .expect("the pinned Rust toolchain contains lld-link");
    let mut command = Command::new(linker);
    let mut library_paths = vec![sysroot.join("lib")];
    if let Some(existing) = std::env::var_os("DYLD_LIBRARY_PATH") {
        library_paths.extend(std::env::split_paths(&existing));
    }
    command.env(
        "DYLD_LIBRARY_PATH",
        std::env::join_paths(library_paths).expect("joins dynamic-library paths"),
    );
    command
}

#[cfg(target_os = "macos")]
fn assert_lldb_resolves_pdb_lines(image: &Path) {
    let linked_directory =
        fs::canonicalize(image.parent().expect("linked image has a parent directory"))
            .expect("canonicalizes the linked image directory");
    let source_map_command = format!(
        "settings set target.source-map {} {}",
        linked_directory.display(),
        env!("CARGO_MANIFEST_DIR")
    );
    let source_command = "source list -f native-debug.mir -l 1";
    let output = Command::new("xcrun")
        .args([
            OsString::from("lldb"),
            OsString::from("--batch"),
            OsString::from("-o"),
            OsString::from(format!("target create {}", image.display())),
            OsString::from("-o"),
            OsString::from(source_map_command),
            OsString::from("-o"),
            OsString::from("image lookup -n prototype_add"),
            OsString::from("-o"),
            OsString::from("image lookup -n prototype_min"),
            OsString::from("-o"),
            OsString::from("image lookup -n prototype_host_callback"),
            OsString::from("-o"),
            OsString::from(source_command),
        ])
        .output()
        .expect("runs LLDB against the generated PDB");
    assert!(
        output.status.success(),
        "LLDB PDB inspection failed with status {}:\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).expect("LLDB output is UTF-8");
    for (function, line) in [
        ("prototype_add", 2),
        ("prototype_min", 9),
        ("prototype_host_callback", 17),
    ] {
        assert!(
            output
                .lines()
                .any(|row| row.contains(function)
                    && row.contains(&format!("native-debug.mir:{line}"))),
            "LLDB did not resolve {function} to source line {line}:\n{output}"
        );
    }
    assert!(output.contains("fn prototype_add"));
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
