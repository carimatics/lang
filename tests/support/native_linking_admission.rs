use super::*;
use std::cell::Cell;

struct Sentinel(Rc<Cell<usize>>);
impl Drop for Sentinel {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
const SOURCE: &str = include_str!("../../examples/scripts/shared.lang");

#[test]
fn linked_target_preflight_never_resolves_cross_target_code_and_drops_its_owner() {
    let source = source(SOURCE);
    let flow = source.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let target: lang_codegen_clif::Triple = if cfg!(target_os = "macos") {
        "x86_64-pc-windows-msvc"
    } else {
        "aarch64-apple-darwin"
    }
    .parse()
    .unwrap();
    let artifact =
        n::emit_object_for_target(flow, &layouts, &target, Default::default(), 1000).unwrap();
    let dropped = Rc::new(Cell::new(0));
    // SAFETY: no addresses are supplied. Target rejection precedes resolution.
    let result = unsafe {
        artifact.bind(
            Sentinel(dropped.clone()),
            |_, _| panic!("cross-target resolution"),
            Default::default(),
        )
    };
    assert!(
        matches!(result, Err(n::Error::LinkedTarget { emitted, host }) if emitted == target && host == lang_codegen_clif::Triple::host())
    );
    assert_eq!(dropped.get(), 1);
}

#[test]
fn missing_linked_symbol_is_structured_and_drops_its_owner_before_entry() {
    let source = source(SOURCE);
    let flow = source.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = n::emit_object_for_target(
        flow,
        &layouts,
        &lang_codegen_clif::Triple::host(),
        Default::default(),
        1000,
    )
    .unwrap();
    let expected = artifact
        .function(m::FunctionId(0))
        .unwrap()
        .metadata()
        .symbol
        .clone();
    let dropped = Rc::new(Cell::new(0));
    let calls = Cell::new(0);
    // SAFETY: the resolver supplies no code address and cannot enter guest code.
    let result = unsafe {
        artifact.bind(
            Sentinel(dropped.clone()),
            |_, name| {
                calls.set(calls.get() + 1);
                assert_eq!(name, expected);
                None
            },
            Default::default(),
        )
    };
    assert!(
        matches!(result, Err(n::Error::MissingLinkedSymbol { function: m::FunctionId(0), symbol }) if symbol == expected)
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(dropped.get(), 1);
}

#[test]
fn binding_reuses_prior_proof_usage_and_refuses_before_resolver_effects() {
    let source = source(SOURCE);
    let flow = source.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    for resource in [
        m::LimitKind::Work,
        m::LimitKind::Elements,
        m::LimitKind::Allocations,
    ] {
        let artifact = n::emit_object_for_target(
            flow,
            &layouts,
            &lang_codegen_clif::Triple::host(),
            Default::default(),
            1000,
        )
        .unwrap();
        let usage = artifact.usage();
        let mut limits = m::Limits::default();
        match resource {
            m::LimitKind::Work => limits.work = usage.work,
            m::LimitKind::Elements => limits.elements = usage.elements,
            m::LimitKind::Allocations => limits.allocations = usage.allocation_bytes,
            _ => unreachable!(),
        }
        let dropped = Rc::new(Cell::new(0));
        // SAFETY: resource preflight rejects before this resolver can run;
        // it supplies no executable address in any case.
        let result = unsafe {
            artifact.bind(
                Sentinel(dropped.clone()),
                |_, _| panic!("resolver after refused budget"),
                limits,
            )
        };
        assert!(
            matches!(result, Err(n::Error::Resource(error)) if matches!(error.kind, m::ErrorKind::Limit { kind, limit, requested } if kind == resource && requested > limit))
        );
        assert_eq!(dropped.get(), 1);
    }
}
