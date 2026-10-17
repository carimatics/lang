use lang_diagnostics::{SourceMap, codes};
use lang_hir::{
    layout::{LayoutAnalysis, LayoutError, LayoutInputErrorKind, LayoutLimits, analyze_layouts},
    resolved as r,
    source::Name,
};

fn record(sources: &mut SourceMap, name: &str, fields: &[(&str, r::Type)]) -> r::Definition {
    let mut text = name.to_owned();
    let mut positions = Vec::new();
    for (field, ty) in fields {
        text.push(' ');
        let start = text.len();
        text.push_str(field);
        let end = text.len();
        text.push_str(": ");
        let type_start = text.len();
        text.push_str(match ty {
            r::Type::I64 => "i64",
            r::Type::Bool => "bool",
            r::Type::Nominal(_) => "Other",
            r::Type::Constructed(_) => "core::Text",
        });
        positions.push((start..end, type_start..text.len(), *ty));
    }
    let source = sources.add(format!("{name}.lang"), &text).unwrap();
    r::Definition {
        module: r::ModuleId(0),
        name: Name {
            text: name.into(),
            span: sources.span(source, 0..name.len()).unwrap(),
        },
        qualified_name: name.into(),
        public: true,
        kind: r::DefinitionKind::Record(
            fields
                .iter()
                .zip(positions)
                .map(|((name, _), (span, ty_span, value_type))| r::Field {
                    name: Name {
                        text: (*name).into(),
                        span: sources.span(source, span).unwrap(),
                    },
                    value_type: r::TypeUse {
                        value_type,
                        span: sources.span(source, ty_span).unwrap(),
                    },
                })
                .collect(),
        ),
        span: sources.span(source, 0..text.len()).unwrap(),
    }
}

fn checked(
    definitions: &[r::Definition],
    sources: &SourceMap,
    limits: LayoutLimits,
) -> Result<LayoutAnalysis, LayoutError> {
    let analysis = analyze_layouts(definitions, sources, limits);
    match &analysis {
        Ok(a) => {
            for d in &a.diagnostics {
                d.validate(sources).unwrap();
            }
        }
        Err(LayoutError::Limit(d)) => {
            assert_eq!(d.code, codes::RESOURCE_LIMIT);
            d.validate(sources).unwrap();
        }
        Err(LayoutError::Input { .. }) => {}
    }
    analysis
}

#[test]
fn empty_records_scalar_fields_and_reused_nominal_fields_have_exact_cells() {
    let mut sources = SourceMap::new();
    let empty = record(&mut sources, "Empty", &[]);
    let pair = record(
        &mut sources,
        "Pair",
        &[("left", r::Type::I64), ("right", r::Type::Bool)],
    );
    let copies = record(
        &mut sources,
        "Copies",
        &[
            ("a", r::Type::Nominal(r::DefinitionId(1))),
            ("b", r::Type::Nominal(r::DefinitionId(1))),
        ],
    );
    let analysis = checked(&[empty, pair, copies], &sources, LayoutLimits::default()).unwrap();
    let layouts = analysis.layouts.unwrap();
    assert_eq!(
        layouts
            .iter()
            .map(|l| (l.cells(), l.depth()))
            .collect::<Vec<_>>(),
        [(1, 1), (3, 2), (7, 3)]
    );
}

#[test]
fn enum_reserves_its_largest_payload_plus_one_tag_cell() {
    let mut sources = SourceMap::new();
    let pair = record(
        &mut sources,
        "Pair",
        &[("left", r::Type::I64), ("right", r::Type::Bool)],
    );
    let mut variant = record(
        &mut sources,
        "E",
        &[
            ("None", r::Type::I64),
            ("Many", r::Type::Nominal(r::DefinitionId(0))),
            ("Small", r::Type::I64),
        ],
    );
    let r::DefinitionKind::Record(fields) = variant.kind else {
        unreachable!()
    };
    variant.kind = r::DefinitionKind::Enum(
        fields
            .into_iter()
            .enumerate()
            .map(|(i, f)| r::Case {
                name: f.name,
                payload: if i == 0 { vec![] } else { vec![f.value_type] },
            })
            .collect(),
    );
    let layouts = checked(&[pair, variant], &sources, LayoutLimits::default())
        .unwrap()
        .layouts
        .unwrap();
    assert_eq!((layouts[1].cells(), layouts[1].depth()), (4, 3));
}

#[test]
fn direct_and_mutual_cycles_report_source_shapes_without_layouts() {
    for targets in [vec![0], vec![1, 0]] {
        let mut sources = SourceMap::new();
        let definitions: Vec<_> = targets
            .iter()
            .enumerate()
            .map(|(i, target)| {
                record(
                    &mut sources,
                    &format!("R{i}"),
                    &[("next", r::Type::Nominal(r::DefinitionId(*target)))],
                )
            })
            .collect();
        let analysis = checked(&definitions, &sources, LayoutLimits::default()).unwrap();
        assert!(analysis.layouts.is_none());
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|d| d.message.contains("cycle"))
        );
        assert!(
            analysis
                .diagnostics
                .iter()
                .all(|d| d.code == codes::INVALID_SHAPE && d.secondary.len() == 1)
        );
    }
}

#[test]
fn cycles_through_enum_payloads_are_also_rejected() {
    let mut sources = SourceMap::new();
    let mut definition = record(
        &mut sources,
        "E",
        &[("Recur", r::Type::Nominal(r::DefinitionId(0)))],
    );
    let r::DefinitionKind::Record(fields) = definition.kind else {
        unreachable!()
    };
    definition.kind = r::DefinitionKind::Enum(
        fields
            .into_iter()
            .map(|f| r::Case {
                name: f.name,
                payload: vec![f.value_type],
            })
            .collect(),
    );
    assert!(
        checked(&[definition], &sources, LayoutLimits::default())
            .unwrap()
            .layouts
            .is_none()
    );
}

#[test]
fn duplicate_members_and_empty_enums_are_not_successful_layouts() {
    let mut sources = SourceMap::new();
    let duplicate = record(
        &mut sources,
        "R",
        &[("same", r::Type::I64), ("same", r::Type::Bool)],
    );
    let analysis = checked(&[duplicate], &sources, LayoutLimits::default()).unwrap();
    assert!(analysis.layouts.is_none());
    assert_eq!(analysis.diagnostics[0].secondary.len(), 1);
    let mut empty = record(&mut sources, "E", &[]);
    empty.kind = r::DefinitionKind::Enum(vec![]);
    assert!(
        checked(&[empty], &sources, LayoutLimits::default())
            .unwrap()
            .layouts
            .is_none()
    );
}

fn chain(sources: &mut SourceMap, count: usize) -> Vec<r::Definition> {
    (0..count)
        .map(|i| {
            if i == 0 {
                record(sources, "R0", &[])
            } else {
                record(
                    sources,
                    &format!("R{i}"),
                    &[("next", r::Type::Nominal(r::DefinitionId(i - 1)))],
                )
            }
        })
        .collect()
}

#[test]
fn default_depth_accepts_64_and_rejects_65_before_any_guest_allocation() {
    let mut sources = SourceMap::new();
    let definitions = chain(&mut sources, 65);
    let layouts = checked(&definitions[..64], &sources, LayoutLimits::default())
        .unwrap()
        .layouts
        .unwrap();
    assert_eq!(layouts[63].depth(), 64);
    assert!(matches!(
        checked(&definitions, &sources, LayoutLimits::default()),
        Err(LayoutError::Limit(_))
    ));
}

#[test]
fn iterative_dependency_walk_accepts_deep_explicit_limits_without_rust_recursion() {
    let mut sources = SourceMap::new();
    let mut definitions = chain(&mut sources, 512);
    // Reverse the declaration order as well as all reference IDs, forcing the
    // first walk to maintain its full dependency stack instead of cache hits.
    definitions.reverse();
    for d in &mut definitions {
        if let r::DefinitionKind::Record(fields) = &mut d.kind {
            for f in fields {
                if let r::Type::Nominal(id) = &mut f.value_type.value_type {
                    id.0 = 511 - id.0;
                }
            }
        }
    }
    let layouts = checked(
        &definitions,
        &sources,
        LayoutLimits {
            depth: 512,
            cells: 512,
            ..LayoutLimits::default()
        },
    )
    .unwrap()
    .layouts
    .unwrap();
    assert_eq!(layouts[0].depth(), 512);
    assert_eq!(layouts[0].cells(), 512);
}

#[test]
fn cell_arithmetic_is_checked_even_under_explicit_maximum_limits() {
    let mut sources = SourceMap::new();
    let mut definitions = vec![record(&mut sources, "R0", &[])];
    for i in 1..usize::BITS as usize + 2 {
        definitions.push(record(
            &mut sources,
            &format!("R{i}"),
            &[
                ("a", r::Type::Nominal(r::DefinitionId(i - 1))),
                ("b", r::Type::Nominal(r::DefinitionId(i - 1))),
            ],
        ));
    }
    let error = checked(
        &definitions,
        &sources,
        LayoutLimits {
            depth: usize::MAX,
            cells: usize::MAX,
            ..LayoutLimits::default()
        },
    )
    .unwrap_err();
    let LayoutError::Limit(diagnostic) = error else {
        panic!("expected checked overflow")
    };
    assert!(diagnostic.message.contains("overflow"));
}

#[test]
fn structural_ids_names_spans_and_source_identity_are_validated() {
    let mut sources = SourceMap::new();
    let bad_id = record(
        &mut sources,
        "R",
        &[("field", r::Type::Nominal(r::DefinitionId(99)))],
    );
    assert!(matches!(
        checked(&[bad_id], &sources, LayoutLimits::default()),
        Err(LayoutError::Input {
            kind: LayoutInputErrorKind::InvalidDefinition(99),
            ..
        })
    ));
    let mut wrong_name = record(&mut sources, "Name", &[]);
    wrong_name.name.text = "Other".into();
    assert!(matches!(
        checked(&[wrong_name], &sources, LayoutLimits::default()),
        Err(LayoutError::Input {
            kind: LayoutInputErrorKind::NameTextMismatch,
            ..
        })
    ));
    let mut outside = record(&mut sources, "Outer", &[("field", r::Type::I64)]);
    outside.span = outside.name.span;
    assert!(matches!(
        checked(&[outside], &sources, LayoutLimits::default()),
        Err(LayoutError::Input {
            kind: LayoutInputErrorKind::SpanOutsideDefinition,
            ..
        })
    ));
    let definition = record(&mut sources, "Foreign", &[]);
    assert!(matches!(
        analyze_layouts(&[definition], &SourceMap::new(), LayoutLimits::default()),
        Err(LayoutError::Input {
            kind: LayoutInputErrorKind::Source(_),
            ..
        })
    ));
}

#[test]
fn work_definitions_fields_and_diagnostics_limits_have_valid_error_spans() {
    let mut sources = SourceMap::new();
    let empty = record(&mut sources, "R", &[]);
    assert!(
        checked(
            std::slice::from_ref(&empty),
            &sources,
            LayoutLimits {
                work: 3,
                ..LayoutLimits::default()
            }
        )
        .unwrap()
        .layouts
        .is_some()
    );
    assert!(matches!(
        checked(
            std::slice::from_ref(&empty),
            &sources,
            LayoutLimits {
                work: 2,
                ..LayoutLimits::default()
            }
        ),
        Err(LayoutError::Limit(_))
    ));
    assert!(matches!(
        checked(
            &[empty],
            &sources,
            LayoutLimits {
                definitions: 0,
                ..LayoutLimits::default()
            }
        ),
        Err(LayoutError::Limit(_))
    ));
    let field = record(&mut sources, "Field", &[("field", r::Type::I64)]);
    assert!(matches!(
        checked(
            &[field],
            &sources,
            LayoutLimits {
                fields: 0,
                ..LayoutLimits::default()
            }
        ),
        Err(LayoutError::Limit(_))
    ));
    let duplicate = record(
        &mut sources,
        "Duplicate",
        &[("x", r::Type::I64), ("x", r::Type::I64)],
    );
    assert!(matches!(
        checked(
            &[duplicate],
            &sources,
            LayoutLimits {
                diagnostics: 0,
                ..LayoutLimits::default()
            }
        ),
        Err(LayoutError::Limit(_))
    ));
}
