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

#[test]
fn native_c_host_loads_snapshots_and_repeats_generated_typed_calls_on_vm_and_jit() {
    let directory =
        std::env::temp_dir().join(format!("lang-component-native-host-{}", std::process::id()));
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
    let header = g::c::generate(module.contract(), target, Default::default()).unwrap();
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
    let declarations =
        array("source", source.as_bytes()) + &array("idl", IDL) + &array("artifact", &bytes);
    let c = format!(
        r#"
#define _POSIX_C_SOURCE 200112L
{header}
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <malloc.h>
#endif
{declarations}
typedef struct probe_allocator {{
    uint32_t version,size;void *user;
    void *(*allocate)(void *,uint64_t,uint64_t);
    void (*deallocate)(void *,void *,uint64_t,uint64_t);
}} probe_allocator;
extern void *probe_load(uint32_t,const uint8_t *,uint64_t,const uint8_t *,uint64_t,probe_allocator);
extern uint32_t probe_call(void *,uint32_t,const uint8_t *,uint64_t,uint8_t *,uint64_t,uint64_t *);
extern void probe_drop(void *);
typedef struct request {{void *data;uint64_t size,alignment;uint32_t live;}} request;
typedef struct owner {{request requests[8192];uint64_t allocated,released,live;}} owner;
static void *allocate(void *user,uint64_t size,uint64_t alignment) {{
    owner *o=(owner *)user;void *data=NULL;assert(size&&alignment);
    assert(o->allocated<8192);
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
static void deallocate(void *user,void *data,uint64_t size,uint64_t alignment) {{
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
uint32_t fixture(void) {{
    uint32_t format,mode,repeat;uint64_t before;
    owner *o=(owner *)calloc(1,sizeof(owner));assert(o);
    probe_allocator allocator={{1,sizeof(probe_allocator),o,allocate,deallocate}};
    before=o->allocated;
    assert(probe_load(99,source,sizeof(source),idl,sizeof(idl),allocator)==NULL);
    assert(o->allocated==before);
    for(format=0;format<2;++format){{
        uint64_t code_length=format?sizeof(artifact):sizeof(source);
        uint64_t idl_length=format?0:sizeof(idl);
        uint8_t *code=(uint8_t *)malloc((size_t)code_length);
        uint8_t *contract=idl_length?(uint8_t *)malloc((size_t)idl_length):NULL;
        void *module;assert(code);memcpy(code,format?artifact:source,(size_t)code_length);
        if(idl_length){{assert(contract);memcpy(contract,idl,(size_t)idl_length);}}
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
    assert(o->allocated>=2&&o->allocated==o->released);free(o);return 0;
}}
"#,
        header = header.header,
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
    std::fs::write(&driver, include_str!("support/component_input_host.rs")).unwrap();
    let lang = artifacts::library("lang");
    let mut rustc = Command::new("rustc");
    rustc
        .arg("--edition=2024")
        .arg(&driver)
        .arg("-L")
        .arg(format!("dependency={}", lang.parent().unwrap().display()))
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
