//! Explicit trusted producer for the no-import installed component smoke test.
use lang_runtime::component::{
    bindings, grants, idl, native,
    negotiation::{InterfaceKey, PackageKey},
    program, wire,
};
fn main() {
    let idl=br#"{"idl":1,"package":"demo:object","version":"1.0.0","interfaces":[{"key":"demo:object/api","resources":[],"types":[],"functions":[{"name":"answer","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"demo.object.answer"}]}],"imports":[],"exports":["demo:object/api"]}"#;
    let analysis=lang::frontend::component::analyze(lang::frontend::component::Request {
        sources:lang_diagnostics::SourceMap::new(),display:std::path::Path::new("installed-component.lang"),
        bytes:b"pub fn answer(value:i64)->i64{if value<0{return core::panic::<i64>(-42);}return value+1;}",
        root:None,compile:Default::default(),document:idl::check(lang_mir::unit::SourceId(82),idl,Default::default()).unwrap(),
        bindings:lang::frontend::component::Input {version:1,resources:vec![],imports:vec![],exports:vec![lang::frontend::component::Export {
            source:"answer".into(),member:bindings::MemberKey {interface:InterfaceKey::new(PackageKey::new("demo","object"),"api"),member:"answer".into()}
        }]},limits:Default::default()
    });
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let semantic = lang::artifact::component::encode(
        analysis.result.unwrap(),
        lang_mir::unit::Source {
            path: "installed-component.idl.json".into(),
            text: String::from_utf8(idl.to_vec()).unwrap(),
        },
        None,
        Default::default(),
    )
    .unwrap();
    let unit = lang::artifact::component::decode(&semantic, Default::default())
        .unwrap()
        .into_unit();
    let mut session = wire::Session::new(Default::default());
    let contract = wire::artifact::source_contract(&unit, &mut session, 65_536).unwrap();
    let providers: [native::Provider<'_>; 0] = [];
    let bound = grants::prepare(&unit, &contract, &[], || &providers, &mut session).unwrap();
    let program = program::Program::prepare(&bound, Default::default()).unwrap();
    let package = lang::native::component::compile(
        &semantic,
        &program,
        unit.exports()[0].0,
        lang_codegen_clif::Triple::host(),
        Default::default(),
    )
    .unwrap();
    package
        .publish(&std::path::PathBuf::from(
            std::env::args_os().nth(1).expect("output package"),
        ))
        .unwrap();
    if let Some(header_path) = std::env::args_os().nth(2) {
        let target = if cfg!(windows) {
            lang::component_bindings::abi::Target::WindowsX86_64Msvc
        } else {
            lang::component_bindings::abi::Target::MacosAarch64
        };
        let generated =
            lang::component_bindings::c::generate(&contract, target, Default::default()).unwrap();
        let function = &generated.functions[0];
        let header_path = std::path::PathBuf::from(header_path);
        std::fs::write(&header_path, &generated.header).unwrap();
        let client = include_str!("component-client.c.in")
            .replace("@NAME@", &function.name)
            .replace("@ARGUMENTS@", &function.arguments)
            .replace("@RESULT@", &function.result);
        std::fs::write(header_path.with_file_name("component-client.c"), client).unwrap();
    }
}
