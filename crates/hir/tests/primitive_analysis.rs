//! Structured-input conformance, deliberately independent of a lexer/parser.

use lang_diagnostics::{SourceId, SourceMap, Span, codes};
use lang_hir::{
    Analysis, AnalysisLimits, InputErrorKind, LimitKind, LocalKind, Type, Warning, analyze,
    source as s,
};

const LIMITS: AnalysisLimits = AnalysisLimits {
    work: 100_000,
    locals: 100,
    depth: 128,
    diagnostics: 100,
};

struct Fixture {
    sources: SourceMap,
    source: SourceId,
    input: s::Function,
}

impl Fixture {
    fn new(text: &str) -> Self {
        let mut sources = SourceMap::new();
        let source = sources.add("input.lang", text).unwrap();
        let function_span = sources.span(source, 0..text.len()).unwrap();
        let name_start = text.find("sample").unwrap();
        let opening = text.find('{').unwrap();
        let input = s::Function {
            name: s::Name {
                text: "sample".into(),
                span: sources.span(source, name_start..name_start + 6).unwrap(),
            },
            parameters: Vec::new(),
            expressions: Vec::new(),
            blocks: vec![s::Block {
                statements: Vec::new(),
                span: sources.span(source, opening..text.len()).unwrap(),
                closing_brace: sources.span(source, text.len() - 1..text.len()).unwrap(),
            }],
            body: s::BlockId(0),
            span: function_span,
            signature_span: sources.span(source, 0..opening).unwrap(),
        };
        Self {
            sources,
            source,
            input,
        }
    }

    fn span(&self, text: &str, occurrence: usize) -> Span {
        let source = self.sources.file(self.source).unwrap().text();
        let start = source
            .match_indices(text)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("missing occurrence {occurrence} of {text:?}"))
            .0;
        self.sources
            .span(self.source, start..start + text.len())
            .unwrap()
    }

    fn name(&self, text: &str, occurrence: usize) -> s::Name {
        s::Name {
            text: text.into(),
            span: self.span(text, occurrence),
        }
    }

    fn parameter(&mut self, text: &str, occurrence: usize) {
        self.input.parameters.push(self.name(text, occurrence));
    }

    fn expression(
        &mut self,
        kind: s::ExpressionKind,
        text: &str,
        occurrence: usize,
    ) -> s::ExpressionId {
        let id = s::ExpressionId(self.input.expressions.len());
        self.input.expressions.push(s::Expression {
            kind,
            span: self.span(text, occurrence),
        });
        id
    }

    fn integer(&mut self, value: i64, occurrence: usize) -> s::ExpressionId {
        self.expression(
            s::ExpressionKind::Integer(value),
            &value.to_string(),
            occurrence,
        )
    }

    fn local(&mut self, text: &str, occurrence: usize) -> s::ExpressionId {
        self.expression(
            s::ExpressionKind::Name(self.name(text, occurrence)),
            text,
            occurrence,
        )
    }

    fn statement(&self, kind: s::StatementKind, text: &str, occurrence: usize) -> s::Statement {
        s::Statement {
            kind,
            span: self.span(text, occurrence),
        }
    }

    fn block(
        &mut self,
        statements: Vec<s::Statement>,
        text: &str,
        occurrence: usize,
    ) -> s::BlockId {
        let id = s::BlockId(self.input.blocks.len());
        let span = self.span(text, occurrence);
        let closing = span.range().end - 1;
        self.input.blocks.push(s::Block {
            statements,
            span,
            closing_brace: self
                .sources
                .span(self.source, closing..closing + 1)
                .unwrap(),
        });
        id
    }

    fn run(&self) -> Analysis {
        let result = analyze(&self.input, &self.sources, LIMITS).unwrap();
        for diagnostic in &result.diagnostics {
            diagnostic.validate(&self.sources).unwrap();
        }
        assert_eq!(result.function.is_some(), result.diagnostics.is_empty());
        result
    }
}

#[test]
fn resolves_loop_locals_and_preserves_types_names_and_spans() {
    let mut f = Fixture::new(
        "fn sample(start: i64) -> i64 { let mut current: i64 = start; while 0 < current { current = current + -1; } return current; }",
    );
    f.parameter("start", 0);
    let start = f.local("start", 1);
    let zero = f.integer(0, 0);
    let current_condition = f.local("current", 1);
    let condition = f.expression(
        s::ExpressionKind::LessThan(zero, current_condition),
        "0 < current",
        0,
    );
    let current_add = f.local("current", 3);
    let minus_one = f.integer(-1, 0);
    let add = f.expression(
        s::ExpressionKind::Add(current_add, minus_one),
        "current + -1",
        0,
    );
    let assign = f.statement(
        s::StatementKind::Assign {
            target: f.name("current", 2),
            value: add,
        },
        "current = current + -1;",
        0,
    );
    let loop_body = f.block(vec![assign], "{ current = current + -1; }", 0);
    let result = f.local("current", 4);
    f.input.blocks[0].statements = vec![
        f.statement(
            s::StatementKind::LetMut {
                name: f.name("current", 0),
                initializer: start,
            },
            "let mut current: i64 = start;",
            0,
        ),
        f.statement(
            s::StatementKind::While {
                condition,
                body: loop_body,
            },
            "while 0 < current { current = current + -1; }",
            0,
        ),
        f.statement(s::StatementKind::Return(result), "return current;", 0),
    ];
    let analysis = f.run();
    assert!(analysis.warnings.is_empty());
    let function = analysis.function.unwrap();
    assert_eq!(function.name(), "sample");
    assert_eq!(function.span(), f.input.span);
    assert_eq!(function.signature_span(), f.input.signature_span);
    assert_eq!(function.parameters().len(), 1);
    assert_eq!(function.locals()[0].kind, LocalKind::Parameter);
    assert_eq!(function.locals()[1].kind, LocalKind::Mutable);
    assert_eq!(function.locals()[1].name, "current");
    assert_eq!(function.locals()[1].span, f.span("current", 0));
    assert_eq!(
        function
            .expressions()
            .iter()
            .filter(|expr| expr.value_type == Type::Bool)
            .count(),
        1
    );
    assert!(!function.blocks()[function.body().index()].falls_through);
    assert!(function.blocks().iter().any(|block| block.falls_through));
    for expression in function.expressions() {
        f.sources.slice(expression.span).unwrap();
    }
}

#[test]
fn declaration_is_not_visible_in_its_own_initializer() {
    let mut f = Fixture::new("fn sample() -> i64 { let mut later: i64 = later; return later; }");
    let before = f.local("later", 1);
    let after = f.local("later", 2);
    f.input.blocks[0].statements = vec![
        f.statement(
            s::StatementKind::LetMut {
                name: f.name("later", 0),
                initializer: before,
            },
            "let mut later: i64 = later;",
            0,
        ),
        f.statement(s::StatementKind::Return(after), "return later;", 0),
    ];
    let result = f.run();
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, codes::UNKNOWN_NAME);
    assert_eq!(result.diagnostics[0].primary.span, f.span("later", 1));
}

#[test]
fn nested_scope_local_does_not_leak() {
    let mut f =
        Fixture::new("fn sample() -> i64 { if 0 < 1 { let mut inner: i64 = 0; } return inner; }");
    let zero = f.integer(0, 0);
    let one = f.integer(1, 0);
    let condition = f.expression(s::ExpressionKind::LessThan(zero, one), "0 < 1", 0);
    let initializer = f.integer(0, 1);
    let declaration = f.statement(
        s::StatementKind::LetMut {
            name: f.name("inner", 0),
            initializer,
        },
        "let mut inner: i64 = 0;",
        0,
    );
    let nested = f.block(vec![declaration], "{ let mut inner: i64 = 0; }", 0);
    let out_of_scope = f.local("inner", 1);
    f.input.blocks[0].statements = vec![
        f.statement(
            s::StatementKind::If {
                condition,
                then_block: nested,
                else_block: None,
            },
            "if 0 < 1 { let mut inner: i64 = 0; }",
            0,
        ),
        f.statement(s::StatementKind::Return(out_of_scope), "return inner;", 0),
    ];
    let result = f.run();
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, codes::UNKNOWN_NAME);
    assert_eq!(result.diagnostics[0].primary.span, f.span("inner", 1));
}

#[test]
fn rejects_duplicate_parameters_with_original_declaration_label() {
    let mut f = Fixture::new("fn sample(value: i64, value: i64) -> i64 { return value; }");
    f.parameter("value", 0);
    f.parameter("value", 1);
    let value = f.local("value", 2);
    f.input.blocks[0].statements =
        vec![f.statement(s::StatementKind::Return(value), "return value;", 0)];
    let result = f.run();
    assert_eq!(result.diagnostics.len(), 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, codes::DUPLICATE_NAME);
    assert_eq!(diagnostic.primary.span, f.span("value", 1));
    assert_eq!(diagnostic.secondary[0].span, f.span("value", 0));
}

#[test]
fn rejects_name_reuse_in_disjoint_blocks() {
    let mut f = Fixture::new(
        "fn sample() -> i64 { if 0 < 1 { let mut value: i64 = 0; } else { let mut value: i64 = 1; } return 0; }",
    );
    let zero = f.integer(0, 0);
    let one = f.integer(1, 0);
    let condition = f.expression(s::ExpressionKind::LessThan(zero, one), "0 < 1", 0);
    let left_decl = f.statement(
        s::StatementKind::LetMut {
            name: f.name("value", 0),
            initializer: zero,
        },
        "let mut value: i64 = 0;",
        0,
    );
    let right_decl = f.statement(
        s::StatementKind::LetMut {
            name: f.name("value", 1),
            initializer: one,
        },
        "let mut value: i64 = 1;",
        0,
    );
    let left = f.block(vec![left_decl], "{ let mut value: i64 = 0; }", 0);
    let right = f.block(vec![right_decl], "{ let mut value: i64 = 1; }", 0);
    f.input.blocks[0].statements = vec![
        f.statement(
            s::StatementKind::If {
                condition,
                then_block: left,
                else_block: Some(right),
            },
            "if 0 < 1 { let mut value: i64 = 0; } else { let mut value: i64 = 1; }",
            0,
        ),
        f.statement(s::StatementKind::Return(zero), "return 0;", 0),
    ];
    let result = f.run();
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, codes::DUPLICATE_NAME);
    assert_eq!(result.diagnostics[0].primary.span, f.span("value", 1));
    assert_eq!(result.diagnostics[0].secondary[0].span, f.span("value", 0));
}

#[test]
fn rejects_parameter_mutation_and_reports_exact_spans() {
    let mut f = Fixture::new("fn sample(value: i64) -> i64 { value = 1; return value; }");
    f.parameter("value", 0);
    let one = f.integer(1, 0);
    let result = f.local("value", 2);
    f.input.blocks[0].statements = vec![
        f.statement(
            s::StatementKind::Assign {
                target: f.name("value", 1),
                value: one,
            },
            "value = 1;",
            0,
        ),
        f.statement(s::StatementKind::Return(result), "return value;", 0),
    ];
    let result = f.run();
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, codes::IMMUTABLE_ASSIGNMENT);
    assert_eq!(result.diagnostics[0].primary.span, f.span("value", 1));
    assert_eq!(result.diagnostics[0].secondary[0].span, f.span("value", 0));
}

#[test]
fn i64_contexts_reject_internal_bool_and_keep_operand_spans() {
    let mut f = Fixture::new(
        "fn sample() -> i64 { let mut value: i64 = 0 < 1; value = 0 < 1; return (0 < 1) + (0 < 1); }",
    );
    let zero = f.integer(0, 0);
    let one = f.integer(1, 0);
    let mut comparisons = Vec::new();
    for occurrence in 0..4 {
        comparisons.push(f.expression(s::ExpressionKind::LessThan(zero, one), "0 < 1", occurrence));
    }
    let addition = f.expression(
        s::ExpressionKind::Add(comparisons[2], comparisons[3]),
        "(0 < 1) + (0 < 1)",
        0,
    );
    f.input.blocks[0].statements = vec![
        f.statement(
            s::StatementKind::LetMut {
                name: f.name("value", 0),
                initializer: comparisons[0],
            },
            "let mut value: i64 = 0 < 1;",
            0,
        ),
        f.statement(
            s::StatementKind::Assign {
                target: f.name("value", 1),
                value: comparisons[1],
            },
            "value = 0 < 1;",
            0,
        ),
        f.statement(
            s::StatementKind::Return(addition),
            "return (0 < 1) + (0 < 1);",
            0,
        ),
    ];
    let result = f.run();
    assert_eq!(result.diagnostics.len(), 4);
    for (occurrence, diagnostic) in result.diagnostics.iter().enumerate() {
        assert_eq!(diagnostic.code, codes::EXPECTED_I64);
        assert_eq!(diagnostic.primary.span, f.span("0 < 1", occurrence));
    }
    // A boolean returned directly is also rejected at the value, not return keyword.
    f.input.blocks[0].statements = vec![f.statement(
        s::StatementKind::Return(comparisons[0]),
        "return (0 < 1) + (0 < 1);",
        0,
    )];
    let result = f.run();
    assert_eq!(result.diagnostics[0].code, codes::EXPECTED_I64);
    assert_eq!(result.diagnostics[0].primary.span, f.span("0 < 1", 0));
}

#[test]
fn if_and_while_require_bool_conditions() {
    let mut f = Fixture::new("fn sample() -> i64 { if 1 { } while 1 { } return 1; }");
    let first = f.integer(1, 0);
    let second = f.integer(1, 1);
    let third = f.integer(1, 2);
    let then_block = f.block(vec![], "{ }", 0);
    let body = f.block(vec![], "{ }", 1);
    f.input.blocks[0].statements = vec![
        f.statement(
            s::StatementKind::If {
                condition: first,
                then_block,
                else_block: None,
            },
            "if 1 { }",
            0,
        ),
        f.statement(
            s::StatementKind::While {
                condition: second,
                body,
            },
            "while 1 { }",
            0,
        ),
        f.statement(s::StatementKind::Return(third), "return 1;", 0),
    ];
    let result = f.run();
    assert_eq!(result.diagnostics.len(), 2);
    for (occurrence, diagnostic) in result.diagnostics.iter().enumerate() {
        assert_eq!(diagnostic.code, codes::EXPECTED_BOOL);
        assert_eq!(diagnostic.primary.span, f.span("1", occurrence));
    }
}

fn conditional_returns() -> Fixture {
    let mut f = Fixture::new("fn sample() -> i64 { if 0 < 1 { return 0; } else { return 1; } }");
    let zero = f.integer(0, 0);
    let one = f.integer(1, 0);
    let condition = f.expression(s::ExpressionKind::LessThan(zero, one), "0 < 1", 0);
    let left_return = f.statement(s::StatementKind::Return(zero), "return 0;", 0);
    let right_return = f.statement(s::StatementKind::Return(one), "return 1;", 0);
    let left = f.block(vec![left_return], "{ return 0; }", 0);
    let right = f.block(vec![right_return], "{ return 1; }", 0);
    f.input.blocks[0].statements = vec![f.statement(
        s::StatementKind::If {
            condition,
            then_block: left,
            else_block: Some(right),
        },
        "if 0 < 1 { return 0; } else { return 1; }",
        0,
    )];
    f
}

#[test]
fn both_conditional_arms_return_but_missing_else_and_loops_can_fall_through() {
    let mut f = conditional_returns();
    assert!(f.run().function.is_some());
    let s::StatementKind::If {
        condition,
        then_block,
        ..
    } = f.input.blocks[0].statements[0].kind
    else {
        unreachable!()
    };
    f.input.blocks[0].statements[0].kind = s::StatementKind::If {
        condition,
        then_block,
        else_block: None,
    };
    let result = f.run();
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, codes::MISSING_RETURN);
    assert_eq!(
        result.diagnostics[0].primary.span,
        f.input.blocks[0].closing_brace
    );
    assert_eq!(
        result.diagnostics[0].secondary[0].span,
        f.input.signature_span
    );
    f.input.blocks[0].statements[0].kind = s::StatementKind::While {
        condition,
        body: then_block,
    };
    assert_eq!(f.run().diagnostics[0].code, codes::MISSING_RETURN);
}

#[test]
fn unreachable_statements_warn_and_are_still_checked() {
    let mut f = Fixture::new("fn sample() -> i64 { return 1; return missing; }");
    let one = f.integer(1, 0);
    let missing = f.local("missing", 0);
    f.input.blocks[0].statements = vec![
        f.statement(s::StatementKind::Return(one), "return 1;", 0),
        f.statement(s::StatementKind::Return(missing), "return missing;", 0),
    ];
    let result = f.run();
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, codes::UNKNOWN_NAME);
    assert_eq!(
        result.warnings,
        vec![Warning::UnreachableStatement {
            span: f.span("return missing;", 0)
        }]
    );
    f.input.blocks[0].statements[1].kind = s::StatementKind::Return(one);
    let result = f.run();
    assert!(result.function.is_some());
    assert_eq!(result.warnings.len(), 1);
}

#[test]
fn rejects_malformed_arena_references_cycles_and_source_snapshots() {
    let mut f = Fixture::new("fn sample() -> i64 { return 1; }");
    let one = f.integer(1, 0);
    f.input.blocks[0].statements = vec![f.statement(s::StatementKind::Return(one), "return 1;", 0)];
    f.input.blocks[0].statements[0].kind = s::StatementKind::Return(s::ExpressionId(100));
    assert_eq!(
        analyze(&f.input, &f.sources, LIMITS).unwrap_err().kind,
        InputErrorKind::InvalidExpressionId(100)
    );
    f.input.blocks[0].statements[0].kind = s::StatementKind::Return(one);
    f.input.expressions[0].kind = s::ExpressionKind::Add(one, one);
    assert_eq!(
        analyze(&f.input, &f.sources, LIMITS).unwrap_err().kind,
        InputErrorKind::CyclicExpression
    );
    f.input.expressions[0].kind = s::ExpressionKind::Integer(1);
    f.input.blocks[0].statements[0].kind = s::StatementKind::While {
        condition: one,
        body: s::BlockId(0),
    };
    assert_eq!(
        analyze(&f.input, &f.sources, LIMITS).unwrap_err().kind,
        InputErrorKind::CyclicBlock
    );
    f.input.body = s::BlockId(999);
    assert_eq!(
        analyze(&f.input, &f.sources, LIMITS).unwrap_err().kind,
        InputErrorKind::InvalidBlockId(999)
    );
    f.input.body = s::BlockId(0);
    assert_eq!(
        analyze(&f.input, &SourceMap::new(), LIMITS)
            .unwrap_err()
            .kind,
        InputErrorKind::InvalidSpan(lang_diagnostics::SourceError::UnknownSource)
    );
    f.input.name.text = "wrong".into();
    assert_eq!(
        analyze(&f.input, &f.sources, LIMITS).unwrap_err().kind,
        InputErrorKind::NameTextMismatch
    );
    f.input.name.text = "sample".into();
    let other = f.sources.add("other.lang", "1").unwrap();
    f.input.signature_span = f.sources.span(other, 0..1).unwrap();
    assert_eq!(
        analyze(&f.input, &f.sources, LIMITS).unwrap_err().kind,
        InputErrorKind::SpanOutsideFunction
    );
}

#[test]
fn limits_cover_work_locals_depth_and_diagnostics_without_partial_success() {
    let mut f = Fixture::new("fn sample(value: i64) -> i64 { return missing; }");
    f.parameter("value", 0);
    let missing = f.local("missing", 0);
    f.input.blocks[0].statements =
        vec![f.statement(s::StatementKind::Return(missing), "return missing;", 0)];
    for (limits, kind) in [
        (AnalysisLimits { work: 0, ..LIMITS }, LimitKind::Work),
        (
            AnalysisLimits {
                locals: 0,
                ..LIMITS
            },
            LimitKind::Locals,
        ),
        (AnalysisLimits { depth: 0, ..LIMITS }, LimitKind::Depth),
        (
            AnalysisLimits {
                diagnostics: 0,
                ..LIMITS
            },
            LimitKind::Diagnostics,
        ),
    ] {
        assert_eq!(
            analyze(&f.input, &f.sources, limits).unwrap_err().kind,
            InputErrorKind::LimitExceeded(kind)
        );
    }
}

#[test]
fn depth_bombs_have_no_recursive_input_or_output_drop() {
    let mut f = Fixture::new("fn sample() -> i64 { return 1; }");
    let one = f.integer(1, 0);
    let mut expression = one;
    for _ in 0..2_000 {
        expression = f.expression(s::ExpressionKind::Add(expression, one), "1", 0);
    }
    f.input.blocks[0].statements =
        vec![f.statement(s::StatementKind::Return(expression), "return 1;", 0)];
    assert_eq!(
        analyze(
            &f.input,
            &f.sources,
            AnalysisLimits {
                depth: usize::MAX,
                ..LIMITS
            }
        )
        .unwrap_err()
        .kind,
        InputErrorKind::LimitExceeded(LimitKind::Depth)
    );
}
