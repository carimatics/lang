use lang_runtime::{allocation as a, execution as e};
use std::path::Path;

fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("task-loans.lang"),
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
fn synchronous_shared_exclusive_and_payload_loans_execute_between_suspensions() {
    let program = prepare(include_str!("fixtures/task_sync_loans.lang"));
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let control = e::Control::with_limits(Default::default());
        let result = if compiled {
            native.execute(&mut allocator, control, Default::default())
        } else {
            lang_vm::unit::tasks::execute_entry(
                program.verified(),
                &mut allocator,
                control,
                Default::default(),
            )
        }
        .unwrap();
        assert_eq!(result.value, 12);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}
const SOURCE: &str = include_str!("fixtures/task_sync_loans.lang");
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
fn cross_activation_replacement_updates_owner_order_and_retires_once() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        assert_eq!(
            execute(&program, &native, compiled, &mut allocator, control())
                .unwrap()
                .value,
            12
        );
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, [1, 7, 2]);
    }
}

#[test]
fn every_recursive_loan_fuel_prefix_ends_loans_before_owner_retirement() {
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
fn every_recursive_loan_provider_refusal_rolls_back_or_retires_owners() {
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
fn cancellation_before_and_after_recursive_calls_sees_no_live_segment_loans() {
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
            assert_eq!(
                trace.borrow().buffers,
                if yields == 1 {
                    vec![1, 2]
                } else {
                    vec![1, 7, 2]
                }
            );
        }
    }
}

#[test]
fn panic_in_recursive_exclusive_call_preserves_origin_and_ends_ancestor_loans() {
    let source = SOURCE.replace(
        "return core::Unit {};",
        "return core::panic::<core::Unit>(91);",
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
            failure.report.as_ref().unwrap().primary().unwrap().span,
            failure.span
        );
        assert!(!failure.poisoned);
        let span = failure.span.unwrap();
        assert_eq!(
            &source[span.start as usize..span.end as usize],
            "core::panic::<core::Unit>(91)"
        );
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, [1, 7, 2]);
    }
}

#[test]
fn scalar_mutation_and_allocator_authority_reborrow_across_sync_calls() {
    let source = r#"
fn make(a:&core::Allocator)->core::Bytes {return core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(a));}
fn set(value:&mut i64)->core::Unit {(*value)=42;return core::Unit {};}
fn read(value:&i64)->i64 {return (*value);}
async fn main()->i64 {
    let mut n:i64=0;
    let mut unit:core::Unit=set(n);
    let mut b:core::Bytes=make(core::allocator());
    yield;
    return read(n)+core::bytes_len(b);
}
"#;
    let program = prepare(source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let result = execute(&program, &native, compiled, &mut allocator, control()).unwrap();
        assert_eq!(result.value, 42);
        assert_eq!(result.tasks.live, 0);
        assert!(trace.borrow().live.is_empty());
    }
}
#[test]
fn borrowed_result_payloads_and_copy_errors_resolve_in_the_owning_activation() {
    let definitions = SOURCE.split("async fn worker").next().unwrap();
    let source = format!(
        "{definitions}{}",
        r#"
fn length(value:&core::Text)->i64 {return core::text_len(value);}
fn inspect_result(value:&core::Result<core::Text,core::Utf8Error>)->i64 {
    match value {
        core::Result::Ok(text)=>{return length(text);},
        core::Result::Err(core::Utf8Error::Invalid(offset))=>{return offset+42;},
    }
}
async fn main()->i64 {
    let mut good:core::Result<core::Text,core::Utf8Error>=core::text_from_utf8(bytes(65));
    let mut bad:core::Result<core::Text,core::Utf8Error>=core::text_from_utf8(bytes(255));
    let mut answer:i64=inspect_result(good)+inspect_result(bad);
    yield;
    return answer;
}
"#
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let result = execute(&program, &native, compiled, &mut allocator, control()).unwrap();
        assert_eq!(result.value, 43);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
    }
}
#[test]
fn repeated_call_and_loop_activations_refresh_loan_generations_before_yield() {
    let definitions = SOURCE.split("async fn worker").next().unwrap();
    let source = format!(
        "{definitions}{}",
        r#"
async fn main()->i64 {
    let mut pair:Pair=Pair {second:bytes(2),first:bytes(1)};
    let mut n:i64=0;
    while n<4 {
        let mut changed:core::Unit=change(pair,n);
        let mut observed:i64=read(pair.first)+read(pair.second);
        if observed<9 {return 0;}
        yield;
        n=n+1;
    }
    return read(pair.first)+read(pair.second);
}
"#
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let result = execute(&program, &native, compiled, &mut allocator, control()).unwrap();
        assert_eq!(result.value, 9);
        assert_eq!(result.tasks.live, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, [1, 7, 7, 7, 7, 2]);
    }
}
#[test]
fn nested_shared_reference_parameters_keep_the_originating_loan_identity() {
    let source = r#"
fn read(value:&i64)->i64 {return (*value);}
fn nested(value:&&i64)->i64 {return read((*value));}
fn forward(value:&i64)->i64 {return nested(value);}
async fn main()->i64 {let mut n:i64=42;let mut answer:i64=forward(n);yield;return answer;}
"#;
    let program = prepare(source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let result = execute(&program, &native, compiled, &mut allocator, control()).unwrap();
        assert_eq!(result.value, 42);
        assert_eq!(result.tasks.live, 0);
        assert!(trace.borrow().live.is_empty());
    }
}
#[test]
fn moving_reference_bindings_through_sync_calls_preserves_the_target() {
    let source = r#"
fn choose(first:&mut i64,second:&mut i64)->core::Unit {
    let mut view:&mut i64=first;
    view=second;
    (*view)=42;
    return core::Unit {};
}
async fn main()->i64 {
    let mut first:i64=0;let mut second:i64=0;
    let mut unit:core::Unit=choose(first,second);
    yield;
    return first+second+second;
}
"#;
    let program = prepare(source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let result = execute(&program, &native, compiled, &mut allocator, control()).unwrap();
        assert_eq!(result.value, 84);
        assert_eq!(result.tasks.live, 0);
        assert!(trace.borrow().live.is_empty());
    }
}
