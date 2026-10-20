//! Bounded raw task MIR mutations; no HIR or executable authority is supplied.
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, places as p, tasks},
};

pub fn exercise(bytes: &[u8]) {
    let byte = |i| bytes.get(i).copied().unwrap_or(0u8);
    let span = m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 5,
    };
    let r = |i| m::Register(i);
    let c = |i| o::Type::Constructed(o::ConstructedId(i));
    let ty = |ty| o::TypeUse { ty, span };
    let instruction = |results, op| b::Instruction { results, op, span };
    let mut instructions = vec![
        instruction(vec![r(0)], b::Op::I64(42)),
        instruction(
            vec![r(1)],
            b::Op::TaskSpawn {
                function: m::FunctionId(0),
                arguments: vec![],
            },
        ),
        instruction(vec![], b::Op::TaskCancel(p::PlaceId(0))),
        instruction(vec![r(2)], b::Op::TaskAwait(r(1))),
        instruction(vec![], b::Op::TaskSaveReturn(r(0))),
        instruction(vec![], b::Op::TaskScopeExit(b::TaskScopeId(0))),
    ];
    let scope = b::TaskScopeId(u32::from(byte(2) % 4));
    let register = r(u32::from(byte(3) % 6));
    if byte(0) & 1 != 0 {
        instructions[usize::from(byte(1) % 6)].op = match byte(4) % 10 {
            0 => b::Op::TaskScopeEnter(scope),
            1 => b::Op::TaskScopeExit(scope),
            2 => b::Op::TaskAwait(register),
            3 => b::Op::TaskCancel(p::PlaceId(register.0)),
            4 => b::Op::TaskYield,
            5 => b::Op::TaskCheck,
            6 => b::Op::TaskSaveReturn(register),
            7 => b::Op::Move(register),
            8 => b::Op::Copy(register),
            _ => b::Op::TaskSpawn {
                function: m::FunctionId(register.0),
                arguments: vec![register],
            },
        };
    }
    let mut body = b::FunctionBody {
        task_scopes: vec![b::TaskScope { parent: None, span }],
        task_bindings: vec![b::TaskBinding {
            target: b::CleanupTarget::Value(r(1)),
            scope: b::TaskScopeId(0),
        }],
        blocks: vec![b::Block {
            parameters: vec![],
            instructions,
            terminator: b::Terminator {
                kind: b::Term::ReturnSaved,
                span,
            },
            span,
        }],
        ..Default::default()
    };
    if byte(0) & 2 != 0 {
        body.task_bindings[0].scope = scope;
        body.task_bindings[0].target = b::CleanupTarget::Value(register);
    }
    if byte(0) & 4 != 0 {
        body.task_scopes.push(b::TaskScope {
            parent: Some(scope),
            span,
        });
    }
    if byte(0) & 8 != 0 {
        body.blocks[0].terminator.kind = match byte(5) % 3 {
            0 => b::Term::Return(register),
            1 => b::Term::Unreachable,
            _ => b::Term::Jump(m::Target {
                block: m::BlockId(u32::from(byte(6) % 3)),
                arguments: vec![],
            }),
        };
    }
    let table = b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "task-fuzz.lang".into(),
                        text: "async".into(),
                    }],
                    constructed: [
                        o::Constructor::Task(o::Type::I64),
                        o::Constructor::Builtin(o::Builtin::TaskError),
                        o::Constructor::Result(o::Type::I64, c(1)),
                    ]
                    .into_iter()
                    .map(|constructor| o::Constructed { constructor, span })
                    .collect(),
                    definitions: vec![],
                },
                imports: vec![],
                functions: vec![d::Function {
                    name: "main".into(),
                    public: false,
                    span,
                    signature_span: span,
                    effect: d::FunctionEffect::Asynchronous { span },
                    abi: d::FunctionAbi::Guest,
                    parameters: vec![],
                    locals: vec![],
                    result: ty(o::Type::I64),
                }],
            },
            functions: vec![p::FunctionPlaces {
                values: vec![ty(o::Type::I64), ty(c(0)), ty(c(2))],
                places: vec![p::Place {
                    origin: r(1),
                    root: p::Root::Value(r(1)),
                    projections: vec![],
                    span,
                }],
            }],
        },
        functions: vec![body],
    };
    let mut limits = tasks::Limits {
        scopes: 16,
        bindings: 16,
        state_cells: 1024,
        failure_nodes: 1024,
        ..Default::default()
    };
    limits.bodies.places.ownership.unit = m::Limits {
        work: 100_000,
        allocations: 2 * 1024 * 1024,
        elements: 100_000,
        ..Default::default()
    };
    let actual = tasks::verify(table.clone(), limits);
    assert_eq!(actual, tasks::verify(table, limits));
    if let Ok(verified) = actual {
        assert!(verified.plan().functions()[0].cancellation().is_some());
        assert!(
            verified.typed().functions()[0].task_scopes[0]
                .parent
                .is_none()
        );
    }
}
