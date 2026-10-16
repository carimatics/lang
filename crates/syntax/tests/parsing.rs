use lang_diagnostics::{Applicability, SourceMap, codes};
use lang_syntax::{
    ExpectedSyntax, ParseError, ParseLimits, SyntaxElement, SyntaxKind as S, SyntaxTree,
    TokenKind as K, parse, parse_expression,
};

fn parsed(text: &str, expression: bool) -> (SourceMap, SyntaxTree) {
    let mut sources = SourceMap::new();
    let source = sources.add("test.lang", text).unwrap();
    let tree = if expression {
        parse_expression(&sources, source, ParseLimits::default())
    } else {
        parse(&sources, source, ParseLimits::default())
    }
    .unwrap();
    validate(&sources, &tree, text);
    (sources, tree)
}

fn validate(sources: &SourceMap, tree: &SyntaxTree, original: &str) {
    assert_eq!(tree.reconstruct(sources).unwrap(), original);
    assert_eq!(
        tree.node(tree.root()).unwrap().span().range(),
        0..original.len()
    );
    let mut visited = vec![0; tree.nodes().len()];
    let mut tokens = Vec::new();
    let mut pending = vec![SyntaxElement::Node(tree.root())];
    while let Some(element) = pending.pop() {
        match element {
            SyntaxElement::Node(id) => {
                visited[id.index()] += 1;
                let node = tree.node(id).unwrap();
                sources.slice(node.span()).unwrap();
                for child in node.children() {
                    let span = match child {
                        SyntaxElement::Node(child) => {
                            assert!(child.index() < id.index());
                            tree.node(*child).unwrap().span()
                        }
                        SyntaxElement::Token(index) => tree.tokens()[*index].span,
                        SyntaxElement::Missing { span, .. } => {
                            assert!(span.range().is_empty());
                            *span
                        }
                    };
                    assert!(span.range().start >= node.span().range().start);
                    assert!(span.range().end <= node.span().range().end);
                }
                pending.extend(node.children().iter().rev().copied());
            }
            SyntaxElement::Token(index) => tokens.push(index),
            SyntaxElement::Missing { span, .. } => {
                sources.slice(span).unwrap();
            }
        }
    }
    assert!(
        visited.iter().all(|count| *count == 1),
        "every arena node is reachable exactly once"
    );
    assert_eq!(tokens, (0..tree.tokens().len()).collect::<Vec<_>>());
    for diagnostic in tree.diagnostics() {
        diagnostic.validate(sources).unwrap();
    }
}

fn shape(sources: &SourceMap, tree: &SyntaxTree) -> String {
    fn node(sources: &SourceMap, tree: &SyntaxTree, id: lang_syntax::NodeId) -> String {
        let syntax = tree.node(id).unwrap();
        let parts: Vec<_> = syntax
            .children()
            .iter()
            .filter_map(|element| match element {
                SyntaxElement::Node(id) => Some(node(sources, tree, *id)),
                SyntaxElement::Token(index) => {
                    let t = tree.tokens()[*index];
                    (!t.kind.is_trivia()).then(|| sources.slice(t.span).unwrap().to_owned())
                }
                SyntaxElement::Missing { expected, .. } => Some(format!("MISSING({expected:?})")),
            })
            .collect();
        format!("{:?}({})", syntax.kind(), parts.join(" "))
    }
    node(sources, tree, tree.root())
}

#[test]
fn complete_accepted_grammar_is_lossless() {
    let (_, tree) = parsed(include_str!("fixtures/accepted.lang"), false);
    assert!(tree.diagnostics().is_empty(), "{:?}", tree.diagnostics());
    for kind in [
        S::Module,
        S::Import,
        S::Host,
        S::Record,
        S::Enum,
        S::Function,
        S::Let,
        S::Assign,
        S::If,
        S::While,
        S::Return,
        S::Match,
        S::RecordPattern,
        S::CasePattern,
        S::BindingPattern,
        S::WildcardPattern,
        S::LiteralPattern,
    ] {
        assert!(
            tree.nodes().iter().any(|n| n.kind() == kind),
            "missing {kind:?}"
        );
    }
}

#[test]
fn precedence_associativity_and_named_calls_have_a_stable_shape() {
    let (sources, tree) = parsed("a + b + c.d < f(1, 2)", true);
    assert!(!tree.has_errors());
    assert_eq!(
        shape(&sources, &tree),
        "ExpressionRoot(LessThan(Add(Add(Name(Path(a)) + Name(Path(b))) + Field(Name(Path(c)) . d)) < Call(Name(Path(f)) Arguments(( Literal(1) , Literal(2) )))))"
    );
}

#[test]
fn parentheses_change_precedence_and_allow_record_heads() {
    let (_, tree) = parsed(
        "fn main() -> i64 { if (R { a: 1 }).a < 2 { return (1 + 2) + 3; } while (E {}).flag { return 1; } match (R { a: 1 }) { _ => { return 2; }, } return 0; }",
        false,
    );
    assert!(!tree.has_errors(), "{:?}", tree.diagnostics());
    assert_eq!(
        tree.nodes()
            .iter()
            .filter(|n| n.kind() == S::Construction)
            .count(),
        3
    );
}

#[test]
fn invalid_expression_forms_cannot_be_silently_accepted() {
    for text in [
        "1 < 2 < 3",
        "a-1",
        "(f)(1)",
        "value.field(1)",
        "f(1)(2)",
        "match x {}",
        "_",
        "R { field }",
        "- 1",
    ] {
        let (_, tree) = parsed(text, true);
        assert!(tree.has_errors(), "accepted {text}");
    }
}

#[test]
fn source_profile_rejects_unsupported_declarations_and_shapes() {
    for text in [
        "let mut x: i64 = 1;",
        "pub use root::a;",
        "pub extern fn h(x: i64) -> i64;",
        "enum E {}",
        "enum E { None() }",
        "extern fn h() -> i64;",
        "extern fn h(x: bool) -> i64;",
        "extern fn h(x: i64, y: i64) -> i64;",
        "fn f() { return 0; }",
        "fn f() -> i64 { let x: i64 = 1; return x; }",
        "fn f() -> i64 { match true { true => { return 1; } } }",
        "fn f() -> i64 { if R { x: true } { return 1; } return 0; }",
    ] {
        let (_, tree) = parsed(text, false);
        assert!(tree.has_errors(), "accepted {text}");
    }
}

#[test]
fn bare_zero_payload_paths_are_syntactically_distinct_from_calls() {
    // Name/type resolution diagnoses parentheses on a zero-payload case.
    let (_, bare) = parsed("E::None", true);
    let (_, call) = parsed("E::None()", true);
    assert!(!bare.has_errors() && !call.has_errors());
    assert!(!bare.nodes().iter().any(|n| n.kind() == S::Call));
    assert!(call.nodes().iter().any(|n| n.kind() == S::Call));
}

#[test]
fn single_name_patterns_are_bindings_and_paths_are_case_patterns() {
    let (_, tree) = parsed(
        "fn f() -> i64 { match true { name => { return 1; }, E::None => { return 2; }, root::E::Some(false) => { return 3; }, } }",
        false,
    );
    assert!(!tree.has_errors());
    assert_eq!(
        tree.nodes()
            .iter()
            .filter(|n| n.kind() == S::BindingPattern)
            .count(),
        1
    );
    assert!(
        tree.nodes()
            .iter()
            .filter(|n| n.kind() == S::CasePattern)
            .count()
            >= 2
    );
}

#[test]
fn nested_expression_recovery_preserves_enclosing_brace_and_later_item() {
    let (sources, tree) = parsed(
        "fn a() -> i64 { return (1 + } fn b() -> i64 { return 2; }",
        false,
    );
    assert!(tree.has_errors());
    assert_eq!(
        tree.nodes()
            .iter()
            .filter(|n| n.kind() == S::Function)
            .count(),
        2
    );
    assert_eq!(
        tree.nodes().iter().filter(|n| n.kind() == S::Block).count(),
        2
    );
    assert!(shape(&sources, &tree).contains(
        "Parenthesized(( Add(Literal(1) + Error(MISSING(Expression))) MISSING(Token(RightParen)))"
    ));
    let functions: Vec<_> = tree
        .nodes()
        .iter()
        .filter(|n| n.kind() == S::Function)
        .map(|n| sources.slice(n.span()).unwrap().trim())
        .collect();
    assert_eq!(
        functions,
        [
            "fn a() -> i64 { return (1 + }",
            "fn b() -> i64 { return 2; }"
        ]
    );
}

#[test]
fn missing_block_closer_recovers_at_the_next_declaration() {
    let (_, tree) = parsed(
        "fn a() -> i64 { return 1; fn b() -> i64 { return 2; }",
        false,
    );
    assert!(tree.has_errors());
    assert_eq!(
        tree.nodes()
            .iter()
            .filter(|n| n.kind() == S::Function)
            .count(),
        2
    );
    assert!(tree.diagnostics().iter().any(|d| {
        d.fixes
            .iter()
            .any(|f| f.edits.iter().any(|e| e.replacement == "}"))
    }));
}

#[test]
fn delimiter_fixes_are_valid_previews_and_never_guess_an_expression() {
    let (sources, tree) = parsed("fn f() -> i64 { return 1 }", false);
    assert_eq!(tree.diagnostics().len(), 1);
    let diagnostic = &tree.diagnostics()[0];
    assert_eq!(diagnostic.code, codes::EXPECTED_SYNTAX);
    assert_eq!(diagnostic.fixes.len(), 1);
    let fix = &diagnostic.fixes[0];
    assert_eq!(fix.applicability, Applicability::MachineApplicable);
    let changes = fix.preview(&sources).unwrap();
    assert!(!parsed(&changes[0].text, false).1.has_errors());
    let (_, missing_expression) = parsed("fn f() -> i64 { return ; }", false);
    assert!(
        missing_expression
            .diagnostics()
            .iter()
            .all(|d| d.fixes.is_empty())
    );
    assert!(
        missing_expression
            .nodes()
            .iter()
            .flat_map(|n| n.children())
            .any(|e| matches!(
                e,
                SyntaxElement::Missing {
                    expected: ExpectedSyntax::Expression,
                    ..
                }
            ))
    );
}

#[test]
fn recovery_retains_unicode_comments_and_every_invalid_real_token() {
    let (_, tree) = parsed(
        "// 😀\n? { bad(1); } fn a() -> i64 { @ (oops { x; }); return 1; } /* 尾 */",
        false,
    );
    assert!(tree.has_errors());
    assert!(tree.nodes().iter().any(|n| n.kind() == S::Function));
    assert_eq!(
        tree.tokens()
            .iter()
            .filter(|t| t.kind == K::Invalid)
            .count(),
        2
    );
}

#[test]
fn bad_nested_call_arguments_do_not_end_the_enclosing_function() {
    let (_, tree) = parsed(
        "fn a() -> i64 { return f(@ { bad; }, 3); return 7; } fn b() -> i64 { return 2; }",
        false,
    );
    assert!(tree.has_errors());
    assert_eq!(
        tree.nodes()
            .iter()
            .filter(|n| n.kind() == S::Function)
            .count(),
        2
    );
    assert_eq!(
        tree.nodes()
            .iter()
            .filter(|n| n.kind() == S::Return)
            .count(),
        3
    );
    let (_, tree) = parsed(
        "fn a() -> i64 { match x { Pair { a: binding } => { return 1; }, } }",
        false,
    );
    assert!(!tree.has_errors());
    assert_eq!(
        tree.nodes()
            .iter()
            .filter(|n| n.kind() == S::BindingPattern)
            .count(),
        1
    );
}

#[test]
fn all_valid_fixture_prefixes_are_recoverable_and_lossless() {
    let fixture = include_str!("fixtures/accepted.lang");
    for end in 0..=fixture.len() {
        if fixture.is_char_boundary(end) {
            parsed(&fixture[..end], false);
        }
    }
}

fn limited(input: &str, limits: ParseLimits, expression: bool) {
    let mut sources = SourceMap::new();
    let source = sources.add("limit", input).unwrap();
    let result = if expression {
        parse_expression(&sources, source, limits)
    } else {
        parse(&sources, source, limits)
    };
    let ParseError::Limit(diagnostic) = result.unwrap_err() else {
        panic!("expected parser resource limit")
    };
    assert_eq!(diagnostic.code, codes::RESOURCE_LIMIT);
    diagnostic.validate(&sources).unwrap();
}

#[test]
fn parser_work_nodes_diagnostics_and_lists_have_exact_limits() {
    limited(
        "",
        ParseLimits {
            nodes: 0,
            ..ParseLimits::default()
        },
        false,
    );
    limited(
        "",
        ParseLimits {
            work: 0,
            ..ParseLimits::default()
        },
        false,
    );
    let mut sources = SourceMap::new();
    let source = sources.add("empty", "").unwrap();
    assert!(
        parse(
            &sources,
            source,
            ParseLimits {
                nodes: 1,
                work: 1,
                depth: 1,
                ..ParseLimits::default()
            }
        )
        .is_ok()
    );
    limited(
        "fn",
        ParseLimits {
            diagnostics: 1,
            ..ParseLimits::default()
        },
        false,
    );
    limited(
        "?",
        ParseLimits {
            diagnostics: 0,
            ..ParseLimits::default()
        },
        false,
    );
    limited(
        "fn f(a: i64, b: i64) -> i64 { return a; }",
        ParseLimits {
            list_elements: 1,
            ..ParseLimits::default()
        },
        false,
    );
    limited(
        "1 + 2",
        ParseLimits {
            nodes: 3,
            ..ParseLimits::default()
        },
        true,
    );
}

#[test]
fn deep_parentheses_patterns_and_left_associative_trees_stop_without_stack_overflow() {
    limited(
        &format!("{}1{}", "(".repeat(4096), ")".repeat(4096)),
        ParseLimits::default(),
        true,
    );
    limited(
        &format!("{}1", "1 + ".repeat(1024)),
        ParseLimits::default(),
        true,
    );
    limited(
        &format!(
            "fn f() -> i64 {{ match true {{ {}_{} => {{ return 1; }}, }} }}",
            "E::Some(".repeat(1024),
            ")".repeat(1024)
        ),
        ParseLimits::default(),
        false,
    );
    limited(
        "(((1)))",
        ParseLimits {
            depth: 3,
            ..ParseLimits::default()
        },
        true,
    );
}

#[test]
fn default_depth_and_list_limits_accept_the_boundary_and_reject_the_next_value() {
    let exact = format!("{}1{}", "(".repeat(254), ")".repeat(254));
    assert!(!parsed(&exact, true).1.has_errors());
    let excessive = format!("({exact})");
    limited(&excessive, ParseLimits::default(), true);
    // A larger configured number must not bypass the Rust recursion ceiling.
    limited(
        &excessive,
        ParseLimits {
            depth: usize::MAX,
            ..ParseLimits::default()
        },
        true,
    );
    let params: Vec<_> = (0..256).map(|i| format!("p{i}: i64")).collect();
    let exact = format!("fn f({}) -> i64 {{ return 0; }}", params.join(", "));
    assert!(!parsed(&exact, false).1.has_errors());
    let excessive = format!(
        "fn f({}, extra: i64) -> i64 {{ return 0; }}",
        params.join(", ")
    );
    limited(&excessive, ParseLimits::default(), false);
}

#[test]
fn tree_text_requires_its_original_snapshot_even_for_empty_input() {
    let (_, tree) = parsed("", false);
    assert!(tree.reconstruct(&SourceMap::new()).is_err());
    let mut sources = SourceMap::new();
    let source = sources.add("foreign", "").unwrap();
    assert!(matches!(
        parse(&SourceMap::new(), source, ParseLimits::default()),
        Err(ParseError::Lex(lang_syntax::LexError::Source(_)))
    ));
}

#[test]
fn deterministic_malformed_inputs_terminate_with_a_tree_or_explicit_limit() {
    let atoms = [
        "fn", " x", " -> ", "i64", "return", "let", "mod", "match", "_", "true", "::", ".", "pub",
        "(", ")", "{", "}", ";", ",", "+", "é", "/*", "*/", "\n", "\0",
    ];
    let mut state = 0x3282_12fb_u32;
    for _ in 0..1000 {
        let mut text = String::new();
        for _ in 0..80 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            text.push_str(atoms[(state >> 16) as usize % atoms.len()]);
        }
        let mut sources = SourceMap::new();
        let source = sources.add("mixed", &text).unwrap();
        let limits = ParseLimits {
            work: 10_000,
            diagnostics: 512,
            ..ParseLimits::default()
        };
        for result in [
            parse(&sources, source, limits),
            parse_expression(&sources, source, limits),
        ] {
            match result {
                Ok(tree) => validate(&sources, &tree, &text),
                Err(ParseError::Limit(diagnostic)) => {
                    diagnostic.validate(&sources).unwrap();
                }
                Err(error) => panic!("unexpected error: {error}"),
            }
        }
    }
}
