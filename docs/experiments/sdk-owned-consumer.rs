//! Readiness experiment for one original linked object at 2712700.
use std::{ffi::c_void, fs, ptr::NonNull};
use lang_codegen_clif::{native_record::Record, owned as n};
use lang_runtime::{allocation as a, cleanup as c, layout as l};

unsafe extern "C" {
    #[link_name = "lang_owned_r1_f0"]
    static ENTRY: u8;
}
fn resolve(_: &(), symbol: &str) -> Option<NonNull<c_void>> {
    (symbol == "lang_owned_r1_f0").then(|| {
        // SAFETY: this symbol is from the original guest.o linked into this
        // process. Take only its address; never read it as a data object.
        NonNull::new(unsafe { &raw const ENTRY }.cast_mut().cast()).unwrap()
    })
}
fn main() {
    let semantic = fs::read("guest.lbc").unwrap();
    let native = fs::read("guest.native").unwrap();
    let program = lang_bytecode::ownership::decode(&semantic, Default::default()).unwrap();
    let flow = program.flow();
    let types = flow.source().activity().moved().typed().places().declarations().types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let object = n::RecordedObject::from_record(Record::decode(&native, &semantic, Default::default()).unwrap()).unwrap()
        .prepare(flow, &layouts, Default::default(), 4096).unwrap();
    // SAFETY: unchanged original guest.o, exact static symbol, matching Rust
    // build/toolchain; process-owned code and dependencies live through main.
    let linked = unsafe { object.bind((), resolve, Default::default()) }.unwrap();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let outcome = n::runtime::execute_entry(&linked, program.entry().unwrap(),
        &mut c::Instance::default(), &mut allocator, Default::default()).unwrap();
    assert_eq!(outcome.value, 42);
    assert!(allocator.stats().allocations > 0);
    assert_eq!(allocator.stats().allocations, allocator.stats().deallocations);
    assert_eq!(allocator.stats().live_allocations, 0);
    println!("{}; live allocations {}", outcome.value, allocator.stats().live_allocations);
}
