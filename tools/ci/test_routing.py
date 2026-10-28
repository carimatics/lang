"""Exercise real Git comparisons and the final CI gate, including failures."""

import copy
from pathlib import Path
import subprocess
import tempfile
import unittest

from classify_changes import classify, documentation_path
from verify_results import failures, FULL_JOBS


class ChangesTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.git("init", "-q", "-b", "main")
        self.write("README.md", "initial documentation")
        self.write("src/code.rs", "initial implementation")
        self.base = self.commit()

    def git(self, *args):
        return subprocess.check_output(
            ["git", "-c", "user.name=CI", "-c", "user.email=ci@example.invalid",
             "-c", "commit.gpgsign=false", *args],
            cwd=self.root, stderr=subprocess.PIPE,
        ).decode().strip()

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def commit(self):
        self.git("add", "--all")
        self.git("commit", "-qm", "fixture")
        return self.git("rev-parse", "HEAD")

    def push(self, head):
        return classify("push", {"before": self.base, "after": head}, self.root)

    def test_known_documentation_and_configuration_use_the_light_path(self):
        self.write("README.md", "updated")
        self.write("docs/ja/space name.md", "translated documentation")
        self.write(".codex/config.example.toml", "# example")
        self.assertFalse(self.push(self.commit()).full)

    def test_mixed_code_or_unknown_input_requires_full_validation(self):
        for path in ("src/code.rs", "examples/new.lang", "docs/data.json",
                     ".github/workflows/ci.yml", "tools/ci/new.py", "new-input"):
            with self.subTest(path=path):
                self.write(path, "changed")
                head = self.commit()
                self.assertTrue(self.push(head).full)
                self.base = head

    def test_embedded_sdk_documents_are_not_documentation_only(self):
        for path in ("docs/native-sdk.md", "docs/ja/native-sdk.md"):
            with self.subTest(path=path):
                self.write(path, "SDK contents")
                head = self.commit()
                self.assertTrue(self.push(head).full)
                self.base = head

    def test_renaming_source_into_documentation_retains_the_old_path(self):
        (self.root / "docs").mkdir()
        (self.root / "src/code.rs").rename(self.root / "docs/code.md")
        self.assertTrue(self.push(self.commit()).full)

    def test_deleted_code_requires_full_but_deleted_documentation_is_classified(self):
        (self.root / "README.md").unlink()
        self.assertFalse(self.push(self.commit()).full)
        (self.root / "src/code.rs").unlink()
        self.assertTrue(self.push(self.commit()).full)

    def test_pr_uses_merge_base_instead_of_including_unrelated_base_changes(self):
        self.git("switch", "-qc", "feature")
        self.write("README.md", "PR documentation")
        head = self.commit()
        self.git("switch", "-q", "main")
        self.write("src/code.rs", "base branch implementation")
        base = self.commit()
        event = {"pull_request": {"base": {"sha": base}, "head": {"sha": head}}}
        self.assertFalse(classify("pull_request", event, self.root).full)

    def test_empty_missing_or_invalid_comparison_requires_full_validation(self):
        events = [
            ("push", {"before": self.base, "after": self.base}),
            ("push", {"before": "0" * 40, "after": self.base}),
            ("push", {"before": "f" * 40, "after": self.base}),
            ("push", {"before": "--help", "after": self.base}),
            ("pull_request", {}), ("push", None), ("workflow_dispatch", {}),
        ]
        for name, event in events:
            with self.subTest(name=name, event=event):
                self.assertTrue(classify(name, event, self.root).full)

    def test_noncanonical_paths_cannot_alias_an_embedded_document(self):
        for path in ("docs//native-sdk.md", "docs/./native-sdk.md",
                     "docs/../src/code.md", "/docs/research.md", ""):
            self.assertFalse(documentation_path(path))


class ResultTests(unittest.TestCase):
    def needs(self, full):
        result = {"changes": {"result": "success", "outputs": {"full": str(full).lower()}},
                  "documentation": {"result": "skipped" if full else "success"}}
        result.update({job: {"result": "success" if full else "skipped"} for job in FULL_JOBS})
        return result

    def test_both_complete_validation_paths_pass(self):
        self.assertEqual(failures(self.needs(True)), [])
        self.assertEqual(failures(self.needs(False)), [])

    def test_failure_cancellation_and_missing_selected_checks_cannot_pass(self):
        for full in (True, False):
            for job in (FULL_JOBS if full else ("documentation",)):
                for result in ("failure", "cancelled", "skipped", "pending", None):
                    with self.subTest(full=full, job=job, result=result):
                        needs = self.needs(full)
                        needs[job]["result"] = result
                        self.assertTrue(failures(needs))

    def test_invalid_classification_cannot_report_a_light_path_success(self):
        valid = self.needs(False)
        for result in ("failure", "cancelled", "skipped"):
            needs = copy.deepcopy(valid)
            needs["changes"]["result"] = result
            self.assertTrue(failures(needs))
        for outputs in ({}, {"full": ""}, {"full": False}):
            needs = copy.deepcopy(valid)
            needs["changes"]["outputs"] = outputs
            self.assertTrue(failures(needs))
        del valid["native-sdk"]
        self.assertTrue(failures(valid))


if __name__ == "__main__":
    unittest.main()
