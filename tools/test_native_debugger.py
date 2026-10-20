"""Regression checks for the real-debugger evidence validator."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "native_debugger", Path(__file__).with_name("check-native-debugger.py"))
DEBUG = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DEBUG)


class LogicalStackEvidence(unittest.TestCase):
    def trace(self):
        records = []
        for function, expression in [(3, "debug_outer(debug_make(41))"),
                                     (2, "debug_take(b)"),
                                     (1, "original + 1")]:
            start = DEBUG.OWNED.index(expression)
            records.append(f"__LANG_FRAME__ {function} 0 {start} {start + len(expression)}")
        return "__LANG_VIEW__ 1 3 48\n" + "\n".join(records) + "\n"

    def test_accepts_live_chain_and_call_sites(self):
        frames = DEBUG.check_logical_stack(self.trace())
        self.assertEqual([f["name"] for f in frames], ["main", "debug_outer", "debug_take"])
        self.assertEqual([f["line"] for f in frames], [15, 12, 8])

    def test_rejects_only_physical_frames(self):
        with self.assertRaises(RuntimeError):
            DEBUG.check_logical_stack("lang_codegen_clif::owned::run\n")

    def test_rejects_missing_or_swapped_parent(self):
        lines = self.trace().splitlines()
        for trace in ["\n".join(lines[:-1]), "\n".join([lines[0], lines[2], lines[1], lines[3]])]:
            with self.subTest(trace=trace), self.assertRaises(RuntimeError):
                DEBUG.check_logical_stack(trace)

    def test_rejects_wrong_call_site_and_revision(self):
        for trace in [self.trace().replace("__LANG_FRAME__ 2 0 ", "__LANG_FRAME__ 2 1 "),
                      self.trace().replace("__LANG_VIEW__ 1 ", "__LANG_VIEW__ 2 ")]:
            with self.subTest(trace=trace), self.assertRaises(RuntimeError):
                DEBUG.check_logical_stack(trace)


if __name__ == "__main__":
    unittest.main()
