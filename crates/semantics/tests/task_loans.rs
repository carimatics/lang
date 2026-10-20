use lang_diagnostics::{SourceMap, codes};
use lang_hir::{
    loans::{self, ViolationKind},
    program::{self, CheckError, CheckLimits},
    tasks,
};
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
};
use std::path::Path;

const WORKER: &str = "async fn worker(value:i64)->i64{yield;return value;}";
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
fn accepted(text: &str) {
    let (sources, typed) = prepare(text);
    let count = typed.functions().len();
    let checked = tasks::check_loans(typed, &sources, Default::default()).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let checked = checked.program.unwrap();
    assert_eq!(checked.functions().len(), count);
    assert_eq!(checked.loaned().functions().len(), count);
}
fn rejected(text: &str, point: &str, origin: &str) {
    let (sources, typed) = prepare(text);
    let checked = tasks::check_loans(typed, &sources, Default::default()).unwrap();
    assert!(checked.program.is_none());
    for d in &checked.diagnostics {
        d.validate(&sources).unwrap();
    }
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|d| d.code == codes::BORROW_ACROSS_SUSPENSION
                && sources.slice(d.primary.span).unwrap() == point
                && d.secondary
                    .iter()
                    .any(|l| sources.slice(l.span).unwrap() == origin)),
        "{text}: {:?}",
        checked.diagnostics
    );
}

#[test]
fn shared_and_exclusive_loans_are_rejected_at_every_explicit_point() {
    for (ty, borrow) in [("&i64", "&value"), ("&mut i64", "&mut value")] {
        for (operation, point) in [
            ("yield;", "yield;"),
            ("cancel_check;", "cancel_check;"),
            (
                "let mut result:core::Result<i64,core::TaskError>=await worker(1);",
                "await worker(1)",
            ),
            (
                "let mut child:core::Task<i64>=spawn worker(1);let mut result:core::Result<i64,core::TaskError>=await child;",
                "await child",
            ),
        ] {
            rejected(
                &format!(
                    "{WORKER}async fn main()->i64{{let mut value:i64=1;let mut view:{ty}={borrow};{operation}return 0;}}"
                ),
                point,
                borrow,
            );
        }
    }
}

#[test]
fn spawn_and_cancel_do_not_suspend_or_consume_a_claim() {
    accepted(&format!(
        "{WORKER}async fn main()->i64{{let mut value:i64=1;let mut view:&i64=&value;let mut child:core::Task<i64>=spawn worker(1);cancel child;cancel child;return 0;}}"
    ));
}

#[test]
fn exiting_local_loans_end_before_joins_and_later_suspension() {
    accepted(
        "async fn main()->i64{let mut value:i64=1;if true{let mut view:&i64=&value;}yield;task_scope{let mut writer:&mut i64=&mut value;}cancel_check;return 0;}",
    );
    accepted(
        "async fn main(flag:bool)->i64{let mut value:i64=1;task_scope{if flag{let mut view:&i64=&value;return 1;}else{let mut writer:&mut i64=&mut value;return 2;}}}",
    );
    accepted(
        "async fn main()->i64{let mut value:i64=1;let mut view:&mut i64=&mut value;return 0;}",
    );
}

#[test]
fn outer_loans_are_live_at_nested_implicit_joins() {
    for (ty, borrow) in [("&i64", "&value"), ("&mut i64", "&mut value")] {
        rejected(
            &format!(
                "async fn main()->i64{{let mut value:i64=1;let mut view:{ty}={borrow};task_scope{{}}return 0;}}"
            ),
            "}",
            borrow,
        );
    }
}

#[test]
fn return_edges_keep_loans_created_by_a_discarded_branch() {
    let text = "async fn main(flag:bool)->i64{let mut left:i64=1;let mut right:i64=2;let mut view:&i64=&left;task_scope{if flag{view=&right;return 7;}}return 0;}";
    rejected(text, "return 7;", "&right");
}

#[test]
fn failure_edges_close_task_scopes_before_branch_merging() {
    let text = "async fn main(flag:bool)->i64{let mut left:i64=1;let mut right:i64=2;let mut view:&i64=&left;task_scope{if flag{view=&right;return core::panic::<i64>(7);}}return 0;}";
    rejected(text, "return core::panic::<i64>(7);", "&right");
}

#[test]
fn call_arguments_keep_earlier_loans_live_during_later_awaits() {
    rejected(
        &format!(
            "{WORKER}fn take(view:&i64,value:i64)->i64{{return value;}}async fn main()->i64{{let mut value:i64=1;return take(&value,core::result_unwrap::<i64,core::TaskError>(await worker(1)));}}"
        ),
        "await worker(1)",
        "&value",
    );
    accepted(&format!(
        "{WORKER}fn read(view:&i64)->i64{{return (*view);}}async fn main()->i64{{let mut value:i64=1;return core::result_unwrap::<i64,core::TaskError>(await worker(read(&value)));}}"
    ));
}

#[test]
fn unsafe_and_reborrows_cannot_bypass_suspension_checks() {
    rejected(
        "async fn main()->i64{let mut value:i64=1;let mut parent:&mut i64=&mut value;unsafe{let mut child:&i64=&(*parent);yield;}return 0;}",
        "yield;",
        "&(*parent)",
    );
    let (sources, typed) = prepare(
        "async fn main()->i64{let mut value:i64=1;let mut parent:&mut i64=&mut value;if true{let mut child:&i64=&(*parent);yield;}return 0;}",
    );
    let result = loans::analyze(typed.moved().clone(), &sources, Default::default()).unwrap();
    assert_eq!(
        result
            .violations
            .iter()
            .filter(|v| v.kind == ViolationKind::Suspension)
            .count(),
        2
    );
}

#[test]
fn branches_loops_and_borrowed_patterns_share_the_same_loan_state() {
    rejected(
        "async fn main(flag:bool)->i64{let mut value:i64=1;let mut view:&i64=&value;while flag{if flag{yield;}}return 0;}",
        "yield;",
        "&value",
    );
    rejected(
        "async fn main()->i64{let mut value:i64=1;let mut view:&i64=&value;match view{bound=>{cancel_check;},}return 0;}",
        "cancel_check;",
        "&value",
    );
    accepted(
        "async fn main(flag:bool)->i64{let mut value:i64=1;while flag{if flag{let mut view:&mut i64=&mut value;}yield;}return 0;}",
    );
}

#[test]
fn loan_evidence_is_not_task_mir_or_executable_admission() {
    for text in [
        "async fn main()->i64{yield;return 1;}".to_owned(),
        format!(
            "{WORKER}async fn main()->i64{{return core::result_unwrap::<i64,core::TaskError>(await worker(1));}}"
        ),
    ] {
        let (sources, typed) = prepare(&text);
        let source = typed.bodies().source().clone();
        let checked = tasks::check_loans(typed, &sources, Default::default())
            .unwrap()
            .program
            .unwrap();
        let regional =
            lang_hir::regions::analyze(checked.loaned().clone(), &sources, Default::default())
                .unwrap()
                .bodies
                .unwrap();
        assert!(matches!(
            lang_semantics::ownership_types::lower_declarations(
                &regional,
                &sources,
                Default::default()
            ),
            Err(lang_semantics::ownership_types::Error::UnsupportedBody {
                feature: lang_semantics::ownership_types::UnsupportedBody::Task,
                ..
            })
        ));
        let checked = program::analyze_program(source, &sources, CheckLimits::default()).unwrap();
        assert!(checked.program.is_none());
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|d| d.code == codes::ASYNC_UNAVAILABLE)
        );
    }
}

#[test]
fn loan_limits_source_identity_and_global_diagnostic_caps_fail_closed() {
    let text = "async fn main()->i64{let mut value:i64=1;let mut view:&i64=&value;yield;return 0;}";
    for limits in [
        loans::Limits {
            work: 0,
            ..Default::default()
        },
        loans::Limits {
            allocations: 0,
            ..Default::default()
        },
        loans::Limits {
            loans: 0,
            ..Default::default()
        },
        loans::Limits {
            depth: 0,
            ..Default::default()
        },
        loans::Limits {
            diagnostics: 0,
            ..Default::default()
        },
    ] {
        let (sources, typed) = prepare(text);
        let Err(loans::Error::Limit(d)) = tasks::check_loans(typed, &sources, limits) else {
            panic!("expected bounded refusal")
        };
        d.validate(&sources).unwrap();
    }
    let (_, typed) = prepare(text);
    assert!(matches!(
        tasks::check_loans(typed, &SourceMap::new(), Default::default()),
        Err(loans::Error::Source(_))
    ));
    let (sources, typed) = prepare(text);
    let limits = CheckLimits {
        diagnostics: 0,
        ..Default::default()
    };
    let Err(CheckError::Limit(d)) =
        program::analyze_program(typed.bodies().source().clone(), &sources, limits)
    else {
        panic!("expected global diagnostic cap")
    };
    d.validate(&sources).unwrap();
}
