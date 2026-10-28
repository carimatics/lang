"""Only known documentation changes may avoid the full tier-1 validation."""

from dataclasses import dataclass
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess


ROOT = Path(__file__).resolve().parents[2]
DOCUMENTATION_FILES = frozenset({
    "README.md", "README.ja.md", "AGENTS.md", "AGENTS.ja.md",
    "CONTRIBUTING.md", "CONTRIBUTING.ja.md",
    ".codex/config.example.toml", ".codex/.gitignore", ".codex/rules/.gitignore",
})
# These documents are embedded in the shipped SDK, not just read by contributors.
SDK_DOCUMENTS = frozenset({"docs/native-sdk.md", "docs/ja/native-sdk.md"})


@dataclass(frozen=True)
class Decision:
    full: bool
    reason: str


def documentation_path(path):
    parts = PurePosixPath(path).parts
    if not path or path.startswith("/") or ".." in parts or str(PurePosixPath(path)) != path:
        return False
    return path in DOCUMENTATION_FILES or (
        path.startswith("docs/") and path.endswith(".md")
        and path not in SDK_DOCUMENTS
    )


def revision(value):
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{40}", value):
        raise ValueError("missing or invalid revision")
    if value == "0" * 40:
        raise ValueError("no previous revision")
    return value


def classify(event_name, event, root=ROOT):
    try:
        if event_name == "pull_request":
            request = event["pull_request"]
            base = revision(request["base"]["sha"])
            head = revision(request["head"]["sha"])
            comparison = f"{base}...{head}"
        elif event_name == "push":
            comparison = f"{revision(event['before'])}..{revision(event['after'])}"
        else:
            return Decision(True, "manual or unrecognized event")
        # Disable rename detection so BOTH paths of a rename are classified.
        # NUL delimiters retain spaces/newlines and avoid Git's display quoting.
        output = subprocess.check_output(
            ["git", "diff", "--name-only", "--no-renames", "-z", comparison, "--"],
            cwd=root, stderr=subprocess.PIPE,
        )
        paths = output.decode("utf-8").rstrip("\0").split("\0") if output else []
        if not paths:
            return Decision(True, "empty comparison")
        full = not all(documentation_path(path) for path in paths)
        return Decision(full, f"{len(paths)} changed paths; " + (
            "implementation or unclassified input" if full else "documentation only"
        ))
    except (KeyError, TypeError, ValueError, OSError, subprocess.CalledProcessError):
        return Decision(True, "comparison unavailable; require full validation")


def main():
    try:
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
        decision = classify(os.environ.get("GITHUB_EVENT_NAME"), event)
    except (KeyError, ValueError, OSError):
        decision = Decision(True, "event unavailable; require full validation")
    print(decision.reason)
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"full={str(decision.full).lower()}\n")


if __name__ == "__main__":
    main()
