use lang_runtime::{allocation as a, execution as e};
use std::path::Path;
fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("task-heap.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analyzed.diagnostics.is_empty(),
        "{:?}",
        analyzed.diagnostics
    );
    let lang_semantics::analysis::Program::Tasks(program) = analyzed.result.unwrap() else {
        panic!("task profile")
    };
    program
}
#[test]
fn vector_ownership_survives_suspension() {
    let program = prepare(
        r#"
struct Item {bytes: core::Bytes}
async fn worker()->i64 {
    let mut values:core::Vec<Item> = core::result_unwrap::<core::Vec<Item>,core::AllocError>(core::vec_new::<Item>(core::allocator()));
    let mut bytes:core::Bytes = core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed:core::Unit = core::result_unwrap::<core::Unit,core::AllocError>(core::vec_push::<Item>(values,Item{bytes:bytes}));
    yield;
    let mut value:Item = core::option_unwrap::<Item>(core::vec_pop::<Item>(values));
    return core::bytes_len(value.bytes)+core::vec_len::<Item>(values)+42;
}
async fn main()->i64 {return core::result_unwrap::<i64,core::TaskError>(await worker());}

"#,
    );
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(allocator.stats().live_allocations, 0);
}
#[test]
fn owned_collection_and_shared_pipeline_survives_suspension() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let result = execute(&program, &native, compiled, &mut allocator, control()).unwrap();
        assert_eq!(result.value, 42);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, [40, 41]);
    }
}
const SOURCE: &str = include_str!("fixtures/task_owned_pipeline.lang");
#[allow(
    clippy::result_large_err,
    reason = "Compare full task failure evidence."
)]
fn execute<P: a::Provider>(
    program: &lang_semantics::analysis::TaskProgram,
    native: &lang_codegen_clif::tasks::CompiledProgram<'_>,
    compiled: bool,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
) -> Result<lang_runtime::task::runner::Outcome, lang_runtime::task::runner::Failure> {
    if compiled {
        native.execute(allocator, control, Default::default())
    } else {
        lang_vm::unit::tasks::execute_entry(
            program.verified(),
            allocator,
            control,
            Default::default(),
        )
    }
}
#[derive(Default)]
struct Trace {
    panic_buffers: bool,
    live: std::collections::BTreeSet<usize>,
    buffers: Vec<u8>,
}
struct Region {
    id: usize,
    storage: a::Storage,
}
impl a::Region for Region {
    fn bytes(&self) -> &[u8] {
        self.storage.bytes()
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        self.storage.bytes_mut()
    }
}
struct Provider {
    at: usize,
    fail: usize,
    trace: std::rc::Rc<std::cell::RefCell<Trace>>,
}
impl a::Provider for Provider {
    type Region = Region;
    fn allocate(&mut self, request: a::Request) -> Option<Region> {
        self.at += 1;
        if self.at == self.fail {
            return None;
        }
        let storage = a::Storage::try_new(request).ok()?;
        assert!(self.trace.borrow_mut().live.insert(self.at));
        Some(Region {
            id: self.at,
            storage,
        })
    }
    fn deallocate(&mut self, region: Region, _: a::Request) {
        use a::Region as _;
        let mut trace = self.trace.borrow_mut();
        assert!(trace.live.remove(&region.id), "duplicate callback");
        let bytes = region.storage.bytes();
        if bytes.len() > 24
            && bytes[..8] == 1u64.to_le_bytes()
            && bytes[8..16] == ((bytes.len() - 24) as u64).to_le_bytes()
            && bytes[16..24] == [0; 8]
        {
            trace.buffers.push(bytes[24]);
            assert!(
                !trace.panic_buffers,
                "injected contained leaf cleanup panic"
            );
        }
    }
}
fn allocator(
    fail: usize,
) -> (
    a::Allocator<Provider>,
    std::rc::Rc<std::cell::RefCell<Trace>>,
) {
    let trace = std::rc::Rc::new(std::cell::RefCell::new(Trace::default()));
    (
        a::Allocator::new(
            Provider {
                at: 0,
                fail,
                trace: trace.clone(),
            },
            Default::default(),
        )
        .unwrap(),
        trace,
    )
}
fn control() -> e::Control<'static> {
    e::Control::with_limits(Default::default())
}

#[test]
fn every_heap_fuel_prefix_ends_leases_before_retiring_the_mixed_graph() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let (mut base, _) = allocator(usize::MAX);
    let fuel = native
        .execute(&mut base, control(), Default::default())
        .unwrap()
        .usage
        .fuel;
    for prefix in 0..fuel {
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            let control = e::Control::with_limits(e::Limits {
                fuel: Some(prefix),
                ..Default::default()
            });
            let failure = execute(&program, &native, compiled, &mut allocator, control)
                .err()
                .unwrap();
            assert!(
                matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Control(e::Failure::FuelExhausted)
                ),
                "prefix {prefix}, native {compiled}: {failure:?}"
            );
            assert!(!failure.poisoned);
            assert_eq!(failure.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(
                trace.borrow().live.is_empty(),
                "prefix {prefix}, native {compiled}"
            );
        }
    }
}

#[test]
fn cleanup_panics_continue_through_both_collection_branches_with_a_full_report() {
    // Only contained leaf cleanup deliberately violates the trusted no-unwind contract.
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        trace.borrow_mut().panic_buffers = true;
        let limits = lang_runtime::task::runner::Limits {
            cleanup_failure_records: 1,
            ..Default::default()
        };
        let failure = if compiled {
            native.execute(&mut allocator, control(), limits)
        } else {
            lang_vm::unit::tasks::execute_entry(
                program.verified(),
                &mut allocator,
                control(),
                limits,
            )
        }
        .err()
        .unwrap();
        assert!(failure.poisoned, "{failure:?}");
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, [40, 41]);
        let report = failure.report.unwrap();
        assert_eq!(report.cleanup_failures(), 2);
        assert_eq!(report.truncated(), 1);
        assert_eq!(report.quarantined(), 2);
    }
}

#[test]
fn mutable_vector_loans_refresh_and_map_removal_moves_the_owned_payload() {
    let definitions = SOURCE.split("async fn worker").next().unwrap();
    let source = format!(
        r#"{definitions}
fn update(values:&mut core::Vec<Item>)->core::Unit {{
    let mut n:i64=0;
    while n<3 {{
        match core::vec_get_mut::<Item>(values,0) {{
            core::Result::Ok(view)=>{{(*view)=item(40+n);}},
            core::Result::Err(_)=>{{return core::Unit {{}};}},
        }}
        n=n+1;
    }}
    return core::Unit {{}};
}}
async fn main()->i64 {{
    let mut v:core::Vec<Item>=core::result_unwrap::<core::Vec<Item>,core::AllocError>(core::vec_new::<Item>(core::allocator()));
    let mut p:core::Unit=core::result_unwrap::<core::Unit,core::AllocError>(core::vec_push::<Item>(v,item(39)));
    yield;
    let mut u:core::Unit=update(v);
    match core::vec_get::<Item>(v,99) {{core::Result::Ok(_)=>{{return -1;}},core::Result::Err(_)=>{{}},}}
    let mut moved:Item=core::option_unwrap::<Item>(core::vec_pop::<Item>(v));
    let mut m:core::Map<core::Text,Item>=core::result_unwrap::<core::Map<core::Text,Item>,core::AllocError>(core::map_new::<core::Text,Item>(core::allocator()));
    let mut old:core::Option<Item>=core::result_unwrap::<core::Option<Item>,core::AllocError>(core::map_insert::<core::Text,Item>(m,word(65),moved));
    let mut key:core::Text=word(65);
    let mut removed:Item=core::option_unwrap::<Item>(core::map_remove::<core::Text,Item>(m,key));
    match core::map_get::<core::Text,Item>(m,key) {{core::Option::Some(_)=>{{return -2;}},core::Option::None=>{{}},}}
    let mut absent:core::Option<Item>=core::map_remove::<core::Text,Item>(m,key);
    return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(removed.bytes,0))+core::map_len::<core::Text,Item>(m);
}}
"#
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        assert_eq!(
            execute(&program, &native, compiled, &mut allocator, control())
                .unwrap()
                .value,
            42
        );
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, [39, 40, 41, 42]);
    }
}

#[test]
fn nested_shared_observation_retains_authority_and_replacement_retires_the_old_graph() {
    let definitions = SOURCE.split("async fn worker").next().unwrap();
    let source = format!(
        r#"{definitions}
struct Nested {{owner:shared Item}}
async fn main()->i64 {{
    let mut value:shared Item=core::result_unwrap::<shared Item,core::OwnershipError>(core::shared_new::<Item>(core::allocator(),item(41)));
    let mut nest:shared Nested=core::result_unwrap::<shared Nested,core::OwnershipError>(core::shared_new::<Nested>(core::allocator(),Nested{{owner:value}}));
    yield;
    match core::shared_read::<Nested>(nest) {{
        core::Result::Ok(view)=>{{
            let mut retained:shared Item=core::result_unwrap::<shared Item,core::OwnershipError>(core::shared_clone::<Item>(view.owner));
            let mut observer:weak Item=core::result_unwrap::<weak Item,core::OwnershipError>(core::weak_new::<Item>(view.owner));
            let mut upgraded:shared Item=core::result_unwrap::<shared Item,core::OwnershipError>(core::weak_upgrade::<Item>(observer));
            match core::shared_read::<Item>(upgraded) {{core::Result::Ok(inner)=>{{if core::bytes_len(inner.bytes)<1 {{return -1;}}}},core::Result::Err(_)=>{{return -2;}},}}
        }},
        core::Result::Err(_)=>{{return -3;}},
    }}
    let mut replacement:shared Item=core::result_unwrap::<shared Item,core::OwnershipError>(core::shared_new::<Item>(core::allocator(),item(42)));
    let mut replaced:core::Unit=core::result_unwrap::<core::Unit,core::OwnershipError>(core::shared_replace::<Nested>(nest,Nested{{owner:replacement}}));
    match core::shared_read::<Nested>(nest) {{
        core::Result::Ok(last)=>{{match core::shared_read::<Item>(last.owner) {{core::Result::Ok(final_view)=>{{return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(final_view.bytes,0));}},core::Result::Err(_)=>{{return -4;}},}}}},
        core::Result::Err(_)=>{{return -5;}},
    }}
}}
"#
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        assert_eq!(
            execute(&program, &native, compiled, &mut allocator, control())
                .unwrap()
                .value,
            42
        );
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, [41, 42]);
    }
}

#[test]
fn exclusive_collection_views_publish_buffer_growth_and_copy_field_replacement() {
    let definitions = SOURCE.split("async fn worker").next().unwrap();
    let source = format!(
        r#"{definitions}
struct Cell {{bytes:core::Bytes,number:i64}}
fn grow(values:&mut core::Vec<Cell>)->i64 {{
    match core::vec_get_mut::<Cell>(values,0) {{
        core::Result::Ok(view)=>{{
            let mut pushed:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(view.bytes,42));
            view.number=40;
            return view.number+core::bytes_len(view.bytes);
        }},
        core::Result::Err(_)=>{{return -1;}},
    }}
}}
async fn main()->i64 {{
    let mut values:core::Vec<Cell>=core::result_unwrap::<core::Vec<Cell>,core::AllocError>(core::vec_new::<Cell>(core::allocator()));
    let mut pushed:core::Unit=core::result_unwrap::<core::Unit,core::AllocError>(core::vec_push::<Cell>(values,Cell{{bytes:bytes(7),number:0}}));
    yield;
    return grow(values);
}}
"#
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        assert_eq!(
            execute(&program, &native, compiled, &mut allocator, control())
                .unwrap()
                .value,
            42
        );
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
    }
}

#[test]
fn every_heap_provider_refusal_rolls_back_or_retires_the_mixed_graph() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let (mut base, _) = allocator(usize::MAX);
    native
        .execute(&mut base, control(), Default::default())
        .unwrap();
    let attempts = base.provider().unwrap().at;
    for fail in 1..=attempts {
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(fail);
            let failure = execute(&program, &native, compiled, &mut allocator, control())
                .err()
                .unwrap();
            assert_eq!(
                failure.tasks.live, 0,
                "request {fail}, native {compiled}: {failure:?}"
            );
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(
                trace.borrow().live.is_empty(),
                "request {fail}, native {compiled}"
            );
        }
    }
}

#[test]
fn cancellation_before_and_after_heap_access_retires_the_mixed_graph() {
    let definitions = SOURCE.split("async fn main").next().unwrap();
    for yields in [1, 2] {
        let source = format!(
            "{definitions}async fn main()->i64{{let mut h:core::Task<i64>=spawn worker();{}cancel h;let mut r:core::Result<i64,core::TaskError>=await h;match r{{core::Result::Ok(n)=>{{return n;}},core::Result::Err(core::TaskError::Cancelled)=>{{return 42;}},}}}}",
            "yield;".repeat(yields)
        );
        let program = prepare(&source);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            let result = execute(&program, &native, compiled, &mut allocator, control()).unwrap();
            assert_eq!(result.value, 42);
            assert_eq!(result.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
            assert_eq!(trace.borrow().buffers, [40, 41]);
        }
    }
}
