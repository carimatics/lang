#!/usr/bin/env python3
"""Execute pinned old sources, migrate through the CLI, then run VM and SDK native."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time


HISTORICAL_REVISION = "69b1c4dc66d04a4a103d4eb843232df22e52eac0"
HISTORICAL_LOCK = "fa5c279a688f72fde9a09f387aee4fe741be36f56a7515c3b0bc1dccfd43fdca"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", required=True, type=Path)
    parser.add_argument("--historical-checkout", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    sdk = args.sdk.resolve(strict=True)
    historical = args.historical_checkout.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = {"schema": 1, "kind": "checked-historical-source-migration",
              "host": platform.platform(), "success": False, "commands": [], "cases": []}

    def run(command, cwd=output, env=None, expected=0, timeout=180):
        command = [str(arg) for arg in command]
        started = time.monotonic()
        result = subprocess.run(command, cwd=cwd, env=env, capture_output=True,
                                text=True, encoding="utf-8", errors="replace",
                                timeout=timeout, check=False)
        report["commands"].append({"argv": command, "cwd": str(cwd),
                                   "exit": result.returncode,
                                   "stdout": result.stdout[-16384:],
                                   "stderr": result.stderr[-16384:],
                                   "seconds": round(time.monotonic() - started, 3)})
        require(result.returncode == expected,
                f"{command} exited {result.returncode}, expected {expected}: {result.stderr[-4096:]}")
        return result.stdout, result.stderr

    try:
        revision, _ = run(["git", "rev-parse", "HEAD"], historical)
        require(revision.strip() == HISTORICAL_REVISION, "historical revision mismatch")
        dirty, _ = run(["git", "status", "--porcelain"], historical)
        require(not dirty.strip(), "historical producer checkout is dirty")
        lock = digest((historical / "Cargo.lock").read_bytes())
        require(lock == HISTORICAL_LOCK, "historical lock identity mismatch")
        current_revision, _ = run(["git", "rev-parse", "HEAD"], root)
        current_dirty, _ = run(["git", "status", "--porcelain"], root)
        rustc, _ = run(["rustc", "--version", "--verbose"], historical)
        old_env = dict(os.environ, CARGO_TARGET_DIR=str(output / "historical-target"))
        run(["cargo", "build", "--locked", "--bin", "lang"], historical, old_env, timeout=600)
        suffix = ".exe" if os.name == "nt" else ""
        old_compiler = output / "historical-target" / "debug" / ("lang" + suffix)
        compiler = sdk / "bin" / ("lang" + suffix)
        manifest_bytes = (sdk / "sdk.json").read_bytes()
        manifest = json.loads(manifest_bytes)
        report["producer"] = {"historical_revision": HISTORICAL_REVISION,
                              "historical_lock_sha256": lock, "rustc": rustc,
                              "historical_compiler_sha256": digest(old_compiler.read_bytes()),
                              "current_revision": current_revision.strip(),
                              "current_dirty": current_dirty.splitlines(),
                              "current_lock_sha256": digest((root / "Cargo.lock").read_bytes()),
                              "current_compiler_sha256": digest(compiler.read_bytes()),
                              "sdk_manifest_sha256": digest(manifest_bytes),
                              "sdk_build_id": manifest["build_id"], "target": manifest["target"]}
        probe = json.loads((root / "docs/evidence/2026-10-30-m5-legacy-source-probe.json").read_text(encoding="utf-8"))
        require(probe["historical_commit"] == HISTORICAL_REVISION, "probe producer mismatch")
        require(digest(probe["source"]["text"].encode()) == probe["source"]["sha256"], "probe source mismatch")
        admission = json.loads((root / "docs/evidence/2026-10-31-m5-scalar-admission.json").read_text(encoding="utf-8"))
        require(admission["historical_commit"] == HISTORICAL_REVISION, "admission producer mismatch")
        cases = [("retained-shared-weak", probe["source"]["text"], "42\n"),
                 ("wrapping-parameter", probe["wrapping_parameter_probe"]["historical_source"],
                  "-9223372036854775808\n")]
        cases.extend((row["name"], row["source"], row["stdout"])
                     for row in admission["cases"] if row["exit"] == 0)
        cases.append(("loop-collision-trivia", "// shared と weak remain in this comment\r\n"
                      "fn main()->i64{let mut shared:i64=0;let mut _migrated_shared_1:i64=2;"
                      "while shared<40{shared=shared+1;}return shared+_migrated_shared_1;}\r\n", "42\n"))
        for name, text, expected in cases:
            original = text.encode("utf-8")
            source = output / (name + ".lang")
            source.write_bytes(original)
            stdout, _ = run([old_compiler, "run", "--source-root", output, "--fuel", "10000", source])
            require(stdout == expected, f"historical result mismatch: {name}")
            migrate = [compiler, "migrate", "--from-profile", "rfc0006-scalar",
                       "--to-edition", "2026", "--source-root", output,
                       "--message-format", "json"]
            preview = json.loads(run([*migrate, source])[0])
            require(source.read_bytes() == original, "preview changed source")
            require(preview["schema"] == 1 and preview["operation"] == "migrate"
                    and preview["applicability"] == "checked" and preview["applied"] == 0
                    and preview["outcome"] == "preview", "unexpected preview contract")
            if preview["edits"]:
                _, error = run([compiler, "run", "--source-root", output, source], expected=1)
                require("SRC0024" in error, "original current source was not rejected for reserved names")
            expected_bytes = bytearray()
            cursor = 0
            for edit in preview["edits"]:
                start, end = edit["span"]["start"], edit["span"]["end"]
                require(cursor <= start < end <= len(original), "invalid original edit span")
                require(edit["path"] == source.name, "unexpected edit path")
                old_name = original[start:end].decode("ascii")
                require(preview["names"].get(old_name) == edit["replacement"], "inconsistent rename")
                expected_bytes.extend(original[cursor:start])
                expected_bytes.extend(edit["replacement"].encode("ascii"))
                cursor = end
            expected_bytes.extend(original[cursor:])
            applied = json.loads(run([*migrate, "--apply", source])[0])
            require(applied["edits"] == preview["edits"]
                    and applied["applied"] == int(bool(preview["edits"])), "apply report mismatch")
            rewritten = source.read_bytes()
            require(rewritten == expected_bytes, "apply changed bytes outside original-span edits")
            stdout, _ = run([compiler, "run", "--source-root", output, "--fuel", "10000", source])
            require(stdout == expected, f"current VM result mismatch: {name}")
            package = output / (name + "-native")
            run([compiler, "native", "--source-root", output, "--output", package, source])
            executable = output / (name + suffix)
            run([compiler, "link-native", "--sdk", sdk, "--package", package, "--output", executable])
            stdout, _ = run([executable])
            require(stdout == expected, f"linked native result mismatch: {name}")
            repeated = json.loads(run([*migrate, "--apply", source])[0])
            require(repeated["changed"] == 0 and repeated["applied"] == 0
                    and not repeated["edits"] and source.read_bytes() == rewritten, "repeat is not a no-op")
            report["cases"].append({"name": name, "original": text,
                                    "original_sha256": digest(original),
                                    "rewritten_sha256": digest(rewritten), "preview": preview,
                                    "applied": applied, "repeat": repeated, "stdout": expected,
                                    "native_manifest_sha256": digest((package / "package.json").read_bytes()),
                                    "executable_sha256": digest(executable.read_bytes())})
        require(digest((historical / "Cargo.lock").read_bytes()) == lock, "historical build changed lock")
        report["success"] = True
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (output / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(json.dumps({"report": str(output / "report.json"), "success": report["success"],
                          "cases": len(report["cases"])}))


if __name__ == "__main__":
    main()
