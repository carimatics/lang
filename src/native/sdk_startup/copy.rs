// Copy beside the package's bindings.rs; link the original object with this SDK.
mod bindings;
fn main() -> std::process::ExitCode {
    use lang_codegen_clif::sdk::runtime::{Input, execute_copy};
    let input = Input {
        target: bindings::TARGET,
        build_id: bindings::BUILD_ID,
        semantic: bindings::SEMANTIC,
        native: bindings::NATIVE,
    };
    // SAFETY: build/link retains the exact original object, SDK and all native
    // relocation dependencies. Their symbols remain live through the call.
    match unsafe {
        execute_copy(
            input,
            &(),
            bindings::resolve,
            Default::default(),
            Default::default(),
        )
    } {
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
