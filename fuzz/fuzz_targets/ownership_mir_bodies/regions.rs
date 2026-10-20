//! Canonical, independently typed arena cases plus an expected-outcome oracle.
//! General malformed bodies remain in the parent generator. These cases ensure
//! accepted loan proofs reach mandatory region admission on every input.
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, places as p},
};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    }
}
fn c(id: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(id))
}
fn ty(ty: o::Type) -> o::TypeUse {
    o::TypeUse { ty, span: span() }
}
fn local(name: &str, ty: o::Type, kind: d::LocalKind) -> d::Local {
    d::Local {
        name: name.into(),
        span: span(),
        kind,
        ty: self::ty(ty),
    }
}
fn instruction(value: Option<u32>, op: b::Op) -> b::Instruction {
    b::Instruction {
        results: value.into_iter().map(m::Register).collect(),
        op,
        span: span(),
    }
}
fn call(
    value: u32,
    operation: o::core::Operation,
    arguments: &[u32],
    types: &[o::Type],
    loan: Option<u32>,
) -> b::Instruction {
    instruction(
        Some(value),
        b::Op::CoreCall(o::core::Call {
            operation,
            type_arguments: types.iter().copied().map(ty).collect(),
            arguments: arguments.iter().copied().map(m::Register).collect(),
            result_loan: loan.map(b::LoanId),
        }),
    )
}
fn borrow(value: u32, loan: u32, place: u32) -> b::Instruction {
    instruction(
        Some(value),
        b::Op::Borrow {
            loan: b::LoanId(loan),
            kind: o::LoanKind::Exclusive,
            place: p::PlaceId(place),
        },
    )
}
fn end(loan: u32) -> b::Instruction {
    instruction(
        None,
        b::Op::EndBorrow {
            loan: b::LoanId(loan),
            mode: b::EndMode::Required,
        },
    )
}
fn place(origin: u32, local: u32, dereference: bool) -> p::Place {
    p::Place {
        origin: m::Register(origin),
        root: p::Root::Local(d::LocalId(local)),
        projections: if dereference {
            vec![p::Projection {
                kind: p::ProjectionKind::Dereference,
                span: span(),
            }]
        } else {
            vec![]
        },
        span: span(),
    }
}
fn function(name: &str, locals: Vec<d::Local>, parameters: &[u32]) -> d::Function {
    d::Function {
        abi: d::FunctionAbi::Guest,
        name: name.into(),
        public: false,
        span: span(),
        signature_span: span(),
        parameters: parameters.iter().copied().map(d::LocalId).collect(),
        result: ty(o::Type::I64),
        locals,
    }
}
fn body(loans: usize, instructions: Vec<b::Instruction>, result: u32) -> b::FunctionBody {
    b::FunctionBody {
        loans,
        prepared: 0,
        blocks: vec![b::Block {
            parameters: vec![],
            instructions,
            terminator: b::Terminator {
                kind: b::Term::Return(m::Register(result)),
                span: span(),
            },
            span: span(),
        }],
    }
}

pub fn exercise(bytes: &[u8]) {
    let byte = |n| bytes.get(n).copied().unwrap_or(0);
    let mode = byte(0) % 5;
    let constructors = [
        o::Constructor::Builtin(o::Builtin::Allocator),
        o::Constructor::Loan(o::LoanKind::Shared, c(0)),
        o::Constructor::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(0),
            local: 0,
        }),
        o::Constructor::Loan(o::LoanKind::Exclusive, c(2)),
        o::Constructor::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(0),
            local: 1,
        }),
        o::Constructor::Loan(o::LoanKind::Exclusive, c(4)),
        o::Constructor::Arena(o::Type::I64),
        o::Constructor::Builtin(o::Builtin::ArenaError),
        o::Constructor::Result(c(6), c(7)),
        o::Constructor::Option(c(6)),
        o::Constructor::Loan(o::LoanKind::Exclusive, c(9)),
        o::Constructor::Builtin(o::Builtin::Unit),
        o::Constructor::Result(c(11), c(7)),
    ];
    let declarations = d::Declarations {
        types: o::TypeTable {
            sources: vec![m::Source {
                path: "arena-fuzz.lang".into(),
                text: "x".into(),
            }],
            definitions: vec![],
            constructed: constructors
                .into_iter()
                .map(|constructor| o::Constructed {
                    constructor,
                    span: span(),
                })
                .collect(),
        },
        imports: vec![],
        functions: vec![
            function(
                "f",
                vec![
                    local("one", c(2), d::LocalKind::ArenaOwner),
                    local("two", c(4), d::LocalKind::ArenaOwner),
                    local("outside", c(9), d::LocalKind::Mutable),
                    local("inside", c(9), d::LocalKind::Mutable),
                ],
                &[],
            ),
            function(
                "store",
                vec![
                    local("out", c(10), d::LocalKind::Parameter),
                    local("input", c(6), d::LocalKind::Parameter),
                ],
                &[0, 1],
            ),
        ],
    };
    let values = [
        c(1),
        o::Type::I64,
        c(3),
        c(8),
        c(6),
        c(5),
        c(8),
        c(6),
        o::Type::I64,
        c(12),
        o::Type::I64,
        c(9),
        c(9),
        c(10),
        o::Type::I64,
        c(2),
        c(4),
        c(9),
        c(9),
        c(5),
    ];
    let places = p::PlaceTable {
        declarations,
        functions: vec![
            p::FunctionPlaces {
                values: values.into_iter().map(ty).collect(),
                places: vec![
                    place(15, 0, false),
                    place(16, 1, false),
                    place(17, 2, false),
                    place(18, 3, false),
                ],
            },
            p::FunctionPlaces {
                values: [c(6), c(9), c(9), o::Type::I64, c(9)]
                    .into_iter()
                    .map(ty)
                    .collect(),
                places: vec![place(4, 0, true), place(0, 1, false)],
            },
        ],
    };
    let mut instructions = vec![
        call(0, o::core::Operation::Allocator, &[], &[], Some(0)),
        instruction(
            Some(11),
            b::Op::Variant {
                case: 0,
                payload: vec![],
            },
        ),
        instruction(
            None,
            b::Op::Initialize {
                local: d::LocalId(2),
                value: m::Register(11),
            },
        ),
        instruction(
            None,
            b::Op::ArenaEnter {
                owner: d::LocalId(0),
                allocator: m::Register(0),
            },
        ),
        instruction(Some(1), b::Op::I64(i64::from(byte(1)))),
        borrow(2, 1, 0),
        call(
            3,
            o::core::Operation::ArenaNew,
            &[2, 1],
            &[o::Type::I64],
            None,
        ),
        end(1),
        call(
            4,
            o::core::Operation::ResultUnwrap,
            &[3],
            &[c(6), c(7)],
            None,
        ),
        instruction(
            None,
            b::Op::ArenaEnter {
                owner: d::LocalId(1),
                allocator: m::Register(0),
            },
        ),
        instruction(Some(8), b::Op::I64(i64::from(byte(2)))),
        borrow(5, 2, 1),
        call(
            6,
            o::core::Operation::ArenaNew,
            &[5, 8],
            &[o::Type::I64],
            None,
        ),
        end(2),
        call(
            7,
            o::core::Operation::ResultUnwrap,
            &[6],
            &[c(6), c(7)],
            None,
        ),
    ];
    if mode == 2 {
        instructions.push(instruction(
            None,
            b::Op::ArenaExit {
                owner: d::LocalId(1),
            },
        ));
        instructions.push(instruction(
            None,
            b::Op::ArenaEnter {
                owner: d::LocalId(1),
                allocator: m::Register(0),
            },
        ));
    }
    instructions.extend([
        instruction(
            Some(12),
            b::Op::Variant {
                case: 0,
                payload: vec![],
            },
        ),
        instruction(
            None,
            b::Op::Initialize {
                local: d::LocalId(3),
                value: m::Register(12),
            },
        ),
    ]);
    if mode >= 3 {
        instructions.extend([
            borrow(13, 4, if mode == 4 { 2 } else { 3 }),
            instruction(
                Some(14),
                b::Op::Call {
                    function: m::FunctionId(1),
                    arguments: vec![m::Register(13), m::Register(7)],
                },
            ),
            end(4),
        ]);
    } else {
        instructions.extend([
            borrow(19, 3, 1),
            call(
                9,
                o::core::Operation::ArenaReplace,
                &[19, if mode == 1 { 4 } else { 7 }, 8],
                &[o::Type::I64],
                None,
            ),
            end(3),
        ]);
    }
    instructions.extend([
        instruction(None, b::Op::DropLocalIfInitialized(d::LocalId(3))),
        instruction(None, b::Op::DropLocalIfInitialized(d::LocalId(2))),
        instruction(
            None,
            b::Op::ArenaExit {
                owner: d::LocalId(1),
            },
        ),
        instruction(
            None,
            b::Op::ArenaExit {
                owner: d::LocalId(0),
            },
        ),
        end(0),
        instruction(Some(10), b::Op::I64(0)),
    ]);
    let store = vec![
        instruction(Some(0), b::Op::CopyPlace(p::PlaceId(1))),
        instruction(
            Some(1),
            b::Op::Variant {
                case: 1,
                payload: vec![m::Register(0)],
            },
        ),
        instruction(
            None,
            b::Op::Replace {
                place: p::PlaceId(0),
                value: m::Register(1),
            },
        ),
        instruction(
            Some(2),
            b::Op::Variant {
                case: 0,
                payload: vec![],
            },
        ),
        instruction(
            None,
            b::Op::Replace {
                place: p::PlaceId(0),
                value: m::Register(2),
            },
        ),
        instruction(None, b::Op::DropLocalIfInitialized(d::LocalId(0))),
        instruction(None, b::Op::DropLocalIfInitialized(d::LocalId(1))),
        instruction(Some(3), b::Op::I64(0)),
    ];
    let typed = b::BodyTable {
        places,
        functions: vec![body(5, instructions, 10), body(0, store, 3)],
    }
    .verify(Default::default())
    .unwrap();
    let moved = o::moves::analyze(typed, Default::default()).unwrap();
    let active = o::loan_activity::analyze(moved, Default::default()).unwrap();
    let loaned = o::loans::analyze(active, Default::default()).unwrap();
    let mut limited = o::failure::Limits::default();
    match byte(3) % 6 {
        0 => limited.regions.type_nodes = 0,
        1 => limited.regions.paths = 0,
        2 => limited.regions.edges = 0,
        3 => limited.regions.analysis_nodes = 0,
        4 => limited.regions.state_cells = 0,
        _ => limited.regions.depth = 0,
    }
    assert!(
        o::failure::elaborate(loaned.clone(), limited)
            .unwrap_err()
            .is_resource()
    );
    let result = o::failure::elaborate(loaned, Default::default());
    use o::regions::local::ErrorKind as K;
    match mode {
        0 | 3 => {
            let flow = result.unwrap();
            assert!(flow.work_used() >= flow.region_usage().work);
        }
        1 => assert!(matches!(
            result.unwrap_err().kind,
            o::failure::ErrorKind::Regions(o::regions::local::Error {
                kind: K::CrossArena { .. },
                ..
            })
        )),
        2 => assert!(matches!(
            result.unwrap_err().kind,
            o::failure::ErrorKind::Regions(o::regions::local::Error {
                kind: K::ExpiredArena(_),
                ..
            })
        )),
        _ => assert!(matches!(
            result.unwrap_err().kind,
            o::failure::ErrorKind::Regions(o::regions::local::Error {
                kind: K::CallStore { .. },
                ..
            })
        )),
    }
}
