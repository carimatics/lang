//! Exercise disk admission through real processes so a blocking open cannot hang libtest.
#[cfg(unix)]
mod unix {
    use std::io::Write;
    use std::process::{Command, Output, Stdio};
    use std::time::{Duration, Instant};

    struct Files(std::path::PathBuf);

    impl Drop for Files {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn bounded(mut command: Command, input: &[u8]) -> Output {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                return child.wait_with_output().unwrap();
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "file admission blocked: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn disk_inputs_refuse_fifos_without_waiting_for_a_writer() {
        let files =
            Files(std::env::temp_dir().join(format!("lang file admission {}", std::process::id())));
        std::fs::create_dir(&files.0).unwrap();
        let fifo = files.0.join("pipe.lang");
        assert!(
            Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        for (artifact, path) in [(false, fifo.clone()), (true, fifo.clone())] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_lang"));
            if artifact {
                command.args(["run", "--artifact"]);
            } else {
                command.arg("check");
            }
            command.arg(path);
            let output = bounded(command, b"");
            assert_eq!(output.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&output.stderr).contains("regular file"));
        }
        std::fs::write(
            files.0.join("source.lang"),
            "mod pipe; fn main()->i64{return 42;}",
        )
        .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_lang"));
        command.arg("check").arg(files.0.join("source.lang"));
        let output = bounded(command, b"");
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("regular file"));

        // The editor's disk provider must apply the same admission before parsing.
        let uri = lang::lsp::uri::file(&files.0.join("source.lang")).unwrap();
        let root = lang::lsp::uri::file(&files.0).unwrap();
        let messages = [
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":root,"capabilities":{}}}),
            serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
            serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"lang","version":1,"text":"mod pipe; fn main()->i64{return 42;}"}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}),
            serde_json::json!({"jsonrpc":"2.0","method":"exit","params":null}),
        ];
        let mut input = Vec::new();
        for message in messages {
            lang::lsp::wire::write(&mut input, &message, Default::default()).unwrap();
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_lang"));
        command.arg("lsp");
        let output = bounded(command, &input);
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("regular file"));
    }
}
