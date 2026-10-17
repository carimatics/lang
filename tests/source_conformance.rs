//! Parsed source through shared MIR and the initial native scalar profile.

use std::fs;

use lang_codegen_clif::{InvokeError, unit as native};
use lang_mir::{HostFunctionId, unit as m};
use lang_runtime::HostFunctionRegistry;
#[path = "support/source.rs"]
mod source;
use source::Files;

#[test]
fn parsed_wrapping_and_signed_branch_results_match_vm_and_jit() {
    let u = Files::new(
        "fn sample(a: i64, b: i64) -> i64 { if a < b { return a + b; } else { return a + -1; } }",
    )
    .unit();
    let scalar = native::scalar_function(&u, m::FunctionId(0), &[], m::Limits::default()).unwrap();
    let bytecode = lang_bytecode::lower(&scalar).unwrap();
    let jit = native::compile(
        &u,
        m::FunctionId(0),
        &[],
        &HostFunctionRegistry::new(),
        m::Limits::default(),
    )
    .unwrap();
    for (a, b) in [(i64::MIN, -1), (i64::MAX, i64::MAX), (-4, 9), (9, -4)] {
        let expected = if a < b {
            a.wrapping_add(b)
        } else {
            a.wrapping_add(-1)
        };
        assert_eq!(lang_vm::execute(&bytecode, &[a, b]).unwrap(), expected);
        assert_eq!(jit.invoke_i64_2(a, b).unwrap(), expected);
        let program = lang_bytecode::unit::Program::from_verified(u.clone());
        let result = lang_vm::unit::execute(
            &program,
            m::FunctionId(0),
            &[lang_vm::unit::Value::i64(a), lang_vm::unit::Value::i64(b)],
            &HostFunctionRegistry::new(),
            &lang_vm::unit::HostBindings::new(),
            lang_vm::unit::Control::default(),
        )
        .unwrap();
        assert_eq!(result.value.as_i64(), Some(expected));
    }
    assert!(matches!(
        jit.invoke_i64_0(),
        Err(InvokeError::SignatureMismatch { .. })
    ));
}

#[test]
fn parsed_nested_loop_carried_values_and_early_returns_match_native() {
    let u=Files::new("fn sample(n: i64) -> i64 { let mut sum: i64 = 0; let mut i: i64 = 0; while i < n { if i < 3 { sum = sum + i; } else { sum = sum + 2; } i = i + 1; } if n < 0 { return -1; } else { return sum; } }").unit();
    let scalar = native::scalar_function(&u, m::FunctionId(0), &[], m::Limits::default()).unwrap();
    let bytecode = lang_bytecode::lower(&scalar).unwrap();
    let jit = native::compile(
        &u,
        m::FunctionId(0),
        &[],
        &HostFunctionRegistry::new(),
        m::Limits::default(),
    )
    .unwrap();
    for n in [-4, 0, 1, 2, 3, 4, 10, 100] {
        let expected = if n < 0 {
            -1
        } else {
            (0..n).map(|i| i.min(2)).sum()
        };
        assert_eq!(lang_vm::execute(&bytecode, &[n]).unwrap(), expected);
        assert_eq!(jit.invoke_i64_1(n).unwrap(), expected);
        let program = lang_bytecode::unit::Program::from_verified(u.clone());
        let result = lang_vm::unit::execute(
            &program,
            m::FunctionId(0),
            &[lang_vm::unit::Value::i64(n)],
            &HostFunctionRegistry::new(),
            &lang_vm::unit::HostBindings::new(),
            lang_vm::unit::Control::default(),
        )
        .unwrap();
        assert_eq!(result.value.as_i64(), Some(expected));
    }
}

#[test]
fn parsed_zero_arity_and_unicode_source_spans_survive_bytecode_jit_and_aot() {
    let u = Files::new("// 😀\n  fn sample() -> i64 {\n    /* α */ return 40 + 2;\n}\n").unit();
    let scalar = native::scalar_function(&u, m::FunctionId(0), &[], m::Limits::default()).unwrap();
    let bytecode = lang_bytecode::lower(&scalar).unwrap();
    let source = scalar.source_map().unwrap();
    assert_eq!(bytecode.source_map(), scalar.source_map());
    assert_eq!(source.function_span().start().line(), 2);
    assert_eq!(source.function_span().start().column(), 3);
    let position = source.blocks()[0].instruction_spans()[0].start();
    assert_eq!((position.line(), position.column()), (3, 20));
    let jit = native::compile(
        &u,
        m::FunctionId(0),
        &[],
        &HostFunctionRegistry::new(),
        m::Limits::default(),
    )
    .unwrap();
    assert_eq!(jit.invoke_i64_0().unwrap(), 42);
    assert!(matches!(
        jit.invoke_i64_1(42),
        Err(InvokeError::SignatureMismatch { .. })
    ));
    let object = native::emit_host_object(&u, m::FunctionId(0), &[], m::Limits::default()).unwrap();
    for source in [jit.source_map().unwrap(), object.source_map().unwrap()] {
        assert_eq!(source.source_path(), "source.lang");
        assert!(source.ranges().iter().any(|r| r.span().start() == position));
    }
}

#[test]
fn extended_operations_and_signatures_fail_before_native_publication() {
    for text in [
        "fn sample() -> i64 { if true { return 1; } else { return 2; } }",
        "fn sample() -> i64 { return later(); } fn later() -> i64 { return 1; }",
        "struct R { a: i64 } fn sample() -> i64 { return R { a: 1 }.a; }",
        "fn sample() -> bool { return true; }",
    ] {
        let u = Files::new(text).unit();
        for error in [
            native::compile(
                &u,
                m::FunctionId(0),
                &[],
                &HostFunctionRegistry::new(),
                m::Limits::default(),
            )
            .err()
            .unwrap(),
            native::emit_host_object(&u, m::FunctionId(0), &[], m::Limits::default())
                .err()
                .unwrap(),
        ] {
            assert!(matches!(
                error.kind,
                native::ErrorKind::UnsupportedType(_) | native::ErrorKind::UnsupportedOperation
            ));
        }
    }
}

extern "C" fn identity(value: i64) -> i64 {
    value
}
#[test]
fn all_declared_host_imports_are_preflighted_and_exactly_mapped() {
    let u=Files::new("extern fn unused(x: i64) -> i64; extern fn echo(x: i64) -> i64; fn sample() -> i64 { return echo(42); }").unit();
    let mut hosts = HostFunctionRegistry::new();
    hosts
        .register_i64(HostFunctionId::new(91), identity)
        .unwrap();
    let ids = [HostFunctionId::new(90), HostFunctionId::new(91)];
    assert!(
        matches!(native::compile(&u,m::FunctionId(0),&ids,&hosts,m::Limits::default()).err().unwrap().kind,native::ErrorKind::UnknownHostFunction(id) if id==ids[0])
    );
    hosts.register_i64(ids[0], identity).unwrap();
    let jit = native::compile(&u, m::FunctionId(0), &ids, &hosts, m::Limits::default()).unwrap();
    assert_eq!(jit.invoke_i64_0().unwrap(), 42);
    assert!(matches!(
        native::scalar_function(&u, m::FunctionId(0), &[], m::Limits::default())
            .err()
            .unwrap()
            .kind,
        native::ErrorKind::ImportArity
    ));
}

#[test]
fn native_adapter_budget_and_foreign_function_failures_are_structured() {
    let u = Files::new("fn sample() -> i64 { return 1; }").unit();
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
            native::scalar_function(&u, m::FunctionId(0), &[], limits)
                .err()
                .unwrap()
                .kind,
            native::ErrorKind::Budget(_)
        ));
    }
    assert!(matches!(
        native::scalar_function(&u, m::FunctionId(99), &[], m::Limits::default())
            .err()
            .unwrap()
            .kind,
        native::ErrorKind::InvalidFunction(99)
    ));
}

#[test]
fn verified_unit_owned_identity_retains_native_type_and_zero_index_checks() {
    let resource = lang_mir::ResourceTypeId::new(17);
    let owned = m::Type::Own(resource);
    let unit = m::Unit {
        sources: vec![],
        definitions: vec![],
        imports: vec![],
        entry: None,
        functions: vec![m::Function {
            name: "transfer".into(),
            signature: m::Signature {
                parameters: vec![owned],
                result: owned,
            },
            registers: vec![owned],
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
    let jit = native::compile(
        &unit,
        m::FunctionId(0),
        &[],
        &HostFunctionRegistry::new(),
        m::Limits::default(),
    )
    .unwrap();
    assert_eq!(jit.invoke_own_resource_1(resource, 42).unwrap(), 42);
    assert!(matches!(
        jit.invoke_own_resource_1(resource, 0),
        Err(InvokeError::InvalidOwnedResourceIndex(0))
    ));
    assert!(matches!(
        jit.invoke_i64_1(42),
        Err(InvokeError::SignatureMismatch { .. })
    ));
}

#[test]
fn partial_or_cross_file_native_source_metadata_is_rejected() {
    let verified = Files::new("fn sample() -> i64 { return 1; }").unit();
    let mut partial = verified.clone().into_unit();
    partial.functions[0].span = None;
    let partial = partial.verify(m::Limits::default()).unwrap();
    assert!(matches!(
        native::scalar_function(&partial, m::FunctionId(0), &[], m::Limits::default())
            .err()
            .unwrap()
            .kind,
        native::ErrorKind::SourceMetadata
    ));
    let mut foreign = verified.into_unit();
    foreign.sources.push(m::Source {
        path: "other.lang".into(),
        text: "1".into(),
    });
    foreign.functions[0].blocks[0].instructions[0].span = Some(m::Span {
        source: m::SourceId(1),
        start: 0,
        end: 1,
    });
    let foreign = foreign.verify(m::Limits::default()).unwrap();
    assert!(matches!(
        native::scalar_function(&foreign, m::FunctionId(0), &[], m::Limits::default())
            .err()
            .unwrap()
            .kind,
        native::ErrorKind::SourceMetadata
    ));
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_env = "msvc", target_arch = "x86_64")
))]
#[test]
fn parsed_scalar_objects_link_and_run_with_direct_and_response_requests() {
    use lang::linker::{LinkRequest, SystemLinker};
    use std::process::Command;
    let files = Files::new(
        "fn sample(n: i64) -> i64 { let mut sum: i64 = 0; let mut i: i64 = 0; while i < n { sum = sum + i; i = i + 1; } return sum; } fn constant() -> i64 { return -9223372036854775808 + -1; }",
    );
    let u = files.unit();
    let mut objects = Vec::new();
    for f in 0..2 {
        let object =
            native::emit_host_object(&u, m::FunctionId(f), &[], m::Limits::default()).unwrap();
        let path = files.0.join(format!("source-{f}.o"));
        fs::write(&path, object.bytes()).unwrap();
        objects.push(path);
    }
    let harness = files.0.join("harness.c");
    let harness_object = files.0.join("harness.o");
    fs::write(&harness,"#include <stdint.h>\n#include <stdio.h>\n#include <inttypes.h>\nextern int64_t sample(int64_t);\nextern int64_t constant(void);\nint main(void) { if(sample(-1) != 0 || sample(0) != 0 || sample(10) != 45) return 1; printf(\"%\" PRId64 \"\\n%\" PRId64 \"\\n\", sample(10), constant()); return 0; }\n").unwrap();
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
            .with_object(&objects[0])
            .with_object(&objects[1]);
        if response {
            linker
                .link_with_response_file(&request, files.0.join("request.rsp"))
                .unwrap();
        } else {
            linker.link(&request).unwrap();
        }
        let output = Command::new(executable).output().unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .replace("\r\n", "\n"),
            "45\n9223372036854775807\n"
        );
    }
}
