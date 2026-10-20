#!/usr/bin/env python3
"""Exercise real source breakpoints, stepping and stacks with a matching SDK."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import time


COPY = """fn debug_leaf(n:i64)->i64 {
    let mut value:i64 = n + 1;
    return value;
}
fn debug_outer(n:i64)->i64 {
    return debug_leaf(n);
}
fn main()->i64 {
    return debug_outer(41);
}
"""
OWNED = """fn debug_make(n:i64)->core::Bytes {
    let mut b:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut p:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(b,n));
    return b;
}
fn debug_take(b:core::Bytes)->i64 {
    let mut original:i64=core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(b,0));
    let mut value:i64 = original + 1;
    return value;
}
fn main()->i64 {
    return debug_take(debug_make(41));
}
"""


def execute(label, arguments, directory, logs, timeout=60):
    log = logs / f"{label}.log"
    started = time.monotonic()
    print(f"{label}: starting", flush=True)
    with log.open("wb") as stream:
        options = {"stdout": stream, "stderr": subprocess.STDOUT, "cwd": directory}
        if os.name == "nt":
            options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
        else:
            options["start_new_session"] = True
        child = subprocess.Popen([str(a) for a in arguments], **options)
        try:
            status = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(child.pid), "/T", "/F"],
                               stdout=stream, stderr=subprocess.STDOUT, timeout=15)
            else:
                os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=15)
            raise RuntimeError(f"{label} exceeded {timeout}s; log: {log}") from None
    result = log.read_text(encoding="utf-8", errors="replace")
    print(f"{label}: status {status}, {time.monotonic() - started:.2f}s; {log}", flush=True)
    if status:
        raise RuntimeError(f"{label} failed:\n{result[-12000:]}")
    return result


def quoted(path):
    return '"' + Path(path).as_posix().replace('"', '\\"') + '"'


def debugger_commands(source, package, root, line, step):
    return [
        "settings set use-color false",
        "settings set target.disable-aslr false",
        f"settings set target.source-map . {quoted(root)}",
        f"settings append target.source-map {quoted(package)} {quoted(root)}",
        f"breakpoint set --file {source.name} --line {line}",
        "run",
        "frame info",
        "thread backtrace -c 32",
        "breakpoint disable",
        step,
        "frame info",
        "thread backtrace -c 16",
        "continue",
    ]


def check_trace(text, profile, line, step):
    frames = re.findall(r"\(lldb\) frame info\s*\r?\n([^\r\n]*frame #0:[^\r\n]*)", text)
    if len(frames) != 2:
        raise RuntimeError(f"{profile}: expected two stopped frames:\n{text[-16000:]}")
    addresses = [int(re.search(r"0x([0-9a-fA-F]+)", frame).group(1), 16) for frame in frames]
    if addresses[0] == addresses[1]:
        raise RuntimeError(f"{profile}: debugger did not advance: {frames}")
    if not re.search(rf"{profile}\.lang:{line}(?::|\b)", frames[0]):
        raise RuntimeError(f"{profile}: wrong initial source location: {frames[0]}")
    if f"{profile}.lang:" not in frames[1]:
        raise RuntimeError(f"{profile}: step did not stay in native source code: {frames[1]}")
    first_stack = text.split("(lldb) thread backtrace -c 32", 1)[1].split("(lldb)", 1)[0]
    if profile == "copy":
        for expected in ["copy.lang:2", "copy.lang:6", "copy.lang:9"]:
            if expected not in first_stack:
                raise RuntimeError(f"copy: missing native caller {expected}:\n{first_stack}")
        if "copy.lang:3" not in frames[1]:
            raise RuntimeError(f"copy: source step did not reach return: {frames[1]}")
    elif "lang_codegen_clif" not in first_stack:
        raise RuntimeError(f"owned: missing production runtime caller:\n{first_stack}")
    if not re.search(r"(?m)^42\s*$", text) or not re.search(r"exited with status = 0", text):
        raise RuntimeError(f"{profile}: debugged program did not finish successfully:\n{text[-16000:]}")
    return {"profile": profile, "step": step, "before": frames[0], "after": frames[1],
            "pc_before": addresses[0], "pc_after": addresses[1], "stack": first_stack.strip(),
            "result": 42}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    sdk = args.sdk.resolve()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((sdk / "sdk.json").read_text(encoding="utf-8"))
    windows = os.name == "nt"
    compiler = sdk / "bin" / ("lang.exe" if windows else "lang")
    if windows:
        path = Path(r"C:\Program Files\LLVM\bin\lldb.exe")
        lldb = [str(path if path.is_file() else shutil.which("lldb.exe"))]
    else:
        lldb = ["xcrun", "lldb"]
    version = execute("debugger-version", lldb + ["--version"], root, root, 20)
    libraries = [sdk / f["path"] for f in manifest["files"] if f["path"].endswith(".rlib")]
    directories = sorted({(sdk / f["path"]).parent for f in manifest["files"]
                          if Path(f["path"]).suffix in {".rlib", ".lib", ".a"}})
    results = []
    for profile, source, line, step in [("copy", COPY, 2, "thread step-over"),
                                        ("owned", OWNED, 8, "thread step-inst")]:
        source_path = root / f"{profile}.lang"
        source_path.write_text(source, encoding="utf-8", newline="\n")
        package = root / profile
        execute(f"{profile}-produce", [compiler, "native", "--output", package, source_path], root, root)
        executable = package / ("debuggee.exe" if windows else "debuggee")
        command = ["rustc", "--edition=2024", f"--crate-name=lang_debug_{profile}",
                   "-Cpanic=unwind", "-Cdebuginfo=2", "-Csplit-debuginfo=packed",
                   package / "main.rs", "-o", executable]
        for directory in directories:
            command.extend(["-L", f"dependency={directory}", "-L", f"native={directory}"])
        for name in ["lang_codegen_clif", "lang_runtime", "lang_mir", "lang_bytecode"]:
            selected = [p for p in libraries if p.name.removeprefix("lib").startswith(name + "-")]
            if len(selected) != 1:
                raise RuntimeError(f"SDK must contain one {name} library: {selected}")
            command.extend(["--extern", f"{name}={selected[0]}"])
        command.extend(["-C", f"link-arg={package / ('program.obj' if windows else 'program.o')}"])
        execute(f"{profile}-debug-link", command, package, root, 180)
        ordinary = execute(f"{profile}-ordinary-run", [executable], root, root, 30)
        if ordinary.strip() != "42":
            raise RuntimeError(f"{profile}: wrong ordinary result: {ordinary}")
        commands = debugger_commands(source_path, package, root, line, step)
        debug = lldb + ["--batch", "--no-lldbinit", "--file", str(executable)]
        for command in commands:
            debug.extend(["-o", command])
        trace = execute(f"{profile}-debugger", debug, root, root, 90)
        results.append(check_trace(trace, profile, line, step))
    report = {"version": 1, "os": platform.system(), "architecture": platform.machine(),
              "sdk_build_id": manifest["build_id"], "debugger": version.strip(), "cases": results}
    (root / "debugger-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2), flush=True)


if __name__ == "__main__":
    main()
