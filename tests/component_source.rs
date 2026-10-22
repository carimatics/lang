use lang::{frontend, native};
use lang_diagnostics::SourceMap;
use lang_hir::resolved as r;
use lang_mir::unit::{self as m, ownership as o};
use lang_semantics::analysis::Program;
use std::path::Path;

fn analyze(text: &str) -> frontend::Analysis {
    frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("component-source.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    )
}

#[test]
fn scalar_component_imports_retain_distinct_source_and_mir_identity_and_spans() {
    for call in ["42", "ping(42)"] {
        let text = format!(
            "extern \"component\" fn ping(value:i64)->i64;fn main()->i64{{return {call};}}"
        );
        let out = analyze(&text);
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        let Program::Ownership(program) = out.result.unwrap() else {
            panic!("component identity requires ownership MIR")
        };
        let source = program.hir().loaned().moved().bodies().source();
        assert_eq!(source.imports[0].abi, r::ImportAbi::Component);
        assert!(
            out.sources
                .slice(source.imports[0].span)
                .unwrap()
                .contains("\"component\"")
        );
        let declarations = program
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations();
        assert_eq!(
            declarations.imports()[0].abi,
            o::declarations::ImportAbi::Component
        );
        let span = declarations.component_feature_span().unwrap();
        let snapshot = &declarations.types().table().sources[span.source.0 as usize].text;
        assert!(snapshot[span.start as usize..span.end as usize].contains("\"component\""));
        let error = lang_bytecode::ownership::encode(
            program
                .mir()
                .activity()
                .moved()
                .typed()
                .clone()
                .into_table(),
            None,
            Default::default(),
        )
        .unwrap_err();
        assert_eq!(error.offset, 0);
        assert!(
            matches!(error.kind,lang_bytecode::ownership::CodecErrorKind::UnsupportedComponentProfile{span:s} if s==span)
        );
        for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            assert!(
                matches!(native::compile(Program::Ownership(program.clone()),&out.sources,Default::default(),native::target(target).unwrap()),Err(native::Error::OwnedCodec(ref e)) if matches!(e.kind,lang_bytecode::ownership::CodecErrorKind::UnsupportedComponentProfile{..}))
            );
        }
        let main = m::FunctionId(
            declarations
                .functions()
                .iter()
                .position(|f| f.name == "main")
                .unwrap() as u32,
        );
        let mut instance = lang_runtime::cleanup::Instance::default();
        let error = lang_vm::unit::owned::execute_function::<lang_runtime::allocation::System>(
            program.failure_flow(),
            main,
            &mut instance,
            None,
            lang_vm::unit::Control::with_limits(Default::default()),
        )
        .err()
        .unwrap();
        assert!(
            matches!(error.reason,lang_vm::unit::owned::Reason::Preparation(lang_runtime::layout::Error::UnsupportedComponentProfile{span:s}) if s==span)
        );
        assert_eq!(error.usage.fuel, 0);
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn component_declarations_admit_rich_sync_types_without_becoming_c_imports() {
    let out = analyze(
        "pub struct Tag{} struct Request{delta:i64,label:core::Text} extern \"component\" fn apply(state:&mut core::ComponentResource<Tag>,request:Request)->core::Result<i64,core::Text>; fn forward(state:&mut core::ComponentResource<Tag>,request:Request)->core::Result<i64,core::Text>{return apply(state,request);}fn main()->i64{return 42;}",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let Program::Ownership(program) = out.result.unwrap() else {
        panic!("component proof")
    };
    let declarations = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    assert_eq!(
        declarations.imports()[0].abi,
        o::declarations::ImportAbi::Component
    );
    assert!(declarations.foreign_interface_span().is_none());
}

#[test]
fn host_c_and_component_declaration_markers_remain_separate() {
    for (marker, expected) in [
        ("", r::ImportAbi::Host),
        ("\"C\" ", r::ImportAbi::C),
        ("\"component\" ", r::ImportAbi::Component),
    ] {
        let out = analyze(&format!(
            "extern {marker}fn ping(value:i64)->i64;fn main()->i64{{return 42;}}"
        ));
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(out.result.unwrap().source().imports[0].abi, expected);
    }
}
