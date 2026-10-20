#!/usr/bin/env python3
"""Recompute the published M2 medians and ratios from immutable native captures."""
import json
from pathlib import Path
import statistics


def main():
    root = Path(__file__).resolve().parents[1]
    files = (
        ("macOS", "2026-10-20-m2-macos-ecc0fd1.json"),
        ("Windows", "2026-10-20-m2-windows-dced632.json"),
    )
    modes = ("source", "rust", "c")
    for host, name in files:
        path = root / "benchmarks/results" / name
        if path.stat().st_size > 1024 * 1024:
            raise ValueError("capture exceeds summary reader limit")
        data = json.loads(path.read_text(encoding="utf-8"))
        if (data["format"] != "lang-m2-benchmark" or data["version"] != 1
                or data["dirty"] or len(data["observations"]) != 135
                or len(data["startup_observations"]) != 45
                or data["inputs"] != {"n": 1000, "repeats": 20, "samples": 15, "warmups": 3}):
            raise ValueError("unexpected published capture")
        print(host, data["revision"], data["inputs"])
        for work in ("scalar", "buffers", "regions"):
            ns = {}
            for mode in modes:
                rows = [x for x in data["observations"]
                        if x["mode"] == mode and x["workload"] == work]
                if len(rows) != 15 or {x["sample"] for x in rows} != set(range(15)):
                    raise ValueError("incomplete or duplicate sample matrix")
                if any(x["checksum"] != x["result"] * 20 or x["live_bytes"]
                       or x["allocations"] != x["releases"] or x["elapsed_ns"] <= 0 for x in rows):
                    raise ValueError("invalid checksum/retirement/time")
                ns[mode] = statistics.median(x["elapsed_ns"] for x in rows) / 20
            print(work, *(format(ns[m] / 1000, ".3f") for m in modes),
                  format(ns["source"] / ns["rust"], ",.0f") + "x",
                  format(ns["source"] / ns["c"], ",.0f") + "x")


if __name__ == "__main__":
    main()
