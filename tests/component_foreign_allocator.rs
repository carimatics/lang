use lang_codegen_clif::c_abi::allocator::ForeignProvider;
use lang_runtime::{
    allocation::{self as a, Provider, Region},
    c_abi::context::config,
};
use std::{
    alloc::{Layout, alloc, dealloc},
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};

#[derive(Default)]
struct Owner {
    fail: Cell<bool>,
    allocated: RefCell<Vec<(usize, u64, u64)>>,
    freed: RefCell<Vec<(usize, u64, u64)>>,
}
unsafe extern "C" fn allocate(user: *mut c_void, size: u64, alignment: u64) -> *mut c_void {
    // SAFETY: each fixture retains this Owner through all callback returns.
    let owner = unsafe { &*user.cast::<Owner>() };
    if owner.fail.get() {
        return ptr::null_mut();
    }
    let layout = Layout::from_size_align(size as usize, alignment as usize).unwrap();
    // SAFETY: the checked positive Request supplies a nonzero valid Layout.
    let address = unsafe { alloc(layout) };
    if !address.is_null() {
        // SAFETY: this allocation owns every byte in the returned range.
        unsafe { address.write_bytes(0xa5, size as usize) };
        owner
            .allocated
            .borrow_mut()
            .push((address as usize, size, alignment));
    }
    address.cast()
}
unsafe extern "C" fn release(user: *mut c_void, data: *mut c_void, size: u64, alignment: u64) {
    // SAFETY: Owner and this exact original allocation remain live until here.
    let owner = unsafe { &*user.cast::<Owner>() };
    let record = (data as usize, size, alignment);
    assert!(owner.allocated.borrow().contains(&record));
    assert!(!owner.freed.borrow().contains(&record));
    owner.freed.borrow_mut().push(record);
    // SAFETY: the original pointer/Layout is uniquely retired exactly once.
    unsafe {
        dealloc(
            data.cast(),
            Layout::from_size_align(size as usize, alignment as usize).unwrap(),
        )
    };
}
fn descriptor(owner: &Owner) -> config::Allocator {
    config::Allocator {
        version: 1,
        size: std::mem::size_of::<config::Allocator>() as u32,
        user: ptr::from_ref(owner).cast_mut().cast(),
        allocate: Some(allocate),
        deallocate: Some(release),
    }
}

#[test]
fn common_foreign_storage_keeps_original_callbacks_requests_and_initialized_bytes() {
    let first = Owner::default();
    let second = Owner::default();
    // SAFETY: both owners/callbacks outlive these providers and all regions;
    // allocate/release satisfy the unique storage, no-unwind/no-reentry contract.
    let mut original = unsafe { ForeignProvider::new(descriptor(&first)) }.unwrap();
    // SAFETY: the same lifetime/storage contract holds for second.
    let mut other = unsafe { ForeignProvider::new(descriptor(&second)) }.unwrap();
    let request = a::Request::new(33, 32).unwrap();
    let mut region = original.allocate(request).unwrap();
    assert_eq!(region.bytes(), &[0; 33]);
    assert_eq!((region.bytes().as_ptr() as usize) % 32, 0);
    region.bytes_mut()[0] = 42;
    assert_eq!(region.bytes()[0], 42);
    other.deallocate(region, a::Request::new(17, 4).unwrap());
    assert_eq!(*first.allocated.borrow(), *first.freed.borrow());
    assert!(second.allocated.borrow().is_empty());
    assert!(second.freed.borrow().is_empty());
    let zero = original.allocate(a::Request::new(0, 8).unwrap()).unwrap();
    assert!(zero.bytes().is_empty());
    original.deallocate(zero, request);
    assert_eq!(first.allocated.borrow().len(), 1);
    first.fail.set(true);
    assert!(original.allocate(request).is_none());
    assert_eq!(first.allocated.borrow().len(), 1);
    for which in 0..4 {
        let mut invalid = descriptor(&first);
        match which {
            0 => invalid.version = 2,
            1 => invalid.size -= 1,
            2 => invalid.allocate = None,
            _ => invalid.deallocate = None,
        }
        // SAFETY: descriptors are owned initialized fields; rejection performs
        // no callbacks, and first remains live for the complete expression.
        assert!(unsafe { ForeignProvider::new(invalid) }.is_none());
    }
}
