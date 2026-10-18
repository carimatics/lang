//! RFC 0012 feasibility experiment, not an ownership execution adapter.
//! Machine continuations execute arithmetic and select the next compiled
//! function. The host loop dispatches addresses, never MIR/bytecode operations.
//! Payload bits below are inert test data, not live managed-value authority.

use super::OwnedJitModule;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, types};
use cranelift_codegen::{isa, settings};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::layout::Layouts;
use std::mem::offset_of;

const RETURNED: i32 = 0;
const DISPATCH: i32 = 1;
const FAILED: i32 = 2;

#[repr(C)]
struct Input {
    context: *mut State,
    frame: u64,
    continuation: u32,
    reserved: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct Output {
    frame: u64,
    function: u32,
    continuation: u32,
}

#[derive(Debug, Eq, PartialEq)]
#[repr(C)]
struct State {
    value: i64,
    inert_payload: [u64; 4],
}

fn define<M: Module>(module: &mut M) -> [FuncId; 2] {
    let pointer = module.target_config().pointer_type();
    assert_eq!(pointer, types::I64, "profile prototype is 64-bit only");
    let mut signature = module.make_signature();
    signature.params.extend([AbiParam::new(pointer); 2]);
    signature.returns.push(AbiParam::new(types::I32));
    let ids = [0, 1].map(|index| {
        module
            .declare_function(
                &format!("lang_owned_probe_f{index}"),
                Linkage::Export,
                &signature,
            )
            .unwrap()
    });
    for (function, id) in ids.iter().copied().enumerate() {
        let mut context = module.make_context();
        context.func.signature = signature.clone();
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut b = FunctionBuilder::new(&mut context.func, &mut builder_context);
            let entry = b.create_block();
            let start = b.create_block();
            let resume = b.create_block();
            let invalid = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let input = b.block_params(entry)[0];
            let output = b.block_params(entry)[1];
            let pc = b.ins().load(
                types::I32,
                MemFlagsData::new(),
                input,
                offset_of!(Input, continuation) as i32,
            );
            let initial = b
                .ins()
                .icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::Equal, pc, 0);
            b.ins().brif(initial, start, &[], resume, &[]);
            b.switch_to_block(resume);
            if function == 0 {
                let returning =
                    b.ins()
                        .icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::Equal, pc, 1);
                let finish = b.create_block();
                b.ins().brif(returning, finish, &[], invalid, &[]);
                b.switch_to_block(finish);
                increment(&mut b, input, 1);
                let status = b.ins().iconst(types::I32, i64::from(RETURNED));
                b.ins().return_(&[status]);
            } else {
                b.ins().jump(invalid, &[]);
            }
            b.switch_to_block(start);
            increment(&mut b, input, if function == 0 { 1 } else { 40 });
            let next = if function == 0 {
                Output {
                    frame: 7,
                    function: 1,
                    continuation: 0,
                }
            } else {
                Output {
                    frame: 0,
                    function: 0,
                    continuation: 1,
                }
            };
            for (value, offset, ty) in [
                (next.frame, offset_of!(Output, frame), types::I64),
                (
                    u64::from(next.function),
                    offset_of!(Output, function),
                    types::I32,
                ),
                (
                    u64::from(next.continuation),
                    offset_of!(Output, continuation),
                    types::I32,
                ),
            ] {
                let value = b.ins().iconst(ty, value as i64);
                b.ins()
                    .store(MemFlagsData::new(), value, output, offset as i32);
            }
            let status = b.ins().iconst(types::I32, i64::from(DISPATCH));
            b.ins().return_(&[status]);
            b.switch_to_block(invalid);
            let status = b.ins().iconst(types::I32, i64::from(FAILED));
            b.ins().return_(&[status]);
            b.seal_all_blocks();
            b.finalize(module.target_config());
        }
        module.define_function(id, &mut context).unwrap();
    }
    ids
}

fn increment(b: &mut FunctionBuilder<'_>, input: cranelift_codegen::ir::Value, amount: i64) {
    let context = b.ins().load(
        types::I64,
        MemFlagsData::new(),
        input,
        offset_of!(Input, context) as i32,
    );
    let value = b.ins().load(
        types::I64,
        MemFlagsData::new(),
        context,
        offset_of!(State, value) as i32,
    );
    let next = b.ins().iadd_imm_s(value, amount);
    b.ins().store(
        MemFlagsData::new(),
        next,
        context,
        offset_of!(State, value) as i32,
    );
}

struct Executable {
    _owner: OwnedJitModule,
    functions: [*const u8; 2],
}

impl Executable {
    fn compile() -> Self {
        let mut owner = OwnedJitModule(Some(JITModule::new(
            JITBuilder::new(default_libcall_names()).unwrap(),
        )));
        let module = owner.0.as_mut().unwrap();
        let ids = define(module);
        module.finalize_definitions().unwrap();
        let functions = ids.map(|id| module.get_finalized_function(id));
        Self {
            _owner: owner,
            functions,
        }
    }

    fn call(
        &self,
        function: usize,
        state: &mut State,
        frame: u64,
        continuation: u32,
        output: &mut Output,
    ) -> i32 {
        let input = Input {
            context: state,
            frame,
            continuation,
            reserved: 0,
        };
        // SAFETY: define emits this exact target-native C signature. Both
        // packets have the checked repr(C) layout used by generated offsets.
        // input.context points to a uniquely borrowed live State for the call;
        // only its i64 field is accessed. The code retains no pointers. The
        // executable owner outlives this synchronous call and never escapes.
        unsafe {
            let call: unsafe extern "C" fn(*const Input, *mut Output) -> i32 =
                std::mem::transmute(self.functions[function]);
            call(&input, output)
        }
    }
}

#[test]
fn native_continuations_compute_and_return_without_interpreting_guest_operations() {
    let executable = Executable::compile();
    let mut state = State {
        value: 0,
        inert_payload: [0, 1, u64::MAX, 0x1020_3040_5060_7080],
    };
    let payload = state.inert_payload;
    let mut output = Output {
        frame: u64::MAX,
        function: u32::MAX,
        continuation: u32::MAX,
    };
    for iteration in 1..=1000 {
        let mut function = 0;
        let mut frame = 0;
        let mut continuation = 0;
        let mut dispatches = 0;
        loop {
            match executable.call(function, &mut state, frame, continuation, &mut output) {
                RETURNED => break,
                DISPATCH => {
                    assert_eq!(
                        (output.function, output.frame),
                        if function == 0 { (1, 7) } else { (0, 0) }
                    );
                    function = output.function as usize;
                    frame = output.frame;
                    continuation = output.continuation;
                    dispatches += 1;
                    assert!(dispatches <= 2);
                }
                status => panic!("unexpected native status {status}"),
            }
        }
        assert_eq!(dispatches, 2);
        assert_eq!(state.value, iteration * 42);
        assert_eq!(state.inert_payload, payload);
    }
    // Bad continuations fail before memory effects or packet publication.
    for (function, continuation) in [(0, 2), (0, u32::MAX), (1, 1), (1, u32::MAX)] {
        let before = state.value;
        let sentinel = Output {
            frame: u64::MAX,
            function: 99,
            continuation: 77,
        };
        output = sentinel;
        assert_eq!(
            executable.call(function, &mut state, 0, continuation, &mut output),
            FAILED
        );
        assert_eq!(state.value, before);
        assert_eq!(state.inert_payload, payload);
        assert_eq!(output, sentinel);
    }
}

#[test]
fn ownership_payload_layout_is_distinct_from_the_native_dispatch_packet() {
    let span = m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 0,
    };
    let mut table = o::TypeTable {
        sources: vec![m::Source {
            path: "native-probe.lang".into(),
            text: String::new(),
        }],
        ..Default::default()
    };
    let bytes = o::Type::Constructed(o::ConstructedId(0));
    table.constructed.push(o::Constructed {
        constructor: o::Constructor::Builtin(o::Builtin::Bytes),
        span,
    });
    table.definitions.push(o::Definition {
        name: "Envelope".into(),
        public: false,
        span,
        kind: o::DefinitionKind::Record(vec![o::Field {
            name: "payload".into(),
            span,
            ty: o::TypeUse { ty: bytes, span },
        }]),
    });
    let verified = table.verify(o::Limits::default()).unwrap();
    let layouts = Layouts::compile(&verified, m::Limits::default()).unwrap();
    assert_eq!(layouts.get(bytes).unwrap().size, 32);
    assert!(
        layouts
            .get(o::Type::Nominal(m::DefinitionId(0)))
            .unwrap()
            .metadata_bytes
            > 0
    );
    assert_eq!(std::mem::size_of::<Input>(), 24);
    assert_eq!(std::mem::align_of::<Input>(), 8);
    assert_eq!(std::mem::size_of::<Output>(), 16);
    assert_eq!(std::mem::align_of::<Output>(), 8);
    assert_eq!(offset_of!(Input, context), 0);
    assert_eq!(offset_of!(Input, frame), 8);
    assert_eq!(offset_of!(Input, continuation), 16);
    assert_eq!(offset_of!(Input, reserved), 20);
    assert_eq!(offset_of!(Output, frame), 0);
    assert_eq!(offset_of!(Output, function), 8);
    assert_eq!(offset_of!(Output, continuation), 12);
}

#[test]
fn continuation_objects_emit_for_both_tier_one_targets() {
    use object::{Object, ObjectSymbol};
    for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let isa = isa::lookup(target.parse().unwrap())
            .unwrap()
            .finish(settings::Flags::new(settings::builder()))
            .unwrap();
        let mut module = ObjectModule::new(
            ObjectBuilder::new(isa, "owned-probe", default_libcall_names()).unwrap(),
        );
        define(&mut module);
        let bytes = module.finish().emit().unwrap();
        let object = object::File::parse(&*bytes).unwrap();
        let symbols = object
            .symbols()
            .filter(|s| s.is_definition() && s.kind() == object::SymbolKind::Text)
            .filter_map(|s| s.name().ok().map(str::to_owned))
            .collect::<Vec<_>>();
        for function in 0..2 {
            assert!(
                symbols
                    .iter()
                    .any(|s| s.trim_start_matches('_') == format!("lang_owned_probe_f{function}"))
            );
        }
    }
}
