use super::*;
use lang_runtime::cleanup as c;
use std::num::NonZeroUsize;

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
