#!/usr/bin/env python3
"""Private P02 CI probe: transport a matching SDK, then exercise it without Cargo.

The bundle contains this probe and trusted SDK bytes, never a repository/test
runtime. This is installation evidence, not the later Q/B/G conformance gates.
"""
import argparse
import io
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import time


def run(command, cwd, env=None):
    started = time.monotonic()
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True,
                            text=True, timeout=180, check=False)
    if result.returncode:
        raise RuntimeError(f"{command[0]} exited {result.returncode}:\n"
                           f"{result.stdout[-8192:]}\n{result.stderr[-8192:]}")
    return result.stdout, round(time.monotonic() - started, 3)


def bundle(args):
    sdk = args.sdk.resolve(strict=True)
    manifest = json.loads((sdk / "sdk.json").read_text(encoding="utf-8"))
    script = Path(__file__).resolve()
    root = script.parents[1]
    revision, _ = run(["git", "rev-parse", "HEAD"], root)
    if revision.strip() != args.revision:
        raise ValueError("producer revision does not match requested revision")
    dirty, _ = run(["git", "status", "--porcelain"], root)
    if dirty.strip():
        raise ValueError("SDK producer must have a clean checkout")
    origin = {"revision": args.revision, "producer_root": str(root),
              "target": manifest["target"], "build_id": manifest["build_id"]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(args.output, "x") as archive:
        archive.add(sdk, arcname="sdk")
        archive.add(script, arcname="probe.py")
        data = json.dumps(origin, sort_keys=True).encode("utf-8")
        member = tarfile.TarInfo("origin.json")
        member.size = len(data)
        archive.addfile(member, io.BytesIO(data))
    print(json.dumps(origin, sort_keys=True))


def absent_repository(root):
    checked = []
    for name in [".git", "Cargo.toml", "Cargo.lock", "src", "crates", "target"]:
        path = root / name
        if path.exists() or path.is_symlink():
            raise ValueError(f"isolated consumer has a repository input: {path}")
        checked.append(str(path))
    return checked


def c_object(sdk, package, env):
    # This fixed private fixture has one i64 export. Read the actual public C
    # declarations, never guess identities or grant authority from metadata.
    header = (package / "program.h").read_text(encoding="utf-8")
    identities = re.findall(r"^uint32_t lang_context_create_([0-9a-f]{64})\(",
                            header, re.MULTILINE)
    symbols = re.findall(r"^uint32_t (lang_entry_[0-9a-f]{64})"
                         r"\(lang_context \*context, int64_t p0, int64_t \*out\);$",
                         header, re.MULTILINE)
    if len(identities) != 1 or len(symbols) != 1:
        raise ValueError("unexpected fixed C fixture declarations")
    identity = identities[0]
    source = (sdk / "startup/c-client.c.in").read_text(encoding="utf-8")
    source = (source.replace("@ID@", identity)
              .replace("@DIGEST_BYTES@", ",".join(map(str, bytes.fromhex(identity))))
              .replace("@ENTRY_SYMBOL@", symbols[0]))
    if re.search(r"@[A-Z_]+@", source):
        raise ValueError("unfilled C fixture placeholder")
    (package / "client.c").write_text(source, encoding="utf-8")
    if os.name == "nt":
        command = ["cl", "/nologo", "/std:c11", "/MD", "/c", "client.c", "/Foclient.obj"]
        output = package / "client.obj"
    else:
        command = ["cc", "-std=c11", "-Werror", "-c", "client.c", "-o", "client.o"]
        output = package / "client.o"
    run(command, package, env)
    return output


def verify(args):
    source_sdk = args.sdk.resolve(strict=True)
    origin_path = source_sdk.parent / "origin.json"
    origin = json.loads(origin_path.read_text(encoding="utf-8")) if origin_path.exists() else None
    absent = []
    if args.isolated_root:
        if not origin or origin["revision"] != args.revision:
            raise ValueError("missing/mismatched exact-head producer provenance")
        absent += absent_repository(args.isolated_root.resolve())
        absent += absent_repository(Path(origin["producer_root"]))
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    # Work only on private disposable copies; preserve the supplied installation.
    sdk = output / "private-sdk"
    shutil.copytree(source_sdk, sdk)
    manifest = json.loads((sdk / "sdk.json").read_text(encoding="utf-8"))
    target = {("Darwin", "arm64"): "aarch64-apple-darwin",
              ("Windows", "AMD64"): "x86_64-pc-windows-msvc"}.get(
                  (platform.system(), platform.machine()))
    if manifest["target"] != target:
        raise ValueError(f"SDK target {manifest['target']} does not match host {target}")
    if origin and (origin["target"] != target or origin["build_id"] != manifest["build_id"]):
        raise ValueError("producer provenance does not match SDK")
    rustc, _ = run(["rustup", "which", "--toolchain", manifest["rustc"]["release"], "rustc"], output)
    rustc = str(Path(rustc.strip()).resolve(strict=True))
    version, _ = run([rustc, "--version", "--verbose"], output)
    for field, expected in [("release", manifest["rustc"]["release"]),
                            ("commit-hash", manifest["rustc"]["commit"]), ("host", target)]:
        if version.splitlines().count(f"{field}: {expected}") != 1:
            raise ValueError(f"installed rustc {field} mismatch")
    env = dict(os.environ)
    env["RUSTC"] = rustc
    for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTDOCFLAGS"]:
        env.pop(key, None)
    env["CARGO"] = str(output / "must-not-run-cargo")
    env["CARGO_HOME"] = str(output / "empty-cargo-home")
    env["CARGO_TARGET_DIR"] = str(output / "must-not-use-target")
    Path(env["CARGO_HOME"]).mkdir()
    env["PATH"] = os.pathsep.join(
        entry for entry in os.get_exec_path(env)
        if entry and not any((Path(entry) / name).is_file()
                             for name in ["cargo", "cargo.exe", "cargo.cmd", "cargo.bat"]))
    if shutil.which("cargo", path=env["PATH"]) is not None:
        raise ValueError("Cargo is still reachable through PATH")
    compiler = sdk / "bin" / ("lang.exe" if os.name == "nt" else "lang")
    texts = {
        "copy": "fn first()->i64{return 7;} fn main()->i64{return first()+35;}",
        "owned": """fn main()->i64 {
            let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
            let mut pushed: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(bytes, 41));
            return core::result_unwrap::<i64, core::BoundsError>(core::bytes_get(bytes, 0))+1;
        }""",
        "c": (sdk / "startup/c-guest.lang").read_text(encoding="utf-8"),
    }
    rows = []
    packages = []
    for name, text in texts.items():
        source = output / f"{name}.lang"
        source.write_text(text, encoding="utf-8")
        package = output / f"{name}-package"
        _, compile_seconds = run([str(compiler), "native", "--output", str(package), str(source)], output, env)
        source.unlink()
        executable = output / (name + (".exe" if os.name == "nt" else ""))
        command = [str(compiler), "link-native", "--sdk", str(sdk), "--package", str(package), "--output", str(executable)]
        if name == "c":
            command += ["--host", str(sdk / "startup/c-host.rs"),
                        "--object", str(c_object(sdk, package, env))]
        _, link_seconds = run(command, output, env)
        package_manifest = json.loads((package / "package.json").read_text(encoding="utf-8"))
        rows.append({"profile": name, "compile_seconds": compile_seconds,
                     "link_seconds": link_seconds, "executable": str(executable),
                     "files": package_manifest["files"]})
        packages.append(package)
    # All actual linked input paths disappear before any application is started.
    for package in packages:
        shutil.rmtree(package)
    shutil.rmtree(sdk)
    for row in rows:
        stdout, seconds = run([row["executable"]], output, env)
        expected = "" if row["profile"] == "c" else "42"
        if stdout.strip() != expected:
            raise ValueError(f"{row['profile']} returned unexpected output: {stdout!r}")
        row.update(stdout=stdout.strip(), startup_seconds=seconds, success=True)
    if list(Path(env["CARGO_HOME"]).iterdir()) or Path(env["CARGO_TARGET_DIR"]).exists():
        raise ValueError("consumer wrote Cargo/registry/target data")
    if list(output.glob(".lang-native-*")):
        raise ValueError("native staging leaked")
    report = {"revision": args.revision, "origin": origin, "target": target,
              "build_id": manifest["build_id"], "rustc": version,
              "isolated": bool(args.isolated_root), "absent_repository_inputs": absent,
              "cargo_on_path": False, "cargo_home_empty": True,
              "linked_input_paths_removed": True, "results": rows}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: report[k] for k in ["revision", "target", "build_id", "isolated"]}
                     | {"passed": [row["profile"] for row in rows]}, sort_keys=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["bundle", "verify"])
    parser.add_argument("--sdk", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--isolated-root", type=Path)
    args = parser.parse_args()
    if args.mode == "bundle":
        bundle(args)
    else:
        verify(args)


if __name__ == "__main__":
    main()
