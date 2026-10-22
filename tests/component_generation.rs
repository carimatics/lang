use lang::{component_bindings as g, frontend::component as c};
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::component::{idl, wire};
use std::path::Path;

fn clang(target: &str, source: &str, language: &str, extra: &[&str]) -> serde_json::Value {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut command = Command::new("clang");
    command.args([
        "-target",
        target,
        "-x",
        language,
        if language == "c++" {
            "-std=c++20"
        } else {
            "-std=c11"
        },
        "-fsyntax-only",
        "-Werror",
        "-",
    ]);
    command
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .expect("clang is required for component C ABI conformance");
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
    if extra.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
fn named<'a>(tree: &'a serde_json::Value, kind: &str, name: &str) -> &'a serde_json::Value {
    let mut pending = vec![tree];
    while let Some(node) = pending.pop() {
        if node["kind"] == kind && node["name"] == name && node.get("inner").is_some() {
            return node;
        }
        if let Some(inner) = node["inner"].as_array() {
            pending.extend(inner);
        }
    }
    panic!("missing {kind} {name}")
}
fn field_types(tree: &serde_json::Value, name: &str) -> Vec<(String, String)> {
    named(tree, "RecordDecl", name)["inner"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["kind"] == "FieldDecl")
        .map(|n| {
            (
                n["name"].as_str().unwrap().into(),
                n["type"]["qualType"].as_str().unwrap().into(),
            )
        })
        .collect()
}

#[test]
fn generated_abi_matches_every_reference_field_signature_and_target_layout() {
    use g::abi::{self, Target};
    for target in [Target::MacosAarch64, Target::WindowsX86_64Msvc] {
        let generated = abi::generate(target, Default::default()).unwrap();
        assert_eq!(generated.layouts.len(), 16);
        assert_eq!(generated.signatures.len(), 38);
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../docs/component-abi-v1-layouts.json")).unwrap();
        assert_eq!(
            hex(&generated.fingerprint),
            vectors["fingerprints"][target.triple()]["layout"]
        );
        for signature in &generated.signatures {
            assert_eq!(
                hex(&signature.fingerprint),
                vectors["fingerprints"][target.triple()]["signatures"][signature.name],
                "{}",
                signature.name
            );
        }
        let reference = clang(
            target.triple(),
            include_str!("../docs/component-abi-v1.h"),
            "c",
            &["-Xclang", "-ast-dump=json"],
        );
        let actual = clang(
            target.triple(),
            &generated.header,
            "c",
            &["-Xclang", "-ast-dump=json"],
        );
        let mut assertions = String::from("\n#include <stddef.h>\n");
        for layout in &generated.layouts {
            let name = format!("lang_component_v1_{}", layout.name);
            assert_eq!(
                field_types(&actual, &name),
                field_types(&reference, &name),
                "{name}"
            );
            let expected = &vectors["layout"][layout.name];
            assert_eq!(layout.size, expected["size"].as_u64().unwrap() as usize);
            assert_eq!(
                layout.alignment,
                expected["align"].as_u64().unwrap() as usize
            );
            assert_eq!(
                layout.fields.len(),
                expected["offsets"].as_object().unwrap().len()
            );
            assertions += &format!(
                "_Static_assert(sizeof({name})=={},\"size\");\n_Static_assert(_Alignof({name})=={},\"align\");\n",
                layout.size, layout.alignment
            );
            for field in &layout.fields {
                assert_eq!(
                    field.offset,
                    expected["offsets"][field.name].as_u64().unwrap() as usize
                );
                assertions += &format!(
                    "_Static_assert(offsetof({name},{})=={},\"offset\");\n",
                    field.name, field.offset
                );
            }
        }
        for signature in &generated.signatures {
            if signature.name.ends_with("_fn") || signature.name == "get_api" {
                let kind = if signature.name == "get_api" {
                    "FunctionDecl"
                } else {
                    "TypedefDecl"
                };
                let name = format!("lang_component_v1_{}", signature.name);
                assert_eq!(
                    named(&actual, kind, &name)["type"],
                    named(&reference, kind, &name)["type"],
                    "{name}"
                );
            }
        }
        clang(
            target.triple(),
            &(generated.header.clone() + &assertions),
            "c",
            &[],
        );
        clang(
            target.triple(),
            &(include_str!("../docs/component-abi-v1.h").to_string() + &assertions),
            "c",
            &[],
        );
        clang(target.triple(), &generated.header, "c++", &[]);
        assert_eq!(
            abi::generate(target, Default::default())
                .unwrap()
                .fingerprint,
            generated.fingerprint
        );
    }
}

#[test]
fn generated_counter_declarations_and_export_signature_pass_independent_mir_check() {
    let document = idl::check(
        m::SourceId(77),
        include_bytes!("fixtures/component_counter.idl.json"),
        Default::default(),
    )
    .unwrap();
    let generated = g::guest(&document, Default::default()).unwrap();
    assert_eq!(generated.exports.len(), 1);
    let source = format!(
        "{}\n{} {{return core::Result::Ok(p1.delta);}}",
        generated.declarations, generated.exports[0].declaration
    );
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display: Path::new("generated.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document,
        bindings: generated.bindings,
        limits: Default::default(),
    });
    assert!(
        out.diagnostics.is_empty(),
        "{:?}\n{source}",
        out.diagnostics
    );
    let unit = out.result.unwrap();
    assert_eq!(unit.exports().len(), 1);
}

fn contract(json: &[u8]) -> wire::Contract {
    let document = idl::check(m::SourceId(77), json, Default::default()).unwrap();
    wire::Contract::new(
        document,
        vec![],
        &mut wire::Session::new(Default::default()),
    )
    .unwrap()
}
fn execute_c(source: &str) -> String {
    use std::{
        io::Write,
        process::{Command, Stdio},
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let directory = std::env::temp_dir().join(format!(
        "lang-component-generation-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let executable = directory.join("fixture");
    let mut child = Command::new("clang")
        .args(["-x", "c", "-std=c11", "-Wall", "-Wextra", "-Werror", "-o"])
        .arg(&executable)
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
    let output = Command::new(executable).output().unwrap();
    assert!(
        output.status.success(),
        "exit={:?}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn frame(digest: [u8; 32], kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"LCVL".to_vec();
    bytes.extend(1u32.to_le_bytes());
    bytes.push(kind);
    bytes.extend(digest);
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
    bytes
}

#[test]
fn generated_typed_counter_c_codecs_and_call_commit_states_run_natively() {
    let contract = contract(include_bytes!("fixtures/component_counter.idl.json"));
    let generated =
        g::c::generate(&contract, g::abi::Target::MacosAarch64, Default::default()).unwrap();
    for target in [
        g::abi::Target::MacosAarch64,
        g::abi::Target::WindowsX86_64Msvc,
    ] {
        clang(target.triple(), &generated.header, "c", &[]);
        clang(target.triple(), &generated.header, "c++", &[]);
    }
    let f = &generated.functions[0];
    let source = format!(
        r#"{}
#include <stdio.h>
#include <assert.h>
struct lang_component_v1_result {{uint32_t flags;}};
static struct lang_component_v1_result answer={{3}};
static uint32_t calls=0,inspect_failure=0,refuse=0;
static const lang_component_v1_context *seen_context;
static lang_component_v1_status LANG_COMPONENT_V1_CALL mock_info(const lang_component_v1_result *r,lang_component_v1_result_info *out,lang_component_v1_error *error) {{
    (void)error;if(inspect_failure)return LANG_COMPONENT_V1_STALE;out->flags=r->flags;return LANG_COMPONENT_V1_OK;
}}
static lang_component_v1_status LANG_COMPONENT_V1_CALL mock_call(lang_component_v1_binding *b,lang_component_v1_control *control,lang_component_v1_bytes bytes,lang_component_v1_result **out,lang_component_v1_error *error) {{
    (void)b;(void)control;(void)error;++calls;assert(bytes.length>45);if(refuse)return LANG_COMPONENT_V1_INVALID_REQUEST;*out=&answer;return LANG_COMPONENT_V1_OK;
}}
static lang_component_v1_status LANG_COMPONENT_V1_CALL mock_context(const lang_component_v1_context *context,lang_component_v1_binding *b,lang_component_v1_bytes bytes,lang_component_v1_result **out,lang_component_v1_error *error) {{seen_context=context;return mock_call(b,NULL,bytes,out,error);}}
static lc_value_writer writer(uint8_t *data,uint64_t capacity) {{lc_value_writer w={{0}};w.data=data;w.capacity=capacity;w.left.work=10000;w.left.elements=1000;w.left.depth=128;w.contextual=1;return w;}}
static lc_value_reader reader(const uint8_t *data,uint64_t length) {{lc_value_reader r={{0}};r.data=data;r.length=length;r.left.work=10000;r.left.elements=1000;r.left.depth=128;r.contextual=1;return r;}}
static void print_frame(const uint8_t *data,uint64_t n) {{uint64_t i;for(i=0;i<n;++i)printf("%02x",data[i]);puts("");}}
int main(void) {{
    uint8_t bytes[1024];lc_value_writer w=writer(bytes,sizeof(bytes));
    {args} args={{0}},decoded={{0}};{result} value={{0}},output={{0}};
    args.p0.index=7;args.p1.f0=40;args.p1.f1.data=(const uint8_t *)"label";args.p1.f1.length=5;
    assert({name}_encode_arguments(&w,&args));print_frame(bytes,w.at);
    lc_value_reader r=reader(bytes,w.at);assert({name}_decode_arguments(&r,&decoded));assert(decoded.p0.index==7&&decoded.p1.f0==40&&decoded.p1.f1.length==5);
    decoded.p0.index=99;r=reader(bytes,w.at);r.contextual=0;assert(!{name}_decode_arguments(&r,&decoded));assert(decoded.p0.index==99);
    value.tag=0;value.data.f0=42;w=writer(bytes,sizeof(bytes));assert({name}_encode_result(&w,&value));print_frame(bytes,w.at);r=reader(bytes,w.at);assert({name}_decode_result(&r,&output));assert(output.tag==0&&output.data.f0==42);
    lang_component_v1_api api={{0}};lang_component_v1_error error={{0}};lang_component_v1_context context={{0}};api.call=mock_call;api.context_call=mock_context;api.result_info=mock_info;
    w=writer(bytes,sizeof(bytes));lc_value_outcome out={name}_call(&api,NULL,NULL,&args,&w,&error);assert(calls==1&&out.commit==1&&out.poisoned==1&&out.result==&answer&&args.p0.index==7);
    inspect_failure=1;w=writer(bytes,sizeof(bytes));out={name}_context_call(&api,&context,NULL,&args,&w,&error);assert(calls==2&&out.commit==2&&out.result==&answer&&seen_context==&context);
    inspect_failure=0;refuse=1;w=writer(bytes,sizeof(bytes));out={name}_call(&api,NULL,NULL,&args,&w,&error);assert(calls==3&&out.commit==0&&!out.result&&args.p0.index==7);
    args.p0.index=0;w=writer(bytes,sizeof(bytes));out={name}_call(&api,NULL,NULL,&args,&w,&error);assert(calls==3&&out.commit==0&&!out.result);
    return 0;
}}
"#,
        generated.header,
        args = f.arguments,
        result = f.result,
        name = f.name
    );
    let actual = execute_c(&source);
    let mut payload = 7u32.to_le_bytes().to_vec();
    payload.extend(40i64.to_le_bytes());
    payload.extend(5u32.to_le_bytes());
    payload.extend(b"label");
    let mut result = vec![0];
    result.extend(42i64.to_le_bytes());
    assert_eq!(
        actual,
        format!(
            "{}\n{}\n",
            hex(&frame(f.signature, 0, &payload)),
            hex(&frame(f.signature, 1, &result))
        )
    );
    assert_eq!(
        generated.header,
        g::c::generate(&contract, g::abi::Target::MacosAarch64, Default::default())
            .unwrap()
            .header
    );
}

#[test]
fn generated_guest_all_data_families_aliases_keywords_and_async_effects_are_checked() {
    let mut json: serde_json::Value = serde_json::from_slice(include_bytes!(
        "fixtures/component_generated_values.idl.json"
    ))
    .unwrap();
    json["interfaces"][0]["functions"].as_array_mut().unwrap().push(serde_json::json!({"name":"later","effect":"task","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"demo.generated.later"}));
    let bytes = serde_json::to_vec(&json).unwrap();
    let contract = contract(&bytes);
    let generated = g::guest(contract.checked(), Default::default()).unwrap();
    assert!(generated.declarations.contains("component_666e:"));
    assert!(
        generated
            .declarations
            .contains("component_636f6d706f6e656e742d6b6579:")
    );
    assert!(
        generated
            .declarations
            .contains("component_68656c6c6f2d776f726c64:")
    );
    let mut source = generated.declarations;
    for export in generated.exports {
        source += &format!("\n{} {{return p0;}}\n", export.declaration);
    }
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display: Path::new("generated.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: idl::check(m::SourceId(77), &bytes, Default::default()).unwrap(),
        bindings: generated.bindings,
        limits: Default::default(),
    });
    assert!(
        out.diagnostics.is_empty(),
        "{:?}\n{source}",
        out.diagnostics
    );
    assert_eq!(out.result.unwrap().exports().len(), 4);
}

#[test]
fn generated_c_all_data_families_match_rust_wire_and_refuse_malformed_or_exhausted_frames() {
    use wire::{FrameKind as K, Value as V, ValueId as I, Values};
    let contract = contract(include_bytes!(
        "fixtures/component_generated_values.idl.json"
    ));
    let generated =
        g::c::generate(&contract, g::abi::Target::MacosAarch64, Default::default()).unwrap();
    let f = generated
        .functions
        .iter()
        .find(|f| f.member.member == "echo")
        .unwrap();
    let u = generated
        .functions
        .iter()
        .find(|f| f.member.member == "units")
        .unwrap();
    let choice = generated
        .functions
        .iter()
        .find(|f| f.member.member == "choices")
        .unwrap();
    let mut session = wire::Session::new(Default::default());
    let sig = wire::Signature::new(&contract, &f.member.interface, "echo", &mut session).unwrap();
    let values = Values {
        nodes: vec![
            V::I64(i64::MIN),
            V::Bool(true),
            V::Text("雪".into()),
            V::Bytes(vec![0, 255]),
            V::I64(i64::MAX),
            V::Some(I(4)),
            V::None,
            V::List(vec![I(5), I(6)]),
            V::Bool(false),
            V::Err(I(8)),
            V::I64(-1),
            V::Variant {
                case: 1,
                payload: I(10),
            },
            V::Record(vec![I(0), I(1), I(2), I(3), I(7), I(9), I(11)]),
        ],
        roots: vec![I(12)],
    };
    let parameters = wire::encode_values(&sig, K::Parameters, &values, &mut session).unwrap();
    let result = wire::encode_values(&sig, K::Result, &values, &mut session).unwrap();
    let literal = |b: &[u8]| {
        b.iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let source = format!(
        r#"{}
#include <assert.h>
#include <stdio.h>
static uint8_t parameters[]={{{parameters}}};
static uint8_t result_frame[]={{{result}}};
static lc_value_reader reader(uint8_t *scratch,uint64_t capacity) {{lc_value_reader r={{0}};r.data=parameters;r.length=sizeof(parameters);r.scratch=scratch;r.capacity=capacity;r.left.work=10000;r.left.elements=1000;r.left.depth=128;return r;}}
static lc_value_writer writer(uint8_t *data,uint64_t capacity) {{lc_value_writer w={{0}};w.data=data;w.capacity=capacity;w.left.work=10000;w.left.elements=1000;w.left.depth=128;return w;}}
int main(void) {{
    uint8_t scratch[1025],output_bytes[1024];{args} args={{0}},sentinel={{0}};{ty} value={{0}};
    lc_value_reader r=reader(scratch+1,1024);assert({name}_decode_arguments(&r,&args));
    assert(args.p0.f0==INT64_MIN&&args.p0.f1==1&&args.p0.f2.length==3&&args.p0.f2.data[0]==0xe9&&args.p0.f3.length==2&&args.p0.f3.data[1]==255);
    assert(args.p0.f4.count==2&&args.p0.f4.data[0].tag==1&&args.p0.f4.data[0].value==INT64_MAX&&args.p0.f4.data[1].tag==0);
    assert((uintptr_t)args.p0.f4.data%8==0&&args.p0.f5.tag==1&&args.p0.f5.data.f1==0&&args.p0.f6.tag==1&&args.p0.f6.data.f1==-1);
    uint64_t work=10000-r.left.work,elements=1000-r.left.elements,used=r.used,i;
    r=reader(scratch+1,used);r.left.work=work;r.left.elements=elements;r.left.depth=4;assert({name}_decode_arguments(&r,&args));
    sentinel.p0.f0=42;
    r=reader(scratch+1,used-1);assert(!{name}_decode_arguments(&r,&sentinel)&&sentinel.p0.f0==42);
    r=reader(scratch+1,1024);r.left.work=work-1;assert(!{name}_decode_arguments(&r,&sentinel)&&sentinel.p0.f0==42);
    r=reader(scratch+1,1024);r.left.elements=elements-1;assert(!{name}_decode_arguments(&r,&sentinel)&&sentinel.p0.f0==42);
    r=reader(scratch+1,1024);r.left.depth=3;assert(!{name}_decode_arguments(&r,&sentinel)&&sentinel.p0.f0==42);
    for(i=0;i<sizeof(parameters);++i){{r=reader(scratch+1,1024);r.length=i;assert(!{name}_decode_arguments(&r,&sentinel)&&sentinel.p0.f0==42);}}
    uint32_t offsets[]={{53,58,83}};for(i=0;i<3;++i){{uint8_t old=parameters[offsets[i]];parameters[offsets[i]]=255;r=reader(scratch+1,1024);assert(!{name}_decode_arguments(&r,&sentinel)&&sentinel.p0.f0==42);parameters[offsets[i]]=old;}}
    r=reader(scratch+1,1024);r.data=result_frame;r.length=sizeof(result_frame);assert({name}_decode_result(&r,&value));
    lc_value_writer w=writer(output_bytes,sizeof(output_bytes));assert({name}_encode_result(&w,&value));work=10000-w.left.work;elements=1000-w.left.elements;assert(w.at==sizeof(result_frame));for(i=0;i<w.at;++i)assert(output_bytes[i]==result_frame[i]);
    w=writer(output_bytes,sizeof(result_frame));w.left.work=work;w.left.elements=elements;w.left.depth=4;assert({name}_encode_result(&w,&value));
    w=writer(output_bytes,sizeof(result_frame)-1);assert(!{name}_encode_result(&w,&value));
    w=writer(output_bytes,sizeof(output_bytes));w.left.work=work-1;assert(!{name}_encode_result(&w,&value));
    w=writer(output_bytes,sizeof(output_bytes));w.left.elements=elements-1;assert(!{name}_encode_result(&w,&value));
    {units_args} units={{0}};units.p0.count=UINT32_MAX;w=writer(output_bytes,sizeof(output_bytes));assert(!{units}_encode_arguments(&w,&units)&&w.status==3);
    lc_value_unit items[3]={{{{0}},{{0}},{{0}}}};units.p0.count=3;units.p0.data=items;
    {units_result} unit_result=units.p0;
    w=writer(output_bytes,sizeof(output_bytes));assert({units}_encode_result(&w,&unit_result)&&w.at==49);
    r=reader(scratch+1,1024);r.data=output_bytes;r.length=w.at;assert({units}_decode_result(&r,&unit_result)&&unit_result.count==3&&unit_result.data[2].unused==0);
    units.p0=unit_result;w=writer(output_bytes,sizeof(output_bytes));assert({units}_encode_arguments(&w,&units));
    {choice_args} choice={{0}};choice.p0.tag=1;choice.p0.value.tag=0;choice.p0.value.data.f0=1;
    {choice_result} chosen=choice.p0;w=writer(output_bytes,sizeof(output_bytes));assert({choice_name}_encode_result(&w,&chosen)&&w.at==48&&output_bytes[45]==1&&output_bytes[46]==0&&output_bytes[47]==1);
    static const uint8_t invalid[][4]={{{{0xc0,0xaf,0,0}},{{0xe0,0x80,0x80,0}},{{0xed,0xa0,0x80,0}},{{0xf4,0x90,0x80,0x80}},{{0x80,0,0,0}},{{0xe9,0,0,0}}}};uint64_t lengths[]={{2,3,3,4,1,1}};
    for(i=0;i<6;++i){{lc_value_limits left={{100,100,128}};uint32_t status=0;assert(!lc_value_utf8(invalid[i],lengths[i],&left,&status)&&status==2);}}
    puts("all values, canonical bytes, malformed frames and exact quotas passed");return 0;
}}
"#,
        generated.header,
        parameters = literal(&parameters),
        result = literal(&result),
        name = f.name,
        args = f.arguments,
        ty = f.result,
        units = u.name,
        units_args = u.arguments,
        units_result = u.result,
        choice_args = choice.arguments,
        choice_result = choice.result,
        choice_name = choice.name
    );
    assert!(execute_c(&source).starts_with("all values"));
    for target in [
        g::abi::Target::MacosAarch64,
        g::abi::Target::WindowsX86_64Msvc,
    ] {
        clang(target.triple(), &generated.header, "c", &[]);
        clang(target.triple(), &generated.header, "c++", &[]);
    }
    let mut reordered: serde_json::Value = serde_json::from_slice(include_bytes!(
        "fixtures/component_generated_values.idl.json"
    ))
    .unwrap();
    reordered["interfaces"][0]["types"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["interfaces"][0]["functions"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let other = contract_from_json(&reordered);
    assert_eq!(
        generated.header,
        g::c::generate(&other, g::abi::Target::MacosAarch64, Default::default())
            .unwrap()
            .header
    );
}
fn contract_from_json(value: &serde_json::Value) -> wire::Contract {
    contract(&serde_json::to_vec(value).unwrap())
}

#[test]
fn reference_vector_packer_reproduces_the_complete_frozen_data() {
    let output = std::process::Command::new("python3")
        .arg("tests/fixtures/component_abi_reference.py")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../docs/component-abi-v1-layouts.json")).unwrap();
    assert_eq!(actual, expected);
}
fn exact_limits(usage: m::Usage, text_bytes: usize) -> g::Limits {
    g::Limits {
        output_bytes: text_bytes,
        budget: m::Limits {
            work: usage.work,
            allocations: usage.allocation_bytes,
            elements: usage.elements,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn one_below(mut limits: g::Limits, which: usize) -> g::Limits {
    match which {
        0 => limits.output_bytes -= 1,
        1 => limits.budget.work -= 1,
        2 => limits.budget.allocations -= 1,
        3 => limits.budget.elements -= 1,
        _ => unreachable!(),
    }
    limits
}

#[test]
fn every_generator_accepts_exact_cumulative_quotas_and_refuses_one_below() {
    let contract = contract(include_bytes!(
        "fixtures/component_generated_values.idl.json"
    ));
    let guest = g::guest(contract.checked(), Default::default()).unwrap();
    let limits = exact_limits(guest.usage, guest.text_bytes);
    assert_eq!(
        g::guest(contract.checked(), limits).unwrap().declarations,
        guest.declarations
    );
    for which in 0..4 {
        assert!(
            g::guest(contract.checked(), one_below(limits, which)).is_err(),
            "guest quota {which}"
        );
    }
    let target = g::abi::Target::MacosAarch64;
    let abi = g::abi::generate(target, Default::default()).unwrap();
    let limits = exact_limits(abi.usage, abi.text_bytes);
    assert_eq!(
        g::abi::generate(target, limits).unwrap().fingerprint,
        abi.fingerprint
    );
    for which in 0..4 {
        assert!(
            g::abi::generate(target, one_below(limits, which)).is_err(),
            "ABI quota {which}"
        );
    }
    let c = g::c::generate(&contract, target, Default::default()).unwrap();
    let limits = exact_limits(c.usage, c.text_bytes);
    assert_eq!(
        g::c::generate(&contract, target, limits).unwrap().header,
        c.header
    );
    for which in 0..4 {
        assert!(
            g::c::generate(&contract, target, one_below(limits, which)).is_err(),
            "C quota {which}"
        );
    }
    let limits = g::Limits {
        depth: 0,
        ..Default::default()
    };
    assert!(matches!(
        g::guest(contract.checked(), limits),
        Err(g::Error::Depth)
    ));
    assert!(matches!(
        g::c::generate(&contract, target, limits),
        Err(g::Error::Depth)
    ));
}

#[test]
fn generated_origins_retain_original_idl_byte_spans_and_import_ownership_proofs() {
    let json = include_bytes!("fixtures/component_generated_values.idl.json");
    let contract = contract(json);
    let guest = g::guest(contract.checked(), Default::default()).unwrap();
    let origin = guest
        .origins
        .iter()
        .find(|o| &guest.declarations[o.start..o.end] == "component_666e:i64,\n")
        .unwrap();
    assert_eq!(
        &json[origin.contract.start as usize..origin.contract.end as usize],
        b"\"fn\""
    );
    assert_eq!(origin.contract.source, m::SourceId(77));
    let c = g::c::generate(&contract, g::abi::Target::MacosAarch64, Default::default()).unwrap();
    for o in c.origins {
        assert!(o.start < o.end && o.end <= c.header.len());
        assert_eq!(o.contract.source, m::SourceId(77));
        assert!(o.contract.end as usize <= json.len());
    }
    let mut value: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_counter.idl.json")).unwrap();
    value["exports"] = serde_json::json!([]);
    value["imports"] = serde_json::json!([{"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["apply"]}]);
    value["interfaces"][0]["functions"][0]["params"] =
        serde_json::json!([{"name":"state","type":{"own":"counter"}}]);
    value["interfaces"][0]["functions"][0]["result"] = serde_json::json!({"own":"counter"});
    let bytes = serde_json::to_vec(&value).unwrap();
    let document = idl::check(m::SourceId(77), &bytes, Default::default()).unwrap();
    let generated = g::guest(&document, Default::default()).unwrap();
    let import = &generated.bindings.imports[0].source;
    let tag = &generated.bindings.resources[0].tag;
    let source = format!(
        "{}\npub fn forward(value:core::ComponentResource<{tag}>)->core::ComponentResource<{tag}>{{return {import}(value);}}",
        generated.declarations
    );
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display: Path::new("generated.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document,
        bindings: generated.bindings,
        limits: Default::default(),
    });
    assert!(
        out.diagnostics.is_empty(),
        "{:?}\n{source}",
        out.diagnostics
    );
    assert_eq!(out.result.unwrap().imports().len(), 1);
}

#[test]
fn generated_typed_source_fixture_executes_the_independently_checked_code_on_vm_and_native() {
    use lang_runtime::{allocation as a, cleanup, component::bindings::Code, layout};
    let contract = contract(include_bytes!(
        "fixtures/component_generated_script.idl.json"
    ));
    let generated = g::guest(contract.checked(), Default::default()).unwrap();
    let export = &generated.exports[0];
    let ty = format!("component_t_{}", hex(b"demo:script/api#request"));
    let source = format!(
        "{}\n{}{{return p0;}}\nfn main()->i64{{let mut value:{ty}={name}({ty}{{value:42,flag:true}});if value.flag{{return value.value;}}return 0;}}",
        generated.declarations,
        export.declaration,
        name = export.source
    );
    let document = idl::check(
        m::SourceId(77),
        include_bytes!("fixtures/component_generated_script.idl.json"),
        Default::default(),
    )
    .unwrap();
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display: Path::new("generated.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document,
        bindings: generated.bindings,
        limits: Default::default(),
    });
    assert!(
        out.diagnostics.is_empty(),
        "{:?}\n{source}",
        out.diagnostics
    );
    let unit = out.result.unwrap();
    let Code::Owned(flow) = unit.code() else {
        panic!("sync fixture")
    };
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = cleanup::Instance::default();
    let vm = lang_vm::unit::owned::execute_entry(
        flow,
        &mut instance,
        Some(&mut allocator),
        Default::default(),
    )
    .unwrap();
    assert_eq!(vm.value, 42);
    let layouts =
        layout::Layouts::compile(unit.code().declarations().types(), Default::default()).unwrap();
    let native = lang_codegen_clif::owned::CompiledProgram::compile(
        flow,
        &layouts,
        Default::default(),
        65536,
    )
    .unwrap();
    let entry = m::FunctionId(
        unit.code()
            .declarations()
            .functions()
            .iter()
            .position(|f| f.name == "main")
            .unwrap() as u32,
    );
    let mut instance = cleanup::Instance::default();
    let result = lang_codegen_clif::owned::runtime::execute_entry(
        &native,
        entry,
        &mut instance,
        &mut allocator,
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(result.usage, vm.usage);
}

#[test]
fn qualified_names_and_contract_namespaces_avoid_cross_interface_and_header_collisions() {
    let original: serde_json::Value = serde_json::from_slice(include_bytes!(
        "fixtures/component_generated_script.idl.json"
    ))
    .unwrap();
    let a = contract_from_json(&original);
    let mut changed = original.clone();
    let mut second = changed["interfaces"][0].clone();
    second["key"] = serde_json::json!("demo:script/other");
    changed["interfaces"].as_array_mut().unwrap().push(second);
    changed["exports"] = serde_json::json!(["demo:script/api", "demo:script/other"]);
    let b = contract_from_json(&changed);
    let guest = g::guest(b.checked(), Default::default()).unwrap();
    assert_eq!(guest.exports.len(), 2);
    assert_ne!(guest.exports[0].source, guest.exports[1].source);
    let first = g::c::generate(&a, g::abi::Target::MacosAarch64, Default::default()).unwrap();
    let second = g::c::generate(&b, g::abi::Target::MacosAarch64, Default::default()).unwrap();
    assert_ne!(first.functions[0].name, second.functions[0].name);
    clang(
        "aarch64-apple-darwin",
        &(first.header + &second.header),
        "c",
        &[],
    );
    for invalid in ["Request", "component_666e", "component-666e"] {
        let mut json = original.clone();
        json["interfaces"][0]["types"][0]["name"] = serde_json::json!(invalid);
        assert!(
            idl::check(
                m::SourceId(77),
                &serde_json::to_vec(&json).unwrap(),
                Default::default()
            )
            .is_err()
        );
    }
}
