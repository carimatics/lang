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
