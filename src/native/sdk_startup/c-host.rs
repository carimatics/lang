// Supply native_client in an explicitly compiled C host object using program.h.
// Copy beside bindings.rs; link that C object and the exact original guest object.
mod bindings;
unsafe extern "C" {
    fn native_client() -> i32;
}
fn main() {
    use lang_codegen_clif::sdk::runtime::{Input, with_c};
    let input = Input {
        target: bindings::TARGET,
        build_id: bindings::BUILD_ID,
        semantic: bindings::SEMANTIC,
        native: bindings::NATIVE,
    };
    // Strides describe byte extents, not frame/cell counts. The C configuration
    // supplies the actual finite limits, allocator and explicit import grants.
    let capacities =
        lang_runtime::c_abi::context::config::Capacities::from_strides(64, 16, 64).unwrap();
    // SAFETY: all symbols belong to this exact SDK/original linked image. The
    // synchronous C client stays on this owner thread, returns from every call,
    // and destroys every context before returning, even on its failure paths.
    // No raw client/context escapes this closure, including through unwind.
    let status = unsafe {
        with_c(
            input,
            &(),
            bindings::resolve,
            capacities,
            Default::default(),
            |_| native_client(),
        )
    }
    .unwrap();
    assert_eq!(status, 0, "explicit C host failed");
}
