use super::*;
use lang_runtime::{allocation as a, cleanup as c};
use lang_vm::{
    InterruptionFlag,
    unit::{self as vm, owned as engine},
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy)]
enum Selection {
    Main,
    Function(m::FunctionId),
}
#[derive(Clone, Copy)]
enum Injection {
    None,
    Refuse(usize),
    Interrupt(usize),
}
struct Trace {
    requests: Vec<u64>,
    allocated: Vec<usize>,
    released: Vec<usize>,
    injection: Injection,
    flag: Rc<InterruptionFlag>,
}
struct Provider(Rc<RefCell<Trace>>);
struct Region {
    storage: a::Storage,
    id: usize,
}
impl a::Region for Region {
    fn bytes(&self) -> &[u8] {
        self.storage.bytes()
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        self.storage.bytes_mut()
    }
}
impl a::Provider for Provider {
    type Region = Region;
    fn allocate(&mut self, request: a::Request) -> Option<Region> {
        let mut trace = self.0.borrow_mut();
        trace.requests.push(request.bytes());
        let id = trace.requests.len();
        if matches!(trace.injection,Injection::Refuse(at) if at==id) {
            return None;
        }
        let storage = a::Storage::try_new(request).ok()?;
        trace.allocated.push(id);
        if matches!(trace.injection,Injection::Interrupt(at) if at==id) {
            trace.flag.interrupt();
        }
        Some(Region { storage, id })
    }
    fn deallocate(&mut self, region: Region, _: a::Request) {
        self.0.borrow_mut().released.push(region.id);
    }
}
#[derive(Debug, PartialEq)]
struct Observed {
    value: Result<i64, String>,
    fuel: u64,
    host_calls: u64,
    peak_frames: usize,
    peak_cells: usize,
    function: Option<m::FunctionId>,
    fault: Option<m::Span>,
    panic_code: Option<i64>,
    stack: Vec<(m::FunctionId, Option<m::Span>)>,
    report: Option<String>,
    requests: Vec<u64>,
    released: Vec<usize>,
    bytes: u64,
}
fn released(allocator: &a::Allocator<Provider>, trace: &Trace) {
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
    let mut ids = trace.released.clone();
    ids.sort_unstable();
    assert_eq!(
        ids, trace.allocated,
        "all real regions released exactly once"
    );
}
#[allow(
    clippy::result_large_err,
    reason = "Keep production failure records intact in the differential oracle."
)]
fn execute(
    flow: &o::failure::Elaborated,
    selection: Selection,
    instance: &mut c::Instance,
    allocator: &mut a::Allocator<Provider>,
    control: vm::Control<'_>,
) -> Result<engine::Outcome, engine::Failure> {
    match selection {
        Selection::Main => engine::execute_entry(flow, instance, Some(allocator), control),
        Selection::Function(id) => {
            engine::execute_function(flow, id, instance, Some(allocator), control)
        }
    }
}
fn observe(
    flow: &o::failure::Elaborated,
    selection: Selection,
    injection: Injection,
    fuel: u64,
    retry: Option<i64>,
) -> Observed {
    let flag = Rc::new(InterruptionFlag::new());
    let trace = Rc::new(RefCell::new(Trace {
        requests: vec![],
        allocated: vec![],
        released: vec![],
        injection,
        flag: Rc::clone(&flag),
    }));
    let mut allocator = a::Allocator::new(Provider(Rc::clone(&trace)), Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let result = execute(
        flow,
        selection,
        &mut instance,
        &mut allocator,
        vm::Control::with_limits(vm::Limits {
            fuel: Some(fuel),
            ..Default::default()
        })
        .and_interruption(&flag),
    );
    let (value, usage, function, fault, panic_code, stack, report) = match result {
        Ok(out) => (Ok(out.value), out.usage, None, None, None, vec![], None),
        Err(error) => (
            Err(format!("{:?}", error.reason)),
            error.usage,
            error.function,
            error.fault,
            error.panic_code,
            error
                .stack
                .into_iter()
                .map(|f| (f.function, f.span))
                .collect(),
            error.report.map(|r| format!("{r:?}")),
        ),
    };
    assert!(!instance.is_poisoned());
    released(&allocator, &trace.borrow());
    let observed = Observed {
        value,
        fuel: usage.fuel,
        host_calls: usage.host_calls,
        peak_frames: usage.peak_frames,
        peak_cells: usage.peak_live_cells,
        function,
        fault,
        panic_code,
        stack,
        report,
        requests: trace.borrow().requests.clone(),
        released: trace.borrow().released.clone(),
        bytes: allocator.stats().cumulative_bytes,
    };
    if let Some(expected) = retry {
        trace.borrow_mut().injection = Injection::None;
        let out = execute(
            flow,
            selection,
            &mut instance,
            &mut allocator,
            vm::Control::default(),
        )
        .unwrap();
        assert_eq!(out.value, expected);
        released(&allocator, &trace.borrow());
        assert!(!instance.is_poisoned());
    }
    observed
}
fn programs(source: &str) -> (o::failure::Elaborated, wire::Program) {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("artifact.lang"),
        source.as_bytes(),
        None,
        CompileLimits::default(),
        None,
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let program = analysis.result.unwrap();
    let Program::Ownership(original) = &program else {
        panic!("ownership");
    };
    let direct = original.failure_flow().clone();
    let bytes = lang::artifact::compile(
        program,
        &analysis.sources,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let lang::artifact::Program::Ownership(loaded) =
        lang::artifact::decode(&bytes, Default::default()).unwrap()
    else {
        panic!("ownership");
    };
    (direct, loaded)
}

#[test]
fn loaded_services_match_every_provider_refusal_and_fuel_prefix_and_recover() {
    for (name, source) in [
        ("bytes", BYTES),
        ("text", include_str!("../../examples/scripts/text.lang")),
        ("vec", include_str!("../../examples/scripts/vec_owned.lang")),
        ("map", include_str!("../../examples/scripts/map_owned.lang")),
        (
            "shared",
            include_str!("../../examples/scripts/shared_owned.lang"),
        ),
        (
            "arena",
            include_str!("../../examples/scripts/arena_owned.lang"),
        ),
        (
            "nested collections",
            include_str!("../../examples/scripts/nested_collections.lang"),
        ),
        (
            "nested cycle",
            include_str!("../../examples/scripts/nested_cycle.lang"),
        ),
        (
            "nested arena",
            include_str!("../../examples/scripts/nested_arena.lang"),
        ),
        (
            "nested mutation",
            include_str!("../../examples/scripts/nested_mutation.lang"),
        ),
    ] {
        let (source, loaded) = programs(source);
        let selection = Selection::Function(loaded.entry().unwrap());
        let fuel = vm::Limits::default().fuel.unwrap();
        let baseline = observe(&source, Selection::Main, Injection::None, fuel, None);
        assert_eq!(
            observe(loaded.flow(), selection, Injection::None, fuel, None),
            baseline,
            "{name}"
        );
        let expected = *baseline.value.as_ref().unwrap();
        assert!(!baseline.requests.is_empty());
        for at in 1..=baseline.requests.len() {
            let reference = observe(
                &source,
                Selection::Main,
                Injection::Refuse(at),
                fuel,
                Some(expected),
            );
            let actual = observe(
                loaded.flow(),
                selection,
                Injection::Refuse(at),
                fuel,
                Some(expected),
            );
            assert_eq!(actual, reference, "{name} OOM {at}");
        }
        for prefix in 0..=baseline.fuel {
            let reference = observe(
                &source,
                Selection::Main,
                Injection::None,
                prefix,
                Some(expected),
            );
            let actual = observe(
                loaded.flow(),
                selection,
                Injection::None,
                prefix,
                Some(expected),
            );
            assert_eq!(actual, reference, "{name} fuel {prefix}");
        }
        for at in [1, baseline.requests.len()] {
            let reference = observe(
                &source,
                Selection::Main,
                Injection::Interrupt(at),
                fuel,
                Some(expected),
            );
            let actual = observe(
                loaded.flow(),
                selection,
                Injection::Interrupt(at),
                fuel,
                Some(expected),
            );
            assert_eq!(actual, reference, "{name} interrupt {at}");
            assert!(actual.value.as_ref().unwrap_err().contains("Interrupted"));
        }
    }
}

#[test]
fn explicit_entry_selects_non_main_for_single_body_and_guest_call_execution() {
    for expression in ["7", "leaf()"] {
        let source = format!(
            "fn main()->i64{{return 42;}}fn selected()->i64{{let mut bytes:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));return {expression};}}fn leaf()->i64{{return 7;}}"
        );
        let raw = table(&source);
        let entry = m::FunctionId(
            raw.places
                .declarations
                .functions
                .iter()
                .position(|f| f.name == "selected")
                .unwrap() as u32,
        );
        let bytes = wire::encode(raw, Some(entry), Default::default()).unwrap();
        let loaded = wire::decode(&bytes, Default::default()).unwrap();
        let out = observe(
            loaded.flow(),
            Selection::Function(loaded.entry().unwrap()),
            Injection::None,
            1_000_000,
            Some(7),
        );
        assert_eq!(out.value, Ok(7));
        assert_eq!(
            observe(
                loaded.flow(),
                Selection::Main,
                Injection::None,
                1_000_000,
                Some(42)
            )
            .value,
            Ok(42)
        );
    }
}

#[test]
fn reachable_owned_host_calls_are_refused_before_effects_and_unused_metadata_stays_inert() {
    let source = "extern fn missing(x:i64)->i64;fn main()->i64{let mut bytes:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));return missing(1);}";
    let (direct, loaded) = programs(source);
    let out = observe(
        loaded.flow(),
        Selection::Function(loaded.entry().unwrap()),
        Injection::None,
        1_000_000,
        None,
    );
    assert_eq!(
        out,
        observe(&direct, Selection::Main, Injection::None, 1_000_000, None)
    );
    assert!(out.value.as_ref().unwrap_err().contains("Unsupported"));
    assert_eq!(out.fuel, 0);
    assert!(out.requests.is_empty());
    // RFC 0010 sessions may retain unused declarations. Metadata grants no
    // authority and must not turn an unrelated owning expression into a call.
    let source = format!("extern fn missing(x:i64)->i64;{BYTES}");
    let (direct, loaded) = programs(&source);
    let expected = observe(
        &direct,
        Selection::Main,
        Injection::None,
        1_000_000,
        Some(0),
    );
    assert_eq!(expected.value, Ok(0));
    assert_eq!(
        observe(
            loaded.flow(),
            Selection::Function(loaded.entry().unwrap()),
            Injection::None,
            1_000_000,
            Some(0)
        ),
        expected
    );
}
