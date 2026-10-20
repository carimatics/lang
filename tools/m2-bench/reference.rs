#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn rust_scalar(n: i64) -> i64 {
    let mut value = 17;
    for index in 0..n {
        value = if value < 500_003 {
            value + value + index
        } else {
            value + (-500_003) + index
        };
    }
    value
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn rust_buffers(n: i64) -> i64 {
    let mut bytes = Vec::new();
    let mut values = Vec::new();
    let mut value = 0_u8;
    for index in 0..n {
        bytes.push(value);
        values.push(index + i64::from(value));
        value += 1;
        if value > 249 {
            value = 0;
        }
    }
    std::hint::black_box((&bytes, &values));
    let mut sum = 0;
    for index in 0..n as usize {
        sum += i64::from(bytes[index]) + values[index];
    }
    sum
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn rust_regions(n: i64) -> i64 {
    use std::rc::Rc;
    let mut sum = 0;
    for index in 0..n {
        let owner = Rc::new(index);
        let observer = Rc::downgrade(&owner);
        {
            let retained = Rc::clone(&owner);
            let arena = vec![Box::new(index + 1)];
            std::hint::black_box((&observer, &retained, &arena));
            drop(owner);
            sum += *retained + *arena[0];
        }
        if observer.upgrade().is_some() {
            return -4;
        }
        sum += 1;
    }
    sum
}
