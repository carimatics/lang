"""Fail the stable CI check unless every selected job actually succeeded."""

import json
import os
import sys


FULL_JOBS = ("macos", "windows", "native-sdk")


def failures(needs):
    required = {"changes", "documentation", *FULL_JOBS}
    if not isinstance(needs, dict) or set(needs) != required:
        return ["missing or unexpected CI jobs"]
    if any(not isinstance(value, dict) for value in needs.values()):
        return ["invalid CI job state"]
    changes = needs["changes"]
    if changes.get("result") != "success":
        return ["change classification did not succeed"]
    full = changes.get("outputs", {}).get("full")
    if full not in ("true", "false"):
        return ["missing or invalid validation scope"]
    expected = {"documentation": "skipped" if full == "true" else "success"}
    expected.update({job: "success" if full == "true" else "skipped" for job in FULL_JOBS})
    return [f"{job}: expected {result}, got {needs[job].get('result')}"
            for job, result in expected.items() if needs[job].get("result") != result]


def main():
    errors = failures(json.loads(os.environ["CI_RESULTS"]))
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print("All selected CI checks succeeded.")


if __name__ == "__main__":
    main()
