//! Structured-input HIR conformance. These fixtures do not implement a parser.

use lang_codegen_clif::{CompiledFunction, emit_host_object};
use lang_diagnostics::{SourceId, SourceMap, Span};
use lang_hir::{AnalysisLimits, source as s};
use lang_semantics::lowering::{self, LimitKind, LowerError, LoweringLimits};

const LIMITS: LoweringLimits = LoweringLimits {
    work: 10_000_000,
    values: 10_000,
    blocks: 1_000,
};

struct Fixture {
    sources: SourceMap,
    source: SourceId,
    input: s::Function,
}

impl Fixture {
    fn new(text: &str, parameters: &[&str]) -> Self {
        let mut sources = SourceMap::new();
        let source = sources.add("structured-input.lang", text).unwrap();
        let name = text.find("sample").unwrap();
        let open = text.find('{').unwrap();
        let mut fixture = Self {
            input: s::Function {
                name: s::Name {
                    text: "sample".into(),
                    span: sources.span(source, name..name + 6).unwrap(),
                },
                parameters: Vec::new(),
                expressions: Vec::new(),
                body: s::BlockId(0),
                blocks: vec![s::Block {
                    statements: Vec::new(),
                    span: sources.span(source, open..text.len()).unwrap(),
                    closing_brace: sources.span(source, text.len() - 1..text.len()).unwrap(),
                }],
                span: sources.span(source, 0..text.len()).unwrap(),
                signature_span: sources.span(source, 0..open).unwrap(),
            },
            sources,
            source,
        };
        fixture.input.parameters = parameters
            .iter()
            .map(|name| fixture.name(name, 0))
            .collect();
        fixture
    }

    fn span(&self, text: &str, occurrence: usize) -> Span {
        let start = self
            .sources
            .file(self.source)
            .unwrap()
            .text()
            .match_indices(text)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("missing {text:?} occurrence {occurrence}"))
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

    fn expr(&mut self, kind: s::ExpressionKind, text: &str, occurrence: usize) -> s::ExpressionId {
        let id = s::ExpressionId(self.input.expressions.len());
        self.input.expressions.push(s::Expression {
            kind,
            span: self.span(text, occurrence),
        });
        id
    }

    fn local(&mut self, name: &str, occurrence: usize) -> s::ExpressionId {
        self.expr(
            s::ExpressionKind::Name(self.name(name, occurrence)),
            name,
            occurrence,
        )
    }
    fn integer(&mut self, value: i64, occurrence: usize) -> s::ExpressionId {
        self.expr(
            s::ExpressionKind::Integer(value),
            &value.to_string(),
            occurrence,
        )
    }
    fn stmt(&self, kind: s::StatementKind, text: &str) -> s::Statement {
        s::Statement {
            kind,
            span: self.span(text, 0),
        }
    }

    fn block(&mut self, statements: Vec<s::Statement>, text: &str) -> s::BlockId {
        let span = self.span(text, 0);
        let end = span.range().end;
        let id = s::BlockId(self.input.blocks.len());
        self.input.blocks.push(s::Block {
            statements,
            span,
            closing_brace: self.sources.span(self.source, end - 1..end).unwrap(),
        });
        id
    }

    fn hir(&self) -> lang_hir::Function {
        let result = lang_hir::analyze(
            &self.input,
            &self.sources,
            AnalysisLimits {
                work: 100_000,
                locals: 100,
                depth: 128,
                diagnostics: 100,
            },
        )
        .unwrap();
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        result.function.unwrap()
    }

    fn lower(&self) -> lang_mir::Function {
        let function = lowering::lower(&self.hir(), &self.sources, LIMITS).unwrap();
        function.validate().unwrap();
        let bytecode = lang_bytecode::lower(&function).unwrap();
        bytecode.verify().unwrap();
        assert_eq!(bytecode.source_map(), function.source_map());
        function
    }
}

fn check_one(function: &lang_mir::Function, cases: &[(i64, i64)]) {
    let bytecode = lang_bytecode::lower(function).unwrap();
    let jit = CompiledFunction::compile(function).unwrap();
    for &(input, expected) in cases {
        assert_eq!(
            lang_vm::execute(&bytecode, &[input]).unwrap(),
            expected,
            "VM input {input}"
        );
        assert_eq!(
            jit.invoke_i64_1(input).unwrap(),
            expected,
            "JIT input {input}"
        );
    }
}

fn check_two(function: &lang_mir::Function, cases: &[(i64, i64, i64)]) {
    let bytecode = lang_bytecode::lower(function).unwrap();
    let jit = CompiledFunction::compile(function).unwrap();
    for &(left, right, expected) in cases {
        assert_eq!(
            lang_vm::execute(&bytecode, &[left, right]).unwrap(),
            expected,
            "VM inputs {left}, {right}"
        );
        assert_eq!(
            jit.invoke_i64_2(left, right).unwrap(),
            expected,
            "JIT inputs {left}, {right}"
        );
    }
}

#[test]
fn wrapping_add_preserves_expression_return_and_native_debug_spans() {
    let mut f = Fixture::new(
        "fn sample(left: i64, right: i64) -> i64 {\n    return left + right;\n}",
        &["left", "right"],
    );
    let left = f.local("left", 1);
    let right = f.local("right", 1);
    let add = f.expr(s::ExpressionKind::Add(left, right), "left + right", 0);
    f.input.blocks[0].statements =
        vec![f.stmt(s::StatementKind::Return(add), "return left + right;")];
    let function = f.lower();
    check_two(
        &function,
        &[
            (20, 22, 42),
            (i64::MAX, 1, i64::MIN),
            (i64::MIN, -1, i64::MAX),
            (-4, 7, 3),
        ],
    );
    let map = function.source_map().unwrap();
    assert_eq!(map.source_path(), "structured-input.lang");
    assert_eq!(map.blocks()[0].instruction_spans()[0].start().line(), 2);
    assert_eq!(map.blocks()[0].instruction_spans()[0].start().column(), 12);
    assert_eq!(map.blocks()[0].terminator_span().start().column(), 5);
    let jit = CompiledFunction::compile(&function).unwrap();
    assert_eq!(jit.source_map().unwrap().source_path(), map.source_path());
    let object = emit_host_object(&function).unwrap();
    assert_eq!(
        object.source_map().unwrap().function_span(),
        map.function_span()
    );
    assert!(object.code_size() > 0);
    assert!(!object.bytes().is_empty());
}

#[test]
fn if_assignment_joins_only_changed_values_and_covers_both_paths() {
    let mut f = Fixture::new(
        "fn sample(left: i64, right: i64) -> i64 { let mut result: i64 = left; if left < right { result = right; } return result; }",
        &["left", "right"],
    );
    let initial = f.local("left", 1);
    let left = f.local("left", 2);
    let right_condition = f.local("right", 1);
    let condition = f.expr(
        s::ExpressionKind::LessThan(left, right_condition),
        "left < right",
        0,
    );
    let right_assign = f.local("right", 2);
    let assignment = f.stmt(
        s::StatementKind::Assign {
            target: f.name("result", 1),
            value: right_assign,
        },
        "result = right;",
    );
    let then_block = f.block(vec![assignment], "{ result = right; }");
    let result = f.local("result", 2);
    f.input.blocks[0].statements = vec![
        f.stmt(
            s::StatementKind::LetMut {
                name: f.name("result", 0),
                initializer: initial,
            },
            "let mut result: i64 = left;",
        ),
        f.stmt(
            s::StatementKind::If {
                condition,
                then_block,
                else_block: None,
            },
            "if left < right { result = right; }",
        ),
        f.stmt(s::StatementKind::Return(result), "return result;"),
    ];
    let function = f.lower();
    assert_eq!(
        function
            .blocks()
            .iter()
            .map(|block| block.parameters().len())
            .sum::<usize>(),
        1
    );
    check_two(
        &function,
        &[
            (1, 2, 2),
            (2, 1, 2),
            (-8, -3, -3),
            (i64::MIN, i64::MAX, i64::MAX),
            (5, 5, 5),
        ],
    );
}

#[test]
fn returning_both_arms_does_not_create_an_unreachable_join() {
    let mut f = Fixture::new(
        "fn sample(left: i64, right: i64) -> i64 { if left < right { return left; } else { return right; } }",
        &["left", "right"],
    );
    let left_cond = f.local("left", 1);
    let right_cond = f.local("right", 1);
    let condition = f.expr(
        s::ExpressionKind::LessThan(left_cond, right_cond),
        "left < right",
        0,
    );
    let left_value = f.local("left", 2);
    let right_value = f.local("right", 2);
    let left_return = f.stmt(s::StatementKind::Return(left_value), "return left;");
    let right_return = f.stmt(s::StatementKind::Return(right_value), "return right;");
    let then_block = f.block(vec![left_return], "{ return left; }");
    let else_block = f.block(vec![right_return], "{ return right; }");
    f.input.blocks[0].statements = vec![f.stmt(
        s::StatementKind::If {
            condition,
            then_block,
            else_block: Some(else_block),
        },
        "if left < right { return left; } else { return right; }",
    )];
    let function = f.lower();
    assert_eq!(function.blocks().len(), 3);
    check_two(
        &function,
        &[(1, 2, 1), (2, 1, 1), (i64::MIN, i64::MAX, i64::MIN)],
    );
}

fn conditional_sum() -> Fixture {
    let mut f = Fixture::new(
        "fn sample(start: i64, limit: i64) -> i64 { let mut total: i64 = 0; let mut current: i64 = start; while 0 < current { if current < limit { total = total + current; } current = current + -1; } return total; }",
        &["start", "limit"],
    );
    let zero = f.integer(0, 0);
    let initial = f.local("start", 1);
    let zero_condition = f.integer(0, 1);
    let current_condition = f.local("current", 1);
    let condition = f.expr(
        s::ExpressionKind::LessThan(zero_condition, current_condition),
        "0 < current",
        0,
    );
    let current_inner = f.local("current", 2);
    let limit = f.local("limit", 1);
    let inner_condition = f.expr(
        s::ExpressionKind::LessThan(current_inner, limit),
        "current < limit",
        0,
    );
    let total_add = f.local("total", 2);
    let current_add = f.local("current", 3);
    let add_total = f.expr(
        s::ExpressionKind::Add(total_add, current_add),
        "total + current",
        0,
    );
    let total_assignment = f.stmt(
        s::StatementKind::Assign {
            target: f.name("total", 1),
            value: add_total,
        },
        "total = total + current;",
    );
    let add_block = f.block(vec![total_assignment], "{ total = total + current; }");
    let inner_if = f.stmt(
        s::StatementKind::If {
            condition: inner_condition,
            then_block: add_block,
            else_block: None,
        },
        "if current < limit { total = total + current; }",
    );
    let decrement_current = f.local("current", 5);
    let minus_one = f.integer(-1, 0);
    let decrement = f.expr(
        s::ExpressionKind::Add(decrement_current, minus_one),
        "current + -1",
        0,
    );
    let decrement_statement = f.stmt(
        s::StatementKind::Assign {
            target: f.name("current", 4),
            value: decrement,
        },
        "current = current + -1;",
    );
    let body = f.block(
        vec![inner_if, decrement_statement],
        "{ if current < limit { total = total + current; } current = current + -1; }",
    );
    let result = f.local("total", 3);
    f.input.blocks[0].statements = vec![
        f.stmt(s::StatementKind::LetMut { name: f.name("total", 0), initializer: zero }, "let mut total: i64 = 0;"),
        f.stmt(s::StatementKind::LetMut { name: f.name("current", 0), initializer: initial }, "let mut current: i64 = start;"),
        f.stmt(s::StatementKind::While { condition, body }, "while 0 < current { if current < limit { total = total + current; } current = current + -1; }"),
        f.stmt(s::StatementKind::Return(result), "return total;"),
    ];
    f
}

#[test]
fn nested_branch_and_loop_carry_two_values_through_joins_and_back_edges() {
    let function = conditional_sum().lower();
    assert!(
        function
            .blocks()
            .iter()
            .any(|block| block.parameters().len() == 2)
    );
    assert!(
        function
            .blocks()
            .iter()
            .any(|block| block.parameters().len() == 1)
    );
    check_two(
        &function,
        &[
            (0, 3, 0),
            (-2, 3, 0),
            (5, 3, 3),
            (5, 5, 10),
            (5, 8, 15),
            (5, -1, 0),
        ],
    );
    // IDs, block ordering, and debug spans are deterministic for the same HIR.
    assert_eq!(function, conditional_sum().lower());
}

#[test]
fn early_return_in_a_loop_keeps_the_other_branch_and_false_loop_exit() {
    let mut f = Fixture::new(
        "fn sample(value: i64) -> i64 { let mut current: i64 = value; while 0 < current { if current < 3 { return current; } current = current + -1; } return current; }",
        &["value"],
    );
    let initial = f.local("value", 1);
    let zero = f.integer(0, 0);
    let while_current = f.local("current", 1);
    let condition = f.expr(
        s::ExpressionKind::LessThan(zero, while_current),
        "0 < current",
        0,
    );
    let inner_current = f.local("current", 2);
    let three = f.integer(3, 0);
    let inner_condition = f.expr(
        s::ExpressionKind::LessThan(inner_current, three),
        "current < 3",
        0,
    );
    let early_value = f.local("current", 3);
    let early_return = f.stmt(s::StatementKind::Return(early_value), "return current;");
    let early_block = f.block(vec![early_return], "{ return current; }");
    let branch = f.stmt(
        s::StatementKind::If {
            condition: inner_condition,
            then_block: early_block,
            else_block: None,
        },
        "if current < 3 { return current; }",
    );
    let decrement_value = f.local("current", 5);
    let minus_one = f.integer(-1, 0);
    let decrement = f.expr(
        s::ExpressionKind::Add(decrement_value, minus_one),
        "current + -1",
        0,
    );
    let assign = f.stmt(
        s::StatementKind::Assign {
            target: f.name("current", 4),
            value: decrement,
        },
        "current = current + -1;",
    );
    let body = f.block(
        vec![branch, assign],
        "{ if current < 3 { return current; } current = current + -1; }",
    );
    let result = f.local("current", 6);
    let last_return = s::Statement {
        kind: s::StatementKind::Return(result),
        span: f.span("return current;", 1),
    };
    f.input.blocks[0].statements = vec![
        f.stmt(
            s::StatementKind::LetMut {
                name: f.name("current", 0),
                initializer: initial,
            },
            "let mut current: i64 = value;",
        ),
        f.stmt(
            s::StatementKind::While { condition, body },
            "while 0 < current { if current < 3 { return current; } current = current + -1; }",
        ),
        last_return,
    ];
    check_one(
        &f.lower(),
        &[(-5, -5), (0, 0), (1, 1), (2, 2), (3, 2), (7, 2)],
    );
}

#[test]
fn unreachable_well_typed_statements_do_not_change_execution_or_mir() {
    let mut f = Fixture::new(
        "fn sample(value: i64) -> i64 { return value; return 99; }",
        &["value"],
    );
    let value = f.local("value", 1);
    let unused = f.integer(99, 0);
    f.input.blocks[0].statements = vec![
        f.stmt(s::StatementKind::Return(value), "return value;"),
        f.stmt(s::StatementKind::Return(unused), "return 99;"),
    ];
    let function = f.lower();
    assert_eq!(function.blocks().len(), 1);
    assert!(function.blocks()[0].instructions().is_empty());
    check_one(
        &function,
        &[(0, 0), (i64::MAX, i64::MAX), (i64::MIN, i64::MIN)],
    );
}

#[test]
fn lowering_budgets_and_foreign_source_maps_return_errors_not_artifacts() {
    let f = conditional_sum();
    let hir = f.hir();
    for (limits, kind) in [
        (LoweringLimits { work: 0, ..LIMITS }, LimitKind::Work),
        (
            LoweringLimits {
                values: 0,
                ..LIMITS
            },
            LimitKind::Values,
        ),
        (
            LoweringLimits {
                blocks: 0,
                ..LIMITS
            },
            LimitKind::Blocks,
        ),
    ] {
        assert_eq!(
            lowering::lower(&hir, &f.sources, limits),
            Err(LowerError::LimitExceeded(kind))
        );
    }
    assert_eq!(
        lowering::lower(&hir, &SourceMap::new(), LIMITS),
        Err(LowerError::Source(
            lang_diagnostics::SourceError::UnknownSource
        ))
    );
}

#[test]
fn a_nonterminating_structured_loop_still_obeys_existing_vm_fuel_control() {
    let mut f = Fixture::new(
        "fn sample(value: i64) -> i64 { while value < 1 { } return value; }",
        &["value"],
    );
    let value = f.local("value", 1);
    let one = f.integer(1, 0);
    let condition = f.expr(s::ExpressionKind::LessThan(value, one), "value < 1", 0);
    let body = f.block(vec![], "{ }");
    let result = f.local("value", 2);
    f.input.blocks[0].statements = vec![
        f.stmt(
            s::StatementKind::While { condition, body },
            "while value < 1 { }",
        ),
        f.stmt(s::StatementKind::Return(result), "return value;"),
    ];
    let program = lang_bytecode::lower(&f.lower()).unwrap();
    assert!(matches!(
        lang_vm::execute_with_limits(
            &program,
            &[0],
            lang_vm::ExecutionLimits::with_instruction_fuel(20)
        ),
        Err(lang_vm::VmError::InstructionFuelExhausted { consumed: 20, .. })
    ));
    assert_eq!(lang_vm::execute(&program, &[1]).unwrap(), 1);
}
