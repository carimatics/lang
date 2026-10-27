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

WORKLOADS = {
    "spawn-join": ("examples/scripts/tasks.lang", 42),
    "owned-channel": ("tests/fixtures/task_channels.lang", 42),
    "mixed-cancellation": ("tests/fixtures/task_channel_mixed_cancel.lang", 42),
    "owned-buffers": ("tests/fixtures/task_owned_buffers.lang", 71),
    "service-state": ("tests/fixtures/task_service_state_replace.lang", 42),
    "single-task": ("inline:single-task", 42),
}


def integer(value, minimum=0):
    return type(value) is int and minimum <= value <= 2**64 - 1


def validate_distribution(row, samples):
    values = row.get("samples_ns")
    if (not isinstance(values, list) or len(values) != samples
            or not all(integer(value, 1) for value in values)):
        raise ValueError("invalid or incomplete timing samples")
    ordered = sorted(values)
    expected = dict(min_ns=ordered[0], median_ns=ordered[samples // 2],
                    p95_ns=ordered[(95 * samples + 99) // 100 - 1],
                    max_ns=ordered[-1])
    if any(not integer(row.get(key), 1) or row[key] != value
           for key, value in expected.items()):
        raise ValueError("timing summary does not match raw samples")


def validate_counters(counters):
    if (not isinstance(counters, dict) or not counters
            or not all(integer(value) for value in counters.values())):
        raise ValueError("invalid logical counters")


def validate_results(results, samples, root):
    """Validate the observed matrix before publishing measurement evidence."""
    if not integer(samples, 1) or samples > 1001 or len(results) != 3:
        raise ValueError("invalid sample count or result matrix")
    tasks, vm, jit = results
    for row in results:
        if (type(row.get("version")) is not int or row["version"] != 1
                or row.get("profile") != "release"
                or type(row.get("samples")) is not int or row["samples"] != samples):
            raise ValueError("version, profile or sample count mismatch")
    workloads = tasks.get("workloads")
    if (tasks.get("format") != "lang-m3-entry-costs" or tasks.get("warmups") != 3
            or not isinstance(workloads, list) or len(workloads) != len(WORKLOADS)
            or {row.get("workload") for row in workloads} != set(WORKLOADS)):
        raise ValueError("incomplete, duplicate or unknown workload matrix")
    for row in workloads:
        source, expected = WORKLOADS[row["workload"]]
        if row.get("source") != source or type(row.get("expected")) is not int or row["expected"] != expected:
            raise ValueError("workload source or oracle mismatch")
        source_bytes = (b"async fn main() -> i64 { return 42; }"
                        if source == "inline:single-task" else (root / source).read_bytes())
        if row.get("sha256") != hashlib.sha256(source_bytes).hexdigest():
            raise ValueError("workload source digest mismatch")
        validate_counters(row["counters"])
        if any(row["counters"].get(key) != 0 for key in
               ("final_live_tasks", "final_live_allocations", "final_live_bytes")):
            raise ValueError("task inventory not retired")
        for backend in ("vm", "jit"):
            validate_distribution(row[backend], samples)
    for row, backend in ((vm, "vm"), (jit, "jit")):
        if (row.get("format") != "lang-m3-boundary-costs" or row.get("backend") != backend
                or not integer(row.get("final_live_allocations")) or row["final_live_allocations"] != 0
                or not integer(row.get("owner_releases")) or row["owner_releases"] != 1
                or not integer(row.get("native_calls")) or row["native_calls"] != samples + 5
                or row["entry"].get("warmups") != 3
                or not integer(row["entry"].get("live_allocations_after_each_call"))
                or row["entry"]["live_allocations_after_each_call"] != 0):
            raise ValueError("component backend, lifecycle or warmup checks failed")
        validate_counters(row["entry"]["counters"])
        validate_distribution(row["entry"], samples)
    if vm["entry"]["counters"] != jit["entry"]["counters"]:
        raise ValueError("component VM/JIT logical counters differ")


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
    command = ["cargo", "build", "--locked", "--release", "--example", "m3-costs",
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
    validate_results(results, args.samples, root)
    tasks, vm, jit = results
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
