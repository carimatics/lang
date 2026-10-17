//! Whole-source RFC 0006 VM/JIT/object conformance. Wire round trips precede
//! native compilation too, so unit-table relocation cannot hide mismatches.
use lang_bytecode::unit::{CodecLimits, Program, decode, encode};
use lang_codegen_clif::program::{self as native, CompiledProgram, InvokeError, Value};
use lang_mir::{HostFunctionId, unit as m};
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit as vm;
use std::cell::RefCell;
#[path = "support/source.rs"]
mod source;
use source::Files;

fn program(text: &str) -> Program {
    let p = Program::from_verified(Files::new(text).unit());
    decode(
        &encode(&p, CodecLimits::default()).unwrap(),
        CodecLimits::default(),
    )
    .unwrap()
}
fn function(p: &Program, name: &str) -> m::FunctionId {
    m::FunctionId(
        p.verified()
            .unit()
            .functions
            .iter()
            .position(|f| f.name == name)
            .unwrap() as u32,
    )
}
fn compare(p: &Program, name: &str, cases: &[Vec<Value>]) {
    let compiled = CompiledProgram::compile(
        p.verified(),
        &[],
        &HostFunctionRegistry::new(),
        m::Limits::default(),
    )
    .unwrap();
    for args in cases {
        let vm_args: Vec<_> = args
            .iter()
            .map(|a| {
                vm::Value::from_cells(p, a.value_type, a.cells.iter().map(|c| *c as u64).collect())
                    .unwrap()
            })
            .collect();
        let expected = vm::execute(
            p,
            function(p, name),
            &vm_args,
            &HostFunctionRegistry::new(),
            &vm::HostBindings::new(),
            vm::Control::default(),
        )
        .unwrap()
        .value;
        let actual = compiled.invoke(function(p, name), args).unwrap();
        assert_eq!(actual.value_type, expected.value_type());
        assert_eq!(
            actual.cells,
            expected
                .cells()
                .iter()
                .map(|c| *c as i64)
                .collect::<Vec<_>>(),
            "{name}({args:?})"
        );
    }
}

#[test]
fn forward_mutual_recursive_calls_and_boolean_interfaces_match_vm() {
    let p = program(
        "fn even(n: i64) -> bool { if n < 1 { return true; } else { return odd(n + -1); } } fn odd(n: i64) -> bool { if n < 1 { return false; } else { return even(n + -1); } } fn choose(flag: bool, a: i64, b: i64) -> i64 { if flag { return a + b; } else { return a + -1; } }",
    );
    let cases: Vec<_> = (0..50).map(|n| vec![Value::i64(n)]).collect();
    compare(&p, "even", &cases);
    compare(&p, "odd", &cases);
    compare(
        &p,
        "choose",
        &[
            vec![Value::boolean(true), Value::i64(i64::MAX), Value::i64(1)],
            vec![Value::boolean(false), Value::i64(i64::MIN), Value::i64(9)],
        ],
    );
}

const AGGREGATES: &str = "
struct Empty {}
struct Pair { flag: bool, x: i64 }
struct Boxed { p: Pair, empty: Empty }
enum Choice { None, One(Boxed), Two(i64, bool) }
fn pack(x: i64) -> Choice { return Choice::One(Boxed { empty: Empty {}, p: Pair { x: x, flag: true } }); }
fn identity(c: Choice) -> Choice { return c; }
fn inspect(c: Choice) -> i64 {
    match c {
        Choice::None => { return -1; },
        Choice::One(Boxed { p: Pair { flag: true, x: 7 }, empty: Empty {} }) => { return 70; },
        Choice::One(Boxed { p: Pair { flag: flag, x: x }, empty: Empty {} }) => { if flag { return x; } else { return x + -1; } },
        Choice::Two(y, two_flag) => { if two_flag { return y + 2; } else { return y; } },
    }
}
fn main() -> i64 { return inspect(identity(pack(7))); }
fn looped(n: i64) -> Choice {
    let mut current: Choice = Choice::None;
    let mut i: i64 = 0;
    while i < n { if i < 2 { current = pack(i); } else { current = Choice::Two(i, false); } i = i + 1; }
    return current;
}
";

#[test]
fn nested_aggregate_calls_patterns_padding_and_loop_joins_match_vm() {
    let p = program(AGGREGATES);
    compare(&p, "main", &[vec![]]);
    let cases: Vec<_> = [i64::MIN, -1, 0, 7, i64::MAX]
        .into_iter()
        .map(|n| vec![Value::i64(n)])
        .collect();
    compare(&p, "pack", &cases);
    let cases: Vec<_> = (0..8).map(|n| vec![Value::i64(n)]).collect();
    compare(&p, "looped", &cases);
    let compiled = CompiledProgram::compile(
        p.verified(),
        &[],
        &HostFunctionRegistry::new(),
        m::Limits::default(),
    )
    .unwrap();
    let c = compiled
        .invoke(function(&p, "pack"), &[Value::i64(7)])
        .unwrap();
    compare(&p, "identity", &[vec![c.clone()]]);
    compare(&p, "inspect", &[vec![c]]);
    let enum_type = p.verified().unit().functions[function(&p, "identity").0 as usize]
        .signature
        .result;
    let cells = p.verified().facts().type_cells(enum_type).unwrap();
    let mut cases = Vec::new();
    for (tag, payload) in [(0, vec![]), (2, vec![i64::MIN, 0]), (2, vec![i64::MAX, 1])] {
        let mut value = vec![0; cells];
        value[0] = tag;
        value[1..1 + payload.len()].copy_from_slice(&payload);
        cases.push(vec![Value {
            value_type: enum_type,
            cells: value,
        }]);
    }
    compare(&p, "inspect", &cases);
    let info = &compiled.functions()[function(&p, "inspect").0 as usize];
    assert!(info.code_size > 0);
    assert!(
        info.source_ranges
            .iter()
            .all(|r| r.start < r.end && r.end as usize <= info.code_size)
    );
    assert!(info.source_ranges.iter().any(|r| {
        let span = r.span;
        p.verified().unit().sources[span.source.0 as usize].text
            [span.start as usize..span.end as usize]
            .contains("y + 2")
    }));
}

#[test]
fn module_calls_and_multiple_source_ids_survive_whole_unit_compilation() {
    let files = Files::new("mod model; fn sample() -> bool { return model::make(42).ok; }");
    std::fs::write(
        files.0.join("model.lang"),
        "pub struct Item { ok: bool } pub fn make(x: i64) -> Item { return Item { ok: x < 43 }; }",
    )
    .unwrap();
    let p = Program::from_verified(files.unit());
    compare(&p, "sample", &[vec![]]);
    let artifact = native::emit_host_object(p.verified(), &[], m::Limits::default()).unwrap();
    let mut sources: Vec<_> = artifact
        .functions
        .iter()
        .flat_map(|f| f.source_ranges.iter().map(|r| r.span.source))
        .collect();
    sources.sort_unstable();
    sources.dedup();
    assert_eq!(sources.len(), 2);
}

#[test]
fn native_frame_capacity_is_checked_before_codegen() {
    let small = m::Type::Record(m::DefinitionId(0));
    let wide = m::Type::Record(m::DefinitionId(1));
    let unit = m::Unit {
        sources: vec![],
        imports: vec![],
        entry: None,
        definitions: vec![
            m::Definition {
                name: "Small".into(),
                kind: m::DefinitionKind::Record(vec![m::Field {
                    name: "x".into(),
                    value_type: m::Type::I64,
                }]),
            },
            m::Definition {
                name: "Wide".into(),
                kind: m::DefinitionKind::Record(
                    (0..256)
                        .map(|i| m::Field {
                            name: format!("x{i}"),
                            value_type: small,
                        })
                        .collect(),
                ),
            },
        ],
        functions: vec![m::Function {
            name: "large".into(),
            signature: m::Signature {
                parameters: vec![wide; 256],
                result: wide,
            },
            registers: vec![wide; 256],
            span: None,
            blocks: vec![m::Block {
                parameters: vec![],
                instructions: vec![],
                terminator: m::Terminator {
                    kind: m::Term::Return(m::Register(0)),
                    span: None,
                },
            }],
        }],
    }
    .verify(m::Limits::default())
    .unwrap();
    assert!(matches!(
        CompiledProgram::compile(
            &unit,
            &[],
            &HostFunctionRegistry::new(),
            m::Limits::default()
        ),
        Err(native::Error::NativeCapacity)
    ));
    assert!(matches!(
        native::emit_host_object(&unit, &[], m::Limits::default()),
        Err(native::Error::NativeCapacity)
    ));
}

#[test]
fn self_edge_parameter_swaps_have_parallel_assignment_semantics() {
    use m::{
        Block, BlockId, Function, Instruction, Op, Register as R, Signature, Target, Term,
        Terminator, Type,
    };
    let inst = |op| Instruction { op, span: None };
    let term = |kind| Terminator { kind, span: None };
    let target = |block, args: &[u32]| Target {
        block: BlockId(block),
        arguments: args.iter().map(|r| R(*r)).collect(),
    };
    let unit = m::Unit {
        sources: vec![],
        definitions: vec![],
        imports: vec![],
        entry: None,
        functions: vec![Function {
            name: "swap".into(),
            signature: Signature {
                parameters: vec![Type::I64],
                result: Type::I64,
            },
            registers: vec![
                Type::I64,
                Type::I64,
                Type::I64,
                Type::I64,
                Type::I64,
                Type::I64,
                Type::I64,
                Type::I64,
                Type::I64,
                Type::Bool,
            ],
            span: None,
            blocks: vec![
                Block {
                    parameters: vec![],
                    instructions: vec![
                        inst(Op::I64Constant {
                            destination: R(1),
                            value: 11,
                        }),
                        inst(Op::I64Constant {
                            destination: R(2),
                            value: 22,
                        }),
                        inst(Op::I64Constant {
                            destination: R(3),
                            value: 0,
                        }),
                    ],
                    terminator: term(Term::Jump(target(1, &[1, 2, 3]))),
                },
                Block {
                    parameters: vec![R(4), R(5), R(6)],
                    instructions: vec![
                        inst(Op::I64Constant {
                            destination: R(7),
                            value: 1,
                        }),
                        inst(Op::Add {
                            destination: R(8),
                            left: R(6),
                            right: R(7),
                        }),
                        inst(Op::LessThan {
                            destination: R(9),
                            left: R(6),
                            right: R(0),
                        }),
                    ],
                    terminator: term(Term::Branch {
                        condition: R(9),
                        then_target: target(1, &[5, 4, 8]),
                        else_target: target(2, &[]),
                    }),
                },
                Block {
                    parameters: vec![],
                    instructions: vec![],
                    terminator: term(Term::Return(R(4))),
                },
            ],
        }],
    };
    let p = Program::from_verified(unit.verify(m::Limits::default()).unwrap());
    compare(
        &p,
        "swap",
        &(0..20).map(|n| vec![Value::i64(n)]).collect::<Vec<_>>(),
    );
}

thread_local! { static EVENTS: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) }; }
extern "C" fn trace(value: i64) -> i64 {
    EVENTS.with(|events| events.borrow_mut().push(value));
    value
}
#[test]
fn source_evaluation_order_and_complete_import_preflight_are_preserved() {
    let p = program(
        "extern fn unused(x: i64) -> i64; extern fn trace(x: i64) -> i64; struct Pair { a: i64, b: i64 } fn use_pair(p: Pair) -> i64 { return p.a + p.b; } fn main() -> i64 { return use_pair(Pair { b: trace(2), a: trace(1) }); }",
    );
    let mut hosts = HostFunctionRegistry::new();
    let ids: Vec<_> = p
        .verified()
        .unit()
        .imports
        .iter()
        .enumerate()
        .map(|(i, _)| HostFunctionId::new(i as u32))
        .collect();
    for id in &ids {
        hosts.register_i64(*id, trace).unwrap();
    }
    let mut bindings = vm::HostBindings::new();
    for (import, id) in p.verified().unit().imports.iter().zip(&ids) {
        bindings.bind(import.name.clone(), *id).unwrap();
    }
    EVENTS.with(|e| e.borrow_mut().clear());
    let result = vm::execute_entry(&p, &hosts, &bindings, vm::Control::default()).unwrap();
    assert_eq!(result.value.as_i64(), Some(3));
    assert_eq!(EVENTS.with(|e| e.take()), [2, 1]);
    let compiled =
        CompiledProgram::compile(p.verified(), &ids, &hosts, m::Limits::default()).unwrap();
    assert_eq!(
        compiled.invoke(function(&p, "main"), &[]).unwrap(),
        Value::i64(3)
    );
    assert_eq!(EVENTS.with(|e| e.take()), [2, 1]);
    let empty = HostFunctionRegistry::new();
    assert!(matches!(
        CompiledProgram::compile(p.verified(), &ids, &empty, m::Limits::default()),
        Err(native::Error::UnknownHostFunction(_))
    ));
    assert!(matches!(
        CompiledProgram::compile(p.verified(), &[], &hosts, m::Limits::default()),
        Err(native::Error::ImportArity)
    ));
    assert!(EVENTS.with(|e| e.borrow().is_empty()));
}

#[test]
fn malformed_aggregate_and_boolean_inputs_are_rejected_before_native_calls() {
    let p = program(AGGREGATES);
    let compiled = CompiledProgram::compile(
        p.verified(),
        &[],
        &HostFunctionRegistry::new(),
        m::Limits::default(),
    )
    .unwrap();
    let id = function(&p, "identity");
    let result = compiled
        .invoke(function(&p, "pack"), &[Value::i64(9)])
        .unwrap();
    for cells in [
        vec![],
        vec![-1; result.cells.len()],
        vec![999; result.cells.len()],
    ] {
        assert_eq!(
            compiled.invoke(
                id,
                &[Value {
                    value_type: result.value_type,
                    cells
                }]
            ),
            Err(InvokeError::InvalidArguments)
        );
    }
    let mut wrong = result.clone();
    wrong.cells[3] = 2; // nested bool
    assert_eq!(
        compiled.invoke(id, &[wrong]),
        Err(InvokeError::InvalidArguments)
    );
    let mut padding = result;
    padding.cells.fill(0);
    padding.cells[1] = 1; // inactive None payload
    assert_eq!(
        compiled.invoke(id, &[padding]),
        Err(InvokeError::InvalidArguments)
    );
    assert_eq!(
        compiled.invoke(id, &[Value::i64(9)]),
        Err(InvokeError::InvalidArguments)
    );
    assert_eq!(
        compiled.invoke(m::FunctionId(u32::MAX), &[]),
        Err(InvokeError::InvalidFunction(m::FunctionId(u32::MAX)))
    );
    for limits in [
        m::Limits {
            work: 0,
            ..m::Limits::default()
        },
        m::Limits {
            allocations: 0,
            ..m::Limits::default()
        },
    ] {
        assert!(matches!(
            CompiledProgram::compile(p.verified(), &[], &HostFunctionRegistry::new(), limits),
            Err(native::Error::Verification(_))
        ));
    }
}

#[test]
fn wrong_case_projection_fails_and_propagates_across_guest_calls() {
    let p = program(
        "enum E { A(i64), B(i64) } fn leaf(e: E) -> i64 { match e { E::A(x) => { return x; }, E::B(y) => { return y; }, } } fn main() -> i64 { return leaf(E::A(42)); }",
    );
    let mut u = p.verified().clone().into_unit();
    let leaf = function(&p, "leaf");
    let mut changed = false;
    for b in &mut u.functions[leaf.0 as usize].blocks {
        for i in &mut b.instructions {
            if let m::Op::EnumField { case, .. } = &mut i.op {
                *case = 1 - *case;
                changed = true;
            }
        }
    }
    assert!(changed);
    let verified = u.verify(m::Limits::default()).unwrap();
    let p = Program::from_verified(verified);
    assert!(matches!(
        vm::execute_entry(
            &p,
            &HostFunctionRegistry::new(),
            &vm::HostBindings::new(),
            vm::Control::default()
        )
        .unwrap_err()
        .reason,
        vm::Reason::InvalidVariantProjection { .. }
    ));
    let compiled = CompiledProgram::compile(
        p.verified(),
        &[],
        &HostFunctionRegistry::new(),
        m::Limits::default(),
    )
    .unwrap();
    assert_eq!(
        compiled.invoke(function(&p, "main"), &[]),
        Err(InvokeError::InvalidVariantProjection)
    );
}

#[test]
fn mach_o_and_coff_objects_define_every_function_and_guest_relocation() {
    use object::{Object, ObjectSection, ObjectSymbol};
    let p = program(AGGREGATES);
    for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let object = native::emit_object_for_target(
            p.verified(),
            &[],
            m::Limits::default(),
            target.parse().unwrap(),
        )
        .unwrap();
        let parsed = object::File::parse(object.bytes.as_slice()).unwrap();
        for code in &object.functions {
            assert!(parsed.symbols().any(|symbol| symbol.is_definition()
                && symbol.name().unwrap().trim_start_matches('_') == code.symbol));
            assert!(!code.source_ranges.is_empty());
        }
        assert!(parsed.sections().any(|s| s.relocations().next().is_some()));
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_env = "msvc", target_arch = "x86_64")
))]
#[test]
fn whole_source_object_links_and_runs_with_both_linker_request_forms() {
    use lang::linker::{LinkRequest, SystemLinker};
    use std::{fs, process::Command};
    let files = Files::new(AGGREGATES);
    let p = Program::from_verified(files.unit());
    let artifact = native::emit_host_object(p.verified(), &[], m::Limits::default()).unwrap();
    let object = files.0.join("source.o");
    fs::write(&object, &artifact.bytes).unwrap();
    let symbol = &artifact.functions[function(&p, "main").0 as usize].symbol;
    let harness = files.0.join("harness.c");
    let harness_object = files.0.join("harness.o");
    fs::write(&harness, format!("#include <stdint.h>\nextern int32_t {symbol}(const int64_t*, int64_t*);\nint main(void) {{ int64_t result = 0; return {symbol}(0, &result) != 0 || result != 70; }}\n")).unwrap();
    let compilation = if cfg!(target_env = "msvc") {
        let mut output = std::ffi::OsString::from("/Fo:");
        output.push(&harness_object);
        Command::new("cl.exe")
            .args(["/nologo", "/O2", "/c"])
            .arg(&harness)
            .arg(output)
            .output()
            .unwrap()
    } else {
        Command::new("cc")
            .args(["-O2", "-c"])
            .arg(&harness)
            .arg("-o")
            .arg(&harness_object)
            .output()
            .unwrap()
    };
    assert!(
        compilation.status.success(),
        "{}",
        String::from_utf8_lossy(&compilation.stderr)
    );
    let linker = SystemLinker::discover_host().unwrap();
    for response in [false, true] {
        let executable = files.0.join(if response {
            "response.exe"
        } else {
            "direct.exe"
        });
        let request = LinkRequest::new(&executable)
            .with_object(&harness_object)
            .with_object(&object);
        if response {
            linker
                .link_with_response_file(&request, files.0.join("request.rsp"))
                .unwrap();
        } else {
            linker.link(&request).unwrap();
        }
        let output = Command::new(executable).output().unwrap();
        assert!(output.status.success(), "{output:?}");
    }
}
