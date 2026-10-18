use super::*;
use lang::{frontend, frontend::CompileLimits};
use lang_diagnostics::SourceMap;
use lang_semantics::analysis::Program;
use std::path::Path;

const VECTOR: &str = "let mut values:core::Vec<i64>=core::result_unwrap::<core::Vec<i64>,core::AllocError>(core::vec_new::<i64>(core::allocator()));let mut added:core::Unit=core::result_unwrap::<core::Unit,core::AllocError>(core::vec_push::<i64>(values,7));";
const PUSH: &str = "let mut changed:core::Unit=core::result_unwrap::<core::Unit,core::AllocError>(core::vec_push::<i64>(values,9));";
const MAP: &str = "let mut values:core::Map<i64,i64>=core::result_unwrap::<core::Map<i64,i64>,core::AllocError>(core::map_new::<i64,i64>(core::allocator()));let mut added:core::Option<i64>=core::result_unwrap::<core::Option<i64>,core::AllocError>(core::map_insert::<i64,i64>(values,0,7));let mut key:i64=0;";
const INSERT: &str = "let mut changed:core::Option<i64>=core::result_unwrap::<core::Option<i64>,core::AllocError>(core::map_insert::<i64,i64>(values,1,9));";
const SHARED: &str = "let mut owner:shared i64=core::result_unwrap::<shared i64,core::OwnershipError>(core::shared_new::<i64>(core::allocator(),7));";
const REPLACE_SHARED: &str = "let mut changed:core::Unit=core::result_unwrap::<core::Unit,core::OwnershipError>(core::shared_replace::<i64>(owner,9));";
const ARENA: &str = "let mut token:arena i64=core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(graph,7));";
const REPLACE_ARENA: &str = "let mut changed:core::Unit=core::result_unwrap::<core::Unit,core::ArenaError>(core::arena_replace::<i64>(graph,token,9));";

fn cases() -> Vec<(&'static str, String, i64)> {
    vec![
        (
            "vec read success",
            format!(
                "fn probe()->i64{{{VECTOR}let mut n:i64=0;match core::vec_get::<i64>(values,0){{core::Result::Ok(value)=>{{n=(*value);}},core::Result::Err(_)=>{{return -1;}},}}{PUSH}return n+core::vec_len::<i64>(values);}}"
            ),
            9,
        ),
        (
            "vec unused success",
            format!(
                "fn probe()->i64{{{VECTOR}match core::vec_get::<i64>(values,0){{core::Result::Ok(_)=>{{{PUSH}return core::vec_len::<i64>(values);}},core::Result::Err(_)=>{{return -1;}},}}}}"
            ),
            2,
        ),
        (
            "vec failed read",
            format!(
                "fn probe()->i64{{{VECTOR}match core::vec_get::<i64>(values,2){{core::Result::Ok(_)=>{{return -1;}},core::Result::Err(error)=>{{{PUSH}match error{{core::BoundsError::OutOfBounds=>{{return core::vec_len::<i64>(values);}},}}}},}}}}"
            ),
            2,
        ),
        (
            "vec mutable success",
            format!(
                "fn probe()->i64{{{VECTOR}match core::vec_get_mut::<i64>(values,0){{core::Result::Ok(value)=>{{(*value)=11;}},core::Result::Err(_)=>{{return -1;}},}}{PUSH}match core::vec_get::<i64>(values,0){{core::Result::Ok(saved)=>{{return (*saved)+core::vec_len::<i64>(values);}},core::Result::Err(_)=>{{return -2;}},}}}}"
            ),
            13,
        ),
        (
            "vec mutable unused",
            format!(
                "fn probe()->i64{{{VECTOR}match core::vec_get_mut::<i64>(values,0){{core::Result::Ok(_)=>{{{PUSH}return core::vec_len::<i64>(values);}},core::Result::Err(_)=>{{return -1;}},}}}}"
            ),
            2,
        ),
        (
            "vec mutable failure",
            format!(
                "fn probe()->i64{{{VECTOR}match core::vec_get_mut::<i64>(values,2){{core::Result::Ok(_)=>{{return -1;}},core::Result::Err(_)=>{{{PUSH}return core::vec_len::<i64>(values);}},}}}}"
            ),
            2,
        ),
        (
            "map success",
            format!(
                "fn probe()->i64{{{MAP}let mut n:i64=0;match core::map_get::<i64,i64>(values,key){{core::Option::Some(value)=>{{n=(*value);}},core::Option::None=>{{return -1;}},}}{INSERT}return n+core::map_len::<i64,i64>(values);}}"
            ),
            9,
        ),
        (
            "map unused success",
            format!(
                "fn probe()->i64{{{MAP}match core::map_get::<i64,i64>(values,key){{core::Option::Some(_)=>{{{INSERT}return core::map_len::<i64,i64>(values);}},core::Option::None=>{{return -1;}},}}}}"
            ),
            2,
        ),
        (
            "map absent read",
            format!(
                "fn probe()->i64{{{MAP}let mut missing:i64=2;match core::map_get::<i64,i64>(values,missing){{core::Option::Some(_)=>{{return -1;}},core::Option::None=>{{{INSERT}return core::map_len::<i64,i64>(values);}},}}}}"
            ),
            2,
        ),
        (
            "shared success",
            format!(
                "fn probe()->i64{{{SHARED}let mut n:i64=0;match core::shared_read::<i64>(owner){{core::Result::Ok(value)=>{{n=(*value);}},core::Result::Err(_)=>{{return -1;}},}}{REPLACE_SHARED}match core::shared_read::<i64>(owner){{core::Result::Ok(saved)=>{{return n+(*saved);}},core::Result::Err(_)=>{{return -2;}},}}}}"
            ),
            16,
        ),
        (
            "shared unused success",
            format!(
                "fn probe()->i64{{{SHARED}match core::shared_read::<i64>(owner){{core::Result::Ok(_)=>{{{REPLACE_SHARED}return 9;}},core::Result::Err(_)=>{{return -1;}},}}}}"
            ),
            9,
        ),
        (
            "arena success",
            format!(
                "fn probe()->i64{{arena graph(core::allocator()){{{ARENA}let mut n:i64=0;match core::arena_read::<i64>(graph,token){{core::Result::Ok(value)=>{{n=(*value);}},core::Result::Err(_)=>{{return -1;}},}}{REPLACE_ARENA}match core::arena_read::<i64>(graph,token){{core::Result::Ok(saved)=>{{return n+(*saved);}},core::Result::Err(_)=>{{return -2;}},}}}}}}"
            ),
            16,
        ),
        (
            "arena unused success",
            format!(
                "fn probe()->i64{{arena graph(core::allocator()){{{ARENA}match core::arena_read::<i64>(graph,token){{core::Result::Ok(_)=>{{{REPLACE_ARENA}return 9;}},core::Result::Err(_)=>{{return -1;}},}}}}}}"
            ),
            9,
        ),
        (
            "whole unused result",
            format!(
                "fn probe()->i64{{{VECTOR}match core::vec_get_mut::<i64>(values,0){{_=>{{{PUSH}return core::vec_len::<i64>(values);}},}}}}"
            ),
            2,
        ),
    ]
}

fn vm(
    source: &str,
    allocator: &mut a::Allocator<Provider>,
    control: Control<'_>,
) -> engine::Outcome {
    let source = format!("{source}\nfn main()->i64{{return probe();}}");
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("provisional.lang"),
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
    let Program::Ownership(program) = analysis.result.unwrap() else {
        panic!("ownership fixture");
    };
    engine::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(allocator),
        control,
    )
    .unwrap()
}

#[test]
fn core_read_success_unused_and_failure_arms_agree_in_source_vm_and_repl() {
    for (name, source, expected) in cases() {
        let (mut allocator, trace) = allocator(Default::default());
        assert_eq!(
            vm(&source, &mut allocator, Control::default()).value,
            expected,
            "{name}"
        );
        released(&allocator, &trace);
        let mut session = Session::default();
        commit(&mut session, &source);
        let count = session.definition_count();
        for _ in 0..2 {
            expect_value(
                eval(&mut session, "probe()", &mut allocator, Control::default()),
                &expected.to_string(),
            );
            released(&allocator, &trace);
            assert_eq!(session.definition_count(), count);
        }
        expect_value(
            eval(&mut session, "1+2", &mut allocator, Control::default()),
            "3",
        );
        released(&allocator, &trace);
    }
}

#[test]
fn named_core_reads_keep_the_loan_even_when_the_binding_is_unused() {
    for (name, prefix, read, pattern, change, suffix) in [
        (
            "vec",
            VECTOR,
            "core::vec_get::<i64>(values,0)",
            "core::Result::Ok(held)",
            PUSH,
            "",
        ),
        (
            "vec mut",
            VECTOR,
            "core::vec_get_mut::<i64>(values,0)",
            "core::Result::Ok(held)",
            PUSH,
            "",
        ),
        (
            "map",
            MAP,
            "core::map_get::<i64,i64>(values,key)",
            "core::Option::Some(held)",
            INSERT,
            "",
        ),
        (
            "shared",
            SHARED,
            "core::shared_read::<i64>(owner)",
            "core::Result::Ok(held)",
            REPLACE_SHARED,
            "",
        ),
        (
            "arena",
            ARENA,
            "core::arena_read::<i64>(graph,token)",
            "core::Result::Ok(held)",
            REPLACE_ARENA,
            "}",
        ),
    ] {
        let region = if name == "arena" {
            "arena graph(core::allocator()){ "
        } else {
            ""
        };
        let source = format!(
            "fn rejected()->i64{{{region}{prefix}match {read}{{{pattern}=>{{{change}return 0;}},_=>{{return -1;}},}}{suffix}}}"
        );
        let analysis = frontend::analyze_virtual(
            SourceMap::new(),
            Path::new("named-read.lang"),
            source.as_bytes(),
            None,
            CompileLimits::default(),
            None,
        );
        assert!(analysis.result.is_err(), "{name}");
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|d| d.code.as_str() == "SRC0021"),
            "{name}: {:?}",
            analysis.diagnostics
        );
        let mut session = Session::default();
        commit(&mut session, "fn retained()->i64{return 42;}");
        let count = session.definition_count();
        let (mut allocator, trace) = allocator(Default::default());
        let report = eval(&mut session, &source, &mut allocator, Control::default());
        assert!(report.result.is_err(), "{name}");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code.as_str() == "SRC0021"),
            "{name}: {report:?}"
        );
        assert_eq!(session.definition_count(), count);
        assert_eq!(trace.borrow().requests, 0, "rejected item has no effects");
        expect_value(
            eval(
                &mut session,
                "retained()",
                &mut allocator,
                Control::default(),
            ),
            "42",
        );
        released(&allocator, &trace);
    }
}

#[test]
fn shared_read_failure_ends_only_the_failed_provisional_loan() {
    // Keep 256 named reads live; the next shared_read returns AllocError::Limit.
    // The surviving enclosing reads remain usable while the failed read ends.
    let source = format!(
        r#"
        fn dive(owner:&shared i64,n:i64)->i64 {{
            match core::shared_read::<i64>(owner) {{
                core::Result::Ok(held)=>{{
                    if n < 300 {{return dive(owner,n+1)+(*held)+(-7);}}
                    return -1;
                }},
                core::Result::Err(error)=>{{
                    match error {{
                        core::OwnershipError::Allocation(allocation)=>{{
                            match allocation {{core::AllocError::Limit=>{{return n;}},_=>{{return -2;}},}}
                        }},
                        _=>{{return -3;}},
                    }}
                }},
            }}
        }}
        fn probe()->i64 {{{SHARED}let mut count:i64=dive(owner,0);{REPLACE_SHARED}
            match core::shared_read::<i64>(owner) {{
                core::Result::Ok(saved)=>{{return count+(*saved);}},
                core::Result::Err(_)=>{{return -4;}},
            }}
        }}
    "#
    );
    let control = || {
        Control::with_limits(lang_vm::unit::Limits {
            frames: Some(512),
            ..Default::default()
        })
    };
    let (mut allocator, trace) = allocator(Default::default());
    assert_eq!(vm(&source, &mut allocator, control()).value, 265);
    released(&allocator, &trace);
    let mut session = Session::default();
    // REPL commits one item at a time.
    let split = source.find("fn probe").unwrap();
    commit(&mut session, &source[..split]);
    commit(&mut session, &source[split..]);
    for _ in 0..2 {
        expect_value(
            eval(&mut session, "probe()", &mut allocator, control()),
            "265",
        );
        released(&allocator, &trace);
    }
    expect_value(
        eval(&mut session, "1+2", &mut allocator, Control::default()),
        "3",
    );
}
