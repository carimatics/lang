//! The task object is linked into a separate process; no JIT compile/invoke API
//! is called by that process. Exact object bytes bind its private metadata.
use super::*;
use std::{fs, path::PathBuf, process::Command};
#[path = "rust_artifacts.rs"]
mod artifacts;
struct Files(PathBuf);
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn standalone_task_objects_execute_generated_continuations() {
    let files =
        Files(std::env::temp_dir().join(format!("lang task static-{}", std::process::id())));
    fs::create_dir(&files.0).unwrap();
    let profiles = [
        (
            "join",
            include_str!("../../examples/scripts/tasks.lang").to_owned(),
        ),
        ("aggregate", AGGREGATES.to_owned()),
        (
            "owned-cancel",
            include_str!("../fixtures/task_owned.lang").to_owned(),
        ),
        (
            "owned-buffers",
            include_str!("../fixtures/task_owned_buffers.lang").to_owned(),
        ),
        (
            "sync-loans",
            include_str!("../fixtures/task_sync_loans.lang").to_owned(),
        ),
        (
            "owned-pipeline",
            include_str!("../fixtures/task_owned_pipeline.lang").to_owned(),
        ),
        (
            "owned-arena",
            include_str!("../fixtures/task_owned_arena.lang").to_owned(),
        ),
        (
            "owned-integration",
            include_str!("../fixtures/task_owned_integrated.lang").to_owned(),
        ),
        (
            "owned-transfer",
            include_str!("../fixtures/task_owned_transfer.lang").to_owned(),
        ),
        (
            "owned-transfer-all",
            include_str!("../fixtures/task_owned_transfer_all.lang").to_owned(),
        ),
        (
            "channel-backpressure",
            include_str!("../fixtures/task_channels.lang").to_owned(),
        ),
        (
            "channel-self-cycle",
            include_str!("../fixtures/task_channel_self_cycle.lang").to_owned(),
        ),
        (
            "channel-reinspect",
            include_str!("../fixtures/task_channel_reinspect.lang").to_owned(),
        ),
        (
            "channel-mixed",
            include_str!("../fixtures/task_channel_mixed.lang").to_owned(),
        ),
        (
            "channel-shared-sender",
            include_str!("../fixtures/task_channel_shared_sender.lang").to_owned(),
        ),
        (
            "channel-mixed-cancel",
            include_str!("../fixtures/task_channel_mixed_cancel.lang").to_owned(),
        ),
        (
            "channel-text",
            include_str!("../fixtures/task_channel_text.lang").to_owned(),
        ),
        (
            "channel-copy-option",
            include_str!("../fixtures/task_channel_copy_sum.lang").to_owned(),
        ),
        (
            "channel-copy-unit",
            include_str!("../fixtures/task_channel_copy_unit.lang").to_owned(),
        ),
        (
            "channel-copy-aggregate",
            include_str!("../fixtures/task_channel_copy.lang").to_owned(),
        ),
        (
            "channel-copy-cancel",
            include_str!("../fixtures/task_channel_copy_cancel.lang").to_owned(),
        ),
        (
            "channel-copy-recover",
            include_str!("../fixtures/task_channel_copy_recover.lang").to_owned(),
        ),
        (
            "owned-integration-panic",
            include_str!("../fixtures/task_owned_integrated.lang")
                .replace("await scenario(false)", "await scenario(true)"),
        ),
        (
            "cancel",
            format!(
                "{WORKER}async fn main()->i64{{let mut h:core::Task<i64>=spawn worker(42);cancel h;let mut r:core::Result<i64,core::TaskError>=await h;match r{{core::Result::Ok(n)=>{{return n;}},core::Result::Err(core::TaskError::Cancelled)=>{{return 7;}},}}}}"
            ),
        ),
        (
            "panic",
            format!(
                "{WORKER}async fn bad()->i64{{return core::panic::<i64>(91);}}async fn main()->i64{{let mut h:core::Task<i64>=spawn bad();let mut s:core::Task<i64>=spawn worker(42);let mut r:core::Result<i64,core::TaskError>=await h;return 0;}}"
            ),
        ),
    ];
    let mut objects = Vec::new();
    let mut declarations = String::new();
    let mut symbols = String::new();
    let mut ordinal = 0;
    for (label, text) in &profiles {
        let proof = prepare(text);
        let object =
            lang_codegen_clif::tasks::ObjectArtifact::emit(proof.verified(), Default::default())
                .unwrap();
        for function in object.metadata() {
            declarations.push_str(&format!(
                "#[link_name={:?}] fn task_{ordinal}();\n",
                function.symbol
            ));
            symbols.push_str(&format!(
                "{:?} => Some(task_{ordinal} as *const u8),\n",
                function.symbol
            ));
            ordinal += 1;
        }
        let path = files.0.join(format!("{label}.o"));
        fs::write(&path, object.bytes()).unwrap();
        objects.push(path);
        fs::write(files.0.join(format!("{label}.lang")), text).unwrap();
    }
    let bootstrap = files.0.join("driver.rs");
    fs::write(&bootstrap,format!(r#"
use lang_runtime::{{allocation as a,execution as e,task::runner as r}};
unsafe extern "C" {{ {declarations} }}
fn allocator()->a::Allocator<a::System>{{a::Allocator::new(a::System,Default::default()).unwrap()}}
fn compare(proof:&lang_mir::unit::ownership::tasks::VerifiedTasks,native:&lang_codegen_clif::tasks::LinkedProgram<'_,()>,control:e::Control<'_>,limits:r::Limits)->Result<r::Outcome,r::Failure>{{
    let mut va=allocator();let mut na=allocator();let expected=lang_vm::unit::tasks::execute_entry(proof,&mut va,control,limits);let actual=native.execute(&mut na,control,limits);
    assert_eq!(va.stats(),na.stats());assert_eq!(na.stats().live_allocations,0);
    match (&expected,&actual){{(Ok(a),Ok(b))=>{{assert_eq!(a.value,b.value);assert_eq!(a.events,b.events);assert_eq!(a.usage,b.usage);assert_eq!(a.tasks,b.tasks);}},(Err(a),Err(b))=>{{assert_eq!(format!("{{:?}}",a.reason),format!("{{:?}}",b.reason));assert_eq!(a.span,b.span);assert_eq!(a.function,b.function);assert_eq!(a.events,b.events);assert_eq!(a.usage,b.usage);assert_eq!(a.tasks,b.tasks);assert_eq!(a.poisoned,b.poisoned);assert_eq!(format!("{{:?}}",a.report),format!("{{:?}}",b.report));assert_eq!(b.tasks.live,0);}},_=>panic!("VM/object result differs")}}actual
}}
fn main(){{
    let args:Vec<_>=std::env::args().collect();let text=std::fs::read_to_string(&args[1]).unwrap();let original=std::fs::read(&args[2]).unwrap();
    let analyzed=lang::frontend::analyze_virtual(lang_diagnostics::SourceMap::new(),std::path::Path::new("native-tasks.lang"),text.as_bytes(),None,Default::default(),None);assert!(analyzed.diagnostics.is_empty(),"{{:?}}",analyzed.diagnostics);
    let lang_semantics::analysis::Program::Tasks(proof)=analyzed.result.unwrap()else{{panic!("task profile")}};
    let object=lang_codegen_clif::tasks::ObjectArtifact::emit(proof.verified(),Default::default()).unwrap();assert_eq!(object.bytes(),original,"exact linked object/compiler/SDK required");
    // SAFETY: the parent links the exact compared objects into this executable.
    // These symbols remain executable until process exit and match emit's ABI.
    let native=unsafe{{object.bind((),|_,symbol|match symbol{{{symbols}_=>None}},Default::default())}}.unwrap();
    let limits=r::Limits{{trace_events:512,..Default::default()}};let control=e::Control::with_limits(Default::default());
    let baseline=compare(proof.verified(),&native,control,limits);let usage=match &baseline{{Ok(o)=>o.usage,Err(e)=>e.usage}};
    for fuel in 0..=usage.fuel{{let _=compare(proof.verified(),&native,e::Control::with_limits(e::Limits{{fuel:Some(fuel),..Default::default()}}),limits);}}
    for work in 0..=usage.service_work{{let _=compare(proof.verified(),&native,control.and_service_work(Some(work)),limits);}}
    match baseline{{Ok(out)=>println!("{{}}",out.value),Err(error)=>{{let r::Reason::Panic(Some(code))=error.reason else{{panic!("unexpected linked task failure")}};assert!(matches!(code,91|-42));println!("panic {{code}}");}}}}
}}
"#)).unwrap();
    let executable = files
        .0
        .join(if cfg!(windows) { "tasks.exe" } else { "tasks" });
    let dependencies = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut rustc = Command::new("rustc");
    rustc
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--edition=2024", "--crate-type=bin"])
        .arg(&bootstrap)
        .arg("-o")
        .arg(&executable)
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
    ] {
        rustc
            .arg("--extern")
            .arg(format!("{name}={}", artifacts::library(name).display()));
    }
    for object in &objects {
        let mut argument = std::ffi::OsString::from("link-arg=");
        argument.push(object);
        rustc.arg("-C").arg(argument);
    }
    let output = rustc.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for ((label, _), object) in profiles.iter().zip(&objects) {
        let output = Command::new(&executable)
            .arg(files.0.join(format!("{label}.lang")))
            .arg(object)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            match *label {
                "join" | "aggregate" | "owned-pipeline" | "owned-arena" | "owned-integration"
                | "owned-transfer" | "owned-transfer-all" => "42",
                "cancel" | "owned-cancel" => "7",
                "owned-buffers" => "71",
                "sync-loans" => "12",
                "owned-integration-panic" => "panic -42",
                label if label.starts_with("channel-") => "42",
                _ => "panic 91",
            },
            "{label}"
        );
    }
}
