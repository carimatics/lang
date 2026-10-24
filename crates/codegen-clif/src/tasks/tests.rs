use super::*;
use o::{declarations as d, places as p};
fn proof() -> o::tasks::VerifiedTasks {
    let text = "async main i64 42";
    let span = m::Span {
        source: m::SourceId(0),
        start: 0,
        end: text.len() as u32,
    };
    let ty = o::TypeUse {
        ty: o::Type::I64,
        span,
    };
    let instruction = |results, op| b::Instruction { results, op, span };
    o::tasks::verify(
        b::BodyTable {
            places: p::PlaceTable {
                declarations: d::Declarations {
                    types: o::TypeTable {
                        sources: vec![m::Source {
                            path: "native-packet.lang".into(),
                            text: text.into(),
                        }],
                        definitions: vec![],
                        constructed: vec![],
                    },
                    imports: vec![],
                    functions: vec![d::Function {
                        effect: d::FunctionEffect::Asynchronous {
                            span: m::Span { end: 5, ..span },
                        },
                        abi: d::FunctionAbi::Guest,
                        name: "main".into(),
                        public: false,
                        span,
                        signature_span: span,
                        parameters: vec![],
                        result: ty,
                        locals: vec![],
                    }],
                },
                functions: vec![p::FunctionPlaces {
                    values: vec![ty],
                    places: vec![],
                }],
            },
            functions: vec![b::FunctionBody {
                task_scopes: vec![b::TaskScope { parent: None, span }],
                task_bindings: vec![],
                loans: 0,
                prepared: 0,
                blocks: vec![b::Block {
                    parameters: vec![],
                    span,
                    instructions: vec![
                        instruction(vec![m::Register(0)], b::Op::I64(42)),
                        instruction(vec![], b::Op::TaskSaveReturn(m::Register(0))),
                        instruction(vec![], b::Op::TaskScopeExit(b::TaskScopeId(0))),
                    ],
                    terminator: b::Terminator {
                        kind: b::Term::ReturnSaved,
                        span,
                    },
                }],
            }],
        },
        Default::default(),
    )
    .unwrap()
}
#[test]
fn native_continuation_rejects_out_of_range_pc_before_any_callback() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "C" fn unexpected(_: *mut Packet, _: u64, _: u64, _: u64, _: u64) -> u64 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        0
    }
    let proof = proof();
    let compiled = CompiledProgram::compile(&proof, Default::default()).unwrap();
    let function = compiled.image.functions[0].as_ref().unwrap();
    let mut packet = Packet {
        helper: unexpected,
        data: std::ptr::null_mut(),
        failed: 0,
    };
    // SAFETY: the finalized owner keeps this exact generated signature alive;
    // invalid continuations must return before reading the packet's opaque data.
    let entry: unsafe extern "C" fn(*mut Packet, u32) -> u32 =
        unsafe { std::mem::transmute(function.code) };
    assert_eq!(unsafe { entry(&mut packet, u32::MAX) }, 2);
    assert_eq!(CALLS.load(Ordering::Relaxed), 0);
}
#[test]
fn private_callback_contains_a_panic_and_records_its_source() {
    let proof = proof();
    let layouts = l::Layouts::compile_tasks(&proof, Default::default()).unwrap();
    let plan = l::FramePlan::compile_tasks(
        &layouts,
        &proof,
        m::FunctionId(0),
        Default::default(),
        usize::MAX,
    )
    .unwrap();
    let mut budget = m::Budget::with_usage(Default::default(), plan.usage()).unwrap();
    let plans = [Some(plan)];
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut machine = Machine::new(
        &proof,
        &plans,
        &layouts,
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
        &mut budget,
        m::FunctionId(0),
    )
    .unwrap();
    let span = proof.typed().places().declarations().functions()[0].span;
    // Corrupt trusted metadata only in this boundary test, to exercise unwinding
    // without violating the allocation provider's separate no-unwind contract.
    let nodes = [Node {
        primitive: Primitive::DiscardPrepared(usize::MAX),
        emit: Emit::Storage,
        span,
        raised: None,
        condition: None,
    }];
    {
        let mut invocation = Invocation {
            machine: &mut machine,
            task: 0,
            nodes: &nodes,
            fatal: None,
            flow: None,
        };
        let mut packet = Packet {
            helper: helper::<a::System>,
            data: std::ptr::from_mut(&mut invocation).cast(),
            failed: 0,
        };
        // SAFETY: the packet and generic invocation have exactly the matching
        // live exclusive extent required by the real generated-code callback.
        assert_eq!(
            unsafe { helper::<a::System>(&mut packet, 0, APPLY, 0, 0) },
            2
        );
        assert_eq!(packet.failed, 1);
        assert!(invocation.fatal.is_none());
    }
    let fault = machine.faults[0].as_ref().unwrap();
    assert!(matches!(
        fault.reason,
        r::Reason::Execution(f::ExecutionFailure::HostPanic)
    ));
    assert_eq!(fault.span, span);
    // A test-forged packet is never resumed as guest code. Retire its live frame.
    while let Some(mut frame) = machine.records[0].frames.pop() {
        frame
            .release(machine.allocator, &mut machine.services)
            .unwrap();
    }
    assert_eq!(machine.allocator.stats().live_allocations, 0);
}
