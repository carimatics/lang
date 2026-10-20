//! Compile source tasks to native continuations, using the shared task driver.
use lang_runtime::{allocation as a, execution as e};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("tasks.lang"),
        include_bytes!("scripts/tasks.lang"),
        None,
        Default::default(),
        None,
    );
    assert!(
        analyzed.diagnostics.is_empty(),
        "{:?}",
        analyzed.diagnostics
    );
    let lang_semantics::analysis::Program::Tasks(program) = analyzed.result.unwrap() else {
        panic!("task program")
    };
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())?;
    let mut allocator = a::Allocator::new(a::System, Default::default())?;
    let result = native.execute(
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )?;
    assert_eq!(allocator.stats().live_allocations, 0);
    println!("{}", result.value);
    Ok(())
}
