//! Trusted Rust embedding fixture: no callbacks are registered by source.
use lang::dap::{self, mixed};

#[inline(never)]
extern "C" fn native_bridge(value: i64) -> i64 {
    if mixed::pause().is_err() {
        return 0;
    }
    value.wrapping_add(1)
}

fn main() -> std::process::ExitCode {
    let mut hosts = mixed::NativeHosts::new();
    if let Err(error) = hosts.register("native_bridge", native_bridge) {
        eprintln!("{error}");
        return std::process::ExitCode::FAILURE;
    }
    match dap::serve_stdio_with_native_hosts(hosts) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
