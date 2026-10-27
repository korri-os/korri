#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pyyaml typeshare bash gnused diffutils
"""Exercise the treaty check and validation workflow's actual orchestration."""

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "check_plan", Path(__file__).with_name("check-plan.py")
)
ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci)


class TreatyDriftTest(unittest.TestCase):
    def test_generation_drift_fails_without_repairing_checked_in_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "services/korrid/src").mkdir(parents=True)
            (root / "contracts/generated").mkdir(parents=True)
            (root / "services/korrid/src/lib.rs").write_text(
                "#[typeshare]\npub struct CheckTreaty { pub count: u32 }\n"
            )
            output = root / "contracts/generated/korrid.ts"
            output.write_text("stale checked-in treaty\n")
            command = ["bash", str(ROOT / "services/korrid/check-treaty.sh")]
            env = {**os.environ, "KORRI_ROOT": str(root)}
            drift = subprocess.run(
                command, env=env, capture_output=True, text=True, check=False
            )
            self.assertEqual(drift.returncode, 1, drift.stderr)
            self.assertIn("Treaty drift", drift.stderr)
            self.assertEqual(output.read_text(), "stale checked-in treaty\n")
            subprocess.run(
                [*command, "--write"], env=env, check=True, capture_output=True
            )
            generated = output.read_text()
            self.assertIn("CheckTreaty", generated)
            subprocess.run(command, env=env, check=True, capture_output=True)
            self.assertEqual(output.read_text(), generated)
            output.unlink()
            self.assertNotEqual(
                subprocess.run(
                    command, env=env, capture_output=True, check=False
                ).returncode,
                0,
            )

    def test_invalid_mode_fails_instead_of_running_an_incomplete_gate(self):
        result = subprocess.run(
            ["bash", str(ROOT / "services/korrid/check-in-shell.sh"), "--typo"],
            check=False,
            env={**os.environ, "KORRI_ROOT": str(ROOT)},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage:", result.stderr)


class WorkflowContractTest(unittest.TestCase):
    def test_portal_typecheck_precedes_bun_tests(self):
        task = (
            (ROOT / "nix/tasks.nix")
            .read_text()
            .split("    portal-check = {", 1)[1]
            .split("    };", 1)[0]
        )
        self.assertLess(task.index("bun run typecheck"), task.index("bun test"))

    def test_parity_runs_without_installing_nix_or_compiling_rust(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/korri-checks.yml").read_text()
        )
        steps = workflow["jobs"]["parity"]["steps"]
        self.assertEqual(steps[-1]["run"], "python3 clients/cli/tests/parity.py")
        self.assertFalse(any("install-nix" in step.get("uses", "") for step in steps))

    def test_normal_cli_lane_uses_fast_identity_evaluation_not_two_binary_builds(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/korri-checks.yml").read_text()
        )
        commands = [
            step["run"] for step in workflow["jobs"]["cli"]["steps"] if "run" in step
        ]
        self.assertEqual(commands.count("nix run .#cli-build-identity-eval-check"), 1)
        self.assertNotIn("nix run .#cli-build-identity-check", commands)

    def test_static_jobs_and_aggregate_dependencies_cover_full_plan(self):
        for filename, expected, scope in [
            ("korri-checks.yml", set(ci.LANES) - {"acceptance"}, "checks"),
            ("published-plugin-checks.yml", {"acceptance"}, "published"),
        ]:
            with self.subTest(filename=filename):
                workflow = yaml.safe_load(
                    (ROOT / ".github/workflows" / filename).read_text()
                )
                jobs = workflow["jobs"]
                self.assertEqual(set(jobs), expected | {"select", "check"})
                self.assertEqual(set(jobs["check"]["needs"]), expected | {"select"})
                self.assertEqual(jobs["check"]["if"], "always()")
                self.assertIn(
                    f"aggregate --scope {scope}", jobs["check"]["steps"][-1]["run"]
                )
                self.assertEqual(
                    set(jobs["select"]["outputs"]), set(ci.LANES) | {"plan"}
                )
                self.assertEqual(workflow["permissions"], {"contents": "read"})
                selection_steps = jobs["select"]["steps"]
                self.assertEqual(selection_steps[0]["with"]["fetch-depth"], 2)
                select = selection_steps[-1]
                self.assertNotIn("github.event_name == 'push'", select["env"]["MODE"])
                self.assertIn("github.event_name == 'schedule'", select["env"]["MODE"])
                self.assertIn("--fetch-base", select["run"])
                for lane in expected:
                    self.assertEqual(jobs[lane]["needs"], "select")
                    self.assertEqual(
                        jobs[lane]["if"], f"needs.select.outputs.{lane} == 'true'"
                    )
                # Consumers use checked-in treaties. Neither compilation nor a
                # large Nix snapshot can sit in the fast lanes' ancestry.
                if scope == "checks":
                    for lane in ["portal", "fmt", "treaty"]:
                        self.assertFalse(
                            any(
                                "cache-nix-action" in step.get("uses", "")
                                for step in jobs[lane]["steps"]
                            )
                        )
                for job in jobs.values():
                    checkout = [
                        step
                        for step in job["steps"]
                        if step.get("uses", "").startswith("actions/checkout@")
                    ]
                    self.assertEqual(len(checkout), 1)
                    self.assertEqual(
                        checkout[0]["with"]["ref"],
                        "${{ inputs.source_sha || github.sha }}",
                    )
                    self.assertFalse(checkout[0]["with"]["persist-credentials"])
                    checks = [
                        step
                        for step in job["steps"]
                        if "EXPECTED_SOURCE_SHA" in step.get("env", {})
                    ]
                    self.assertEqual(len(checks), 1)
                    self.assertIn(
                        'test "$(git rev-parse HEAD)" = "$EXPECTED_SOURCE_SHA"',
                        checks[0]["run"],
                    )


if __name__ == "__main__":
    unittest.main()
