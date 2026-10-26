#![allow(
    dead_code,
    reason = "Shared source and stored-object consumer fixture helpers."
)]
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
    with_unit(unit(), run)
}

pub fn artifact() -> Vec<u8> {
    lang::artifact::component::encode(
        unit(),
        m::Source {
            path: "component-object.idl.json".into(),
            text: String::from_utf8(IDL.to_vec()).unwrap(),
        },
        None,
        Default::default(),
    )
    .unwrap()
}

pub fn with_artifact<T>(
    bytes: &[u8],
    run: impl FnOnce(&program::Program<'_>, &mut wire::Session, m::FunctionId) -> T,
) -> T {
    let unit = lang::artifact::component::decode(bytes, Default::default())
        .unwrap()
        .into_unit();
    with_unit(unit, run)
}

fn unit() -> bindings::Unit {
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
    out.result.unwrap()
}

fn with_unit<T>(
    unit: bindings::Unit,
    run: impl FnOnce(&program::Program<'_>, &mut wire::Session, m::FunctionId) -> T,
) -> T {
    let mut session = wire::Session::new(Default::default());
    let contract = if unit
        .code()
        .declarations()
        .types()
        .table()
        .sources
        .get(unit.document().document().source.0 as usize)
        .is_some()
    {
        wire::artifact::source_contract(&unit, &mut session, 65_536).unwrap()
    } else {
        // The direct source fixture has not embedded its independent IDL snapshot.
        wire::Contract::new(
            idl::check(unit.document().document().source, IDL, Default::default()).unwrap(),
            vec![],
            &mut session,
        )
        .unwrap()
    };
    let providers: [native::Provider<'_>; 0] = [];
    let bound = grants::prepare(&unit, &contract, &[], || &providers, &mut session).unwrap();
    let program = program::Program::prepare(&bound, Default::default()).unwrap();
    let entry = unit.exports()[0].0;
    run(&program, &mut session, entry)
}
