use std::{fs, path::PathBuf, process::Command};
#[path = "support/rust_artifacts.rs"]
mod artifacts;
#[path = "support/component_object_program.rs"]
mod fixture;
#[path = "support/component_native_image_provider.rs"]
mod provider;
#[derive(Clone, Copy, Debug)]
enum Profile {
    Scalar,
    Task,
    Callback,
    Resource,
}
struct Files(PathBuf);
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn exact_sdk_image_executes_generated_c_calls_after_package_paths_disappear() {
    image_profile(Profile::Scalar);
}

#[test]
fn exact_sdk_task_image_executes_generated_c_calls_with_owned_suspension_cleanup() {
    image_profile(Profile::Task);
}

#[test]
fn exact_sdk_image_reenters_a_second_aot_instance_on_the_original_scope() {
    image_profile(Profile::Callback);
}

#[test]
fn exact_sdk_resource_image_preserves_destructors_poison_and_both_code_owners() {
    image_profile(Profile::Resource);
}

fn callback_artifact(resource: bool) -> Vec<u8> {
    use lang::{component_bindings as g, frontend::component as f};
    let idl = if resource {
        provider::RESOURCE_SOURCE_IDL
    } else {
        provider::SOURCE_IDL
    };
    let document =
        lang_runtime::component::idl::check(lang_mir::unit::SourceId(1), idl, Default::default())
            .unwrap();
    let guest = g::guest(&document, Default::default()).unwrap();
    let allocation = if resource {
        format!(
            "let mut item:core::ComponentResource<{}> = {}();",
            guest.bindings.resources[0].tag,
            guest
                .bindings
                .imports
                .iter()
                .find(|i| i.member.member == "make")
                .unwrap()
                .source
        )
    } else {
        String::new()
    };
    let source = format!(
        r#"{}
{} {{
    {allocation}
    let mut bytes:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(bytes,42));
    if p0<0 {{return core::panic::<i64>(-42);}}
    let mut value:i64={}(p0);
    if core::bytes_len(bytes)<2 {{return value;}}
    return 0;
}}
"#,
        guest.declarations, guest.exports[0].declaration, guest.bindings.imports[0].source
    );
    let analysis = f::analyze(f::Request {
        sources: lang_diagnostics::SourceMap::new(),
        display: std::path::Path::new("native-image-callback.lang"),
        bytes: source.as_bytes(),
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
    lang::artifact::component::encode(
        analysis.result.unwrap(),
        lang_mir::unit::Source {
            path: "native-image-callback.idl.json".into(),
            text: String::from_utf8(idl.to_vec()).unwrap(),
        },
        None,
        Default::default(),
    )
    .unwrap()
}

fn callback_package(semantic: &[u8], resource: bool) -> lang::native::Package {
    use lang_runtime::component::{
        bindings, grants, native,
        negotiation::{InterfaceKey, PackageKey},
        program,
        serialization::{DigestAlgorithm, VersionedDigest},
        wire,
    };
    use sha2::Digest;
    let module = lang::component_embedding::load(1, semantic, &[], Default::default()).unwrap();
    let state = std::rc::Rc::new(provider::State::with_resources(resource));
    let mut session = wire::Session::new(Default::default());
    let encoded = wire::encode_contract(&state.host, &mut session).unwrap();
    let digest = VersionedDigest::new(
        DigestAlgorithm::Sha256,
        sha2::Sha256::digest(&encoded).to_vec(),
    )
    .unwrap();
    let mut methods = vec![native::Method {
        interface: 0,
        member: 0,
        invoke: provider::invoke,
        user: std::rc::Rc::as_ptr(&state).cast_mut().cast(),
        flags: 1,
    }];
    if resource {
        methods.push(native::Method {
            interface: 0,
            member: 1,
            invoke: provider::make,
            user: std::rc::Rc::as_ptr(&state).cast_mut().cast(),
            flags: 0,
        });
    }
    let resources = if resource {
        vec![native::ResourceType {
            interface: 0,
            resource: 0,
            destroy: provider::destroy,
            user: std::rc::Rc::as_ptr(&state).cast_mut().cast(),
        }]
    } else {
        vec![]
    };
    let providers = [native::Provider::new_owned(
        provider::contract(if resource {
            provider::HOST_RESOURCE_IDL
        } else {
            provider::HOST_IDL
        }),
        digest,
        methods,
        resources,
        &mut || state.clone(),
        &mut session,
    )
    .unwrap()];
    let signature = state
        .signature(true, &mut session)
        .preimage(&mut session)
        .unwrap();
    let member = bindings::MemberKey {
        interface: InterfaceKey::new(PackageKey::new("demo", "host"), "api"),
        member: "add".into(),
    };
    let mut explicit = vec![grants::Grant {
        member: &member,
        capability: "host.add",
        signature: &signature,
    }];
    let make_member = bindings::MemberKey {
        interface: member.interface.clone(),
        member: "make".into(),
    };
    let make_signature = if resource {
        wire::Signature::new(&state.host, &make_member.interface, "make", &mut session)
            .unwrap()
            .preimage(&mut session)
            .unwrap()
    } else {
        vec![]
    };
    if resource {
        explicit.push(grants::Grant {
            member: &make_member,
            capability: "host.make",
            signature: &make_signature,
        });
    }
    let bound = grants::prepare(
        module.unit(),
        module.contract(),
        &explicit,
        || &providers,
        &mut session,
    )
    .unwrap();
    let program = program::Program::prepare(&bound, Default::default()).unwrap();
    lang::native::component::compile(
        semantic,
        &program,
        module.unit().exports()[0].0,
        lang_codegen_clif::Triple::host(),
        Default::default(),
    )
    .unwrap()
}

fn task_artifact() -> Vec<u8> {
    use lang::{component_bindings as g, frontend::component as f};
    use lang_runtime::component::idl;
    let idl = br#"{"idl":1,"package":"demo:object","version":"1.0.0","interfaces":[{"key":"demo:object/api","resources":[],"types":[],"functions":[{"name":"answer","effect":"task","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"demo.object.answer"}]}],"imports":[],"exports":["demo:object/api"]}"#;
    let document = idl::check(lang_mir::unit::SourceId(1), idl, Default::default()).unwrap();
    let guest = g::guest(&document, Default::default()).unwrap();
    let source = format!(
        r#"{}
async fn worker(value:i64)->i64 {{
    let mut bytes:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(bytes,42));
    yield;
    if value<0 {{return core::panic::<i64>(-42);}}
    return value+core::bytes_len(bytes);
}}
{} {{
    let mut child:core::Task<i64> = spawn worker(p0);
    let mut value:core::Result<i64,core::TaskError> = await child;
    return core::result_unwrap::<i64,core::TaskError>(value);
}}
"#,
        guest.declarations, guest.exports[0].declaration
    );
    let analysis = f::analyze(f::Request {
        sources: lang_diagnostics::SourceMap::new(),
        display: std::path::Path::new("native-image-task.lang"),
        bytes: source.as_bytes(),
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
    lang::artifact::component::encode(
        analysis.result.unwrap(),
        lang_mir::unit::Source {
            path: "native-image-task.idl.json".into(),
            text: String::from_utf8(idl.to_vec()).unwrap(),
        },
        None,
        Default::default(),
    )
    .unwrap()
}

fn image_profile(profile: Profile) {
    let task = matches!(profile, Profile::Task);
    let resource = matches!(profile, Profile::Resource);
    let callback = matches!(profile, Profile::Callback | Profile::Resource);
    use lang::{component_bindings as g, component_embedding as embedding};
    use lang_runtime::component::{idl, wire};
    let files = Files(std::env::temp_dir().join(format!(
        "lang native image-{}-{profile:?}",
        std::process::id()
    )));
    fs::create_dir(&files.0).unwrap();
    let semantic = if callback {
        callback_artifact(resource)
    } else if task {
        task_artifact()
    } else {
        fixture::artifact()
    };
    let package = if callback {
        callback_package(&semantic, resource)
    } else {
        fixture::with_artifact(&semantic, |program, _, entry| {
            lang::native::component::compile(
                &semantic,
                program,
                entry,
                lang_codegen_clif::Triple::host(),
                Default::default(),
            )
            .unwrap()
        })
    };
    let root = files.0.join("package");
    package.publish(&root).unwrap();
    let object = root.join(
        package
            .files()
            .iter()
            .find(|f| f.role == "object")
            .unwrap()
            .path,
    );
    let module = embedding::load(1, &semantic, &[], Default::default()).unwrap();
    let target = if cfg!(windows) {
        g::abi::Target::WindowsX86_64Msvc
    } else {
        g::abi::Target::MacosAarch64
    };
    let header = g::c::generate(module.contract(), target, Default::default()).unwrap();
    let function = header
        .functions
        .iter()
        .find(|f| f.member.member == "answer")
        .unwrap();
    let c = files.0.join("host.c");
    fs::write(&c,format!(r#"{header}
#include <assert.h>
typedef uint32_t (*Call)(lang_component_v1_binding *,lang_component_v1_control *,lang_component_v1_bytes,lang_component_v1_result **,lang_component_v1_error *);
typedef uint32_t (*Info)(const lang_component_v1_result *,lang_component_v1_result_info *,lang_component_v1_error *);
uint32_t typed_call(Call call,Info info,lang_component_v1_binding *binding,lang_component_v1_control *control,int64_t value,lang_component_v1_result **result,lang_component_v1_error *error) {{
    lang_component_v1_api api={{0}};{arguments} args={{0}};lc_value_writer writer={{0}};lc_value_outcome outcome;uint8_t scratch[1024];
    api.call=call;api.result_info=info;args.p0=value;
    writer.data=scratch;writer.capacity=sizeof(scratch);writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
    outcome={name}_call(&api,control,binding,&args,&writer,error);
    if(outcome.status==0)assert(outcome.poisoned==0);*result=outcome.result;return outcome.status;
}}
int64_t typed_result(lang_component_v1_bytes bytes) {{
    {result} value=0;lc_value_reader reader={{0}};
    reader.data=bytes.data;reader.length=bytes.length;reader.contextual=1;
    reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
    assert({name}_decode_result(&reader,&value));return value;
}}
"#,header=header.header,arguments=function.arguments,name=function.name,result=function.result)).unwrap();
    let c_object = files
        .0
        .join(if cfg!(windows) { "host.obj" } else { "host.o" });
    let output = Command::new("clang")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(&c)
        .arg("-o")
        .arg(&c_object)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let projection = br#"{"idl":1,"package":"demo:caller","version":"1.0.0","interfaces":[{"key":"demo:object/api","resources":[],"types":[],"functions":[{"name":"answer","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"demo.object.answer"}]}],"imports":[{"interface":"demo:object/api","version":{"exact":"1.0.0"},"members":["answer"]}],"exports":[]}"#;
    let mut session = wire::Session::new(Default::default());
    let mut projection: serde_json::Value = serde_json::from_slice(projection).unwrap();
    if task {
        projection["package"] = serde_json::json!("demo:object");
        projection["interfaces"][0]["functions"][0]["effect"] = serde_json::json!("task");
        projection["imports"] = serde_json::json!([]);
        projection["exports"] = serde_json::json!(["demo:object/api"]);
    }
    let projection = serde_json::to_vec(&projection).unwrap();
    let request = wire::Contract::new(
        idl::check(lang_mir::unit::SourceId(1), &projection, Default::default()).unwrap(),
        vec![],
        &mut session,
    )
    .unwrap();
    fs::write(
        root.join("request.bin"),
        wire::encode_contract(&request, &mut session).unwrap(),
    )
    .unwrap();
    let driver = files.0.join("consumer.rs");
    fs::write(
        &driver,
        include_str!("support/component_native_image_host.rs")
            .replace("@PACKAGE@", &format!("{:?}", root))
            .replace("@TASK@", if task { "true" } else { "false" })
            .replace("@CALLBACK@", if callback { "true" } else { "false" })
            .replace("@RESOURCE@", if resource { "true" } else { "false" })
            .replace(
                "@PROVIDER@",
                include_str!("support/component_native_image_provider.rs"),
            ),
    )
    .unwrap();
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
    let mut compiler = Command::new("rustc");
    compiler
        .current_dir(&files.0)
        .args(["--edition=2024", "--crate-type=bin"])
        .arg(&driver)
        .arg("-o")
        .arg(&executable)
        .arg("-L")
        .arg(format!("dependency={}", dependencies.display()));
    for name in [
        "lang",
        "lang_codegen_clif",
        "lang_mir",
        "lang_runtime",
        "serde_json",
    ] {
        compiler
            .arg("--extern")
            .arg(format!("{name}={}", artifacts::library(name).display()));
    }
    for path in [&object, &c_object] {
        let mut arg = std::ffi::OsString::from("link-arg=");
        arg.push(path);
        compiler.arg("-C").arg(arg);
    }
    let output = compiler.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::rename(&root, files.0.join("package-away")).unwrap();
    fs::rename(&driver, files.0.join("consumer-away.rs")).unwrap();
    let output = Command::new(&executable)
        .current_dir(&files.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "exit={:?}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "native image C: 42; panic; reuse; fuel; owner released"
    );
}
