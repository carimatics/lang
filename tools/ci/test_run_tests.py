"""Coverage checks for CI target partitioning, including newly added targets."""

import unittest

from run_tests import SHARDS, commands


class PartitionTests(unittest.TestCase):
    def setUp(self):
        self.names = [
            "repository_consistency", "native_owned_boundary", "native_link",
            "new_integration", "native_future_integration",
        ]
        self.metadata = {"packages": [{
            "name": "lang",
            "targets": [
                {"name": name, "kind": ["test"]} for name in self.names
            ] + [{"name": "lang", "kind": ["lib"]}],
        }]}

    def test_every_integration_target_runs_exactly_once(self):
        selected = []
        for shard in SHARDS:
            for command in commands(self.metadata, shard):
                selected.extend(command[i + 1] for i, arg in enumerate(command)
                                if arg == "--test")
                self.assertIn("--locked", command)
                self.assertIn("--all-features", command)
        self.assertCountEqual(selected, self.names)

    def test_core_keeps_other_workspace_members_and_all_non_integration_targets(self):
        workspace, root = commands(self.metadata, "core")
        self.assertEqual(workspace[-4:], ["--workspace", "--exclude", "lang", "--all-targets"])
        for flag in ("--lib", "--bins", "--examples", "--benches"):
            self.assertIn(flag, root)
        self.assertNotIn("--all-targets", root)

    def test_heavy_suites_use_separate_runners(self):
        self.assertEqual(commands(self.metadata, "owned")[0][-2:],
                         ["--test", "native_owned_boundary"])
        native = commands(self.metadata, "native")[0]
        self.assertIn("native_future_integration", native)
        self.assertNotIn("native_owned_boundary", native)

    def test_invalid_or_empty_shard_cannot_fall_back_to_running_all_tests(self):
        with self.assertRaises(ValueError):
            commands(self.metadata, "typo")
        self.metadata["packages"][0]["targets"] = []
        with self.assertRaises(ValueError):
            commands(self.metadata, "native")


if __name__ == "__main__":
    unittest.main()
