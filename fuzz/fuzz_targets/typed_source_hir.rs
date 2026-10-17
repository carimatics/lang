#![no_main]

use lang_diagnostics::SourceMap;
use lang_hir::{
    core::Operation,
    layout::{LayoutError, LayoutLimits},
    program::{CheckError, CheckLimits, check_program},
    resolved as r,
    source::Name,
    types,
};
use libfuzzer_sys::fuzz_target;

// A debug snapshot supplies stable valid spans. Semantic IR is intentionally
// constructed independently of parsing to exercise the foreign-input boundary.
fuzz_target!(|bytes: &[u8]| {
    let text = "R a b E None Some f x i64 bool next value }";
    let mut sources = SourceMap::new();
    let source = sources.add("hir-fuzz.lang", text).unwrap();
    let span = sources.span(source, 0..text.len()).unwrap();
    let name = |word: &str| {
        let start = text.find(word).unwrap();
        Name {
            text: word.into(),
            span: sources.span(source, start..start + word.len()).unwrap(),
        }
    };
    let byte = |index: usize| bytes.get(index).copied().unwrap_or(0);
    let ty = |value: u8| match value % 7 {
        0 => r::Type::I64,
        1 => r::Type::Bool,
        2 => r::Type::Nominal(r::DefinitionId(0)),
        3 => r::Type::Nominal(r::DefinitionId(1)),
        4 => r::Type::Nominal(r::DefinitionId(99)),
        5 => r::Type::Constructed(r::ConstructedTypeId(0)),
        _ => r::Type::Constructed(r::ConstructedTypeId(99)),
    };
    let use_type = |value_type| r::TypeUse {
        value_type,
        span: name("i64").span,
    };
    let definitions = vec![
        r::Definition {
            module: r::ModuleId(0),
            name: name("R"),
            qualified_name: "R".into(),
            public: true,
            span,
            kind: r::DefinitionKind::Record(vec![
                r::Field {
                    name: name("a"),
                    value_type: use_type(ty(byte(0))),
                },
                r::Field {
                    name: name("b"),
                    value_type: use_type(r::Type::Bool),
                },
            ]),
        },
        r::Definition {
            module: r::ModuleId(0),
            name: name("E"),
            qualified_name: "E".into(),
            public: true,
            span,
            kind: r::DefinitionKind::Enum(vec![
                r::Case {
                    name: name("None"),
                    payload: vec![],
                },
                r::Case {
                    name: name("Some"),
                    payload: vec![use_type(ty(byte(1)))],
                },
            ]),
        },
    ];
    let mut expressions = Vec::new();
    for chunk in bytes.chunks(4).take(32) {
        let a = r::ExpressionId(chunk.get(1).copied().unwrap_or(0) as usize);
        let b = r::ExpressionId(chunk.get(2).copied().unwrap_or(0) as usize);
        let kind = match chunk[0] % 13 {
            0 => r::ExpressionKind::Integer(i64::from(chunk[0] as i8)),
            1 => r::ExpressionKind::Boolean(chunk[0] & 16 != 0),
            2 => r::ExpressionKind::Local(r::LocalId(a.0)),
            3 => r::ExpressionKind::Add(a, b),
            4 => r::ExpressionKind::LessThan(a, b),
            5 => r::ExpressionKind::Call {
                callee: r::Callable::Guest(r::FunctionId(b.0)),
                arguments: vec![a],
            },
            6 => r::ExpressionKind::Call {
                callee: r::Callable::Host(r::ImportId(b.0)),
                arguments: vec![a],
            },
            7 => r::ExpressionKind::Case {
                definition: r::DefinitionId(a.0),
                index: b.0,
            },
            8 => r::ExpressionKind::Record {
                definition: r::DefinitionId(b.0),
                fields: vec![r::FieldValue {
                    name: name("a"),
                    value: a,
                }],
            },
            9 => r::ExpressionKind::Borrow { kind: r::LoanKind::Shared, place: r::PlaceId(a.0) },
            10 => r::ExpressionKind::Dereference(a),
            11 => r::ExpressionKind::CoreCall { operation: Operation::ALL[b.0 % Operation::ALL.len()], type_arguments: vec![use_type(ty(chunk[0]))], arguments: vec![a], callee_span: span },
            _ => r::ExpressionKind::Field {
                receiver: a,
                name: name("b"),
            },
        };
        expressions.push(r::Expression { kind, span });
    }
    if expressions.is_empty() {
        expressions.push(r::Expression {
            kind: r::ExpressionKind::Integer(0),
            span,
        });
    }
    let root = r::ExpressionId(expressions.len() - 1);
    let mut blocks = Vec::new();
    let mut patterns = Vec::new();
    let statement = if byte(0) & 128 == 0 {
        r::StatementKind::Return(root)
    } else {
        let return_value = r::ExpressionId(expressions.len());
        expressions.push(r::Expression {
            kind: r::ExpressionKind::Integer(0),
            span,
        });
        let kind = match byte(2) % 5 {
            0 => r::PatternKind::Wildcard,
            1 => r::PatternKind::Boolean(false),
            2 => r::PatternKind::Integer(0),
            3 => r::PatternKind::Case {
                definition: r::DefinitionId(1),
                index: byte(3) as usize,
                payload: vec![],
                explicit_payload: false,
            },
            _ => r::PatternKind::Record {
                definition: r::DefinitionId(0),
                fields: vec![],
            },
        };
        patterns.push(r::Pattern { kind, span });
        blocks.push(r::Block {
            statements: vec![r::Statement {
                kind: r::StatementKind::Return(return_value),
                span,
            }],
            span,
            closing_brace: sources.span(source, text.len() - 1..text.len()).unwrap(),
        });
        r::StatementKind::Match {
            scrutinee: root,
            arms: vec![r::Arm {
                pattern: r::PatternId(0),
                body: r::BlockId(0),
                span,
            }],
        }
    };
    let body = r::BlockId(blocks.len());
    blocks.push(r::Block {
        statements: vec![r::Statement {
            kind: statement,
            span,
        }],
        span,
        closing_brace: sources.span(source, text.len() - 1..text.len()).unwrap(),
    });
    let function = r::Function {
        module: r::ModuleId(0),
        name: name("f"),
        qualified_name: "f".into(),
        public: false,
        parameters: vec![r::LocalId(0)],
        result: use_type(ty(byte(4))),
        locals: vec![r::Local {
            name: name("x"),
            kind: r::BindingKind::Parameter,
            annotation: Some(use_type(ty(byte(5)))),
        }],
        expressions,
        patterns,
        blocks,
        body,
        span,
        signature_span: span,
    };
    let constructed = if byte(31) & 1 == 0 { vec![] } else {
        bytes.chunks(3).take(32).map(|chunk| {
            let a = ty(chunk.get(1).copied().unwrap_or(0));
            let b = ty(chunk.get(2).copied().unwrap_or(0));
            let constructor = match chunk[0] % 12 {
                0 => r::TypeConstructor::Builtin(r::BuiltinType::Text),
                1 => r::TypeConstructor::Builtin(r::BuiltinType::Allocator),
                2 => r::TypeConstructor::Option(a),
                3 => r::TypeConstructor::Result(a, b),
                4 => r::TypeConstructor::Vec(a),
                5 => r::TypeConstructor::Map(a, b),
                6 => r::TypeConstructor::Shared(a),
                7 => r::TypeConstructor::Weak(a),
                8 => r::TypeConstructor::Arena(a),
                9 => r::TypeConstructor::Loan(r::LoanKind::Shared, a),
                10 => r::TypeConstructor::Loan(r::LoanKind::Exclusive, a),
                _ => r::TypeConstructor::Builtin(r::BuiltinType::Unit),
            };
            r::ConstructedType { constructor, span }
        }).collect()
    };
    let program = r::Program {
        modules: vec![r::Module {
            qualified_name: String::new(),
            span,
        }],
        definitions,
        types: constructed,
        imports: vec![r::HostImport {
            module: r::ModuleId(0),
            name: name("next"),
            qualified_name: "next".into(),
            parameter: name("value"),
            span,
        }],
        functions: vec![function],
    };
    let limits = CheckLimits {
        types: types::Limits { constructed: 32, definitions: 8, functions: 8, modules: 4, members: 8, depth: 8, work: 16_384, allocations: 1024 * 1024, diagnostics: 32 },
        layout: LayoutLimits {
            definitions: 8,
            fields: 8,
            depth: 8,
            cells: 128,
            diagnostics: 32,
            work: 4096,
        },
        work: 8192,
        exhaustiveness: 4096,
        diagnostics: 32,
        witness_bytes: 256,
        ..CheckLimits::default()
    };
    match types::analyze(&program, &sources, limits.types) {
        Ok(analysis) => for diagnostic in analysis.diagnostics { diagnostic.validate(&sources).unwrap(); },
        Err(types::Error::Limit(d)) => d.validate(&sources).unwrap(),
        Err(types::Error::Input { .. } | types::Error::AllocationFailed { .. }) => {},
    }
    match check_program(program, &sources, limits) {
        Ok(analysis) => {
            for diagnostic in analysis.diagnostics {
                diagnostic.validate(&sources).unwrap();
            }
            if let Some(program) = analysis.program {
                assert_eq!(
                    program.functions()[0].expressions().len(),
                    program.source().functions[0].expressions.len()
                );
            }
        }
        Err(CheckError::Limit(d) | CheckError::Layout(LayoutError::Limit(d)) | CheckError::Types(types::Error::Limit(d))) => {
            d.validate(&sources).unwrap();
        }
        Err(CheckError::Input { .. } | CheckError::Layout(LayoutError::Input { .. }) | CheckError::Types(types::Error::Input { .. } | types::Error::AllocationFailed { .. }) | CheckError::OwnershipProfile { .. }) => {}
    }
});
