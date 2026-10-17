use a::Region;
use lang_runtime::{
    allocation as a,
    buffer::{AllocError, BoundsError, BufferError, Bytes, Failure, Fault, Text, Utf8Error},
};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Trace {
    calls: usize,
    fail_on: usize,
    requests: Vec<a::Request>,
    released: Vec<(a::Request, Vec<u8>)>,
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<a::Storage> {
        let mut trace = self.0.borrow_mut();
        trace.calls += 1;
        trace.requests.push(request);
        if trace.calls == trace.fail_on {
            return None;
        }
        let mut result = a::Storage::try_new(request).unwrap();
        result.bytes_mut().fill(0xA5);
        Some(result)
    }
    fn deallocate(&mut self, region: a::Storage, request: a::Request) {
        self.0
            .borrow_mut()
            .released
            .push((request, region.bytes().to_vec()));
    }
}
fn allocator(limits: a::Limits) -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    (
        a::Allocator::new(Provider(Rc::clone(&trace)), limits).unwrap(),
        trace,
    )
}
fn limits() -> a::Limits {
    a::Limits {
        live_allocations: 32,
        ..Default::default()
    }
}
fn identity<P: a::Provider>(bytes: &mut Bytes, allocator: &a::Allocator<P>) -> [u8; 24] {
    let mut token = bytes.take_token(allocator).unwrap();
    let id = token[..24].try_into().unwrap();
    *bytes = Bytes::from_token(&mut token, allocator).unwrap();
    assert_eq!(token, [0; 32]);
    id
}
fn text<P: a::Provider>(allocator: &mut a::Allocator<P>, value: &str) -> Text {
    Bytes::from_slice(allocator, value.as_bytes())
        .unwrap()
        .take_text(allocator)
        .unwrap()
}

#[test]
fn byte_operations_check_indices_and_values_without_truncation_or_mutation() {
    let (mut allocator, trace) = allocator(limits());
    let mut bytes = Bytes::new(&mut allocator).unwrap();
    assert_eq!(bytes.len(&allocator), Ok(0));
    assert_eq!(allocator.stats().live_bytes, 24);
    for index in [-1, 0, i64::MAX] {
        assert_eq!(
            bytes.get(&allocator, index),
            Err(Failure::Error(BoundsError::OutOfBounds))
        );
    }
    for byte in [0, 255, 128] {
        bytes.push(&mut allocator, byte).unwrap();
    }
    assert_eq!(bytes.as_slice(&allocator).unwrap(), [0, 255, 128]);
    assert_eq!(bytes.get(&allocator, 1), Ok(255));
    let before = allocator.stats();
    for value in [-1, 256, i64::MIN, i64::MAX] {
        assert_eq!(
            bytes.push(&mut allocator, value),
            Err(Failure::Error(BufferError::InvalidByte))
        );
        assert_eq!(
            bytes.set(&mut allocator, 1, value),
            Err(Failure::Error(BufferError::InvalidByte))
        );
    }
    for index in [-1, 3, i64::MAX] {
        assert_eq!(
            bytes.set(&mut allocator, index, 0),
            Err(Failure::Error(BufferError::Bounds))
        );
    }
    assert_eq!(allocator.stats(), before);
    assert_eq!(bytes.as_slice(&allocator).unwrap(), [0, 255, 128]);
    bytes.set(&mut allocator, 0, 42).unwrap();
    assert_eq!(bytes.get(&allocator, 0), Ok(42));
    bytes.release(&mut allocator).unwrap();
    bytes.release(&mut allocator).unwrap();
    assert_eq!(bytes.len(&allocator), Err(Fault::Consumed));
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(trace.borrow().requests.iter().all(|r| r.alignment() == 8));
    assert_eq!(
        trace
            .borrow()
            .requests
            .iter()
            .map(|r| r.bytes())
            .collect::<Vec<_>>(),
        [24, 25, 26, 28]
    );
}

#[test]
fn growth_oom_preserves_data_handle_and_original_allocator_and_retains_attempt_charge() {
    let (mut allocator, trace) = allocator(limits());
    let mut bytes = Bytes::from_slice(&mut allocator, b"abc").unwrap();
    let original = identity(&mut bytes, &allocator);
    trace.borrow_mut().fail_on = 2;
    assert_eq!(
        bytes.push(&mut allocator, 100),
        Err(Failure::Error(BufferError::Allocation(
            AllocError::OutOfMemory
        )))
    );
    assert_eq!(bytes.as_slice(&allocator).unwrap(), b"abc");
    assert_eq!(identity(&mut bytes, &allocator), original);
    assert_eq!(
        (
            allocator.stats().live_bytes,
            allocator.stats().cumulative_bytes,
            allocator.stats().peak_live_bytes
        ),
        (27, 57, 27)
    );
    bytes.push(&mut allocator, 100).unwrap();
    let next = identity(&mut bytes, &allocator);
    assert_eq!(&next[..8], &original[..8]);
    assert_ne!(next, original);
    assert_eq!(bytes.as_slice(&allocator).unwrap(), b"abcd");
    assert_eq!(
        (
            allocator.stats().live_bytes,
            allocator.stats().cumulative_bytes,
            allocator.stats().peak_live_bytes
        ),
        (30, 87, 57)
    );
    assert_eq!(trace.borrow().released.len(), 1);
    assert_eq!(
        trace.borrow().released[0].0,
        a::Request::new(27, 8).unwrap()
    );
    assert_eq!(&trace.borrow().released[0].1[24..], b"abc");
    bytes.release(&mut allocator).unwrap();
}

#[test]
fn header_capacity_and_simultaneous_grow_storage_obey_each_configured_budget() {
    for limits in [
        a::Limits {
            request_bytes: 23,
            ..limits()
        },
        a::Limits {
            live_bytes: 23,
            ..limits()
        },
        a::Limits {
            cumulative_bytes: 23,
            ..limits()
        },
        a::Limits {
            live_allocations: 0,
            ..limits()
        },
        a::Limits {
            alignment: 4,
            ..limits()
        },
    ] {
        let (mut allocator, trace) = allocator(limits);
        assert!(matches!(
            Bytes::new(&mut allocator),
            Err(Failure::Error(AllocError::Limit))
        ));
        assert_eq!(allocator.stats(), a::Stats::default());
        assert_eq!(trace.borrow().calls, 0);
    }
    for limits in [
        a::Limits {
            live_bytes: 48,
            ..limits()
        },
        a::Limits {
            live_allocations: 1,
            ..limits()
        },
    ] {
        let (mut allocator, trace) = allocator(limits);
        let mut bytes = Bytes::new(&mut allocator).unwrap();
        let before = allocator.stats();
        assert_eq!(
            bytes.push(&mut allocator, 1),
            Err(Failure::Error(BufferError::Allocation(AllocError::Limit)))
        );
        assert_eq!(bytes.len(&allocator), Ok(0));
        assert_eq!(allocator.stats(), before);
        assert_eq!(trace.borrow().calls, 1);
        bytes.release(&mut allocator).unwrap();
    }
}

#[test]
fn utf8_transfer_keeps_storage_identity_and_consumes_bytes_on_both_domain_outcomes() {
    let (mut allocator, trace) = allocator(limits());
    let mut bytes = Bytes::from_slice(&mut allocator, "\0é😀".as_bytes()).unwrap();
    let original = identity(&mut bytes, &allocator);
    let before = allocator.stats();
    let mut text = bytes.take_text(&mut allocator).unwrap();
    assert_eq!(text.len(&allocator), Ok(7));
    assert_eq!(text.as_str(&allocator).unwrap(), "\0é😀");
    assert_eq!(bytes.len(&allocator), Err(Fault::Consumed));
    assert_eq!(allocator.stats(), before);
    let mut token = text.take_token(&allocator).unwrap();
    assert_eq!(&token[..24], &original);
    let mut text = Text::from_token(&mut token, &allocator).unwrap();
    text.release(&mut allocator).unwrap();
    for (input, offset) in [
        (vec![0x80], 0),
        (vec![b'a', 0xC0, 0xAF], 1),
        (vec![0xE2, 0x82], 0),
        (vec![b'a', 0xF4, 0x90, 0x80, 0x80], 1),
    ] {
        let mut bytes = Bytes::from_slice(&mut allocator, &input).unwrap();
        let before = allocator.stats();
        let frees = trace.borrow().released.len();
        assert!(
            matches!(bytes.take_text(&mut allocator),Err(Failure::Error(Utf8Error::Invalid(n))) if n==offset)
        );
        assert_eq!(bytes.len(&allocator), Err(Fault::Consumed));
        assert_eq!(allocator.stats().requests, before.requests);
        assert_eq!(allocator.stats().cumulative_bytes, before.cumulative_bytes);
        assert_eq!(trace.borrow().released.len(), frees + 1);
        bytes.release(&mut allocator).unwrap();
        assert_eq!(trace.borrow().released.len(), frees + 1);
    }
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn empty_text_and_allocation_authority_survive_transfer_without_an_extra_request() {
    let (mut allocator, _) = allocator(limits());
    let mut bytes = Bytes::new(&mut allocator).unwrap();
    let before = allocator.stats();
    let mut text = bytes.take_text(&mut allocator).unwrap();
    assert_eq!(text.len(&allocator), Ok(0));
    assert_eq!(text.as_str(&allocator), Ok(""));
    assert_eq!(allocator.stats(), before);
    assert!(matches!(
        allocator.revoke(),
        Err(a::Error::InUse { allocations: 1 })
    ));
    text.release(&mut allocator).unwrap();
    drop(allocator.revoke().unwrap());
    assert!(matches!(
        Bytes::new(&mut allocator),
        Err(Failure::Fault(Fault::Allocator(a::Error::MissingAuthority)))
    ));
}

#[test]
fn text_clone_bytes_concat_and_equality_preserve_exact_utf8_and_independent_owners() {
    let (mut allocator, _) = allocator(limits());
    let mut first = text(&mut allocator, "é");
    let mut second = text(&mut allocator, "e\u{301}");
    assert!(!first.equal(&second, &allocator).unwrap());
    let mut copy = first.try_clone(&mut allocator).unwrap();
    assert!(first.equal(&copy, &allocator).unwrap());
    let mut bytes = first.to_bytes(&mut allocator).unwrap();
    bytes.set(&mut allocator, 0, 65).unwrap();
    assert_eq!(first.as_str(&allocator), Ok("é"));
    assert_eq!(copy.as_str(&allocator), Ok("é"));
    let mut joined = first.concat(&second, &mut allocator).unwrap();
    assert_eq!(joined.as_str(&allocator), Ok("ée\u{301}"));
    let mut doubled = first.concat(&first, &mut allocator).unwrap();
    assert_eq!(doubled.as_str(&allocator), Ok("éé"));
    bytes.release(&mut allocator).unwrap();
    for value in [
        &mut first,
        &mut second,
        &mut copy,
        &mut joined,
        &mut doubled,
    ] {
        value.release(&mut allocator).unwrap();
    }
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn cloning_to_a_different_allocator_is_explicit_fallible_and_uses_target_identity() {
    let (mut source, _) = allocator(limits());
    let (mut target, trace) = allocator(limits());
    let mut bytes = Bytes::from_slice(&mut source, b"hello").unwrap();
    trace.borrow_mut().fail_on = 1;
    assert!(matches!(
        bytes.try_clone_in(&source, &mut target),
        Err(Failure::Error(AllocError::OutOfMemory))
    ));
    assert_eq!(bytes.as_slice(&source).unwrap(), b"hello");
    assert_eq!(target.stats().live_bytes, 0);
    assert_eq!(target.stats().cumulative_bytes, 29);
    let mut cloned = bytes.try_clone_in(&source, &mut target).unwrap();
    let source_id = identity(&mut bytes, &source);
    let target_id = identity(&mut cloned, &target);
    assert_ne!(&source_id[..8], &target_id[..8]);
    cloned.set(&mut target, 0, 72).unwrap();
    assert_eq!(bytes.as_slice(&source).unwrap(), b"hello");
    let mut source_text = bytes.take_text(&mut source).unwrap();
    let mut text_copy = source_text.try_clone_in(&source, &mut target).unwrap();
    let mut bytes_copy = source_text.to_bytes_in(&source, &mut target).unwrap();
    assert_eq!(text_copy.as_str(&target), Ok("hello"));
    assert_eq!(bytes_copy.as_slice(&target).unwrap(), b"hello");
    let (mut destination, _) = allocator(limits());
    let mut combined = source_text
        .concat_in(&source, &text_copy, &target, &mut destination)
        .unwrap();
    assert_eq!(combined.as_str(&destination), Ok("hellohello"));
    combined.release(&mut destination).unwrap();
    assert_eq!(destination.stats().live_bytes, 0);
    cloned.release(&mut target).unwrap();
    text_copy.release(&mut target).unwrap();
    bytes_copy.release(&mut target).unwrap();
    source_text.release(&mut source).unwrap();
    assert_eq!(source.stats().live_bytes, 0);
    assert_eq!(target.stats().live_bytes, 0);
}

#[test]
fn private_tokens_reject_wrong_kind_foreign_reserved_and_stale_handles_before_consumption() {
    let (mut foreign, foreign_trace) = allocator(limits());
    let (mut allocator, trace) = allocator(limits());
    let mut bytes = Bytes::from_slice(&mut allocator, b"hi").unwrap();
    let mut token = bytes.take_token(&allocator).unwrap();
    let original = token;
    assert!(matches!(
        Bytes::from_token(&mut token, &foreign),
        Err(Fault::Allocator(a::Error::ForeignAllocator))
    ));
    assert_eq!(token, original);
    assert_eq!(foreign_trace.borrow().calls, 0);
    assert!(matches!(
        Text::from_token(&mut token, &allocator),
        Err(Fault::WrongKind)
    ));
    assert_eq!(token, original);
    let mut reserved = token;
    reserved[12] = 1;
    assert!(matches!(
        Bytes::from_token(&mut reserved, &allocator),
        Err(Fault::Allocator(a::Error::InvalidStorage))
    ));
    let mut wrong = token;
    wrong[24..].copy_from_slice(&1_u64.to_le_bytes());
    assert!(matches!(
        Text::from_token(&mut wrong, &allocator),
        Err(Fault::WrongKind)
    ));
    let mut restored = Bytes::from_token(&mut token, &allocator).unwrap();
    assert_eq!(token, [0; 32]);
    restored.release(&mut allocator).unwrap();
    let mut stale = original;
    assert!(matches!(
        Bytes::from_token(&mut stale, &allocator),
        Err(Fault::Allocator(a::Error::StaleHandle))
    ));
    assert_eq!(trace.borrow().released.len(), 1);
    assert!(matches!(
        restored.take_text(&mut foreign),
        Err(Failure::Fault(Fault::Consumed))
    ));
}

#[test]
fn rejected_preflight_and_failed_clone_or_concat_leave_source_owners_live() {
    let (mut foreign, _) = allocator(limits());
    let (mut allocator, trace) = allocator(limits());
    let mut bytes = Bytes::from_slice(&mut allocator, b"a").unwrap();
    assert!(matches!(
        bytes.take_text(&mut foreign),
        Err(Failure::Fault(Fault::Allocator(a::Error::ForeignAllocator)))
    ));
    assert_eq!(bytes.as_slice(&allocator).unwrap(), b"a");
    let mut text = bytes.take_text(&mut allocator).unwrap();
    let call = trace.borrow().calls + 1;
    trace.borrow_mut().fail_on = call;
    assert!(matches!(
        text.try_clone(&mut allocator),
        Err(Failure::Error(AllocError::OutOfMemory))
    ));
    assert_eq!(text.as_str(&allocator), Ok("a"));
    let call = trace.borrow().calls + 1;
    trace.borrow_mut().fail_on = call;
    assert!(matches!(
        text.concat(&text, &mut allocator),
        Err(Failure::Error(AllocError::OutOfMemory))
    ));
    assert_eq!(text.as_str(&allocator), Ok("a"));
    text.release(&mut allocator).unwrap();
}

#[test]
fn generated_byte_edits_and_utf8_outcomes_match_standard_library_oracles() {
    for seed in 1..=100_u64 {
        let (mut allocator, _) = allocator(limits());
        let mut random = seed;
        let mut expected = Vec::new();
        let mut bytes = Bytes::new(&mut allocator).unwrap();
        for _ in 0..128 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            if random % 3 == 0 && !expected.is_empty() {
                let index = random as usize % expected.len();
                let value = (random >> 8) as u8;
                expected[index] = value;
                bytes
                    .set(&mut allocator, index as i64, i64::from(value))
                    .unwrap();
            } else {
                let value = random as u8;
                expected.push(value);
                bytes.push(&mut allocator, i64::from(value)).unwrap();
            }
            assert_eq!(bytes.as_slice(&allocator).unwrap(), expected);
        }
        let before = allocator.stats();
        match std::str::from_utf8(&expected) {
            Ok(expected) => {
                let mut text = bytes.take_text(&mut allocator).unwrap();
                assert_eq!(text.as_str(&allocator).unwrap(), expected);
                text.release(&mut allocator).unwrap();
            }
            Err(error) => assert!(
                matches!(bytes.take_text(&mut allocator),Err(Failure::Error(Utf8Error::Invalid(offset))) if offset==error.valid_up_to()as i64)
            ),
        }
        assert_eq!(allocator.stats().requests, before.requests);
        assert_eq!(allocator.stats().live_bytes, 0);
    }
}
