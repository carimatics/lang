#!/usr/bin/env python3
"""M2 source/Rust/C parity and measurement harness; keeps prototype schema20 separate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import time

WORKLOADS = ("scalar", "buffers", "regions")
INPUTS = (0, 1, 7, 64, 250, 251, 1000)


def run(command, cwd, accepted=(0,)):
    started = time.perf_counter_ns()
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True,
                            timeout=180, check=False)
    elapsed = time.perf_counter_ns() - started
    if result.returncode not in accepted:
        raise RuntimeError(f"{command[0]} exited {result.returncode}:\n"
                           f"{result.stdout[-8192:]}\n{result.stderr[-8192:]}")
    if len(result.stdout) + len(result.stderr) > 1024 * 1024:
        raise ValueError("child output exceeds harness limit")
    return result.stdout, result.stderr, elapsed


def expected(workload, n):
    if workload == "buffers":
        cycles, rest = divmod(n, 250)
        return n * (n - 1) // 2 + 2 * (cycles * 31125 + rest * (rest - 1) // 2)
    if workload == "regions":
        return n * (n + 1)
    value = 17
    for index in range(n):
        value = value + value + index if value < 500003 else value - 500003 + index
    return value


def parse_parity(stdout):
    rows = []
    lines = stdout.splitlines()
    if len(lines) != len(WORKLOADS) * len(INPUTS):
        raise ValueError("incomplete parity matrix")
    for line, (workload, n) in zip(lines, ((w, n) for w in WORKLOADS for n in INPUTS)):
        fields = line.split(",")
        if len(fields) != 5 or fields[0] != workload or int(fields[1]) != n:
            raise ValueError("unexpected parity row")
        outputs = [int(value) for value in fields[2:]]
        if outputs != [expected(workload, n)] * 3:
            raise ValueError("source/Rust/C/oracle mismatch")
        rows.append({"workload": workload, "input": n, "result": outputs[0]})
    return rows


def summary(values):
    return {"min": min(values), "median": statistics.median(values), "max": max(values),
            "mean": statistics.mean(values), "stdev": statistics.stdev(values) if len(values) > 1 else 0}


def validate_shape(row):
    required = {"preparation_ns", "elapsed_ns", "checksum", "result", "allocations",
                "releases", "requested_bytes", "live_bytes", "peak_live_bytes",
                "peak_working_set_bytes"}
    if set(row) != required or any(type(value) is not int or value < 0 for value in row.values()):
        raise ValueError("invalid child measurement")


def validate_sample(row, workload, n, repeats):
    validate_shape(row)
    if (row["result"] != expected(workload, n) or row["checksum"] != row["result"] * repeats
            or row["allocations"] != row["releases"] or row["live_bytes"]
            or row["elapsed_ns"] <= 0):
        raise ValueError("measurement checksum/retirement failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--repeats", type=int, default=10)
    parser.add_argument("--input", type=int, default=1000)
    parser.add_argument("--parity-only", action="store_true")
    args = parser.parse_args()
    if not (1 <= args.samples <= 101 and 1 <= args.repeats <= 10000 and 0 <= args.input <= 100000):
        raise ValueError("samples/repeats/input outside finite harness limits")
    source = Path(__file__).resolve().parent
    sdk = args.sdk.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((sdk / "sdk.json").read_text(encoding="utf-8"))
    host = {("Darwin", "arm64"): "aarch64-apple-darwin",
            ("Windows", "AMD64"): "x86_64-pc-windows-msvc"}.get((platform.system(), platform.machine()))
    if manifest["build_profile"] != "release" or manifest["target"] != host:
        raise ValueError("comparison requires a release SDK for this native tier-1 host")
    rustc = run(["rustup", "which", "--toolchain", manifest["rustc"]["release"], "rustc"], output)[0].strip()
    rust_version = run([rustc, "--version", "--verbose"], output)[0]
    for key, value in (("release", manifest["rustc"]["release"]), ("commit-hash", manifest["rustc"]["commit"]), ("host", host)):
        if rust_version.splitlines().count(f"{key}: {value}") != 1:
            raise ValueError("rustc does not match release SDK")
    # cl without a source prints its version and exits 2 (missing input).
    c_version = run(["cl"] if os.name == "nt" else ["cc", "--version"], output,
                    accepted=(0, 2) if os.name == "nt" else (0,))
    compiler = sdk / "bin" / ("lang.exe" if os.name == "nt" else "lang")
    package = output / "package"
    compile_command = [str(compiler), "native", "--output", str(package), str(source / "workloads.lang")]
    compile_ns = run(compile_command, output)[2]
    package_manifest = json.loads((package / "package.json").read_text(encoding="utf-8"))
    guest_objects = [entry["path"] for entry in package_manifest["files"] if entry["role"] == "object"]
    if len(guest_objects) != 1:
        raise ValueError("expected one original source object")
    header = (package / "program.h").read_text(encoding="utf-8")
    identities = re.findall(r"^uint32_t lang_context_create_([0-9a-f]{64})\(", header, re.MULTILINE)
    if len(identities) != 1:
        raise ValueError("unexpected C context declarations")
    identity = identities[0]
    client = (source / "client.c.in").read_text(encoding="utf-8")
    client = (client.replace("@ID@", identity)
              .replace("@DIGEST_BYTES@", ",".join(map(str, bytes.fromhex(identity)))))
    for name in WORKLOADS:
        symbols = re.findall(r"^uint32_t (lang_" + name + r"_[0-9a-f]{64})"
                             r"\(lang_context \*context, int64_t p0, int64_t \*out\);$", header, re.MULTILINE)
        if len(symbols) != 1:
            raise ValueError(f"unexpected {name} C declarations")
        client = client.replace(f"@{name.upper()}_SYMBOL@", symbols[0])
    if re.search(r"@[A-Z_]+@", client):
        raise ValueError("unfilled C placeholder")
    (package / "client.c").write_text(client, encoding="utf-8")
    for filename in ("host.rs", "reference.rs"):
        shutil.copyfile(source / filename, package / filename)
    sdk_arguments = []
    directories = sorted({(sdk / entry["path"]).parent for entry in manifest["files"]
                          if entry["role"] in ("runtime", "dependency")})
    for directory in directories:
        sdk_arguments += ["-L", f"dependency={directory}", "-L", f"native={directory}"]
    for name in ("lang_codegen_clif", "lang_runtime", "lang_mir", "lang_bytecode"):
        libraries = [entry for entry in manifest["files"]
                     if Path(entry["path"]).name.startswith(f"lib{name}-") and entry["path"].endswith(".rlib")]
        if len(libraries) != 1:
            raise ValueError(f"expected one SDK {name} rlib")
        sdk_arguments += ["--extern", f"{name}={sdk / libraries[0]['path']}"]
    builds = {}
    for mode, macro in (("parity", "PARITY"), ("source", "SOURCE"), ("rust", "RUST"), ("c", "CREF")):
        suffix = ".obj" if os.name == "nt" else ".o"
        obj = package / (mode + suffix)
        if os.name == "nt":
            c_flags = ["cl", "/nologo", "/std:c11", "/O2", "/MD", "/c", f"/D{macro}", "client.c", f"/Fo{obj}"]
        else:
            c_flags = ["cc", "-std=c11", "-O3", "-Wall", "-Wextra", "-Werror", "-c", f"-D{macro}", "client.c", "-o", str(obj)]
        c_compile_ns = run(c_flags, package)[2]
        executable = output / (mode + (".exe" if os.name == "nt" else ""))
        rust_flags = [rustc, "--edition=2024", "--crate-name=m2_bench", "-Dwarnings",
                      "-Copt-level=3", "-Cpanic=unwind", "--cfg", mode]
        for cfg in ("source", "parity", "rust", "c"):
            rust_flags += ["--check-cfg", f"cfg({cfg})"]
        rust_flags += ["host.rs", "-C", f"link-arg={obj}", "-o", str(executable)]
        if mode in ("source", "parity"):
            rust_flags += sdk_arguments + ["-C", f"link-arg={package / guest_objects[0]}"]
        link_ns = run(rust_flags, package)[2]
        builds[mode] = {"executable": str(executable), "executable_bytes": executable.stat().st_size,
                        "c_command": c_flags, "c_compile_ns": c_compile_ns,
                        "rust_command": rust_flags, "link_ns": link_ns}
    # The equality gate is mandatory and occurs before any timing invocation.
    rows = parse_parity(run([builds["parity"]["executable"], "parity"], output)[0])
    unwind = run([builds["source"]["executable"], "unwind-probe"], output)[0].strip()
    if unwind != "unwind retirement ok":
        raise ValueError("unwind retirement probe did not complete")
    observations = []
    startup_observations = []
    if not args.parity_only:
        # Rotate order by sample to reduce systematic temperature/order bias.
        for sample in range(args.samples):
            modes = ("source", "rust", "c")
            modes = modes[sample % 3:] + modes[:sample % 3]
            for mode in modes:
                stdout, _, wall_ns = run([builds[mode]["executable"], "startup"], output)
                row = json.loads(stdout)
                validate_shape(row)
                if (row["elapsed_ns"] or row["checksum"] or row["result"] or row["live_bytes"]
                        or row["allocations"] != row["releases"]):
                    raise ValueError("startup performed work or failed retirement")
                row.update(sample=sample, mode=mode, process_wall_ns=wall_ns)
                startup_observations.append(row)
            for workload in WORKLOADS:
                for mode in modes:
                    command = [builds[mode]["executable"], workload, str(args.input), str(args.repeats)]
                    if os.name != "nt":
                        command = ["/usr/bin/time", "-l"] + command
                    stdout, stderr, process_ns = run(command, output)
                    row = json.loads(stdout)
                    validate_sample(row, workload, args.input, args.repeats)
                    if os.name == "nt":
                        peak = row["peak_working_set_bytes"]
                    else:
                        matches = re.findall(r"^\s*(\d+)\s+maximum resident set size\s*$", stderr, re.MULTILINE)
                        if len(matches) != 1:
                            raise ValueError("missing exact-child macOS peak RSS")
                        peak = int(matches[0])
                    if peak <= 0:
                        raise ValueError("missing OS process memory measurement")
                    row.update(sample=sample, mode=mode, workload=workload, process_wall_ns=process_ns,
                               peak_resident_bytes=peak)
                    observations.append(row)
    aggregates = []
    for workload in WORKLOADS:
        for mode in ("source", "rust", "c"):
            subset = [row for row in observations if row["workload"] == workload and row["mode"] == mode]
            if subset:
                aggregates.append({"workload": workload, "mode": mode,
                                   **{metric: summary([row[metric] for row in subset]) for metric in
                                      ("elapsed_ns", "preparation_ns", "process_wall_ns", "peak_resident_bytes")}})
    root = source.parents[1]
    revision = run(["git", "rev-parse", "HEAD"], root)[0].strip()
    dirty = run(["git", "status", "--porcelain"], root)[0].strip()
    report = {"format": "lang-m2-benchmark", "version": 1,
              "verified_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "revision": revision, "dirty": bool(dirty), "target": host,
              "sdk_build_id": manifest["build_id"], "sdk_profile": manifest["build_profile"],
              "rustc": rust_version, "c_compiler": c_version[0] + c_version[1],
              "host": {"platform": platform.platform(), "processor": platform.processor(),
                       "cpu_count": os.cpu_count(), "python": platform.python_version()},
              "inputs": {"n": args.input, "repeats": args.repeats, "samples": args.samples, "warmups": 3},
              "harness_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                                 for p in sorted(source.iterdir()) if p.is_file()},
              "compile_command": compile_command, "compile_ns": compile_ns, "builds": builds,
              "source_object_bytes": (package / guest_objects[0]).stat().st_size,
              "parity": rows, "unwind_retirement": unwind,
              "observations": observations, "aggregates": aggregates,
              "startup_observations": startup_observations,
              "startup_aggregates": [{"mode": mode, "process_wall_ns": summary([
                  row["process_wall_ns"] for row in startup_observations if row["mode"] == mode])}
                  for mode in ("source", "rust", "c")] if startup_observations else []}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"parity: {len(rows)} source/Rust/C/oracle cases; measurements: {len(observations)}; {output / 'report.json'}")


if __name__ == "__main__":
    main()
