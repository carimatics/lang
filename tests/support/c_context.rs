use super::{Artifact, Direction, Limits, Target, artifact};
use lang_mir::unit::{SourceId, Span};
use lang_runtime::c_abi::context::{
    Call, EntryPoint, ErrorSummary, Fault, Kind, Program, Status,
    config::{
        self, Allocator, Binding, Capacities, Config, Descriptor, InvalidConfig, LimitsDescriptor,
    },
};
use lang_runtime::c_abi::validation::{Address, Argument, Error as ValueError, Reader};
use lang_runtime::{cleanup, execution};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

const SOURCE: &str = r#"
extern "C" fn increment(value: i64) -> i64;
extern "C" fn flag(value: bool) -> bool;
pub extern "C" fn echo(value: bool) -> bool { return value; }
fn main() -> i64 { return 0; }
"#;
static ALLOCATOR_CALLS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn allocate(_: *mut c_void, _: u64, _: u64) -> *mut c_void {
    ALLOCATOR_CALLS.fetch_add(1, Ordering::Relaxed);
    std::ptr::null_mut()
}
unsafe extern "C" fn deallocate(_: *mut c_void, _: *mut c_void, _: u64, _: u64) {
    ALLOCATOR_CALLS.fetch_add(1, Ordering::Relaxed);
}
unsafe extern "C" fn erased() {}
fn allocator() -> Allocator {
    Allocator {
        version: 1,
        size: 32,
        user: std::ptr::null_mut(),
        allocate: Some(allocate),
        deallocate: Some(deallocate),
    }
}
fn fixture() -> Artifact {
    artifact(SOURCE, Target::MacosAarch64, Limits::default()).unwrap()
}
fn capacities() -> Capacities {
    Capacities::from_strides(64, 16, 64).unwrap()
}
fn bindings(artifact: &Artifact) -> Vec<Binding<'_>> {
    artifact
        .functions()
        .iter()
        .filter(|f| f.direction == Direction::Import)
        .map(|f| Binding {
            version: 1,
            size: 80,
            name: Some(f.name.as_bytes()),
            name_bytes: f.name.len() as u64,
            signature: f.digest,
            function: Some(erased),
            user: std::ptr::null_mut(),
            flags: config::NATIVE_FFI,
        })
        .collect()
}
fn config<'a>(
    artifact: &Artifact,
    allocator: &'a Allocator,
    limits: &'a LimitsDescriptor,
    bindings: &'a [Binding<'a>],
) -> Config<'a> {
    Config {
        version: 1,
        size: 80,
        allocator: Some(allocator),
        limits: Some(limits),
        imports: Some(bindings),
        import_count: bindings.len() as u64,
        artifact: artifact.digest(),
        flags: 0,
    }
}
fn index(artifact: &Artifact, name: &str) -> usize {
    artifact
        .functions()
        .iter()
        .position(|f| f.name.rsplit("::").next() == Some(name))
        .unwrap()
}
fn control() -> execution::Control<'static> {
    execution::Control::with_limits(execution::Limits::unbounded())
}
fn context_address() -> Address {
    Address {
        start: 0x1000,
        extent: 64,
    }
}

// Logical address maps let these safe fixtures detect reads/effects without
// dereferencing raw C storage; native address validity belongs to F04/F05.
struct Storage {
    bytes: RefCell<Vec<u8>>,
    reads: Cell<usize>,
    panic_read: Cell<bool>,
}
impl Storage {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: RefCell::new(bytes),
            reads: Cell::new(0),
            panic_read: Cell::new(false),
        }
    }
}
impl Reader for Storage {
    fn address(&self) -> Address {
        Address {
            start: 0x4000,
            extent: self.bytes.borrow().len(),
        }
    }
    fn read(&self, offset: usize, out: &mut [u8]) -> Result<(), ValueError> {
        self.reads.set(self.reads.get() + 1);
        assert!(!self.panic_read.get(), "injected reader panic");
        let bytes = self.bytes.borrow();
        let data = bytes
            .get(offset..offset + out.len())
            .ok_or(ValueError::Read {
                offset,
                bytes: out.len(),
            })?;
        out.copy_from_slice(data);
        Ok(())
    }
}

#[test]
fn complete_config_rejects_bad_headers_pointers_counts_and_digests_before_callbacks() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    let base = config(&artifact, &allocator, &limits, &imports);
    let mut cases = Vec::new();
    let mut c = base;
    c.version = 2;
    cases.push((c, InvalidConfig::Header(Descriptor::Config)));
    let mut c = base;
    c.size = 81;
    cases.push((c, InvalidConfig::Header(Descriptor::Config)));
    let mut c = base;
    c.flags = 1;
    cases.push((c, InvalidConfig::Flags(Descriptor::Config)));
    let mut c = base;
    c.allocator = None;
    cases.push((c, InvalidConfig::Missing(Descriptor::Allocator)));
    let mut c = base;
    c.limits = None;
    cases.push((c, InvalidConfig::Missing(Descriptor::Limits)));
    let mut c = base;
    c.imports = None;
    cases.push((c, InvalidConfig::ImportCount));
    let mut c = base;
    c.import_count = 4097;
    cases.push((c, InvalidConfig::ImportCount));
    let mut c = base;
    c.import_count -= 1;
    cases.push((c, InvalidConfig::ImportCount));
    let mut c = base;
    c.artifact.0[0] ^= 1;
    cases.push((c, InvalidConfig::Artifact));
    for (config, expected) in cases {
        assert_eq!(
            program.create(config).err().unwrap().kind,
            Kind::Config(expected)
        );
    }
    let mut bad_allocator = allocator;
    bad_allocator.allocate = None;
    assert_eq!(
        program
            .create(Config {
                allocator: Some(&bad_allocator),
                ..base
            })
            .err()
            .unwrap()
            .kind,
        Kind::Config(InvalidConfig::Missing(Descriptor::Allocator))
    );
    let mut bad_limits = limits;
    bad_limits.reserved = 1;
    assert_eq!(
        program
            .create(Config {
                limits: Some(&bad_limits),
                ..base
            })
            .err()
            .unwrap()
            .kind,
        Kind::Config(InvalidConfig::Flags(Descriptor::Limits))
    );
    assert_eq!(ALLOCATOR_CALLS.load(Ordering::Relaxed), 0);
}

#[test]
fn all_declared_imports_need_exact_names_signatures_nonnull_code_and_explicit_grants() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let original = bindings(&artifact);
    for (change, expected) in [
        (0, InvalidConfig::Name(0)),
        (1, InvalidConfig::Name(0)),
        (2, InvalidConfig::Name(0)),
        (3, InvalidConfig::Name(0)),
        (4, InvalidConfig::Function(0)),
        (5, InvalidConfig::Flags(Descriptor::Binding(0))),
        (6, InvalidConfig::Signature(0)),
        (7, InvalidConfig::Grant(0)),
        (8, InvalidConfig::Unknown(0)),
        (9, InvalidConfig::Header(Descriptor::Binding(0))),
    ] {
        let mut imports = original.clone();
        match change {
            0 => imports[0].name = None,
            1 => {
                imports[0].name = Some(b"");
                imports[0].name_bytes = 0;
            }
            2 => {
                imports[0].name = Some(b"bad\0name");
                imports[0].name_bytes = 8;
            }
            3 => {
                imports[0].name = Some(b"\xff");
                imports[0].name_bytes = 1;
            }
            4 => imports[0].function = None,
            5 => imports[0].flags = 3,
            6 => imports[0].signature.0[0] ^= 1,
            7 => imports[0].flags = 0,
            8 => {
                imports[0].name = Some(b"other");
                imports[0].name_bytes = 5;
            }
            9 => imports[0].size = 0,
            _ => unreachable!(),
        }
        let fault = program
            .create(config(&artifact, &allocator, &limits, &imports))
            .err()
            .unwrap();
        assert_eq!(fault.kind, Kind::Config(expected));
        assert_eq!(
            fault.status(),
            if change == 7 {
                Status::Capability
            } else {
                Status::Invalid
            }
        );
    }
    let duplicate = [original[0], original[0]];
    assert!(matches!(
        program
            .create(config(&artifact, &allocator, &limits, &duplicate))
            .err()
            .unwrap()
            .kind,
        Kind::Config(InvalidConfig::Duplicate(_))
    ));
    assert!(matches!(
        program
            .create(config(&artifact, &allocator, &limits, &original[..1]))
            .err()
            .unwrap()
            .kind,
        Kind::Config(InvalidConfig::MissingImport(_))
    ));
    let name = vec![b'a'; config::MAX_NAME_BYTES + 1];
    let mut huge = original.clone();
    huge[0].name = Some(&name);
    huge[0].name_bytes = name.len() as u64;
    assert_eq!(
        program
            .create(config(&artifact, &allocator, &limits, &huge))
            .err()
            .unwrap()
            .kind,
        Kind::Config(InvalidConfig::NameBytes)
    );
    assert_eq!(ALLOCATOR_CALLS.load(Ordering::Relaxed), 0);
}

#[test]
fn checked_limits_preserve_zero_and_only_fuel_and_host_calls_allow_unbounded() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(
        &artifact,
        &owner,
        capacities().bounded(256, 1_000_000, 65_536),
    )
    .unwrap();
    let allocator = allocator();
    let imports = bindings(&artifact);
    let zero = LimitsDescriptor {
        live_bytes: 0,
        cumulative_bytes: 0,
        live_allocations: 0,
        frames: 0,
        cells: 0,
        fuel: 0,
        host_calls: 0,
        cleanup_records: 0,
        ..LimitsDescriptor::default()
    };
    let context = program
        .create(config(&artifact, &allocator, &zero, &imports))
        .unwrap();
    assert_eq!(context.limits().execution.fuel, Some(0));
    assert_eq!(context.limits().allocation.live_allocations, 0);
    let mut usage = execution::Usage::default();
    let fault = context
        .run(
            &program,
            control(),
            &execution::ServiceWork::new(None),
            &mut usage,
            EntryPoint::Guest,
            &mut |_| panic!("zero frame entry"),
        )
        .unwrap_err();
    assert_eq!(
        fault.kind,
        Kind::Execution(cleanup::ExecutionFailure::FrameLimit)
    );
    let unbounded = LimitsDescriptor {
        fuel: u64::MAX,
        host_calls: u64::MAX,
        ..LimitsDescriptor::default()
    };
    let context = program
        .create(config(&artifact, &allocator, &unbounded, &imports))
        .unwrap();
    assert_eq!(context.limits().execution.fuel, None);
    assert_eq!(context.limits().execution.host_calls, None);
    for which in 0..4 {
        let mut limits = unbounded;
        match which {
            0 => limits.frames = u64::MAX,
            1 => limits.cells = u64::MAX,
            2 => limits.live_allocations = u64::MAX,
            _ => limits.cleanup_records = 4097,
        }
        assert!(matches!(
            program
                .create(config(&artifact, &allocator, &limits, &imports))
                .err()
                .unwrap()
                .kind,
            Kind::Config(InvalidConfig::Limit { .. })
        ));
    }
    assert!(Capacities::from_strides(0, 16, 64).is_err());
}

#[test]
fn context_owns_descriptor_copies_and_distinguishes_same_abi_code_and_allocator_instances() {
    let artifact = fixture();
    let owner = ();
    let other_owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let other = Program::new(&artifact, &other_owner, capacities()).unwrap();
    let mut allocator = allocator();
    let mut limits = LimitsDescriptor::default();
    let mut names: Vec<_> = bindings(&artifact)
        .iter()
        .map(|b| b.name.unwrap().to_vec())
        .collect();
    let mut imports = bindings(&artifact);
    for (binding, name) in imports.iter_mut().zip(&names) {
        binding.name = Some(name);
    }
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let second = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    assert_ne!(context.allocator_identity(), second.allocator_identity());
    drop(imports);
    for name in &mut names {
        name.fill(b'x');
    }
    allocator.allocate = None;
    limits.host_calls = 0;
    assert!(allocator.allocate.is_none());
    assert_eq!(limits.host_calls, 0);
    assert!(context.allocator().allocate.is_some());
    assert_eq!(context.limits().execution.host_calls, Some(1000));
    let mut usage = execution::Usage::default();
    let work = execution::ServiceWork::new(None);
    assert_eq!(
        context
            .run(
                &other,
                control(),
                &work,
                &mut usage,
                EntryPoint::Guest,
                &mut |_| panic!("wrong code owner")
            )
            .unwrap_err()
            .kind,
        Kind::WrongProgram
    );
    context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Guest,
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(context.last_error().unwrap(), ErrorSummary::success());
}

#[test]
fn imports_preflight_values_before_effects_and_validate_successful_results() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let function = index(&artifact, "flag");
    let output = Storage::new(vec![0]);
    let effects = Cell::new(0);
    let work = execution::ServiceWork::new(None);
    let mut usage = execution::Usage::default();
    let fault = context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Guest,
            &mut |entry| {
                let (_, session) = entry.parts();
                session.invoke(
                    function,
                    Call {
                        span: None,
                        context: context_address(),
                        arguments: &[Argument::Bool(2)],
                        output: Some(output.address()),
                    },
                    Some(&output),
                    &mut |_| {
                        effects.set(effects.get() + 1);
                        0
                    },
                )
            },
        )
        .unwrap_err();
    assert!(matches!(
        fault.kind,
        Kind::Value(ValueError::InvalidBool { .. })
    ));
    assert_eq!(effects.get(), 0);
    assert_eq!(usage.host_calls, 0);
    assert_eq!(output.reads.get(), 0);
    let fault = context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Guest,
            &mut |entry| {
                let (_, session) = entry.parts();
                session.invoke(
                    function,
                    Call {
                        span: None,
                        context: context_address(),
                        arguments: &[Argument::Bool(1)],
                        output: Some(output.address()),
                    },
                    Some(&output),
                    &mut |binding| {
                        assert!(binding.name().ends_with("flag"));
                        assert_eq!(binding.signature(), artifact.functions()[function].digest);
                        output.bytes.borrow_mut()[0] = 2;
                        effects.set(effects.get() + 1);
                        0
                    },
                )
            },
        )
        .unwrap_err();
    assert!(matches!(
        fault.kind,
        Kind::ForeignValue(ValueError::InvalidBool { value: 2, .. })
    ));
    assert_eq!(fault.status(), Status::Runtime);
    assert_eq!(usage.host_calls, 1);
    assert_eq!(effects.get(), 1);
}

#[test]
fn foreign_status_is_structured_latched_and_never_a_guest_panic_or_enum_tag() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let function = index(&artifact, "increment");
    let output = Storage::new(vec![0; 8]);
    let effects = Cell::new(0);
    for status in [1, 2, 3, 4, 5, 6, 7, u32::MAX] {
        let mut usage = execution::Usage::default();
        let work = execution::ServiceWork::new(None);
        let fault = context
            .run(
                &program,
                control(),
                &work,
                &mut usage,
                EntryPoint::Guest,
                &mut |entry| {
                    let (_, session) = entry.parts();
                    let call = Call {
                        span: None,
                        context: context_address(),
                        arguments: &[Argument::I64(9)],
                        output: Some(output.address()),
                    };
                    let first = session
                        .invoke(function, call, Some(&output), &mut |_| {
                            effects.set(effects.get() + 1);
                            status
                        })
                        .unwrap_err();
                    let again = session
                        .invoke(function, call, Some(&output), &mut |_| {
                            panic!("latched callback must not run")
                        })
                        .unwrap_err();
                    assert_eq!(first, again);
                    Ok(()) // Deliberately ignore backend error.
                },
            )
            .unwrap_err();
        assert_eq!(
            fault.kind,
            Kind::ForeignStatus {
                code: status,
                known: status <= 6
            }
        );
        assert_eq!(fault.status(), Status::Runtime);
        assert_eq!(fault.panic_code, 0);
        assert_eq!(usage.host_calls, 1);
        assert!(!context.is_poisoned().unwrap());
    }
    assert_eq!(effects.get(), 8);
    assert_eq!(output.reads.get(), 0);
}

#[test]
fn reentry_is_rejected_before_mutable_state_and_preserves_the_outer_call() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let output = Storage::new(vec![0; 8]);
    let work = execution::ServiceWork::new(None);
    let mut usage = execution::Usage::default();
    context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Guest,
            &mut |entry| {
                let (_, session) = entry.parts();
                session.invoke(
                    index(&artifact, "increment"),
                    Call {
                        span: None,
                        context: context_address(),
                        arguments: &[Argument::I64(3)],
                        output: Some(output.address()),
                    },
                    Some(&output),
                    &mut |_| {
                        let mut nested = execution::Usage::default();
                        assert_eq!(
                            context
                                .run(
                                    &program,
                                    control(),
                                    &work,
                                    &mut nested,
                                    EntryPoint::Guest,
                                    &mut |_| panic!("reentered")
                                )
                                .unwrap_err()
                                .kind,
                            Kind::Reentry
                        );
                        assert_eq!(context.last_error().unwrap_err().kind, Kind::Reentry);
                        assert_eq!(
                            context
                                .close(&mut || panic!("nested destroy"))
                                .unwrap_err()
                                .kind,
                            Kind::Reentry
                        );
                        0
                    },
                )
            },
        )
        .unwrap();
    assert_eq!(usage.host_calls, 1);
    assert_eq!(context.last_error().unwrap(), ErrorSummary::success());
}

#[test]
fn contained_callback_and_payload_drop_panics_poison_and_preserve_primary_summary() {
    struct BadDrop;
    impl Drop for BadDrop {
        fn drop(&mut self) {
            panic!("panic payload destructor");
        }
    }
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    for bad_payload in [false, true] {
        let context = program
            .create(config(&artifact, &allocator, &limits, &imports))
            .unwrap();
        let output = Storage::new(vec![0; 8]);
        let work = execution::ServiceWork::new(None);
        let mut usage = execution::Usage::default();
        let fault = context
            .run(
                &program,
                control(),
                &work,
                &mut usage,
                EntryPoint::Guest,
                &mut |entry| {
                    let (_, session) = entry.parts();
                    session.invoke(
                        index(&artifact, "increment"),
                        Call {
                            span: None,
                            context: context_address(),
                            arguments: &[Argument::I64(0)],
                            output: Some(output.address()),
                        },
                        Some(&output),
                        &mut |_| {
                            if bad_payload {
                                std::panic::panic_any(BadDrop);
                            }
                            panic!("host callback");
                        },
                    )
                },
            )
            .unwrap_err();
        assert_eq!(fault.kind, Kind::HostPanic);
        assert_eq!(fault.cleanup_count, u32::from(bad_payload));
        assert_eq!(fault.quarantined, usize::from(bad_payload));
        let summary = context.last_error().unwrap();
        assert_eq!(summary.status, 3);
        assert!(context.is_poisoned().unwrap());
        assert_eq!(
            context
                .run(
                    &program,
                    control(),
                    &work,
                    &mut usage,
                    EntryPoint::Guest,
                    &mut |_| panic!("poisoned entry")
                )
                .unwrap_err()
                .kind,
            Kind::Poisoned
        );
        assert_eq!(context.last_error().unwrap(), summary);
    }
}

#[test]
fn cleanup_report_counts_truncated_failures_and_keeps_guest_panic_primary() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor {
        cleanup_records: 1,
        ..LimitsDescriptor::default()
    };
    let imports = bindings(&artifact);
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let span = Span {
        source: SourceId(0),
        start: 5,
        end: 12,
    };
    let mut usage = execution::Usage::default();
    let fault = context
        .run(
            &program,
            control(),
            &execution::ServiceWork::new(None),
            &mut usage,
            EntryPoint::Guest,
            &mut |entry| {
                let records = entry.cleanup_records();
                let (instance, _) = entry.parts();
                let mut frame = instance
                    .enter(
                        cleanup::Limits {
                            slots: 3,
                            scopes: 1,
                            failure_records: records,
                        },
                        |_: &mut u32, _| Err(7),
                    )
                    .unwrap();
                for id in 0..3 {
                    frame
                        .declare(cleanup::Slot(id), cleanup::Kind::Owner)
                        .unwrap();
                    frame
                        .initialize(cleanup::Slot(id), id as u32, Some(span))
                        .unwrap();
                }
                let completion = frame.finish(cleanup::Exit::Failure {
                    failure: cleanup::ExecutionFailure::Panic,
                    span: Some(span),
                });
                assert_eq!(completion.report.cleanup_failures(), 3);
                assert_eq!(completion.report.truncated(), 3);
                Err(Fault::from_report(&completion.report, 42).unwrap())
            },
        )
        .unwrap_err();
    assert_eq!(fault.status(), Status::GuestPanic);
    assert_eq!(fault.cleanup_count, 3);
    assert!(fault.truncated);
    assert_eq!(
        context.last_error().unwrap(),
        ErrorSummary {
            panic_code: 42,
            status: 2,
            source: 0,
            start: 5,
            end: 12,
            cleanup_count: 3,
            flags: 1
        }
    );
    assert!(context.is_poisoned().unwrap());
}

#[test]
fn ordinary_guest_panic_is_reusable_and_success_clears_last_error() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let work = execution::ServiceWork::new(None);
    let mut usage = execution::Usage::default();
    context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Guest,
            &mut |_| Err(Fault::guest_panic(-9, None)),
        )
        .unwrap_err();
    assert_eq!(context.last_error().unwrap().panic_code, -9);
    assert!(!context.is_poisoned().unwrap());
    context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Guest,
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(context.last_error().unwrap(), ErrorSummary::success());
    context
        .close(&mut || Err(Fault::new(Kind::Cleanup)))
        .unwrap_err();
    assert_eq!(context.last_error().unwrap_err().kind, Kind::Closed);
    assert_eq!(
        context
            .run(
                &program,
                control(),
                &work,
                &mut usage,
                EntryPoint::Guest,
                &mut |_| panic!("closed context")
            )
            .unwrap_err()
            .kind,
        Kind::Closed
    );
}

#[test]
fn export_checks_before_body_and_validates_staged_result_after_success() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let result = Storage::new(vec![2]);
    let work = execution::ServiceWork::new(None);
    let mut usage = execution::Usage::default();
    let effects = Cell::new(0);
    let export = index(&artifact, "echo");
    let output = Address {
        start: 0x8000,
        extent: 1,
    };
    for bad in [true, false] {
        let args = [Argument::Bool(if bad { 2 } else { 1 })];
        let fault = context
            .run(
                &program,
                control(),
                &work,
                &mut usage,
                EntryPoint::Export {
                    function: export,
                    call: Call {
                        span: None,
                        context: context_address(),
                        arguments: &args,
                        output: Some(output),
                    },
                    result: Some(&result),
                },
                &mut |_| {
                    effects.set(effects.get() + 1);
                    Ok(())
                },
            )
            .unwrap_err();
        assert!(matches!(
            fault.kind,
            Kind::Value(ValueError::InvalidBool { .. })
        ));
        assert_eq!(effects.get(), usize::from(!bad));
        assert_eq!(result.reads.get(), usize::from(!bad));
    }
    result.bytes.borrow_mut()[0] = 1;
    context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Export {
                function: export,
                call: Call {
                    span: None,
                    context: context_address(),
                    arguments: &[Argument::Bool(1)],
                    output: Some(output),
                },
                result: Some(&result),
            },
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(context.last_error().unwrap(), ErrorSummary::success());
}

#[test]
fn context_bounds_keep_polling_and_shared_work_and_stop_callbacks_before_effects() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor {
        host_calls: 0,
        ..LimitsDescriptor::default()
    };
    let imports = bindings(&artifact);
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let result = Storage::new(vec![0; 8]);
    let mut usage = execution::Usage::default();
    let work = execution::ServiceWork::new(None);
    let fault = context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Guest,
            &mut |entry| {
                let (_, session) = entry.parts();
                assert_eq!(session.control().limits().host_calls, Some(0));
                session.invoke(
                    index(&artifact, "increment"),
                    Call {
                        span: None,
                        context: context_address(),
                        arguments: &[Argument::I64(1)],
                        output: Some(result.address()),
                    },
                    Some(&result),
                    &mut |_| panic!("zero host budget"),
                )
            },
        )
        .unwrap_err();
    assert_eq!(fault.kind, Kind::Control(execution::Failure::HostCallLimit));
    assert_eq!(usage.host_calls, 0);
    let flag = execution::InterruptionFlag::new();
    flag.interrupt();
    assert_eq!(
        context
            .run(
                &program,
                control().and_interruption(&flag),
                &work,
                &mut usage,
                EntryPoint::Guest,
                &mut |_| panic!("interrupted")
            )
            .unwrap_err()
            .kind,
        Kind::Control(execution::Failure::Interrupted)
    );
    let work = execution::ServiceWork::new(Some(0));
    let fault = context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Guest,
            &mut |entry| {
                let (_, session) = entry.parts();
                session.invoke(
                    index(&artifact, "increment"),
                    Call {
                        span: None,
                        context: context_address(),
                        arguments: &[Argument::I64(1)],
                        output: Some(result.address()),
                    },
                    Some(&result),
                    &mut |_| panic!("no work"),
                )
            },
        )
        .unwrap_err();
    assert!(matches!(
        fault.kind,
        Kind::Value(ValueError::Control(
            execution::Failure::ServiceWorkLimit { .. }
        ))
    ));
    assert_eq!(work.used(), 0);
    assert_eq!(usage.fuel, 0);
}

#[test]
fn body_and_post_result_reader_panics_are_contained_and_poison() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    for result_panic in [false, true] {
        let context = program
            .create(config(&artifact, &allocator, &limits, &imports))
            .unwrap();
        let result = Storage::new(vec![0]);
        result.panic_read.set(true);
        let work = execution::ServiceWork::new(None);
        let mut usage = execution::Usage::default();
        let point = if result_panic {
            EntryPoint::Export {
                function: index(&artifact, "echo"),
                call: Call {
                    span: None,
                    context: context_address(),
                    arguments: &[Argument::Bool(1)],
                    output: Some(Address {
                        start: 0x8000,
                        extent: 1,
                    }),
                },
                result: Some(&result),
            }
        } else {
            EntryPoint::Guest
        };
        let fault = context
            .run(&program, control(), &work, &mut usage, point, &mut |_| {
                if !result_panic {
                    panic!("host body");
                }
                Ok(())
            })
            .unwrap_err();
        assert_eq!(fault.kind, Kind::HostPanic);
        assert!(context.is_poisoned().unwrap());
    }
}

#[test]
fn foreign_failure_retains_callsite_primary_when_ordinary_cleanup_also_fails() {
    let artifact = fixture();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let work = execution::ServiceWork::new(None);
    let mut usage = execution::Usage::default();
    let result = Storage::new(vec![0; 8]);
    let span = Span {
        source: SourceId(0),
        start: 40,
        end: 55,
    };
    let fault = context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Guest,
            &mut |entry| {
                let (instance, session) = entry.parts();
                let mut frame = instance
                    .enter(
                        cleanup::Limits {
                            slots: 1,
                            scopes: 1,
                            failure_records: 1,
                        },
                        |_: &mut u8, _| Err(99),
                    )
                    .unwrap();
                frame
                    .declare(cleanup::Slot(0), cleanup::Kind::Owner)
                    .unwrap();
                frame.initialize(cleanup::Slot(0), 7, Some(span)).unwrap();
                session
                    .invoke(
                        index(&artifact, "increment"),
                        Call {
                            span: Some(span),
                            context: context_address(),
                            arguments: &[Argument::I64(0)],
                            output: Some(result.address()),
                        },
                        Some(&result),
                        &mut |_| 2,
                    )
                    .unwrap_err();
                let completion = frame.finish(cleanup::Exit::Failure {
                    failure: cleanup::ExecutionFailure::HostCall,
                    span: Some(span),
                });
                Err(Fault::from_report(&completion.report, 0).unwrap())
            },
        )
        .unwrap_err();
    assert_eq!(
        fault.kind,
        Kind::ForeignStatus {
            code: 2,
            known: true
        }
    );
    assert_eq!(fault.span, Some(span));
    assert_eq!(fault.cleanup_count, 1);
    assert_eq!(fault.status(), Status::Runtime);
    assert_eq!(
        context.last_error().unwrap(),
        ErrorSummary {
            panic_code: 0,
            status: 3,
            source: 0,
            start: 40,
            end: 55,
            cleanup_count: 1,
            flags: 1
        }
    );
    assert!(context.is_poisoned().unwrap());
}

#[test]
fn unit_callbacks_and_empty_import_config_need_no_result_pointer() {
    let artifact = artifact(r#"extern "C" fn notify() -> core::Unit; pub extern "C" fn ping() -> core::Unit { return core::Unit {}; }"#, Target::MacosAarch64, Limits::default()).unwrap();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let limits = LimitsDescriptor::default();
    let imports = bindings(&artifact);
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    let work = execution::ServiceWork::new(None);
    let mut usage = execution::Usage::default();
    context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Export {
                function: index(&artifact, "ping"),
                call: Call {
                    span: None,
                    context: context_address(),
                    arguments: &[],
                    output: None,
                },
                result: None,
            },
            &mut |entry| {
                let (_, session) = entry.parts();
                session.invoke(
                    index(&artifact, "notify"),
                    Call {
                        span: None,
                        context: context_address(),
                        arguments: &[],
                        output: None,
                    },
                    None,
                    &mut |_| 0,
                )
            },
        )
        .unwrap();
    assert_eq!(usage.host_calls, 1);
    let empty = super::artifact(
        "pub extern \"C\" fn echo(value: i64) -> i64 { return value; }",
        Target::WindowsX86_64,
        Limits::default(),
    )
    .unwrap();
    let program = Program::new(&empty, &owner, capacities()).unwrap();
    let mut config = config(&empty, &allocator, &limits, &[]);
    config.imports = None;
    assert!(program.create(config).is_ok());
}

#[test]
fn configured_cleanup_capacity_and_deadline_refuse_before_body_and_admission_panics_poison() {
    let artifact = super::artifact(r#"pub struct Data { value: bool } pub extern "C" fn echo(value: Data) -> bool { return value.value; }"#, Target::MacosAarch64, Limits::default()).unwrap();
    let owner = ();
    let program = Program::new(&artifact, &owner, capacities()).unwrap();
    let allocator = allocator();
    let imports = [];
    let zero = LimitsDescriptor {
        cleanup_records: 0,
        ..LimitsDescriptor::default()
    };
    let context = program
        .create(config(&artifact, &allocator, &zero, &imports))
        .unwrap();
    let mut usage = execution::Usage::default();
    let work = execution::ServiceWork::new(None);
    assert_eq!(
        context
            .run(
                &program,
                control(),
                &work,
                &mut usage,
                EntryPoint::Guest,
                &mut |_| panic!("no failure-record reserve")
            )
            .unwrap_err()
            .kind,
        Kind::Capacity
    );
    let limits = LimitsDescriptor {
        cleanup_records: 4096,
        ..LimitsDescriptor::default()
    };
    let context = program
        .create(config(&artifact, &allocator, &limits, &imports))
        .unwrap();
    assert_eq!(
        context
            .run(
                &program,
                control().and_deadline(std::time::Instant::now()),
                &work,
                &mut usage,
                EntryPoint::Guest,
                &mut |_| panic!("deadline")
            )
            .unwrap_err()
            .kind,
        Kind::Control(execution::Failure::DeadlineExceeded)
    );
    let input = Storage::new(vec![0]);
    input.panic_read.set(true);
    let result = Storage::new(vec![0]);
    let fault = context
        .run(
            &program,
            control(),
            &work,
            &mut usage,
            EntryPoint::Export {
                function: index(&artifact, "echo"),
                call: Call {
                    span: None,
                    context: context_address(),
                    arguments: &[Argument::Object(&input)],
                    output: Some(Address {
                        start: 0x8000,
                        extent: 1,
                    }),
                },
                result: Some(&result),
            },
            &mut |_| panic!("admission must contain reader panic"),
        )
        .unwrap_err();
    assert_eq!(fault.kind, Kind::HostPanic);
    assert!(context.is_poisoned().unwrap());
}
