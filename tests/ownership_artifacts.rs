use lang::frontend::{self, CompileLimits};
use lang_bytecode::ownership as wire;
use lang_diagnostics::SourceMap;
use lang_mir::unit::{self as m, ownership as o};
use lang_semantics::analysis::Program;
use std::path::Path;

const BYTES: &str = include_str!("../examples/scripts/owned_bytes.lang");

fn table(source: &str) -> o::bodies::BodyTable {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("artifact.lang"),
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
    program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table()
}

fn prove(
    table: o::bodies::BodyTable,
    limits: m::Limits,
    initial: m::Usage,
) -> Result<o::failure::Elaborated, Box<dyn std::error::Error>> {
    let cfg = CompileLimits::default().ownership;
    let mut budget = m::Budget::with_usage(limits, initial)?;
    let typed = table.verify_with_budget(
        &mut budget,
        cfg.bodies.places.ownership.constructed,
        cfg.bodies.places.places,
        cfg.bodies.loans,
        cfg.bodies.prepared,
    )?;
    macro_rules! capped {
        ($value:expr) => {{
            let mut value = $value;
            value.work = limits.work;
            value.allocations = limits.allocations;
            value.elements = limits.elements;
            value
        }};
    }
    let moved = o::moves::analyze(typed, capped!(cfg.moves))?;
    let activity = o::loan_activity::analyze(moved, capped!(cfg.activity))?;
    let loaned = o::loans::analyze(activity, capped!(cfg.loans))?;
    Ok(o::failure::elaborate(loaned, capped!(cfg.failure))?)
}

#[test]
fn decode_admission_usage_can_flow_through_every_independent_ownership_proof() {
    let raw = table(BYTES);
    let plain = prove(raw.clone(), m::Limits::default(), m::Usage::default()).unwrap();
    let initial = m::Usage {
        work: 17,
        allocation_bytes: 31,
        elements: 23,
    };
    let seeded = prove(raw.clone(), m::Limits::default(), initial).unwrap();
    assert_eq!(seeded.work_used(), plain.work_used() + initial.work);
    assert_eq!(
        seeded.requested_bytes(),
        plain.requested_bytes() + initial.allocation_bytes
    );
    assert_eq!(
        seeded.elements_used(),
        plain.elements_used() + initial.elements
    );
    for limits in [
        m::Limits {
            work: plain.work_used(),
            ..Default::default()
        },
        m::Limits {
            allocations: plain.requested_bytes(),
            ..Default::default()
        },
        m::Limits {
            elements: plain.elements_used(),
            ..Default::default()
        },
    ] {
        assert!(prove(raw.clone(), limits, m::Usage::default()).is_ok());
        assert!(prove(raw.clone(), limits, initial).is_err());
    }
}

#[test]
fn source_bytes_round_trip_preserves_raw_tables_and_original_spans() {
    let raw = table(BYTES);
    let entry = raw
        .places
        .declarations
        .functions
        .iter()
        .position(|f| f.name == "main")
        .map(|i| m::FunctionId(i.try_into().unwrap()));
    assert!(entry.is_some());
    let bytes = wire::encode(raw.clone(), entry, wire::CodecLimits::default()).unwrap();
    assert_eq!(&bytes[..12], b"LANGBC\0\0\x02\0\0\0");
    let decoded = wire::decode(&bytes, wire::CodecLimits::default()).unwrap();
    let plain = prove(raw.clone(), m::Limits::default(), m::Usage::default()).unwrap();
    assert!(decoded.usage().work > plain.work_used());
    assert!(decoded.usage().allocation_bytes > plain.requested_bytes());
    assert!(decoded.usage().elements > plain.elements_used());
    let (recovered, recovered_entry) = decoded.into_parts();
    assert_eq!(recovered, raw);
    assert_eq!(recovered_entry, entry);
    assert_eq!(
        wire::encode(recovered, recovered_entry, wire::CodecLimits::default()).unwrap(),
        bytes
    );
}

#[test]
fn accepted_ownership_source_shapes_round_trip_without_table_renumbering() {
    for (name, source) in [
        ("text", include_str!("../examples/scripts/text.lang")),
        ("utf8", include_str!("../examples/scripts/utf8.lang")),
        (
            "calls",
            include_str!("../examples/scripts/owned_calls.lang"),
        ),
        (
            "cleanup",
            include_str!("../examples/scripts/cleanup_calls.lang"),
        ),
        (
            "replace",
            include_str!("../examples/scripts/owned_replace.lang"),
        ),
        (
            "aggregate",
            include_str!("../examples/scripts/owned_aggregate.lang"),
        ),
        ("arena", include_str!("../examples/scripts/arena.lang")),
        (
            "arena owned",
            include_str!("../examples/scripts/arena_owned.lang"),
        ),
        ("shared", include_str!("../examples/scripts/shared.lang")),
        (
            "shared owned",
            include_str!("../examples/scripts/shared_owned.lang"),
        ),
        ("vec", include_str!("../examples/scripts/vec.lang")),
        (
            "vec owned",
            include_str!("../examples/scripts/vec_owned.lang"),
        ),
        ("map", include_str!("../examples/scripts/map.lang")),
        (
            "map owned",
            include_str!("../examples/scripts/map_owned.lang"),
        ),
        (
            "nested collections",
            include_str!("../examples/scripts/nested_collections.lang"),
        ),
        (
            "nested cycle",
            include_str!("../examples/scripts/nested_cycle.lang"),
        ),
        (
            "nested arena",
            include_str!("../examples/scripts/nested_arena.lang"),
        ),
        (
            "nested mutation",
            include_str!("../examples/scripts/nested_mutation.lang"),
        ),
    ] {
        let raw = table(source);
        let encoded = wire::encode(raw.clone(), None, wire::CodecLimits::default())
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let decoded = wire::decode(&encoded, wire::CodecLimits::default())
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let (recovered, entry) = decoded.into_parts();
        assert!(recovered == raw, "{name}: raw tables/spans changed");
        assert_eq!(entry, None);
        assert_eq!(
            wire::encode(recovered, entry, wire::CodecLimits::default()).unwrap(),
            encoded,
            "{name}"
        );
    }
}

#[test]
fn forged_region_with_valid_embedded_source_is_independently_rejected() {
    let source = r#"fn main()->i64 {
        arena first(core::allocator()) {
            let mut left_token:arena i64=core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(first,1));
            arena second(core::allocator()) {
                let mut right_token:arena i64=core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(second,2));
                let mut n:i64=0;
                match core::arena_read::<i64>(second,right_token) {core::Result::Ok(other)=>{n=(*other);},core::Result::Err(_)=>{return -2;},}
                match core::arena_read::<i64>(first,left_token) {core::Result::Ok(value)=>{return (*value)+n;},core::Result::Err(_)=>{return -1;},}
            }
        }
    }"#;
    let raw = table(source);
    let locals = &raw.places.declarations.functions[0].locals;
    let local = |name| {
        o::declarations::LocalId(
            locals
                .iter()
                .position(|l| l.name == name)
                .unwrap()
                .try_into()
                .unwrap(),
        )
    };
    let places = &raw.places.functions[0].places;
    let place = |name| {
        o::places::PlaceId(
            places
                .iter()
                .position(|p| {
                    p.root == o::places::Root::Local(local(name)) && p.projections.is_empty()
                })
                .unwrap()
                .try_into()
                .unwrap(),
        )
    };
    let left = place("left_token");
    let right = place("right_token");
    let instruction = raw.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.op,o::bodies::Op::CopyPlace(id) if id==left))
        .unwrap();
    // Independently construct this instruction's RFC 0011 bytes, then change
    // only its place operand to a live, equally typed token from another arena.
    let mut pattern = Vec::new();
    pattern.extend(
        u32::try_from(instruction.results.len())
            .unwrap()
            .to_le_bytes(),
    );
    for r in &instruction.results {
        pattern.extend(r.0.to_le_bytes());
    }
    pattern.push(8);
    let operand = pattern.len();
    pattern.extend(left.0.to_le_bytes());
    for word in [
        instruction.span.source.0,
        instruction.span.start,
        instruction.span.end,
    ] {
        pattern.extend(word.to_le_bytes());
    }
    let mut bytes = wire::encode(
        raw.clone(),
        Some(m::FunctionId(0)),
        wire::CodecLimits::default(),
    )
    .unwrap();
    let hits: Vec<_> = bytes
        .windows(pattern.len())
        .enumerate()
        .filter_map(|(i, b)| (b == pattern).then_some(i))
        .collect();
    assert_eq!(hits.len(), 1);
    let offset = hits[0] + operand;
    bytes[offset..offset + 4].copy_from_slice(&right.0.to_le_bytes());
    let error = wire::decode(&bytes, wire::CodecLimits::default())
        .err()
        .expect("embedded valid source cannot rescue a forged region");
    assert!(
        matches!(
            error.kind,
            wire::CodecErrorKind::Failure(o::failure::Error {
                kind: o::failure::ErrorKind::Regions(_),
                ..
            })
        ),
        "{error:?}"
    );
}
