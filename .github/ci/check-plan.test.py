#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 git
import copy
import importlib.util
import json
import os
import re
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
            "clients/portal/unknown-runtime.json",
            "clients/portal/src/surface/unknown-runtime.rs",
            "surfaces/pico/src/unknown-runtime.rs",
            "surfaces/pico/unknown-runtime.json",
            "packages/view/src/index.ts",
            "nix/cache/unknown-read-helper.py",
            "nix/cache/identity.nix",
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
            "clients/portal/src/surface/settings-model.ts": {"portal", "parity"}
            | ci.SURFACES,
            "surfaces/pico/src/pico-settings-view.ts": {"pico", "portal", "parity"},
            "clients/portal/src/korrid/client.ts": {"portal", "cli", "korrid"}
            | ci.SURFACES,
            "services/korrid/src/main.rs": {
                "fmt",
                "treaty",
                "korrid",
                "cli",
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
            "clients/cli/src/main.rs": {"fmt", "cli", "parity", "acceptance"},
            "packages/intrinsic-design/intrinsic.css": {"portal"} | ci.SURFACES,
        }
        full = set(ci.LANES)
        for path, expected in probes.items():
            with self.subTest(path=path):
                actual = ci.affected([path])
                self.assertEqual(actual, expected | {"policy"})
                self.assertLessEqual(actual, full)

    def test_inputd_dependency_direction_preserves_the_real_sandbox_consumer(self):
        for path in [
            "services/korrid/src/main.rs",
            "services/korrid/src/config/library.rs",
        ]:
            with self.subTest(path=path):
                self.assertNotIn("inputd", ci.affected([path]))
                self.assertIn("acceptance", ci.affected([path]))
        policy = "services/korrid/src/host/systemd_unit.rs"
        self.assertIn("inputd", ci.affected([policy]))
        consumer = (
            SCRIPT.resolve().parents[2] / "services/inputd/deploy/test-device-check.sh"
        )
        self.assertIn("../../korrid/src/host/systemd_unit.rs", consumer.read_text())
        self.assertIn("korrid", ci.affected(["services/inputd/src/main.rs"]))
        for shared in [
            "services/inputd/nix/korri-input-module-check.nix",
            "contracts/input/src/lib.rs",
            "flake.lock",
        ]:
            with self.subTest(path=shared):
                self.assertIn("inputd", ci.affected([shared]))
        self.assertTrue(ci.plan("full", [], "test")["lanes"]["inputd"])

    def test_cache_inputs_select_their_actual_acceptance_consumer(self):
        workflow = (
            SCRIPT.resolve().parents[2]
            / ".github/workflows/published-plugin-checks.yml"
        ).read_text()
        acceptance = workflow.split("\n  acceptance:\n", 1)[1].split("\n  check:\n", 1)[
            0
        ]
        consumed = set(re.findall(r"\.github/workflows/[\w.-]+", acceptance))
        for path in ci.CACHE_POLICY_INPUTS:
            with self.subTest(path=path):
                expected = {"policy", "cache"}
                if path in consumed:
                    expected.add("acceptance")
                self.assertEqual(ci.affected([path]), expected)
        for path in consumed:
            with self.subTest(acceptance_input=path):
                self.assertIn("acceptance", ci.affected([path]))
        self.assertEqual(ci.affected(["docs/ci.md"]), {"policy"})

    def test_trust_and_snapshot_helpers_cannot_skip_published_acceptance(self):
        for path in [
            ".github/workflows/prepare-nix-cache.py",
            ".github/workflows/test_prepare_nix_cache.py",
            ".github/workflows/trust-korri-cache.sh",
        ]:
            with self.subTest(path=path):
                self.assertEqual(ci.affected([path]), {"policy", "cache", "acceptance"})

    def test_known_frontend_changes_select_no_rust_compilation(self):
        rust = {"cli", "korrid", "inputd", "limitsd", "acceptance"}
        for path in [
            "clients/portal/src/index.css",
            "clients/portal/src/input/keyboard-adapter.ts",
            "clients/portal/src/overlay/OverlayRoot.tsx",
            "clients/portal/src/surface/SurfaceRoot.tsx",
            "clients/portal/src/surface/settings-model.ts",
            "surfaces/pico/src/ui/atoms/PicoClock.css",
            "surfaces/boxbuster/src/BoxbusterSurface.tsx",
            "surfaces/zone/src/plan/options.ts",
        ]:
            with self.subTest(path=path):
                selected = ci.affected([path])
                self.assertIn("portal", selected)
                self.assertFalse(selected & rust)
                self.assertEqual("parity" in selected, path in ci.PARITY_INPUTS)
        self.assertEqual(
            ci.affected(["clients/cli/tests/parity.py"]), {"policy", "parity"}
        )

    def test_every_existing_parity_producer_selects_the_cheap_guard(self):
        for path in ci.PARITY_INPUTS:
            with self.subTest(path=path):
                self.assertIn("parity", ci.affected([path]))

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

    def test_bounded_exact_sha_fetch_recovers_a_multi_commit_push_base(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            origin = root / "origin"
            origin.mkdir()

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", str(origin), *args], text=True
                ).strip()

            git("init", "-q")
            git("config", "user.email", "ci-test@example.invalid")
            git("config", "user.name", "CI test")
            (origin / "docs").mkdir()
            base = None
            for index in range(6):
                (origin / "docs/ci.md").write_text(f"operation {index}")
                git("add", ".")
                git("commit", "-qm", f"change {index}")
                if base is None:
                    base = git("rev-parse", "HEAD")
            checkout = root / "checkout"
            subprocess.run(
                [
                    "git",
                    "clone",
                    "--quiet",
                    "--depth=2",
                    origin.as_uri(),
                    str(checkout),
                ],
                check=True,
            )
            missing = subprocess.run(
                ["git", "cat-file", "-e", base],
                cwd=checkout,
                check=False,
                capture_output=True,
            )
            self.assertNotEqual(missing.returncode, 0)
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "select", "--base", base, "--fetch-base"],
                cwd=checkout,
                check=True,
                capture_output=True,
                text=True,
            )
            selection = json.loads(result.stdout)
            self.assertEqual(selection["mode"], "affected")
            self.assertEqual(
                {lane for lane, enabled in selection["lanes"].items() if enabled},
                {"policy"},
            )
            self.assertIn("check selection elapsed:", result.stderr)
            disconnected = git(
                "commit-tree", "HEAD^{tree}", "-m", "disconnected event base"
            )
            for invalid in ["0" * 40, "HEAD", "f" * 40, disconnected]:
                failed = subprocess.check_output(
                    [
                        sys.executable,
                        str(SCRIPT),
                        "select",
                        "--base",
                        invalid,
                        "--fetch-base",
                    ],
                    cwd=checkout,
                    text=True,
                )
                self.assertEqual(json.loads(failed)["mode"], "full")

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
