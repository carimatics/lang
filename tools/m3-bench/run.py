#!/usr/bin/env python3
"""Capture release M3 task and public-component entry costs on the actual host."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time


def run(command, root, timeout=180):
    result = subprocess.run(command, cwd=root, capture_output=True, text=True,
                            timeout=timeout, check=False)
    if result.returncode:
        raise RuntimeError(f"{command!r}: exit {result.returncode}\n{result.stderr[-8192:]}")
    if len(result.stdout) + len(result.stderr) > 4 * 1024 * 1024:
        raise ValueError("child output exceeds finite harness limit")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=21)
    args = parser.parse_args()
    if not 1 <= args.samples <= 1001:
        parser.error("samples must be in 1..1001")
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    if output.exists():
        raise ValueError("output already exists; preserve previous observations")
    revision = run(["git", "rev-parse", "HEAD"], root).strip()
    if run(["git", "status", "--porcelain"], root).strip():
        raise ValueError("measurement requires a clean committed source tree")
    command = ["cargo", "build", "--release", "--example", "m3-costs",
               "--example", "component-plugin-host", "--message-format=json-render-diagnostics"]
    build = run(command, root, timeout=1800)
    executables = {}
    for line in build.splitlines():
        row = json.loads(line)
        if row.get("reason") == "compiler-artifact" and row.get("executable"):
            executables[row["target"]["name"]] = row["executable"]
    commands = [[executables["m3-costs"], str(args.samples)]] + [
        [executables["component-plugin-host"], mode, "--measure", str(args.samples)]
        for mode in ("vm", "jit")]
    results = [json.loads(run(command, root)) for command in commands]
    tasks, vm, jit = results
    if any(row["profile"] != "release" or row["samples"] != args.samples for row in results):
        raise ValueError("profile or sample count mismatch")
    if tasks["format"] != "lang-m3-entry-costs" or len(tasks["workloads"]) != 6:
        raise ValueError("incomplete workload matrix")
    for row in tasks["workloads"]:
        if any(row["counters"][key] for key in
               ("final_live_tasks", "final_live_allocations", "final_live_bytes")):
            raise ValueError("task inventory not retired")
        for backend in ("vm", "jit"):
            if len(row[backend]["samples_ns"]) != args.samples:
                raise ValueError("incomplete task samples")
    if vm["entry"]["counters"] != jit["entry"]["counters"]:
        raise ValueError("component VM/JIT logical counters differ")
    for row in (vm, jit):
        if (row["format"] != "lang-m3-boundary-costs" or row["final_live_allocations"]
                or row["owner_releases"] != 1 or row["native_calls"] != args.samples + 5
                or len(row["entry"]["samples_ns"]) != args.samples):
            raise ValueError("component lifecycle/sample checks failed")
    paths = ["tools/m3-bench/run.py", "examples/m3-costs.rs",
             "examples/component-plugin-host.rs", "Cargo.lock", "rust-toolchain.toml",
             "examples/plugins/verified.lang", "examples/plugins/verified.idl.json",
             "examples/plugins/native.idl.json"] + [
        row["source"] for row in tasks["workloads"] if not row["source"].startswith("inline:")]
    hashes = {path: hashlib.sha256((root / path).read_bytes()).hexdigest() for path in paths}
    if (run(["git", "rev-parse", "HEAD"], root).strip() != revision
            or run(["git", "status", "--porcelain"], root).strip()):
        raise ValueError("source changed during measurement")
    report = {
        "format": "lang-m3-benchmark", "version": 1,
        "verified_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "revision": revision, "dirty": False,
        "rustc": run(["rustc", "--version", "--verbose"], root),
        "host": {"platform": platform.platform(), "machine": platform.machine(),
                 "processor": platform.processor(), "cpu_count": os.cpu_count(),
                 "python": platform.python_version()},
        "source_sha256": hashes, "build_command": command, "run_commands": commands,
        "tasks": tasks, "components": [vm, jit],
        "limits": "Host-specific observations; no AOT/Windows, pure guest, process RSS or throughput claim. "
                  "Managed allocation counters exclude unmetered compiler/native host allocations. "
                  "Preparation observations are single runs. No performance pass/fail threshold.",
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2)
        handle.write("\n")
    print(output)


if __name__ == "__main__":
    main()
