use lang_diagnostics::SourceMap;
use lang_syntax::{
    NodeId, ParseLimits, SyntaxElement, SyntaxKind, SyntaxTree, TokenKind as K,
    format::{self, FormatError, LimitKind, Limits, Status},
    parse, parse_expression,
};

#[derive(Debug, Eq, PartialEq)]
enum Shape {
    Open(SyntaxKind),
    Close,
    Token(String),
}
fn shape(tree: &SyntaxTree, sources: &SourceMap) -> Vec<Shape> {
    enum Pending {
        Node(NodeId),
        Element(SyntaxElement),
        Close,
    }
    let mut result = Vec::new();
    let mut pending = vec![Pending::Node(tree.root())];
    while let Some(next) = pending.pop() {
        match next {
            Pending::Node(id) => {
                let node = tree.node(id).unwrap();
                result.push(Shape::Open(node.kind()));
                pending.push(Pending::Close);
                pending.extend(node.children().iter().rev().copied().map(Pending::Element));
            }
            Pending::Element(SyntaxElement::Node(id)) => pending.push(Pending::Node(id)),
            Pending::Element(SyntaxElement::Token(i)) => {
                let token = tree.tokens()[i];
                if !token.kind.is_trivia() {
                    result.push(Shape::Token(sources.slice(token.span).unwrap().into()));
                }
            }
            Pending::Element(SyntaxElement::Missing { .. }) => {
                panic!("valid tree has no missing syntax")
            }
            Pending::Close => result.push(Shape::Close),
        }
    }
    result
}
fn comments(tree: &SyntaxTree, sources: &SourceMap) -> Vec<String> {
    tree.tokens()
        .iter()
        .filter(|t| matches!(t.kind, K::LineComment | K::BlockComment))
        .map(|t| sources.slice(t.span).unwrap().into())
        .collect()
}
fn checked(text: &str, expression: bool) -> String {
    let mut sources = SourceMap::new();
    let source = sources.add("original.lang", text).unwrap();
    let parse_source = |sources: &SourceMap, source| {
        if expression {
            parse_expression(sources, source, ParseLimits::default())
        } else {
            parse(sources, source, ParseLimits::default())
        }
    };
    let original = parse_source(&sources, source).unwrap();
    assert!(!original.has_errors(), "{:?}", original.diagnostics());
    let first = format::format(&original, &sources, Limits::default()).unwrap();
    assert_eq!(first.status, Status::Formatted);
    let source = sources.add("formatted.lang", &first.text).unwrap();
    let formatted = parse_source(&sources, source).unwrap();
    assert!(
        !formatted.has_errors(),
        "{}\n{:?}",
        first.text,
        formatted.diagnostics()
    );
    assert_eq!(
        shape(&original, &sources),
        shape(&formatted, &sources),
        "{}",
        first.text
    );
    assert_eq!(
        comments(&original, &sources),
        comments(&formatted, &sources)
    );
    let second = format::format(&formatted, &sources, Limits::default()).unwrap();
    assert_eq!(first, second, "{}", first.text);
    first.text
}

#[test]
fn four_space_blocks_and_one_statement_per_line_have_a_fixed_fixture() {
    let result = checked(
        "fn f(x:i64)->i64{let mut sum:i64=0;while sum<x{sum=sum+1;}if sum<4{return sum;}else{return -1;}}",
        false,
    );
    assert_eq!(
        result,
        "fn f(x: i64) -> i64 {\n    let mut sum: i64 = 0;\n    while sum < x {\n        sum = sum + 1;\n    }\n    if sum < 4 {\n        return sum;\n    } else {\n        return -1;\n    }\n}\n"
    );
}

#[test]
fn task_syntax_comments_and_prefix_parentheses_format_idempotently() {
    checked(include_str!("fixtures/tasks.lang"), false);
    let formatted = checked(
        "pub/*a*/async/*b*/fn f()->i64{task_scope/*c*/{let mut child:core::Task<i64>=spawn/*d*/worker(1);cancel/*e*/child;yield;cancel_check;return core::result_unwrap(await/*f*/child);}}",
        false,
    );
    assert!(
        formatted.contains("pub /*a*/ async /*b*/ fn f() -> i64"),
        "{formatted}"
    );
    assert!(formatted.contains("task_scope /*c*/ {"), "{formatted}");
    assert!(formatted.contains("spawn /*d*/ worker(1)"));
    assert!(formatted.contains("cancel /*e*/ child;"));
    assert!(formatted.contains("await /*f*/ child"));
    assert_eq!(checked("await(child.field)", true), "await (child.field)\n");
    assert_eq!(checked("spawn(worker(1))", true), "spawn (worker(1))\n");
}

#[test]
fn malformed_task_formatting_preserves_original_source_bytes() {
    for text in [
        "async fn bad()->i64{yield cancel_check;}",
        "async fn bad()->i64{task_scope{return await ;}",
        "pub extern\"C\"async fn f()->i64{return 1;}",
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add("invalid-task.lang", text).unwrap();
        let tree = parse(&sources, source, ParseLimits::default()).unwrap();
        assert!(tree.has_errors());
        let output = format::format(&tree, &sources, Limits::default()).unwrap();
        assert_eq!(output.status, Status::PreservedInvalid);
        assert_eq!(output.text, text);
    }
}

#[test]
fn complete_accepted_grammar_round_trips_with_identical_semantic_cst_structure() {
    checked(include_str!("fixtures/accepted.lang"), false);
    checked(
        "pub struct Empty{} enum E{A,B(i64,bool),} mod child;use root::child::f;extern fn next(x:i64)->i64;fn f(flag:bool)->i64{let mut e:E=E::B(next(1),flag);match e{E::A=>{return 0;},E::B(value,true)=>{return value;},E::B(other,false)=>{return other+-1;},}}",
        false,
    );
}

#[test]
fn c_markers_unsafe_blocks_and_legacy_host_forms_round_trip_without_merging() {
    checked(include_str!("fixtures/foreign.lang"), false);
    let formatted = checked(
        "extern\"C\"fn f()->core::Unit;pub extern/* keep */\"C\"fn g()->i64{unsafe/* loan */{unsafe{}}return 1;}",
        false,
    );
    assert!(
        formatted.contains("extern \"C\" fn f() -> core::Unit;"),
        "{formatted}"
    );
    assert!(formatted.contains("unsafe /* loan */ {"), "{formatted}");
}

#[test]
fn ownership_types_loans_generic_calls_and_arenas_round_trip() {
    checked(include_str!("fixtures/ownership.lang"), false);
    let text = "fn f(x:&mut core::Vec<core::Option<i64>>,a:shared Node,b:weak Node)->core::Unit{(*x).field=value;arena graph(core::allocator()){let mut p:arena Node=q;}return core::Unit{};}";
    let formatted = checked(text, false);
    assert!(
        formatted.contains("x: &mut core::Vec<core::Option<i64>>"),
        "{formatted}"
    );
    assert!(formatted.contains("(*x).field = value;"));
    assert!(formatted.contains("arena graph(core::allocator()) {"));
    assert_eq!(
        checked("core::vec_get::<core::Option<i64>>(&values,0)", true),
        "core::vec_get::<core::Option<i64>>(&values, 0)\n"
    );
    assert_eq!(checked("&mut value.field", true), "&mut value.field\n");
    assert_eq!(
        checked("a<core::vec_len::<i64>(&values)", true),
        "a < core::vec_len::<i64>(&values)\n"
    );
    checked(
        "core::result_unwrap::<core::Option<core::Bytes>,core::Text>(core::result_unwrap_err::<i64,core::Result<core::Option<core::Bytes>,core::Text>>(value))",
        true,
    );
    checked("core::option_unwrap::<core::Bytes>(value)", true);
}

#[test]
fn ownership_comment_insertion_preserves_token_ownership_and_precedence() {
    let text = "fn f(x:&mut core::Vec<core::Option<i64>>)->i64{(*x).field=y;arena graph(a){}return core::vec_len::<core::Option<i64>>(&values);}";
    let mut sources = SourceMap::new();
    let source = sources.add("ownership.lang", text).unwrap();
    let tree = parse(&sources, source, ParseLimits::default()).unwrap();
    assert!(!tree.has_errors(), "{:?}", tree.diagnostics());
    for token in tree.tokens().iter().filter(|t| !t.kind.is_trivia()) {
        for comment in [" /* α */ ", " // 😀\n"] {
            let mut changed = text.to_string();
            changed.insert_str(token.span.range().end, comment);
            checked(&changed, false);
        }
    }
}

#[test]
fn record_patterns_constructions_fields_and_parenthesized_heads_stay_distinct() {
    checked(
        "struct R { flag: bool, value: i64 } fn f()->i64{if (R{flag:true,value:1}.flag){match (R {value:2,flag:false}){R{value:x,flag:_}=>{return x;},}}else{return 0;}return 1;}",
        false,
    );
    assert_eq!(checked("(1+(-2))<4", true), "(1 + (-2)) < 4\n");
    checked(
        "root::data::E::Some(root::data::R {b:true,a:1}).a + 2",
        true,
    );
}

#[test]
fn comments_keep_exact_bytes_order_and_trailing_whitespace() {
    let text = "// 😀 leading  \r\nfn /* name */ f(x: /* type */ i64)->i64{\n// body  \nlet mut y:i64=x; // tail  \n/* outer\n  /* nested */ α */ y=y+1;/* after */return y; // return tail\n}\n/* eof */";
    let result = checked(text, false);
    assert!(result.contains("return y; // return tail\n"));
    assert!(result.contains("// 😀 leading  \r\n"));
    assert!(result.ends_with("/* eof */\n"));
}

#[test]
fn comments_between_else_parameters_and_record_members_do_not_merge_tokens() {
    for text in [
        "fn f(x: i64,// parameter\ny: i64)->i64{if x<y{return x;}/* join */else{return y;}}",
        "struct R{/* first */a:i64,// tail\nb:bool}fn f()->R{return R{/* field */b:true,a:1/* last */};}",
        "fn f()->i64{/* body only */}",
        "fn f()->i64{if true{} // before else\nelse{} return 1;}",
        "/* file */fn f()->i64{return /* literal */1;}/* tail */",
    ] {
        checked(text, false);
    }
}

#[test]
fn all_comment_insertion_boundaries_preserve_valid_parse_shapes() {
    let text = "fn f(x: i64) -> i64 { let mut y: i64 = x + -1; if y < 0 { return y; } else { return (y + 1); } }";
    let mut sources = SourceMap::new();
    let source = sources.add("test.lang", text).unwrap();
    let tree = parse(&sources, source, ParseLimits::default()).unwrap();
    let ends: Vec<_> = tree
        .tokens()
        .iter()
        .filter(|t| !t.kind.is_trivia())
        .map(|t| t.span.range().end)
        .collect();
    for end in ends {
        for comment in [" /* α */ ", " // note\n"] {
            let mut changed = text.to_string();
            changed.insert_str(end, comment);
            checked(&changed, false);
        }
    }
}

#[test]
fn invalid_files_are_preserved_byte_for_byte_including_erroneous_tokens() {
    for text in [
        "fn bad( {\n @ ??? /* unclosed",
        "fn f() -> i64 { return 1 }",
        "\0",
        "enum Empty {}",
        "fn f() -> i64 { return (1; }",
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add("invalid.lang", text).unwrap();
        let tree = parse(&sources, source, ParseLimits::default()).unwrap();
        assert!(tree.has_errors());
        let output = format::format(&tree, &sources, Limits::default()).unwrap();
        assert_eq!(output.status, Status::PreservedInvalid);
        assert_eq!(output.text, text);
    }
}

#[test]
fn empty_whitespace_and_comment_only_files_are_idempotent() {
    assert_eq!(checked("", false), "");
    assert_eq!(checked(" \t\r\n", false), "");
    assert_eq!(checked("/* comment */", false), "/* comment */\n");
    checked("// comment\r\n", false);
}

#[test]
fn foreign_snapshots_output_and_work_limits_fail_without_partial_success() {
    let mut sources = SourceMap::new();
    let source = sources
        .add("valid.lang", "fn f() -> i64 { return 1; }")
        .unwrap();
    let tree = parse(&sources, source, ParseLimits::default()).unwrap();
    assert!(matches!(
        format::format(&tree, &SourceMap::new(), Limits::default()),
        Err(FormatError::Source(_))
    ));
    assert_eq!(
        format::format(
            &tree,
            &sources,
            Limits {
                work: 0,
                ..Limits::default()
            }
        ),
        Err(FormatError::Limit(LimitKind::Work))
    );
    assert_eq!(
        format::format(
            &tree,
            &sources,
            Limits {
                output_bytes: 1,
                ..Limits::default()
            }
        ),
        Err(FormatError::Limit(LimitKind::OutputBytes))
    );
    let expected = format::format(&tree, &sources, Limits::default()).unwrap();
    assert_eq!(
        format::format(
            &tree,
            &sources,
            Limits {
                output_bytes: expected.text.len(),
                ..Limits::default()
            }
        )
        .unwrap(),
        expected
    );
}
