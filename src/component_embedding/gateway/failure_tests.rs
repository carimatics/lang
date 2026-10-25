use super::*;
use lang_runtime::cleanup as c;
use std::num::NonZeroUsize;

fn project_failure(
    failure: &r::Failure,
    ceiling: ffi::Limits,
    digest: [u8; 32],
    mode: u32,
    entry: m::FunctionId,
    allocations: a::Stats,
    canonical_indices: u64,
) -> Option<(wire::Report, Vec<u8>)> {
    super::project_failure(
        failure,
        ceiling,
        FailureCode {
            digest,
            mode,
            entry,
            functions: &[],
        },
        allocations,
        canonical_indices,
    )
}

fn metadata_function(name: String, span: m::Span) -> m::ownership::declarations::Function {
    let signature_span = m::Span {
        end: span.start + 1,
        ..span
    };
    m::ownership::declarations::Function {
        effect: m::ownership::declarations::FunctionEffect::Synchronous,
        abi: m::ownership::declarations::FunctionAbi::Guest,
        name,
        public: false,
        span,
        signature_span,
        parameters: Vec::new(),
        result: m::ownership::TypeUse {
            ty: m::ownership::Type::I64,
            span: signature_span,
        },
        locals: Vec::new(),
    }
}

fn cleanup_spans(spans: &[m::Span]) -> r::Failure {
    let mut failure = composite(true, 81);
    failure.function = Some(m::FunctionId(0));
    let mut budget = m::Budget::new(Default::default());
    let mut session =
        c::Session::prepare(NonZeroUsize::new(spans.len() + 1).unwrap(), &mut budget).unwrap();
    session.execution(c::ExecutionFailure::Panic, failure.span);
    let mut cleaner = c::Contextual::new(|_: &mut (), _, _: &mut c::Context<'_>| Err(19));
    for (slot, &span) in spans.iter().enumerate() {
        assert!(
            session
                .retire(
                    c::Slot(slot),
                    (),
                    Some(span),
                    c::Reason::Explicit,
                    &mut cleaner
                )
                .is_err()
        );
    }
    failure.report = Some(session.into_report());
    failure
}

#[test]
fn cleanup_origins_use_unique_retained_function_spans_without_guessing() {
    let span = |source, start, end| m::Span {
        source: m::SourceId(source),
        start,
        end,
    };
    let functions = [
        metadata_function("primary".into(), span(0, 0, 100)),
        metadata_function("cleanup".into(), span(0, 100, 200)),
        metadata_function("overlap".into(), span(0, 145, 180)),
    ];
    let actual = span(0, 110, 120);
    let failure = cleanup_spans(&[actual, span(9, 110, 120), span(0, 160, 170)]);
    let (report, bytes) = super::project_failure(
        &failure,
        ceiling(4, 4),
        FailureCode {
            digest: [7; 32],
            mode: 1,
            entry: m::FunctionId(0),
            functions: &functions,
        },
        Default::default(),
        3,
    )
    .unwrap();
    assert_eq!(
        report.suppressed[0].origin,
        Some(wire::Frame {
            mode: wire::Mode::Jit,
            digest: [7; 32],
            member: 1,
            span: Some(actual),
        })
    );
    assert!(report.suppressed[1].origin.is_none() && report.suppressed[2].origin.is_none());
    assert_eq!(report.omitted, 0);
    assert_eq!(
        wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
        report
    );
}

#[test]
fn bounded_origin_lookup_keeps_unknown_records_and_the_known_primary_origin() {
    let span = |source, start, end| m::Span {
        source: m::SourceId(source),
        start,
        end,
    };
    let mut functions: Vec<_> = (0..4097)
        .map(|i| metadata_function(format!("f{i}"), span(1, 0, 100)))
        .collect();
    functions[0] = metadata_function("primary".into(), span(0, 0, 100));
    functions[4096] = metadata_function("unexamined".into(), span(0, 100, 200));
    let failure = cleanup_spans(&[span(0, 110, 120), span(0, 11, 51)]);
    let (report, _) = super::project_failure(
        &failure,
        ceiling(4, 4),
        FailureCode {
            digest: [7; 32],
            mode: 1,
            entry: m::FunctionId(0),
            functions: &functions,
        },
        Default::default(),
        3,
    )
    .unwrap();
    assert_eq!(report.suppressed.len(), 2);
    assert!(report.suppressed[0].origin.is_none());
    assert_eq!(report.suppressed[1].origin, report.primary.origin);
    assert_eq!(report.omitted, 0);
}

#[test]
fn primary_cleanup_uses_its_original_span_instead_of_the_later_scope_fault() {
    let mut failure = composite(false, 81);
    failure.reason = r::Reason::Cleanup;
    failure.function = Some(m::FunctionId(0));
    let span = |source, start, end| m::Span {
        source: m::SourceId(source),
        start,
        end,
    };
    let original = span(0, 110, 120);
    let mut budget = m::Budget::new(Default::default());
    let mut cleanup = c::Session::prepare(NonZeroUsize::new(1).unwrap(), &mut budget).unwrap();
    let mut cleaner = c::Contextual::new(|_: &mut (), _, _: &mut c::Context<'_>| Err(19));
    assert!(
        cleanup
            .retire(
                c::Slot(0),
                (),
                Some(original),
                c::Reason::Explicit,
                &mut cleaner
            )
            .is_err()
    );
    failure.report = Some(cleanup.into_report());
    let functions = [
        metadata_function("caller".into(), span(0, 0, 100)),
        metadata_function("cleanup".into(), span(0, 100, 200)),
    ];
    let (report, bytes) = super::project_failure(
        &failure,
        ceiling(4, 4),
        FailureCode {
            digest: [7; 32],
            mode: 1,
            entry: m::FunctionId(0),
            functions: &functions,
        },
        Default::default(),
        3,
    )
    .unwrap();
    assert_eq!(report.primary.category, wire::Category::Destructor);
    assert_eq!(report.primary.phase, wire::Phase::Cleanup);
    let origin = report.primary.origin.as_ref().unwrap();
    assert_eq!((origin.member, origin.span), (1, Some(original)));
    assert_eq!(report.frames[0].span, failure.span);
    assert_eq!(
        wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
        report
    );
}

#[test]
fn selected_primaries_survive_exhausted_controls_and_zero_optional_evidence_caps() {
    let cases = [
        (
            r::Reason::Cancelled,
            wire::Category::Cancellation,
            wire::Phase::Call,
            wire::Diagnostic::Cancelled,
        ),
        (
            r::Reason::Control(e::Failure::DeadlineExceeded),
            wire::Category::Interruption,
            wire::Phase::Call,
            wire::Diagnostic::Interrupted,
        ),
        (
            r::Reason::Execution(c::ExecutionFailure::Fault),
            wire::Category::Trap,
            wire::Phase::Call,
            wire::Diagnostic::Trap,
        ),
        (
            r::Reason::Execution(c::ExecutionFailure::HostPanic),
            wire::Category::HostContract,
            wire::Phase::Call,
            wire::Diagnostic::HostContract,
        ),
        (
            r::Reason::Instance(execution::InstanceError::Active),
            wire::Category::Admission,
            wire::Phase::Admission,
            wire::Diagnostic::Reentry,
        ),
        (
            r::Reason::Instance(execution::InstanceError::Poisoned),
            wire::Category::Admission,
            wire::Phase::Admission,
            wire::Diagnostic::InstancePoisoned,
        ),
    ];
    for (reason, category, phase, diagnostic) in cases {
        let mut failure = composite(false, 81);
        failure.reason = reason;
        let mut bounds = ceiling(0, 0);
        bounds.fuel = 37;
        bounds.service_work = 9;
        bounds.host_calls = 3;
        let (report, bytes) = project_failure(
            &failure,
            bounds,
            [7; 32],
            1,
            m::FunctionId(2),
            Default::default(),
            3,
        )
        .unwrap();
        assert_eq!(
            (
                report.primary.category,
                report.primary.phase,
                report.primary.diagnostic
            ),
            (category, phase, diagnostic)
        );
        assert!(report.primary.origin.is_some() && report.primary.message.is_none());
        assert!(report.suppressed.is_empty() && report.frames.is_empty());
        assert_eq!(report.omitted, 81);
        assert!(report.poisoned);
        assert_eq!(
            (
                report.usage.fuel,
                report.usage.service_work,
                report.usage.host_calls
            ),
            (37, 9, 3)
        );
        assert!(bytes.len() < 512);
        assert_eq!(
            wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
            report
        );
    }
}

#[test]
fn structured_limits_keep_the_actual_limit_attempt_and_execution_mode() {
    let cases = [
        (
            r::Reason::Resource(m::Error {
                kind: m::ErrorKind::Limit {
                    kind: m::LimitKind::Work,
                    limit: 47,
                    requested: 61,
                },
                function: None,
                block: None,
                instruction: None,
            }),
            wire::ReportLimitKind::PreparationWork,
            47,
            61,
        ),
        (
            r::Reason::Layout(lang_runtime::layout::Error::CellLimit {
                cells: 69,
                limit: 64,
            }),
            wire::ReportLimitKind::LiveCells,
            64,
            69,
        ),
        (
            r::Reason::ComponentFrame(wire::Error {
                offset: 13,
                kind: wire::ErrorKind::Limit {
                    kind: wire::LimitKind::SerializationBytes,
                    limit: 17,
                    requested: 24,
                },
            }),
            wire::ReportLimitKind::SerializationBytes,
            17,
            24,
        ),
        (
            r::Reason::Execution(
                lang_mir::unit::ownership::failure::ExecutionFailure::ServiceWorkLimit {
                    limit: 17,
                    used: 16,
                    requested: 7,
                },
            ),
            wire::ReportLimitKind::ServiceWork,
            17,
            23,
        ),
        (
            r::Reason::FrameLimit {
                limit: 3,
                attempted: 5,
            },
            wire::ReportLimitKind::Frames,
            3,
            5,
        ),
        (
            r::Reason::CellLimit {
                limit: 7,
                attempted: 12,
            },
            wire::ReportLimitKind::LiveCells,
            7,
            12,
        ),
        (
            r::Reason::Schedule(lang_runtime::task::Error::LiveTaskLimit { limit: 2 }),
            wire::ReportLimitKind::LiveTasks,
            2,
            3,
        ),
    ];
    for (reason, kind, maximum, attempted) in cases {
        let mut failure = composite(false, 81);
        failure.reason = reason;
        failure.report = None;
        failure.poisoned = false;
        for mode in [0, 1, 2] {
            let (report, bytes) = project_failure(
                &failure,
                ceiling(4, 4),
                [7; 32],
                mode,
                m::FunctionId(2),
                Default::default(),
                3,
            )
            .expect("structured failure report");
            assert_eq!(report.primary.category, wire::Category::Limit);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
            assert_eq!(
                report.primary.limit,
                Some(wire::LimitDetail {
                    kind,
                    limit: maximum,
                    attempted
                })
            );
            assert_eq!(report.primary.origin.as_ref().unwrap().member, 7);
            assert_eq!(
                report.primary.origin.as_ref().unwrap().mode,
                match mode {
                    0 => wire::Mode::Vm,
                    1 => wire::Mode::Jit,
                    _ => wire::Mode::Aot,
                }
            );
            assert_eq!(report.primary.origin.as_ref().unwrap().span, failure.span);
            assert!(!report.poisoned);
            assert_eq!(
                wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
                report
            );
        }
    }
}

#[test]
fn allocator_reports_retain_the_rejected_batch_after_original_storage_is_released() {
    for count_limit in [false, true] {
        let mut allocator = a::Allocator::new(
            a::System,
            a::Limits {
                request_bytes: u64::MAX,
                live_bytes: if count_limit { u64::MAX } else { 15 },
                live_allocations: if count_limit { 2 } else { 4 },
                ..Default::default()
            },
        )
        .unwrap();
        let owner = allocator.allocate(a::Request::new(8, 1).unwrap()).unwrap();
        let plan = if count_limit {
            vec![a::Request::new(0, 1).unwrap(); 3]
        } else {
            vec![a::Request::new(8, 1).unwrap()]
        };
        let before = allocator.stats();
        let reason = allocator.check(&plan).unwrap_err();
        assert_eq!(allocator.stats(), before);
        allocator.release(owner).unwrap();
        assert_eq!(allocator.stats().live_allocations, 0);
        let mut failure = composite(false, 81);
        failure.reason = r::Reason::Layout(lang_runtime::layout::Error::Allocation(reason));
        failure.report = None;
        failure.poisoned = false;
        let (report, bytes) = project_failure(
            &failure,
            ceiling(4, 4),
            [7; 32],
            1,
            m::FunctionId(2),
            allocator.stats(),
            3,
        )
        .unwrap();
        let (kind, limit, attempted) = if count_limit {
            (wire::ReportLimitKind::LiveAllocations, 2, 4)
        } else {
            (wire::ReportLimitKind::HeapBytes, 15, 16)
        };
        assert_eq!(
            report.primary.limit,
            Some(wire::LimitDetail {
                kind,
                limit,
                attempted
            })
        );
        assert_eq!(report.usage.peak_heap_bytes, 8);
        assert_eq!(report.usage.peak_live_allocations, 1);
        assert!(!report.poisoned);
        assert_eq!(
            wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
            report
        );
    }
}

#[test]
fn the_full_native_record_and_trace_caps_fit_the_mandatory_envelope() {
    let failure = native_failure(1, native_packet(64, 64));
    let (report, bytes) = project_failure(
        &failure,
        ceiling(u64::MAX, u64::MAX),
        [7; 32],
        1,
        m::FunctionId(2),
        Default::default(),
        3,
    )
    .unwrap();
    assert_eq!(
        (report.suppressed.len(), report.frames.len(), report.omitted),
        (64, 64, 7)
    );
    assert!(bytes.len() <= 16384);
    assert_eq!(
        wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
        report
    );
}

#[test]
fn decoder_limits_keep_their_original_namespace_after_the_shared_budget_loan() {
    let mut original = m::Budget::new(m::Limits {
        allocations: 1000,
        ..Default::default()
    });
    original.allocate::<u8>(100).unwrap();
    let mut session = wire::Session::new(wire::Limits {
        allocation_bytes: 16,
        ..Default::default()
    });
    session.vector::<u8>(4, 0).unwrap();
    let error = {
        let mut preparing = session.scope_preparation(&mut original).unwrap();
        preparing.vector::<u8>(13, 37).unwrap_err()
    };
    assert_eq!(original.allocation_bytes(), 100);
    assert_eq!(session.usage().allocation_bytes, 4);
    let mut failure = composite(false, 81);
    failure.reason = error.into();
    failure.report = None;
    failure.poisoned = false;
    let (report, _) = project_failure(
        &failure,
        ceiling(4, 4),
        [7; 32],
        1,
        m::FunctionId(2),
        Default::default(),
        3,
    )
    .unwrap();
    assert_eq!(
        report.primary.limit,
        Some(wire::LimitDetail {
            kind: wire::ReportLimitKind::DecoderAllocationBytes,
            limit: 16,
            attempted: 17,
        })
    );
}

#[test]
fn the_tighter_compiler_limit_and_restored_decoder_limit_keep_distinct_provenance() {
    let mut original = m::Budget::new(m::Limits {
        allocations: 105,
        ..Default::default()
    });
    original.allocate::<u8>(100).unwrap();
    let mut session = wire::Session::new(wire::Limits {
        allocation_bytes: 16,
        ..Default::default()
    });
    session.vector::<u8>(4, 0).unwrap();
    let compiler = session
        .scope_preparation(&mut original)
        .unwrap()
        .vector::<u8>(13, 37)
        .unwrap_err();
    let decoder = session.vector::<u8>(13, 41).unwrap_err();
    assert_eq!(original.allocation_bytes(), 100);
    assert_eq!(session.usage().allocation_bytes, 4);
    for (error, kind, limit, attempted) in [
        (compiler, wire::ReportLimitKind::MetadataBytes, 105, 113),
        (
            decoder,
            wire::ReportLimitKind::DecoderAllocationBytes,
            16,
            17,
        ),
    ] {
        let mut failure = composite(false, 81);
        failure.reason = error.into();
        failure.report = None;
        failure.poisoned = false;
        let (report, _) = project_failure(
            &failure,
            ceiling(4, 4),
            [7; 32],
            1,
            m::FunctionId(2),
            Default::default(),
            3,
        )
        .unwrap();
        assert_eq!(
            report.primary.limit,
            Some(wire::LimitDetail {
                kind,
                limit,
                attempted
            })
        );
    }
}

#[test]
fn decoded_preparation_history_keeps_decoder_provenance_on_initialization_and_advance() {
    let limits = wire::Limits {
        allocation_bytes: 16,
        ..Default::default()
    };
    let usage = m::Usage {
        allocation_bytes: 17,
        ..Default::default()
    };
    let initialization = wire::Session::with_usage(limits, usage).err().unwrap();
    let mut session = wire::Session::new(limits);
    let advance = session.advance(usage).unwrap_err();
    assert_eq!(session.usage().allocation_bytes, 0);
    for error in [initialization, advance] {
        let mut failure = composite(false, 81);
        failure.reason = error.into();
        failure.report = None;
        failure.poisoned = false;
        let (report, _) = project_failure(
            &failure,
            ceiling(4, 4),
            [7; 32],
            1,
            m::FunctionId(2),
            Default::default(),
            3,
        )
        .unwrap();
        assert_eq!(
            report.primary.limit,
            Some(wire::LimitDetail {
                kind: wire::ReportLimitKind::DecoderAllocationBytes,
                limit: 16,
                attempted: 17,
            })
        );
    }
}

fn ceiling(suppressed_records: u64, trace_frames: u64) -> ffi::Limits {
    ffi::Limits {
        header: ffi::Header {
            size: size_of::<ffi::Limits>() as u32,
            version: 1,
        },
        fuel: 100,
        service_work: 100,
        host_calls: 100,
        heap_bytes: 4096,
        live_allocations: 16,
        frames: 16,
        live_cells: 64,
        live_tasks: 8,
        canonical_indices: 3,
        preparation_work: 100_000,
        metadata_bytes: 65536,
        value_elements: 1024,
        serialization_bytes: 65536,
        depth: 128,
        decoder_allocation_bytes: 65536,
        suppressed_records,
        trace_frames,
        reserved: [0; 2],
    }
}

fn composite(panic: bool, capacity: usize) -> r::Failure {
    let span = m::Span {
        source: m::SourceId(0),
        start: 11,
        end: 51,
    };
    let mut budget = m::Budget::new(Default::default());
    let mut cleanup =
        c::Session::prepare(NonZeroUsize::new(capacity).unwrap(), &mut budget).unwrap();
    cleanup.execution(
        if panic {
            c::ExecutionFailure::Panic
        } else {
            c::ExecutionFailure::Fault
        },
        Some(span),
    );
    let mut cleaner = c::Contextual::new(|_: &mut (), _, _: &mut c::Context<'_>| Err(19));
    for slot in 0..80 {
        assert!(
            cleanup
                .retire(
                    c::Slot(slot),
                    (),
                    Some(span),
                    c::Reason::Explicit,
                    &mut cleaner
                )
                .is_err()
        );
    }
    r::Failure {
        reason: if panic {
            r::Reason::Panic(Some(77))
        } else {
            r::Reason::ComponentIndices {
                limit: 3,
                attempted: 4,
            }
        },
        function: Some(m::FunctionId(7)),
        span: Some(span),
        usage: e::Usage {
            fuel: 37,
            service_work: 9,
            host_calls: 3,
            ..Default::default()
        },
        tasks: Default::default(),
        events: Vec::new(),
        report: Some(cleanup.into_report()),
        poisoned: true,
    }
}

#[test]
fn composite_projection_keeps_the_primary_and_exact_optional_evidence_counts() {
    for panic in [false, true] {
        for maximum in [0, 3, 64] {
            for trace in [0, 8] {
                let failure = composite(panic, 81);
                let (report, bytes) = project_failure(
                    &failure,
                    ceiling(maximum, trace),
                    [7; 32],
                    1,
                    m::FunctionId(2),
                    Default::default(),
                    3,
                )
                .unwrap();
                assert_eq!(
                    report.primary.category,
                    if panic {
                        wire::Category::Panic
                    } else {
                        wire::Category::Limit
                    }
                );
                assert_eq!(
                    report.primary.diagnostic,
                    if panic {
                        wire::Diagnostic::Panic
                    } else {
                        wire::Diagnostic::Limit
                    }
                );
                assert_eq!(report.primary.phase, wire::Phase::Call);
                if !panic {
                    let limit = report.primary.limit.unwrap();
                    assert_eq!(limit.kind, wire::ReportLimitKind::CanonicalIndices);
                    assert_eq!((limit.limit, limit.attempted), (3, 4));
                }
                assert_eq!(report.suppressed.len(), maximum as usize);
                assert_eq!(report.omitted, 80 - maximum + u64::from(trace == 0));
                assert_eq!(report.frames.is_empty(), trace == 0);
                assert!(report.poisoned);
                assert_eq!(report.usage.fuel, 37);
                assert_eq!(report.usage.service_work, 9);
                assert_eq!(report.usage.host_calls, 3);
                assert_eq!(report.usage.canonical_indices, 3);
                assert_eq!(report.primary.origin.as_ref().unwrap().member, 7);
                for record in &report.suppressed {
                    assert_eq!(record.category, wire::Category::Destructor);
                    assert_eq!(record.phase, wire::Phase::Cleanup);
                    assert_eq!(record.diagnostic, wire::Diagnostic::Destructor);
                    assert_eq!(record.origin, report.primary.origin);
                    assert!(record.message.is_none() && record.limit.is_none());
                }
                if maximum == 64 {
                    assert!(bytes.len() > 4096);
                }
                assert_eq!(
                    wire::decode_report(&bytes, &mut wire::Session::new(Default::default()))
                        .unwrap(),
                    report
                );
            }
        }
    }
}

#[test]
fn projection_adds_its_drops_to_existing_cleanup_omission_without_double_counting() {
    let failure = composite(false, 5);
    assert_eq!(failure.report.as_ref().unwrap().secondary().len(), 4);
    assert_eq!(failure.report.as_ref().unwrap().truncated(), 76);
    let (report, bytes) = project_failure(
        &failure,
        ceiling(3, 8),
        [7; 32],
        0,
        m::FunctionId(2),
        Default::default(),
        3,
    )
    .unwrap();
    assert_eq!(report.suppressed.len(), 3);
    assert_eq!(report.omitted, 77);
    assert_eq!(report.primary.limit.unwrap().attempted, 4);
    assert_eq!(
        wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
        report
    );
}

fn native_packet(records: u32, traces: u32) -> Vec<u8> {
    let origin = wire::Frame {
        mode: wire::Mode::Host,
        digest: [17; 32],
        member: 9,
        span: Some(m::Span {
            source: m::SourceId(0),
            start: 3,
            end: 11,
        }),
    };
    let report = wire::Report {
        policy: wire::MessagePolicy::Sanitized,
        primary: wire::Record {
            category: wire::Category::HostContract,
            phase: wire::Phase::Call,
            diagnostic: wire::Diagnostic::HostContract,
            message: Some("native private primary".into()),
            limit: None,
            origin: Some(origin.clone()),
        },
        suppressed: (0..records)
            .map(|index| wire::Record {
                category: wire::Category::Destructor,
                phase: wire::Phase::Cleanup,
                diagnostic: wire::Diagnostic::Destructor,
                message: Some(format!("native private cleanup {index}")),
                limit: None,
                origin: Some(wire::Frame {
                    member: 10 + index,
                    ..origin.clone()
                }),
            })
            .collect(),
        poisoned: true,
        omitted: 7,
        usage: wire::ReportUsage {
            fuel: 9999,
            service_work: 9999,
            host_calls: 9999,
            peak_heap_bytes: 9999,
            peak_live_allocations: 9999,
            peak_frames: 9999,
            peak_live_cells: 9999,
            peak_live_tasks: 9999,
            canonical_indices: 9999,
        },
        frames: (0..traces)
            .map(|index| wire::Frame {
                member: 20 + index,
                ..origin.clone()
            })
            .collect(),
    };
    wire::encode_report(&report, &mut wire::Session::new(Default::default())).unwrap()
}

fn native_failure(code: u32, bytes: Vec<u8>) -> r::Failure {
    let mut failure = composite(false, 81);
    failure.reason = r::Reason::Native(execution::NativeFailure::Status { code, bytes });
    failure.report = None;
    failure.poisoned = false;
    failure
}

#[test]
fn native_report_is_redacted_bounded_and_uses_the_original_usage() {
    for trace in [0, 1] {
        let failure = native_failure(1, native_packet(80, 5));
        let (report, bytes) = project_failure(
            &failure,
            ceiling(3, trace),
            [7; 32],
            1,
            m::FunctionId(2),
            Default::default(),
            3,
        )
        .unwrap();
        assert_eq!(report.policy, wire::MessagePolicy::Redacted);
        assert_eq!(report.primary.category, wire::Category::HostContract);
        assert_eq!(report.primary.diagnostic, wire::Diagnostic::HostContract);
        assert_eq!(report.primary.origin.as_ref().unwrap().digest, [17; 32]);
        assert_eq!(
            report.primary.origin.as_ref().unwrap().mode,
            wire::Mode::Host
        );
        assert!(report.primary.message.is_none() && report.poisoned);
        assert_eq!(report.suppressed.len(), 3);
        assert_eq!(
            report
                .suppressed
                .iter()
                .map(|r| r.origin.as_ref().unwrap().member)
                .collect::<Vec<_>>(),
            [10, 11, 12]
        );
        assert!(report.suppressed.iter().all(|r| r.message.is_none()));
        assert_eq!(report.frames.len(), trace as usize);
        assert_eq!(report.omitted, 7 + 77 + 5 - trace);
        assert_eq!(report.usage.fuel, 37);
        assert_eq!(report.usage.service_work, 9);
        assert_eq!(report.usage.host_calls, 3);
        assert_eq!(report.usage.canonical_indices, 3);
        assert_eq!(report.usage.peak_heap_bytes, 0);
        assert!(
            !bytes
                .windows(b"native private".len())
                .any(|part| part == b"native private")
        );
        assert_eq!(
            wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
            report
        );
    }
}

#[test]
fn malformed_native_report_and_unknown_status_are_contract_failures() {
    let good = native_packet(1, 1);
    let mut trailing = good.clone();
    trailing.push(0);
    for (code, bytes) in [
        (1, Vec::new()),
        (1, b"LCMP".to_vec()),
        (1, trailing),
        (2, good),
    ] {
        let failure = native_failure(code, bytes);
        let (report, bytes) = project_failure(
            &failure,
            ceiling(4, 4),
            [7; 32],
            0,
            m::FunctionId(2),
            Default::default(),
            3,
        )
        .unwrap();
        assert_eq!(report.primary.category, wire::Category::HostContract);
        assert_eq!(report.primary.phase, wire::Phase::Call);
        assert_eq!(report.primary.diagnostic, wire::Diagnostic::HostContract);
        assert_eq!(report.primary.origin.as_ref().unwrap().digest, [7; 32]);
        assert_eq!(report.primary.origin.as_ref().unwrap().mode, wire::Mode::Vm);
        assert!(report.poisoned && report.primary.message.is_none());
        assert_eq!(report.usage.host_calls, 3);
        assert_eq!(
            wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
            report
        );
    }
}

#[test]
fn native_secondaries_precede_later_local_cleanup_and_preserve_omissions() {
    let mut failure = composite(false, 81);
    failure.reason = r::Reason::Native(execution::NativeFailure::Status {
        code: 1,
        bytes: native_packet(2, 5),
    });
    let (report, bytes) = project_failure(
        &failure,
        ceiling(3, 2),
        [7; 32],
        1,
        m::FunctionId(2),
        Default::default(),
        3,
    )
    .unwrap();
    assert_eq!(report.primary.diagnostic, wire::Diagnostic::HostContract);
    assert_eq!(report.suppressed.len(), 3);
    assert_eq!(report.suppressed[0].origin.as_ref().unwrap().member, 10);
    assert_eq!(report.suppressed[1].origin.as_ref().unwrap().member, 11);
    assert_eq!(report.suppressed[2].origin.as_ref().unwrap().member, 7);
    assert_eq!(
        report.suppressed[2].origin.as_ref().unwrap().digest,
        [7; 32]
    );
    assert_eq!(report.omitted, 7 + 79 + 3);
    assert_eq!(
        wire::decode_report(&bytes, &mut wire::Session::new(Default::default())).unwrap(),
        report
    );
}
