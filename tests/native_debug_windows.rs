#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
use object::{Object as _, ObjectSection as _, ObjectSymbol as _};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_bounded(command: &mut Command, logs: &Path, phase: &str) -> Output {
    let stdout = logs.join(format!("{phase}-stdout.log"));
    let stderr = logs.join(format!("{phase}-stderr.log"));
    // Files keep a verbose tool from blocking on an undrained pipe while the
    // parent polls. Each fixed, trusted metadata probe must finish promptly.
    command.stdout(Stdio::from(fs::File::create(&stdout).unwrap()));
    command.stderr(Stdio::from(fs::File::create(&stderr).unwrap()));
    println!("Native debug stage {phase}: {command:?}");
    let mut child = command.spawn().unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            println!(
                "Native debug stage {phase}: {status}, {:.2}s",
                start.elapsed().as_secs_f64()
            );
            return Output {
                status,
                stdout: fs::read(stdout).unwrap(),
                stderr: fs::read(stderr).unwrap(),
            };
        }
        if start.elapsed() >= Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            let out = fs::read(stdout).unwrap();
            let err = fs::read(stderr).unwrap();
            panic!(
                "native debug stage {phase} exceeded 30s:\n{}\n{}",
                String::from_utf8_lossy(&out[out.len().saturating_sub(4096)..]),
                String::from_utf8_lossy(&err[err.len().saturating_sub(4096)..])
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn source_copy_owned_and_c_units_link_source_locations_into_windows_pdbs() {
    let root = Directory(
        std::env::temp_dir().join(format!("lang source CodeView {}", std::process::id())),
    );
    fs::create_dir(&root.0).unwrap();
    let cases = [
        (
            "copy",
            "fn increment(n:i64)->i64{return n+1;}\nfn main()->i64{return increment(41);}",
            ["increment", "main"],
        ),
        (
            "owned",
            "fn make(n:i64)->core::Bytes { let mut b:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator())); let mut p:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(b,n)); return b; }\nfn main()->i64 { let mut b:core::Bytes=make(41); return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(b,0))+1; }",
            ["make", "main"],
        ),
        (
            "c",
            "fn add(n:i64)->i64{return n+1;}\npub extern \"C\" fn entry(n:i64)->i64 { let mut b:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator())); let mut p:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(b,n)); return add(core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(b,0))); }",
            ["add", "entry"],
        ),
    ];
    for (profile, source, names) in cases {
        let input = root.0.join(format!("{profile}.lang"));
        fs::write(
            &input,
            format!("// é 😀\r\n{}\r\n", source.replace('\n', "\r\n")),
        )
        .unwrap();
        let package = root.0.join(profile);
        let emit = run_bounded(
            Command::new(env!("CARGO_BIN_EXE_lang"))
                .args(["native", "--target", "x86_64-pc-windows-msvc", "--output"])
                .arg(&package)
                .arg(&input),
            &root.0,
            &format!("{profile}-emit"),
        );
        assert!(
            emit.status.success(),
            "{}",
            String::from_utf8_lossy(&emit.stderr)
        );
        let bytes = fs::read(package.join("program.obj")).unwrap();
        let object = object::File::parse(bytes.as_slice()).unwrap();
        verify_unwind(&object);
        let debug = object.section_by_name(".debug$S").unwrap();
        assert!(object.section_by_name(".debug$T").is_some());
        let mut symbols: Vec<_> = debug
            .relocations()
            .map(|(_, r)| {
                let object::RelocationTarget::Symbol(index) = r.target() else {
                    panic!("source relocation does not name a native function")
                };
                object
                    .symbol_by_index(index)
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(symbols.len(), names.len() * 4);
        symbols.sort();
        symbols.dedup();
        assert_eq!(symbols.len(), names.len());
        let mut linker = windows_linker();
        linker.current_dir(&package).args([
            "/dll",
            "/noentry",
            "/nodefaultlib",
            "/debug:full",
            "/force:unresolved",
            "/out:source.dll",
            "/pdb:source.pdb",
            "program.obj",
        ]);
        for symbol in &symbols {
            linker.arg(format!("/export:{symbol}"));
        }
        // This image is for metadata inspection only. Actual execution with
        // resolved SDK imports and debugger stepping belongs to G03.
        let link = run_bounded(&mut linker, &root.0, &format!("{profile}-link"));
        assert!(
            link.status.success(),
            "{}{}",
            String::from_utf8_lossy(&link.stdout),
            String::from_utf8_lossy(&link.stderr)
        );
        let image = package.join("source.dll");
        let image_bytes = fs::read(&image).unwrap();
        let pe = object::File::parse(image_bytes.as_slice()).unwrap();
        let reference = pe.pdb_info().unwrap().unwrap();
        assert!(reference.path().ends_with(b"source.pdb"));
        assert_ne!(reference.guid(), [0; 16]);
        let pdb = fs::read(package.join("source.pdb")).unwrap();
        assert!(pdb.starts_with(b"Microsoft C/C++ MSF 7.00"));
        #[cfg(target_os = "macos")]
        {
            let mut reader = Command::new("xcrun");
            reader.args(["lldb", "--batch"]).arg(&image);
            for name in names {
                reader.args(["-o", &format!("image lookup -n {name}")]);
            }
            let result = run_bounded(&mut reader, &root.0, &format!("{profile}-read"));
            let output = String::from_utf8(result.stdout).unwrap();
            assert!(
                result.status.success(),
                "{output}{}",
                String::from_utf8_lossy(&result.stderr)
            );
            for (name, line) in names.into_iter().zip([2, 3]) {
                assert!(
                    output
                        .lines()
                        .any(|row| row.contains(&format!("`{name} at"))
                            && row.contains(&format!("{profile}.lang:{line}"))),
                    "{output}"
                );
            }
        }
        #[cfg(target_os = "windows")]
        inspect_native_pdb(&package.join("source.pdb"), profile, names);
    }
}

fn verify_unwind(object: &object::File<'_>) {
    let pdata = object
        .section_by_name(".pdata")
        .expect("native call frames require .pdata");
    let xdata = object
        .section_by_name(".xdata")
        .expect("native prologues require .xdata");
    let entries = pdata.data().unwrap();
    let info = xdata.data().unwrap();
    assert!(!entries.is_empty());
    assert_eq!(entries.len() % 12, 0);
    let relocations: Vec<_> = pdata.relocations().collect();
    assert_eq!(relocations.len(), entries.len() / 4);
    let address = |at: usize| {
        let (_, relocation) = relocations
            .iter()
            .find(|(offset, _)| *offset == at as u64)
            .unwrap();
        assert_eq!(relocation.kind(), object::RelocationKind::ImageOffset);
        assert_eq!(relocation.size(), 32);
        let object::RelocationTarget::Symbol(index) = relocation.target() else {
            panic!("unwind target must name a symbol")
        };
        let symbol = object.symbol_by_index(index).unwrap();
        let addend = u32::from_le_bytes(entries[at..at + 4].try_into().unwrap());
        (
            symbol.section_index().unwrap(),
            symbol.address() + u64::from(addend),
        )
    };
    for at in (0..entries.len()).step_by(12) {
        let (section, begin) = address(at);
        let (end_section, end) = address(at + 4);
        assert_eq!(section, end_section);
        let text = object.section_by_index(section).unwrap();
        assert_eq!(text.kind(), object::SectionKind::Text);
        assert!(begin < end && end <= text.address() + text.size());
        let (section, offset) = address(at + 8);
        assert_eq!(section, xdata.index());
        let offset = (offset - xdata.address()) as usize;
        assert_eq!(offset % 4, 0);
        assert_eq!(info[offset] & 7, 1); // UNWIND_INFO version 1
        assert_eq!(info[offset] >> 3, 0); // no fabricated SEH/chained handler
        assert!(offset + 4 + 2 * usize::from(info[offset + 2]) <= info.len());
    }
}

#[cfg(target_os = "windows")]
fn inspect_native_pdb(pdb: &std::path::Path, profile: &str, names: [&str; 2]) {
    let unwind = run_bounded(
        Command::new(r"C:\Program Files\LLVM\bin\llvm-readobj.exe")
            .arg("--unwind")
            .arg(pdb.parent().unwrap().join("program.obj")),
        pdb.parent().unwrap(),
        &format!("{profile}-unwind-read"),
    );
    assert!(
        unwind.status.success(),
        "{}{}",
        String::from_utf8_lossy(&unwind.stdout),
        String::from_utf8_lossy(&unwind.stderr)
    );
    assert!(String::from_utf8_lossy(&unwind.stdout).contains("UnwindInfo"));
    let tool = PathBuf::from(r"C:\Program Files\LLVM\bin\llvm-pdbutil.exe");
    let mut command = if tool.is_file() {
        Command::new(tool)
    } else {
        Command::new("llvm-pdbutil")
    };
    let dump = run_bounded(
        command.args(["dump", "-l", "-symbols"]).arg(pdb),
        pdb.parent().unwrap(),
        &format!("{profile}-read"),
    );
    let output = String::from_utf8(dump.stdout).unwrap();
    assert!(
        dump.status.success(),
        "{output}{}",
        String::from_utf8_lossy(&dump.stderr)
    );
    assert!(output.contains(&format!("{profile}.lang")), "{output}");
    for name in names {
        assert!(output.contains(&format!("`{name}`")), "{output}");
    }
    assert_eq!(
        output.matches("line/column/addr entries =").count(),
        2,
        "{output}"
    );
    // llvm-pdbutil prints each line number followed by an eight-digit hex RVA.
    let words: Vec<_> = output.split_whitespace().collect();
    for line in ["2", "3"] {
        assert!(
            words.windows(2).any(|p| p[0] == line
                && p[1].len() == 8
                && p[1].bytes().all(|c| c.is_ascii_hexdigit())),
            "{output}"
        );
    }
    println!("Native Windows PDB source locations ({profile}):\n{output}");
}

#[cfg(target_os = "windows")]
fn windows_linker() -> Command {
    Command::new("link.exe")
}

#[cfg(target_os = "macos")]
fn windows_linker() -> Command {
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let sysroot = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    let linker = fs::read_dir(sysroot.join("lib/rustlib"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path().join("bin/gcc-ld/lld-link"))
        .find(|p| p.is_file())
        .expect("pinned Rust toolchain has lld-link");
    let mut command = Command::new(linker);
    let mut paths = vec![sysroot.join("lib")];
    if let Some(existing) = std::env::var_os("DYLD_LIBRARY_PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    command.env("DYLD_LIBRARY_PATH", std::env::join_paths(paths).unwrap());
    command
}
