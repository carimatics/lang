//! Standalone executable conformance reuses the existing owned runtime hooks.
#[cfg(test)]
#[path = "rust_artifacts.rs"]
mod artifacts;
use super::*;
use std::{cell::Cell, fs, path::PathBuf, process::Command};

/// # Safety
/// The executable must link exactly the supplied object from this source and
/// toolchain, with its private functions available in the process symbol table.
pub(crate) unsafe fn run(text: &str, original: &[u8]) {
    let source = source(text);
    let flow = source.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = n::emit_object_for_target(
        flow,
        &layouts,
        &lang_codegen_clif::Triple::host(),
        Default::default(),
        1000,
    )
    .unwrap();
    assert_eq!(
        artifact.bytes, original,
        "driver metadata must match the linked object exactly"
    );
    let dropped = Rc::new(Cell::new(false));
    let image = linked::Image::current_executable(dropped.clone());
    // SAFETY: the current executable links the exact compared object; it owns
    // its code until process exit and exposes the private symbols to dlsym.
    let program =
        unsafe { artifact.bind(image, linked::Image::symbol, Default::default()) }.unwrap();
    let base = e::Control::with_limits(control::limits());
    let actual = execute(flow, &layouts, &program, base, None, false, None);
    let (expected, requests) = control::vm(flow, base, None);
    control::same(&actual, &expected);
    assert_eq!(actual.total_requests, requests);
    let normal = actual;
    // Compare every work/fuel prefix and each provider refusal/interruption.
    for work in 0..=normal.usage.service_work + 1 {
        compare(flow, &layouts, &program, base.and_service_work(Some(work)));
    }
    for fuel in 0..=normal.usage.fuel + 1 {
        let mut limits = control::limits();
        limits.fuel = Some(fuel);
        compare(flow, &layouts, &program, e::Control::with_limits(limits));
    }
    for frames in [
        0,
        normal.usage.peak_frames.saturating_sub(1),
        normal.usage.peak_frames,
    ] {
        let mut limits = control::limits();
        limits.frames = Some(frames);
        compare(flow, &layouts, &program, e::Control::with_limits(limits));
    }
    for cells in [
        0,
        normal.usage.peak_live_cells.saturating_sub(1),
        normal.usage.peak_live_cells,
    ] {
        let mut limits = control::limits();
        limits.live_cells = Some(cells);
        compare(flow, &layouts, &program, e::Control::with_limits(limits));
    }
    compare(
        flow,
        &layouts,
        &program,
        base.and_deadline(std::time::Instant::now() - std::time::Duration::from_secs(1)),
    );
    for request in 1..=normal.requests {
        let actual = execute(flow, &layouts, &program, base, Some(request), false, None);
        let absolute = normal.total_requests - normal.requests + request;
        let (expected, requests) = control::vm_with_refusal(flow, base, None, Some(absolute));
        control::same(&actual, &expected);
        assert_eq!(actual.total_requests, requests);
    }
    for request in 1..=normal.total_requests {
        let native_flag = Rc::new(e::InterruptionFlag::new());
        let vm_flag = Rc::new(e::InterruptionFlag::new());
        let actual = execute(
            flow,
            &layouts,
            &program,
            base.and_interruption(&native_flag),
            None,
            false,
            Some((request, native_flag.clone())),
        );
        let (expected, requests) = control::vm(
            flow,
            base.and_interruption(&vm_flag),
            Some((request, vm_flag.clone())),
        );
        control::same(&actual, &expected);
        assert_eq!(actual.total_requests, requests);
    }
    let failed = execute(flow, &layouts, &program, base, None, true, None);
    let refused = failed
        .report
        .primary()
        .into_iter()
        .chain(failed.report.secondary())
        .any(|record| record.kind == c::FailureKind::Cleanup(95));
    if refused {
        assert!(failed.scalar.is_none());
    } else {
        // Empty/Copy-only retirement has no leaf callback to refuse.
        assert_eq!(failed.scalar, normal.scalar);
        assert_eq!(failed.usage, normal.usage);
        assert_eq!(failed.report.primary(), normal.report.primary());
    }
    assert!(!dropped.get());
    drop(program);
    assert!(dropped.get());
}

fn execute<'l>(
    flow: &'l m::ownership::failure::Elaborated,
    layouts: &'l l::Layouts<'l>,
    program: &n::LinkedProgram<'l, linked::Image>,
    control: e::Control<'_>,
    refuse: Option<usize>,
    refuse_cleanup: bool,
    interrupt: Option<(usize, Rc<e::InterruptionFlag>)>,
) -> Seen {
    execute_controlled(
        flow,
        layouts,
        program,
        Run {
            inputs: &[],
            fuel: control.limits().fuel,
            max_frames: 8,
            cells: 1000,
            refuse,
            refuse_cleanup,
        },
        control,
        interrupt,
    )
}
fn compare<'l>(
    flow: &'l m::ownership::failure::Elaborated,
    layouts: &'l l::Layouts<'l>,
    program: &n::LinkedProgram<'l, linked::Image>,
    control: e::Control<'_>,
) {
    let actual = execute(flow, layouts, program, control, None, false, None);
    let (expected, requests) = control::vm(flow, control, None);
    control::same(&actual, &expected);
    assert_eq!(actual.total_requests, requests);
}

struct Files(PathBuf);
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn standalone_owned_object_links_and_runs_directly_and_via_response_file() {
    use lang::linker::{LinkRequest, SystemLinker};
    let files =
        Files(std::env::temp_dir().join(format!("lang owned static-{}", std::process::id())));
    fs::create_dir(&files.0).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dependencies = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let bootstrap = files.0.join("driver.rs");
    fs::write(&bootstrap, format!(r#"
#![allow(dead_code, unused_imports)]
#[path = {boundary:?}] mod boundary;
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lang_owned_static_driver(source: *const std::ffi::c_char, object: *const std::ffi::c_char) -> i32 {{
    let result = std::panic::catch_unwind(|| {{
        // SAFETY: the C startup passes its two live, NUL-terminated argv paths.
        let source = unsafe {{ std::ffi::CStr::from_ptr(source) }}.to_str().unwrap();
        let object = unsafe {{ std::ffi::CStr::from_ptr(object) }}.to_str().unwrap();
        let text = std::fs::read_to_string(source).unwrap();
        let bytes = std::fs::read(object).unwrap();
        // SAFETY: the private parent test linked this object into this exact
        // executable and supplied its matching source/object paths to startup.
        unsafe {{ boundary::run_static_owned_conformance(&text, &bytes); }}
    }});
    if result.is_ok() {{ 0 }} else {{ 1 }}
}}
"#, boundary=root.join("tests/native_owned_boundary.rs"))).unwrap();
    let archive = files.0.join(if cfg!(target_env = "msvc") {
        "runtime.lib"
    } else {
        "runtime.a"
    });
    let mut compiler = Command::new("rustc");
    artifacts::native_search_paths(&mut compiler);
    compiler
        .current_dir(&root)
        .args([
            "--edition=2024",
            "--crate-type=staticlib",
            "--print=native-static-libs",
        ])
        .arg(&bootstrap)
        .arg("-o")
        .arg(&archive)
        .arg("-L")
        .arg(format!("dependency={}", dependencies.display()));
    for name in [
        "lang",
        "lang_codegen_clif",
        "lang_diagnostics",
        "lang_mir",
        "lang_runtime",
        "lang_semantics",
        "lang_vm",
        "object",
    ] {
        let path = artifacts::library(name);
        compiler
            .arg("--extern")
            .arg(format!("{name}={}", path.display()));
    }
    let output = compiler.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let libraries = stderr
        .lines()
        .find_map(|line| {
            line.split_once("native-static-libs:")
                .map(|(_, value)| value)
        })
        .unwrap();
    let mut linker = SystemLinker::discover_host().unwrap();
    if cfg!(target_os = "macos") {
        linker.push_argument("-Wl,-export_dynamic");
    }
    if cfg!(target_env = "msvc") {
        // Cargo normally supplies this dependency's build-script search path.
        // The standalone rustc/link.exe pair must retain it explicitly; obtain
        // the resolved package location rather than assuming a registry layout.
        let metadata = Command::new(env!("CARGO"))
            .current_dir(&root)
            .args([
                "metadata",
                "--locked",
                "--offline",
                "--format-version=1",
                "--filter-platform=x86_64-pc-windows-msvc",
            ])
            .output()
            .unwrap();
        assert!(metadata.status.success(), "{metadata:?}");
        let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
        let package = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| package["name"] == "windows_x86_64_msvc")
            .unwrap();
        let directory = PathBuf::from(package["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap()
            .join("lib");
        assert!(directory.is_dir());
        let mut search = std::ffi::OsString::from("/LIBPATH:");
        search.push(directory);
        linker.push_argument(search);
    }
    for argument in libraries.split_whitespace() {
        linker.push_argument(argument);
    }
    let startup = files.0.join("startup.c");
    let startup_object = files.0.join(if cfg!(target_env = "msvc") {
        "startup.obj"
    } else {
        "startup.o"
    });
    fs::write(&startup, "extern int lang_owned_static_driver(const char*, const char*);\nint main(int argc, char** argv) { return argc == 3 ? lang_owned_static_driver(argv[1], argv[2]) : 2; }\n").unwrap();
    let output = if cfg!(target_env = "msvc") {
        let mut object = std::ffi::OsString::from("/Fo:");
        object.push(&startup_object);
        Command::new("cl.exe")
            .args(["/nologo", "/O2", "/MD", "/c"])
            .arg(&startup)
            .arg(object)
            .output()
            .unwrap()
    } else {
        Command::new("cc")
            .arg("-c")
            .arg(&startup)
            .arg("-o")
            .arg(&startup_object)
            .output()
            .unwrap()
    };
    assert!(output.status.success(), "{output:?}");
    let mut profiles = control::work_profiles();
    for (label, text) in [
        (
            "explicit-panic",
            include_str!("../../examples/analysis/explicit_panic.lang"),
        ),
        (
            "explicit-panic-owned",
            include_str!("../../examples/analysis/explicit_panic_owned.lang"),
        ),
    ] {
        profiles.push((label, text.to_owned()));
    }
    profiles.push((
        "owned-calls",
        include_str!("../../examples/scripts/owned_calls.lang").to_owned(),
    ));
    profiles.push((
        "guest-panic",
        include_str!("../../examples/scripts/shared.lang").replace(
            "return (*value);",
            "return core::option_unwrap::<i64>(core::Option::None);",
        ),
    ));
    let make = r#"
fn make_static(value: i64) -> core::Bytes {
    let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(bytes, value));
    return bytes;
}
"#;
    for (label, library, arguments) in [
        (
            "record-calls",
            include_str!("../../examples/analysis/native_owned_records.lang"),
            "make_static(65), make_static(66), make_static(67)",
        ),
        (
            "sum-calls",
            include_str!("../../examples/analysis/native_owned_sums.lang"),
            "make_static(65), make_static(66)",
        ),
        (
            "loan-calls",
            include_str!("../../examples/analysis/native_owned_loans.lang"),
            "make_static(65), make_static(66), make_static(67)",
        ),
        (
            "owner-boundary",
            include_str!("../../examples/analysis/native_owned_calls.lang"),
            "make_static(65), make_static(66), make_static(67)",
        ),
    ] {
        profiles.push((label, format!("{library}\n{make}\nfn main() -> i64 {{ let mut result: core::Bytes = relay({arguments}); return core::result_unwrap::<i64, core::BoundsError>(core::bytes_get(result, 0)); }}")));
    }
    profiles.push((
        "copy-payload-control",
        include_str!("../../examples/analysis/native_owned_control.lang").to_owned(),
    ));
    for (label, text) in profiles {
        let profile = files.0.join(label);
        fs::create_dir(&profile).unwrap();
        let source = source(&text);
        let flow = source.failure_flow();
        let types = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let artifact = n::emit_object_for_target(
            flow,
            &layouts,
            &lang_codegen_clif::Triple::host(),
            Default::default(),
            1000,
        )
        .unwrap();
        let object = profile.join(if cfg!(target_env = "msvc") {
            "guest.obj"
        } else {
            "guest.o"
        });
        let mut target_linker = linker.clone();
        if cfg!(target_env = "msvc") {
            for index in 0..flow.functions().len() {
                let name = &artifact
                    .function(m::FunctionId(index as u32))
                    .unwrap()
                    .metadata()
                    .symbol;
                target_linker.push_argument(format!("/EXPORT:{name}"));
            }
        }
        let source_file = profile.join("guest.lang");
        fs::write(&object, artifact.bytes).unwrap();
        fs::write(&source_file, &text).unwrap();
        for response in [false, true] {
            let executable = profile.join(match (cfg!(target_env = "msvc"), response) {
                (true, true) => "response.exe",
                (true, false) => "direct.exe",
                (false, true) => "response",
                (false, false) => "direct",
            });
            let request = LinkRequest::new(&executable)
                .with_object(&startup_object)
                .with_object(&object)
                .with_object(&archive);
            if response {
                target_linker
                    .link_with_response_file(&request, profile.join("link.rsp"))
                    .unwrap();
            } else {
                target_linker.link(&request).unwrap();
            }
            let output = Command::new(&executable)
                .arg(&source_file)
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{label}, response={response}: {output:?}"
            );
            eprintln!("standalone {label} response={response} passed");
        }
    }
}
