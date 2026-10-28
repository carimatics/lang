"""Partition Cargo targets without changing libtest's in-process execution model."""

import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import time


ROOT = Path(__file__).resolve().parents[2]
SHARDS = ("core", "native", "owned", "native-all")


def test_environment(environment, optimized):
    result = dict(environment)
    if optimized:
        # Child Cargo invocations must select the same dev graph as libtest.
        # Unlike command-line --config, these settings are inherited by them.
        for profile in ("DEV", "TEST"):
            prefix = f"CARGO_PROFILE_{profile}_"
            result.update({
                prefix + "OPT_LEVEL": "1",
                prefix + "DEBUG": "2",
                prefix + "DEBUG_ASSERTIONS": "true",
                prefix + "OVERFLOW_CHECKS": "true",
            })
    return result


def shard_for(name):
    # Balance the expensive ownership and task suites against the other native
    # targets. New targets remain included without a coverage allowlist.
    if name in ("native_owned_boundary", "native_task_execution"):
        return "owned"
    if name.startswith("native_"):
        return "native"
    return "core"


def commands(metadata, shard):
    if shard not in SHARDS:
        raise ValueError(f"unknown shard: {shard}")
    package = next(p for p in metadata["packages"] if p["name"] == "lang")
    targets = sorted(
        t["name"]
        for t in package["targets"]
        if "test" in t["kind"]
        and (shard_for(t["name"]) == shard
             or (shard == "native-all" and shard_for(t["name"]) != "core"))
    )
    if not targets:
        raise ValueError(f"empty shard: {shard}")
    base = ["cargo", "test", "--locked", "--all-features"]
    result = []
    if shard == "core":
        result.append(base + ["--workspace", "--exclude", "lang", "--all-targets"])
    command = base + ["--package", "lang"]
    if shard == "core":
        command += ["--lib", "--bins", "--examples", "--benches"]
    for name in targets:
        command += ["--test", name]
    result.append(command)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shard", choices=SHARDS)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--optimized", action="store_true",
                        help="optimize dev/test code while retaining assertions and overflow checks")
    args = parser.parse_args()
    environment = test_environment(os.environ, args.optimized)
    if args.optimized:
        print("Dev/test opt-level=1, debuginfo=2, debug assertions and overflow checks enabled.",
              flush=True)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
    ))
    for command in commands(metadata, args.shard):
        print(shlex.join(command), flush=True)
        if not args.dry_run:
            started = time.monotonic()
            try:
                subprocess.run(command, cwd=ROOT, env=environment, check=True)
            finally:
                print(f"{args.shard}: command elapsed {time.monotonic() - started:.2f}s", flush=True)


if __name__ == "__main__":
    main()
