use lang_mir::unit::{
    self as m,
    ownership::{
        self as o, bodies as b, declarations as d, loan_activity, loans, moves, places as p,
        regions as g,
    },
};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    }
}
fn prepared_input() -> loans::LoanCheckedBodies {
    let nominal = o::Type::Nominal(m::DefinitionId(0));
    let mut types = o::TypeTable {
        sources: vec![m::Source {
            path: "recursive.lang".into(),
            text: "x".into(),
        }],
        ..Default::default()
    };
    for constructor in [
        o::Constructor::Arena(o::Type::I64),
        o::Constructor::Option(o::Type::Constructed(o::ConstructedId(0))),
        o::Constructor::Vec(nominal),
        o::Constructor::Map(o::Type::I64, nominal),
        o::Constructor::Shared(o::Type::I64),
        o::Constructor::Weak(o::Type::I64),
        o::Constructor::Loan(o::LoanKind::Shared, nominal),
    ] {
        types.constructed.push(o::Constructed {
            constructor,
            span: span(),
        });
    }
    types.definitions.push(o::Definition {
        name: "Node".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Record(
            [("children", 2), ("link", 1)]
                .into_iter()
                .map(|(name, id)| o::Field {
                    name: name.into(),
                    span: span(),
                    ty: o::TypeUse {
                        ty: o::Type::Constructed(o::ConstructedId(id)),
                        span: span(),
                    },
                })
                .collect(),
        ),
    });
    let typed = b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types,
                imports: vec![],
                functions: vec![],
            },
            functions: vec![],
        },
        functions: vec![],
    }
    .verify(Default::default())
    .unwrap();
    let moved = moves::analyze(typed, Default::default()).unwrap();
    let active = loan_activity::analyze(moved, Default::default()).unwrap();
    loans::analyze(active, Default::default()).unwrap()
}

#[test]
fn recursive_indirect_shapes_are_finite_graph_edges_with_exact_type_and_span_identity() {
    let input = prepared_input();
    let plan = g::prepare(&input, Default::default()).unwrap();
    assert_eq!(plan.nodes().len(), 10);
    let node = plan.node_for(o::Type::Nominal(m::DefinitionId(0))).unwrap();
    let record = plan.node(node).unwrap();
    assert_eq!(record.span, Some(span()));
    assert!(record.properties.contains_arena);
    let g::Kind::Record(fields) = &record.kind else {
        panic!("record");
    };
    assert_eq!(plan.node(fields[0]).unwrap().kind, g::Kind::Vec(node));
    let option = plan.node(fields[1]).unwrap();
    assert_eq!(option.case(0).unwrap(), []);
    let token = option.case(1).unwrap()[0];
    assert_eq!(
        plan.node(token).unwrap().kind,
        g::Kind::Arena(plan.node_for(o::Type::I64).unwrap())
    );
    assert!(option.case(u32::MAX).is_none());
    assert_eq!(
        plan.node(
            plan.node_for(o::Type::Constructed(o::ConstructedId(3)))
                .unwrap()
        )
        .unwrap()
        .kind,
        g::Kind::Map(plan.node_for(o::Type::I64).unwrap(), node)
    );
    assert_eq!(
        plan.node(
            plan.node_for(o::Type::Constructed(o::ConstructedId(6)))
                .unwrap()
        )
        .unwrap()
        .kind,
        g::Kind::Loan {
            kind: o::LoanKind::Shared,
            target: node
        }
    );
    assert!(plan.functions().is_empty());
}

#[test]
fn empty_function_graphs_still_charge_shapes_and_edges_before_publication() {
    let input = prepared_input();
    let plan = g::prepare(&input, Default::default()).unwrap();
    let used = plan.usage();
    let counts = plan.counts();
    assert_eq!(counts.nodes, plan.nodes().len());
    assert_eq!(counts.paths, 0);
    assert!(used.work > input.work_used());
    assert!(used.allocation_bytes > input.requested_bytes());
    let exact = g::Limits {
        work: used.work,
        allocations: used.allocation_bytes,
        elements: used.elements,
        nodes: counts.nodes,
        paths: 0,
        edges: counts.edges,
    };
    assert!(g::prepare(&input, exact).is_ok());
    assert!(matches!(
        g::prepare(&input, g::Limits { nodes: 0, ..exact }),
        Err(g::Error {
            kind: g::ErrorKind::Nodes { .. },
            ..
        })
    ));
    assert!(matches!(
        g::prepare(
            &input,
            g::Limits {
                edges: counts.edges - 1,
                ..exact
            }
        ),
        Err(g::Error {
            kind: g::ErrorKind::Edges { .. },
            ..
        })
    ));
    assert!(g::prepare(&input, exact).is_ok());
}
