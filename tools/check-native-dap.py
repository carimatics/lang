"""Exercise actual native source breakpoints/steps through a bounded DAP client."""
import argparse
import os
import platform
import json
import queue
import subprocess
import threading
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--sdk", required=True, type=Path)
parser.add_argument("--output", required=True, type=Path)
args = parser.parse_args()
SDK = args.sdk.resolve()
ROOT = args.output.resolve()
WINDOWS = platform.system() == "Windows"
debugger_environment = None
if WINDOWS:
    debugger_environment = os.environ.copy()
    debugger_environment["LLDB_USE_NATIVE_PDB_READER"] = "1"
    adapter = Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "LLVM" / "bin" / "lldb-dap.exe"
    if not adapter.is_file():
        raise RuntimeError(f"required native DAP adapter is unavailable: {adapter}")
    debugger = [str(adapter)]
    runtime = os.environ.get("LLDB_PYTHON_RUNTIME")
    if runtime:
        home = Path(runtime).resolve().parent
        if not (home / "python310.dll").is_file():
            raise RuntimeError("LLVM 20 requires an x64 Python 3.10 runtime containing python310.dll")
        debugger_environment["PYTHONHOME"] = str(home)
        debugger_environment["PATH"] = str(home) + os.pathsep + debugger_environment.get("PATH", "")
else:
    debugger = ["xcrun", "lldb-dap"]
ROOT.mkdir(parents=True, exist_ok=False)
SOURCE = ROOT / "main.lang"
SOURCE.write_text("fn answer()->i64 {\n    return 41+1;\n}\nfn main()->i64 {\n    return answer();\n}\n", encoding="utf-8", newline="\n")
PACKAGE = ROOT / "program"
EXE = PACKAGE / ("debuggee.exe" if WINDOWS else "debuggee")
manifest = json.loads((SDK / "sdk.json").read_text())
selected = subprocess.run(["rustup", "which", "--toolchain", manifest["rustc"]["release"], "rustc"],
                          capture_output=True, text=True, timeout=20, check=True)
rustc = selected.stdout.strip()
version = subprocess.run([rustc, "-vV"], capture_output=True, text=True, timeout=20, check=True)
rustc_fields = dict(line.split(": ", 1) for line in version.stdout.splitlines() if ": " in line)
if (rustc_fields.get("release") != manifest["rustc"]["release"]
        or rustc_fields.get("commit-hash") != manifest["rustc"]["commit"]
        or rustc_fields.get("host") != manifest["target"]):
    raise RuntimeError(f"rustc does not match the native SDK: {version.stdout}")
compiler = SDK / next(f["path"] for f in manifest["files"] if f["role"] == "compiler")
produced = subprocess.run([str(compiler), "native", "--output", str(PACKAGE), str(SOURCE)], capture_output=True, text=True, timeout=60)
assert produced.returncode == 0, produced.stderr
libraries = [SDK / f["path"] for f in manifest["files"] if f["path"].endswith(".rlib")]
command = [rustc, "--edition=2024", "--crate-name=lang_native_dap_probe",
           "-Cpanic=unwind", "-Cdebuginfo=2", "-Csplit-debuginfo=packed",
           str(PACKAGE / "main.rs"), "-o", str(EXE)]
for directory in sorted({(SDK / f["path"]).parent for f in manifest["files"]}):
    command += ["-L", f"dependency={directory}", "-L", f"native={directory}"]
for name in ["lang_codegen_clif", "lang_runtime", "lang_mir", "lang_bytecode"]:
    selected = [p for p in libraries if p.name.removeprefix("lib").startswith(name + "-")]
    assert len(selected) == 1, (name, selected)
    command += ["--extern", f"{name}={selected[0]}"]
command += ["-C", f"link-arg={PACKAGE / ('program.obj' if WINDOWS else 'program.o')}"]
linked = subprocess.run(command, cwd=PACKAGE, capture_output=True, timeout=180)
assert linked.returncode == 0, linked.stderr.decode(errors="replace")
ordinary = subprocess.run([str(EXE)], capture_output=True, text=True, timeout=30)
assert ordinary.returncode == 0 and ordinary.stdout.strip() == "42", ordinary
print(json.dumps({"checkpoint": "ordinary-native", "result": ordinary.stdout.strip()}), flush=True)

process = subprocess.Popen(debugger, stdin=subprocess.PIPE,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=ROOT,
                           env=debugger_environment)
messages = queue.Queue(128)
errors = bytearray()

def read_messages():
    try:
        for _ in range(256):
            length = None
            used = 0
            while True:
                line = process.stdout.readline(8193)
                if not line:
                    messages.put(RuntimeError("adapter EOF"), timeout=5)
                    return
                used += len(line)
                assert used <= 8192 and line.endswith(b"\r\n") and line.isascii()
                if line == b"\r\n":
                    break
                name, value = line[:-2].split(b":", 1)
                if name.lower() == b"content-length":
                    assert length is None and value.strip().isdigit()
                    length = int(value.strip())
            assert length is not None and length <= 4 * 1024 * 1024
            body = bytearray()
            while len(body) < length:
                part = process.stdout.read(length-len(body))
                assert part
                body.extend(part)
            messages.put(json.loads(body), timeout=5)
        raise RuntimeError("message count limit")
    except BaseException as error:
        messages.put(error, timeout=5)

def read_errors():
    while len(errors) < 1024 * 1024:
        data = process.stderr.read(1024)
        if not data:
            return
        errors.extend(data)

reader = threading.Thread(target=read_messages, daemon=True)
stderr_reader = threading.Thread(target=read_errors, daemon=True)
reader.start()
stderr_reader.start()
sequence = 0
responses = {}
events = []

def source_path(frame):
    path = Path(frame.get("source", {}).get("path", "."))
    return (path if path.is_absolute() else ROOT / path).resolve()

def send(command, arguments):
    global sequence
    sequence += 1
    body = json.dumps({"seq": sequence, "type": "request", "command": command,
                       "arguments": arguments}).encode()
    process.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
    process.stdin.flush()
    return sequence

def receive(deadline):
    value = messages.get(timeout=max(0.001, deadline-time.monotonic()))
    if isinstance(value, BaseException):
        raise value
    if value["type"] == "response":
        responses[value["request_seq"]] = value
    elif value["type"] == "event":
        events.append(value)
    else:
        raise RuntimeError(f"unsupported reverse request: {value}")
    assert len(events) <= 128

def response(sequence):
    deadline = time.monotonic()+30
    while sequence not in responses:
        receive(deadline)
    value = responses.pop(sequence)
    assert value["success"], value
    return value.get("body", {})

def request(command, arguments):
    return response(send(command, arguments))

def event(name):
    deadline = time.monotonic()+30
    while True:
        for index, value in enumerate(events):
            if value.get("event") == name:
                return events.pop(index)
        receive(deadline)

completed = False
disconnected = False
try:
    caps = request("initialize", {"adapterID": "lang-native-probe", "linesStartAt1": True,
        "columnsStartAt1": True, "pathFormat": "path", "supportsRunInTerminalRequest": False})
    launch = send("launch", {"program": str(EXE), "cwd": str(ROOT), "stopOnEntry": False,
        "sourceMap": [[".",str(ROOT)]]})
    event("initialized")
    breakpoints = request("setBreakpoints", {"source": {"path": "main.lang"},
        "breakpoints": [{"line": 2}]})
    print(json.dumps({"checkpoint":"native-dap-configured","breakpoints":breakpoints}),flush=True)
    request("configurationDone", {})
    response(launch)
    stopped = event("stopped")
    stack = request("stackTrace", {"threadId": stopped["body"]["threadId"], "levels": 32})
    frames = stack["stackFrames"]
    assert any(source_path(f) == SOURCE and f["line"] == 2 for f in frames), frames
    guest_frames = [f for f in frames if source_path(f) == SOURCE]
    assert len(guest_frames) >= 2 and guest_frames[0]["line"] == 2 and guest_frames[1]["line"] == 5, guest_frames
    print(json.dumps({"checkpoint": "native-dap-breakpoint", "stopped": stopped["body"],
        "breakpoints": breakpoints, "frames": frames[:8]}), flush=True)
    request("next", {"threadId": stopped["body"]["threadId"]})
    stepped = event("stopped")
    stepped_stack = request("stackTrace", {"threadId": stepped["body"]["threadId"], "levels": 8})
    assert stepped["body"]["reason"] == "step", stepped
    assert any(source_path(f) == SOURCE and f["line"] == 5
               for f in stepped_stack["stackFrames"]), stepped_stack
    print(json.dumps({"checkpoint": "native-dap-step", "stopped": stepped["body"],
        "frames": stepped_stack["stackFrames"][:4]}), flush=True)
    request("continue", {"threadId": stepped["body"]["threadId"]})
    terminated = event("terminated")
    exited = event("exited")
    assert exited["body"]["exitCode"] == 0, exited
    request("disconnect", {"terminateDebuggee": True})
    disconnected = True
    report = {"version": 1, "os": platform.system(), "architecture": platform.machine(),
        "sdk_build_id": manifest["build_id"], "debugger": debugger, "rustc": rustc_fields,
        "breakpoints": breakpoints["breakpoints"], "frames": guest_frames,
        "step_frames": stepped_stack["stackFrames"], "exit_code": exited["body"]["exitCode"]}
    (ROOT / "native-dap-report.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
    print(json.dumps({"checkpoint": "native-dap-finished", "report": report}), flush=True)
    completed = True
finally:
    if not completed:
        print(json.dumps({"pending_native_events": events}), flush=True)
    if not disconnected and process.poll() is None:
        try:
            send("disconnect", {"terminateDebuggee": True})
        except (BrokenPipeError, OSError):
            pass
    try:
        process.stdin.close()
    except OSError:
        pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
    reader.join(timeout=5)
    stderr_reader.join(timeout=5)
    print(json.dumps({"adapter_exit_code": process.returncode,
                      "adapter_exit_hex": hex(process.returncode & 0xffffffff)}), flush=True)
    if errors:
        print(json.dumps({"adapter_stderr": errors.decode(errors="replace")[:4096]}), flush=True)
    if WINDOWS and not completed:
        inspector = adapter.parent / "llvm-readobj.exe"
        for binary in [adapter, adapter.parent / "liblldb.dll"]:
            if inspector.is_file() and binary.is_file():
                imports = subprocess.run([str(inspector), "--coff-imports", str(binary)],
                                         capture_output=True, text=True, timeout=20)
                print(json.dumps({"binary": str(binary), "imports": [line.strip()
                    for line in imports.stdout.splitlines() if line.strip().startswith("Name:")],
                    "inspect_status": imports.returncode}), flush=True)
        driver = adapter.with_name("lldb.exe")
        if driver.is_file():
            for query in ["target modules dump line-table main.lang",
                          "breakpoint set --file main.lang --line 2",
                          "image lookup -vrn lang_unit_f"]:
                metadata = subprocess.run([str(driver), "--batch", "--no-lldbinit", "--file", str(EXE), "-o", query],
                    cwd=ROOT, env=debugger_environment, capture_output=True, text=True, timeout=20)
                print(json.dumps({"metadata_query": query, "status": metadata.returncode,
                    "stdout": metadata.stdout[:16384], "stderr": metadata.stderr[:4096]}), flush=True)
    if completed:
        assert process.returncode == 0, f"native adapter failed to exit cleanly: {process.returncode}"

