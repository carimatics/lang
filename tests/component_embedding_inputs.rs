use lang::{component_bindings as g, component_embedding as embedding, frontend::component as f};
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::{
    allocation as a,
    component::{bindings, execution, grants, idl, program, storage, wire},
    execution as e,
    layout::Immediate,
};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[path = "support/rust_artifacts.rs"]
mod artifacts;

const IDL: &[u8] = br#"{
  "idl":1,"package":"demo:embed","version":"1.0.0",
  "interfaces":[{"key":"demo:embed/api","resources":[],"types":[],
    "functions":[{"name":"answer","effect":"sync",
      "params":[{"name":"value","type":"i64"}],"result":"i64",
      "capability":"demo.embed.answer"}]}],
  "imports":[],"exports":["demo:embed/api"]
}"#;

fn source_input() -> String {
    let document = idl::check(m::SourceId(1), IDL, Default::default()).unwrap();
    let generated = g::guest(&document, Default::default()).unwrap();
    format!(
        "{}\n{} {{if p0 < 0 {{return core::panic::<i64>(77);}} return p0 + 1;}}",
        generated.declarations, generated.exports[0].declaration
    )
}

fn source_unit() -> bindings::Unit {
    let document = idl::check(m::SourceId(1), IDL, Default::default()).unwrap();
    let generated = g::guest(&document, Default::default()).unwrap();
    let source = source_input();
    let analysis = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-source.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document,
        bindings: generated.bindings,
        limits: Default::default(),
    });
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    analysis.result.unwrap()
}

fn artifact_input() -> Vec<u8> {
    lang::artifact::component::encode(
        source_unit(),
        m::Source {
            path: "component-idl.json".into(),
            text: std::str::from_utf8(IDL).unwrap().into(),
        },
        None,
        Default::default(),
    )
    .unwrap()
}

struct NativeCodec(PathBuf);
impl NativeCodec {
    fn new(module: &embedding::Module, native: bool) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "lang-component-input-{}-{native}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let this = Self(directory);
        let target = if cfg!(windows) {
            g::abi::Target::WindowsX86_64Msvc
        } else {
            g::abi::Target::MacosAarch64
        };
        let generated = g::c::generate(module.contract(), target, Default::default()).unwrap();
        let function = &generated.functions[0];
        let source = format!(
            r#"{}
#include <assert.h>
#include <stdio.h>
#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#endif
int main(int argc,char **argv) {{
    uint8_t bytes[1024];
    (void)argv;
#ifdef _WIN32
    assert(_setmode(_fileno(stdin),_O_BINARY)!=-1);
    assert(_setmode(_fileno(stdout),_O_BINARY)!=-1);
#endif
    if(argc==1) {{
        {} args={{0}};
        lc_value_writer w={{0}};
        args.p0=41;w.data=bytes;w.capacity=sizeof(bytes);
        w.left.work=10000;w.left.elements=1000;w.left.depth=128;w.contextual=1;
        assert({}_encode_arguments(&w,&args));
        assert(fwrite(bytes,1,(size_t)w.at,stdout)==w.at);
    }} else {{
        {} value=0;
        lc_value_reader r={{0}};
        r.data=bytes;r.length=fread(bytes,1,sizeof(bytes),stdin);
        r.left.work=10000;r.left.elements=1000;r.left.depth=128;r.contextual=1;
        assert({}_decode_result(&r,&value));
        assert(value==42);
    }}
    return 0;
}}
"#,
            generated.header, function.arguments, function.name, function.result, function.name
        );
        let mut child = Command::new("clang")
            .args(["-x", "c", "-std=c11", "-Wall", "-Wextra", "-Werror", "-o"])
            .arg(this.executable())
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        this
    }
    fn executable(&self) -> PathBuf {
        self.0
            .join(format!("codec{}", std::env::consts::EXE_SUFFIX))
    }
    fn arguments(&self) -> Vec<u8> {
        let output = Command::new(self.executable()).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }
    fn check(&self, bytes: &[u8]) {
        let mut child = Command::new(self.executable())
            .arg("result")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(bytes).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
impl Drop for NativeCodec {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn canonical_source_input_and_stored_artifact_execute_on_the_same_retained_vm_native_state() {
    for native in [false, true] {
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut budget = m::Budget::new(Default::default());
        let mut resources = storage::Resources::prepare(&allocator, 4, 4, &mut budget).unwrap();
        let mut state = execution::InstanceState::prepare(&mut resources).unwrap();
        let mut engine = lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
        let bytes = artifact_input();
        let modules = [
            embedding::load(0, source_input().as_bytes(), IDL, Default::default()).unwrap(),
            embedding::load(1, &bytes, &[], Default::default()).unwrap(),
        ];
        let codec = NativeCodec::new(&modules[0], native);
        let input = codec.arguments();
        for module in modules {
            let unit = module.unit();
            let mut session = wire::Session::new(Default::default());
            let providers = [];
            let bound =
                grants::prepare(unit, module.contract(), &[], || &providers, &mut session).unwrap();
            let program = program::Program::prepare(&bound, Default::default()).unwrap();
            let entry = unit.exports()[0].0;
            let control = e::Control::default();
            let mut invoker = engine.begin(control).unwrap();
            state
                .with_program(&program, |instance| {
                    let mut call = execution::Invocation::prepare_owned(
                        instance,
                        entry,
                        &mut resources,
                        &mut invoker,
                        &mut budget,
                    )
                    .unwrap();
                    let arguments = call
                        .decode_arguments(&input, &[], &allocator, &mut session)
                        .unwrap();
                    call.prepare_output(&mut session).unwrap();
                    let outcome = if native {
                        lang_codegen_clif::tasks::CompiledProgram::compile_component(
                            &program,
                            entry,
                            Default::default(),
                        )
                        .unwrap()
                        .execute_component(
                            &mut allocator,
                            control,
                            Default::default(),
                            &arguments,
                            &mut call,
                        )
                    } else {
                        lang_vm::unit::tasks::execute_component(
                            &program,
                            entry,
                            &mut allocator,
                            control,
                            Default::default(),
                            &arguments,
                            &mut call,
                        )
                    }
                    .unwrap();
                    match outcome.value {
                        execution::Value::Copy(Immediate::I64(42)) => {}
                        execution::Value::Frame(frame) => {
                            assert_eq!(frame.values().nodes, [wire::Value::I64(42)]);
                            assert!(frame.resources().is_empty());
                            codec.check(frame.bytes());
                        }
                        other => panic!("unexpected component result: {other:?}"),
                    }
                })
                .unwrap();
            assert!(!state.poisoned());
        }
    }
}

#[test]
fn module_input_caps_are_cumulative_and_preserve_the_original_idl_snapshot() {
    let source = source_input();
    let module = embedding::load(0, source.as_bytes(), IDL, Default::default()).unwrap();
    assert_eq!(module.idl_snapshot().text.as_bytes(), IDL);
    assert_eq!(module.unit().document().document().source, m::SourceId(1));
    assert_eq!(module.contract().document().source, m::SourceId(1));
    let used = module.usage();
    let mut limits = embedding::Limits::default();
    limits.preparation.work = used.work;
    limits.preparation.allocations = used.allocation_bytes;
    limits.preparation.elements = used.elements;
    assert_eq!(
        embedding::load(0, source.as_bytes(), IDL, limits)
            .unwrap()
            .usage(),
        used
    );
    for which in 0..3 {
        let mut below = limits;
        match which {
            0 => below.preparation.work -= 1,
            1 => below.preparation.allocations -= 1,
            _ => below.preparation.elements -= 1,
        }
        assert!(embedding::load(0, source.as_bytes(), IDL, below).is_err());
    }
    limits.input_bytes = source.len() + IDL.len() - 1;
    assert!(matches!(
        embedding::load(0, source.as_bytes(), IDL, limits),
        Err(embedding::Error::InputBytes { .. })
    ));
    assert!(matches!(
        embedding::load(19, b"", b"", Default::default()),
        Err(embedding::Error::Format(19))
    ));
    assert!(matches!(
        embedding::load(1, b"", IDL, Default::default()),
        Err(embedding::Error::ArtifactContract)
    ));
    assert!(matches!(
        embedding::load(0, source.as_bytes(), b"{", Default::default()),
        Err(embedding::Error::Idl(_))
    ));
    let wrong = source.replace("component_e_", "other_");
    assert!(matches!(
        embedding::load(0, wrong.as_bytes(), IDL, Default::default()),
        Err(embedding::Error::Source(_))
    ));
    let external = format!("mod external;\n{source}");
    assert!(matches!(
        embedding::load(0, external.as_bytes(), IDL, Default::default()),
        Err(embedding::Error::Source(_))
    ));
}

#[test]
fn artifact_input_retains_provenance_and_obeys_cumulative_and_structural_limits() {
    let bytes = artifact_input();
    let module = embedding::load(1, &bytes, &[], Default::default()).unwrap();
    assert_eq!(module.idl_snapshot().text.as_bytes(), IDL);
    let used = module.usage();
    let mut limits = embedding::Limits::default();
    limits.preparation.work = used.work;
    limits.preparation.allocations = used.allocation_bytes;
    limits.preparation.elements = used.elements;
    assert_eq!(
        embedding::load(1, &bytes, &[], limits).unwrap().usage(),
        used
    );
    for which in 0..3 {
        let mut below = limits;
        match which {
            0 => below.preparation.work -= 1,
            1 => below.preparation.allocations -= 1,
            _ => below.preparation.elements -= 1,
        }
        assert!(embedding::load(1, &bytes, &[], below).is_err());
    }
    let mut structural = embedding::Limits::default();
    structural
        .artifact
        .tasks
        .bodies
        .places
        .ownership
        .unit
        .functions = 0;
    assert!(matches!(
        embedding::load(1, &bytes, &[], structural),
        Err(embedding::Error::Artifact(_))
    ));
    for length in [0, 8, bytes.len() - 1] {
        assert!(embedding::load(1, &bytes[..length], &[], Default::default()).is_err());
    }
}

#[test]
fn input_refusal_retains_structured_source_and_idl_diagnostics_before_execution() {
    let source = source_input();
    let mut zero = embedding::Limits::default();
    zero.preparation.work = 0;
    assert!(matches!(
        embedding::load(0, source.as_bytes(), b"{", zero),
        Err(embedding::Error::Budget(_))
    ));
    assert!(matches!(
        embedding::load(0, source.as_bytes(), &[0xff], Default::default()),
        Err(embedding::Error::Utf8(_))
    ));
    let malformed = embedding::load(0, source.as_bytes(), b"{", Default::default());
    let Err(embedding::Error::Idl(error)) = malformed else {
        panic!("expected IDL diagnostic")
    };
    assert_eq!(error.span.source, m::SourceId(1));
    let malformed = source.replace("return p0 + 1;", "return missing;");
    let Err(embedding::Error::Source(analysis)) =
        embedding::load(0, malformed.as_bytes(), IDL, Default::default())
    else {
        panic!("expected source diagnostic")
    };
    assert!(!analysis.diagnostics.is_empty());
    let (_, original) = analysis.sources.iter().next().unwrap();
    assert_eq!(original.text(), malformed);
}

#[test]
fn native_c_host_loads_snapshots_and_repeats_generated_typed_calls_on_vm_and_jit() {
    native_c_host(false);
}

#[test]
fn previous_v1_c_client_runs_current_vm_jit_ownership_failures_resources_and_tasks() {
    native_c_host(true);
}

fn native_c_host(previous_client: bool) {
    let directory = std::env::temp_dir().join(format!(
        "lang-component-native-host-{}-{previous_client}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    let files = NativeCodec(directory);
    let source = source_input();
    let bytes = artifact_input();
    let module = embedding::load(0, source.as_bytes(), IDL, Default::default()).unwrap();
    let target = if cfg!(windows) {
        g::abi::Target::WindowsX86_64Msvc
    } else {
        g::abi::Target::MacosAarch64
    };
    let mut header = g::c::generate(module.contract(), target, Default::default()).unwrap();
    if previous_client {
        // Compile the historical declarations/codecs, not a regenerated substitute.
        // Their guards also select the old ABI for every later generated header.
        header.header = concat!(
            include_str!("fixtures/component-v1-40158d6/component-abi-v1.h"),
            "\n",
            include_str!("fixtures/component-v1-40158d6/embed-v1.h")
        )
        .into();
    }
    let function = &header.functions[0];
    let array = |name: &str, bytes: &[u8]| {
        format!(
            "static const uint8_t {name}[]={{ {} }};\n",
            bytes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let mut consumer: serde_json::Value = serde_json::from_slice(IDL).unwrap();
    consumer["package"] = serde_json::json!("demo:caller");
    consumer["exports"] = serde_json::json!([]);
    consumer["imports"] = serde_json::json!([{"interface":"demo:embed/api","version":{"exact":"1.0.0"},"members":["answer"]}]);
    let consumer = serde_json::to_vec(&consumer).unwrap();
    let mut projection_session = wire::Session::new(Default::default());
    let consumer = wire::Contract::new(
        idl::check(m::SourceId(71), &consumer, Default::default()).unwrap(),
        vec![],
        &mut projection_session,
    )
    .unwrap();
    let projection = wire::encode_contract(&consumer, &mut projection_session).unwrap();
    let malformed_source = format!(
        "// 日本語の元スナップショット\n{}",
        source.replace("return p0 + 1;", "return missing;")
    );
    assert_ne!(source, malformed_source);
    let source_error_start = malformed_source.find("missing").unwrap();
    let source_error_end = source_error_start + "missing".len();
    let malformed_source_digest =
        <sha2::Sha256 as sha2::Digest>::digest(malformed_source.as_bytes());
    let invalid_digest = <sha2::Sha256 as sha2::Digest>::digest(b"{");
    let invalid_utf8_digest = <sha2::Sha256 as sha2::Digest>::digest([0xff]);
    let invalid_idl = idl::check(m::SourceId(1), b"{", Default::default()).unwrap_err();
    let invalid_idl_start = invalid_idl.span.start;
    let invalid_idl_end = invalid_idl.span.end;
    let invalid_artifact = match lang::artifact::component::decode(b"{", Default::default()) {
        Err(lang::artifact::component::Error::Code(reason)) => reason.offset,
        _ => panic!("expected the original artifact decoder failure"),
    };
    let mut unsupported_artifact = bytes.clone();
    unsupported_artifact[8..10].copy_from_slice(&255_u16.to_le_bytes());
    let unsupported_artifact_digest = <sha2::Sha256 as sha2::Digest>::digest(&unsupported_artifact);
    let host_idl = br#"{
      "idl":1,"package":"demo:host","version":"1.0.0",
      "interfaces":[{"key":"demo:host/api","resources":[],"types":[],
        "functions":[{"name":"add","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"host.add"}]}],
      "imports":[],"exports":["demo:host/api"]
    }"#;
    let host_document = idl::check(m::SourceId(0), host_idl, Default::default()).unwrap();
    let mut host_session = wire::Session::new(Default::default());
    let host_contract = wire::Contract::new(host_document, vec![], &mut host_session).unwrap();
    let native_contract = wire::encode_contract(&host_contract, &mut host_session).unwrap();
    let native_signature = wire::Signature::new(
        &host_contract,
        &host_contract.document().interfaces[0].key.value,
        "add",
        &mut host_session,
    )
    .unwrap()
    .digest();
    let host_generated = g::c::generate(&host_contract, target, Default::default()).unwrap();
    let host_function = &host_generated.functions[0];
    let mut host_json: serde_json::Value = serde_json::from_slice(host_idl).unwrap();
    let mut script_json: serde_json::Value = serde_json::from_slice(IDL).unwrap();
    script_json["interfaces"]
        .as_array_mut()
        .unwrap()
        .push(host_json["interfaces"][0].clone());
    script_json["imports"] = serde_json::json!([{"interface":"demo:host/api","version":{"exact":"1.0.0"},"members":["add"]}]);
    let native_idl = serde_json::to_vec(&script_json).unwrap();
    let document = idl::check(m::SourceId(1), &native_idl, Default::default()).unwrap();
    let generated = g::guest(&document, Default::default()).unwrap();
    let native_source = format!(
        "{}\n{} {{return {}(p0);}}",
        generated.declarations,
        generated.exports[0].declaration,
        generated.bindings.imports[0].source
    );
    let native_root_call = format!("{}(p0)", generated.bindings.imports[0].source);
    let analysis = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-source.lang"),
        bytes: native_source.as_bytes(),
        root: None,
        compile: Default::default(),
        document,
        bindings: generated.bindings,
        limits: Default::default(),
    });
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let native_artifact = lang::artifact::component::encode(
        analysis.result.unwrap(),
        m::Source {
            path: "component-idl.json".into(),
            text: std::str::from_utf8(&native_idl).unwrap().into(),
        },
        None,
        Default::default(),
    )
    .unwrap();
    host_json["package"] = serde_json::json!("demo:grant");
    let stack_document = idl::check(m::SourceId(1), &native_idl, Default::default()).unwrap();
    let stack_guest = g::guest(&stack_document, Default::default()).unwrap();
    let native_stack_source = format!(
        "{}\n{} {{return trace_recur(p0, true, true);}}\nfn trace_recur(value: i64, again: bool, next: bool) -> i64 {{if again {{return trace_recur(value, next, false);}} return {}(value);}}",
        stack_guest.declarations,
        stack_guest.exports[0].declaration,
        stack_guest.bindings.imports[0].source,
    );
    let stack_analysis = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-stack.lang"),
        bytes: native_stack_source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: stack_document,
        bindings: stack_guest.bindings,
        limits: Default::default(),
    });
    assert!(
        stack_analysis.diagnostics.is_empty(),
        "{:?}",
        stack_analysis.diagnostics
    );
    let native_stack_artifact = lang::artifact::component::encode(
        stack_analysis.result.unwrap(),
        m::Source {
            path: "component-idl.json".into(),
            text: std::str::from_utf8(&native_idl).unwrap().into(),
        },
        None,
        Default::default(),
    )
    .unwrap();
    host_json["exports"] = serde_json::json!([]);
    let long_document = idl::check(m::SourceId(1), &native_idl, Default::default()).unwrap();
    let long_guest = g::guest(&long_document, Default::default()).unwrap();
    let native_long_source = format!(
        "{}\n{} {{return trace_long(p0, 0);}}\nfn trace_long(value: i64, depth: i64) -> i64 {{if depth < 70 {{return trace_long(value, depth + 1);}} return {}(value);}}",
        long_guest.declarations,
        long_guest.exports[0].declaration,
        long_guest.bindings.imports[0].source,
    );
    let long_analysis = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-long-stack.lang"),
        bytes: native_long_source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: long_document,
        bindings: long_guest.bindings,
        limits: Default::default(),
    });
    assert!(
        long_analysis.diagnostics.is_empty(),
        "{:?}",
        long_analysis.diagnostics
    );
    let native_long_artifact = lang::artifact::component::encode(
        long_analysis.result.unwrap(),
        m::Source {
            path: "component-idl.json".into(),
            text: std::str::from_utf8(&native_idl).unwrap().into(),
        },
        None,
        Default::default(),
    )
    .unwrap();
    let fail_document = idl::check(m::SourceId(1), &native_idl, Default::default()).unwrap();
    let fail_guest = g::guest(&fail_document, Default::default()).unwrap();
    let native_fail_source = format!(
        "{}\n{} {{let mut value: i64 = {}(41); if p0 < 0 {{return trace_panic(p0, true, true);}} if p0 < 42 {{return trace_fail(value, true, true);}} return value;}}\nfn trace_fail(value: i64, again: bool, next: bool) -> i64 {{if again {{return trace_fail(value, next, false);}} return trace_fail(value, true, true);}}\nfn trace_panic(value: i64, again: bool, next: bool) -> i64 {{if again {{return trace_panic(value, next, false);}} return core::panic::<i64>(value);}}",
        fail_guest.declarations,
        fail_guest.exports[0].declaration,
        fail_guest.bindings.imports[0].source,
    );
    let native_child_call = format!("{}(41)", fail_guest.bindings.imports[0].source);
    let fail_analysis = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-guest-failure.lang"),
        bytes: native_fail_source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: fail_document,
        bindings: fail_guest.bindings,
        limits: Default::default(),
    });
    assert!(
        fail_analysis.diagnostics.is_empty(),
        "{:?}",
        fail_analysis.diagnostics
    );
    let native_fail_artifact = lang::artifact::component::encode(
        fail_analysis.result.unwrap(),
        m::Source {
            path: "component-idl.json".into(),
            text: std::str::from_utf8(&native_idl).unwrap().into(),
        },
        None,
        Default::default(),
    )
    .unwrap();
    host_json["imports"] = script_json["imports"].clone();
    let grant_json = serde_json::to_vec(&host_json).unwrap();
    let grant = wire::Contract::new(
        idl::check(m::SourceId(2), &grant_json, Default::default()).unwrap(),
        vec![],
        &mut host_session,
    )
    .unwrap();
    let native_grant = wire::encode_contract(&grant, &mut host_session).unwrap();
    let registry_host = include_str!("support/component_registry_host.c")
        .replace("@HOST_ARGUMENTS@", &host_function.arguments)
        .replace("@HOST_RESULT@", &host_function.result)
        .replace("@HOST_NAME@", &host_function.name)
        .replace("@ROOT_ARGUMENTS@", &function.arguments)
        .replace("@ROOT_RESULT@", &function.result)
        .replace("@ROOT_NAME@", &function.name);
    let native_header = host_generated.header;
    let resource_host_json = serde_json::json!({
        "idl":1,"package":"demo:res","version":"1.0.0",
        "interfaces":[{"key":"demo:res/api","resources":["item"],"types":[],
            "functions":[{"name":"touch","effect":"sync","params":[{"name":"item","type":{"borrow-mut":"item"}}],
                "result":"i64","capability":"res.touch"}]}],
        "imports":[],"exports":["demo:res/api"]
    });
    let resource_host_idl = serde_json::to_vec(&resource_host_json).unwrap();
    let mut resource_session = wire::Session::new(Default::default());
    let resource_host = wire::Contract::new(
        idl::check(m::SourceId(0), &resource_host_idl, Default::default()).unwrap(),
        vec![],
        &mut resource_session,
    )
    .unwrap();
    let resource_host_contract =
        wire::encode_contract(&resource_host, &mut resource_session).unwrap();
    let resource_method_signature = wire::Signature::new(
        &resource_host,
        &resource_host.document().interfaces[0].key.value,
        "touch",
        &mut resource_session,
    )
    .unwrap()
    .digest();
    let mut resource_caller = resource_host_json.clone();
    resource_caller["package"] = serde_json::json!("demo:rescaller");
    resource_caller["interfaces"].as_array_mut().unwrap().push(serde_json::json!({
        "key":"demo:rescaller/api","resources":[],"types":[],
        "functions":[{"name":"run","effect":"sync","params":[{"name":"item","type":{"borrow-mut":"demo:res/api.item"}}],
            "result":"i64","capability":"res.run"}]
    }));
    resource_caller["imports"] = serde_json::json!([{"interface":"demo:res/api","version":{"exact":"1.0.0"},"members":["touch"]}]);
    resource_caller["exports"] = serde_json::json!(["demo:rescaller/api"]);
    resource_caller["interfaces"][1]["functions"].as_array_mut().unwrap().push(serde_json::json!({
        "name":"relay","effect":"sync","params":[{"name":"item","type":{"own":"demo:res/api.item"}}],
        "result":{"own":"demo:res/api.item"},"capability":"res.relay"
    }));
    for member in ["explode", "retire"] {
        resource_caller["interfaces"][1]["functions"].as_array_mut().unwrap().push(serde_json::json!({
            "name":member,"effect":"sync","params":[{"name":"item","type":{"own":"demo:res/api.item"}}],
            "result":"i64","capability":format!("res.{member}")
        }));
    }
    let resource_idl = serde_json::to_vec(&resource_caller).unwrap();
    let document = idl::check(m::SourceId(1), &resource_idl, Default::default()).unwrap();
    let guest = g::guest(&document, Default::default()).unwrap();
    let resource_run_member = guest
        .exports
        .iter()
        .position(|export| export.member.member == "run")
        .unwrap();
    let bodies = guest
        .exports
        .iter()
        .map(|export| {
            let body = match export.member.member.as_str() {
                "relay" => "return p0;".to_owned(),
                "explode" | "retire" => {
                    let parameters = export
                        .declaration
                        .split_once('(')
                        .unwrap()
                        .1
                        .split_once(')')
                        .unwrap()
                        .0;
                    let item_type = parameters.split_once(':').unwrap().1.trim();
                    format!(
                        "let mut item: {item_type} = p0; let mut value: i64 = {}(item); {}",
                        guest.bindings.imports[0].source,
                        if export.member.member == "explode" {
                            "return core::panic::<i64>(value);"
                        } else {
                            "return value;"
                        }
                    )
                }
                _ => format!("return {}(p0);", guest.bindings.imports[0].source),
            };
            format!("{} {{{body}}}", export.declaration)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let resource_program = format!("{}\n{bodies}", guest.declarations);
    let analysis = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-source.lang"),
        bytes: resource_program.as_bytes(),
        root: None,
        compile: Default::default(),
        document,
        bindings: guest.bindings,
        limits: Default::default(),
    });
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let resource_artifact = lang::artifact::component::encode(
        analysis.result.unwrap(),
        m::Source {
            path: "component-idl.json".into(),
            text: String::from_utf8(resource_idl.clone()).unwrap(),
        },
        None,
        Default::default(),
    )
    .unwrap();
    let mut resource_grant_json = resource_host_json.clone();
    resource_grant_json["package"] = serde_json::json!("demo:resgrant");
    resource_grant_json["imports"] = resource_caller["imports"].clone();
    resource_grant_json["exports"] = serde_json::json!([]);
    let resource_grant = wire::encode_contract(
        &wire::Contract::new(
            idl::check(
                m::SourceId(2),
                &serde_json::to_vec(&resource_grant_json).unwrap(),
                Default::default(),
            )
            .unwrap(),
            vec![],
            &mut resource_session,
        )
        .unwrap(),
        &mut resource_session,
    )
    .unwrap();
    let mut resource_request_json = resource_caller.clone();
    resource_request_json["package"] = serde_json::json!("demo:resclient");
    resource_request_json["exports"] = serde_json::json!([]);
    resource_request_json["imports"] = serde_json::json!([{"interface":"demo:rescaller/api","version":{"exact":"1.0.0"},"members":["run"]}]);
    let resource_projection = wire::encode_contract(
        &wire::Contract::new(
            idl::check(
                m::SourceId(3),
                &serde_json::to_vec(&resource_request_json).unwrap(),
                Default::default(),
            )
            .unwrap(),
            vec![],
            &mut resource_session,
        )
        .unwrap(),
        &mut resource_session,
    )
    .unwrap();
    let resource_codecs = wire::Contract::new(
        idl::check(m::SourceId(4), &resource_idl, Default::default()).unwrap(),
        vec![],
        &mut resource_session,
    )
    .unwrap();
    resource_request_json["imports"][0]["members"] = serde_json::json!(["relay"]);
    let resource_relay_projection = wire::encode_contract(
        &wire::Contract::new(
            idl::check(
                m::SourceId(3),
                &serde_json::to_vec(&resource_request_json).unwrap(),
                Default::default(),
            )
            .unwrap(),
            vec![],
            &mut resource_session,
        )
        .unwrap(),
        &mut resource_session,
    )
    .unwrap();
    let mut failure_projection = |member: &str| {
        resource_request_json["imports"][0]["members"] = serde_json::json!([member]);
        wire::encode_contract(
            &wire::Contract::new(
                idl::check(
                    m::SourceId(3),
                    &serde_json::to_vec(&resource_request_json).unwrap(),
                    Default::default(),
                )
                .unwrap(),
                vec![],
                &mut resource_session,
            )
            .unwrap(),
            &mut resource_session,
        )
        .unwrap()
    };
    let resource_explode_projection = failure_projection("explode");
    let resource_retire_projection = failure_projection("retire");
    let resource_header = g::c::generate(&resource_codecs, target, Default::default()).unwrap();
    let resource_host_header = g::c::generate(&resource_host, target, Default::default()).unwrap();
    let resource_touch = resource_host_header
        .functions
        .iter()
        .find(|f| f.member.member == "touch")
        .unwrap();
    let resource_run = resource_header
        .functions
        .iter()
        .find(|f| f.member.member == "run")
        .unwrap();
    let resource_relay = resource_header
        .functions
        .iter()
        .find(|f| f.member.member == "relay")
        .unwrap();
    let resource_explode = resource_header
        .functions
        .iter()
        .find(|f| f.member.member == "explode")
        .unwrap();
    let resource_retire = resource_header
        .functions
        .iter()
        .find(|f| f.member.member == "retire")
        .unwrap();
    let resources_host = include_str!("support/component_resources_host.c")
        .replace("@RESOURCE_HOST_ARGS@", &resource_touch.arguments)
        .replace("@RESOURCE_HOST_RESULT@", &resource_touch.result)
        .replace("@RESOURCE_HOST_NAME@", &resource_touch.name)
        .replace("@RESOURCE_ROOT_ARGS@", &resource_run.arguments)
        .replace("@RESOURCE_ROOT_RESULT@", &resource_run.result)
        .replace("@RESOURCE_ROOT_NAME@", &resource_run.name)
        .replace("@RESOURCE_RELAY_ARGS@", &resource_relay.arguments)
        .replace("@RESOURCE_RELAY_RESULT@", &resource_relay.result)
        .replace("@RESOURCE_RELAY_NAME@", &resource_relay.name)
        .replace("@RESOURCE_EXPLODE_ARGS@", &resource_explode.arguments)
        .replace("@RESOURCE_EXPLODE_NAME@", &resource_explode.name)
        .replace("@RESOURCE_RETIRE_ARGS@", &resource_retire.arguments)
        .replace("@RESOURCE_RETIRE_NAME@", &resource_retire.name);
    let task_json = serde_json::json!({
        "idl":1,"package":"demo:callback-task","version":"1.0.0",
        "interfaces":[{"key":"demo:callback-task/api","resources":[],"types":[],"functions":[
            {"name":"later","effect":"task","params":[],"result":"i64","capability":"demo.callback.later"}
        ]}],"imports":[],"exports":["demo:callback-task/api"]
    });
    let task_idl = serde_json::to_vec(&task_json).unwrap();
    let task_document = idl::check(m::SourceId(1), &task_idl, Default::default()).unwrap();
    let task_guest = g::guest(&task_document, Default::default()).unwrap();
    let task_source = format!(
        "{}\nasync fn task_worker() -> i64 {{ let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); let mut pushed: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(bytes, 42)); yield; return core::bytes_len(bytes) + 41; }}\n{} {{ let mut child: core::Task<i64> = spawn task_worker(); let mut value: core::Result<i64, core::TaskError> = await child; return core::result_unwrap::<i64, core::TaskError>(value); }}",
        task_guest.declarations, task_guest.exports[0].declaration
    );
    let task_analysis = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-source.lang"),
        bytes: task_source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: task_document,
        bindings: task_guest.bindings,
        limits: Default::default(),
    });
    assert!(
        task_analysis.diagnostics.is_empty(),
        "{:?}",
        task_analysis.diagnostics
    );
    let task_artifact = lang::artifact::component::encode(
        task_analysis.result.unwrap(),
        m::Source {
            path: "component-idl.json".into(),
            text: String::from_utf8(task_idl.clone()).unwrap(),
        },
        None,
        Default::default(),
    )
    .unwrap();
    let mut task_consumer = task_json;
    task_consumer["interfaces"][0]["functions"][0]["effect"] = serde_json::json!("sync");
    task_consumer["package"] = serde_json::json!("demo:task-consumer");
    task_consumer["exports"] = serde_json::json!([]);
    task_consumer["imports"] = serde_json::json!([{"interface":"demo:callback-task/api","version":{"exact":"1.0.0"},"members":["later"]}]);
    let task_contract = wire::Contract::new(
        idl::check(
            m::SourceId(1),
            &serde_json::to_vec(&task_consumer).unwrap(),
            Default::default(),
        )
        .unwrap(),
        vec![],
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    let task_projection =
        wire::encode_contract(&task_contract, &mut wire::Session::new(Default::default())).unwrap();
    task_consumer["interfaces"][0]["functions"][0]["effect"] = serde_json::json!("task");
    task_consumer["package"] = serde_json::json!("demo:callback-task");
    task_consumer["imports"] = serde_json::json!([]);
    task_consumer["exports"] = serde_json::json!(["demo:callback-task/api"]);
    let task_root_contract = wire::Contract::new(
        idl::check(
            m::SourceId(1),
            &serde_json::to_vec(&task_consumer).unwrap(),
            Default::default(),
        )
        .unwrap(),
        vec![],
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    let task_root_projection = wire::encode_contract(
        &task_root_contract,
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    let task_module =
        embedding::load(0, task_source.as_bytes(), &task_idl, Default::default()).unwrap();
    let task_header = g::c::generate(task_module.contract(), target, Default::default()).unwrap();
    let task_function = &task_header.functions[0];
    let tasks_host = include_str!("support/component_tasks_host.c")
        .replace("@TASK_ARGUMENTS@", &task_function.arguments)
        .replace("@TASK_RESULT@", &task_function.result)
        .replace("@TASK_NAME@", &task_function.name);
    let mut task_refusals = String::new();
    for case in 0..8 {
        let mut request = task_consumer.clone();
        match case {
            0 => {
                request["interfaces"][0]["functions"][0]["capability"] =
                    serde_json::json!("wrong.capability")
            }
            1 => request["version"] = serde_json::json!("2.0.0"),
            2 => request["interfaces"][0]["functions"][0]["result"] = serde_json::json!("bool"),
            3 => request["interfaces"][0]["functions"][0]["name"] = serde_json::json!("missing"),
            4 => request["interfaces"][0]["functions"][0]["effect"] = serde_json::json!("sync"),
            5 => request["exports"] = serde_json::json!([]),
            6 => {
                let mut extra = request["interfaces"][0]["functions"][0].clone();
                extra["name"] = serde_json::json!("other");
                request["interfaces"][0]["functions"]
                    .as_array_mut()
                    .unwrap()
                    .push(extra);
            }
            7 => {
                request["interfaces"][0]["key"] = serde_json::json!("demo:callback-task/other");
                request["exports"] = serde_json::json!(["demo:callback-task/other"]);
            }
            _ => unreachable!(),
        }
        let checked = idl::check(
            m::SourceId(1),
            &serde_json::to_vec(&request).unwrap(),
            Default::default(),
        )
        .unwrap();
        let mut session = wire::Session::new(Default::default());
        let contract = wire::Contract::new(checked, vec![], &mut session).unwrap();
        task_refusals += &array(
            &format!("task_refusal_{case}"),
            &wire::encode_contract(&contract, &mut session).unwrap(),
        );
    }
    task_refusals += "static const lang_component_v1_bytes task_refusals[]={";
    for case in 0..8 {
        task_refusals += &format!("{{task_refusal_{case},sizeof(task_refusal_{case})}},");
    }
    task_refusals += "};\n";
    // The task-root request must not broaden ordinary IDL import validity.
    let mut task_import = task_consumer.clone();
    task_import["exports"] = serde_json::json!([]);
    task_import["imports"] = serde_json::json!([{"interface":"demo:callback-task/api","version":{"exact":"1.0.0"},"members":["later"]}]);
    assert!(matches!(
        idl::check(
            m::SourceId(1),
            &serde_json::to_vec(&task_import).unwrap(),
            Default::default()
        )
        .unwrap_err()
        .reason,
        idl::Reason::InvalidEffect
    ));
    let declarations = array("task_source", task_source.as_bytes())
        + &array("task_idl", &task_idl)
        + &array("task_artifact", &task_artifact)
        + &array("task_projection", &task_projection)
        + &array("task_root_projection", &task_root_projection)
        + &array("source", source.as_bytes())
        + &array("idl", IDL)
        + &array("artifact", &bytes)
        + &array("projection", &projection)
        + &array("malformed_source", malformed_source.as_bytes())
        + &array("malformed_source_digest", &malformed_source_digest)
        + &array("invalid_digest", &invalid_digest)
        + &array("invalid_utf8_digest", &invalid_utf8_digest)
        + &array("unsupported_artifact", &unsupported_artifact)
        + &array("unsupported_artifact_digest", &unsupported_artifact_digest)
        + &array("native_contract", &native_contract)
        + &array("native_signature", &native_signature)
        + &array("native_key", b"demo:host/api.add")
        + &array("native_idl", &native_idl)
        + &array("native_source", native_source.as_bytes())
        + &array("native_artifact", &native_artifact)
        + &array("native_stack_source", native_stack_source.as_bytes())
        + &array("native_stack_artifact", &native_stack_artifact)
        + &array("native_long_source", native_long_source.as_bytes())
        + &array("native_long_artifact", &native_long_artifact)
        + &array("native_fail_source", native_fail_source.as_bytes())
        + &array("native_fail_artifact", &native_fail_artifact)
        + &array("native_grant", &native_grant)
        + &array("resource_host_contract", &resource_host_contract)
        + &array("resource_method_signature", &resource_method_signature)
        + &array("resource_method_key", b"demo:res/api.touch")
        + &array("resource_key", b"demo:res/api.item")
        + &array("resource_grant", &resource_grant)
        + &array("resource_projection", &resource_projection)
        + &array("resource_relay_projection", &resource_relay_projection)
        + &array("resource_explode_projection", &resource_explode_projection)
        + &array("resource_retire_projection", &resource_retire_projection)
        + &array("resource_program", resource_program.as_bytes())
        + &array("resource_idl", &resource_idl)
        + &array("resource_artifact", &resource_artifact);
    let c = format!(
        r#"
#define _POSIX_C_SOURCE 200112L
{header}
{native_header}
{resource_header}
{task_header}
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#ifdef _WIN32
#include <malloc.h>
#endif
{declarations}
{task_refusals}
extern void *probe_load(uint32_t,const uint8_t *,uint64_t,const uint8_t *,uint64_t,lang_component_v1_allocator);
extern uint32_t probe_call(void *,uint32_t,const uint8_t *,uint64_t,uint8_t *,uint64_t,uint64_t *);
extern void probe_drop(void *);
extern uint32_t probe_engine_lifecycle(const lang_component_v1_config *,uint32_t,lang_component_v1_bytes,lang_component_v1_bytes);
extern uint32_t probe_engine_create(const lang_component_v1_config *,lang_component_v1_engine **,lang_component_v1_error *);
extern uint32_t probe_engine_destroy(lang_component_v1_engine *,lang_component_v1_error *);
extern uint32_t probe_module_load(lang_component_v1_engine *,uint32_t,lang_component_v1_bytes,lang_component_v1_bytes,lang_component_v1_module **,lang_component_v1_error *);
extern uint32_t probe_module_drop(lang_component_v1_module *,lang_component_v1_error *);
extern uint32_t probe_instance_create(lang_component_v1_module *,const lang_component_v1_instance_config *,lang_component_v1_instance **,lang_component_v1_error *);
extern uint32_t probe_instance_close(lang_component_v1_instance *,lang_component_v1_result **,lang_component_v1_error *);
extern uint32_t probe_bind_export(lang_component_v1_instance *,lang_component_v1_bytes,lang_component_v1_binding **,lang_component_v1_error *);
extern uint32_t probe_binding_drop(lang_component_v1_binding *,lang_component_v1_error *);
extern uint32_t probe_control_create(lang_component_v1_engine *,const lang_component_v1_limits *,lang_component_v1_control **,lang_component_v1_error *);
extern uint32_t probe_control_drop(lang_component_v1_control *,lang_component_v1_error *);
extern void probe_interrupt(lang_component_v1_control *);
extern uint32_t probe_api_call(lang_component_v1_binding *,lang_component_v1_control *,lang_component_v1_bytes,lang_component_v1_result **,lang_component_v1_error *);
extern uint32_t probe_context_call(const lang_component_v1_context *,lang_component_v1_binding *,lang_component_v1_bytes,lang_component_v1_result **,lang_component_v1_error *);
extern uint32_t probe_context_thread(const lang_component_v1_context *,lang_component_v1_binding *);
extern uint32_t probe_native_failure_report(const lang_component_v1_context *,lang_component_v1_buffer **,uint32_t);
extern uint32_t probe_exhaust_preparation(const lang_component_v1_context *,uint32_t);
extern uint32_t probe_exhaust_resource_preparation(const lang_component_v1_context *,uint32_t);
extern uint32_t probe_result_info(const lang_component_v1_result *,lang_component_v1_result_info *,lang_component_v1_error *);
extern uint32_t probe_result_usage(const lang_component_v1_result *,lang_component_v1_usage *,lang_component_v1_error *);
extern uint32_t probe_result_bytes(const lang_component_v1_result *,lang_component_v1_bytes *,lang_component_v1_error *);
extern uint32_t probe_result_drop(lang_component_v1_result *,lang_component_v1_error *);
extern uint32_t probe_report(lang_component_v1_bytes,uint32_t,uint32_t);
extern uint32_t probe_task_report(lang_component_v1_bytes,uint32_t,uint32_t,uint64_t);
extern uint32_t probe_provider_register(lang_component_v1_engine *,const lang_component_v1_provider_config *,lang_component_v1_provider **,lang_component_v1_error *);
extern uint32_t probe_provider_drop(lang_component_v1_provider *,lang_component_v1_result **,lang_component_v1_error *);
extern uint32_t probe_context_check(const lang_component_v1_context *,lang_component_v1_error *);
extern uint32_t probe_context_buffer_create(const lang_component_v1_context *,uint64_t,uint64_t,lang_component_v1_buffer **,lang_component_v1_error *);
extern uint32_t probe_context_buffer_drop(const lang_component_v1_context *,lang_component_v1_buffer *,lang_component_v1_error *);
extern uint32_t probe_buffer_mut_bytes(const lang_component_v1_context *,lang_component_v1_buffer *,lang_component_v1_mut_bytes *,lang_component_v1_error *);
extern uint32_t probe_resource_publish(lang_component_v1_instance *,lang_component_v1_provider *,lang_component_v1_bytes,lang_component_v1_bytes,uint64_t,uint32_t *,lang_component_v1_error *);
extern uint32_t probe_resource_drop(lang_component_v1_instance *,uint32_t,lang_component_v1_result **,lang_component_v1_error *);
extern uint32_t probe_context_resource_write(const lang_component_v1_context *,uint32_t,lang_component_v1_mut_bytes *,lang_component_v1_error *);
extern uint32_t probe_context_resource_read(const lang_component_v1_context *,uint32_t,lang_component_v1_bytes *,lang_component_v1_error *);
extern uint32_t probe_context_resource_keep(const lang_component_v1_context *,uint32_t,uint32_t *,lang_component_v1_error *);
extern uint32_t probe_context_resource_take(const lang_component_v1_context *,uint32_t,uint32_t *,lang_component_v1_error *);
extern uint32_t probe_context_resource_publish(const lang_component_v1_context *,lang_component_v1_bytes,lang_component_v1_buffer *,uint32_t *,lang_component_v1_error *);
{tasks_host}
{registry_host}
{resources_host}
static void bound_case(const lang_component_v1_api *api,const lang_component_v1_config *config,uint32_t format,lang_component_v1_bytes program,lang_component_v1_bytes contract,uint32_t limited) {{
    lang_component_v1_engine *engine=NULL;lang_component_v1_module *module=NULL;
    lang_component_v1_instance *instance=NULL;lang_component_v1_binding *binding=NULL;
    lang_component_v1_error error={{0}};lang_component_v1_instance_config options={{0}};
    lang_component_v1_bytes request={{projection,sizeof(projection)}};
    uint8_t *p=(uint8_t *)malloc((size_t)program.length),*c=contract.length?(uint8_t *)malloc((size_t)contract.length):NULL;
    uint32_t repeat;assert(p);memcpy(p,program.data,(size_t)program.length);program.data=p;
    if(contract.length){{assert(c);memcpy(c,contract.data,(size_t)contract.length);contract.data=c;}}
    error.header.size=sizeof(error);error.header.version=1;
    options.header.size=sizeof(options);options.header.version=1;options.ceiling=config->ceiling;
    assert(api->header.version==0); /* Private fixture; no version-1 publication. */
    assert(api->engine_create(config,&engine,&error)==0);
    {{
        lang_component_v1_module *untouched=(lang_component_v1_module *)(uintptr_t)42;
        assert(api->module_load(engine,99,program,contract,&untouched,&error)==2);
        assert(untouched==(lang_component_v1_module *)(uintptr_t)42&&error.diagnostic==1);
        assert(error.flags==0&&error.detail_code==0&&error.limit==0&&error.attempted==0);
    }}
    {{
        const uint8_t invalid[]={{123}};
        lang_component_v1_bytes bad={{invalid,sizeof(invalid)}};
        lang_component_v1_module *untouched=(lang_component_v1_module *)(uintptr_t)42;
        memset(&error,0xa5,sizeof(error));error.header.size=sizeof(error);error.header.version=1;
        assert(api->module_load(engine,format,format?bad:program,format?contract:bad,&untouched,&error)==2);
        assert(untouched==(lang_component_v1_module *)(uintptr_t)42);
        assert(error.diagnostic==5&&error.flags==(format?7u:6u));
        assert(error.detail_code==(format?0u:1u)&&error.source_id==(format?0u:1u));
        assert(error.start==(format?{invalid_artifact}u:{invalid_idl_start}u));
        assert(error.end==(format?{invalid_artifact}u:{invalid_idl_end}u));
        assert(memcmp(error.input_digest,invalid_digest,32)==0);
        assert(error.limit==0&&error.attempted==0&&error.limit_kind==0);
        assert(error.reserved0==0&&error.reserved[0]==0&&error.reserved[1]==0);
        assert(error.header.size==sizeof(error)&&error.header.version==1);
    }}
    if(format==0){{
        lang_component_v1_bytes bad={{malformed_source,sizeof(malformed_source)}};
        lang_component_v1_module *untouched=(lang_component_v1_module *)(uintptr_t)42;
        assert(api->module_load(engine,format,bad,contract,&untouched,&error)==2);
        assert(untouched==(lang_component_v1_module *)(uintptr_t)42);
        assert(error.diagnostic==5&&error.flags==5&&error.detail_code==2&&error.source_id==0);
        assert(error.start=={source_error_start}&&error.end=={source_error_end});
        assert(memcmp(error.input_digest,malformed_source_digest,32)==0);
        assert(error.limit==0&&error.attempted==0&&error.limit_kind==0);
    }}
    if(format==0){{
        const uint8_t invalid[]={{255}};
        lang_component_v1_bytes bad={{invalid,sizeof(invalid)}};
        lang_component_v1_module *untouched=(lang_component_v1_module *)(uintptr_t)42;
        assert(api->module_load(engine,format,program,bad,&untouched,&error)==2);
        assert(untouched==(lang_component_v1_module *)(uintptr_t)42);
        assert(error.diagnostic==5&&error.flags==6&&error.detail_code==1&&error.source_id==1);
        assert(error.start==0&&error.end==1&&memcmp(error.input_digest,invalid_utf8_digest,32)==0);
    }} else {{
        lang_component_v1_bytes bad={{unsupported_artifact,sizeof(unsupported_artifact)}};
        lang_component_v1_module *untouched=(lang_component_v1_module *)(uintptr_t)42;
        assert(api->module_load(engine,format,bad,contract,&untouched,&error)==3);
        assert(untouched==(lang_component_v1_module *)(uintptr_t)42);
        assert(error.diagnostic==2&&error.flags==7&&error.source_id==0&&error.detail_code==0);
        assert(error.start==8&&error.end==8&&error.limit==0&&error.attempted==0);
        assert(memcmp(error.input_digest,unsupported_artifact_digest,32)==0);
    }}
    for(repeat=0;repeat<2;++repeat){{
        const uint8_t zero[32]={{0}};
        uint64_t length=repeat?config->ceiling.preparation_work*3/5:config->ceiling.metadata_bytes+1;
        uint64_t attempted=length+contract.length;
        uint8_t *large=(uint8_t *)calloc(1,(size_t)length);
        lang_component_v1_bytes bad={{large,length}};
        lang_component_v1_module *untouched=(lang_component_v1_module *)(uintptr_t)42;
        assert(large);memset(&error,0xa5,sizeof(error));error.header.size=sizeof(error);error.header.version=1;
        assert(api->module_load(engine,format,bad,contract,&untouched,&error)==2);
        assert(untouched==(lang_component_v1_module *)(uintptr_t)42);
        assert(error.diagnostic==12&&error.limit_kind==(repeat?9u:10u));
        assert(error.limit==(repeat?config->ceiling.preparation_work-attempted:config->ceiling.metadata_bytes));
        assert(error.attempted==attempted&&error.flags==0&&error.detail_code==0);
        assert(error.source_id==0&&error.start==0&&error.end==0);
        assert(memcmp(error.input_digest,zero,32)==0);
        assert(error.reserved0==0&&error.reserved[0]==0&&error.reserved[1]==0);
        free(large);
    }}
    assert(api->module_load(engine,format,program,contract,&module,&error)==0);
    assert(api->instance_create(module,&options,&instance,&error)==0);
    assert(api->bind_export(instance,request,&binding,&error)==0);
    assert(api->module_drop(module,&error)==0);
    memset(p,0,(size_t)program.length);free(p);if(c){{memset(c,0,(size_t)contract.length);free(c);}}
    assert(api->engine_destroy(engine,&error)==4);
    for(repeat=0;repeat<2;++repeat){{
        lang_component_v1_control *control=NULL;lang_component_v1_bytes bytes={{0}};
        lang_component_v1_usage usage={{0}};lang_component_v1_result *untouched=(lang_component_v1_result *)(uintptr_t)42;
        uint8_t scratch[1024];{arguments_type} args={{0}};{result_type} result=0;
        lc_value_writer w={{0}};lc_value_reader r={{0}};lc_value_outcome out;
        usage.header.size=sizeof(usage);usage.header.version=1;
        assert(api->control_create(engine,&config->ceiling,&control,&error)==0);
        args.p0=41;w.data=scratch;w.capacity=sizeof(scratch);w.left.work=1000000;w.left.elements=1000;w.left.depth=128;
        if(limited){{memset(&error,0xa5,sizeof(error));error.header.size=sizeof(error);error.header.version=1;}}
        out={name}_call(api,control,binding,&args,&w,&error);
        if(limited){{
            assert(out.commit==0&&!out.poisoned);
            if(out.result){{
                assert(out.status==1&&api->result_bytes(out.result,&bytes,&error)==0);
                assert(probe_report(bytes,config->mode,13)==0);
                assert(api->result_drop(out.result,&error)==0);
            }}else{{
                assert(out.status==2&&error.diagnostic==12&&error.limit_kind==10);
                assert(error.limit==1048576&&error.attempted>error.limit);
                assert(error.flags==0&&error.detail_code==0&&error.reserved0==0);
            }}
            assert(api->control_drop(control,&error)==0);continue;
        }}
        assert(out.status==0&&out.commit==1&&out.poisoned==0&&out.result);
        assert(api->result_bytes(out.result,&bytes,&error)==0);
        r.data=bytes.data;r.length=bytes.length;r.left.work=1000000;r.left.elements=1000;r.left.depth=128;r.contextual=1;
        assert({name}_decode_result(&r,&result)&&result==42);
        assert(api->result_usage(out.result,&usage,&error)==0&&usage.fuel>0&&usage.service_work>0);
        {{lang_component_v1_bytes input={{scratch,w.at}};assert(api->call(binding,control,input,&untouched,&error)==2);assert(untouched==(lang_component_v1_result *)(uintptr_t)42);}}
        assert(api->result_drop(out.result,&error)==0);
        assert(api->control_drop(control,&error)==0);
    }}
    if(limited)goto bound_cleanup;
    for(repeat=0;repeat<4;++repeat){{
        lang_component_v1_control *control=NULL;lang_component_v1_limits limits=config->ceiling;
        lang_component_v1_bytes bytes;lang_component_v1_result_info info={{0}};
        uint8_t scratch[1024];{arguments_type} args={{0}};lc_value_writer w={{0}};lc_value_outcome out;
        if(repeat==1)limits.fuel=0;if(repeat==3)limits.service_work=0;
        assert(api->control_create(engine,&limits,&control,&error)==0);
        if(repeat==2)api->interrupt(control);
        args.p0=repeat==0?-1:41;w.data=scratch;w.capacity=sizeof(scratch);w.left.work=1000000;w.left.elements=1000;w.left.depth=128;
        out={name}_call(api,control,binding,&args,&w,&error);
        assert(out.status==1&&out.result&&out.poisoned==0);
        if(repeat==0)assert(out.commit==1);
        if(repeat>=2)assert(out.commit==0);
        info.header.size=sizeof(info);info.header.version=1;
        assert(api->result_info(out.result,&info,&error)==0&&info.kind==1&&info.frame_count==(repeat<2?1u:0u));
        assert(api->result_bytes(out.result,&bytes,&error)==0);
        assert(probe_report(bytes,config->mode,repeat)==0);
        assert(api->result_drop(out.result,&error)==0);
        assert(api->control_drop(control,&error)==0);
    }}
    {{
        lang_component_v1_control *control=NULL;lang_component_v1_bytes bytes;
        uint8_t scratch[1024];{arguments_type} args={{0}};{result_type} result=0;
        lc_value_writer w={{0}};lc_value_reader r={{0}};lc_value_outcome out;
        assert(api->control_create(engine,&config->ceiling,&control,&error)==0);
        args.p0=41;w.data=scratch;w.capacity=sizeof(scratch);w.left.work=1000000;w.left.elements=1000;w.left.depth=128;
        out={name}_call(api,control,binding,&args,&w,&error);assert(out.status==0&&out.result);
        assert(api->result_bytes(out.result,&bytes,&error)==0);
        r.data=bytes.data;r.length=bytes.length;r.left.work=1000000;r.left.elements=1000;r.left.depth=128;r.contextual=1;
        assert({name}_decode_result(&r,&result)&&result==42);
        assert(api->result_drop(out.result,&error)==0);assert(api->control_drop(control,&error)==0);
    }}
bound_cleanup:
    {{lang_component_v1_result *closed=(lang_component_v1_result *)(uintptr_t)42;assert(api->instance_close(instance,&closed,&error)==0&&closed==NULL);}}
    assert(api->binding_drop(binding,&error)==0);
    assert(api->engine_destroy(engine,&error)==0);
}}
static void decoder_refusal_case(const lang_component_v1_api *api,const lang_component_v1_config *original,uint32_t format,lang_component_v1_bytes program,lang_component_v1_bytes metadata) {{
    lang_component_v1_config config=*original;lang_component_v1_engine *engine=NULL;
    lang_component_v1_error error={{0}};uint32_t repeat;
    config.ceiling.decoder_allocation_bytes=1;
    error.header.size=sizeof(error);error.header.version=1;
    assert(api->engine_create(&config,&engine,&error)==0);
    for(repeat=0;repeat<2;++repeat){{
        lang_component_v1_module *untouched=(lang_component_v1_module *)(uintptr_t)42;
        memset(&error,0xa5,sizeof(error));error.header.size=sizeof(error);error.header.version=1;
        assert(api->module_load(engine,format,program,metadata,&untouched,&error)==2);
        assert(untouched==(lang_component_v1_module *)(uintptr_t)42);
        assert(error.diagnostic==12&&error.limit_kind==14&&error.limit==1&&error.attempted>1);
        assert(error.reserved0==0&&error.reserved[0]==0&&error.reserved[1]==0);
    }}
    assert(api->engine_destroy(engine,&error)==0);
}}
typedef struct request {{void *data;uint64_t size,alignment;uint32_t live;}} request;
typedef struct owner {{request *requests;uint64_t capacity,allocated,released,live;}} owner;
static uint8_t *LANG_COMPONENT_V1_CALL allocate(void *user,uint64_t size,uint64_t alignment) {{
    owner *o=(owner *)user;void *data=NULL;assert(size&&alignment);
    if(o->allocated==o->capacity){{
        uint64_t capacity=o->capacity?o->capacity*2:8192;
        request *requests;assert(capacity>o->capacity&&capacity<=SIZE_MAX/sizeof(request));
        requests=(request *)realloc(o->requests,(size_t)capacity*sizeof(request));assert(requests);
        o->requests=requests;o->capacity=capacity;
    }}
#ifdef _WIN32
    data=_aligned_malloc((size_t)size,(size_t)(alignment<sizeof(void *)?sizeof(void *):alignment));
#else
    if(posix_memalign(&data,(size_t)(alignment<sizeof(void *)?sizeof(void *):alignment),(size_t)size)!=0)return NULL;
#endif
    if(!data)return NULL;
    memset(data,0xa5,(size_t)size);
    o->requests[o->allocated].data=data;o->requests[o->allocated].size=size;
    o->requests[o->allocated].alignment=alignment;o->requests[o->allocated].live=1;
    ++o->allocated;++o->live;return data;
}}
static void LANG_COMPONENT_V1_CALL deallocate(void *user,uint8_t *data,uint64_t size,uint64_t alignment) {{
    owner *o=(owner *)user;uint64_t i;uint32_t found=0;
    for(i=0;i<o->allocated;++i)if(o->requests[i].data==data&&o->requests[i].live){{
        assert(o->requests[i].size==size&&o->requests[i].alignment==alignment);
        o->requests[i].live=0;found=1;break;
    }}
    assert(found);++o->released;--o->live;
#ifdef _WIN32
    _aligned_free(data);
#else
    free(data);
#endif
}}
static void checkpoint(uint32_t format,uint32_t mode,const char *stage) {{
    fprintf(stderr,"component fixture format=%u mode=%u stage=%s\n",format,mode,stage);
    fflush(stderr);
}}
uint32_t fixture(void) {{
    uint32_t format,mode,repeat;uint64_t before;
    owner *o=(owner *)calloc(1,sizeof(owner));assert(o);
    lang_component_v1_allocator allocator={{{{sizeof(lang_component_v1_allocator),1}},o,allocate,deallocate,{{0,0}}}};
    before=o->allocated;
    assert(probe_load(99,source,sizeof(source),idl,sizeof(idl),allocator)==NULL);
    assert(o->allocated==before);
    for(mode=0;mode<5;++mode){{
        lang_component_v1_allocator bad=allocator;
        if(mode==0)bad.header.version=2;
        else if(mode==1)--bad.header.size;
        else if(mode==2)bad.reserved[0]=1;
        else if(mode==3)bad.allocate=NULL;
        else bad.deallocate=NULL;
        assert(probe_load(0,source,sizeof(source),idl,sizeof(idl),bad)==NULL);
        assert(o->allocated==before);
    }}
    for(format=0;format<2;++format){{
        uint64_t code_length=format?sizeof(artifact):sizeof(source);
        uint64_t idl_length=format?0:sizeof(idl);
        uint8_t *code=(uint8_t *)malloc((size_t)code_length);
        uint8_t *contract=idl_length?(uint8_t *)malloc((size_t)idl_length):NULL;
        void *module;assert(code);memcpy(code,format?artifact:source,(size_t)code_length);
        if(idl_length){{assert(contract);memcpy(contract,idl,(size_t)idl_length);}}
        {{
            lang_component_v1_config config={{0}};
            lang_component_v1_bytes program={{code,code_length}},metadata={{contract,idl_length}};
            config.header.size=sizeof(config);config.header.version=1;config.mode=format;
            config.allocator=allocator;
            config.ceiling.header.size=sizeof(config.ceiling);config.ceiling.header.version=1;
            config.ceiling.fuel=1000000;config.ceiling.service_work=1000000;config.ceiling.host_calls=1000;
            config.ceiling.heap_bytes=1048576;config.ceiling.live_allocations=128;config.ceiling.frames=64;
            config.ceiling.live_cells=10000;config.ceiling.live_tasks=64;config.ceiling.canonical_indices=4;
            config.ceiling.preparation_work=1000000;config.ceiling.metadata_bytes=16777216;
            config.ceiling.value_elements=65536;config.ceiling.serialization_bytes=1048576;
            config.ceiling.depth=128;config.ceiling.decoder_allocation_bytes=16777216;
            config.ceiling.suppressed_records=64;config.ceiling.trace_frames=64;
            checkpoint(format,0,"engine-lifecycle");
            assert(probe_engine_lifecycle(&config,format,program,metadata)==0);
            {{
                lang_component_v1_api api={{0}};
                api.engine_create=probe_engine_create;api.engine_destroy=probe_engine_destroy;
                api.module_load=probe_module_load;api.module_drop=probe_module_drop;
                api.provider_register=probe_provider_register;api.provider_drop=probe_provider_drop;
                api.resource_publish=probe_resource_publish;api.resource_drop=probe_resource_drop;
                api.context_resource_write=probe_context_resource_write;api.context_resource_read=probe_context_resource_read;api.context_resource_publish=probe_context_resource_publish;
                api.context_resource_keep=probe_context_resource_keep;api.context_resource_take=probe_context_resource_take;
                api.context_check=probe_context_check;api.context_buffer_create=probe_context_buffer_create;
                api.context_buffer_drop=probe_context_buffer_drop;api.buffer_mut_bytes=probe_buffer_mut_bytes;
                api.instance_create=probe_instance_create;api.instance_close=probe_instance_close;
                api.bind_export=probe_bind_export;api.binding_drop=probe_binding_drop;
                api.control_create=probe_control_create;api.control_drop=probe_control_drop;api.interrupt=probe_interrupt;
                api.call=probe_api_call;api.context_call=probe_context_call;api.result_info=probe_result_info;api.result_usage=probe_result_usage;
                api.result_bytes=probe_result_bytes;api.result_drop=probe_result_drop;
                for(mode=0;mode<2;++mode){{
                    lang_component_v1_config reduced=config;config.mode=mode;reduced.mode=mode;
                    reduced.ceiling.metadata_bytes=1048576;
                    checkpoint(format,mode,"bound-limited");bound_case(&api,&reduced,format,program,metadata,1);
                    checkpoint(format,mode,"bound");bound_case(&api,&config,format,program,metadata,0);
                    checkpoint(format,mode,"decoder-refusal");decoder_refusal_case(&api,&config,format,program,metadata);
                    checkpoint(format,mode,"registry");registry_case(&api,&config,format);
                    checkpoint(format,mode,"resource");resource_case(&api,&config,format,0);
                    checkpoint(format,mode,"resource-limit");resource_case(&api,&config,format,1);
                    checkpoint(format,mode,"task-root");
                    task_root_case(&api,&config,format);
                    assert(o->live==0&&o->allocated==o->released);
                }}
            }}
        }}
        checkpoint(format,0,"legacy-load");
        module=probe_load(format,code,code_length,contract,idl_length,allocator);assert(module);
        memset(code,0,(size_t)code_length);free(code);
        if(contract){{memset(contract,0,(size_t)idl_length);free(contract);}}
        for(mode=0;mode<2;++mode)for(repeat=0;repeat<2;++repeat){{
            uint8_t arguments[1024],output[1024];uint64_t length=0;
            {arguments_type} args={{0}};{result_type} result=0;
            lc_value_writer w={{0}};lc_value_reader r={{0}};
            args.p0=41;w.data=arguments;w.capacity=sizeof(arguments);
            w.left.work=1000000;w.left.elements=1000;w.left.depth=128;w.contextual=1;
            assert({name}_encode_arguments(&w,&args));
            assert(probe_call(module,mode,arguments,w.at,output,sizeof(output),&length)==0);
            r.data=output;r.length=length;r.left.work=1000000;r.left.elements=1000;r.left.depth=128;r.contextual=1;
            assert({name}_decode_result(&r,&result));assert(result==42);
        }}
        probe_drop(module);assert(o->live==0&&o->allocated==o->released);
    }}
    assert(o->allocated>=2&&o->allocated==o->released);free(o->requests);free(o);return 0;
}}
"#,
        header = header.header,
        resource_header = resource_host_header.header + &resource_header.header,
        task_header = task_header.header,
        declarations = declarations,
        arguments_type = function.arguments,
        result_type = function.result,
        name = function.name
    );
    let object = files
        .0
        .join(if cfg!(windows) { "host.obj" } else { "host.o" });
    let mut child = Command::new("clang")
        .args([
            "-x", "c", "-std=c11", "-Wall", "-Wextra", "-Werror", "-c", "-o",
        ])
        .arg(&object)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(c.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let driver = files.0.join("driver.rs");
    let registry_provider_digest: [u8; 32] =
        <sha2::Sha256 as sha2::Digest>::digest(&native_contract).into();
    let registry_root_digests: [[u8; 32]; 2] = [
        <sha2::Sha256 as sha2::Digest>::digest(native_source.as_bytes()).into(),
        <sha2::Sha256 as sha2::Digest>::digest(&native_artifact).into(),
    ];
    let source_span = |source: &str, expression: &str| {
        let start = source.find(expression).expect("unique fixture expression");
        assert_eq!(source.matches(expression).count(), 1);
        [start as u32, (start + expression.len()) as u32]
    };
    let registry_mixed_spans = [
        source_span(&native_source, &native_root_call),
        source_span(&native_fail_source, "trace_panic(p0, true, true)"),
        source_span(&native_fail_source, "trace_panic(value, next, false)"),
        source_span(&native_fail_source, "core::panic::<i64>(value)"),
        source_span(&native_fail_source, &native_child_call),
    ];
    let registry_stack_digests: [[u8; 32]; 6] = [
        <sha2::Sha256 as sha2::Digest>::digest(native_stack_source.as_bytes()).into(),
        <sha2::Sha256 as sha2::Digest>::digest(&native_stack_artifact).into(),
        <sha2::Sha256 as sha2::Digest>::digest(native_long_source.as_bytes()).into(),
        <sha2::Sha256 as sha2::Digest>::digest(&native_long_artifact).into(),
        <sha2::Sha256 as sha2::Digest>::digest(native_fail_source.as_bytes()).into(),
        <sha2::Sha256 as sha2::Digest>::digest(&native_fail_artifact).into(),
    ];
    let resource_provider_digest: [u8; 32] =
        <sha2::Sha256 as sha2::Digest>::digest(&resource_host_contract).into();
    let resource_module_digests: [[u8; 32]; 2] = [
        <sha2::Sha256 as sha2::Digest>::digest(resource_program.as_bytes()).into(),
        <sha2::Sha256 as sha2::Digest>::digest(&resource_artifact).into(),
    ];
    let driver_source = include_str!("support/component_input_host.rs")
        .replace(
            "const REGISTRY_ROOT_DIGESTS: [[u8; 32]; 2] = [[0; 32]; 2];",
            &format!("const REGISTRY_ROOT_DIGESTS: [[u8; 32]; 2] = {registry_root_digests:?};"),
        )
        .replace(
            "const REGISTRY_MIXED_SPANS: [[u32; 2]; 5] = [[0; 2]; 5];",
            &format!("const REGISTRY_MIXED_SPANS: [[u32; 2]; 5] = {registry_mixed_spans:?};"),
        )
        .replace(
            "const REGISTRY_STACK_DIGESTS: [[u8; 32]; 6] = [[0; 32]; 6];",
            &format!("const REGISTRY_STACK_DIGESTS: [[u8; 32]; 6] = {registry_stack_digests:?};"),
        )
        .replace(
            "const REGISTRY_PROVIDER_DIGEST: [u8; 32] = [0; 32];",
            &format!("const REGISTRY_PROVIDER_DIGEST: [u8; 32] = {registry_provider_digest:?};"),
        )
        .replace(
            "const RESOURCE_RUN_MEMBER: u32 = 0;",
            &format!("const RESOURCE_RUN_MEMBER: u32 = {resource_run_member};"),
        )
        .replace(
            "const RESOURCE_PROVIDER_DIGEST: [u8; 32] = [0; 32];",
            &format!("const RESOURCE_PROVIDER_DIGEST: [u8; 32] = {resource_provider_digest:?};"),
        )
        .replace(
            "const RESOURCE_MODULE_DIGESTS: [[u8; 32]; 2] = [[0; 32]; 2];",
            &format!("const RESOURCE_MODULE_DIGESTS: [[u8; 32]; 2] = {resource_module_digests:?};"),
        );
    std::fs::write(&driver, driver_source).unwrap();
    let dependency = artifacts::library("lang_runtime");
    let mut rustc = Command::new("rustc");
    artifacts::configure_host(&mut rustc);
    rustc
        .arg("--edition=2024")
        .arg(&driver)
        .arg("-L")
        .arg(format!(
            "dependency={}",
            dependency.parent().unwrap().display()
        ))
        .arg("-C")
        .arg(format!("link-arg={}", object.display()))
        .arg("-o")
        .arg(files.executable());
    for name in [
        "lang",
        "lang_codegen_clif",
        "lang_mir",
        "lang_runtime",
        "lang_vm",
    ] {
        rustc
            .arg("--extern")
            .arg(format!("{name}={}", artifacts::library(name).display()));
    }
    let output = rustc.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(files.executable()).output().unwrap();
    assert!(
        output.status.success(),
        "exit={:?}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}
