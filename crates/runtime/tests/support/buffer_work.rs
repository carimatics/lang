use super::*;
use lang_runtime::execution as e;

struct Meter {
    control: e::Control<'static>,
    usage: e::Usage,
}
impl Meter {
    fn new(limit: u64) -> Self {
        Self {
            control: e::Control::with_limits(e::Limits::unbounded()).and_service_work(Some(limit)),
            usage: e::Usage::default(),
        }
    }
    fn work(&mut self) -> impl FnMut(u64) -> Result<(), e::Failure> + '_ {
        |cost| self.control.charge_service_work(&mut self.usage, cost)
    }
}
fn refusal(limit: u64, used: u64, requested: u64) -> Fault {
    Fault::ServiceWork(e::Failure::ServiceWorkLimit {
        limit,
        used,
        requested,
    })
}

#[test]
fn growth_is_admitted_before_allocation_and_provider_failure_does_not_refund_work() {
    let (mut allocator, trace) = allocator(limits());
    let mut bytes = Bytes::from_slice(&mut allocator, b"A").unwrap();
    let original = identity(&mut bytes, &allocator);
    let requests = trace.borrow().calls;
    let mut meter = Meter::new(1);
    assert_eq!(
        bytes.push_metered(&mut allocator, 66, &mut meter.work()),
        Err(Failure::Fault(refusal(1, 0, 2)))
    );
    assert_eq!(meter.usage.service_work, 0);
    assert_eq!(trace.borrow().calls, requests);
    assert_eq!(identity(&mut bytes, &allocator), original);
    assert_eq!(bytes.get(&allocator, 0), Ok(65));

    meter.control = Meter::new(2).control;
    trace.borrow_mut().fail_on = requests + 1;
    assert_eq!(
        bytes.push_metered(&mut allocator, 66, &mut meter.work()),
        Err(Failure::Error(BufferError::Allocation(
            AllocError::OutOfMemory
        )))
    );
    assert_eq!(meter.usage.service_work, 2);
    assert_eq!(identity(&mut bytes, &allocator), original);
    assert_eq!(allocator.stats().live_allocations, 1);
    meter.control = Meter::new(4).control;
    bytes
        .push_metered(&mut allocator, 66, &mut meter.work())
        .unwrap();
    assert_eq!(meter.usage.service_work, 4);
    assert_eq!(bytes.get(&allocator, 1), Ok(66));
    bytes.release(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn utf8_admission_preserves_owners_and_tokens_until_the_scan_is_admitted() {
    let (mut allocator, trace) = allocator(limits());
    let mut bytes = Bytes::from_slice(&mut allocator, "あ".as_bytes()).unwrap();
    let original = identity(&mut bytes, &allocator);
    let requests = trace.borrow().calls;
    let mut meter = Meter::new(2);
    assert_eq!(
        bytes
            .take_text_metered(&mut allocator, &mut meter.work())
            .unwrap_err(),
        Failure::Fault(refusal(2, 0, 3))
    );
    assert_eq!(identity(&mut bytes, &allocator), original);
    assert_eq!(meter.usage.service_work, 0);
    meter.control = Meter::new(3).control;
    let mut text = bytes
        .take_text_metered(&mut allocator, &mut meter.work())
        .unwrap();
    assert_eq!(meter.usage.service_work, 3);
    let mut token = text.take_token(&allocator).unwrap();
    let original = token;
    let mut meter = Meter::new(2);
    assert_eq!(
        Text::from_token_metered(&mut token, &allocator, &mut meter.work()).unwrap_err(),
        refusal(2, 0, 3)
    );
    assert_eq!(token, original);
    meter.control = Meter::new(3).control;
    let mut text = Text::from_token_metered(&mut token, &allocator, &mut meter.work()).unwrap();
    assert_eq!(token, [0; 32]);
    assert_eq!(text.as_str(&allocator), Ok("あ"));
    assert_eq!(meter.usage.service_work, 3);
    assert_eq!(trace.borrow().calls, requests);
    text.release(&mut allocator).unwrap();
    assert_eq!(meter.usage.service_work, 3);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn copies_precharge_full_lengths_and_comparison_uses_only_logical_bytes() {
    let (mut allocator, trace) = allocator(limits());
    let mut left = text(&mut allocator, "ab");
    let mut right = text(&mut allocator, "cd");
    let requests = trace.borrow().calls;
    let mut meter = Meter::new(3);
    assert_eq!(
        left.concat_metered(&right, &mut allocator, &mut meter.work())
            .unwrap_err(),
        Failure::Fault(refusal(3, 0, 4))
    );
    let mut meter = Meter::new(1);
    assert_eq!(
        left.try_clone_metered(&mut allocator, &mut meter.work())
            .unwrap_err(),
        Failure::Fault(refusal(1, 0, 2))
    );
    assert_eq!(trace.borrow().calls, requests);
    assert_eq!(allocator.stats().live_allocations, 2);
    assert_eq!(left.as_str(&allocator), Ok("ab"));
    assert_eq!(right.as_str(&allocator), Ok("cd"));

    let mut meter = Meter::new(4);
    let mut joined = left
        .concat_metered(&right, &mut allocator, &mut meter.work())
        .unwrap();
    assert_eq!(meter.usage.service_work, 4);
    assert_eq!(joined.as_str(&allocator), Ok("abcd"));
    let mut meter = Meter::new(0);
    assert_eq!(
        left.equal_metered(&joined, &allocator, &mut meter.work()),
        Ok(false)
    );
    assert_eq!(meter.usage.service_work, 0);
    assert_eq!(
        left.equal_metered(&right, &allocator, &mut meter.work()),
        Err(refusal(0, 0, 2))
    );
    meter.control = Meter::new(2).control;
    assert_eq!(
        left.equal_metered(&right, &allocator, &mut meter.work()),
        Ok(false)
    );
    assert_eq!(meter.usage.service_work, 2);
    for value in [&mut left, &mut right, &mut joined] {
        value.release(&mut allocator).unwrap();
    }
    assert_eq!(allocator.stats().live_allocations, 0);
}
