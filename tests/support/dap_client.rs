//! Independent bounded DAP consumer shared by stdio and installed-tool journeys.
pub struct DapClient {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    messages: std::sync::mpsc::Receiver<serde_json::Value>,
    reader: Option<std::thread::JoinHandle<()>>,
    sequence: u32,
    last_sequence: u64,
    terminated: bool,
}
impl DapClient {
    pub fn new(executable: &std::path::Path) -> Self {
        use std::{
            io::{BufRead, Read},
            process::{Command, Stdio},
        };
        let mut child = Command::new(executable)
            .arg("dap")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (send, messages) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut input = std::io::BufReader::new(stdout);
            for _ in 0..128 {
                let mut length = None;
                let mut used = 0usize;
                loop {
                    let mut line = Vec::new();
                    let read = input
                        .by_ref()
                        .take(8193)
                        .read_until(b'\n', &mut line)
                        .unwrap();
                    if read == 0 {
                        return;
                    }
                    used += read;
                    assert!(used <= 8192 && line.ends_with(b"\r\n") && line.is_ascii());
                    if line == b"\r\n" {
                        break;
                    }
                    let header = std::str::from_utf8(&line).unwrap();
                    let (name, value) = header.trim_end().split_once(':').unwrap();
                    assert!(name.eq_ignore_ascii_case("Content-Length"));
                    assert!(length.is_none());
                    let value = value.trim();
                    assert!(!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()));
                    length = Some(value.parse::<usize>().unwrap());
                }
                let length = length.unwrap();
                assert!(length <= 4 * 1024 * 1024);
                let mut bytes = vec![0; length];
                input.read_exact(&mut bytes).unwrap();
                if send.send(serde_json::from_slice(&bytes).unwrap()).is_err() {
                    return;
                }
            }
            panic!("excessive DAP messages");
        });
        Self {
            child,
            input,
            messages,
            reader: Some(reader),
            sequence: 0,
            last_sequence: 0,
            terminated: false,
        }
    }
    pub fn receive(&mut self) -> serde_json::Value {
        let message = self
            .messages
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let seq = message["seq"].as_u64().unwrap();
        assert!(seq > self.last_sequence);
        self.last_sequence = seq;
        self.terminated |= message["event"] == "terminated";
        assert!(message.get("jsonrpc").is_none());
        message
    }
    pub fn request(&mut self, command: &str, args: serde_json::Value) -> serde_json::Value {
        use std::io::Write;
        self.sequence += 1;
        let bytes = serde_json::to_vec(&serde_json::json!({"seq":self.sequence,"type":"request",
            "command":command,"arguments":args}))
        .unwrap();
        let input = self.input.as_mut().unwrap();
        write!(input, "Content-Length: {}\r\n\r\n", bytes.len()).unwrap();
        input.write_all(&bytes).unwrap();
        input.flush().unwrap();
        let message = self.receive();
        assert_eq!(message["type"], "response", "{message:?}");
        assert_eq!(message["request_seq"], self.sequence);
        assert_eq!(message["command"], command);
        assert_eq!(message["success"], true, "{message:?}");
        message["body"].clone()
    }
    pub fn finish(mut self) {
        self.request("disconnect", serde_json::json!({}));
        if !self.terminated {
            assert_eq!(self.receive()["event"], "terminated");
        }
        self.input.take();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        self.reader.take().unwrap().join().unwrap();
        use std::io::Read;
        let mut errors = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut errors)
            .unwrap();
        assert!(errors.is_empty(), "{errors}");
    }
}
impl Drop for DapClient {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
