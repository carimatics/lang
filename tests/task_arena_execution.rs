use lang_runtime::{allocation as a, execution as e};
use std::path::Path;
fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("task-arena.lang"),
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
const SOURCE: &str = include_str!("fixtures/task_owned_arena.lang");
#[test]
fn independent_mir_rejects_an_unended_arena_entry_loan() {
    use lang_mir::unit::ownership::{self as o, bodies as b};
    let program = prepare("async fn main()->i64{arena graph(core::allocator()){yield;return 42;}}");
    let mut raw = program.verified().typed().clone().into_table();
    let body = raw
        .functions
        .iter_mut()
        .find(|f| {
            f.blocks.iter().any(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.op, b::Op::ArenaEnter { .. }))
            })
        })
        .unwrap();
    let block = body
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.op, b::Op::ArenaEnter { .. }))
        })
        .unwrap();
    let entry = block
        .instructions
        .iter()
        .position(|i| matches!(i.op, b::Op::ArenaEnter { .. }))
        .unwrap();
    let cleanup = block
        .instructions
        .iter_mut()
        .skip(entry + 1)
        .find_map(|i| match &mut i.op {
            b::Op::Cleanup(cleanup) => Some(cleanup),
            _ => None,
        })
        .unwrap();
    let mut delayed = cleanup.clone();
    delayed
        .steps
        .retain(|step| matches!(step.target, b::CleanupTarget::Loan(_)));
    assert!(!delayed.steps.is_empty());
    cleanup
        .steps
        .retain(|step| !matches!(step.target, b::CleanupTarget::Loan(_)));
    let suspension = block
        .instructions
        .iter()
        .position(|i| matches!(i.op, b::Op::TaskYield))
        .unwrap();
    let span = block.instructions[suspension].span;
    block.instructions.insert(
        suspension + 1,
        b::Instruction {
            results: vec![],
            op: b::Op::Cleanup(delayed),
            span,
        },
    );
    let error = o::tasks::verify(raw, Default::default()).unwrap_err();
    assert!(
        matches!(
            error.kind,
            o::tasks::ErrorKind::Loans(o::loans::ErrorKind::Suspension(_))
        ),
        "{error:?}"
    );
}
#[test]
fn owning_arena_cycle_survives_suspension_and_descendants_join_before_retirement() {
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
        assert_eq!(trace.borrow().buffers, [1, 7, 2, 40]);
    }
}

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
    panic_buffers: Vec<u8>,
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
                !trace.panic_buffers.contains(&bytes[24]),
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
fn every_arena_fuel_prefix_ends_reads_and_joins_before_retirement() {
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
fn every_arena_provider_refusal_rolls_back_and_retires_owned_payloads() {
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
fn cancellation_before_and_after_arena_descendant_admission_joins_then_retires() {
    let definitions = SOURCE.split("async fn main").next().unwrap();
    for yields in [1, 2, 3] {
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
            assert_eq!(result.tasks.admitted, if yields == 1 { 2 } else { 3 });
            assert_eq!(
                trace.borrow().buffers,
                if yields <= 2 {
                    vec![1, 2, 40]
                } else {
                    vec![1, 7, 2, 40]
                }
            );
        }
    }
}

#[test]
fn arena_cleanup_continues_after_panics_and_report_truncation() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        trace.borrow_mut().panic_buffers = vec![2, 40];
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
        assert_eq!(trace.borrow().buffers, [1, 7, 2, 40]);
        let report = failure.report.unwrap();
        assert_eq!(report.cleanup_failures(), 2);
        assert_eq!(report.truncated(), 1);
        assert_eq!(report.quarantined(), 2);
    }
}

#[test]
fn arena_publication_rejects_shared_authority_and_retires_the_rejected_input() {
    let program = prepare(
        r#"
struct Foreign {owner:shared i64}
async fn main()->i64{
    let mut value:shared i64=core::result_unwrap::<shared i64,core::OwnershipError>(core::shared_new::<i64>(core::allocator(),42));
    yield;
    arena graph(core::allocator()){
        match core::arena_new::<Foreign>(graph,Foreign{owner:value}){
            core::Result::Ok(_)=>{return -1;},
            core::Result::Err(core::ArenaError::CrossRegion)=>{return 42;},
            core::Result::Err(_)=>{return -2;},
        }
    }
}
"#,
    );
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
fn arena_payload_collection_views_and_mixed_retirement_share_the_existing_drain() {
    let definitions = SOURCE.split("async fn leaf").next().unwrap();
    let source = format!(
        r#"{definitions}
struct Store {{values:core::Vec<core::Bytes>,map:core::Map<i64,core::Bytes>}}
async fn main()->i64{{
    let mut values:core::Vec<core::Bytes>=core::result_unwrap::<core::Vec<core::Bytes>,core::AllocError>(core::vec_new::<core::Bytes>(core::allocator()));
    let mut pushed:core::Unit=core::result_unwrap::<core::Unit,core::AllocError>(core::vec_push::<core::Bytes>(values,bytes(40)));
    let mut map:core::Map<i64,core::Bytes>=core::result_unwrap::<core::Map<i64,core::Bytes>,core::AllocError>(core::map_new::<i64,core::Bytes>(core::allocator()));
    let mut old:core::Option<core::Bytes>=core::result_unwrap::<core::Option<core::Bytes>,core::AllocError>(core::map_insert::<i64,core::Bytes>(map,1,bytes(2)));
    arena graph(core::allocator()){{
        let mut handle:arena Store=core::result_unwrap::<arena Store,core::ArenaError>(core::arena_new::<Store>(graph,Store{{values:values,map:map}}));
        yield;
        match core::arena_read::<Store>(graph,handle){{
            core::Result::Ok(view)=>{{
                let mut answer:i64=0;
                match core::vec_get::<core::Bytes>(view.values,0){{core::Result::Ok(b)=>{{answer=core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(b,0));}},core::Result::Err(_)=>{{return -1;}},}}
                let mut key:i64=1;
                match core::map_get::<i64,core::Bytes>(view.map,key){{core::Option::Some(found)=>{{return answer+core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(found,0));}},core::Option::None=>{{return -2;}},}}
            }},
            core::Result::Err(_)=>{{return -3;}},
        }}
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
        assert_eq!(trace.borrow().buffers, [2, 40]);
    }
}

#[test]
fn arena_scope_reentry_refreshes_reads_and_keeps_each_region_alive_across_yield() {
    let definitions = SOURCE.split("async fn leaf").next().unwrap();
    let source = format!(
        r#"{definitions}
async fn main()->i64{{
    let mut n:i64=0;
    let mut answer:i64=1;
    while n<2{{
        arena graph(core::allocator()){{
            let mut handle:arena core::Bytes=core::result_unwrap::<arena core::Bytes,core::ArenaError>(core::arena_new::<core::Bytes>(graph,bytes(20+n)));
            yield;
            match core::arena_read::<core::Bytes>(graph,handle){{core::Result::Ok(view)=>{{answer=answer+core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(view,0));}},core::Result::Err(_)=>{{return -1;}},}}
        }}
        n=n+1;
        yield;
    }}
    return answer;
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
        assert_eq!(trace.borrow().buffers, [20, 21]);
    }
}

#[test]
fn panic_in_an_arena_read_call_preserves_origin_and_joins_the_owned_child() {
    let definitions = SOURCE.split("async fn worker").next().unwrap().replace(
        "return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(value.bytes,0));",
        "return core::panic::<i64>(91);",
    );
    let source = format!(
        r#"{definitions}
async fn worker()->i64{{
    arena graph(core::allocator()){{
        let mut node:arena Node=core::result_unwrap::<arena Node,core::ArenaError>(core::arena_new::<Node>(graph,Node{{bytes:bytes(40),next:core::Option::None}}));
        task_scope{{
            let mut child:core::Task<i64>=spawn leaf();
            yield;
            match core::arena_read::<Node>(graph,node){{core::Result::Ok(view)=>{{return read(view);}},core::Result::Err(_)=>{{return -1;}},}}
        }}
    }}
}}
async fn main()->i64{{return core::result_unwrap::<i64,core::TaskError>(await worker());}}
"#
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let failure = execute(&program, &native, compiled, &mut allocator, control())
            .err()
            .unwrap();
        assert!(
            matches!(
                failure.reason,
                lang_runtime::task::runner::Reason::Panic(Some(91))
            ),
            "{failure:?}"
        );
        assert_eq!(
            failure.span.unwrap().start as usize,
            source.find("core::panic::<i64>(91)").unwrap()
        );
        assert_eq!(
            program
                .verified()
                .typed()
                .places()
                .declarations()
                .functions()[failure.function.unwrap().0 as usize]
                .name,
            "read"
        );
        assert_eq!(
            failure.report.as_ref().unwrap().primary().unwrap().span,
            failure.span
        );
        assert!(!failure.poisoned);
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, [7, 40]);
    }
}
