"""Reject incomplete or corrupt benchmark evidence before it can be reported."""
import unittest

import run


class EvidenceTests(unittest.TestCase):
    def matrix(self):
        return "\n".join(f"{w},{n},{v},{v},{v}"
                         for w in run.WORKLOADS for n in run.INPUTS
                         for v in [run.expected(w, n)])

    def sample(self):
        return dict(preparation_ns=10, elapsed_ns=20, checksum=126, result=63,
                    allocations=2, releases=2, requested_bytes=72, live_bytes=0,
                    peak_live_bytes=72, peak_working_set_bytes=0)

    def test_buffer_wrap_and_region_lifecycle_oracles(self):
        self.assertEqual([run.expected("buffers", n) for n in (0, 1, 250, 251, 1000)],
                         [0, 0, 93375, 93625, 748500])
        self.assertEqual([run.expected("regions", n) for n in (0, 1, 64, 1000)],
                         [0, 2, 4160, 1001000])

    def test_complete_matrix(self):
        self.assertEqual(len(run.parse_parity(self.matrix())), 21)

    def test_incomplete_or_duplicate_matrix_rejected(self):
        lines = self.matrix().splitlines()
        for invalid in ("\n".join(lines[:-1]), "\n".join([lines[0]] + lines[:-1])):
            with self.assertRaises(ValueError):
                run.parse_parity(invalid)

    def test_wrong_reference_or_equal_wrong_values_rejected(self):
        for replacement in ("scalar,0,17,18,17", "scalar,0,18,18,18"):
            with self.assertRaises(ValueError):
                run.parse_parity(self.matrix().replace("scalar,0,17,17,17", replacement))

    def test_valid_sample(self):
        run.validate_sample(self.sample(), "buffers", 7, 2)

    def test_wrong_result_checksum_or_retirement_rejected(self):
        for key, value in (("result", 62), ("checksum", 125), ("releases", 1),
                           ("live_bytes", 1), ("elapsed_ns", 0)):
            with self.subTest(key=key), self.assertRaises(ValueError):
                run.validate_sample(dict(self.sample(), **{key: value}), "buffers", 7, 2)

    def test_invalid_schema_or_types_rejected(self):
        for invalid in (dict(self.sample(), unknown=0), dict(self.sample(), checksum=True),
                        dict(self.sample(), elapsed_ns=-1)):
            with self.assertRaises(ValueError):
                run.validate_sample(invalid, "buffers", 7, 2)


if __name__ == "__main__":
    unittest.main()
