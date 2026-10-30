use lang::{
    frontend,
    package::migration::{Limits, Preview},
};
use lang_diagnostics::SourceMap;
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{Control, HostBindings, execute_entry};
use std::path::Path;

fn run(source: &str) -> i64 {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("migrated.lang"),
        source.as_bytes(),
        None,
        Default::default(),
        None,
    );
    let lang_semantics::analysis::Program::Copy(typed) = analysis.result.unwrap() else {
        panic!("scalar fixture")
    };
    let program = frontend::compile_script(&typed, &analysis.sources, Default::default()).unwrap();
    execute_entry(
        &program,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::default(),
    )
    .unwrap()
    .value
    .as_i64()
    .unwrap()
}

#[test]
fn retained_historical_source_has_a_real_checked_transition_and_repeat_noop() {
    let source = b"fn main() -> i64 {\n    let mut shared: i64 = 40;\n    let mut weak: i64 = 2;\n    return shared + weak;\n}\n";
    let current = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("old.lang"),
        source,
        None,
        Default::default(),
        None,
    );
    assert!(current.result.is_err());
    assert!(
        current
            .diagnostics
            .iter()
            .any(|d| d.code.as_str() == "SRC0024")
    );
    let preview = Preview::capture(Path::new("old.lang"), source, Limits::default()).unwrap();
    assert_eq!(preview.original(), source);
    assert_eq!(preview.edits().len(), 4);
    assert_eq!(run(preview.replacement()), 42);
    let report = serde_json::to_value(&preview).unwrap();
    assert_eq!(report["operation"], "migrate");
    assert_eq!(report["source_profile"], "rfc0006-scalar");
    assert_eq!(report["target_edition"], "2026");
    assert_eq!(report["changed"], 1);
    assert_eq!(report["applied"], 0);
    assert_eq!(report["applicability"], "checked");
    for edit in preview.edits() {
        let old = std::str::from_utf8(&source[edit.span.start..edit.span.end]).unwrap();
        assert_eq!(edit.replacement, format!("_migrated_{old}_1"));
    }
    let repeat = Preview::capture(
        Path::new("old.lang"),
        preview.replacement().as_bytes(),
        Limits::default(),
    )
    .unwrap();
    assert!(repeat.edits().is_empty());
    assert_eq!(repeat.replacement(), preview.replacement());
    assert_eq!(
        serde_json::to_value(
            Preview::capture(Path::new("old.lang"), source, Limits::default()).unwrap()
        )
        .unwrap(),
        report
    );
}

#[test]
fn pinned_producer_admission_cases_are_preserved_before_rewriting() {
    let evidence: serde_json::Value = serde_json::from_str(include_str!(
        "../docs/evidence/2026-10-31-m5-scalar-admission.json"
    ))
    .unwrap();
    for case in evidence["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let result = Preview::capture(Path::new("old.lang"), source.as_bytes(), Limits::default());
        if case["exit"] == 0 {
            let preview = result.unwrap_or_else(|e| panic!("{}: {e:?}", case["name"]));
            assert_eq!(run(preview.replacement()), 42, "{}", case["name"]);
        } else {
            assert_eq!(result.unwrap_err().code, "MIG0001", "{}", case["name"]);
        }
    }
}

#[test]
fn global_freshness_preserves_calls_loops_trivia_and_original_spans() {
    let source = "// 😀 shared weak core stay in this comment\r\nfn shared(weak:i64)->i64{let mut _migrated_shared_1:i64=0;let mut core:i64=weak;while core<42{core=core+1;}if false{return _migrated_shared_1;}else{return core;}}\r\nfn main()->i64{return shared(40);}\r\n";
    let preview =
        Preview::capture(Path::new("old.lang"), source.as_bytes(), Limits::default()).unwrap();
    assert!(
        preview
            .replacement()
            .starts_with("// 😀 shared weak core stay in this comment\r\n")
    );
    assert!(preview.replacement().contains("fn _migrated_shared_2("));
    assert!(
        preview
            .replacement()
            .contains("let mut _migrated_shared_1:i64=0;")
    );
    assert_eq!(run(preview.replacement()), 42);
    let mut applied = source.to_string();
    for edit in preview.edits().iter().rev() {
        applied.replace_range(edit.span.start..edit.span.end, &edit.replacement);
    }
    assert_eq!(applied, preview.replacement());
    assert!(
        preview
            .edits()
            .windows(2)
            .all(|w| w[0].span.end <= w[1].span.start)
    );
    let wrapped = "fn bump(shared:i64)->i64{return shared+1;}fn main()->i64{return bump(9223372036854775807);}";
    let migrated =
        Preview::capture(Path::new("old.lang"), wrapped.as_bytes(), Limits::default()).unwrap();
    assert_eq!(run(migrated.replacement()), i64::MIN);
}

#[test]
fn current_keywords_and_legacy_identifiers_are_classified_by_the_old_set() {
    let mut source = String::from("fn main()->i64{");
    for name in [
        "async",
        "task_scope",
        "spawn",
        "await",
        "yield",
        "cancel",
        "cancel_check",
        "unsafe",
        "shared",
        "weak",
        "arena",
        "core",
        "break",
        "continue",
    ] {
        source.push_str(&format!("let mut {name}:i64=1;"));
    }
    source.push_str("return break+continue+40;}");
    let preview =
        Preview::capture(Path::new("old.lang"), source.as_bytes(), Limits::default()).unwrap();
    assert_eq!(run(preview.replacement()), 42);
    assert!(preview.replacement().contains("let mut break:i64=1;"));
    assert!(preview.replacement().contains("let mut continue:i64=1;"));
    for unsupported in [
        "pub fn main()->i64{return 42;}",
        "mod helper;fn main()->i64{return 42;}",
        "fn main()->i64{return core::panic();}",
        "fn main()->i64{return 42-1;}",
        "fn main()->i64{let mut _:i64=1;return 42;}",
        "fn main()->i64{return \"C\";}",
        "fn main()->i64{break;return 42;}",
        "extern fn other()->i64;fn main()->i64{return other();}",
        "fn main()->i64{return true;}",
        "fn main()->bool{return true;}",
        "fn main()->i64{return 42;}\0",
        "/* unclosed",
    ] {
        assert_eq!(
            Preview::capture(
                Path::new("old.lang"),
                unsupported.as_bytes(),
                Limits::default()
            )
            .unwrap_err()
            .code,
            "MIG0001",
            "{unsupported}"
        );
    }
    assert_eq!(
        Preview::capture(Path::new("old.lang"), &[0xff], Limits::default())
            .unwrap_err()
            .code,
        "MIG0001"
    );
}

#[test]
fn migration_bounds_each_phase_and_exact_encoded_report() {
    let source = b"fn main()->i64{let mut shared:i64=42;return shared;}";
    let preview = Preview::capture(Path::new("old.lang"), source, Limits::default()).unwrap();
    for limits in [
        Limits {
            source_bytes: source.len() - 1,
            ..Limits::default()
        },
        Limits {
            tokens: 0,
            ..Limits::default()
        },
        Limits {
            depth: 0,
            ..Limits::default()
        },
        Limits {
            nodes: 0,
            ..Limits::default()
        },
        Limits {
            functions: 0,
            ..Limits::default()
        },
        Limits {
            phase_work: 0,
            ..Limits::default()
        },
        Limits {
            work: 0,
            ..Limits::default()
        },
        Limits {
            analysis_bytes: 0,
            ..Limits::default()
        },
        Limits {
            rewritten_bytes: source.len(),
            ..Limits::default()
        },
        Limits {
            report_bytes: 0,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            Preview::capture(Path::new("old.lang"), source, limits)
                .unwrap_err()
                .code,
            "MIG0002"
        );
    }
    let size = serde_json::to_vec(&preview).unwrap().len();
    let mut exact = Limits {
        report_bytes: size,
        ..Limits::default()
    };
    Preview::capture(Path::new("old.lang"), source, exact).unwrap();
    exact.report_bytes -= 1;
    assert_eq!(
        Preview::capture(Path::new("old.lang"), source, exact)
            .unwrap_err()
            .code,
        "MIG0002"
    );
}

#[test]
fn exact_resource_boundaries_and_unraisable_recursion_ceiling() {
    let source = b"/* outer /* inner */ */fn main()->i64{let mut shared:i64=42;return shared;}";
    let default = Limits::default();
    type SetLimit = fn(&mut Limits, usize);
    let cases: &[(usize, SetLimit)] = &[
        (default.source_bytes, |l, n| l.source_bytes = n),
        (default.tokens, |l, n| l.tokens = n),
        (default.comment_depth, |l, n| l.comment_depth = n),
        (default.depth, |l, n| l.depth = n),
        (default.nodes, |l, n| l.nodes = n),
        (default.functions, |l, n| l.functions = n),
        (default.phase_work, |l, n| l.phase_work = n),
        (default.work, |l, n| l.work = n),
        (default.analysis_bytes, |l, n| l.analysis_bytes = n),
        (default.rewritten_bytes, |l, n| l.rewritten_bytes = n),
        (default.report_bytes, |l, n| l.report_bytes = n),
    ];
    for (index, &(mut high, set)) in cases.iter().enumerate() {
        let mut low = 0;
        while low < high {
            let middle = low + (high - low) / 2;
            let mut limits = default;
            set(&mut limits, middle);
            match Preview::capture(Path::new("quoted\"😀.lang"), source, limits) {
                Ok(_) => high = middle,
                Err(error) => {
                    assert_eq!(error.code, "MIG0002", "limit {index}: {middle}");
                    low = middle + 1;
                }
            }
        }
        assert!(low > 0);
        let mut exact = default;
        set(&mut exact, low);
        Preview::capture(Path::new("quoted\"😀.lang"), source, exact).unwrap();
        set(&mut exact, low - 1);
        assert_eq!(
            Preview::capture(Path::new("quoted\"😀.lang"), source, exact)
                .unwrap_err()
                .code,
            "MIG0002"
        );
    }
    let deep = format!(
        "fn main()->i64{{return {}42{};}}",
        "(".repeat(400),
        ")".repeat(400)
    );
    assert_eq!(
        Preview::capture(
            Path::new("deep.lang"),
            deep.as_bytes(),
            Limits {
                depth: usize::MAX,
                ..default
            }
        )
        .unwrap_err()
        .code,
        "MIG0002"
    );
}

#[test]
fn generated_scalar_programs_preserve_values_and_call_control_structure() {
    for start in [0, 7, 40] {
        for enabled in [false, true] {
            for name in ["shared", "weak", "core", "await"] {
                let source = format!(
                    "fn {name}(value:i64,flag:bool)->i64{{let mut result:i64=value;if flag{{while result<42{{result=result+1;}}}}else{{result=result+2;}}return result;}}fn main()->i64{{return {name}({start},{enabled});}}"
                );
                let preview = Preview::capture(
                    Path::new("generated.lang"),
                    source.as_bytes(),
                    Limits::default(),
                )
                .unwrap();
                assert_eq!(
                    run(preview.replacement()),
                    if enabled { 42 } else { start + 2 }
                );
                let reference = source.replace(name, "ordinary");
                assert_eq!(run(preview.replacement()), run(&reference));
            }
        }
    }
}

#[test]
fn unused_initializer_still_executes_and_exhausts_fuel() {
    let source = "fn shared()->i64{while true{}return 0;}fn main()->i64{let mut weak:i64=shared();return 42;}";
    let preview =
        Preview::capture(Path::new("old.lang"), source.as_bytes(), Limits::default()).unwrap();
    let reference = source.replace("shared", "spin").replace("weak", "unused");
    for text in [preview.replacement(), &reference] {
        let analysis = frontend::analyze_virtual(
            SourceMap::new(),
            Path::new("fuel.lang"),
            text.as_bytes(),
            None,
            Default::default(),
            None,
        );
        let lang_semantics::analysis::Program::Copy(typed) = analysis.result.unwrap() else {
            panic!("scalar fixture")
        };
        let program =
            frontend::compile_script(&typed, &analysis.sources, Default::default()).unwrap();
        assert_eq!(
            execute_entry(
                &program,
                &HostFunctionRegistry::new(),
                &HostBindings::new(),
                Control::with_limits(lang_runtime::execution::Limits {
                    fuel: Some(100),
                    ..Default::default()
                })
            )
            .unwrap_err()
            .reason,
            lang_vm::unit::Reason::FuelExhausted
        );
    }
    assert!(
        preview
            .replacement()
            .contains("let mut _migrated_weak_1:i64=_migrated_shared_1();")
    );
}
