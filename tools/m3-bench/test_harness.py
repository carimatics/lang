"""Replay real evidence and reject corrupt observations without measuring CI."""
import copy
import json
from pathlib import Path
import tempfile
import unittest

import run

ROOT = Path(__file__).resolve().parents[2]
CAPTURE = ROOT / "benchmarks/results/2026-10-27-m3-macos-c9ffd19.json"


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        capture = json.loads(CAPTURE.read_text(encoding="utf-8"))
        self.results = [capture["tasks"], *capture["components"]]
        self.samples = capture["tasks"]["samples"]
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # Replay the archived macOS capture's LF inputs even from a Windows
        # CRLF checkout. Production validation still hashes exact input bytes.
        for source, _ in run.WORKLOADS.values():
            if source.startswith("inline:"):
                continue
            path = self.root / source
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((ROOT / source).read_bytes().replace(b"\r\n", b"\n"))

    def validate(self):
        run.validate_results(self.results, self.samples, self.root)

    def test_existing_release_capture(self):
        self.validate()

    def test_measurement_hashes_exact_source_bytes_including_newlines(self):
        source = self.root / self.results[0]["workloads"][0]["source"]
        source.write_bytes(source.read_bytes().replace(b"\n", b"\r\n"))
        with self.assertRaisesRegex(ValueError, "source digest"):
            self.validate()

    def test_duplicate_workload_cannot_replace_missing_coverage(self):
        rows = self.results[0]["workloads"]
        rows[-1] = copy.deepcopy(rows[0])
        with self.assertRaisesRegex(ValueError, "workload matrix"):
            self.validate()

    def test_vm_capture_cannot_substitute_for_jit(self):
        self.results[2] = copy.deepcopy(self.results[1])
        with self.assertRaisesRegex(ValueError, "backend"):
            self.validate()

    def test_source_and_oracle_must_match(self):
        for key, value in (("source", "../../outside.lang"), ("expected", 0),
                           ("sha256", "0" * 64)):
            with self.subTest(key=key):
                self.setUp()
                self.results[0]["workloads"][0][key] = value
                with self.assertRaises(ValueError):
                    self.validate()

    def test_invalid_durations_and_false_summary_rejected(self):
        for value in (-1, True, 0, 1.5, "10", 2**64):
            with self.subTest(value=value):
                self.setUp()
                self.results[0]["workloads"][0]["vm"]["samples_ns"][0] = value
                with self.assertRaises(ValueError):
                    self.validate()
        self.setUp()
        self.results[1]["entry"]["median_ns"] += 1
        with self.assertRaisesRegex(ValueError, "summary"):
            self.validate()

    def test_missing_samples_and_unsupported_schema_rejected(self):
        self.results[2]["entry"]["samples_ns"].pop()
        with self.assertRaises(ValueError):
            self.validate()
        for value in (2, True):
            self.setUp()
            self.results[0]["version"] = value
            with self.assertRaises(ValueError):
                self.validate()

    def test_invalid_counters_and_lifecycle_rejected(self):
        for value in (-1, True, "0"):
            self.setUp()
            self.results[0]["workloads"][0]["counters"]["final_live_tasks"] = value
            with self.assertRaises(ValueError):
                self.validate()
        for key, value in (("live_allocations_after_each_call", 1), ("warmups", 0)):
            self.setUp()
            self.results[1]["entry"][key] = value
            with self.assertRaises(ValueError):
                self.validate()
        self.setUp()
        self.results[2]["entry"]["counters"]["fuel"] += 1
        with self.assertRaisesRegex(ValueError, "counters differ"):
            self.validate()


if __name__ == "__main__":
    unittest.main()
