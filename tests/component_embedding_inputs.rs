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
        "{}\n{} {{return p0 + 1;}}",
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
