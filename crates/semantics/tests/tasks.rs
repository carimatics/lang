//! RFC 0022 non-executable source task typing and lexical claim proofs.
use lang_diagnostics::{SourceMap, codes};
use lang_hir::{
    program::{CheckError, CheckLimits},
    resolved as r, tasks,
};
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
};
use std::path::Path;

fn bound(text: &str) -> (SourceMap, r::Program) {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("tasks.lang"),
        text.as_bytes(),
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let bound = bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
    assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
    (sources, bound.program.unwrap())
}
fn checked(text: &str) -> (SourceMap, tasks::TaskTypes) {
    let (sources, program) = bound(text);
    let result = tasks::analyze(program, &sources, CheckLimits::default()).unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    (sources, result.program.unwrap())
}
fn rejected(
    text: &str,
    code: lang_diagnostics::DiagnosticCode,
) -> Vec<lang_diagnostics::Diagnostic> {
    let (sources, program) = bound(text);
    let result = tasks::analyze(program, &sources, CheckLimits::default()).unwrap();
    assert!(result.program.is_none());
    for d in &result.diagnostics {
        d.validate(&sources).unwrap();
    }
    assert!(
        result.diagnostics.iter().any(|d| d.code == code),
        "{:?}",
        result.diagnostics
    );
    result.diagnostics
}
const WORKER: &str = "async fn worker(value:i64)->i64{yield;return value;}";

#[test]
fn fixture_binds_async_effects_and_types_tasks_and_join_results() {
    let (sources, task) = checked(include_str!("../../syntax/tests/fixtures/tasks.lang"));
    let bodies = task.bodies();
    assert_eq!(bodies.source().functions.len(), 3);
    for f in &bodies.source().functions {
        let r::FunctionEffect::Asynchronous { span } = f.effect else {
            panic!("lost async effect");
        };
        assert_eq!(sources.slice(span).unwrap(), "async");
    }
    assert!(
        bodies
            .types()
            .iter()
            .any(|t| matches!(t.constructor, r::TypeConstructor::Task(r::Type::I64)))
    );
    assert!(
        bodies
            .types()
            .iter()
            .any(|t| t.constructor == r::TypeConstructor::Builtin(r::BuiltinType::TaskError))
    );
    let index = bodies
        .source()
        .functions
        .iter()
        .position(|f| f.name.text == "main")
        .unwrap();
    let f = &bodies.source().functions[index];
    for (i, e) in f.expressions.iter().enumerate() {
        if matches!(e.kind, r::ExpressionKind::Spawn(_)) {
            assert_eq!(task.functions()[index].origins(r::ExpressionId(i)).len(), 1);
            let ty = bodies.functions()[index].expressions()[i];
            assert!(!bodies.properties(ty).unwrap().is_copy);
            assert!(bodies.properties(ty).unwrap().contains_task);
        }
        if matches!(e.kind, r::ExpressionKind::Await(_)) {
            let r::Type::Constructed(id) = bodies.functions()[index].expressions()[i] else {
                panic!("join must return Result");
            };
            assert!(matches!(
                bodies.types()[id.0].constructor,
                r::TypeConstructor::Result(r::Type::I64, _)
            ));
        }
    }
}

#[test]
fn direct_await_cancel_without_consumption_and_nested_typed_results() {
    checked(&format!(
        "{WORKER}async fn main()->i64{{let mut child:core::Task<i64>=spawn worker(1);cancel child;cancel child;let mut joined:core::Result<i64,core::TaskError>=await child;let mut direct:core::Result<i64,core::TaskError>=await worker(2);cancel_check;return core::result_unwrap::<i64,core::TaskError>(joined)+core::result_unwrap::<i64,core::TaskError>(direct);}}"
    ));
    checked(
        "async fn worker()->core::Result<i64,core::BoundsError>{return core::Result::Ok(1);}async fn main()->core::Result<core::Result<i64,core::BoundsError>,core::TaskError>{return await worker();}",
    );
}

#[test]
fn task_claim_moves_once_and_cancel_cannot_observe_a_consumed_claim() {
    for tail in [
        "let mut again:core::Result<i64,core::TaskError>=await child;",
        "cancel child;",
    ] {
        rejected(
            &format!(
                "{WORKER}async fn main()->i64{{let mut child:core::Task<i64>=spawn worker(1);let mut first:core::Result<i64,core::TaskError>=await child;{tail}return 0;}}"
            ),
            codes::INVALID_OWNER_USE,
        );
    }
    rejected(
        &format!(
            "{WORKER}async fn main()->i64{{let mut child:core::Task<i64>=spawn worker(1);let mut moved:core::Task<i64>=child;cancel child;return 0;}}"
        ),
        codes::INVALID_OWNER_USE,
    );
}

#[test]
fn task_scopes_reject_outward_assignment_and_allow_moving_inward() {
    let d = rejected(
        &format!(
            "{WORKER}async fn main()->i64{{let mut outer:core::Task<i64>=spawn worker(1);task_scope{{outer=spawn worker(2);}}return 0;}}"
        ),
        codes::INVALID_TASK,
    );
    assert!(
        d.iter()
            .any(|d| !d.secondary.is_empty() && d.message.contains("escapes"))
    );
    checked(&format!(
        "{WORKER}async fn main()->i64{{let mut outer:core::Task<i64>=spawn worker(1);task_scope{{let mut inner:core::Task<i64>=outer;let mut result:core::Result<i64,core::TaskError>=await inner;}}return 0;}}"
    ));
}

#[test]
fn branch_and_loop_claim_flows_remain_scoped() {
    checked(&format!(
        "{WORKER}async fn main(flag:bool)->i64{{let mut child:core::Task<i64>=spawn worker(1);if flag{{child=spawn worker(2);}}else{{child=spawn worker(3);}}while flag{{child=spawn worker(4);}}let mut result:core::Result<i64,core::TaskError>=await child;return core::result_unwrap::<i64,core::TaskError>(result);}}"
    ));
    rejected(
        &format!(
            "{WORKER}async fn main(flag:bool)->i64{{let mut child:core::Task<i64>=spawn worker(1);if flag{{task_scope{{child=spawn worker(2);}}}}return 0;}}"
        ),
        codes::INVALID_TASK,
    );
}

#[test]
fn async_context_call_effect_and_operand_rules_are_checked() {
    for text in [
        format!("{WORKER}fn main()->i64{{return core::result_unwrap::<i64,core::TaskError>(await worker(1));}}"),
        format!("{WORKER}async fn main()->i64{{return worker(1);}}"),
        "fn sync()->i64{return 1;}async fn main()->i64{let mut t:core::Task<i64>=spawn sync();return 0;}".into(),
        "async fn main()->i64{return core::result_unwrap::<i64,core::TaskError>(await 1);}".into(),
        "fn main()->i64{yield;return 0;}".into(),
        "fn main()->i64{task_scope{}return 0;}".into(),
        "async fn main()->i64{cancel 1;return 0;}".into(),
    ] { rejected(&text, codes::INVALID_TASK); }
}

#[test]
fn task_claims_cannot_be_stored_nested_passed_or_returned() {
    for text in [
        "struct Bad{child:core::Task<i64>}fn main()->i64{return 0;}",
        "async fn bad(child:core::Task<i64>)->i64{return 0;}",
        "async fn bad()->core::Task<i64>{let mut child:core::Task<i64>=spawn worker(1);return child;}async fn worker(value:i64)->i64{return value;}",
        "async fn bad(v:core::Vec<core::Task<i64>>)->i64{return 0;}",
        "async fn bad(v:core::Task<core::Task<i64>>)->i64{return 0;}",
    ] {
        rejected(text, codes::INVALID_TASK);
    }
}

#[test]
fn typed_task_snapshots_do_not_authorize_loan_proofs_or_execution() {
    let text =
        "async fn main()->i64{let mut value:i64=1;let mut view:&i64=&value;yield;return value;}";
    let (sources, snapshot) = checked(text);
    assert!(matches!(
        lang_hir::loans::analyze(snapshot.moved().clone(), &sources, Default::default()),
        Err(lang_hir::loans::Error::TaskProfile { .. })
    ));
    let (_, program) = bound(text);
    let result = lang_hir::program::analyze_program(program, &sources, CheckLimits::default());
    // Use the exact source identity of the newly bound program below.
    assert!(result.is_err());
    let (sources, program) = bound(text);
    let result =
        lang_hir::program::analyze_program(program, &sources, CheckLimits::default()).unwrap();
    assert!(result.program.is_none());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == codes::ASYNC_UNAVAILABLE)
    );
}

#[test]
fn malformed_effect_spans_and_task_operands_are_rejected_without_publication() {
    let (sources, mut program) = bound("async fn main()->i64{yield;return 0;}");
    program.functions[0].effect = r::FunctionEffect::Asynchronous {
        span: program.functions[0].name.span,
    };
    assert!(matches!(
        tasks::analyze(program, &sources, CheckLimits::default()),
        Err(CheckError::Input { .. })
    ));
    let (sources, mut program) = bound(&format!(
        "{WORKER}async fn main()->i64{{return core::result_unwrap::<i64,core::TaskError>(await worker(1));}}"
    ));
    let f = program.functions.last_mut().unwrap();
    let e = f
        .expressions
        .iter_mut()
        .find(|e| matches!(e.kind, r::ExpressionKind::Await(_)))
        .unwrap();
    e.kind = r::ExpressionKind::Await(r::ExpressionId(usize::MAX));
    assert!(matches!(
        tasks::analyze(program, &sources, CheckLimits::default()),
        Err(CheckError::Input { .. })
    ));
}

#[test]
fn task_claims_reject_implicit_pattern_borrows() {
    for pattern in ["_", "moved"] {
        rejected(
            &format!(
                "{WORKER}async fn main()->i64{{let mut child:core::Task<i64>=spawn worker(1);match child{{{pattern}=>{{return 0;}},}}}}"
            ),
            codes::INVALID_TASK,
        );
    }
}

#[test]
fn task_claims_cannot_enter_core_generic_payloads_or_loans() {
    rejected(
        &format!(
            "{WORKER}async fn main()->i64{{let mut child:core::Task<i64>=spawn worker(1);match core::shared_new::<core::Task<i64>>(core::allocator(),child){{core::Result::Ok(stored)=>{{return 0;}},core::Result::Err(error)=>{{return 0;}},}}}}"
        ),
        codes::INVALID_TASK,
    );
    rejected(
        &format!(
            "{WORKER}async fn main()->i64{{let mut child:core::Task<i64>=spawn worker(1);let mut view:&core::Task<i64>=&child;return 0;}}"
        ),
        codes::INVALID_TASK,
    );
}

#[test]
fn task_error_cases_are_type_checked_but_not_executable_or_c_values() {
    let text = "fn main()->i64{match core::TaskError::Cancelled{core::TaskError::Cancelled=>{return 42;},}}";
    let (sources, raw) = bound(text);
    assert!(
        tasks::analyze(raw.clone(), &sources, CheckLimits::default())
            .unwrap()
            .program
            .is_some()
    );
    let result = lang_hir::program::analyze_program(raw, &sources, CheckLimits::default()).unwrap();
    assert!(result.program.is_none());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == codes::ASYNC_UNAVAILABLE)
    );
    rejected(
        "pub extern\"C\"fn value()->core::TaskError{return core::TaskError::Cancelled;}",
        codes::FOREIGN_BOUNDARY,
    );
    rejected(
        "extern\"C\"fn value(error:core::TaskError)->i64;fn main()->i64{return 0;}",
        codes::FOREIGN_BOUNDARY,
    );
}

#[test]
fn transfer_rules_keep_borrows_out_and_explicitly_defer_owned_send_derivation() {
    rejected(
        "async fn bad(value:&i64)->i64{return 0;}",
        codes::INVALID_TASK,
    );
    rejected(
        "async fn pending(value:core::Bytes)->core::Bytes{return value;}",
        codes::ASYNC_UNAVAILABLE,
    );
    let (sources, mut raw) = bound("async fn main()->i64{return 0;}");
    raw.functions[0].abi = r::FunctionAbi::CExport;
    let result = tasks::analyze(raw, &sources, CheckLimits::default()).unwrap();
    assert!(result.program.is_none());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == codes::INVALID_TASK)
    );
}

#[test]
fn task_analysis_limits_refuse_without_publishing_a_snapshot() {
    let (sources, raw) = bound(&format!(
        "{WORKER}async fn main()->i64{{let mut child:core::Task<i64>=spawn worker(1);return 0;}}"
    ));
    for limits in [
        CheckLimits {
            work: 0,
            ..CheckLimits::default()
        },
        CheckLimits {
            types: lang_hir::types::Limits {
                allocations: 0,
                ..Default::default()
            },
            ..CheckLimits::default()
        },
    ] {
        assert!(matches!(
            tasks::analyze(raw.clone(), &sources, limits),
            Err(CheckError::Limit(_) | CheckError::Types(lang_hir::types::Error::Limit(_)))
        ));
    }
    assert!(matches!(
        lang_hir::program::analyze_program(
            raw,
            &sources,
            CheckLimits {
                diagnostics: 0,
                ..CheckLimits::default()
            }
        ),
        Err(CheckError::Limit(_))
    ));
}
