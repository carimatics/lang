#![no_main]
use lang_runtime::{
    allocation as a,
    buffer::{AllocError, BoundsError, BufferError, Bytes, Failure, Utf8Error},
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let byte = |i: usize| input.get(i).copied().unwrap_or(0);
    let mut allocator = a::Allocator::new(
        a::System,
        a::Limits {
            request_bytes: 512,
            live_bytes: 32 + u64::from(byte(0)),
            cumulative_bytes: 64 + u64::from(byte(1)),
            live_allocations: 4,
            ..Default::default()
        },
    )
    .unwrap();
    let initial = &input[..input.len().min(16)];
    let mut bytes = match Bytes::from_slice(&mut allocator, initial) {
        Ok(bytes) => bytes,
        Err(Failure::Error(AllocError::Limit | AllocError::OutOfMemory | AllocError::Overflow)) => {
            return;
        }
        Err(error) => panic!("unexpected buffer construction failure: {error:?}"),
    };
    let mut oracle = initial.to_vec();
    for operation in input
        .get(16..)
        .unwrap_or(&[])
        .as_chunks::<3>()
        .0
        .iter()
        .take(128)
    {
        let index = i64::from(operation[1] as i8);
        let value = i64::from(operation[2]) - if operation[0] & 128 != 0 { 128 } else { 0 };
        match operation[0] % 4 {
            0 => match bytes.push(&mut allocator, value) {
                Ok(()) => {
                    assert!((0..=255).contains(&value));
                    oracle.push(value as u8);
                }
                Err(Failure::Error(BufferError::InvalidByte)) => {
                    assert!(!(0..=255).contains(&value))
                }
                Err(Failure::Error(BufferError::Allocation(_))) => {
                    assert!((0..=255).contains(&value))
                }
                Err(error) => panic!("unexpected push failure: {error:?}"),
            },
            1 => {
                let valid = usize::try_from(index).ok().filter(|&i| i < oracle.len());
                let expected = if let Some(index) = valid {
                    if let Ok(value) = u8::try_from(value) {
                        oracle[index] = value;
                        Ok(())
                    } else {
                        Err(Failure::Error(BufferError::InvalidByte))
                    }
                } else {
                    Err(Failure::Error(BufferError::Bounds))
                };
                assert_eq!(bytes.set(&mut allocator, index, value), expected);
            }
            2 => {
                let expected = usize::try_from(index)
                    .ok()
                    .and_then(|index| oracle.get(index))
                    .map(|&b| i64::from(b))
                    .ok_or(Failure::Error(BoundsError::OutOfBounds));
                assert_eq!(bytes.get(&allocator, index), expected);
            }
            _ => match bytes.try_clone(&mut allocator) {
                Ok(mut cloned) => {
                    assert_eq!(cloned.as_slice(&allocator).unwrap(), oracle);
                    cloned.release(&mut allocator).unwrap();
                }
                Err(Failure::Error(
                    AllocError::Limit | AllocError::OutOfMemory | AllocError::Overflow,
                )) => {}
                Err(error) => panic!("unexpected clone failure: {error:?}"),
            },
        }
        assert_eq!(bytes.as_slice(&allocator).unwrap(), oracle);
        assert_eq!(bytes.len(&allocator).unwrap(), oracle.len() as i64);
    }
    let mut token = bytes.take_token(&allocator).unwrap();
    let mut corrupt = token;
    corrupt[usize::from(byte(2)) % 32] ^= byte(3).max(1);
    let before = corrupt;
    assert!(Bytes::from_token(&mut corrupt, &allocator).is_err());
    assert_eq!(corrupt, before);
    bytes = Bytes::from_token(&mut token, &allocator).unwrap();
    assert_eq!(token, [0; 32]);
    let requests = allocator.stats().requests;
    match std::str::from_utf8(&oracle) {
        Ok(expected) => {
            let mut text = bytes.take_text(&mut allocator).unwrap();
            assert_eq!(text.as_str(&allocator).unwrap(), expected);
            text.release(&mut allocator).unwrap();
        }
        Err(error) => assert!(
            matches!(bytes.take_text(&mut allocator),Err(Failure::Error(Utf8Error::Invalid(offset))) if offset==error.valid_up_to()as i64)
        ),
    }
    bytes.release(&mut allocator).unwrap();
    assert_eq!(allocator.stats().requests, requests);
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
});
