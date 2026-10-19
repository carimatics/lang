//! Readiness experiment at 2712700, not the product native compiler.
use std::{fs, path::Path};
use lang::{frontend, frontend::CompileLimits};
use lang_codegen_clif::owned as n;
use lang_runtime::layout::Layouts;

fn main() {
    let root = std::env::args_os().nth(1).map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let text = fs::read(root.join("guest.lang")).unwrap();
    let analysis = frontend::analyze_virtual(lang_diagnostics::SourceMap::new(),
        Path::new("guest.lang"), &text, None, CompileLimits::default(), None);
    assert!(analysis.diagnostics.is_empty(), "{:?}", analysis.diagnostics);
    let lang_semantics::analysis::Program::Ownership(producer) = analysis.result.unwrap() else {
        panic!("ownership source required")
    };
    let flow = producer.failure_flow();
    let decl = flow.source().activity().moved().typed().places().declarations();
    let entry = lang_mir::unit::FunctionId(decl.functions().iter().position(|f| f.name == "main").unwrap() as u32);
    let layouts = Layouts::compile(decl.types(), Default::default()).unwrap();
    let object = n::emit_object_for_target(flow, &layouts, &lang_codegen_clif::Triple::host(), Default::default(), 4096).unwrap();
    fs::write(root.join("guest.o"), &object.bytes).unwrap();
    for index in 0..flow.functions().len() {
        println!("{}", object.function(lang_mir::unit::FunctionId(index as u32)).unwrap().metadata().symbol);
    }
    let record = object.into_recorded(Default::default()).unwrap();
    drop(layouts);
    let table = producer.into_failure_flow().into_source().into_activity().into_moved().into_typed().into_table();
    let semantic = lang_bytecode::ownership::encode(table, Some(entry), Default::default()).unwrap();
    let native = record.into_record().encode(&semantic, Default::default()).unwrap();
    fs::write(root.join("guest.lbc"), semantic).unwrap();
    fs::write(root.join("guest.native"), native).unwrap();
}
