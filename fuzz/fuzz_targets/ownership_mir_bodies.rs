#![no_main]

use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, places as p},
};
use libfuzzer_sys::fuzz_target;

// This is a bounded test-input generator, not an ownership wire decoder.
fuzz_target!(|bytes: &[u8]| {
    let byte = |i: usize| bytes.get(i).copied().unwrap_or(0);
    let span = m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 16,
    };
    let r = |i: u8| m::Register(u32::from(i % 16));
    let c = |i| o::Type::Constructed(o::ConstructedId(i));
    let constructors = [
        o::Constructor::Builtin(o::Builtin::AllocError),
        o::Constructor::Option(o::Type::I64),
        o::Constructor::Result(o::Type::I64, c(0)),
        o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
    ];
    let values = [
        o::Type::Bool,
        o::Type::I64,
        o::Type::I64,
        o::Type::I64,
        o::Type::I64,
        o::Type::I64,
        o::Type::I64,
        o::Type::I64,
        c(0),
        c(1),
        c(2),
        c(3),
    ];
    let declaration = d::Function {
        name: "f".into(),
        public: false,
        span,
        signature_span: span,
        parameters: vec![],
        result: o::TypeUse {
            ty: o::Type::I64,
            span,
        },
        locals: vec![d::Local {
            name: "local".into(),
            span,
            kind: d::LocalKind::Mutable,
            ty: o::TypeUse {
                ty: o::Type::I64,
                span,
            },
        }],
    };
    let places = p::PlaceTable {
        declarations: d::Declarations {
            types: o::TypeTable {
                sources: vec![m::Source {
                    path: "body-fuzz.lang".into(),
                    text: "x".repeat(16),
                }],
                definitions: vec![],
                constructed: constructors
                    .into_iter()
                    .map(|constructor| o::Constructed { constructor, span })
                    .collect(),
            },
            imports: vec![],
            functions: vec![declaration],
        },
        functions: vec![p::FunctionPlaces {
            values: values
                .into_iter()
                .map(|ty| o::TypeUse { ty, span })
                .collect(),
            places: vec![
                p::Place {
                    origin: m::Register(1),
                    root: p::Root::Local(d::LocalId(0)),
                    projections: vec![],
                    span,
                },
                p::Place {
                    origin: m::Register(2),
                    root: p::Root::Value(m::Register(11)),
                    projections: vec![p::Projection {
                        kind: p::ProjectionKind::Dereference,
                        span,
                    }],
                    span,
                },
            ],
        }],
    };
    let count = usize::from(byte(0) % 8) + 1;
    let mut function = b::FunctionBody {
        loans: 4,
        prepared: 4,
        blocks: vec![],
    };
    for block in 0..count {
        let mut instructions = vec![];
        if block == 0 {
            instructions.push(b::Instruction {
                results: vec![m::Register(7)],
                op: b::Op::I64(0),
                span,
            });
        }
        for i in 0..usize::from(byte(block + 1) % 5) {
            let at = block * 40 + i * 6 + 10;
            let a = r(byte(at + 1));
            let z = r(byte(at + 2));
            let loan = b::LoanId(u32::from(byte(at + 3) % 8));
            let place = p::PlaceId(u32::from(byte(at + 3) % 4));
            let destination = b::PreparedPlaceId(u32::from(byte(at + 3) % 8));
            let (op, output) = match byte(at) % 22 {
                0 => (b::Op::Bool(byte(at + 1) & 1 != 0), true),
                1 => (b::Op::I64(i64::from(byte(at + 1))), true),
                2 => (b::Op::Add(a, z), true),
                3 => (b::Op::LessThan(a, z), true),
                4 => (b::Op::Copy(a), true),
                5 => (b::Op::Move(a), true),
                6 => (b::Op::CopyPlace(place), true),
                7 => (b::Op::MovePlace(place), true),
                8 => (
                    b::Op::Initialize {
                        local: d::LocalId(u32::from(byte(at + 3) % 3)),
                        value: a,
                    },
                    false,
                ),
                9 => (b::Op::Replace { place, value: a }, false),
                10 => (
                    b::Op::Borrow {
                        loan,
                        kind: o::LoanKind::Shared,
                        place,
                    },
                    true,
                ),
                11 => (
                    b::Op::EndBorrow {
                        loan,
                        mode: if byte(at + 4) & 1 == 0 {
                            b::EndMode::Required
                        } else {
                            b::EndMode::IfActive
                        },
                    },
                    false,
                ),
                12 => (
                    b::Op::Variant {
                        case: u32::from(byte(at + 3) % 8),
                        payload: if byte(at + 4) & 1 == 0 {
                            vec![]
                        } else {
                            vec![a]
                        },
                    },
                    true,
                ),
                13 => (
                    b::Op::IsCase {
                        value: a,
                        case: u32::from(byte(at + 3) % 8),
                    },
                    true,
                ),
                14 => (
                    b::Op::CopyCaseField {
                        value: a,
                        case: u32::from(byte(at + 3) % 8),
                        field: 0,
                        guard: z,
                    },
                    true,
                ),
                15 => (
                    b::Op::BorrowCaseField {
                        loan,
                        value: a,
                        case: u32::from(byte(at + 3) % 8),
                        field: 0,
                        guard: z,
                    },
                    true,
                ),
                16 => (
                    b::Op::UnpackVariant {
                        value: a,
                        case: u32::from(byte(at + 3) % 8),
                        guard: z,
                    },
                    true,
                ),
                17 => (b::Op::DropValue(a), false),
                18 => (
                    b::Op::DropLocal(d::LocalId(u32::from(byte(at + 3) % 3))),
                    false,
                ),
                19 => (b::Op::PreparePlace { destination, place }, false),
                20 => (
                    b::Op::ReplacePrepared {
                        destination,
                        value: a,
                    },
                    false,
                ),
                _ => (b::Op::DiscardPrepared(destination), false),
            };
            instructions.push(b::Instruction {
                results: if output {
                    vec![r(byte(at + 5))]
                } else {
                    vec![]
                },
                op,
                span,
            });
        }
        let dest = |v: u8| m::Target {
            block: m::BlockId(u32::from(v) % count as u32),
            arguments: vec![],
        };
        let kind = match byte(block + 350) % 3 {
            0 => m::Term::Return(if byte(block + 351) == 0 {
                m::Register(7)
            } else {
                r(byte(block + 351))
            }),
            1 => m::Term::Jump(dest(byte(block + 352))),
            _ => m::Term::Branch {
                condition: r(byte(block + 353)),
                then_target: dest(byte(block + 354)),
                else_target: dest(byte(block + 355)),
            },
        };
        function.blocks.push(b::Block {
            parameters: vec![],
            instructions,
            terminator: b::Terminator { kind, span },
            span,
        });
    }
    let mut limits = b::Limits {
        loans: 8,
        ..Default::default()
    };
    limits.places.ownership.unit.work = 100_000;
    limits.places.ownership.unit.allocations = 2 * 1024 * 1024;
    limits.places.ownership.unit.elements = 100_000;
    if let Ok(typed) = (b::BodyTable {
        places,
        functions: vec![function],
    })
    .verify(limits)
    {
        assert_eq!(typed.clone().into_table().verify(limits).unwrap(), typed);
        use lang_mir::unit::ownership::moves as flow;
        let limits = flow::Limits {
            work: 100_000,
            allocations: 2 * 1024 * 1024,
            elements: 100_000,
            state_cells: 4096,
        };
        if let Ok(checked) = flow::analyze(typed, limits) {
            assert_eq!(
                flow::analyze(checked.clone().into_typed(), limits).unwrap(),
                checked
            );
            use lang_mir::unit::ownership::loan_activity as activity;
            let limits = activity::Limits {
                work: 100_000,
                allocations: 2 * 1024 * 1024,
                elements: 100_000,
                state_cells: 4096,
            };
            if let Ok(checked) = activity::analyze(checked, limits) {
                assert_eq!(
                    activity::analyze(checked.clone().into_moved(), limits).unwrap(),
                    checked
                );
                use lang_mir::unit::ownership::loans;
                let limits = loans::Limits {
                    work: 100_000,
                    allocations: 2 * 1024 * 1024,
                    elements: 100_000,
                    nodes: 256,
                    depth: 16,
                    state_words: 4096,
                };
                if let Ok(checked) = loans::analyze(checked, limits) {
                    assert_eq!(
                        loans::analyze(checked.clone().into_activity(), limits).unwrap(),
                        checked
                    );
                }
            }
        }
    }
});
