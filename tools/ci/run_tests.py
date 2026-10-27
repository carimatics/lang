"""Partition Cargo targets without changing libtest's in-process execution model."""

import argparse
import json
from pathlib import Path
import shlex
import subprocess


ROOT = Path(__file__).resolve().parents[2]
SHARDS = ("core", "native", "owned")


def shard_for(name):
    # The longest suite gets its own runner. New integration targets are included
    # automatically; do not maintain an allowlist that can silently omit tests.
    if name == "native_owned_boundary":
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
        if "test" in t["kind"] and shard_for(t["name"]) == shard
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
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
    ))
    for command in commands(metadata, args.shard):
        print(shlex.join(command), flush=True)
        if not args.dry_run:
            subprocess.run(command, cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
