#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
use object::{Object as _, ObjectSection as _, ObjectSymbol as _};
use std::{fs, path::PathBuf, process::Command};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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
        let emit = Command::new(env!("CARGO_BIN_EXE_lang"))
            .args(["native", "--target", "x86_64-pc-windows-msvc", "--output"])
            .arg(&package)
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            emit.status.success(),
            "{}",
            String::from_utf8_lossy(&emit.stderr)
        );
        let bytes = fs::read(package.join("program.obj")).unwrap();
        let object = object::File::parse(bytes.as_slice()).unwrap();
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
        let link = linker.output().unwrap();
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
            let result = reader.output().unwrap();
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

#[cfg(target_os = "windows")]
fn inspect_native_pdb(pdb: &std::path::Path, profile: &str, names: [&str; 2]) {
    let tool = PathBuf::from(r"C:\Program Files\LLVM\bin\llvm-pdbutil.exe");
    let mut command = if tool.is_file() {
        Command::new(tool)
    } else {
        Command::new("llvm-pdbutil")
    };
    let dump = command
        .args(["dump", "-l", "-symbols"])
        .arg(pdb)
        .output()
        .expect("LLVM PDB reader must be installed for native metadata validation");
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
