"""Coverage checks for CI target partitioning, including newly added targets."""

import unittest

from run_tests import commands, test_environment


class PartitionTests(unittest.TestCase):
    def setUp(self):
        self.names = [
            "repository_consistency", "native_owned_boundary", "native_link",
            "new_integration", "native_future_integration", "native_task_execution",
        ]
        self.metadata = {"packages": [{
            "name": "lang",
            "targets": [
                {"name": name, "kind": ["test"]} for name in self.names
            ] + [{"name": "lang", "kind": ["lib"]}],
        }]}

    def test_every_integration_target_runs_exactly_once(self):
        for partition in (("core", "native", "owned"), ("core", "native-all"),
                          ("workspace", "core-1", "core-2", "native", "owned")):
            with self.subTest(partition=partition):
                selected = []
                for shard in partition:
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

    def test_windows_splits_compilation_without_duplicating_non_integration_targets(self):
        workspace = commands(self.metadata, "workspace")
        first = commands(self.metadata, "core-1")
        second = commands(self.metadata, "core-2")
        self.assertEqual(workspace, commands(self.metadata, "core")[:1])
        self.assertEqual(len(first), 1)
        self.assertEqual(len(second), 1)
        for flag in ("--lib", "--bins", "--examples", "--benches"):
            self.assertIn(flag, first[0])
            self.assertNotIn(flag, second[0])
        self.assertNotIn("--workspace", first[0])
        self.assertNotIn("--workspace", second[0])
        # Metadata order cannot change group membership or command order.
        self.metadata["packages"][0]["targets"].reverse()
        self.assertEqual(first, commands(self.metadata, "core-1"))
        self.assertEqual(second, commands(self.metadata, "core-2"))

    def test_heavy_suites_use_separate_runners(self):
        self.assertEqual(commands(self.metadata, "owned")[0][-4:],
                         ["--test", "native_owned_boundary", "--test", "native_task_execution"])
        native = commands(self.metadata, "native")[0]
        self.assertIn("native_future_integration", native)
        self.assertNotIn("native_owned_boundary", native)

    def test_invalid_or_empty_shard_cannot_fall_back_to_running_all_tests(self):
        with self.assertRaises(ValueError):
            commands(self.metadata, "typo")
        self.metadata["packages"][0]["targets"] = []
        with self.assertRaises(ValueError):
            commands(self.metadata, "native")

    def test_optimization_is_opt_in_and_preserves_nested_cargo_checks(self):
        original = {"PATH": "kept", "CARGO_PROFILE_TEST_DEBUG_ASSERTIONS": "false"}
        self.assertEqual(test_environment(original, False), original)
        optimized = test_environment(original, True)
        self.assertEqual(original["CARGO_PROFILE_TEST_DEBUG_ASSERTIONS"], "false")
        self.assertEqual(optimized["PATH"], "kept")
        for profile in ("DEV", "TEST"):
            self.assertEqual(optimized[f"CARGO_PROFILE_{profile}_OPT_LEVEL"], "1")
            self.assertEqual(optimized[f"CARGO_PROFILE_{profile}_DEBUG"], "2")
            for check in ("DEBUG_ASSERTIONS", "OVERFLOW_CHECKS"):
                self.assertEqual(optimized[f"CARGO_PROFILE_{profile}_{check}"], "true")


if __name__ == "__main__":
    unittest.main()
