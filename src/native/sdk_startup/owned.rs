// Copy beside bindings.rs; this is an ordinary ownership entry, not a C host.
mod bindings;
fn run() -> Result<i64, String> {
    use lang_codegen_clif::{
        owned::runtime::Config,
        sdk::runtime::{Input, execute_owned},
    };
    use lang_runtime::{allocation as a, cleanup::Instance};
    let input = Input {
        target: bindings::TARGET,
        build_id: bindings::BUILD_ID,
        semantic: bindings::SEMANTIC,
        native: bindings::NATIVE,
    };
    let mut allocator =
        a::Allocator::new(a::System, Default::default()).map_err(|e| e.to_string())?;
    let mut instance = Instance::default();
    let config = Config {
        limits: lang_mir::unit::Limits {
            allocations: 512 * 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: link retains the exact original object/SDK and every relocation
    // dependency for the complete synchronous invocation.
    unsafe {
        execute_owned(
            input,
            &(),
            bindings::resolve,
            &mut instance,
            &mut allocator,
            config,
            Default::default(),
        )
    }
    .map(|v| v.value)
    .map_err(|e| e.to_string())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
