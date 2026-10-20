#!/usr/bin/env python3
"""Build and assemble one host-native RFC 0020 SDK from exact Cargo output."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", required=True, type=Path)
parser.add_argument("--release", action="store_true")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
output = args.output.resolve()
with tempfile.TemporaryDirectory(prefix="lang-sdk-build-") as temporary:
    records = Path(temporary) / "cargo.jsonl"
    command = ["cargo", "build", "--locked", "--bin", "lang", "--example", "native-sdk", "--message-format=json"]
    if args.release:
        command.append("--release")
    with records.open("wb") as stream:
        result = subprocess.run(command, cwd=root, stdout=stream, check=False)
    if result.returncode:
        with records.open(encoding="utf-8") as stream:
            for line in stream:
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if record.get("reason") == "compiler-message":
                    rendered = record.get("message", {}).get("rendered")
                    if rendered:
                        sys.stderr.write(rendered)
        raise SystemExit(result.returncode)
    if records.stat().st_size > 32 * 1024 * 1024:
        raise SystemExit("Cargo output exceeds SDK builder limit")
    builders = []
    with records.open(encoding="utf-8") as stream:
        for line in stream:
            if not line.lstrip().startswith("{"):
                continue
            record = json.loads(line)
            if (record.get("reason") == "compiler-artifact"
                    and record["target"]["name"] == "native-sdk"
                    and "example" in record["target"]["kind"]
                    and record.get("executable")):
                builders.append(record["executable"])
    if len(builders) != 1:
        raise SystemExit("expected one freshly selected SDK builder executable")
    result = subprocess.run([builders[0], "--artifacts", str(records), "--output", str(output)], cwd=root, check=False)
    raise SystemExit(result.returncode)
