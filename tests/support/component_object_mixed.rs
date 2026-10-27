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
fn linked_component_resources_call_vm_jit_and_object_children() {
    linked_component_profile(Profile::Fresh);
}
#[test]
fn stored_component_resources_call_vm_jit_and_recorded_object_children() {
    linked_component_profile(Profile::Stored);
}
#[test]
fn installed_component_resources_call_vm_jit_and_recorded_object_children() {
    linked_component_profile(Profile::Installed);
}
#[test]
fn installed_component_interruption_preserves_original_vm_jit_aot_callbacks() {
    linked_component_profile(Profile::Interrupted);
}
#[test]
fn installed_component_exhaustion_preserves_combined_limits_and_cleanup() {
    linked_component_profile(Profile::Exhaustion);
}
#[derive(Clone, Copy, Debug)]
enum Profile {
    Fresh,
    Stored,
    Installed,
    Interrupted,
    Exhaustion,
}
fn linked_component_profile(profile: Profile) {
    let stored = !matches!(profile, Profile::Fresh);
    let installed = matches!(
        profile,
        Profile::Installed | Profile::Interrupted | Profile::Exhaustion
    );
    let interruption = matches!(profile, Profile::Interrupted);
    let exhaustion = matches!(profile, Profile::Exhaustion);
    let files = Files(std::env::temp_dir().join(format!(
        "lang component mixed-{}-{profile:?}",
        std::process::id()
    )));
    fs::create_dir(&files.0).unwrap();
    let mut h = Harness::new();
    let unit = if exhaustion {
        unit_callback_results(
            include_bytes!("../fixtures/component_scoped_roots_exhaustion.lang"),
            false,
            false,
            false,
            false,
            false,
            true,
        )
        .0
    } else {
        stored_unit()
    };
    let snapshot = unit.code().declarations().types().table().sources
        [unit.document().document().source.0 as usize]
        .clone();
    let semantic =
        lang::artifact::component::encode(unit, snapshot, None, Default::default()).unwrap();
    if stored {
        h.unit = lang::artifact::component::decode(&semantic, Default::default())
            .unwrap()
            .into_unit();
    }
    let semantic_path = files.0.join("component.lbc");
    fs::write(&semantic_path, &semantic).unwrap();
    let (registration, _) = h.registration(false);
    let mut declarations = String::new();
    let mut symbols = String::new();
    let mut objects = Vec::new();
    let mut packages = Vec::new();
    let mut embedded_objects = String::new();
    let mut embedded_records = String::new();
    let cleanup_semantic = if exhaustion {
        let unit = stored_unit();
        let snapshot = unit.code().declarations().types().table().sources
            [unit.document().document().source.0 as usize]
            .clone();
        lang::artifact::component::encode(unit, snapshot, None, Default::default()).unwrap()
    } else {
        Vec::new()
    };
    {
        let mut emit = |program: &program::Program<'_>, name: &str, semantic: &[u8]| {
            let object_index = objects.len();
            let entry_name = if name == "cleanup" { "consume" } else { name };
            let object = lang_codegen_clif::tasks::ObjectArtifact::emit_component(
                program,
                entry(program, entry_name),
                Default::default(),
            )
            .unwrap();
            if installed {
                let package = lang::native::component::compile(
                    semantic,
                    program,
                    entry(program, entry_name),
                    lang_codegen_clif::Triple::host(),
                    Default::default(),
                )
                .unwrap();
                let path = files.0.join(format!("{name}-package"));
                package.publish(&path).unwrap();
                packages.push(path);
            }
            embedded_objects.push_str(&format!("{name:?} => &{:?},", object.bytes()));
            let path = files.0.join(format!("{name}.o"));
            fs::write(&path, object.bytes()).unwrap();
            objects.push(path);
            for (index, function) in object.metadata().enumerate() {
                declarations.push_str(&format!(
                    "#[link_name={:?}] fn entry_{object_index}_{index}();\n",
                    function.symbol
                ));
                symbols.push_str(&format!(
                    "{:?} => Some(entry_{object_index}_{index} as *const u8),\n",
                    function.symbol
                ));
            }
            if stored {
                let record = object
                    .into_recorded(Default::default())
                    .unwrap()
                    .into_record();
                let native = record.encode(semantic, Default::default()).unwrap();
                embedded_records.push_str(&format!("{name:?} => &{native:?},"));
                fs::write(files.0.join(format!("{name}.native")), native).unwrap();
            }
        };
        prepared_unit(&h.unit, registration.clone(), |program, _| {
            for name in ["create", "consume", "answer"] {
                emit(program, name, &semantic);
            }
        });
        if exhaustion {
            let unit = lang::artifact::component::decode(&cleanup_semantic, Default::default())
                .unwrap()
                .into_unit();
            prepared_unit(&unit, registration, |program, _| {
                emit(program, "cleanup", &cleanup_semantic)
            });
        }
    }
    drop(h);

    // Reuse the real resource owner and callback probes in a separately linked
    // executable. Embed fixture bytes, so its build has no relative file lookup.
    let mut prelude = include_str!("../component_scoped_roots.rs")
        .split("\n#[path = \"support/component_")
        .next()
        .unwrap()
        .to_owned();
    for (expression, bytes) in [
        (
            "include_bytes!(\"fixtures/component_counter.idl.json\")",
            include_bytes!("../fixtures/component_counter.idl.json").as_slice(),
        ),
        (
            "include_bytes!(\"fixtures/component_scoped_roots.lang\")",
            include_bytes!("../fixtures/component_scoped_roots.lang").as_slice(),
        ),
    ] {
        prelude = prelude.replace(expression, &format!("&{bytes:?}"));
    }
    prelude = prelude
        .replace("unit: stored_unit(),", "unit: load_unit(),")
        .replace("let unit = stored_unit();", "let unit = load_unit();");
    let interruption_body = include_str!("component_interruption_native.rs");
    let exhaustion_body = include_str!("component_exhaustion_native.rs");
    let exhaustion_checks = include_str!("component_exhaustion_checks.rs");
    let exhaustion_reference = if exhaustion {
        let (stats, requests, usage) = component_exhaustion::reference();
        let requests: Vec<_> = requests
            .iter()
            .map(|r| (r.bytes(), r.alignment()))
            .collect();
        format!(
            "fn verify_exhaustion_reference(value:&quota::Observed){{assert_eq!(value.allocation,a::{stats:?});assert_eq!(value.usage,e::{usage:?});assert_eq!(value.requests.iter().map(|r|(r.bytes(),r.alignment())).collect::<Vec<_>>(),vec!{requests:?});}}"
        )
    } else {
        "fn verify_exhaustion_reference(_: &quota::Observed) {}".into()
    };
    let source = files.0.join("consumer.rs");
    fs::write(&source, format!(r#"
#![allow(dead_code, unused_imports)]
{prelude}
const STORED:bool={stored};
const INSTALLED:bool={installed};
const INTERRUPTION:bool={interruption};
const EXHAUSTION:bool={exhaustion};
const CLEANUP_SEMANTIC:&[u8]=&{cleanup_semantic:?};
const SEMANTIC:&[u8]=&{semantic:?};
fn object_bytes(name:&str)->&'static[u8]{{match name{{{embedded_objects}_=>panic!("object")}}}}
fn native_bytes(name:&str)->&'static[u8]{{match name{{{embedded_records}_=>panic!("record")}}}}
unsafe extern "C" {{ {declarations} }}
fn resolve(symbol: &str) -> Option<*const u8> {{ match symbol {{ {symbols} _ => None }} }}
fn load_unit()->bindings::Unit {{
    if STORED {{lang::artifact::component::decode(SEMANTIC,Default::default()).unwrap().into_unit()}}
    else {{stored_unit()}}
}}
fn object_for<'p>(program:&'p program::Program<'p>,name:&str)->lang_codegen_clif::tasks::ObjectArtifact<'p>{{
    if STORED {{
        let record=lang_codegen_clif::native_record::Record::decode(native_bytes(name),SEMANTIC,Default::default()).unwrap();
        lang_codegen_clif::tasks::RecordedObject::from_record(record).unwrap().prepare_component(program,Default::default()).unwrap()
    }}else{{lang_codegen_clif::tasks::ObjectArtifact::emit_component(program,entry(program,name),Default::default()).unwrap()}}
}}
fn call_object(h: &mut Harness, provider: native::Provider<'static>, name: &str, input: Option<storage::Claim>) -> Result<r::Outcome<execution::Value>, r::Failure> {{
    call_object_control(h,provider,name,input,e::Control::with_limits(Default::default()))
}}
fn call_object_control(h: &mut Harness, provider: native::Provider<'static>, name: &str, input: Option<storage::Claim>, control:e::Control<'_>) -> Result<r::Outcome<execution::Value>, r::Failure> {{
    let semantic=if name=="cleanup"{{CLEANUP_SEMANTIC}}else{{SEMANTIC}};
    let unit=if name=="cleanup"{{lang::artifact::component::decode(semantic,Default::default()).unwrap().into_unit()}}else{{load_unit()}};
    prepared_unit(&unit,provider, |program, session| {{
        let function=entry(program,if name=="cleanup"{{"consume"}}else{{name}});
        let linked=if INSTALLED {{
            let target=lang_codegen_clif::Triple::host().to_string();
            let build=lang_codegen_clif::sdk::build_id(&lang_codegen_clif::Triple::host());
            let input=lang_codegen_clif::sdk::runtime::Input{{target:&target,build_id:&build,semantic,native:native_bytes(name)}};
            // SAFETY: exact package objects/addresses are linked by this SDK.
            unsafe{{lang::native::component::bind(input,program,(),|_,symbol|resolve(symbol).and_then(|p|std::ptr::NonNull::new(p.cast_mut().cast())),Default::default())}}.unwrap()
        }}else{{
            let object=object_for(program,name);
            assert_eq!(object.bytes(),object_bytes(name));
            // SAFETY: exact compared objects are statically linked for process life.
            unsafe {{object.bind((),|_,symbol|resolve(symbol),Default::default())}}.unwrap()
        }};
        let mut invoker=h.engine.begin(control).unwrap();
        let mut budget=m::Budget::new(Default::default());
        h.state.with_program(program,|instance| {{
            let mut invocation=execution::Invocation::prepare_owned(instance,function,&mut h.values,&mut invoker,&mut budget).unwrap();
            invocation.prepare_output(session).unwrap();
            let arguments:Vec<_>=input.into_iter().map(EntryArgument::Resource).collect();
            linked.execute_component_with_budget(&mut h.allocator,control,r::Limits{{tasks:h.task_limits,..Default::default()}},&arguments,&mut invocation,&mut budget)
        }}).unwrap()
    }})
}}
{interruption_body}
mod quota {{ {exhaustion_checks} }}
{exhaustion_reference}
{exhaustion_body}
fn main() {{
    if EXHAUSTION {{
        exhaustion_profiles();
        println!("installed AOT exhaustion 72 profiles; original limits; custody and cleanup");
        return;
    }}
    if INTERRUPTION {{
        interruption_profiles();
        println!("installed AOT interruption 15 profiles; original context; cleanup and reuse");
        return;
    }}
    for cancel in [false,true] {{
    for backend in 0..3 {{
    for cleanup_failure in [false,true] {{
        if cancel && cleanup_failure {{continue;}}
        let mut h=Harness::new();
        let mut child=Child::new(&mut h.values,backend==1,1,usize::MAX);
        if backend==2 {{
            child.object_resolver=Some(resolve);
            if STORED {{
                child.object_record=Some(ObjectRecord{{native:native_bytes("answer").into(),semantic:SEMANTIC.into()}});
            }}
        }}
        child.cancel_task.set(cancel);
        let child=Rc::new(child);
        let (registration,watch)=h.registration_child(cleanup_failure,Some(child.clone()));
        // Check the child's exact object too before lending the original task.
        prepared(registration.clone(),|program,_|{{
            let object=object_for(program,"answer");
            assert_eq!(object.bytes(),object_bytes("answer"));
        }});
        let created=call_object(&mut h,registration.clone(),"create",None);
        if cancel {{
            let failure=created.err().expect("original callback cancellation");
            assert!(matches!(failure.reason.primary_cause(),r::Reason::Cancelled));
            assert_eq!(failure.tasks.admitted,1);assert_eq!(failure.tasks.live,0);
            assert!(!failure.poisoned && !h.state.poisoned());
            assert!(child.failure.borrow().as_ref().unwrap().contains("Cancelled"));
            assert_eq!(child.task.borrow().as_ref().unwrap().index(),0);
            assert_eq!(child.task_usage.get().unwrap().admitted,1);
            assert_eq!(child.task_observations.get(),1);
            {{
                let frames=child.failure_callers.borrow();
                let frames=frames.as_ref().expect("cancelled child trace");
                assert_eq!(frames.frames.len(),1,"cancelled child frame count");
                assert_eq!(frames.omitted,0);
                assert!(frames.frames.iter().all(|frame|Some(&frame.task)==child.task.borrow().as_ref()));
            }}
            assert_eq!(watch.produced.get(),0);assert_eq!(watch.destroyed.get(),0);
            assert!(h.values.is_empty());assert_eq!(h.allocator.stats().live_allocations,0);
            assert_eq!(h.engine.check(&child.parent.get().unwrap()),5);
            drop(registration);assert_eq!(watch.released.get(),0);
            drop(failure);assert_eq!(watch.released.get(),1);
            continue;
        }}
        let created=created.unwrap();
        let execution::Value::Frame(frame)=created.value else{{panic!("resource frame")}};
        assert_eq!(frame.resources().len(),1);
        let claim=frame.resources()[0];
        drop(frame);
        assert_eq!(child.value.get(),42);
        assert_eq!(child.task.borrow().as_ref().unwrap().index(),0);
        assert_eq!(child.task_usage.get().unwrap().admitted,1);
        let consumed=call_object(&mut h,registration.clone(),"consume",Some(claim));
        if cleanup_failure {{
            let failure=consumed.err().expect("mandatory native destruction fails");
            assert!(failure.poisoned && h.state.poisoned());
            assert_eq!(failure.report.as_ref().unwrap().cleanup_failures(),1);
            assert_eq!(watch.produced.get(),1);assert_eq!(watch.destroyed.get(),1);
            assert!(h.values.is_empty());assert_eq!(h.allocator.stats().live_allocations,0);
            drop(registration);assert_eq!(watch.released.get(),0);
            drop(failure);assert_eq!(watch.released.get(),1);
            continue;
        }}
        let consumed=consumed.unwrap();
        let execution::Value::Frame(frame)=consumed.value else{{panic!("scalar frame")}};
        assert!(matches!(frame.values().nodes.as_slice(),[wire::Value::I64(42)]));
        drop(frame);
        assert_eq!(watch.produced.get(),1);assert_eq!(watch.destroyed.get(),1);
        assert!(h.values.is_empty());assert_eq!(h.allocator.stats().live_allocations,0);
        assert!(!h.state.poisoned());
        assert_eq!(h.engine.check(&child.parent.get().unwrap()),5);
        assert_eq!(h.engine.check(&child.nested.get().unwrap()),5);
        drop(registration);assert_eq!(watch.released.get(),1);
    }}
    }}
    }}
    println!("AOT resource callback VM/JIT/AOT 42; destroyed once; original task; cancelled");
}}
"#)).unwrap();
    let executable = files.0.join(if cfg!(windows) {
        "consumer.exe"
    } else {
        "consumer"
    });
    if installed {
        let built = Command::new(env!("CARGO"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "build",
                "--locked",
                "--bin",
                "lang",
                "--example",
                "native-sdk",
                "--message-format=json",
            ])
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let sdk = files.0.join("sdk");
        lang::native::sdk_package::assemble(&built.stdout, &sdk, Default::default()).unwrap();
        lang::native::link::link(
            &lang::native::link::Request {
                rustc: None,
                sdk: sdk.clone(),
                package: packages[0].clone(),
                host: Some(source.clone()),
                objects: packages
                    .iter()
                    .skip(1)
                    .map(|p| {
                        p.join(if cfg!(windows) {
                            "program.obj"
                        } else {
                            "program.o"
                        })
                    })
                    .collect(),
                output: executable.clone(),
            },
            Default::default(),
        )
        .unwrap();
        fs::rename(&sdk, files.0.join("sdk-away")).unwrap();
        for (index, path) in packages.iter().enumerate() {
            fs::rename(path, files.0.join(format!("package-away-{index}"))).unwrap();
        }
        fs::rename(&source, files.0.join("source-away.rs")).unwrap();
        fs::rename(&semantic_path, files.0.join("semantic-away")).unwrap();
        for (index, path) in objects.iter().enumerate() {
            fs::rename(path, files.0.join(format!("object-away-{index}"))).unwrap();
            fs::rename(
                path.with_extension("native"),
                files.0.join(format!("record-away-{index}")),
            )
            .unwrap();
        }
    } else {
        let dependencies = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let mut command = Command::new("rustc");
        artifacts::configure_host(&mut command);
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
            "lang_vm",
            "serde_json",
        ] {
            command
                .arg("--extern")
                .arg(format!("{name}={}", artifacts::library(name).display()));
        }
        for path in &objects {
            let mut option = std::ffi::OsString::from("link-arg=");
            option.push(path);
            command.arg("-C").arg(option);
        }
        let compiled = command.output().unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
    }
    let output = Command::new(executable)
        .current_dir(&files.0)
        .env("CARGO", files.0.join("must-not-run-cargo"))
        .env("CARGO_HOME", files.0.join("empty-cargo-home"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        if exhaustion {
            "installed AOT exhaustion 72 profiles; original limits; custody and cleanup"
        } else if interruption {
            "installed AOT interruption 15 profiles; original context; cleanup and reuse"
        } else {
            "AOT resource callback VM/JIT/AOT 42; destroyed once; original task; cancelled"
        }
    );
}
