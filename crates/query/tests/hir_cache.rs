//! Shared compiler/editor cache mechanics with real accepted-subset analysis.
//! The host constructs structured syntax; this is not an LSP/text parser test.

use std::sync::Arc;

use lang_diagnostics::SourceMap;
use lang_hir::{Analysis, AnalysisLimits, analyze, source as s};
use lang_query::{Graph, GraphError, Key, Limits};

enum Value {
    Source(s::Function),
    Analysis(Analysis),
}

fn function(sources: &mut SourceMap, return_name: &str) -> s::Function {
    let text = format!("fn sample(value: i64) -> i64 {{ return {return_name}; }}");
    let id = sources.add("editor.lang", text.clone()).unwrap();
    let span = |start, end| sources.span(id, start..end).unwrap();
    let parameter = text.find("value").unwrap();
    let returned = text.find("return ").unwrap() + 7;
    let opening = text.find('{').unwrap();
    s::Function {
        name: s::Name {
            text: "sample".into(),
            span: span(3, 9),
        },
        parameters: vec![s::Name {
            text: "value".into(),
            span: span(parameter, parameter + 5),
        }],
        expressions: vec![s::Expression {
            kind: s::ExpressionKind::Name(s::Name {
                text: return_name.into(),
                span: span(returned, returned + return_name.len()),
            }),
            span: span(returned, returned + return_name.len()),
        }],
        blocks: vec![s::Block {
            statements: vec![s::Statement {
                kind: s::StatementKind::Return(s::ExpressionId(0)),
                span: span(returned - 7, returned + return_name.len() + 1),
            }],
            span: span(opening, text.len()),
            closing_brace: span(text.len() - 1, text.len()),
        }],
        body: s::BlockId(0),
        span: span(0, text.len()),
        signature_span: span(0, opening),
    }
}

fn analysis(input: &Value, sources: &SourceMap) -> Arc<Value> {
    let Value::Source(input) = input else {
        panic!("expected source input")
    };
    Arc::new(Value::Analysis(
        analyze(
            input,
            sources,
            AnalysisLimits {
                work: 10_000,
                locals: 100,
                depth: 128,
                diagnostics: 100,
            },
        )
        .unwrap(),
    ))
}

#[test]
fn hir_cache_reuses_results_and_rejects_diagnostics_from_an_old_edit() {
    let mut sources = SourceMap::new();
    let mut graph = Graph::new(Limits {
        nodes: 10,
        edges: 10,
        work: 100,
    })
    .unwrap();
    let source = Key::new(1);
    let query = Key::new(2);
    graph
        .set_input(
            source,
            Arc::new(Value::Source(function(&mut sources, "value"))),
        )
        .unwrap();
    graph.define_query(query).unwrap();
    let ticket = graph.begin(query, [source]).unwrap();
    graph
        .publish(ticket, analysis(graph.get(source).unwrap(), &sources))
        .unwrap();
    let cached = graph.get(query).unwrap().clone();
    assert!(Arc::ptr_eq(&cached, graph.get(query).unwrap()));
    let Value::Analysis(initial) = cached.as_ref() else {
        panic!("expected analysis")
    };
    assert!(initial.function.is_some());
    assert!(initial.diagnostics.is_empty());

    let old = graph.begin(query, [source]).unwrap();
    let old_result = analysis(graph.get(source).unwrap(), &sources);
    graph
        .set_input(
            source,
            Arc::new(Value::Source(function(&mut sources, "unknown"))),
        )
        .unwrap();
    let current = graph.begin(query, [source]).unwrap();
    graph
        .publish(current, analysis(graph.get(source).unwrap(), &sources))
        .unwrap();
    assert_eq!(graph.publish(old, old_result), Err(GraphError::StaleTicket));
    let current_result = graph.get(query).unwrap().clone();
    let Value::Analysis(result) = current_result.as_ref() else {
        panic!("expected analysis")
    };
    assert!(result.function.is_none());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code.as_str(), "SRC0002");
    result.diagnostics[0].validate(&sources).unwrap();
    assert_eq!(
        sources.slice(result.diagnostics[0].primary.span).unwrap(),
        "unknown"
    );
    // The original immutable successful HIR result remains valid for its holder.
    assert!(initial.function.is_some());

    graph.remove(source).unwrap();
    assert!(
        graph.get(query).is_none(),
        "closed input cannot retain publishable diagnostics"
    );
    graph
        .set_input(
            source,
            Arc::new(Value::Source(function(&mut sources, "value"))),
        )
        .unwrap();
    let reopened = graph.begin(query, [source]).unwrap();
    graph
        .publish(reopened, analysis(graph.get(source).unwrap(), &sources))
        .unwrap();
    let Value::Analysis(result) = graph.get(query).unwrap().as_ref() else {
        panic!("expected analysis")
    };
    assert!(result.function.is_some());
    assert!(result.diagnostics.is_empty());
}
