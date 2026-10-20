#![no_main]

use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, places as p},
};
use libfuzzer_sys::fuzz_target;

#[path = "ownership_mir_bodies/regions.rs"]
mod region_cases;

// This is a bounded test-input generator, not an ownership wire decoder.
fuzz_target!(|bytes: &[u8]| {
    region_cases::exercise(bytes);
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
        o::Constructor::Builtin(o::Builtin::Allocator),
        o::Constructor::Loan(o::LoanKind::Shared, c(4)),
        o::Constructor::Vec(o::Type::I64),
        o::Constructor::Loan(
            if byte(360) & 1 == 0 {
                o::LoanKind::Shared
            } else {
                o::LoanKind::Exclusive
            },
            c(6),
        ),
        o::Constructor::Builtin(o::Builtin::BoundsError),
        o::Constructor::Result(c(3), c(8)),
        o::Constructor::Loan(o::LoanKind::Exclusive, o::Type::I64),
        o::Constructor::Result(c(10), c(8)),
    ];
    let values = [
        o::Type::Bool,
        o::Type::I64,
        o::Type::I64,
        o::Type::I64,
        o::Type::Nominal(m::DefinitionId(0)),
        o::Type::I64,
        o::Type::I64,
        o::Type::I64,
        c(0),
        c(1),
        c(2),
        c(3),
        c(5),
        c(7),
        c(9),
        c(11),
    ];
    let declaration = d::Function {
        abi: d::FunctionAbi::Guest,
        name: "f".into(),
        public: false,
        span,
        signature_span: span,
        parameters: vec![d::LocalId(1)],
        result: o::TypeUse {
            ty: o::Type::I64,
            span,
        },
        locals: vec![
            d::Local {
                name: "local".into(),
                span,
                kind: d::LocalKind::Mutable,
                ty: o::TypeUse {
                    ty: o::Type::I64,
                    span,
                },
            },
            d::Local {
                name: "container".into(),
                span,
                kind: d::LocalKind::Parameter,
                ty: o::TypeUse { ty: c(7), span },
            },
        ],
    };
    let places = p::PlaceTable {
        declarations: d::Declarations {
            types: o::TypeTable {
                sources: vec![m::Source {
                    path: "body-fuzz.lang".into(),
                    text: "x".repeat(16),
                }],
                definitions: vec![o::Definition {
                    name: "Pair".into(),
                    public: false,
                    span,
                    kind: o::DefinitionKind::Record(
                        ["first", "second"]
                            .into_iter()
                            .map(|name| o::Field {
                                name: name.into(),
                                span,
                                ty: o::TypeUse {
                                    ty: o::Type::I64,
                                    span,
                                },
                            })
                            .collect(),
                    ),
                }],
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
                p::Place {
                    origin: m::Register(13),
                    root: p::Root::Local(d::LocalId(1)),
                    projections: vec![],
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
            instructions.push(b::Instruction {
                results: vec![m::Register(13)],
                op: b::Op::MovePlace(p::PlaceId(2)),
                span,
            });
            if byte(0) & 8 != 0 {
                instructions.push(b::Instruction {
                    results: vec![m::Register(12)],
                    op: b::Op::CoreCall(o::core::Call {
                        operation: o::core::Operation::Allocator,
                        type_arguments: vec![],
                        arguments: vec![],
                        result_loan: Some(b::LoanId(0)),
                    }),
                    span,
                });
                instructions.push(b::Instruction {
                    results: vec![],
                    op: b::Op::EndBorrow {
                        loan: b::LoanId(0),
                        mode: b::EndMode::Required,
                    },
                    span,
                });
            }
        }
        for i in 0..usize::from(byte(block + 1) % 5) {
            let at = block * 40 + i * 6 + 10;
            let a = r(byte(at + 1));
            let z = r(byte(at + 2));
            let loan = b::LoanId(u32::from(byte(at + 3) % 8));
            let place = p::PlaceId(u32::from(byte(at + 3) % 4));
            let destination = b::PreparedPlaceId(u32::from(byte(at + 3) % 8));
            let (op, output) = match byte(at) % 27 {
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
                21 => (b::Op::DiscardPrepared(destination), false),
                22 => (
                    b::Op::CoreCall(o::core::Call {
                        operation: o::core::Operation::ALL
                            [usize::from(byte(at + 3)) % o::core::Operation::ALL.len()],
                        type_arguments: (0..byte(at + 4) % 3)
                            .map(|index| o::TypeUse {
                                ty: if index == 0 {
                                    o::Type::I64
                                } else {
                                    c(u32::from(byte(at + 2) % 12))
                                },
                                span,
                            })
                            .collect(),
                        arguments: [a, z, m::Register(7)][..usize::from(byte(at + 1) % 4)].to_vec(),
                        result_loan: (byte(at + 4) & 4 != 0).then_some(loan),
                    }),
                    true,
                ),
                23 => (
                    b::Op::DropLocalIfInitialized(d::LocalId(u32::from(byte(at + 3) % 3))),
                    false,
                ),
                24 => (b::Op::DropValueIfAvailable(a), false),
                25 => (
                    b::Op::Cleanup(b::Cleanup {
                        steps: (0..byte(at + 4) % 6)
                            .map(|index| {
                                let id = u32::from(byte(at + 5 + usize::from(index)) % 8);
                                b::CleanupStep {
                                    target: match byte(at + usize::from(index)) % 4 {
                                        0 => b::CleanupTarget::Local(d::LocalId(id)),
                                        1 => b::CleanupTarget::Value(m::Register(id)),
                                        2 => b::CleanupTarget::Loan(b::LoanId(id)),
                                        _ => b::CleanupTarget::Arena(d::LocalId(id)),
                                    },
                                    span: if byte(at + 3) & 128 == 0 {
                                        span
                                    } else {
                                        m::Span {
                                            source: m::SourceId(u32::MAX),
                                            ..span
                                        }
                                    },
                                }
                            })
                            .collect(),
                    }),
                    false,
                ),
                _ => (
                    b::Op::Record {
                        definition: m::DefinitionId(u32::from(byte(at + 3) % 3)),
                        fields: vec![a, z],
                        initialization_order: (0..byte(at + 4) % 5)
                            .map(|index| u32::from(byte(at + 5 + usize::from(index)) % 5))
                            .collect(),
                    },
                    true,
                ),
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
        let kind = match byte(block + 350) % 4 {
            0 => b::Term::Return(if byte(block + 351) == 0 {
                m::Register(7)
            } else {
                r(byte(block + 351))
            }),
            1 => b::Term::Jump(dest(byte(block + 352))),
            2 => b::Term::Branch {
                condition: r(byte(block + 353)),
                then_target: dest(byte(block + 354)),
                else_target: dest(byte(block + 355)),
            },
            _ => b::Term::Unreachable,
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
                    use lang_mir::unit::ownership::{failure, regions};
                    if let Ok(plan) = regions::prepare(
                        &checked,
                        regions::Limits {
                            work: 100_000,
                            allocations: 2 * 1024 * 1024,
                            elements: 100_000,
                            nodes: 256,
                            paths: 256,
                            edges: 4096,
                        },
                    ) {
                        let typed = checked.activity().moved().typed();
                        let places = typed.places();
                        assert_eq!(plan.nodes().len(), plan.counts().nodes);
                        assert_eq!(plan.functions().len(), typed.functions().len());
                        assert!(plan.usage().work >= checked.work_used());
                        if let Ok(local) = regions::local::analyze(
                            &plan,
                            regions::local::Limits {
                                work: 100_000,
                                allocations: 2 * 1024 * 1024,
                                elements: 100_000,
                                depth: 16,
                                nodes: 512,
                                state_cells: 4096,
                            },
                        ) {
                            assert_eq!(local.functions().len(), plan.functions().len());
                            assert!(local.usage().work >= plan.usage().work);
                        }
                        if let Ok(calls) = regions::calls::analyze(
                            &plan,
                            regions::local::Limits {
                                work: 100_000,
                                allocations: 2 * 1024 * 1024,
                                elements: 100_000,
                                depth: 16,
                                nodes: 512,
                                state_cells: 4096,
                            },
                        ) {
                            assert_eq!(calls.functions().len(), plan.functions().len());
                            assert!(calls.usage().work >= plan.usage().work);
                            assert!(
                                calls
                                    .functions()
                                    .iter()
                                    .all(|f| f.pending_calls().is_empty())
                            );
                        }
                        for (index, function) in plan.functions().iter().enumerate() {
                            for (position, path) in function.paths().iter().enumerate() {
                                let actual = plan.node(path.result()).unwrap().ty;
                                let expected = places
                                    .facts(
                                        m::FunctionId(index as u32),
                                        o::places::PlaceId(position as u32),
                                    )
                                    .unwrap()
                                    .value_type;
                                assert_eq!(
                                    places.declarations().types().equivalent(actual, expected),
                                    Some(true)
                                );
                            }
                        }
                    }
                    let limits = failure::Limits {
                        work: 100_000,
                        allocations: 2 * 1024 * 1024,
                        elements: 100_000,
                        nodes: 4096,
                        regions: failure::RegionLimits {
                            type_nodes: 256,
                            paths: 256,
                            edges: 4096,
                            analysis_nodes: 512,
                            state_cells: 4096,
                            depth: 16,
                        },
                    };
                    if let Ok(flow) = failure::elaborate(checked, limits) {
                        assert!(flow.region_usage().work >= flow.source().work_used());
                        assert!(flow.work_used() >= flow.region_usage().work);
                        assert_eq!(
                            failure::elaborate(flow.source().clone(), limits).unwrap(),
                            flow
                        );
                        for function in flow.functions() {
                            assert!(
                                function
                                    .nodes()
                                    .iter()
                                    .flat_map(|node| node.kind.targets())
                                    .all(|target| (target.0 as usize) < function.nodes().len())
                            );
                        }
                    }
                }
            }
        }
    }
});
