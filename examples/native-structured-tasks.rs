//! Compile source tasks to native continuations, using the shared task driver.
//! An optional first argument selects a source file, including channel fixtures.
use lang_runtime::{allocation as a, execution as e};
use std::path::{Path, PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source_path = std::env::args_os().nth(1).map(PathBuf::from);
    let source = source_path
        .as_ref()
        .map(std::fs::read)
        .transpose()?
        .unwrap_or_else(|| include_bytes!("scripts/tasks.lang").to_vec());
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        source_path
            .as_deref()
            .unwrap_or_else(|| Path::new("tasks.lang")),
        &source,
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
    assert_eq!(result.tasks.live, 0);
    println!("{}", result.value);
    Ok(())
}
