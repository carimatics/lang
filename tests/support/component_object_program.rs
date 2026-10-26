use lang::frontend::component as f;
use lang_mir::unit as m;
use lang_runtime::component::{
    bindings, grants, idl, native,
    negotiation::{InterfaceKey, PackageKey},
    program, wire,
};
use std::path::Path;

const IDL: &[u8] = br#"{"idl":1,"package":"demo:object","version":"1.0.0","interfaces":[{"key":"demo:object/api","resources":[],"types":[],"functions":[{"name":"answer","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"demo.object.answer"}]}],"imports":[],"exports":["demo:object/api"]}"#;
const SOURCE: &[u8] =
    b"pub fn answer(value:i64)->i64{if value<0{return core::panic::<i64>(-42);}return value+1;}";

pub fn with_program<T>(
    run: impl FnOnce(&program::Program<'_>, &mut wire::Session, m::FunctionId) -> T,
) -> T {
    let interface = InterfaceKey::new(PackageKey::new("demo", "object"), "api");
    let out = f::analyze(f::Request {
        sources: lang_diagnostics::SourceMap::new(),
        display: Path::new("component-object.lang"),
        bytes: SOURCE,
        root: None,
        compile: Default::default(),
        document: idl::check(m::SourceId(82), IDL, Default::default()).unwrap(),
        bindings: f::Input {
            version: 1,
            resources: vec![],
            imports: vec![],
            exports: vec![f::Export {
                source: "answer".into(),
                member: bindings::MemberKey {
                    interface,
                    member: "answer".into(),
                },
            }],
        },
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let unit = out.result.unwrap();
    let mut session = wire::Session::new(Default::default());
    let contract = wire::Contract::new(
        idl::check(m::SourceId(82), IDL, Default::default()).unwrap(),
        vec![],
        &mut session,
    )
    .unwrap();
    let providers: [native::Provider<'_>; 0] = [];
    let bound = grants::prepare(&unit, &contract, &[], || &providers, &mut session).unwrap();
    let program = program::Program::prepare(&bound, Default::default()).unwrap();
    let entry = unit.exports()[0].0;
    run(&program, &mut session, entry)
}
