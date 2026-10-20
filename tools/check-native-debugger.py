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
fn debug_outer(b:core::Bytes)->i64 {
    return debug_take(b);
}
fn main()->i64 {
    return debug_outer(debug_make(41));
}
"""

LLDB_STACK = """
import lldb, struct
thread = lldb.debugger.GetSelectedTarget().GetProcess().GetSelectedThread()
view = next((f.FindVariable("debug_view") for f in thread
             if f.FindVariable("debug_view").IsValid()), None)
assert view is not None, "no owned driver debug_view"
revision, address, count, stride = [view.GetChildMemberWithName(k).GetValueAsUnsigned()
                                   for k in ("revision", "address", "count", "stride")]
assert revision == 1 and 0 < count <= 64 and 16 <= stride <= 1024, "bad debug view"
print("__LANG_VIEW__ %d %d %d" % (revision, count, stride))
for i in range(count):
    error = lldb.SBError()
    data = thread.GetProcess().ReadMemory(address + i * stride, 16, error)
    assert error.Success() and len(data) == 16, "unreadable logical frame"
    print("__LANG_FRAME__ %d %d %d %d" % struct.unpack("<IIII", data))
"""


def check_logical_stack(text):
    """Decode target memory; assert real logical parents, including call sites."""
    views = re.findall(r"(?m)^__LANG_VIEW__ (\d+) (\d+) (\d+)\s*$", text)
    records = [tuple(map(int, row)) for row in
               re.findall(r"(?m)^__LANG_FRAME__ (\d+) (\d+) (\d+) (\d+)\s*$", text)]
    if len(views) != 1 or tuple(map(int, views[0][:2])) != (1, 3) or len(records) != 3:
        raise RuntimeError(f"owned: missing three live logical frames: {views}, {records}")
    frames = []
    for (function, source, start, end), expected, name, line in zip(
            records, [3, 2, 1], ["main", "debug_outer", "debug_take"], [15, 12, None]):
        if function != expected or source != 0 or not 0 <= start <= end <= len(OWNED):
            raise RuntimeError(f"owned: invalid logical frame: {records}")
        actual_line = OWNED.count("\n", 0, start) + 1
        # The innermost frame records the latest charged instruction; the
        # physical PC supplies its exact current line. Parents retain call sites.
        if (line is not None and actual_line != line) or (line is None and actual_line not in (7, 8)):
            raise RuntimeError(f"owned: wrong logical call site: {records}")
        frames.append({"function": function, "name": name, "source": source,
                       "start": start, "end": end, "line": actual_line})
    return frames


def execute(label, arguments, directory, logs, timeout=60, accepted_statuses=(0,)):
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
    if status not in accepted_statuses:
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
        *([f"script exec({LLDB_STACK!r})"] if source.stem == "owned" else []),
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
            "logical_stack": check_logical_stack(text) if profile == "owned" else [],
            "pc_before": addresses[0], "pc_after": addresses[1], "stack": first_stack.strip(),
            "result": 42}


def cdb_commands(location, profile):
    return [
        'sxe -c ".echo __LANG_EXIT__; .lastevent; q" epr',
        f"bp {location}",
        "g",
        ".echo __LANG_BEFORE__",
        ".frame /r 0",
        "r rip",
        "k",
        *([
            ".frame /r 1",
            'r @$t0 = @@c++(debug_view.address)',
            'r @$t1 = @@c++(debug_view.stride)',
            'r @$t2 = @@c++(debug_view.count)',
            '.printf "__LANG_VIEW__ %I64u %I64u %I64u\\n", @@c++(debug_view.revision), @$t2, @$t1',
            '.if (@$t2 > 0 && @$t2 <= 0n64 && @$t1 >= 0n16 && @$t1 <= 0n1024) { '
            '.for (r @$t3 = 0; @$t3 < @$t2; r @$t3 = @$t3 + 1) { '
            '.printf "__LANG_FRAME__ %u %u %u %u\\n", '
            'dwo(@$t0+@$t3*@$t1), dwo(@$t0+@$t3*@$t1+4), '
            'dwo(@$t0+@$t3*@$t1+8), dwo(@$t0+@$t3*@$t1+0xc) } }',
            ".frame /r 0",
        ] if profile == "owned" else []),
        "bc *",
        "l+t" if profile == "copy" else "l-t",
        "p" if profile == "copy" else "t",
        ".echo __LANG_AFTER__",
        ".frame /r 0",
        "r rip",
        "k",
        "g",
        ".echo __LANG_UNEXPECTED_STOP__",
        ".lastevent",
        "q",
    ]


def check_cdb_trace(text, profile, line):
    text = text.replace("\r\n", "\n")
    before = re.search(r"(?ms)^__LANG_BEFORE__\s*\n(.*?)^__LANG_AFTER__\s*\n", text)
    after = re.search(r"(?ms)^__LANG_AFTER__\s*\n(.*?)^__LANG_EXIT__\s*\n", text)
    if not before or not after:
        raise RuntimeError(f"{profile}: CDB did not stop, step and exit:\n{text[-18000:]}")
    frames = [before.group(1), after.group(1)]
    addresses = []
    for frame in frames:
        pc = re.search(r"rip=([0-9a-fA-F`]+)", frame)
        if not pc:
            raise RuntimeError(f"{profile}: no CDB instruction pointer:\n{frame}")
        addresses.append(int(pc.group(1).replace("`", ""), 16))
    if addresses[0] == addresses[1]:
        raise RuntimeError(f"{profile}: CDB instruction pointer did not advance")
    location = lambda number: rf"{profile}\.lang\s*@\s*{number}\b"
    if not re.search(location(line), frames[0]):
        raise RuntimeError(f"{profile}: wrong CDB source location:\n{frames[0]}")
    if f"{profile}.lang" not in frames[1]:
        raise RuntimeError(f"{profile}: CDB step left native source code:\n{frames[1]}")
    if profile == "copy":
        if not all(re.search(location(n), frames[0]) for n in [2, 6, 9]):
            raise RuntimeError(f"copy: missing native callers:\n{frames[0]}")
        if not re.search(location(3), frames[1]):
            raise RuntimeError(f"copy: CDB source step did not reach return:\n{frames[1]}")
    elif "lang_codegen_clif" not in frames[0]:
        raise RuntimeError(f"owned: missing production runtime caller:\n{frames[0]}")
    exit_text = text.split("__LANG_EXIT__", 1)[-1]
    if not re.search(r"(?m)^42\s*$", text) or not re.search(r"[Ee]xit process.*code 0\b", exit_text):
        raise RuntimeError(f"{profile}: missing normal program completion:\n{text[-12000:]}")
    return {"profile": profile, "step": "source p" if profile == "copy" else "instruction t",
            "logical_stack": check_logical_stack(text) if profile == "owned" else [],
            "before": frames[0].strip(), "after": frames[1].strip(),
            "pc_before": addresses[0], "pc_after": addresses[1], "stack": frames[0].strip(),
            "result": 42}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rustc", type=Path, help="Matching rustc (defaults to rustup's SDK release)")
    args = parser.parse_args()
    sdk = args.sdk.resolve()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((sdk / "sdk.json").read_text(encoding="utf-8"))
    if args.rustc:
        rustc = str(args.rustc.resolve())
    else:
        rustc = execute("rustc-location", ["rustup", "which", "--toolchain",
                                          manifest["rustc"]["release"], "rustc"],
                        Path.cwd(), root, 20).strip()
    verbose = execute("rustc-version", [rustc, "-vV"], root, root, 20)
    fields = dict(line.split(": ", 1) for line in verbose.splitlines() if ": " in line)
    if (fields.get("release") != manifest["rustc"]["release"]
            or fields.get("commit-hash") != manifest["rustc"]["commit"]
            or fields.get("host") != manifest["target"]):
        raise RuntimeError(f"rustc does not match the native SDK: {verbose}")
    windows = os.name == "nt"
    compiler = sdk / "bin" / ("lang.exe" if windows else "lang")
    if windows:
        path = Path(r"C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe")
        debugger = [str(path if path.is_file() else shutil.which("cdb.exe"))]
    else:
        debugger = ["xcrun", "lldb"]
    version = execute("debugger-version", debugger + ["-version" if windows else "--version"], root, root, 20)
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
        command = [rustc, "--edition=2024", f"--crate-name=lang_debug_{profile}",
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
        if windows:
            # A source line may map to several native instruction ranges.
            # Resolve it with CDB, then keep the selected symbol+offset across
            # launches (an absolute address would be invalid after ASLR).
            resolve = debugger + ["-lines", "-y", str(package), "-srcpath", str(root),
                                  "-c", f"bp `{source_path.name}:{line}`; bl; q", str(executable)]
            # An ambiguous query consumes the batch command and reaches EOF;
            # CDB returns HRESULT_FROM_WIN32(ERROR_BROKEN_PIPE) after printing
            # the matches. Actual execution still requires normal status 0.
            locations = execute(f"{profile}-resolve-source", resolve, root, root, 30,
                                (0, 1, 0x8007006D))
            candidates = re.findall(r"debuggee![A-Za-z_][A-Za-z_0-9]*(?:\+0x[0-9a-fA-F]+)?", locations)
            if not candidates:
                raise RuntimeError(f"{profile}: CDB did not resolve source line {line}:\n{locations}")
            commands = cdb_commands(candidates[0], profile)
            debug = debugger + ["-lines", "-y", str(package), "-srcpath", str(root),
                                "-c", "; ".join(commands), str(executable)]
        else:
            commands = debugger_commands(source_path, package, root, line, step)
            debug = debugger + ["--batch", "--no-lldbinit", "--file", str(executable)]
            for command in commands:
                debug.extend(["-o", command])
        trace = execute(f"{profile}-debugger", debug, root, root, 90)
        results.append(check_cdb_trace(trace, profile, line) if windows
                       else check_trace(trace, profile, line, step))
    report = {"version": 1, "os": platform.system(), "architecture": platform.machine(),
              "sdk_build_id": manifest["build_id"], "debugger": version.strip(), "cases": results}
    (root / "debugger-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2), flush=True)


if __name__ == "__main__":
    main()
