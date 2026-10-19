//! Real native tools verify linker diagnostics and output handling in both
//! command forms. The same source runs on the two tier-1 CI hosts.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
use lang::linker::{LinkError, LinkRequest, SystemLinker};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang linker errors {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn compile_main(directory: &Path) -> PathBuf {
    let source = directory.join("main.c");
    let object = directory.join("main.obj");
    fs::write(&source, "int main(void) { return 23; }\n").unwrap();
    let output = if cfg!(target_os = "windows") {
        Command::new("cl")
            .current_dir(directory)
            .args(["/nologo", "/std:c11", "/W4", "/WX", "/c"])
            .arg(&source)
            .arg(format!("/Fo{}", object.display()))
            .output()
            .unwrap()
    } else {
        Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
            .arg(&source)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap()
    };
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    object
}

#[test]
fn native_linker_replaces_an_existing_output_in_both_request_forms() {
    let directory = Directory::new();
    let object = compile_main(&directory.0);
    let linker = SystemLinker::discover_host().unwrap();
    for response in [false, true] {
        let executable = directory.0.join(if response {
            "response.exe"
        } else {
            "direct.exe"
        });
        fs::write(&executable, b"previous output").unwrap();
        let request = LinkRequest::new(&executable).with_object(&object);
        if response {
            linker
                .link_with_response_file(&request, directory.0.join("link args.rsp"))
                .unwrap();
        } else {
            linker.link(&request).unwrap();
        }
        assert_ne!(fs::read(&executable).unwrap(), b"previous output");
        assert_eq!(Command::new(&executable).status().unwrap().code(), Some(23));
        assert!(fs::metadata(&object).unwrap().is_file());
    }
}

#[test]
fn native_tool_failures_retain_status_command_and_diagnostics() {
    let directory = Directory::new();
    let object = directory.0.join("invalid.obj");
    fs::write(&object, b"not an object file\n").unwrap();
    let linker = SystemLinker::discover_host().unwrap();
    for response in [false, true] {
        let executable = directory.0.join(if response {
            "bad-response.exe"
        } else {
            "bad-direct.exe"
        });
        let request = LinkRequest::new(&executable).with_object(&object);
        let error = if response {
            linker
                .link_with_response_file(&request, directory.0.join("failed args.rsp"))
                .unwrap_err()
        } else {
            linker.link(&request).unwrap_err()
        };
        let message = error.to_string();
        let LinkError::Failed {
            command,
            output,
            status,
            stdout,
            stderr,
        } = error
        else {
            panic!("expected a native-tool failure: {message}");
        };
        assert_eq!(output, executable);
        assert!(!status.success());
        assert!(!stdout.is_empty() || !stderr.is_empty());
        assert!(message.contains("stdout:") && message.contains("stderr:"));
        assert!(message.contains(executable.file_name().unwrap().to_str().unwrap()));
        assert_eq!(command.program(), linker.program());
        assert!(!executable.exists());
        assert_eq!(fs::read(&object).unwrap(), b"not an object file\n");
    }
}
