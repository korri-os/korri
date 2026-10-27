#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 git
import copy
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check-plan.py")
spec = importlib.util.spec_from_file_location("check_plan", SCRIPT)
ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci)


class SelectionTest(unittest.TestCase):
    def test_full_mode_selects_every_documented_lane(self):
        self.assertTrue(all(ci.plan("full", [], "test")["lanes"].values()))

    def test_known_nonfixture_document_can_skip_checks(self):
        self.assertEqual(ci.affected(["docs/agents/domain.md"]), {"policy"})

    def test_unknown_and_fixture_inputs_broaden_to_full(self):
        for path in [
            "docs/research/retroarch-plugin-route/device.yaml",
            "docs/research/retroarch-effect-quickjs-probe/src/main.rs",
            "docs/research/android-app-plugin-schema-checkpoint/device.yaml",
            "nix/product/game-library-access.nix",
            "clients/portal/nix/nixos-module.nix",
            "clients/cli/package.nix",
            "services/korrid/check-portal-setup.sh",
            "services/korrid/check-in-shell.sh",
            "services/inputd/nix/korri-linux-host.nix",
            "docs/new-guide.md",
            "unknown/file",
            "surfaces/new/src.ts",
            ".github/workflows/korri-checks.yml",
            "flake.lock",
            "clients/cli/Cargo.lock",
            "clients/portal/bun.lock",
            "contracts/generated/korrid.ts",
            "nix/product/requirements.nix",
        ]:
            with self.subTest(path=path):
                self.assertEqual(ci.affected([path]), set(ci.LANES))

    def test_path_probes_are_subsets_of_full_and_include_consumers(self):
        probes = {
            "clients/portal/src/surface/settings-model.ts": {"portal", "cli", "korrid"}
            | ci.SURFACES,
            "surfaces/pico/src/pico-settings-view.ts": {"pico", "portal", "cli"},
            "services/korrid/src/main.rs": {
                "fmt",
                "treaty",
                "korrid",
                "cli",
                "inputd",
                "acceptance",
            },
            "services/inputd/src/main.rs": {
                "fmt",
                "inputd",
                "korrid",
                "cli",
                "acceptance",
            },
            "services/limitsd/src/main.rs": {
                "fmt",
                "limitsd",
                "korrid",
                "cli",
                "acceptance",
            },
            "clients/cli/src/main.rs": {"fmt", "cli", "acceptance"},
            "packages/view/src/index.ts": {"portal", "cli", "korrid"} | ci.SURFACES,
        }
        full = set(ci.LANES)
        for path, expected in probes.items():
            with self.subTest(path=path):
                actual = ci.affected([path])
                self.assertEqual(actual, expected | {"policy"})
                self.assertLessEqual(actual, full)

    def test_invalid_paths_fail(self):
        for path in ["", "../outside", "/absolute", "docs/ci.md\ninjected"]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                ci.affected([path])

    def test_actual_git_diff_includes_deleted_and_both_renamed_paths(self):
        with tempfile.TemporaryDirectory() as directory:

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", directory, *args], text=True
                ).strip()

            git("init", "-q")
            git("config", "user.email", "ci-test@example.invalid")
            git("config", "user.name", "CI test")
            root = Path(directory)
            (root / "docs").mkdir()
            (root / "docs/ci.md").write_text("operation")
            (root / "removed fixture").write_text("fixture")
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            git("mv", "docs/ci.md", "docs/runtime.yaml")
            (root / "removed fixture").unlink()
            git("add", "-A")
            git("commit", "-qm", "rename and delete")
            result = subprocess.check_output(
                [sys.executable, str(SCRIPT), "select", "--base", base],
                cwd=directory,
                text=True,
            )
            self.assertTrue(all(json.loads(result)["lanes"].values()))
            old_cwd = os.getcwd()
            try:
                os.chdir(directory)
                self.assertEqual(
                    set(ci.changed_paths(base, "HEAD")),
                    {
                        "docs/ci.md",
                        "docs/runtime.yaml",
                        "removed fixture",
                    },
                )
            finally:
                os.chdir(old_cwd)

    def test_unavailable_diff_runs_full_and_emits_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "outputs"
            result = subprocess.check_output(
                [sys.executable, str(SCRIPT), "select", "--base", "missing-base"],
                env={**os.environ, "GITHUB_OUTPUT": str(output)},
                text=True,
            )
            self.assertEqual(json.loads(result)["mode"], "full")
            self.assertIn("portal=true", output.read_text())
            self.assertIn("plan=", output.read_text())

    def test_missing_base_broadens_and_empty_reviewed_diff_still_checks_policy(self):
        result = subprocess.check_output(
            [sys.executable, str(SCRIPT), "select"], text=True
        )
        self.assertEqual(json.loads(result)["mode"], "full")
        self.assertEqual(ci.affected([]), {"policy"})

    def test_manual_probe_uses_same_selection_and_rejects_bad_json(self):
        for paths in ["{}", "[]", "[1]", "not json", '["../outside"]']:
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "select",
                    "--mode",
                    "probe",
                    "--paths",
                    paths,
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0, paths)
        result = subprocess.check_output(
            [
                sys.executable,
                str(SCRIPT),
                "select",
                "--mode",
                "probe",
                "--paths",
                '["docs/ci.md"]',
            ],
            text=True,
        )
        self.assertEqual(
            {
                lane
                for lane, selected in json.loads(result)["lanes"].items()
                if selected
            },
            {"policy"},
        )


class AggregateTest(unittest.TestCase):
    def setUp(self):
        self.selection = ci.plan("affected", ["surfaces/pico/src/index.ts"], "test")
        self.needs = {"select": {"result": "success"}}
        self.needs.update(
            {
                lane: {"result": "success" if selected else "skipped"}
                for lane, selected in self.selection["lanes"].items()
                if lane != "acceptance"
            }
        )

    def test_selected_success_and_only_expected_skips_pass(self):
        ci.aggregate(self.selection, self.needs, "checks")
        ci.aggregate(
            self.selection,
            {"select": {"result": "success"}, "acceptance": {"result": "skipped"}},
            "published",
        )

    def test_failure_skip_cancel_or_missing_selected_result_fails(self):
        for result in ["failure", "skipped", "cancelled", None, ""]:
            needs = copy.deepcopy(self.needs)
            needs["portal"]["result"] = result
            with self.subTest(result=result), self.assertRaises(ValueError):
                ci.aggregate(self.selection, needs, "checks")

    def test_missing_extra_or_unexpected_jobs_fail(self):
        for change in ["missing", "extra", "unexpected_success"]:
            needs = copy.deepcopy(self.needs)
            if change == "missing":
                del needs["portal"]
            elif change == "extra":
                needs["unknown"] = {"result": "success"}
            else:
                needs["treaty"]["result"] = "success"
            with self.subTest(change=change), self.assertRaises(ValueError):
                ci.aggregate(self.selection, needs, "checks")

    def test_selection_failure_or_malformed_plan_fails_closed(self):
        for result in ["failure", "skipped", "cancelled", None]:
            needs = copy.deepcopy(self.needs)
            needs["select"]["result"] = result
            with self.subTest(result=result), self.assertRaises(ValueError):
                ci.aggregate(self.selection, needs, "checks")
        malformed = [
            None,
            {},
            {**self.selection, "lanes": {}},
            {**self.selection, "mode": "full"},
            {**self.selection, "lanes": {**self.selection["lanes"], "policy": False}},
            {**self.selection, "lanes": {**self.selection["lanes"], "portal": "true"}},
        ]
        for selection in malformed:
            with self.subTest(selection=selection), self.assertRaises(ValueError):
                ci.aggregate(selection, self.needs, "checks")

    def test_full_success_for_both_workflows(self):
        selection = ci.plan("full", [], "test")
        for scope, lanes in [
            ("checks", set(ci.LANES) - {"acceptance"}),
            ("published", {"acceptance"}),
        ]:
            needs = {lane: {"result": "success"} for lane in lanes | {"select"}}
            ci.aggregate(selection, needs, scope)

    def test_every_selected_lane_fails_closed_in_its_workflow_scope(self):
        selection = ci.plan("full", [], "test")
        for scope, lanes in [
            ("checks", set(ci.LANES) - {"acceptance"}),
            ("published", {"acceptance"}),
        ]:
            for lane in lanes:
                for result in ["failure", "skipped", "cancelled", None]:
                    needs = {name: {"result": "success"} for name in lanes | {"select"}}
                    needs[lane]["result"] = result
                    with (
                        self.subTest(scope=scope, lane=lane, result=result),
                        self.assertRaises(ValueError),
                    ):
                        ci.aggregate(selection, needs, scope)

    def test_real_aggregate_command_reports_failure(self):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "aggregate", "--scope", "checks"],
            env={**os.environ, "PLAN": json.dumps(self.selection), "NEEDS": "{}"},
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("dependencies", result.stderr)


if __name__ == "__main__":
    unittest.main()
