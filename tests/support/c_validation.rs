use super::*;
use lang_runtime::c_abi::validation::{self as v, Address, Argument, Object, Reader, Storage};
use lang_runtime::execution::{Control, Failure, InterruptionFlag, ServiceWork};
use std::cell::RefCell;

const SOURCE: &str = r#"
pub struct Pair { flag: bool, value: i64 }
pub struct Empty {}
pub enum Choice { Empty, Value(Pair) }
pub enum Mixed { Small(i64), Wide(core::AllocError, bool) }
extern "C" fn inspect(value: Choice, flag: bool) -> core::Unit;
extern "C" fn mixed(value: Mixed) -> Mixed;
extern "C" fn pair(first: Pair, second: Pair) -> Pair;
extern "C" fn errors(value: core::AllocError) -> core::AllocError;
extern "C" fn empty(value: core::Unit, empty: Empty) -> Empty;
extern "C" fn scalar(value: i64) -> bool;
fn main() -> i64 { return 0; }
"#;
fn schema() -> Artifact {
    artifact(SOURCE, Target::MacosAarch64, Limits::default()).unwrap()
}
fn function(schema: &Artifact, name: &str) -> usize {
    schema
        .functions()
        .iter()
        .position(|f| f.name == name)
        .unwrap()
}
fn parameter(schema: &Artifact, name: &str) -> lang_runtime::c_abi::TypeId {
    schema.functions()[function(schema, name)].parameters[0]
}
fn context() -> Address {
    Address {
        start: 0x10000,
        extent: 256,
    }
}
struct Input {
    bytes: Vec<u8>,
    address: Address,
    forbidden: Vec<std::ops::Range<usize>>,
    reads: RefCell<Vec<std::ops::Range<usize>>>,
}
impl Input {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            address: Address {
                start: 0x1000,
                extent: bytes.len(),
            },
            bytes,
            forbidden: Vec::new(),
            reads: RefCell::default(),
        }
    }
}
impl Reader for Input {
    fn address(&self) -> Address {
        self.address
    }
    fn read(&self, offset: usize, destination: &mut [u8]) -> Result<(), v::Error> {
        let end = offset + destination.len();
        assert!(
            !self
                .forbidden
                .iter()
                .any(|r| r.start < end && offset < r.end),
            "ordinary padding requested: {offset}..{end}"
        );
        self.reads.borrow_mut().push(offset..end);
        let bytes = self.bytes.get(offset..end).ok_or(v::Error::Read {
            offset,
            bytes: destination.len(),
        })?;
        destination.copy_from_slice(bytes);
        Ok(())
    }
}
fn check(
    schema: &Artifact,
    id: lang_runtime::c_abi::TypeId,
    reader: &dyn Reader,
) -> Result<(), v::Error> {
    v::object(
        schema,
        id,
        reader,
        &ServiceWork::new(None),
        Control::default(),
    )
}

#[test]
fn c_fields_are_read_individually_without_touching_ordinary_padding() {
    let schema = schema();
    let pair = parameter(&schema, "pair");
    let mut bytes = vec![0xA5; 16];
    bytes[0] = 1;
    bytes[8..16].copy_from_slice(&(-17i64).to_le_bytes());
    let mut input = Input::new(bytes);
    input.forbidden.push(1..8);
    check(&schema, pair, &input).unwrap();
    assert_eq!(*input.reads.borrow(), [0..1, 8..16]);

    let choice = parameter(&schema, "inspect");
    let mut bytes = vec![0xB7; 24];
    bytes[..4].copy_from_slice(&1u32.to_le_bytes());
    bytes[8] = 1;
    bytes[16..24].copy_from_slice(&i64::MIN.to_le_bytes());
    let mut input = Input::new(bytes);
    input.forbidden = vec![4..8, 9..16];
    check(&schema, choice, &input).unwrap();
    assert_eq!(*input.reads.borrow(), [0..4, 8..9, 16..24]);
}

#[test]
fn enum_tag_zero_and_unused_union_bytes_have_distinct_canonical_rules() {
    let schema = schema();
    let choice = parameter(&schema, "inspect");
    let mut bytes = vec![0; 24];
    bytes[4..8].fill(0xDD);
    let mut input = Input::new(bytes);
    input.forbidden.push(4..8);
    check(&schema, choice, &input).unwrap();
    input.bytes[23] = 1;
    assert_eq!(
        check(&schema, choice, &input),
        Err(v::Error::UnusedUnion { offset: 23 })
    );
    input.bytes[..4].copy_from_slice(&99u32.to_le_bytes());
    assert_eq!(
        check(&schema, choice, &input),
        Err(v::Error::InvalidTag {
            offset: 0,
            value: 99
        })
    );
}

#[test]
fn union_extent_excludes_enum_tail_padding_and_includes_union_rounding() {
    let schema = schema();
    let error = parameter(&schema, "errors");
    let Kind::Enum {
        payload_size,
        payload_offset,
        ..
    } = &schema.types()[error.0].kind
    else {
        panic!()
    };
    assert_eq!(
        (*payload_offset, *payload_size, schema.types()[error.0].size),
        (4, 1, 8)
    );
    let mut input = Input::new(vec![0; 8]);
    input.bytes[5..8].fill(0xFF);
    input.forbidden.push(5..8);
    check(&schema, error, &input).unwrap();
    assert_eq!(*input.reads.borrow(), [0..4, 4..5]);

    let mixed = parameter(&schema, "mixed");
    let Kind::Enum {
        cases,
        payload_size,
        payload_offset,
        ..
    } = &schema.types()[mixed.0].kind
    else {
        panic!()
    };
    assert_eq!((*payload_offset, *payload_size), (8, 16));
    assert_eq!((cases[0].size, cases[1].size), (8, 12));
    let mut input = Input::new(vec![0; 24]);
    input.bytes[4..8].fill(0xA5);
    input.forbidden.push(4..8);
    input.bytes[8..16].copy_from_slice(&i64::MAX.to_le_bytes());
    check(&schema, mixed, &input).unwrap();
    input.bytes[23] = 1;
    assert_eq!(
        check(&schema, mixed, &input),
        Err(v::Error::UnusedUnion { offset: 23 })
    );
    input.bytes.fill(0);
    input.bytes[..4].copy_from_slice(&1u32.to_le_bytes());
    input.bytes[13..16].fill(0xCC);
    input.bytes[16] = 1;
    input.bytes[17..20].fill(0xDD);
    input.forbidden = vec![4..8, 13..16, 17..20];
    check(&schema, mixed, &input).unwrap();
    input.bytes[20] = 1;
    assert_eq!(
        check(&schema, mixed, &input),
        Err(v::Error::UnusedUnion { offset: 20 })
    );
}

#[test]
fn noncanonical_bool_unit_and_empty_record_are_rejected() {
    let schema = schema();
    let mut input = Input::new(vec![0; 16]);
    input.bytes[0] = 2;
    assert_eq!(
        check(&schema, parameter(&schema, "pair"), &input),
        Err(v::Error::InvalidBool {
            offset: 0,
            value: 2
        })
    );
    let empty = &schema.functions()[function(&schema, "empty")];
    for id in &empty.parameters {
        assert_eq!(
            check(&schema, *id, &Input::new(vec![1])),
            Err(v::Error::Reserved { offset: 0 })
        );
        check(&schema, *id, &Input::new(vec![0])).unwrap();
    }
}

#[test]
fn complete_address_preflight_precedes_any_value_read() {
    let schema = schema();
    let function = function(&schema, "pair");
    let first = Input::new(vec![0; 16]);
    for (address, expected) in [
        (
            Address {
                start: 0,
                extent: 16,
            },
            Storage::Null,
        ),
        (
            Address {
                start: 0x2001,
                extent: 16,
            },
            Storage::Misaligned { align: 8 },
        ),
        (
            Address {
                start: 0x2000,
                extent: 15,
            },
            Storage::Short {
                required: 16,
                actual: 15,
            },
        ),
        (
            Address {
                start: usize::MAX - 7,
                extent: 16,
            },
            Storage::Overflow,
        ),
    ] {
        let mut second = Input::new(vec![0; 16]);
        second.address = address;
        let result = v::call(
            &schema,
            function,
            context(),
            &[Argument::Object(&first), Argument::Object(&second)],
            Some(Address {
                start: 0x8000,
                extent: 16,
            }),
            &ServiceWork::new(None),
            Control::default(),
        );
        assert_eq!(
            result,
            Err(v::Error::Storage {
                object: Object::Argument(1),
                kind: expected
            })
        );
        assert!(first.reads.borrow().is_empty());
        assert!(second.reads.borrow().is_empty());
    }
}

#[test]
fn output_aliases_fail_before_reads_while_const_input_aliases_are_valid() {
    let schema = schema();
    let function = function(&schema, "pair");
    let input = Input::new(vec![0; 16]);
    for (output, other) in [
        (input.address, Object::Argument(0)),
        (
            Address {
                start: 0x10008,
                extent: 16,
            },
            Object::Context,
        ),
    ] {
        assert_eq!(
            v::call(
                &schema,
                function,
                context(),
                &[Argument::Object(&input), Argument::Object(&input)],
                Some(output),
                &ServiceWork::new(None),
                Control::default()
            ),
            Err(v::Error::OutputOverlap(other))
        );
        assert!(input.reads.borrow().is_empty());
    }
    v::call(
        &schema,
        function,
        context(),
        &[Argument::Object(&input), Argument::Object(&input)],
        Some(Address {
            start: 0x8000,
            extent: 16,
        }),
        &ServiceWork::new(None),
        Control::default(),
    )
    .unwrap();
    assert_eq!(input.reads.borrow().len(), 4);
}

#[test]
fn scalar_shapes_unit_results_and_output_storage_are_checked() {
    let schema = schema();
    let scalar = function(&schema, "scalar");
    v::call(
        &schema,
        scalar,
        context(),
        &[Argument::I64(i64::MIN)],
        Some(Address {
            start: 0x8000,
            extent: 1,
        }),
        &ServiceWork::new(None),
        Control::default(),
    )
    .unwrap();
    assert_eq!(
        v::call(
            &schema,
            scalar,
            context(),
            &[Argument::Bool(1)],
            Some(Address {
                start: 0x8000,
                extent: 1
            }),
            &ServiceWork::new(None),
            Control::default()
        ),
        Err(v::Error::Shape(Object::Argument(0)))
    );
    assert_eq!(
        v::call(
            &schema,
            scalar,
            context(),
            &[Argument::I64(0)],
            None,
            &ServiceWork::new(None),
            Control::default()
        ),
        Err(v::Error::Shape(Object::Result))
    );
    let inspect = function(&schema, "inspect");
    let input = Input::new(vec![0; 24]);
    v::call(
        &schema,
        inspect,
        context(),
        &[Argument::Object(&input), Argument::Bool(1)],
        None,
        &ServiceWork::new(None),
        Control::default(),
    )
    .unwrap();
    assert_eq!(
        v::call(
            &schema,
            inspect,
            context(),
            &[Argument::Object(&input), Argument::Bool(2)],
            None,
            &ServiceWork::new(None),
            Control::default()
        ),
        Err(v::Error::InvalidBool {
            offset: 0,
            value: 2
        })
    );
    assert_eq!(
        v::call(
            &schema,
            inspect,
            context(),
            &[],
            None,
            &ServiceWork::new(None),
            Control::default()
        ),
        Err(v::Error::Arity {
            expected: 2,
            actual: 0
        })
    );
}

#[test]
fn shared_work_limits_and_interruption_refuse_before_reading_unbudgeted_bytes() {
    let schema = schema();
    let id = parameter(&schema, "pair");
    let input = Input::new(vec![0; 16]);
    let work = ServiceWork::new(None);
    let same = work.clone();
    v::object(&schema, id, &input, &work, Control::default()).unwrap();
    let used = same.used();
    assert!(used > 0);
    let exact = ServiceWork::new(Some(used));
    v::object(&schema, id, &input, &exact, Control::default()).unwrap();
    let short = ServiceWork::new(Some(used - 1));
    let error = v::object(&schema, id, &input, &short, Control::default()).unwrap_err();
    assert!(matches!(
        error,
        v::Error::Control(Failure::ServiceWorkLimit { .. })
    ));
    assert!(lang_runtime::execution::service_failure(&error).is_some());
    input.reads.borrow_mut().clear();
    assert!(
        v::object(
            &schema,
            id,
            &input,
            &ServiceWork::new(Some(0)),
            Control::default()
        )
        .is_err()
    );
    assert!(input.reads.borrow().is_empty());
    let interrupted = InterruptionFlag::new();
    interrupted.interrupt();
    assert_eq!(
        v::object(
            &schema,
            id,
            &input,
            &ServiceWork::new(None),
            Control::default().and_interruption(&interrupted)
        ),
        Err(v::Error::Control(Failure::Interrupted))
    );
    assert!(input.reads.borrow().is_empty());
}

#[test]
fn output_and_schema_refusals_never_request_input_bytes() {
    let schema = schema();
    let input = Input::new(vec![0; 16]);
    let pair = function(&schema, "pair");
    for (out, kind) in [
        (
            Address {
                start: 0,
                extent: 16,
            },
            Storage::Null,
        ),
        (
            Address {
                start: 0x8001,
                extent: 16,
            },
            Storage::Misaligned { align: 8 },
        ),
        (
            Address {
                start: 0x8000,
                extent: 15,
            },
            Storage::Short {
                required: 16,
                actual: 15,
            },
        ),
        (
            Address {
                start: usize::MAX - 7,
                extent: 16,
            },
            Storage::Overflow,
        ),
    ] {
        assert_eq!(
            v::call(
                &schema,
                pair,
                context(),
                &[Argument::Object(&input), Argument::Object(&input)],
                Some(out),
                &ServiceWork::new(None),
                Control::default()
            ),
            Err(v::Error::Storage {
                object: Object::Result,
                kind
            })
        );
        assert!(input.reads.borrow().is_empty());
    }
    assert_eq!(
        v::call(
            &schema,
            usize::MAX,
            context(),
            &[],
            None,
            &ServiceWork::new(None),
            Control::default()
        ),
        Err(v::Error::Function(usize::MAX))
    );
    assert_eq!(
        check(&schema, lang_runtime::c_abi::TypeId(usize::MAX), &input),
        Err(v::Error::Type(lang_runtime::c_abi::TypeId(usize::MAX)))
    );
    let inspect = function(&schema, "inspect");
    assert_eq!(
        v::call(
            &schema,
            inspect,
            context(),
            &[Argument::Object(&input), Argument::Bool(0)],
            Some(Address {
                start: 0x8000,
                extent: 1
            }),
            &ServiceWork::new(None),
            Control::default()
        ),
        Err(v::Error::Shape(Object::Result))
    );
    assert!(input.reads.borrow().is_empty());
}

#[test]
fn interrupted_final_read_and_reader_failure_are_reported_before_publication() {
    struct Interrupt<'a> {
        bytes: Input,
        flag: &'a InterruptionFlag,
    }
    impl Reader for Interrupt<'_> {
        fn address(&self) -> Address {
            self.bytes.address()
        }
        fn read(&self, offset: usize, out: &mut [u8]) -> Result<(), v::Error> {
            self.bytes.read(offset, out)?;
            self.flag.interrupt();
            Ok(())
        }
    }
    struct Unreadable;
    impl Reader for Unreadable {
        fn address(&self) -> Address {
            Address {
                start: 0x1000,
                extent: 1,
            }
        }
        fn read(&self, offset: usize, out: &mut [u8]) -> Result<(), v::Error> {
            Err(v::Error::Read {
                offset,
                bytes: out.len(),
            })
        }
    }
    let schema = schema();
    let unit = parameter(&schema, "empty");
    let flag = InterruptionFlag::new();
    let reader = Interrupt {
        bytes: Input::new(vec![0]),
        flag: &flag,
    };
    assert_eq!(
        v::object(
            &schema,
            unit,
            &reader,
            &ServiceWork::new(None),
            Control::default().and_interruption(&flag)
        ),
        Err(v::Error::Control(Failure::Interrupted))
    );
    assert_eq!(
        check(&schema, unit, &Unreadable),
        Err(v::Error::Read {
            offset: 0,
            bytes: 1
        })
    );
    assert_eq!(
        v::object(
            &schema,
            unit,
            &Input::new(vec![0]),
            &ServiceWork::new(None),
            Control::default().and_deadline(std::time::Instant::now())
        ),
        Err(v::Error::Control(Failure::DeadlineExceeded))
    );
}
