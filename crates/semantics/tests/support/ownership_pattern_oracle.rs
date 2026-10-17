//! A small Copy-value test oracle. It deliberately has no heap/loan execution
//! support and is not an implementation of the ownership runtime profile.
use super::*;
use o::{bodies as b, places as p};

#[derive(Clone, Debug, PartialEq)]
enum Value {
    I(i64),
    B(bool),
    Record(Vec<Value>),
    Case(u32, Vec<Value>),
}
fn integer(v: &Value) -> i64 {
    let Value::I(n) = v else { panic!("{v:?}") };
    *n
}
fn boolean(v: &Value) -> bool {
    let Value::B(n) = v else { panic!("{v:?}") };
    *n
}
fn read(path: &p::Place, locals: &[Option<Value>], values: &[Option<Value>]) -> Value {
    let mut v = match path.root {
        p::Root::Local(l) => locals[l.0 as usize].as_ref().unwrap(),
        p::Root::Value(r) => values[r.0 as usize].as_ref().unwrap(),
    };
    for step in &path.projections {
        let p::ProjectionKind::Field(field) = step.kind else {
            panic!("oracle only handles Copy fields")
        };
        let Value::Record(fields) = v else {
            panic!("{v:?}")
        };
        v = &fields[field as usize];
    }
    v.clone()
}
fn write(
    path: &p::Place,
    locals: &mut [Option<Value>],
    values: &mut [Option<Value>],
    value: Value,
) {
    assert!(
        path.projections.is_empty(),
        "oracle fixtures assign whole locals"
    );
    let slot = match path.root {
        p::Root::Local(l) => &mut locals[l.0 as usize],
        p::Root::Value(r) => &mut values[r.0 as usize],
    };
    *slot = Some(value);
}
fn execute(typed: &b::TypedBodies, arguments: Vec<Value>) -> Result<Value, ()> {
    let table = typed.clone().into_table();
    let f = &table.functions[0];
    let frame = &table.places.functions[0];
    let declaration = &table.places.declarations.functions[0];
    let mut locals = vec![None; declaration.locals.len()];
    for (local, value) in declaration.parameters.iter().zip(arguments) {
        locals[local.0 as usize] = Some(value);
    }
    let mut values: Vec<Option<Value>> = vec![None; frame.values.len()];
    let mut prepared = vec![None; f.prepared];
    let mut at = 0;
    for _ in 0..10_000 {
        let block = &f.blocks[at];
        for i in &block.instructions {
            let get = |r: m::Register| values[r.0 as usize].as_ref().unwrap().clone();
            let result = match &i.op {
                b::Op::I64(n) => Some(Value::I(*n)),
                b::Op::Bool(n) => Some(Value::B(*n)),
                b::Op::Unit => Some(Value::Record(vec![])),
                b::Op::Copy(r) | b::Op::Move(r) => Some(get(*r)),
                b::Op::Add(a, z) => {
                    Some(Value::I(integer(&get(*a)).wrapping_add(integer(&get(*z)))))
                }
                b::Op::LessThan(a, z) => Some(Value::B(integer(&get(*a)) < integer(&get(*z)))),
                b::Op::EqualI64(a, z) => Some(Value::B(integer(&get(*a)) == integer(&get(*z)))),
                b::Op::CopyPlace(path) => {
                    Some(read(&frame.places[path.0 as usize], &locals, &values))
                }
                b::Op::Initialize { local, value } => {
                    locals[local.0 as usize] = Some(get(*value));
                    None
                }
                b::Op::PreparePlace { destination, place } => {
                    prepared[destination.0 as usize] = Some(*place);
                    None
                }
                b::Op::ReplacePrepared { destination, value } => {
                    let value = get(*value);
                    let path = prepared[destination.0 as usize].take().unwrap();
                    write(
                        &frame.places[path.0 as usize],
                        &mut locals,
                        &mut values,
                        value,
                    );
                    None
                }
                b::Op::DropLocalIfInitialized(local) => {
                    locals[local.0 as usize] = None;
                    None
                }
                b::Op::DropValueIfAvailable(_) => None,
                b::Op::Cleanup(cleanup) => {
                    for step in &cleanup.steps {
                        match step.target {
                            b::CleanupTarget::Local(local) => locals[local.0 as usize] = None,
                            b::CleanupTarget::Value(_) => {}
                            _ => panic!("non-Copy cleanup in the Copy-only oracle"),
                        }
                    }
                    None
                }
                b::Op::Record { fields, .. } => {
                    Some(Value::Record(fields.iter().map(|r| get(*r)).collect()))
                }
                b::Op::Variant { case, payload } => Some(Value::Case(
                    *case,
                    payload.iter().map(|r| get(*r)).collect(),
                )),
                b::Op::IsCase { value, case } => {
                    let Value::Case(tag, _) = get(*value) else {
                        panic!()
                    };
                    Some(Value::B(tag == *case))
                }
                b::Op::CopyCaseField {
                    value, case, field, ..
                } => {
                    let Value::Case(tag, fields) = get(*value) else {
                        panic!()
                    };
                    assert_eq!(tag, *case);
                    Some(fields[*field as usize].clone())
                }
                other => panic!("outside Copy oracle: {other:?}"),
            };
            if let Some(value) = result {
                assert_eq!(i.results.len(), 1);
                values[i.results[0].0 as usize] = Some(value);
            } else {
                assert!(i.results.is_empty());
            }
        }
        let target = match &block.terminator.kind {
            b::Term::Return(value) => return Ok(values[value.0 as usize].clone().unwrap()),
            b::Term::Unreachable => return Err(()),
            b::Term::Jump(target) => target,
            b::Term::Branch {
                condition,
                then_target,
                else_target,
            } => {
                if boolean(values[condition.0 as usize].as_ref().unwrap()) {
                    then_target
                } else {
                    else_target
                }
            }
        };
        assert!(target.arguments.is_empty());
        at = target.block.0 as usize;
    }
    panic!("oracle did not terminate")
}

#[test]
fn nested_nominal_and_core_patterns_match_an_independent_value_oracle() {
    let checked = automatic_mir(
        "struct Pair { n: i64, flag: bool } enum Item { A(Pair), B(i64), C } fn f(x: Item) -> i64 { match x { Item::A(Pair { n: 0, flag: true }) => { return 1; }, Item::A(Pair { n: n, flag: _ }) => { return n + 10; }, Item::B(0) => { return 2; }, Item::B(other) => { return other + 20; }, Item::C => { return 3; }, } }",
    );
    let typed = checked.activity().moved().typed();
    for n in -3..=3 {
        for flag in [false, true] {
            let input = Value::Case(0, vec![Value::Record(vec![Value::I(n), Value::B(flag)])]);
            assert_eq!(
                execute(typed, vec![input]),
                Ok(Value::I(if n == 0 && flag { 1 } else { n + 10 }))
            );
            assert_eq!(
                execute(typed, vec![Value::Case(1, vec![Value::I(n)])]),
                Ok(Value::I(if n == 0 { 2 } else { n + 20 }))
            );
        }
    }
    assert_eq!(
        execute(typed, vec![Value::Case(2, vec![])]),
        Ok(Value::I(3))
    );
    assert_eq!(execute(typed, vec![Value::Case(99, vec![])]), Err(()));
    let checked = automatic_mir(
        "fn f(x: core::Result<core::Option<i64>, core::AllocError>) -> i64 { match x { core::Result::Ok(core::Option::Some(0)) => { return 3; }, core::Result::Ok(core::Option::Some(n)) => { return n + 4; }, core::Result::Ok(core::Option::None) => { return 5; }, core::Result::Err(core::AllocError::Limit) => { return 6; }, core::Result::Err(_) => { return 7; }, } }",
    );
    let typed = checked.activity().moved().typed();
    for n in -3..=3 {
        assert_eq!(
            execute(
                typed,
                vec![Value::Case(0, vec![Value::Case(1, vec![Value::I(n)])])]
            ),
            Ok(Value::I(if n == 0 { 3 } else { n + 4 }))
        );
    }
    assert_eq!(
        execute(typed, vec![Value::Case(0, vec![Value::Case(0, vec![])])]),
        Ok(Value::I(5))
    );
    for tag in 0..=2 {
        assert_eq!(
            execute(typed, vec![Value::Case(1, vec![Value::Case(tag, vec![])])]),
            Ok(Value::I(if tag == 0 { 6 } else { 7 }))
        );
    }
}

#[test]
fn scalar_and_nested_loop_matches_select_the_expected_arms() {
    let checked = automatic_mir(
        "fn f(x: bool) -> i64 { let mut n: i64 = 0; let mut total: i64 = 0; while n < 4 { match n { 0 => { total = total + 1; }, 1 => { match x { true => { total = total + 2; }, false => { total = total + 3; }, } }, other => { total = total + other; }, } n = n + 1; } return total; }",
    );
    let typed = checked.activity().moved().typed();
    assert_eq!(execute(typed, vec![Value::B(true)]), Ok(Value::I(8)));
    assert_eq!(execute(typed, vec![Value::B(false)]), Ok(Value::I(9)));
}

#[test]
fn unreachable_exits_keep_cleanup_obligations_and_require_ended_loans() {
    let (sources, checked) = prepare(
        "fn f(v: &core::Vec<i64>, t: core::Text) -> i64 { match core::vec_get::<i64>(v, 0) { core::Result::Ok(value) => { return (*value); }, core::Result::Err(_) => { return 0; }, } }",
    );
    let typed = ownership_types::bodies::lower(&checked, &sources, Default::default()).unwrap();
    let mut table = typed.clone().into_table();
    let fault = table.functions[0]
        .blocks
        .iter()
        .position(|b| b.terminator.kind == b::Term::Unreachable)
        .unwrap();
    for i in &mut table.functions[0].blocks[fault].instructions {
        if let b::Op::Cleanup(cleanup) = &mut i.op {
            cleanup
                .steps
                .retain(|s| !matches!(s.target, b::CleanupTarget::Loan(_)));
        }
    }
    let moved = o::moves::analyze(
        table.verify(Default::default()).unwrap(),
        Default::default(),
    )
    .unwrap();
    assert!(matches!(
        o::loan_activity::analyze(moved, Default::default())
            .unwrap_err()
            .kind,
        o::loan_activity::ErrorKind::Unended { .. }
    ));
    let mut table = typed.into_table();
    for i in &mut table.functions[0].blocks[fault].instructions {
        if let b::Op::Cleanup(cleanup) = &mut i.op {
            cleanup
                .steps
                .retain(|s| s.target != b::CleanupTarget::Local(o::declarations::LocalId(1)));
        }
    }
    let moved = o::moves::analyze(
        table.verify(Default::default()).unwrap(),
        Default::default(),
    )
    .unwrap();
    let exit = moved.functions()[0]
        .exits()
        .iter()
        .find(|e| e.block == m::BlockId(fault as u32))
        .unwrap();
    assert!(
        exit.owners()
            .iter()
            .any(|o| o.slot == o::moves::Slot::Local(o::declarations::LocalId(1)))
    );
}
