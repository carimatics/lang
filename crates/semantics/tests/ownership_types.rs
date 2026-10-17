use lang_diagnostics::SourceMap;
use lang_hir::{
    loans, moves,
    program::{CheckLimits, analyze_bodies},
    regions::{self, RegionCheckedBodies},
    resolved as r,
};
use lang_mir::unit::{self as m, ownership as o};
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
    ownership_types::{self, Error},
};
use std::path::Path;

fn prepare(text: &str) -> (SourceMap, RegionCheckedBodies) {
    let mut sources = SourceMap::new();
    lang_syntax::add_source(&mut sources, "unused.lang", b"unused", Default::default()).unwrap();
    let loaded = load_virtual(
        &mut sources,
        Path::new("types.lang"),
        text.as_bytes(),
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let bound = bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
    assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
    let typed = analyze_bodies(bound.program.unwrap(), &sources, CheckLimits::default()).unwrap();
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let moved = moves::analyze(typed.bodies.unwrap(), &sources, Default::default()).unwrap();
    assert!(moved.diagnostics.is_empty(), "{:?}", moved.diagnostics);
    let loaned = loans::analyze(moved.bodies.unwrap(), &sources, Default::default()).unwrap();
    assert!(loaned.diagnostics.is_empty(), "{:?}", loaned.diagnostics);
    let checked = regions::analyze(loaned.bodies.unwrap(), &sources, Default::default()).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    (sources, checked.bodies.unwrap())
}
fn compare(checked: &RegionCheckedBodies, v: &o::VerifiedTypes) {
    let b = checked.loaned().moved().bodies();
    let pairs = (0..b.source().definitions.len())
        .map(|i| {
            (
                r::Type::Nominal(r::DefinitionId(i)),
                o::Type::Nominal(m::DefinitionId(i as u32)),
            )
        })
        .chain((0..b.types().len()).map(|i| {
            (
                r::Type::Constructed(r::ConstructedTypeId(i)),
                o::Type::Constructed(o::ConstructedId(i as u32)),
            )
        }));
    for (r, m) in pairs {
        let a = b.properties(r).unwrap();
        let z = v.properties(m).unwrap();
        assert_eq!(
            (
                a.is_copy,
                a.requires_drop,
                a.contains_loan,
                a.contains_arena,
                a.contains_allocator
            ),
            (
                z.is_copy,
                z.requires_drop,
                z.contains_loan,
                z.contains_arena,
                z.contains_allocator
            ),
            "{r:?}"
        );
    }
}
const FAMILY: &str = "struct Node { next: core::Option<arena Node> } enum Choice { Empty, Value(core::Text) } fn f(a: &core::Allocator, t: core::Text, b: core::Bytes, u: core::Unit, o: core::Option<i64>, r: core::Result<i64, core::AllocError>, v: core::Vec<i64>, m: core::Map<core::Text, i64>, s: shared i64, w: weak i64, n: arena Node, x: &mut i64, be: core::BoundsError, bu: core::BufferError, utf: core::Utf8Error, own: core::OwnershipError, ar: core::ArenaError) -> i64 { return 0; }";

#[test]
fn checked_source_family_lowers_with_stable_identities_and_remapped_snapshot_spans() {
    let (sources, checked) = prepare(FAMILY);
    let original = checked.clone();
    let v = ownership_types::lower(&checked, &sources, o::Limits::default()).unwrap();
    compare(&checked, &v);
    assert_eq!(checked, original);
    assert_eq!(v.table().sources.len(), 1);
    assert_eq!(v.table().sources[0].text, FAMILY);
    assert_eq!(v.table().definitions[0].span.source, m::SourceId(0));
    for (from, to) in checked
        .loaned()
        .moved()
        .bodies()
        .types()
        .iter()
        .zip(&v.table().constructed)
    {
        assert_eq!(
            sources.file(from.span.source()).unwrap().name(),
            v.table().sources[0].path
        );
        assert_eq!(
            &FAMILY[to.span.start as usize..to.span.end as usize],
            sources.slice(from.span).unwrap()
        );
    }
    assert_eq!(
        v,
        ownership_types::lower(&checked, &sources, o::Limits::default()).unwrap()
    );
}

#[test]
fn lexical_owners_and_compiler_created_loan_sums_survive_type_lowering() {
    let (sources, checked) = prepare(include_str!("fixtures/arena-body.lang"));
    let v = ownership_types::lower(&checked, &sources, o::Limits::default()).unwrap();
    compare(&checked, &v);
    let mut owner = false;
    let mut read_sum = false;
    for (i, t) in v.table().constructed.iter().enumerate() {
        let p = v
            .properties(o::Type::Constructed(o::ConstructedId(i as u32)))
            .unwrap();
        owner |= matches!(t.constructor, o::Constructor::ArenaOwner(_));
        read_sum |= matches!(t.constructor, o::Constructor::Result(..)) && p.contains_loan;
    }
    assert!(owner && read_sum);
    assert!(v.table().constructed.len() > checked.loaned().moved().bodies().source().types.len());
}

#[test]
fn lowering_and_validation_share_exact_cumulative_resource_limits() {
    let (sources, checked) = prepare(FAMILY);
    let v = ownership_types::lower(&checked, &sources, o::Limits::default()).unwrap();
    for (kind, used) in [
        (m::LimitKind::Work, v.work_used()),
        (m::LimitKind::Allocations, v.requested_bytes()),
        (m::LimitKind::Elements, v.elements_used()),
    ] {
        let mut l = o::Limits::default();
        match kind {
            m::LimitKind::Work => l.unit.work = used,
            m::LimitKind::Allocations => l.unit.allocations = used,
            _ => l.unit.elements = used,
        }
        ownership_types::lower(&checked, &sources, l).unwrap();
        match kind {
            m::LimitKind::Work => l.unit.work -= 1,
            m::LimitKind::Allocations => l.unit.allocations -= 1,
            _ => l.unit.elements -= 1,
        }
        let error = ownership_types::lower(&checked, &sources, l).unwrap_err();
        assert!(
            matches!(error,Error::Mir(m::Error{kind:m::ErrorKind::Limit{kind:k,..},..})|Error::Types(o::Error{kind:o::ErrorKind::Mir(m::ErrorKind::Limit{kind:k,..}),..}) if k==kind)
        );
    }
    assert!(matches!(
        ownership_types::lower(
            &checked,
            &sources,
            o::Limits {
                constructed: 0,
                ..Default::default()
            }
        )
        .unwrap_err(),
        Error::Types(o::Error {
            kind: o::ErrorKind::ConstructedLimit { .. },
            ..
        })
    ));
    assert!(matches!(
        ownership_types::lower(&checked, &SourceMap::new(), o::Limits::default()).unwrap_err(),
        Error::Source(_)
    ));
}

#[test]
fn function_local_and_owner_declarations_preserve_checked_source_identity() {
    use o::declarations::LocalKind;
    for text in [
        FAMILY,
        include_str!("fixtures/arena-body.lang"),
        "fn first(a: &core::Allocator) -> i64 { arena one(a) {} return 1; } fn second(a: &core::Allocator) -> i64 { arena two(a) {} return 2; }",
    ] {
        let (sources, checked) = prepare(text);
        let v =
            ownership_types::lower_declarations(&checked, &sources, o::Limits::default()).unwrap();
        compare(&checked, v.types());
        let b = checked.loaned().moved().bodies();
        assert_eq!(v.functions().len(), b.source().functions.len());
        for (i, (from, to)) in b.source().functions.iter().zip(v.functions()).enumerate() {
            assert_eq!(from.qualified_name, to.name);
            assert_eq!(from.locals.len(), to.locals.len());
            assert_eq!(
                from.parameters.iter().map(|p| p.0).collect::<Vec<_>>(),
                to.parameters
                    .iter()
                    .map(|p| p.0 as usize)
                    .collect::<Vec<_>>()
            );
            for (j, local) in to.locals.iter().enumerate() {
                assert_eq!(from.locals[j].name.text, local.name);
                if local.kind == LocalKind::ArenaOwner {
                    let o::Type::Constructed(id) = local.ty.ty else {
                        panic!("owner type")
                    };
                    assert_eq!(
                        v.types().table().constructed[id.0 as usize].constructor,
                        o::Constructor::ArenaOwner(o::ArenaOwner {
                            function: m::FunctionId(i as u32),
                            local: j as u32
                        })
                    );
                }
            }
        }
        assert_eq!(
            v,
            ownership_types::lower_declarations(&checked, &sources, o::Limits::default()).unwrap()
        );
    }
}

#[test]
fn source_scalar_host_declarations_and_materialized_calls_validate_without_execution() {
    let (sources, checked) = prepare(
        "extern fn host(value: i64) -> i64; fn answer(value: i64) -> i64 { return host(value); }",
    );
    let v = ownership_types::lower_declarations(&checked, &sources, o::Limits::default()).unwrap();
    let span = v.functions()[0].span;
    let mut budget = m::Budget::new(m::Limits::default());
    assert_eq!(
        v.check_call(m::FunctionId(0), &[o::Type::I64], span, &mut budget)
            .unwrap(),
        o::Type::I64
    );
    assert_eq!(
        v.check_import(m::ImportId(0), &[o::Type::I64], span, &mut budget)
            .unwrap(),
        o::Type::I64
    );
}

#[test]
fn source_declaration_lowering_and_independent_verification_share_one_budget() {
    let (sources, checked) = prepare(include_str!("fixtures/arena-body.lang"));
    let v = ownership_types::lower_declarations(&checked, &sources, o::Limits::default()).unwrap();
    for (k, used) in [
        (m::LimitKind::Work, v.work_used()),
        (m::LimitKind::Allocations, v.requested_bytes()),
        (m::LimitKind::Elements, v.elements_used()),
    ] {
        let mut l = o::Limits::default();
        match k {
            m::LimitKind::Work => l.unit.work = used,
            m::LimitKind::Allocations => l.unit.allocations = used,
            _ => l.unit.elements = used,
        }
        ownership_types::lower_declarations(&checked, &sources, l).unwrap();
        match k {
            m::LimitKind::Work => l.unit.work -= 1,
            m::LimitKind::Allocations => l.unit.allocations -= 1,
            _ => l.unit.elements -= 1,
        }
        let e = ownership_types::lower_declarations(&checked, &sources, l).unwrap_err();
        assert!(
            matches!(e,Error::Mir(m::Error{kind:m::ErrorKind::Limit{kind,..},..})|Error::Types(o::Error{kind:o::ErrorKind::Mir(m::ErrorKind::Limit{kind,..}),..}) if kind==k)
        );
    }
}

const PLACE_SOURCE: &str = "struct Inner { value: i64 } struct Outer { inner: Inner, text: core::Text } fn fields(p: &mut Outer) -> i64 { (*p).inner.value = 7; p.inner.value = 8; let mut read: &i64 = &p.inner.value; return (*read); } fn temporary() -> i64 { let mut value: i64 = 7; return (*(&value)); }";

#[test]
fn source_places_normalize_automatic_dereference_fields_and_temporary_loan_roots() {
    use lang_mir::unit::ownership::places as p;
    let (sources, checked) = prepare(PLACE_SOURCE);
    let original = checked.clone();
    let v = ownership_types::places::lower(&checked, &sources, p::Limits::default()).unwrap();
    assert_eq!(checked, original);
    let expected = vec![
        p::ProjectionKind::Dereference,
        p::ProjectionKind::Field(0),
        p::ProjectionKind::Field(0),
    ];
    let mut found = 0;
    for (i, place) in v.functions()[0].places.iter().enumerate() {
        let path: Vec<_> = place.projections.iter().map(|p| p.kind).collect();
        if path == expected {
            found += 1;
            assert_eq!(
                v.facts(m::FunctionId(0), p::PlaceId(i as u32))
                    .unwrap()
                    .value_type,
                o::Type::I64
            );
        }
        assert_eq!(
            &PLACE_SOURCE[place.span.start as usize..place.span.end as usize],
            sources
                .slice(
                    checked.loaned().moved().bodies().source().functions[0].expressions
                        [place.origin.0 as usize]
                        .span
                )
                .unwrap()
        );
    }
    assert!(found >= 2);
    assert!(v.functions()[1].places.iter().any(|place| {
        matches!(place.root, p::Root::Value(_))
            && place
                .projections
                .iter()
                .map(|p| p.kind)
                .eq([p::ProjectionKind::Dereference])
    }));
    assert_eq!(
        v,
        ownership_types::places::lower(&checked, &sources, p::Limits::default()).unwrap()
    );
}

#[test]
fn source_nested_reference_permissions_intersect_in_the_mir_path() {
    use lang_mir::unit::ownership::places as p;
    let text = "fn read(value: &&mut i64) -> i64 { return (*(*value)); } fn write(value: &mut &mut i64) -> i64 { (*(*value)) = 3; return (*(*value)); }";
    let (sources, checked) = prepare(text);
    let v = ownership_types::places::lower(&checked, &sources, p::Limits::default()).unwrap();
    for (f, function) in v.functions().iter().enumerate() {
        let mut found = false;
        for (i, place) in function.places.iter().enumerate() {
            if place.projections.iter().map(|p| p.kind).eq([
                p::ProjectionKind::Dereference,
                p::ProjectionKind::Dereference,
            ]) {
                found = true;
                assert_eq!(
                    v.facts(m::FunctionId(f as u32), p::PlaceId(i as u32))
                        .unwrap()
                        .exclusive_borrow,
                    f == 1
                );
            }
        }
        assert!(found);
    }
}

#[test]
fn source_arena_owners_and_provisional_read_locals_keep_place_identity() {
    use lang_mir::unit::ownership::places as p;
    let (sources, checked) = prepare(include_str!("fixtures/arena-body.lang"));
    let v = ownership_types::places::lower(&checked, &sources, p::Limits::default()).unwrap();
    let mut found = false;
    for (i, place) in v.functions()[0].places.iter().enumerate() {
        if let p::Root::Local(local) = place.root
            && v.declarations().functions()[0].locals[local.0 as usize].kind
                == o::declarations::LocalKind::ArenaOwner
            && place.projections.is_empty()
        {
            found = true;
            v.check_access(
                m::FunctionId(0),
                p::PlaceId(i as u32),
                p::Access::Borrow(o::LoanKind::Exclusive),
                place.span,
                &mut m::Budget::new(m::Limits::default()),
            )
            .unwrap();
            assert!(
                v.check_access(
                    m::FunctionId(0),
                    p::PlaceId(i as u32),
                    p::Access::Move,
                    place.span,
                    &mut m::Budget::new(m::Limits::default())
                )
                .is_err()
            );
        }
    }
    assert!(found);
}

#[test]
fn source_place_construction_and_all_verifiers_share_exact_budgets() {
    use lang_mir::unit::ownership::places as p;
    let (sources, checked) = prepare(PLACE_SOURCE);
    let v = ownership_types::places::lower(&checked, &sources, p::Limits::default()).unwrap();
    for (kind, used) in [
        (m::LimitKind::Work, v.work_used()),
        (m::LimitKind::Allocations, v.requested_bytes()),
        (m::LimitKind::Elements, v.elements_used()),
    ] {
        let mut l = p::Limits::default();
        match kind {
            m::LimitKind::Work => l.ownership.unit.work = used,
            m::LimitKind::Allocations => l.ownership.unit.allocations = used,
            _ => l.ownership.unit.elements = used,
        }
        ownership_types::places::lower(&checked, &sources, l).unwrap();
        match kind {
            m::LimitKind::Work => l.ownership.unit.work -= 1,
            m::LimitKind::Allocations => l.ownership.unit.allocations -= 1,
            _ => l.ownership.unit.elements -= 1,
        }
        assert!(ownership_types::places::lower(&checked, &sources, l).is_err());
    }
    assert!(
        ownership_types::places::lower(
            &checked,
            &sources,
            p::Limits {
                places: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn checked_source_owned_return_can_supply_typed_mir_instructions_and_cfg() {
    use lang_mir::unit::ownership::{bodies as b, places as p};
    let (sources, checked) =
        prepare("fn forward(value: core::Text) -> core::Text { return value; }");
    let places = ownership_types::places::lower(&checked, &sources, p::Limits::default()).unwrap();
    let place = &places.functions()[0].places[0];
    let value = place.origin;
    let span = place.span;
    let function_span = places.declarations().functions()[0].span;
    let out = b::BodyTable {
        places: places.into_table(),
        functions: vec![b::FunctionBody {
            loans: 0,
            prepared: 0,
            blocks: vec![b::Block {
                parameters: vec![],
                instructions: vec![b::Instruction {
                    results: vec![value],
                    op: b::Op::MovePlace(p::PlaceId(0)),
                    span,
                }],
                terminator: b::Terminator {
                    kind: m::Term::Return(value),
                    span: function_span,
                },
                span: function_span,
            }],
        }],
    }
    .verify(b::Limits::default())
    .unwrap();
    assert!(out.facts()[0].definitions()[value.0 as usize].is_some());
    let moved = lang_mir::unit::ownership::moves::analyze(out, Default::default()).unwrap();
    assert!(moved.functions()[0].exits()[0].owners().is_empty());
}

#[test]
fn checked_source_assignment_supplies_destination_before_rhs_in_a_mir_assembly_fixture() {
    use lang_mir::unit::ownership::{bodies as b, places as p};
    let text = "fn update(p: &mut i64) -> i64 { (*p) = 7; return (*p); }";
    let (sources, checked) = prepare(text);
    let places = ownership_types::places::lower(&checked, &sources, p::Limits::default()).unwrap();
    let frame = &places.functions()[0];
    let mut paths = frame
        .places
        .iter()
        .enumerate()
        .filter(|(_, place)| {
            place
                .projections
                .iter()
                .any(|p| p.kind == p::ProjectionKind::Dereference)
        })
        .collect::<Vec<_>>();
    paths.sort_by_key(|(_, p)| p.span.start);
    let (destination, path) = paths[0];
    let (returned, read) = *paths.last().unwrap();
    let rhs = frame
        .values
        .iter()
        .position(|v| &text[v.span.start as usize..v.span.end as usize] == "7")
        .unwrap();
    let rhs_span = frame.values[rhs].span;
    assert!(path.span.start < rhs_span.start && rhs_span.start < read.span.start);
    let span = places.declarations().functions()[0].span;
    let instructions = vec![
        b::Instruction {
            results: vec![],
            op: b::Op::PreparePlace {
                destination: b::PreparedPlaceId(0),
                place: p::PlaceId(destination as u32),
            },
            span: path.span,
        },
        b::Instruction {
            results: vec![m::Register(rhs as u32)],
            op: b::Op::I64(7),
            span: rhs_span,
        },
        b::Instruction {
            results: vec![],
            op: b::Op::ReplacePrepared {
                destination: b::PreparedPlaceId(0),
                value: m::Register(rhs as u32),
            },
            span: path.span,
        },
        b::Instruction {
            results: vec![read.origin],
            op: b::Op::CopyPlace(p::PlaceId(returned as u32)),
            span: read.span,
        },
    ];
    let returned = read.origin;
    let typed = b::BodyTable {
        places: places.into_table(),
        functions: vec![b::FunctionBody {
            loans: 0,
            prepared: 1,
            blocks: vec![b::Block {
                parameters: vec![],
                instructions,
                terminator: b::Terminator {
                    kind: m::Term::Return(returned),
                    span,
                },
                span,
            }],
        }],
    }
    .verify(Default::default())
    .unwrap();
    let moved = lang_mir::unit::ownership::moves::analyze(typed, Default::default()).unwrap();
    assert!(moved.functions()[0].exits()[0].owners().is_empty());
}

#[test]
fn checked_source_lexical_loan_can_supply_explicit_begin_and_end_in_a_mir_assembly_fixture() {
    use lang_mir::unit::ownership::{bodies as b, loan_activity as a, places as p};
    let text = "fn read(value: i64) -> i64 { let mut loan: &i64 = &value; return (*loan); }";
    let (sources, checked) = prepare(text);
    let places = ownership_types::places::lower(&checked, &sources, p::Limits::default()).unwrap();
    let frame = &places.functions()[0];
    let owner = frame
        .places
        .iter()
        .position(|p| {
            p.root == p::Root::Local(lang_mir::unit::ownership::declarations::LocalId(0))
                && p.projections.is_empty()
        })
        .unwrap();
    let (read_id, read) = frame
        .places
        .iter()
        .enumerate()
        .find(|(_, p)| {
            p.projections
                .iter()
                .any(|p| p.kind == p::ProjectionKind::Dereference)
        })
        .unwrap();
    let borrowed = frame
        .values
        .iter()
        .position(|v| &text[v.span.start as usize..v.span.end as usize] == "&value")
        .unwrap();
    let borrow_span = frame.values[borrowed].span;
    let span = places.declarations().functions()[0].span;
    let returned = read.origin;
    let instructions = vec![
        b::Instruction {
            results: vec![m::Register(borrowed as u32)],
            op: b::Op::Borrow {
                loan: b::LoanId(0),
                kind: o::LoanKind::Shared,
                place: p::PlaceId(owner as u32),
            },
            span: borrow_span,
        },
        b::Instruction {
            results: vec![],
            op: b::Op::Initialize {
                local: lang_mir::unit::ownership::declarations::LocalId(1),
                value: m::Register(borrowed as u32),
            },
            span: borrow_span,
        },
        b::Instruction {
            results: vec![returned],
            op: b::Op::CopyPlace(p::PlaceId(read_id as u32)),
            span: read.span,
        },
        b::Instruction {
            results: vec![],
            op: b::Op::EndBorrow {
                loan: b::LoanId(0),
                mode: b::EndMode::Required,
            },
            span: read.span,
        },
    ];
    let typed = b::BodyTable {
        places: places.into_table(),
        functions: vec![b::FunctionBody {
            loans: 1,
            prepared: 0,
            blocks: vec![b::Block {
                parameters: vec![],
                instructions,
                terminator: b::Terminator {
                    kind: m::Term::Return(returned),
                    span,
                },
                span,
            }],
        }],
    }
    .verify(Default::default())
    .unwrap();
    let moved = lang_mir::unit::ownership::moves::analyze(typed, Default::default()).unwrap();
    let activity = a::analyze(moved, Default::default()).unwrap();
    assert_eq!(
        activity.functions()[0].entry(m::BlockId(0), b::LoanId(0)),
        Some(a::Activity::Inactive)
    );
    assert!(
        activity.moved().functions()[0].exits()[0]
            .owners()
            .is_empty()
    );
}
