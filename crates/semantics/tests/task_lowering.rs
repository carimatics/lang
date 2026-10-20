use lang_diagnostics::SourceMap;
use lang_hir::{program::CheckLimits, tasks};
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
};
use std::path::Path;

fn prepare(text: &str) -> (SourceMap, tasks::TaskTypes) {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("task-loans.lang"),
        text.as_bytes(),
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let bound = bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
    assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
    let checked = tasks::analyze(bound.program.unwrap(), &sources, CheckLimits::default()).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    (sources, checked.program.unwrap())
}
#[test]
fn source_task_fixture_lowers_to_independently_verified_ownership_mir() {
    let source = include_str!("../../syntax/tests/fixtures/tasks.lang");
    let (sources, typed) = prepare(source);
    let loaned = tasks::check_loans(typed, &sources, Default::default()).unwrap();
    assert!(loaned.diagnostics.is_empty(), "{:?}", loaned.diagnostics);
    let verified = lang_semantics::ownership_types::bodies::lower_tasks(
        &loaned.program.unwrap(),
        &sources,
        Default::default(),
    )
    .unwrap();
    let typed = verified.typed();
    assert_eq!(typed.functions().len(), 3);
    assert_eq!(typed.functions()[1].task_scopes.len(), 2);
    assert!(
        verified
            .plan()
            .functions()
            .iter()
            .all(|f| f.cancellation().is_some())
    );
    assert_eq!(
        typed.places().declarations().types().table().sources[0].text,
        source
    );
}
