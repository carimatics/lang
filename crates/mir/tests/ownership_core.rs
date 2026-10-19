use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, core as c, declarations as d, places as p},
};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1024,
    }
}
fn r(n: usize) -> m::Register {
    m::Register(n as u32)
}
fn instruction(results: &[m::Register], op: b::Op) -> b::Instruction {
    b::Instruction {
        results: results.to_vec(),
        op,
        span: span(),
    }
}
fn block(instructions: Vec<b::Instruction>, term: b::Term) -> b::Block {
    b::Block {
        parameters: vec![],
        instructions,
        terminator: b::Terminator {
            kind: term,
            span: span(),
        },
        span: span(),
    }
}
fn ctor(t: &mut b::BodyTable, constructor: o::Constructor) -> o::Type {
    let v = &mut t.places.declarations.types.constructed;
    let id = o::ConstructedId(v.len() as u32);
    v.push(o::Constructed {
        constructor,
        span: span(),
    });
    o::Type::Constructed(id)
}
fn value(t: &mut b::BodyTable, ty: o::Type) -> m::Register {
    let v = &mut t.places.functions[0].values;
    let id = r(v.len());
    v.push(o::TypeUse { ty, span: span() });
    id
}
fn base() -> b::BodyTable {
    b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "core.lang".into(),
                        text: "x".repeat(1024),
                    }],
                    definitions: vec![],
                    constructed: vec![o::Constructed {
                        constructor: o::Constructor::ArenaOwner(o::ArenaOwner {
                            function: m::FunctionId(0),
                            local: 0,
                        }),
                        span: span(),
                    }],
                },
                imports: vec![],
                functions: vec![d::Function {
                    abi: d::FunctionAbi::Guest,
                    name: "f".into(),
                    public: false,
                    span: span(),
                    signature_span: span(),
                    parameters: vec![],
                    result: o::TypeUse {
                        ty: o::Type::I64,
                        span: span(),
                    },
                    locals: vec![d::Local {
                        name: "owner".into(),
                        span: span(),
                        kind: d::LocalKind::ArenaOwner,
                        ty: o::TypeUse {
                            ty: o::Type::Constructed(o::ConstructedId(0)),
                            span: span(),
                        },
                    }],
                }],
            },
            functions: vec![p::FunctionPlaces::default()],
        },
        functions: vec![b::FunctionBody::default()],
    }
}
fn parts(text: &str) -> Vec<&str> {
    if text.trim().is_empty() {
        return vec![];
    }
    let (mut depth, mut start) = (0, 0);
    let mut out = vec![];
    for (i, byte) in text.bytes().enumerate() {
        match byte {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b',' if depth == 0 => {
                out.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(text[start..].trim());
    out
}
fn parse(t: &mut b::BodyTable, text: &str, arguments: [o::Type; 2]) -> o::Type {
    let text = text.trim();
    for (prefix, kind) in [
        ("&mut ", o::LoanKind::Exclusive),
        ("&", o::LoanKind::Shared),
    ] {
        if let Some(inner) = text.strip_prefix(prefix) {
            let inner = parse(t, inner, arguments);
            return ctor(t, o::Constructor::Loan(kind, inner));
        }
    }
    for prefix in ["shared ", "weak ", "arena "] {
        if let Some(inner) = text.strip_prefix(prefix) {
            let inner = parse(t, inner, arguments);
            return ctor(
                t,
                match prefix {
                    "shared " => o::Constructor::Shared(inner),
                    "weak " => o::Constructor::Weak(inner),
                    _ => o::Constructor::Arena(inner),
                },
            );
        }
    }
    if let Some((name, inner)) = text.split_once('<') {
        let fields = parts(inner.strip_suffix('>').unwrap());
        let a = parse(t, fields[0], arguments);
        let constructor = match name {
            "Vec" => o::Constructor::Vec(a),
            "Option" => o::Constructor::Option(a),
            "Result" => o::Constructor::Result(a, parse(t, fields[1], arguments)),
            "Map" => o::Constructor::Map(a, parse(t, fields[1], arguments)),
            _ => panic!("{name}"),
        };
        return ctor(t, constructor);
    }
    match text {
        "i64" => return o::Type::I64,
        "bool" => return o::Type::Bool,
        "T" => return arguments[0],
        "V" => return arguments[1],
        "arena-owner" => return o::Type::Constructed(o::ConstructedId(0)),
        _ => {}
    }
    let builtin = match text {
        "Unit" => o::Builtin::Unit,
        "Text" => o::Builtin::Text,
        "Bytes" => o::Builtin::Bytes,
        "Allocator" => o::Builtin::Allocator,
        "AllocError" => o::Builtin::AllocError,
        "BoundsError" => o::Builtin::BoundsError,
        "BufferError" => o::Builtin::BufferError,
        "Utf8Error" => o::Builtin::Utf8Error,
        "OwnershipError" => o::Builtin::OwnershipError,
        "ArenaError" => o::Builtin::ArenaError,
        _ => panic!("{text}"),
    };
    ctor(t, o::Constructor::Builtin(builtin))
}
fn rows() -> Vec<(&'static str, usize, &'static str)> {
    include_str!("../../../tests/fixtures/core-signatures.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|line| {
            let mut p = line.split('|');
            (
                p.next().unwrap(),
                p.next().unwrap().parse().unwrap(),
                p.next().unwrap(),
            )
        })
        .collect()
}
struct Fixture {
    table: b::BodyTable,
    call: c::Call,
    arguments: Vec<o::Type>,
    result: o::Type,
}
fn fixture(name: &str, arity: usize, signature: &str, variant: usize) -> Fixture {
    let mut table = base();
    let text = ctor(&mut table, o::Constructor::Builtin(o::Builtin::Text));
    table
        .places
        .declarations
        .types
        .definitions
        .push(o::Definition {
            name: "Record".into(),
            public: false,
            span: span(),
            kind: o::DefinitionKind::Record(vec![o::Field {
                name: "value".into(),
                span: span(),
                ty: o::TypeUse {
                    ty: text,
                    span: span(),
                },
            }]),
        });
    let element = [o::Type::I64, text, o::Type::Nominal(m::DefinitionId(0))][variant];
    let type_arguments = if arity == 2 {
        vec![if variant == 2 { text } else { o::Type::I64 }, element]
    } else if arity == 1 {
        vec![element]
    } else {
        vec![]
    };
    let args = [
        type_arguments.first().copied().unwrap_or(o::Type::I64),
        type_arguments.get(1).copied().unwrap_or(o::Type::I64),
    ];
    let (parameters, result) = signature.split_once(") -> ").unwrap();
    let arguments: Vec<_> = parts(parameters.strip_prefix('(').unwrap())
        .into_iter()
        .map(|p| parse(&mut table, p, args))
        .collect();
    let result = parse(&mut table, result, args);
    let lease = matches!(
        name,
        "allocator" | "vec_get" | "vec_get_mut" | "map_get" | "shared_read" | "arena_read"
    );
    let call = c::Call {
        operation: c::Operation::from_name(name).unwrap(),
        type_arguments: type_arguments
            .into_iter()
            .map(|ty| o::TypeUse { ty, span: span() })
            .collect(),
        arguments: (0..arguments.len()).map(r).collect(),
        result_loan: lease.then_some(b::LoanId(0)),
    };
    Fixture {
        table,
        call,
        arguments,
        result,
    }
}
fn check(f: Fixture) -> Result<c::CheckedCall, o::Error> {
    let declarations = f
        .table
        .places
        .declarations
        .verify(Default::default())
        .unwrap();
    c::check(
        &declarations,
        m::FunctionId(0),
        &f.call,
        &f.arguments,
        f.result,
        span(),
        &mut m::Budget::new(Default::default()),
    )
}
fn by_name(name: &str) -> Fixture {
    let (name, arity, signature) = rows().into_iter().find(|r| r.0 == name).unwrap();
    fixture(name, arity, signature, 0)
}
fn loan_check(t: b::BodyTable) -> Result<o::loans::LoanCheckedBodies, o::loans::Error> {
    let typed = t.verify(Default::default()).unwrap();
    let moved = o::moves::analyze(typed, Default::default()).unwrap();
    let active = o::loan_activity::analyze(moved, Default::default()).unwrap();
    o::loans::analyze(active, Default::default())
}
fn fully_check(t: b::BodyTable) -> o::loans::LoanCheckedBodies {
    loan_check(t).unwrap()
}
fn body(mut f: Fixture) -> b::BodyTable {
    let mut instructions = vec![];
    let mut arguments = vec![];
    for ty in &f.arguments {
        let declaration = &mut f.table.places.declarations.functions[0];
        let local = d::LocalId(declaration.locals.len() as u32);
        declaration.locals.push(d::Local {
            name: format!("argument{}", local.0),
            span: span(),
            kind: d::LocalKind::Parameter,
            ty: o::TypeUse {
                ty: *ty,
                span: span(),
            },
        });
        declaration.parameters.push(local);
        let output = value(&mut f.table, *ty);
        let places = &mut f.table.places.functions[0].places;
        let place = p::PlaceId(places.len() as u32);
        places.push(p::Place {
            origin: output,
            root: p::Root::Local(local),
            projections: vec![],
            span: span(),
        });
        instructions.push(instruction(&[output], b::Op::MovePlace(place)));
        arguments.push(output);
    }
    f.call.arguments = arguments;
    let output = value(&mut f.table, f.result);
    let zero = value(&mut f.table, o::Type::I64);
    let lease = f.call.result_loan;
    instructions.push(instruction(&[output], b::Op::CoreCall(f.call)));
    instructions.push(instruction(&[], b::Op::DropValue(output)));
    if let Some(loan) = lease {
        instructions.push(instruction(
            &[],
            b::Op::EndBorrow {
                loan,
                mode: b::EndMode::Required,
            },
        ));
    }
    instructions.push(instruction(&[zero], b::Op::I64(0)));
    f.table.functions[0].loans = usize::from(lease.is_some());
    f.table.functions[0].blocks = vec![block(instructions, b::Term::Return(zero))];
    f.table
}

#[test]
fn all_contracts_accept_three_independent_concrete_rfc_signatures() {
    assert_eq!(rows().len(), 37);
    let mut seen = std::collections::BTreeSet::new();
    for (name, arity, signature) in rows() {
        for variant in 0..3 {
            let f = fixture(name, arity, signature, variant);
            seen.insert(f.call.operation);
            let out = check(f).unwrap();
            if name.starts_with("arena_") {
                assert_eq!(
                    out.arena_owner,
                    Some(o::ArenaOwner {
                        function: m::FunctionId(0),
                        local: 0
                    })
                );
            } else {
                assert_eq!(out.arena_owner, None);
            }
        }
    }
    assert_eq!(seen.len(), c::Operation::ALL.len());
    assert!(c::Operation::from_name("vec_clear").is_none());
}

#[test]
fn non_arena_calls_flow_through_types_moves_activity_and_provenance() {
    for (name, arity, signature) in rows().into_iter().filter(|r| !r.0.starts_with("arena_")) {
        let out = fully_check(body(fixture(name, arity, signature, 0)));
        let typed = out.activity().moved().typed();
        let facts = &typed.facts()[0];
        assert_eq!(facts.core_calls().len(), 1);
        assert_eq!(facts.core_calls()[0].operation.name(), name);
        assert_eq!(facts.needs_instance_allocator(), name == "allocator");
    }
}

#[test]
fn extraction_checks_reject_forged_result_types_and_nonstorable_specializations() {
    for (name, case, panic_code) in [
        ("option_unwrap", 1, -1),
        ("result_unwrap", 0, -2),
        ("result_unwrap_err", 1, -3),
    ] {
        let contract = c::Operation::from_name(name).unwrap().extraction().unwrap();
        assert_eq!(contract, c::Extraction { case, panic_code });
        let mut f = by_name(name);
        f.result = o::Type::Bool;
        assert!(matches!(
            check(f).unwrap_err().kind,
            o::ErrorKind::CoreType {
                position: c::Position::Result,
                ..
            }
        ));
        for index in 0..c::Operation::from_name(name)
            .unwrap()
            .signature()
            .type_arguments
            .len()
        {
            let mut f = by_name(name);
            let loan = parse(&mut f.table, "&i64", [o::Type::I64; 2]);
            f.call.type_arguments[index].ty = loan;
            assert!(matches!(
                check(f).unwrap_err().kind,
                o::ErrorKind::CoreTypeArgument {
                    rule: c::TypeArgumentRule::Storable,
                    ..
                }
            ));
        }
    }
}

#[test]
fn core_type_value_arity_result_shapes_and_required_loan_ids_are_checked() {
    let mut f = by_name("vec_len");
    f.call.type_arguments.clear();
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::CoreTypeArity { .. }
    ));
    let mut f = by_name("text_len");
    f.call.arguments.clear();
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::CallArity { actual: 0, .. }
    ));
    let mut f = by_name("bytes_set");
    f.arguments[2] = o::Type::Bool;
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::CoreType {
            position: c::Position::Argument(2),
            ..
        }
    ));
    let mut f = by_name("text_equal");
    f.result = o::Type::I64;
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::CoreType {
            position: c::Position::Result,
            ..
        }
    ));
    for name in [
        "allocator",
        "vec_get",
        "vec_get_mut",
        "map_get",
        "shared_read",
        "arena_read",
    ] {
        let mut f = by_name(name);
        f.call.result_loan = None;
        assert!(matches!(
            check(f).unwrap_err().kind,
            o::ErrorKind::CoreLoan { expected: true, .. }
        ));
    }
    let mut f = by_name("vec_pop");
    f.call.result_loan = Some(b::LoanId(0));
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::CoreLoan {
            expected: false,
            ..
        }
    ));
}

#[test]
fn generic_storage_key_sharing_and_source_span_rules_are_enforced() {
    let mut f = by_name("map_len");
    f.call.type_arguments[0].ty = o::Type::Bool;
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::CoreTypeArgument {
            index: 0,
            rule: c::TypeArgumentRule::MapKey,
            ..
        }
    ));
    let mut f = by_name("vec_new");
    let borrowed = parse(&mut f.table, "&i64", [o::Type::I64; 2]);
    f.call.type_arguments[0].ty = borrowed;
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::CoreTypeArgument {
            rule: c::TypeArgumentRule::Storable,
            ..
        }
    ));
    let mut f = by_name("shared_new");
    let arena = parse(&mut f.table, "arena i64", [o::Type::I64; 2]);
    f.call.type_arguments[0].ty = arena;
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::CoreTypeArgument {
            rule: c::TypeArgumentRule::SharedPayload,
            ..
        }
    ));
    let mut f = by_name("vec_new");
    f.call.type_arguments[0].span.source = m::SourceId(9);
    assert!(matches!(check(f).unwrap_err().kind, o::ErrorKind::Mir(_)));
    let mut f = by_name("vec_new");
    f.call.type_arguments[0].ty = o::Type::Constructed(o::ConstructedId(9999));
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::InvalidType(_)
    ));
}

#[test]
fn only_the_current_functions_lexical_owner_can_authorize_an_arena_call() {
    let mut f = by_name("arena_new");
    let other_owner = ctor(
        &mut f.table,
        o::Constructor::ArenaOwner(o::ArenaOwner {
            function: m::FunctionId(1),
            local: 0,
        }),
    );
    let mut other = f.table.places.declarations.functions[0].clone();
    other.name = "g".into();
    other.locals[0].ty.ty = other_owner;
    f.table.places.declarations.functions.push(other);
    let borrowed = ctor(
        &mut f.table,
        o::Constructor::Loan(o::LoanKind::Exclusive, other_owner),
    );
    f.arguments[0] = borrowed;
    assert!(matches!(
        check(f).unwrap_err().kind,
        o::ErrorKind::CoreType {
            position: c::Position::Argument(0),
            ..
        }
    ));
}

#[test]
fn provisional_sum_storage_nesting_forwarding_and_block_arguments_are_rejected() {
    for operation in 0..4 {
        let mut t = body(by_name("vec_get"));
        let (position, output) = t.functions[0].blocks[0]
            .instructions
            .iter()
            .enumerate()
            .find_map(|(n, i)| matches!(i.op, b::Op::CoreCall(_)).then_some((n, i.results[0])))
            .unwrap();
        let result = t.places.functions[0].values[output.0 as usize].ty;
        let replacement = match operation {
            0 => {
                let local = d::LocalId(t.places.declarations.functions[0].locals.len() as u32);
                t.places.declarations.functions[0].locals.push(d::Local {
                    name: "stored".into(),
                    span: span(),
                    kind: d::LocalKind::Mutable,
                    ty: o::TypeUse {
                        ty: result,
                        span: span(),
                    },
                });
                instruction(
                    &[],
                    b::Op::Initialize {
                        local,
                        value: output,
                    },
                )
            }
            1 => {
                let moved = value(&mut t, result);
                instruction(&[moved], b::Op::Move(output))
            }
            2 => {
                let nested = ctor(&mut t, o::Constructor::Option(result));
                let nested = value(&mut t, nested);
                instruction(
                    &[nested],
                    b::Op::Variant {
                        case: 1,
                        payload: vec![output],
                    },
                )
            }
            _ => {
                let mut destination = t.functions[0].blocks[0].clone();
                let parameter = value(&mut t, result);
                destination.parameters = vec![parameter];
                destination.instructions.clear();
                let zero = value(&mut t, o::Type::I64);
                destination
                    .instructions
                    .push(instruction(&[zero], b::Op::I64(0)));
                destination.terminator.kind = b::Term::Return(zero);
                t.functions[0].blocks.push(destination);
                t.functions[0].blocks[0].instructions.truncate(position + 1);
                t.functions[0].blocks[0].terminator.kind = b::Term::Jump(m::Target {
                    block: m::BlockId(1),
                    arguments: vec![output],
                });
                assert!(matches!(
                    t.verify(Default::default()).unwrap_err().kind,
                    o::ErrorKind::CoreProvisionalUse(_)
                ));
                continue;
            }
        };
        t.functions[0].blocks[0]
            .instructions
            .insert(position + 1, replacement);
        assert!(matches!(
            t.verify(Default::default()).unwrap_err().kind,
            o::ErrorKind::CoreProvisionalUse(_)
        ));
    }
}

#[test]
fn owning_core_arguments_are_consumed_and_cannot_be_implicitly_restored_on_failure() {
    for name in [
        "text_from_utf8",
        "option_unwrap",
        "result_unwrap",
        "result_unwrap_err",
    ] {
        let (name, arity, signature) = rows().into_iter().find(|row| row.0 == name).unwrap();
        let mut t = body(fixture(name, arity, signature, 1));
        let (position, argument) = t.functions[0].blocks[0]
            .instructions
            .iter()
            .enumerate()
            .find_map(|(n, i)| {
                if let b::Op::CoreCall(call) = &i.op {
                    Some((n, call.arguments[0]))
                } else {
                    None
                }
            })
            .unwrap();
        t.functions[0].blocks[0]
            .instructions
            .insert(position + 1, instruction(&[], b::Op::DropValue(argument)));
        let typed = t.verify(Default::default()).unwrap();
        assert!(matches!(
            o::moves::analyze(typed, Default::default())
                .unwrap_err()
                .kind,
            o::moves::ErrorKind::Unavailable { .. }
        ));
    }
}

#[test]
fn arena_core_calls_retain_the_lexical_owner_until_the_read_ends() {
    let mut t = base();
    let owner = o::Type::Constructed(o::ConstructedId(0));
    let allocator = parse(&mut t, "&Allocator", [o::Type::I64; 2]);
    let mutable_owner = parse(&mut t, "&mut arena-owner", [o::Type::I64; 2]);
    let shared_owner = parse(&mut t, "&arena-owner", [o::Type::I64; 2]);
    let created = parse(&mut t, "Result<arena i64, ArenaError>", [o::Type::I64; 2]);
    let read = parse(&mut t, "Result<&i64, ArenaError>", [o::Type::I64; 2]);
    let replaced = parse(&mut t, "Result<Unit, ArenaError>", [o::Type::I64; 2]);
    let token_type = parse(&mut t, "arena i64", [o::Type::I64; 2]);
    let reference_type = parse(&mut t, "&i64", [o::Type::I64; 2]);
    let a = value(&mut t, allocator);
    let number = value(&mut t, o::Type::I64);
    let new_owner = value(&mut t, mutable_owner);
    let raw_new = value(&mut t, created);
    let new_guard = value(&mut t, o::Type::Bool);
    let token = value(&mut t, token_type);
    let read_owner = value(&mut t, shared_owner);
    let raw_read = value(&mut t, read);
    let read_guard = value(&mut t, o::Type::Bool);
    let reference = value(&mut t, reference_type);
    let result = value(&mut t, o::Type::I64);
    let replace_owner = value(&mut t, mutable_owner);
    let raw_replace = value(&mut t, replaced);
    let origin = value(&mut t, owner);
    t.places.functions[0].places = vec![
        p::Place {
            origin,
            root: p::Root::Local(d::LocalId(0)),
            projections: vec![],
            span: span(),
        },
        p::Place {
            origin: result,
            root: p::Root::Value(reference),
            projections: vec![p::Projection {
                kind: p::ProjectionKind::Dereference,
                span: span(),
            }],
            span: span(),
        },
    ];
    let call = |operation, arguments, loan: Option<u32>| {
        b::Op::CoreCall(c::Call {
            operation,
            type_arguments: if operation == c::Operation::Allocator {
                vec![]
            } else {
                vec![o::TypeUse {
                    ty: o::Type::I64,
                    span: span(),
                }]
            },
            arguments,
            result_loan: loan.map(b::LoanId),
        })
    };
    let borrow = |id, kind| b::Op::Borrow {
        loan: b::LoanId(id),
        kind,
        place: p::PlaceId(0),
    };
    let end = |id| {
        instruction(
            &[],
            b::Op::EndBorrow {
                loan: b::LoanId(id),
                mode: b::EndMode::Required,
            },
        )
    };
    let exit = || {
        instruction(
            &[],
            b::Op::ArenaExit {
                owner: d::LocalId(0),
            },
        )
    };
    let branch = |condition, yes, no| b::Term::Branch {
        condition,
        then_target: m::Target {
            block: m::BlockId(yes),
            arguments: vec![],
        },
        else_target: m::Target {
            block: m::BlockId(no),
            arguments: vec![],
        },
    };
    t.functions[0].loans = 5;
    t.functions[0].blocks = vec![
        block(
            vec![
                instruction(&[a], call(c::Operation::Allocator, vec![], Some(0))),
                instruction(&[number], b::Op::I64(7)),
                instruction(
                    &[],
                    b::Op::ArenaEnter {
                        owner: d::LocalId(0),
                        allocator: a,
                    },
                ),
                instruction(&[new_owner], borrow(1, o::LoanKind::Exclusive)),
                instruction(
                    &[raw_new],
                    call(c::Operation::ArenaNew, vec![new_owner, number], None),
                ),
                end(1),
                instruction(
                    &[new_guard],
                    b::Op::IsCase {
                        value: raw_new,
                        case: 0,
                    },
                ),
            ],
            branch(new_guard, 1, 2),
        ),
        block(
            vec![
                instruction(
                    &[token],
                    b::Op::UnpackVariant {
                        value: raw_new,
                        case: 0,
                        guard: new_guard,
                    },
                ),
                instruction(&[read_owner], borrow(2, o::LoanKind::Shared)),
                instruction(
                    &[raw_read],
                    call(c::Operation::ArenaRead, vec![read_owner, token], Some(3)),
                ),
                instruction(
                    &[read_guard],
                    b::Op::IsCase {
                        value: raw_read,
                        case: 0,
                    },
                ),
            ],
            branch(read_guard, 3, 4),
        ),
        block(
            vec![instruction(&[], b::Op::DropValue(raw_new)), exit(), end(0)],
            b::Term::Return(number),
        ),
        block(
            vec![
                instruction(
                    &[reference],
                    b::Op::UnpackVariant {
                        value: raw_read,
                        case: 0,
                        guard: read_guard,
                    },
                ),
                instruction(&[result], b::Op::CopyPlace(p::PlaceId(1))),
                end(3),
                end(2),
                instruction(&[replace_owner], borrow(4, o::LoanKind::Exclusive)),
                instruction(
                    &[raw_replace],
                    call(
                        c::Operation::ArenaReplace,
                        vec![replace_owner, token, number],
                        None,
                    ),
                ),
                instruction(&[], b::Op::DropValue(raw_replace)),
                end(4),
                exit(),
                end(0),
            ],
            b::Term::Return(result),
        ),
        block(
            vec![
                instruction(&[], b::Op::DropValue(raw_read)),
                end(3),
                end(2),
                exit(),
                end(0),
            ],
            b::Term::Return(number),
        ),
    ];
    let checked = fully_check(t.clone());
    let facts = &checked.activity().moved().typed().facts()[0];
    assert!(facts.needs_instance_allocator());
    for fact in &facts.core_calls()[1..] {
        assert_eq!(
            fact.checked.arena_owner,
            Some(o::ArenaOwner {
                function: m::FunctionId(0),
                local: 0
            })
        );
    }
    // Ending the arena while its returned reference is live is a provenance error.
    let yes = &mut t.functions[0].blocks[3].instructions;
    let i = yes
        .iter()
        .position(|i| matches!(i.op, b::Op::ArenaExit { .. }))
        .unwrap();
    let early = yes.remove(i);
    yes.drain(4..8);
    yes.insert(2, early);
    assert!(loan_check(t).is_err());
}

fn get_flow(name: &str, early_parent: bool) -> b::BodyTable {
    let mut t = body(by_name(name));
    let position = t.functions[0].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.op, b::Op::CoreCall(_)))
        .unwrap();
    let mut prefix = t.functions[0].blocks[0].instructions[..position].to_vec();
    let mut call = t.functions[0].blocks[0].instructions[position].clone();
    let b::Op::CoreCall(core) = &mut call.op else {
        unreachable!()
    };
    let original = core.arguments[0];
    let reference = t.places.functions[0].values[original.0 as usize].ty;
    let o::Type::Constructed(id) = reference else {
        unreachable!()
    };
    let o::Constructor::Loan(kind, target) =
        t.places.declarations.types.constructed[id.0 as usize].constructor
    else {
        unreachable!()
    };
    let origin = value(&mut t, target);
    let path = p::PlaceId(t.places.functions[0].places.len() as u32);
    t.places.functions[0].places.push(p::Place {
        origin,
        root: p::Root::Value(original),
        projections: vec![p::Projection {
            kind: p::ProjectionKind::Dereference,
            span: span(),
        }],
        span: span(),
    });
    let parent = value(&mut t, reference);
    prefix.push(instruction(
        &[parent],
        b::Op::Borrow {
            loan: b::LoanId(1),
            kind,
            place: path,
        },
    ));
    core.arguments[0] = parent;
    let mut key_loan = None;
    if name == "map_get" {
        let original = core.arguments[1];
        let reference = t.places.functions[0].values[original.0 as usize].ty;
        let o::Type::Constructed(id) = reference else {
            unreachable!()
        };
        let o::Constructor::Loan(kind, target) =
            t.places.declarations.types.constructed[id.0 as usize].constructor
        else {
            unreachable!()
        };
        let origin = value(&mut t, target);
        let path = p::PlaceId(t.places.functions[0].places.len() as u32);
        t.places.functions[0].places.push(p::Place {
            origin,
            root: p::Root::Value(original),
            projections: vec![p::Projection {
                kind: p::ProjectionKind::Dereference,
                span: span(),
            }],
            span: span(),
        });
        let key = value(&mut t, reference);
        prefix.push(instruction(
            &[key],
            b::Op::Borrow {
                loan: b::LoanId(2),
                kind,
                place: path,
            },
        ));
        core.arguments[1] = key;
        key_loan = Some(b::LoanId(2));
    }
    let raw = call.results[0];
    let result = t.places.functions[0].values[raw.0 as usize].ty;
    let o::Type::Constructed(id) = result else {
        unreachable!()
    };
    let loan_type = match t.places.declarations.types.constructed[id.0 as usize].constructor {
        o::Constructor::Option(t) | o::Constructor::Result(t, _) => t,
        _ => unreachable!(),
    };
    let success = if name == "map_get" { 1 } else { 0 };
    let reference = value(&mut t, loan_type);
    let read = value(&mut t, o::Type::I64);
    let guard = value(&mut t, o::Type::Bool);
    let zero = value(&mut t, o::Type::I64);
    let path = p::PlaceId(t.places.functions[0].places.len() as u32);
    t.places.functions[0].places.push(p::Place {
        origin: read,
        root: p::Root::Value(reference),
        projections: vec![p::Projection {
            kind: p::ProjectionKind::Dereference,
            span: span(),
        }],
        span: span(),
    });
    prefix.push(call);
    if let Some(loan) = key_loan {
        prefix.push(instruction(
            &[],
            b::Op::EndBorrow {
                loan,
                mode: b::EndMode::Required,
            },
        ));
    }
    if early_parent {
        prefix.push(instruction(
            &[],
            b::Op::EndBorrow {
                loan: b::LoanId(1),
                mode: b::EndMode::Required,
            },
        ));
    }
    prefix.extend([
        instruction(&[zero], b::Op::I64(0)),
        instruction(
            &[guard],
            b::Op::IsCase {
                value: raw,
                case: success,
            },
        ),
    ]);
    let cleanup = || {
        [
            instruction(
                &[],
                b::Op::EndBorrow {
                    loan: b::LoanId(0),
                    mode: b::EndMode::Required,
                },
            ),
            instruction(
                &[],
                b::Op::EndBorrow {
                    loan: b::LoanId(1),
                    mode: b::EndMode::IfActive,
                },
            ),
        ]
    };
    let mut yes = vec![
        instruction(
            &[reference],
            b::Op::UnpackVariant {
                value: raw,
                case: success,
                guard,
            },
        ),
        instruction(&[read], b::Op::CopyPlace(path)),
    ];
    yes.extend(cleanup());
    let mut no = vec![instruction(&[], b::Op::DropValue(raw))];
    no.extend(cleanup());
    t.functions[0].loans = if key_loan.is_some() { 3 } else { 2 };
    t.functions[0].blocks = vec![
        block(
            prefix,
            b::Term::Branch {
                condition: guard,
                then_target: m::Target {
                    block: m::BlockId(1),
                    arguments: vec![],
                },
                else_target: m::Target {
                    block: m::BlockId(2),
                    arguments: vec![],
                },
            },
        ),
        block(yes, b::Term::Return(read)),
        block(no, b::Term::Return(zero)),
    ];
    t
}

#[test]
fn successful_get_payloads_preserve_parent_lifetimes_but_map_keys_can_end_before_the_result() {
    for name in ["vec_get", "vec_get_mut", "map_get", "shared_read"] {
        fully_check(get_flow(name, false));
        assert!(matches!(
            loan_check(get_flow(name, true)).unwrap_err().kind,
            o::loans::ErrorKind::EndParent { .. }
        ));
    }
}

#[test]
fn allocator_preflight_requirements_reach_recursive_callers_and_leave_unrelated_functions_clear() {
    let mut t = body(by_name("allocator"));
    for name in ["caller", "recursive", "plain"] {
        let mut function = t.places.declarations.functions[0].clone();
        function.name = name.into();
        function.locals.clear();
        function.parameters.clear();
        t.places.declarations.functions.push(function);
        t.places.functions.push(p::FunctionPlaces {
            values: vec![
                o::TypeUse {
                    ty: o::Type::I64,
                    span: span()
                };
                2
            ],
            places: vec![],
        });
        t.functions.push(b::FunctionBody::default());
    }
    t.functions[1].blocks = vec![block(
        vec![
            instruction(
                &[r(0)],
                b::Op::Call {
                    function: m::FunctionId(2),
                    arguments: vec![],
                },
            ),
            instruction(
                &[r(1)],
                b::Op::Call {
                    function: m::FunctionId(0),
                    arguments: vec![],
                },
            ),
        ],
        b::Term::Return(r(1)),
    )];
    t.functions[2].blocks = vec![block(
        vec![instruction(
            &[r(0)],
            b::Op::Call {
                function: m::FunctionId(1),
                arguments: vec![],
            },
        )],
        b::Term::Return(r(0)),
    )];
    t.functions[3].blocks = vec![block(
        vec![instruction(&[r(0)], b::Op::I64(0))],
        b::Term::Return(r(0)),
    )];
    let typed = t.clone().verify(Default::default()).unwrap();
    assert_eq!(
        typed
            .facts()
            .iter()
            .map(|f| f.needs_instance_allocator())
            .collect::<Vec<_>>(),
        [true, true, true, false]
    );
    assert_eq!(
        typed
            .clone()
            .into_table()
            .verify(Default::default())
            .unwrap(),
        typed
    );
    fully_check(t.clone());
    let zero = t.functions[0].blocks[0]
        .terminator
        .kind
        .operands()
        .next()
        .unwrap();
    t.functions[0].loans = 0;
    t.functions[0].blocks = vec![block(
        vec![instruction(&[zero], b::Op::I64(0))],
        b::Term::Return(zero),
    )];
    assert!(
        t.verify(Default::default())
            .unwrap()
            .facts()
            .iter()
            .all(|f| !f.needs_instance_allocator())
    );
}

#[test]
fn core_loan_creation_sites_and_exact_cumulative_budgets_are_validated() {
    let t = body(by_name("allocator"));
    let out = t.clone().verify(Default::default()).unwrap();
    for dimension in 0..3 {
        let mut limits = b::Limits::default();
        let unit = &mut limits.places.ownership.unit;
        match dimension {
            0 => unit.work = out.work_used(),
            1 => unit.allocations = out.requested_bytes(),
            _ => unit.elements = out.elements_used(),
        }
        assert_eq!(t.clone().verify(limits).unwrap(), out);
        let unit = &mut limits.places.ownership.unit;
        match dimension {
            0 => unit.work -= 1,
            1 => unit.allocations -= 1,
            _ => unit.elements -= 1,
        }
        assert!(matches!(
            t.clone().verify(limits).unwrap_err().kind,
            o::ErrorKind::Mir(m::ErrorKind::Limit { .. })
        ));
    }
    let mut invalid = t.clone();
    let b::Op::CoreCall(call) = &mut invalid.functions[0].blocks[0].instructions[0].op else {
        unreachable!()
    };
    call.result_loan = Some(b::LoanId(99));
    assert_eq!(
        invalid.verify(Default::default()).unwrap_err().kind,
        o::ErrorKind::InvalidLoan(99)
    );
    let mut duplicate = t;
    let mut call = duplicate.functions[0].blocks[0].instructions[0].clone();
    let result_type = duplicate.places.functions[0].values[call.results[0].0 as usize].ty;
    call.results[0] = value(&mut duplicate, result_type);
    duplicate.functions[0].blocks[0]
        .instructions
        .insert(1, call);
    assert_eq!(
        duplicate.verify(Default::default()).unwrap_err().kind,
        o::ErrorKind::DuplicateLoan(0)
    );
}
