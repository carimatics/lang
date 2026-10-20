#[cfg(any(source, parity))]
mod bindings;
#[cfg(any(rust, parity))]
mod reference;

unsafe extern "C" {
    fn benchmark_begin() -> i32;
    fn benchmark_call(workload: i32, n: i64, out: *mut i64) -> i32;
    fn benchmark_end() -> i32;
    fn benchmark_stats(out: *mut HeapStats);
    fn benchmark_peak_memory() -> u64;
    #[cfg(parity)]
    fn native_parity() -> i32;
}

#[repr(C)]
#[derive(Default)]
struct HeapStats {
    allocations: u64,
    releases: u64,
    requested_bytes: u64,
    live_bytes: u64,
    peak_live_bytes: u64,
}

struct Session(bool);
impl Session {
    fn begin() -> Self {
        // SAFETY: the single owner thread is inside with_c for source builds;
        // reference builds do not create a language context.
        assert_eq!(unsafe { benchmark_begin() }, 0, "context create failed");
        Self(true)
    }
    fn finish(mut self) -> i32 {
        self.0 = false;
        // SAFETY: the context is live, and no call or borrowed output remains.
        unsafe { benchmark_end() }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: unwind still occurs inside with_c on the owner thread.
            // No context escapes even if a measured call or assertion fails.
            unsafe { benchmark_end() };
        }
    }
}

fn measure(workload: i32, n: i64, repeats: usize, preparation: Option<std::time::Instant>) {
    let session = Session::begin();
    let preparation_ns = preparation.map_or(0, |t| t.elapsed().as_nanos());
    let call = |n: i64| {
        let mut out = 0;
        // SAFETY: this synchronous call uses the live owner-thread session and
        // writes only this valid i64 output. n has been checked by main.
        let status = unsafe { benchmark_call(workload, std::hint::black_box(n), &mut out) };
        assert_eq!(status, 0, "workload failed");
        std::hint::black_box(out)
    };
    let expected = call(n);
    for _ in 0..2 {
        assert_eq!(call(n), expected, "warmup changed output");
    }
    let start = std::time::Instant::now();
    let mut checksum = 0_i64;
    for _ in 0..repeats {
        checksum = checksum.wrapping_add(call(n));
    }
    let elapsed_ns = start.elapsed().as_nanos();
    assert_eq!(checksum, expected.wrapping_mul(repeats as i64));
    assert_eq!(session.finish(), 0, "context/allocator retirement failed");
    emit(preparation_ns, elapsed_ns, checksum, expected);
}

fn startup(preparation: Option<std::time::Instant>) {
    let session = Session::begin();
    let preparation_ns = preparation.map_or(0, |t| t.elapsed().as_nanos());
    assert_eq!(session.finish(), 0, "startup retirement failed");
    emit(preparation_ns, 0, 0, 0);
}

fn emit(preparation_ns: u128, elapsed_ns: u128, checksum: i64, expected: i64) {
    let mut stats = HeapStats::default();
    // SAFETY: a complete, properly aligned output lives through the call.
    unsafe { benchmark_stats(&mut stats) };
    // SAFETY: reads only the current process's OS memory counters.
    let peak_working_set_bytes = unsafe { benchmark_peak_memory() };
    println!(
        "{{\"preparation_ns\":{},\"elapsed_ns\":{},\"checksum\":{},\"result\":{},\"allocations\":{},\"releases\":{},\"requested_bytes\":{},\"live_bytes\":{},\"peak_live_bytes\":{},\"peak_working_set_bytes\":{}}}",
        preparation_ns,
        elapsed_ns,
        checksum,
        expected,
        stats.allocations,
        stats.releases,
        stats.requested_bytes,
        stats.live_bytes,
        stats.peak_live_bytes,
        peak_working_set_bytes
    );
}

#[cfg(any(source, parity))]
fn with_source(body: impl FnOnce()) {
    use lang_codegen_clif::sdk::runtime::{Input, with_c};
    let input = Input {
        target: bindings::TARGET,
        build_id: bindings::BUILD_ID,
        semantic: bindings::SEMANTIC,
        native: bindings::NATIVE,
    };
    let capacities =
        lang_runtime::c_abi::context::config::Capacities::from_strides(64, 16, 64).unwrap();
    // SAFETY: these are the exact original linked symbols. All synchronous C
    // calls and contexts end inside this scope on this owner thread. Session's
    // Drop also destroys a live context during unwind; no pointer escapes.
    unsafe {
        with_c(
            input,
            &(),
            bindings::resolve,
            capacities,
            Default::default(),
            |_| body(),
        )
    }
    .unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["startup"] {
        #[cfg(any(source, parity))]
        {
            let start = std::time::Instant::now();
            with_source(|| startup(Some(start)));
        }
        #[cfg(not(any(source, parity)))]
        startup(None);
        return;
    }
    #[cfg(source)]
    if args == ["unwind-probe"] {
        let result = std::panic::catch_unwind(|| {
            with_source(|| {
                let _session = Session::begin();
                panic!("intentional benchmark unwind probe");
            });
        });
        assert!(result.is_err());
        let mut stats = HeapStats::default();
        // SAFETY: this valid output is filled after the unwound context ended.
        unsafe { benchmark_stats(&mut stats) };
        assert_eq!(stats.allocations, stats.releases);
        assert_eq!(stats.live_bytes, 0);
        // A second scope proves that the first registration/context retired.
        with_source(|| {
            let session = Session::begin();
            let mut out = 0;
            // SAFETY: a live owner-thread context and valid local output.
            assert_eq!(unsafe { benchmark_call(0, 1, &mut out) }, 0);
            assert_eq!(out, 34);
            assert_eq!(session.finish(), 0);
        });
        println!("unwind retirement ok");
        return;
    }
    #[cfg(parity)]
    if args == ["parity"] {
        with_source(|| {
            // SAFETY: native_parity owns/destroys its context in this scope.
            assert_eq!(unsafe { native_parity() }, 0, "source/Rust/C parity failed");
        });
        return;
    }
    assert_eq!(args.len(), 3, "usage: WORKLOAD N REPEATS");
    let workload = match args[0].as_str() {
        "scalar" => 0,
        "buffers" => 1,
        "regions" => 2,
        _ => panic!("unknown workload"),
    };
    let n: i64 = args[1].parse().unwrap();
    let repeats: usize = args[2].parse().unwrap();
    assert!((0..=100_000).contains(&n));
    assert!((1..=10_000).contains(&repeats));
    #[cfg(any(source, parity))]
    {
        let start = std::time::Instant::now();
        with_source(|| measure(workload, n, repeats, Some(start)));
    }
    #[cfg(not(any(source, parity)))]
    measure(workload, n, repeats, None);
}
