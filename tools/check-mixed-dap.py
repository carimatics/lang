"""Independent exact-SDK Rust-host DAP fixture, including live failure cleanup."""
import argparse
import json
from pathlib import Path
import platform
import queue
import subprocess
import threading
import time
from urllib.parse import unquote, urlsplit

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--sdk", required=True, type=Path)
parser.add_argument("--output", required=True, type=Path)
args = parser.parse_args()
SDK = args.sdk.resolve()
ROOT = args.output.resolve()
REPO = Path(__file__).resolve().parents[1]
WINDOWS = platform.system() == "Windows"
manifest = json.loads((SDK / "sdk.json").read_text(encoding="utf-8"))
rustc = subprocess.run(["rustup", "which", "--toolchain", manifest["rustc"]["release"], "rustc"],
    capture_output=True, text=True, timeout=20, check=True).stdout.strip()
version = subprocess.run([rustc, "-vV"], capture_output=True, text=True, timeout=20, check=True)
fields = dict(line.split(": ", 1) for line in version.stdout.splitlines() if ": " in line)
assert (fields["release"], fields["commit-hash"], fields["host"]) == (
    manifest["rustc"]["release"], manifest["rustc"]["commit"], manifest["target"]), fields
ROOT.mkdir(parents=True, exist_ok=False)
HOST = ROOT / "host.rs"
EXE = ROOT / ("mixed-host.exe" if WINDOWS else "mixed-host")
HOST.write_text(r'''
use lang::dap::{self,mixed};
use std::sync::atomic::{AtomicU64,Ordering};
static ENTERED:AtomicU64=AtomicU64::new(0);
static RETURNED:AtomicU64=AtomicU64::new(0);
static DROPPED:AtomicU64=AtomicU64::new(0);
struct Lease;
impl Drop for Lease { fn drop(&mut self){DROPPED.fetch_add(1,Ordering::SeqCst);} }
#[inline(never)]
fn deep(n:u32)->bool {
    if n==0 {mixed::pause().is_ok()} else {
        let answer=deep(n-1);
        std::hint::black_box((n,answer)).1
    }
}
#[inline(never)]
extern "C" fn native_bridge(value:i64)->i64 {
    let _lease=Lease;
    let ordinal=ENTERED.fetch_add(1,Ordering::SeqCst)+1;
    let resumed=if value==300 {deep(300)} else {mixed::pause().is_ok()};
    if !resumed {return 0;}
    RETURNED.fetch_add(1,Ordering::SeqCst);
    value.wrapping_add(ordinal as i64)
}
fn main()->std::process::ExitCode {
    let report=std::env::args_os().nth(1).expect("explicit report path");
    let mut hosts=mixed::NativeHosts::new();
    hosts.register("native_bridge",native_bridge).unwrap();
    let result=dap::serve_stdio_with_native_hosts(hosts);
    let outside=mixed::pause()==Err(mixed::Error::OutsideNativeCall);
    let body=format!("{{\"entered\":{},\"returned\":{},\"dropped\":{},\"outside\":{}}}\n",
        ENTERED.load(Ordering::SeqCst),RETURNED.load(Ordering::SeqCst),DROPPED.load(Ordering::SeqCst),outside);
    std::fs::write(report,body).unwrap();
    match result {Ok(code)=>std::process::ExitCode::from(code),Err(e)=>{eprintln!("{e}");std::process::ExitCode::FAILURE}}
}
''', encoding="utf-8")
libraries = [f for f in manifest["files"] if Path(f["path"]).name.startswith("liblang-") and f["path"].endswith(".rlib")]
assert len(libraries) == 1, libraries
command = [rustc, "--edition=2024", "--crate-name=mixed_dap_host", "-Cpanic=unwind", "-Cdebuginfo=2",
    "-Copt-level=0", str(HOST), "--extern", f"lang={SDK / libraries[0]['path']}", "-o", str(EXE)]
for directory in sorted({(SDK / f["path"]).parent for f in manifest["files"]}):
    command += ["-L", f"dependency={directory}", "-L", f"native={directory}"]
linked = subprocess.run(command, capture_output=True, text=True, timeout=180)
assert linked.returncode == 0, linked.stderr


class Client:
    def __init__(self, report):
        self.process = subprocess.Popen([str(EXE), str(report)], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.messages = queue.Queue(32)
        self.events = []
        self.responses = {}
        self.sequence = 0
        self.last = -1
        self.errors = bytearray()
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.stderr = threading.Thread(target=self.read_errors, daemon=True)
        self.reader.start()
        self.stderr.start()

    def read(self):
        try:
            for _ in range(256):
                length = None
                used = 0
                while True:
                    line = self.process.stdout.readline(8193)
                    if not line:
                        self.messages.put(RuntimeError("adapter EOF"), timeout=5)
                        return
                    used += len(line)
                    assert used <= 8192 and line.endswith(b"\r\n") and line.isascii()
                    if line == b"\r\n":
                        break
                    name, value = line[:-2].split(b":", 1)
                    assert name.lower() == b"content-length" and length is None and value.strip().isdigit()
                    length = int(value.strip())
                assert length is not None and 0 < length <= 4 * 1024 * 1024
                body = bytearray()
                while len(body) < length:
                    part = self.process.stdout.read(length-len(body))
                    assert part
                    body.extend(part)
                self.messages.put(json.loads(body), timeout=5)
            raise RuntimeError("DAP message count limit")
        except BaseException as error:
            self.messages.put(error, timeout=5)

    def read_errors(self):
        while True:
            data = self.process.stderr.read(1024)
            if not data:
                return
            self.errors.extend(data[:max(0, 1024*1024-len(self.errors))])

    def receive(self, deadline):
        value = self.messages.get(timeout=max(0.001, deadline-time.monotonic()))
        if isinstance(value, BaseException):
            raise value
        assert value["seq"] > self.last and value.get("jsonrpc") is None, value
        self.last = value["seq"]
        if value["type"] == "response":
            self.responses[value["request_seq"]] = value
        elif value["type"] == "event":
            assert len(self.events) < 128
            self.events.append(value)
        else:
            raise RuntimeError(f"unsupported reverse request: {value}")

    def request(self, command, arguments, success=True):
        self.sequence += 1
        sequence = self.sequence
        body = json.dumps({"seq": sequence, "type": "request", "command": command, "arguments": arguments}).encode()
        self.process.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode()+body)
        self.process.stdin.flush()
        deadline = time.monotonic()+30
        while sequence not in self.responses:
            self.receive(deadline)
        response = self.responses.pop(sequence)
        assert response["command"] == command and response["success"] is success, response
        return response.get("body", {})

    def event(self, name):
        deadline = time.monotonic()+30
        while True:
            for i, value in enumerate(self.events):
                if value["event"] == name:
                    return self.events.pop(i)
            self.receive(deadline)

    def finish(self):
        self.request("disconnect", {})
        self.process.wait(timeout=10)
        assert self.process.returncode == 0
        self.reader.join(timeout=5)
        self.stderr.join(timeout=5)
        assert not self.errors, self.errors.decode(errors="replace")

    def cleanup(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=5)


def source_path(frame):
    value = frame["source"]["path"]
    if value.startswith("file:"):
        value = unquote(urlsplit(value).path)
        if WINDOWS and value.startswith("/") and value[2:3] == ":":
            value = value[1:]
    return Path(value).resolve()


evidence = []
for case in ["continue", "disconnect", "capture-limit"]:
    directory = ROOT / case
    directory.mkdir()
    source = (REPO / "examples/packages/debug_mixed/main.lang").read_text(encoding="utf-8")
    if case == "capture-limit":
        source = source.replace("child(40)", "child(300)")
    (directory / "main.lang").write_text(source, encoding="utf-8", newline="\n")
    (directory / "Lang.json").write_bytes((REPO / "examples/packages/debug_mixed/Lang.json").read_bytes())
    report = directory / "owners.json"
    client = Client(report)
    try:
        client.request("initialize", {"linesStartAt1": False, "columnsStartAt1": False, "pathFormat": "uri"})
        client.request("launch", {"mode": "mixed", "root": str(directory), "manifestPath": "Lang.json"})
        assert client.event("capabilities")["body"]["capabilities"]["supportsEvaluateForHovers"] is False
        client.event("initialized")
        (directory / "main.lang").write_text("invalid after captured launch\n", encoding="utf-8")
        client.request("configurationDone", {})
        frames = []
        if case != "capture-limit":
            assert client.event("stopped")["body"]["reason"] == "pause"
            stack = client.request("stackTrace", {"threadId": 1, "levels": 256})
            frames = stack["stackFrames"]
            native = [f for f in frames if "instructionPointerReference" in f]
            vm = [f for f in frames if "source" in f]
            assert native and len(frames) <= 256
            assert all("source" not in f for f in native)
            assert frames == native+vm and [f["name"] for f in vm] == ["child", "main"], frames
            assert [f["line"] for f in vm] == [3, 7], vm
            assert all(source_path(f) == directory / "main.lang" for f in vm)
            frozen = client.request("source", {"sourceReference": vm[0]["source"]["sourceReference"]})
            assert frozen["content"] == source
            page = client.request("stackTrace", {"threadId": 1, "startFrame": len(native), "levels": 2})
            assert page["stackFrames"] == vm
            client.request("scopes", {"frameId": vm[0]["id"]}, success=False)
            client.request("stepIn", {"threadId": 1}, success=False)
        if case == "continue":
            client.request("continue", {"threadId": 1})
            output = client.event("output")
            assert output["body"]["output"] == "42\n" and output["body"]["data"]["fuel"] == 7, output
            assert client.event("exited")["body"]["exitCode"] == 0
            client.event("terminated")
        elif case == "capture-limit":
            output = client.event("output")
            assert "CaptureLimit" in output["body"]["output"], output
            assert client.event("exited")["body"]["exitCode"] == 1
            client.event("terminated")
            assert all(e["event"] != "stopped" for e in client.events), client.events
        client.finish()
        owners = json.loads(report.read_text(encoding="utf-8"))
        assert owners == {"entered": 1, "returned": int(case == "continue"), "dropped": 1, "outside": True}, owners
        evidence.append({"case": case, "frames": frames, "owners": owners, "adapter_exit": 0})
        print(json.dumps({"checkpoint": "mixed-dap-case", "case": case, "frames": len(frames), "owners": owners}), flush=True)
    finally:
        client.cleanup()
(ROOT / "mixed-dap-report.json").write_text(json.dumps({"version": 1, "os": platform.system(),
    "architecture": platform.machine(), "sdk_build_id": manifest["build_id"], "cases": evidence}, indent=2)+"\n", encoding="utf-8")
