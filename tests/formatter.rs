use lang_bytecode::unit::Program;
use lang_codegen_clif::unit as native;
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::HostFunctionRegistry;
use lang_syntax::{ParseLimits, format, parse};
use lang_vm::unit::{self as vm, Control, HostBindings};
#[path = "support/source.rs"]
mod source;
use source::Files;
fn formatted(text: &str) -> String {
    let mut sources = SourceMap::new();
    let source = sources.add("format.lang", text).unwrap();
    let tree = parse(&sources, source, ParseLimits::default()).unwrap();
    format::format(&tree, &sources, format::Limits::default())
        .unwrap()
        .text
}
fn vm_result(unit: m::VerifiedUnit) -> vm::Outcome {
    vm::execute_entry(
        &Program::from_verified(unit),
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::default(),
    )
    .unwrap()
}

#[test]
fn formatted_multi_file_aggregate_scripts_keep_execution_results_and_instruction_counts() {
    let text = include_str!("../examples/scripts/answer.lang");
    let child = include_str!("../examples/scripts/math.lang");
    let files = Files::new(text);
    std::fs::write(files.0.join("math.lang"), child).unwrap();
    let before = vm_result(files.unit());
    std::fs::write(files.0.join("source.lang"), formatted(text)).unwrap();
    std::fs::write(files.0.join("math.lang"), formatted(child)).unwrap();
    let after = vm_result(files.unit());
    assert_eq!(before, after);
    assert_eq!(after.value.as_i64(), Some(42));
}

#[test]
fn formatter_preserves_parsed_scalar_vm_and_native_behavior() {
    let text = "// 😀\nfn main()->i64{let mut i:i64=0;let mut sum:i64=0;while i<4{sum=sum+i;i=i+1;}return sum;}";
    let files = Files::new(text);
    let before = files.unit();
    std::fs::write(files.0.join("source.lang"), formatted(text)).unwrap();
    let after = files.unit();
    assert_eq!(vm_result(before.clone()), vm_result(after.clone()));
    for unit in [before, after] {
        let jit = native::compile(
            &unit,
            m::FunctionId(0),
            &[],
            &HostFunctionRegistry::new(),
            m::Limits::default(),
        )
        .unwrap();
        assert_eq!(jit.invoke_i64_0().unwrap(), 6);
    }
}
