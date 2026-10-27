use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
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
fn task_artifact() -> Vec<u8> {
    task_artifact_for(include_bytes!("../examples/scripts/tasks.lang"))
}
fn task_artifact_for(bytes: &[u8]) -> Vec<u8> {
    let analysis = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("stored-object-tasks.lang"),
        bytes,
        None,
        Default::default(),
        None,
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let lang_semantics::analysis::Program::Tasks(program) = analysis.result.unwrap() else {
        panic!("task program")
    };
    let table = program
        .into_verified()
        .into_plan()
        .into_source()
        .into_activity()
        .into_moved()
        .into_typed()
        .into_table();
    let entry = table
        .places
        .declarations
        .functions
        .iter()
        .position(|function| function.name == "main")
        .unwrap();
    lang_bytecode::tasks::encode(
        table,
        Some(lang_mir::unit::FunctionId(entry as u32)),
        Default::default(),
    )
    .unwrap()
}

#[test]
fn stored_metadata_and_exact_preparation_limits_refuse_before_binding() {
    use lang_codegen_clif::{
        native_record::{ErrorKind, Record},
        tasks::{Error, MetadataError, RecordedObject},
    };
    use lang_mir::unit as m;
    let semantic = task_artifact();
    let program = lang_bytecode::tasks::decode(&semantic, Default::default()).unwrap();
    let record = lang_codegen_clif::tasks::ObjectArtifact::emit_function(
        program.proof(),
        program.entry().unwrap(),
        Default::default(),
    )
    .unwrap()
    .into_recorded(Default::default())
    .unwrap()
    .into_record();
    let bytes = record.encode(&semantic, Default::default()).unwrap();
    let prepare = |bytes: &[u8], limits| {
        RecordedObject::from_record(Record::decode(bytes, &semantic, Default::default()).unwrap())
            .unwrap()
            .prepare(program.proof(), limits)
    };
    let baseline = prepare(&bytes, Default::default()).unwrap();
    let usage = baseline.usage();
    assert_eq!(baseline.bytes(), record.object());
    for (dimension, exact) in [usage.work, usage.allocation_bytes, usage.elements]
        .into_iter()
        .enumerate()
    {
        assert!(exact > 0);
        for (limit, success) in [(exact, true), (exact - 1, false), (0, false)] {
            let mut limits = m::Limits::default();
            match dimension {
                0 => limits.work = limit,
                1 => limits.allocations = limit,
                _ => limits.elements = limit,
            }
            assert_eq!(
                prepare(&bytes, limits).is_ok(),
                success,
                "dimension {dimension} limit {limit}"
            );
        }
    }
    let mut bad = bytes.clone();
    bad[16] ^= 1;
    assert!(matches!(
        prepare(&bad, Default::default()),
        Err(Error::Target { .. })
    ));
    bad = bytes.clone();
    bad[17] = 1;
    assert!(matches!(
        prepare(&bad, Default::default()),
        Err(Error::Profile)
    ));
    bad = bytes.clone();
    bad[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        prepare(&bad, Default::default()),
        Err(Error::Entry)
    ));
    // Independent revision-2 offsets: header/target/profile/entry/digests,
    // object count+bytes, function count, ID and symbol length+bytes.
    let object_length = u32::from_le_bytes(bytes[86..90].try_into().unwrap()) as usize;
    let count = 90 + object_length;
    let symbol = count + 12;
    let symbol_length =
        u32::from_le_bytes(bytes[count + 8..count + 12].try_into().unwrap()) as usize;
    bad = bytes.clone();
    bad[symbol] = b'x';
    assert!(matches!(
        prepare(&bad, Default::default()),
        Err(Error::Metadata {
            kind: MetadataError::Symbol,
            ..
        })
    ));
    let first_range = symbol + symbol_length + 8;
    bad = bytes.clone();
    bad[first_range + 8..first_range + 12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        prepare(&bad, Default::default()),
        Err(Error::Metadata {
            kind: MetadataError::SourceSpan,
            ..
        })
    ));
    bad = bytes[..count + 4].to_vec();
    bad[count..count + 4].copy_from_slice(&0u32.to_le_bytes());
    let payload = (bad.len() - 16) as u32;
    bad[12..16].copy_from_slice(&payload.to_le_bytes());
    assert!(matches!(
        prepare(&bad, Default::default()),
        Err(Error::Metadata {
            kind: MetadataError::FunctionSet,
            ..
        })
    ));
    bad = bytes.clone();
    bad[90] ^= 1;
    assert!(
        matches!(Record::decode(&bad,&semantic,Default::default()),Err(error) if error.kind==ErrorKind::ObjectDigest)
    );
    assert!(
        matches!(Record::decode(&bytes,b"different semantics",Default::default()),Err(error) if error.kind==ErrorKind::SemanticDigest)
    );
    // Rejection never consumes the original passive object or its proof.
    assert_eq!(
        prepare(&bytes, Default::default()).unwrap().bytes(),
        record.object()
    );

    let semantic = fixture::artifact();
    fixture::with_artifact(&semantic, |component, _, entry| {
        let record = lang_codegen_clif::tasks::ObjectArtifact::emit_component(
            component,
            entry,
            Default::default(),
        )
        .unwrap()
        .into_recorded(Default::default())
        .unwrap()
        .into_record();
        let bytes = record.encode(&semantic, Default::default()).unwrap();
        let loaded = || {
            RecordedObject::from_record(
                Record::decode(&bytes, &semantic, Default::default()).unwrap(),
            )
            .unwrap()
        };
        assert!(matches!(
            loaded().prepare(program.proof(), Default::default()),
            Err(Error::Profile)
        ));
        let mut bad = bytes.clone();
        bad[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
        let object = RecordedObject::from_record(
            Record::decode(&bad, &semantic, Default::default()).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            object.prepare_component(component, Default::default()),
            Err(Error::Entry)
        ));
        let object = loaded()
            .prepare_component(component, Default::default())
            .unwrap();
        assert_eq!(object.bytes(), record.object());
    });
}

#[test]
fn saved_task_and_component_records_rebuild_linked_entries_without_codegen() {
    let files = Files(
        std::env::temp_dir().join(format!("lang stored native objects-{}", std::process::id())),
    );
    fs::create_dir(&files.0).unwrap();
    let task_semantic = task_artifact();
    let component_semantic = fixture::artifact();
    let owned_semantic = task_artifact_for(include_bytes!("fixtures/task_owned_buffers.lang"));
    let owned_record = {
        let program = lang_bytecode::tasks::decode(&owned_semantic, Default::default()).unwrap();
        lang_codegen_clif::tasks::ObjectArtifact::emit_function(
            program.proof(),
            program.entry().unwrap(),
            Default::default(),
        )
        .unwrap()
        .into_recorded(Default::default())
        .unwrap()
        .into_record()
    };
    let task_record = {
        let program = lang_bytecode::tasks::decode(&task_semantic, Default::default()).unwrap();
        lang_codegen_clif::tasks::ObjectArtifact::emit_function(
            program.proof(),
            program.entry().unwrap(),
            Default::default(),
        )
        .unwrap()
        .into_recorded(Default::default())
        .unwrap()
        .into_record()
    };
    let component_record = fixture::with_artifact(&component_semantic, |program, _, entry| {
        lang_codegen_clif::tasks::ObjectArtifact::emit_component(program, entry, Default::default())
            .unwrap()
            .into_recorded(Default::default())
            .unwrap()
            .into_record()
    });
    let mut declarations = String::new();
    let mut symbols = String::new();
    let mut paths = Vec::new();
    let mut objects = Vec::new();
    for (index, (semantic, record)) in [
        (task_semantic, task_record),
        (component_semantic, component_record),
        (owned_semantic, owned_record),
    ]
    .into_iter()
    .enumerate()
    {
        let semantic_path = files.0.join(format!("{index}.lbc"));
        let native_path = files.0.join(format!("{index}.native"));
        let object_path = files.0.join(format!("{index}.o"));
        fs::write(&semantic_path, &semantic).unwrap();
        fs::write(
            &native_path,
            record.encode(&semantic, Default::default()).unwrap(),
        )
        .unwrap();
        fs::write(&object_path, record.object()).unwrap();
        for (function_index, function) in record.functions().iter().enumerate() {
            declarations.push_str(&format!(
                "#[link_name={:?}] fn entry_{index}_{function_index}();\n",
                function.symbol
            ));
            symbols.push_str(&format!(
                "{:?}=>Some(entry_{index}_{function_index} as *const u8),\n",
                function.symbol
            ));
        }
        paths.extend([semantic_path, native_path, object_path.clone()]);
        objects.push(object_path);
    }
    // Both producer programs, all original semantic buffers and records are gone.
    let source = files.0.join("consumer.rs");
    let helper = include_str!("support/component_object_program.rs");
    fs::write(&source,format!(r#"
mod fixture {{ {helper} }}
use lang_runtime::{{allocation as a, execution as e, component::{{execution,storage,wire}},task::runner as r}};
unsafe extern "C" {{ {declarations} }}
fn resolve(symbol:&str)->Option<*const u8>{{match symbol{{{symbols}_=>None}}}}
fn load(index:usize)->(Vec<u8>,lang_codegen_clif::tasks::RecordedObject){{
    let args:Vec<_>=std::env::args_os().skip(1).collect();
    let semantic=std::fs::read(&args[index*3]).unwrap();
    let bytes=std::fs::read(&args[index*3+1]).unwrap();
    assert_eq!(&bytes[8..10],&2u16.to_le_bytes());
    let record=lang_codegen_clif::native_record::Record::decode(&bytes,&semantic,Default::default()).unwrap();
    assert_eq!(record.object(),std::fs::read(&args[index*3+2]).unwrap());
    (semantic,lang_codegen_clif::tasks::RecordedObject::from_record(record).unwrap())
}}
fn main(){{
    for (index,expected) in [(0,42),(2,71)]{{
    let (semantic,record)=load(index);
    let program=lang_bytecode::tasks::decode(&semantic,Default::default()).unwrap();
    let object=record.prepare(program.proof(),Default::default()).unwrap();
    // SAFETY: exact compared object bytes and symbols are statically linked
    // with this compiler/SDK; code remains executable until process exit.
    let linked=unsafe{{object.bind((),|_,symbol|resolve(symbol),Default::default())}}.unwrap();
    let mut allocator=a::Allocator::new(a::System,Default::default()).unwrap();
    let outcome=linked.execute(&mut allocator,e::Control::with_limits(Default::default()),Default::default()).unwrap();
    assert_eq!(outcome.value,expected);if index==0{{assert_eq!(outcome.tasks.admitted,3);}}assert_eq!(outcome.tasks.live,0);
    assert_eq!(allocator.stats().live_allocations,0);
    let args:Vec<_>=std::env::args_os().skip(1).collect();
    let native=std::fs::read(&args[index*3+1]).unwrap();
    let target=lang_codegen_clif::Triple::host().to_string();
    let build=lang_codegen_clif::sdk::build_id(&lang_codegen_clif::Triple::host());
    let input=lang_codegen_clif::sdk::runtime::Input{{target:&target,build_id:&build,semantic:&semantic,native:&native}};
    let mut sdk_allocator=a::Allocator::new(a::System,Default::default()).unwrap();
    // SAFETY: the exact compared original objects are linked for process life.
    let failure=unsafe{{lang_codegen_clif::sdk::runtime::execute_tasks(input,&(),|_,name|resolve(name).and_then(|p|std::ptr::NonNull::new(p.cast_mut().cast())),&mut sdk_allocator,e::Control::with_limits(e::Limits{{fuel:Some(0),..Default::default()}}),Default::default(),Default::default())}}.err().unwrap();
    assert!(matches!(failure.kind,lang_codegen_clif::sdk::runtime::ErrorKind::TaskRun(ref failure) if matches!(failure.reason,r::Reason::Control(_)) && failure.tasks.live==0 && !failure.poisoned));
    assert_eq!(sdk_allocator.stats().live_allocations,0);
    if index==0{{
        for (tasks,expected_error) in [(lang_runtime::task::Limits{{live_tasks:2,total_tasks:3}},lang_runtime::task::Error::LiveTaskLimit{{limit:2}}),(lang_runtime::task::Limits{{live_tasks:3,total_tasks:2}},lang_runtime::task::Error::TotalTaskLimit{{limit:2}})]{{
            // SAFETY: same statically linked exact image and original allocator.
            let failure=unsafe{{lang_codegen_clif::sdk::runtime::execute_tasks(input,&(),|_,name|resolve(name).and_then(|p|std::ptr::NonNull::new(p.cast_mut().cast())),&mut sdk_allocator,e::Control::with_limits(Default::default()),r::Limits{{tasks,..Default::default()}},Default::default())}}.err().unwrap();
            assert!(matches!(failure.kind,lang_codegen_clif::sdk::runtime::ErrorKind::TaskRun(ref failure) if matches!(failure.reason.primary_cause(),r::Reason::Schedule(error) if *error==expected_error) && failure.tasks.admitted==2 && failure.tasks.live==0 && !failure.poisoned));
            assert_eq!(sdk_allocator.stats().live_allocations,0);
        }}
    }}
    let calls=std::cell::Cell::new(0);
    // SAFETY: malformed metadata/configuration must refuse before this resolver.
    let refused=unsafe{{lang_codegen_clif::sdk::runtime::execute_tasks(input,&(),|_,_|{{calls.set(calls.get()+1);None}},&mut sdk_allocator,e::Control::with_limits(Default::default()),Default::default(),lang_codegen_clif::sdk::runtime::Limits{{plans:lang_mir::unit::Limits{{work:0,..Default::default()}},..Default::default()}})}}.err().unwrap();
    assert!(matches!(refused.kind,lang_codegen_clif::sdk::runtime::ErrorKind::TaskPlan(_)));assert_eq!(calls.get(),0);
    let mut bad=native.clone();bad[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
    // SAFETY: invalid entry must refuse before any code resolution/effects.
    let refused=unsafe{{lang_codegen_clif::sdk::runtime::execute_tasks(lang_codegen_clif::sdk::runtime::Input{{native:&bad,..input}},&(),|_,_|{{calls.set(calls.get()+1);None}},&mut sdk_allocator,e::Control::with_limits(Default::default()),Default::default(),Default::default())}}.err().unwrap();
    assert!(matches!(refused.kind,lang_codegen_clif::sdk::runtime::ErrorKind::Entry));assert_eq!(calls.get(),0);
    // SAFETY: a fresh scoped call reuses the same caller allocator after refusal.
    let outcome=unsafe{{lang_codegen_clif::sdk::runtime::execute_tasks(input,&(),|_,name|resolve(name).and_then(|p|std::ptr::NonNull::new(p.cast_mut().cast())),&mut sdk_allocator,e::Control::with_limits(Default::default()),Default::default(),Default::default())}}.unwrap();
    assert_eq!(outcome.value,expected);assert_eq!(outcome.tasks.live,0);assert_eq!(sdk_allocator.stats().live_allocations,0);
    }}
    let (semantic,record)=load(1);
    fixture::with_artifact(&semantic,move|program,session,entry|{{
        let object=record.prepare_component(program,Default::default()).unwrap();
        // SAFETY: the same exact static object/image contract applies.
        let linked=unsafe{{object.bind((),|_,symbol|resolve(symbol),Default::default())}}.unwrap();
        let mut allocator=a::Allocator::new(a::System,Default::default()).unwrap();
        let mut budget=lang_mir::unit::Budget::new(Default::default());
        let mut resources=storage::Resources::prepare(&allocator,4,4,&mut budget).unwrap();
        let mut state=execution::InstanceState::prepare(&mut resources).unwrap();
        let mut engine=lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
        for value in [41,-1,0]{{
            let control=e::Control::with_limits(Default::default());
            let mut invoker=engine.begin(control).unwrap();
            let result=state.with_program(program,|instance|{{
                let mut invocation=execution::Invocation::prepare_owned(instance,entry,&mut resources,&mut invoker,&mut budget).unwrap();
                invocation.prepare_output(session).unwrap();
                linked.execute_component(&mut allocator,control,Default::default(),&[r::EntryArgument::I64(value)],&mut invocation)
            }}).unwrap();
            if value<0{{let failure=result.err().unwrap();assert!(matches!(failure.reason,r::Reason::Panic(Some(-42))));assert!(!failure.poisoned);}}
            else{{let outcome=result.unwrap();let execution::Value::Frame(frame)=outcome.value else{{panic!("scalar frame")}};assert!(matches!(frame.values().nodes.as_slice(),[wire::Value::I64(answer)] if *answer==value+1));}}
            assert!(resources.is_empty());assert_eq!(allocator.stats().live_allocations,0);assert!(!state.poisoned());
        }}
    }});
    println!("stored tasks 42/3; component 42/panic/reuse; no codegen");
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
    artifacts::native_search_paths(&mut command);
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--edition=2024", "--crate-type=bin"])
        .arg(source)
        .arg("-o")
        .arg(&executable)
        .arg("-L")
        .arg(format!("dependency={}", dependencies.display()));
    for name in [
        "lang",
        "lang_codegen_clif",
        "lang_bytecode",
        "lang_diagnostics",
        "lang_mir",
        "lang_runtime",
    ] {
        command
            .arg("--extern")
            .arg(format!("{name}={}", artifacts::library(name).display()));
    }
    for path in objects {
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
    let output = Command::new(executable).args(paths).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "stored tasks 42/3; component 42/panic/reuse; no codegen"
    );
}
