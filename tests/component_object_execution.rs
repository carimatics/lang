use std::{fs, path::PathBuf, process::Command};
#[path = "support/rust_artifacts.rs"]
mod artifacts;
#[path = "support/component_object_program.rs"]
mod fixture;

struct Files(PathBuf);
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn exact_linked_component_object_executes_arguments_and_failures_without_jit_or_vm() {
    let files =
        Files(std::env::temp_dir().join(format!("lang component object-{}", std::process::id())));
    fs::create_dir(&files.0).unwrap();
    let path = files.0.join("component.o");
    let (declarations, symbols) = fixture::with_program(|program, _, entry| {
        assert!(matches!(
            lang_codegen_clif::tasks::ObjectArtifact::emit_component(
                program,
                lang_mir::unit::FunctionId(u32::MAX),
                Default::default()
            ),
            Err(lang_codegen_clif::tasks::Error::Entry)
        ));
        assert!(
            lang_codegen_clif::tasks::ObjectArtifact::emit_function(
                program.proof(),
                entry,
                Default::default()
            )
            .is_err()
        );
        let object = lang_codegen_clif::tasks::ObjectArtifact::emit_component(
            program,
            entry,
            Default::default(),
        )
        .unwrap();
        let mut declarations = String::new();
        let mut symbols = String::new();
        for (index, function) in object.metadata().enumerate() {
            assert!(function.symbol.starts_with("lang_component_tasks_r1_"));
            declarations.push_str(&format!(
                "#[link_name={:?}] fn entry_{index}();\n",
                function.symbol
            ));
            symbols.push_str(&format!(
                "{:?} => Some(entry_{index} as *const u8),\n",
                function.symbol
            ));
        }
        fs::write(&path, object.bytes()).unwrap();
        (declarations, symbols)
    });
    let helper = include_str!("support/component_object_program.rs");
    let source = files.0.join("consumer.rs");
    fs::write(&source, format!(r#"
mod fixture {{ {helper} }}
use lang_runtime::{{allocation as a, component::{{execution, storage, wire}}, execution as e, task::runner as r}};
unsafe extern "C" {{ {declarations} }}
fn main() {{
    let expected=std::fs::read(std::env::args_os().nth(1).unwrap()).unwrap();
    fixture::with_program(|program,session,entry| {{
        let object=lang_codegen_clif::tasks::ObjectArtifact::emit_component(program,entry,Default::default()).unwrap();
        assert_eq!(object.bytes(),expected);
        // SAFETY: these exact compared objects are linked into this executable;
        // each generated symbol remains executable until process exit.
        let linked=unsafe{{object.bind((),|_,symbol|match symbol{{{symbols}_=>None}},Default::default())}}.unwrap();
        let mut allocator=a::Allocator::new(a::System,Default::default()).unwrap();
        let mut budget=lang_mir::unit::Budget::new(Default::default());
        let mut resources=storage::Resources::prepare(&allocator,4,4,&mut budget).unwrap();
        let mut state=execution::InstanceState::prepare(&mut resources).unwrap();
        let mut engine=lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
        for value in [41,-1,0] {{
            let control=e::Control::with_limits(Default::default());
            let mut invoker=engine.begin(control).unwrap();
            let result=state.with_program(program,|instance|{{
                let mut invocation=execution::Invocation::prepare_owned(instance,entry,&mut resources,&mut invoker,&mut budget).unwrap();
                invocation.prepare_output(session).unwrap();
                linked.execute_component(&mut allocator,control,Default::default(),&[r::EntryArgument::I64(value)],&mut invocation)
            }}).unwrap();
            if value<0 {{let failure=result.err().unwrap();assert!(matches!(failure.reason,r::Reason::Panic(Some(-42))));assert!(!failure.poisoned);}}
            else {{let outcome=result.unwrap();let execution::Value::Frame(frame)=outcome.value else{{panic!("canonical scalar result")}};assert!(matches!(frame.values().nodes.as_slice(),[wire::Value::I64(answer)] if *answer==value+1));assert_eq!(outcome.tasks.live,0);}}
            assert!(resources.is_empty());assert_eq!(allocator.stats().live_allocations,0);assert!(!state.poisoned());
        }}
    }});
    println!("component object 42; panic -42; reused 1");
}}
"#)).unwrap();
    let executable = files.0.join(if cfg!(windows) {
        "consumer.exe"
    } else {
        "consumer"
    });
    let dependencies = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut command = Command::new("rustc");
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--edition=2024", "--crate-type=bin"])
        .arg(&source)
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
    ] {
        command
            .arg("--extern")
            .arg(format!("{name}={}", artifacts::library(name).display()));
    }
    let mut object = std::ffi::OsString::from("link-arg=");
    object.push(&path);
    command.arg("-C").arg(object);
    let compiled = command.output().unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = Command::new(executable).arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "component object 42; panic -42; reused 1"
    );
}
