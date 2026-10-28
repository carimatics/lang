use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "lang-artifact-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn source(&self, name: &str, text: &str) -> PathBuf {
        let p = self.file(name);
        std::fs::write(&p, text).unwrap();
        p
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn cli(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lang"))
        .args(args)
        .output()
        .unwrap()
}
fn compile(source: &Path, output: &Path) -> Output {
    cli(&[
        "compile".as_ref(),
        "--output".as_ref(),
        output.as_os_str(),
        source.as_os_str(),
    ])
}
fn run_source(source: &Path) -> Output {
    cli(&["run".as_ref(), source.as_os_str()])
}
fn run_loaded(path: &Path) -> Output {
    cli(&["run".as_ref(), "--artifact".as_ref(), path.as_os_str()])
}
fn ok(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn same(a: &Output, b: &Output) {
    assert_eq!(a.status.code(), b.status.code());
    assert_eq!(a.stdout, b.stdout);
    assert_eq!(
        String::from_utf8_lossy(&a.stderr),
        String::from_utf8_lossy(&b.stderr)
    );
}
const BYTES: &str = include_str!("../examples/scripts/owned_bytes.lang");

#[test]
fn compile_emits_explicit_copy_or_owned_versions_and_executes_their_entries() {
    let dir = Directory::new();
    for (i, text, version, result) in [
        (0, "fn main()->i64{return 42;}", 1, "42\n"),
        (1, BYTES, 2, "0\n"),
    ] {
        let source = dir.source(&format!("main{i}.lang"), text);
        let artifact = dir.file(&format!("main{i}.lbc"));
        let compiled = compile(&source, &artifact);
        ok(&compiled);
        assert!(compiled.stdout.is_empty());
        let bytes = std::fs::read(&artifact).unwrap();
        assert_eq!(&bytes[..8], b"LANGBC\0\0");
        assert_eq!(
            u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
            version
        );
        let direct = run_source(&source);
        let loaded = run_loaded(&artifact);
        ok(&loaded);
        same(&direct, &loaded);
        assert_eq!(String::from_utf8_lossy(&loaded.stdout), result);
        std::fs::remove_file(&source).unwrap();
        same(&loaded, &run_loaded(&artifact));
    }
}

#[test]
fn original_snapshots_and_unicode_call_sites_survive_source_removal() {
    let dir = Directory::new();
    let source=dir.source("snapshot.lang",r#"// 日本語のsnapshot
fn fail()->i64 {
    let mut bytes:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut none:core::Option<i64>=core::Option::None;
    return core::option_unwrap::<i64>(none);
}
fn main()->i64 {return fail();}
"#);
    let artifact = dir.file("snapshot.lbc");
    ok(&compile(&source, &artifact));
    let direct = run_source(&source);
    assert_eq!(direct.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&direct.stderr);
    assert!(stderr.contains("snapshot.lang"));
    assert!(stderr.contains("at fail"));
    assert!(stderr.contains("at main"));
    std::fs::remove_file(&source).unwrap();
    same(&direct, &run_loaded(&artifact));
}

#[test]
fn malformed_binary_and_cli_options_fail_without_source_fallback() {
    let dir = Directory::new();
    let source = dir.source("main.lang", BYTES);
    let artifact = dir.file("main.lbc");
    ok(&compile(&source, &artifact));
    let valid = std::fs::read(&artifact).unwrap();
    for (i, bad) in [
        b"fn main()->i64{return 42;}".to_vec(),
        valid[..15].to_vec(),
        {
            let mut b = valid.clone();
            b[8] = 99;
            b
        },
        {
            let mut b = valid.clone();
            b[10] = 1;
            b
        },
    ]
    .into_iter()
    .enumerate()
    {
        let path = dir.file(&format!("bad{i}.lbc"));
        std::fs::write(&path, bad).unwrap();
        let out = run_loaded(&path);
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
    for args in [
        vec![
            "run".as_ref(),
            "--artifact".as_ref(),
            "--source-root".as_ref(),
            dir.0.as_os_str(),
            artifact.as_os_str(),
        ],
        vec![
            "run".as_ref(),
            "--artifact".as_ref(),
            "--artifact".as_ref(),
            artifact.as_os_str(),
        ],
        vec!["compile".as_ref(), source.as_os_str()],
        vec![
            "compile".as_ref(),
            "--fuel".as_ref(),
            "1".as_ref(),
            source.as_os_str(),
        ],
    ] {
        let out = cli(&args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    let plain = cli(&["run".as_ref(), artifact.as_os_str()]);
    assert_eq!(plain.status.code(), Some(1));
    assert!(plain.stdout.is_empty());
    let library = dir.file("library.lbc");
    let lang::artifact::Program::Ownership(program) =
        lang::artifact::decode(&valid, Default::default()).unwrap()
    else {
        panic!("owned");
    };
    let (raw, _) = program.into_parts();
    std::fs::write(
        &library,
        lang_bytecode::ownership::encode(raw, None, Default::default()).unwrap(),
    )
    .unwrap();
    let out = run_loaded(&library);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no script entry"));
}

#[test]
fn compilation_preserves_output_and_inputs_on_diagnostics_and_publication_failure() {
    let dir = Directory::new();
    let output = dir.file("kept.lbc");
    std::fs::write(&output, b"kept").unwrap();
    for (i, text, code) in [
        (0, "fn main()->i64{return true;}", "SRC0005"),
        (1, "fn helper()->i64{return 0;}", "SRC0018"),
        (
            2,
            "fn main()->bool{let mut bytes:core::Result<core::Bytes,core::AllocError>=core::bytes_new(core::allocator());return true;}",
            "SRC0018",
        ),
    ] {
        let source = dir.source(&format!("invalid{i}.lang"), text);
        let result = compile(&source, &output);
        assert_eq!(result.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(code),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(std::fs::read(&output).unwrap(), b"kept");
    }
    let source = dir.source("valid.lang", BYTES);
    let result = compile(&source, &source);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&source).unwrap(), BYTES);
    let directory = dir.file("directory");
    std::fs::create_dir(&directory).unwrap();
    assert_eq!(compile(&source, &directory).status.code(), Some(1));
    assert!(!std::fs::read_dir(&dir.0).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lang-")
    }));
    let missing = dir.file("missing/child.lbc");
    assert_eq!(compile(&source, &missing).status.code(), Some(1));
    assert!(!missing.exists());
    ok(&compile(&source, &output));
    ok(&run_loaded(&output));
}

#[test]
fn loaded_limits_match_source_and_initial_binary_input_is_bounded() {
    let dir = Directory::new();
    let source = dir.source("main.lang", BYTES);
    let artifact = dir.file("main.lbc");
    ok(&compile(&source, &artifact));
    for flag in ["--fuel", "--frames", "--cells"] {
        let direct = cli(&[
            "run".as_ref(),
            flag.as_ref(),
            "0".as_ref(),
            source.as_os_str(),
        ]);
        let loaded = cli(&[
            "run".as_ref(),
            "--artifact".as_ref(),
            flag.as_ref(),
            "0".as_ref(),
            artifact.as_os_str(),
        ]);
        assert_eq!(loaded.status.code(), Some(1));
        same(&direct, &loaded);
    }
    let huge = dir.file("huge.lbc");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    let out = run_loaded(&huge);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("exceeds 67108864 bytes"));
}

#[test]
fn compilation_cannot_replace_any_loaded_module_with_its_artifact() {
    let dir = Directory::new();
    std::fs::create_dir(dir.file("helper")).unwrap();
    let root_text = "mod helper;fn main()->i64{return helper::answer();}";
    let helper_text = "mod nested;pub fn answer()->i64{return nested::value();}";
    let nested_text = "pub fn value()->i64{return 42;}";
    let root = dir.source("main.lang", root_text);
    let helper = dir.source("helper.lang", helper_text);
    let nested = dir.source("helper/nested.lang", nested_text);
    for output in [&helper, &nested, &root] {
        let result = compile(&root, output);
        assert_eq!(result.status.code(), Some(1), "{output:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr)
                .contains("artifact output must differ from every source input"),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        for (path, expected) in [
            (&root, root_text),
            (&helper, helper_text),
            (&nested, nested_text),
        ] {
            assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
        }
    }
    #[cfg(unix)]
    {
        let alias = dir.file("module-alias.lbc");
        std::os::unix::fs::symlink(&nested, &alias).unwrap();
        assert_eq!(compile(&root, &alias).status.code(), Some(1));
        assert!(std::fs::symlink_metadata(&alias).unwrap().is_symlink());
        assert_eq!(std::fs::read_to_string(&nested).unwrap(), nested_text);
    }
    let artifact = dir.file("program.lbc");
    ok(&compile(&root, &artifact));
    ok(&compile(&root, &artifact));
    same(&run_source(&root), &run_loaded(&artifact));
}

#[test]
fn existing_owned_service_fixtures_agree_through_source_and_loaded_cli_execution() {
    let dir = Directory::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/scripts");
    for name in [
        "text",
        "utf8",
        "owned_calls",
        "cleanup_calls",
        "owned_replace",
        "owned_aggregate",
        "arena",
        "arena_owned",
        "shared",
        "shared_owned",
        "vec",
        "vec_owned",
        "map",
        "map_owned",
        "nested_collections",
        "nested_cycle",
        "nested_arena",
        "nested_mutation",
    ] {
        let source = root.join(format!("{name}.lang"));
        let artifact = dir.file(&format!("{name}.lbc"));
        ok(&compile(&source, &artifact));
        same(&run_source(&source), &run_loaded(&artifact));
    }
}

#[test]
fn loaded_cli_uses_an_explicit_non_main_entry_for_both_execution_adapters() {
    let dir = Directory::new();
    for (i, expression) in ["7", "leaf()"].into_iter().enumerate() {
        let source = format!(
            "fn main()->i64{{return 42;}}fn selected()->i64{{let mut bytes:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));return {expression};}}fn leaf()->i64{{return 7;}}"
        );
        let analysis = lang::frontend::analyze_virtual(
            lang_diagnostics::SourceMap::new(),
            Path::new("entry.lang"),
            source.as_bytes(),
            None,
            Default::default(),
            None,
        );
        assert!(analysis.diagnostics.is_empty());
        let lang_semantics::analysis::Program::Ownership(program) = analysis.result.unwrap() else {
            panic!("owned");
        };
        let raw = program
            .into_failure_flow()
            .into_source()
            .into_activity()
            .into_moved()
            .into_typed()
            .into_table();
        let entry = lang_mir::unit::FunctionId(
            raw.places
                .declarations
                .functions
                .iter()
                .position(|f| f.name == "selected")
                .unwrap() as u32,
        );
        let bytes = lang_bytecode::ownership::encode(raw, Some(entry), Default::default()).unwrap();
        let artifact = dir.file(&format!("entry{i}.lbc"));
        std::fs::write(&artifact, bytes).unwrap();
        let out = run_loaded(&artifact);
        ok(&out);
        assert_eq!(out.stdout, b"7\n");
    }
}

#[test]
fn multi_file_copy_and_owned_artifacts_keep_embedded_module_snapshots() {
    let dir = Directory::new();
    for (i, body, result) in [
        (0, "pub fn answer()->i64{return 43;}", "43\n"),
        (
            1,
            "pub fn answer()->i64{let mut bytes:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));return core::bytes_len(bytes);}",
            "0\n",
        ),
    ] {
        let source = dir.source(
            "main.lang",
            "mod helper;fn main()->i64{return helper::answer();}",
        );
        let helper = dir.source("helper.lang", body);
        let artifact = dir.file(&format!("modules{i}.lbc"));
        let out = cli(&[
            "compile".as_ref(),
            "--source-root".as_ref(),
            dir.0.as_os_str(),
            "--output".as_ref(),
            artifact.as_os_str(),
            source.as_os_str(),
        ]);
        ok(&out);
        let direct = run_source(&source);
        ok(&direct);
        assert_eq!(String::from_utf8_lossy(&direct.stdout), result);
        std::fs::remove_file(source).unwrap();
        std::fs::remove_file(helper).unwrap();
        same(&direct, &run_loaded(&artifact));
    }
}

#[test]
fn explicit_panic_schema_three_preserves_dynamic_code_stack_and_source_snapshot() {
    let dir = Directory::new();
    for (name, text, code) in [
        (
            "panic",
            include_str!("../examples/analysis/explicit_panic.lang"),
            42,
        ),
        (
            "panic-owned",
            include_str!("../examples/analysis/explicit_panic_owned.lang"),
            -42,
        ),
    ] {
        let source = dir.source(&format!("{name}.lang"), text);
        let artifact = dir.file(&format!("{name}.lbc"));
        ok(&compile(&source, &artifact));
        let bytes = std::fs::read(&artifact).unwrap();
        assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 3);
        let direct = run_source(&source);
        assert_eq!(direct.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&direct.stderr);
        assert!(stderr.contains(&format!("panic code {code}")), "{stderr}");
        assert!(
            stderr.contains("at fail") && stderr.contains("at main"),
            "{stderr}"
        );
        std::fs::remove_file(&source).unwrap();
        same(&direct, &run_loaded(&artifact));
        let loaded = lang::artifact::decode(&bytes, Default::default()).unwrap();
        let lang::artifact::Program::Ownership(loaded) = loaded else {
            panic!("ownership schema 3");
        };
        let (raw, entry) = loaded.into_parts();
        assert_eq!(
            lang_bytecode::ownership::encode(raw, entry, Default::default()).unwrap(),
            bytes
        );
    }
}

#[test]
fn c_aware_schema_four_loads_without_source_or_implicit_c_authority() {
    let dir = Directory::new();
    let source = dir.source(
        "c-caller.lang",
        include_str!("../examples/analysis/c_scalar_imports.lang"),
    );
    let artifact = dir.file("c-caller.lbc");
    let direct = run_source(&source);
    assert!(!direct.status.success());
    ok(&compile(&source, &artifact));
    let bytes = std::fs::read(&artifact).unwrap();
    assert_eq!(&bytes[8..10], &4u16.to_le_bytes());
    std::fs::remove_file(&source).unwrap();
    let loaded = lang::artifact::decode(&bytes, Default::default()).unwrap();
    let lang::artifact::Program::Ownership(loaded) = loaded else {
        panic!("C-aware ownership artifact");
    };
    let (raw, _) = loaded.into_parts();
    assert!(
        raw.places
            .declarations
            .imports
            .iter()
            .all(|i| i.abi == lang_mir::unit::ownership::declarations::ImportAbi::C)
    );
    let output = run_loaded(&artifact);
    same(&direct, &output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("UnsupportedCInterface"), "{stderr}");
    assert!(stderr.contains("fuel 0, host calls 0"), "{stderr}");
    assert!(stderr.contains("service work 0"), "{stderr}");
}

#[test]
fn task_artifact_survives_source_deletion_and_retains_runtime_controls() {
    let directory = Directory::new();
    let source = directory.source("tasks.lang", include_str!("../examples/scripts/tasks.lang"));
    let artifact = directory.file("tasks.lbc");
    let source_result = run_source(&source);
    ok(&source_result);
    ok(&compile(&source, &artifact));
    assert_eq!(
        &std::fs::read(&artifact).unwrap()[8..10],
        &5u16.to_le_bytes()
    );
    std::fs::remove_file(&source).unwrap();
    let loaded = run_loaded(&artifact);
    ok(&loaded);
    same(&source_result, &loaded);
    let limited = cli(&[
        "run".as_ref(),
        "--artifact".as_ref(),
        "--fuel".as_ref(),
        "0".as_ref(),
        artifact.as_os_str(),
    ]);
    assert!(!limited.status.success());
    assert!(String::from_utf8_lossy(&limited.stderr).contains("fuel"));
}

#[test]
fn valid_component_artifact_loads_without_source_but_requires_explicit_host_bindings() {
    let directory = Directory::new();
    let bytes: Vec<u8> = include_str!("fixtures/component_artifact_v5.hex")
        .split_whitespace()
        .map(|s| u8::from_str_radix(s, 16).unwrap())
        .collect();
    let artifact = directory.file("component.lbc");
    std::fs::write(&artifact, &bytes).unwrap();
    let result = run_loaded(&artifact);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("component artifacts require explicit host bindings")
    );
    assert!(result.stdout.is_empty());
}
