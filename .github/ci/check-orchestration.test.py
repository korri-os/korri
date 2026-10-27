#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pyyaml typeshare bash gnused diffutils
"""Exercise the treaty check and validation workflow's actual orchestration."""

import importlib.util
import json
import os
import subprocess
import sys
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
    def test_inputd_prepares_only_signed_native_inputs_at_its_module_boundary(self):
        source = (ROOT / "services/inputd/check-in-shell.sh").read_text()
        preparation = source.index('nix run "$ROOT#inputd-plugins-prepare"')
        self.assertLess(source.index("cargo test --all-targets"), preparation)
        self.assertLess(source.index("services/inputd/deploy/test-device-check.sh\n"), preparation)
        self.assertLess(preparation, source.index("nix build --no-link"))
        tasks = (ROOT / "nix/tasks.nix").read_text()
        task = tasks.split("    inputd-plugins-prepare = {", 1)[1].split("\n    };", 1)[0]
        self.assertIn("pkgs.nix", task)
        self.assertIn("pkgs.jq", task)
        self.assertIn('fetch-image-plugins.sh" --inputd', task)
        self.assertNotIn("korri-published-plugins-check", task)
        for helper in (".github/workflows/fetch-image-plugins.sh", "nix/cache/plugin-read-failure.py"):
            self.assertIn("inputd", ci.affected([helper]))

    def test_cache_app_runs_real_plugin_read_cases_outside_a_builder(self):
        tasks = (ROOT / "nix/tasks.nix").read_text()
        cache = tasks.split("    korri-nix-cache-check = {", 1)[1].split("\n    };", 1)[0]
        self.assertIn('python3 "$KORRI_ROOT/nix/cache/prepare.test.py"', cache)
        self.assertIn('python3 "$KORRI_ROOT/nix/cache/plugin-read-failure.test.py"', cache)
        fixture = (ROOT / "nix/cache/prepare.test.py").read_text()
        self.assertIn("def test_plugin_reads_use_real_signed_receiving_stores", fixture)
        self.assertIn(".github/workflows/test_fetch_image_plugins.py", fixture)
        self.assertNotIn("nix build", cache)

    def test_inputd_prepares_real_kvm_access_before_nix_installation(self):
        checks = yaml.safe_load(
            (ROOT / ".github/workflows/korri-checks.yml").read_text()
        )
        published = yaml.safe_load(
            (ROOT / ".github/workflows/published-plugin-checks.yml").read_text()
        )
        steps = checks["jobs"]["inputd"]["steps"]
        kvm = next(
            index
            for index, step in enumerate(steps)
            if step["name"] == "Enable the disposable Inputd module VMs"
        )
        install = next(
            index for index, step in enumerate(steps) if step["name"] == "Install Nix"
        )
        run = next(
            index
            for index, step in enumerate(steps)
            if step.get("run") == "nix run .#inputd-check"
        )
        self.assertLess(kvm, install)
        self.assertLess(install, run)
        established = next(
            step
            for step in published["jobs"]["acceptance"]["steps"]
            if step["name"] == "Enable the disposable lifecycle VM"
        )
        self.assertEqual(steps[kvm]["run"], established["run"])
        self.assertIn("sudo udevadm control --reload-rules", steps[kvm]["run"])
        self.assertIn(
            "sudo udevadm trigger --settle --name-match=kvm", steps[kvm]["run"]
        )
        self.assertIn("test -r /dev/kvm -a -w /dev/kvm", steps[kvm]["run"])

    def test_retained_host_coverage_does_not_raise_the_original_thirty_minute_budget(
        self,
    ):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/korri-checks.yml").read_text()
        )
        for lane in ["korrid", "portal", "fmt", "treaty"]:
            with self.subTest(lane=lane):
                self.assertLessEqual(workflow["jobs"][lane]["timeout-minutes"], 30)

    def test_cache_and_existing_delivery_checks_run_once_per_full_local_check(self):
        task = (
            (ROOT / "nix/tasks.nix")
            .read_text()
            .split("    ci-full-check = {", 1)[1]
            .split("    };", 1)[0]
        )
        for app in ["korri-nix-cache-check", "rpminiv2-delivery-check"]:
            self.assertEqual(task.count(app), 1)
        cache = (
            (ROOT / "nix/tasks.nix")
            .read_text()
            .split("    korri-nix-cache-check = {", 1)[1]
            .split("    };", 1)[0]
        )
        self.assertEqual(cache.count("test_prepare_nix_cache.py"), 1)
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/korri-checks.yml").read_text()
        )
        commands = [
            step["run"] for step in workflow["jobs"]["cache"]["steps"] if "run" in step
        ]
        self.assertEqual(commands.count("nix run .#korri-nix-cache-check"), 1)
        self.assertEqual(commands.count("nix run .#rpminiv2-delivery-check"), 1)

    def test_zone_font_builder_receives_the_locked_native_package_source(self):
        task = (
            (ROOT / "nix/tasks.nix")
            .read_text()
            .split("    zone-check = {", 1)[1]
            .split("    };", 1)[0]
        )
        self.assertIn('env.NIX_PATH = "nixpkgs=${pkgs.path}";', task)
        self.assertIn('bun test "$@"', task)

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

    def test_one_job_acceptance_keeps_all_heavy_steps_conditional_and_bounded(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/published-plugin-checks.yml").read_text()
        )
        job = workflow["jobs"]["acceptance"]
        self.assertEqual(job["timeout-minutes"], 60)
        steps = job["steps"]
        plan = next(
            index for index, step in enumerate(steps) if step.get("id") == "plan"
        )
        self.assertEqual(
            steps[plan - 1]["run"], "python3 .github/ci/check-plan.test.py"
        )
        expected_heavy = [
            "Enable the disposable lifecycle VM",
            "Install Nix",
            "Test Nix cache preparation",
            "Name the Nix store snapshot",
            "Restore the Nix store",
            "Read existing signed Core outputs",
            "Test Core without rebuilding or publishing plugins",
            "Prepare the Nix store for caching",
            "Save the Nix store",
        ]
        heavy = steps[plan + 1 : -1]
        self.assertEqual([step["name"] for step in heavy], expected_heavy)
        for step in heavy:
            self.assertIn("steps.plan.outputs.acceptance == 'true'", step["if"])
        self.assertFalse(any(step.get("continue-on-error", False) for step in steps))
        by_id = {step["id"]: step for step in steps if "id" in step}
        self.assertEqual(
            by_id["published-plugins-check"]["run"],
            "nix run .#korri-published-plugins-check",
        )
        self.assertEqual(by_id["published-plugins-check"]["timeout-minutes"], 40)
        prepare = by_id["prepare-nix-store"]
        self.assertEqual(prepare["timeout-minutes"], 3)
        self.assertIn("always()", prepare["if"])
        self.assertIn("steps.nix-store.outcome == 'success'", prepare["if"])
        self.assertIn(
            "steps.nix-store.outputs.hit-primary-key != 'true'", prepare["if"]
        )
        save = heavy[-1]
        self.assertIn("always()", save["if"])
        self.assertIn("steps.prepare-nix-store.outcome == 'success'", save["if"])
        self.assertIn(
            "steps.published-plugins-check.outcome == 'success'",
            save["with"]["primary-key"],
        )
        self.assertIn("github.run_id, github.run_attempt", save["with"]["primary-key"])
        final = steps[-1]
        self.assertEqual(final["if"], "always()")
        self.assertEqual(final["env"]["PLAN"], "${{ steps.plan.outputs.plan }}")

    def test_published_step_outcome_adapter_fails_closed_through_the_real_command(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/published-plugin-checks.yml").read_text()
        )
        final = workflow["jobs"]["acceptance"]["steps"][-1]
        full = ci.plan("full", [], "test")
        omitted = ci.plan("probe", ["docs/ci.md"], "test")
        cases = [
            (full, "success", "success", True),
            (omitted, "success", "skipped", True),
            (omitted, "success", "success", False),
            (omitted, "failure", "skipped", False),
            (omitted, "success", "", False),
            (None, "success", "skipped", False),
        ]
        for outcome in ["failure", "skipped", "cancelled", ""]:
            cases.append((full, "success", outcome, False))
            cases.append((full, outcome, "success", False))
        for selection, selector, check, succeeds in cases:
            with self.subTest(
                selector=selector, check=check, selected=selection == full
            ):
                needs = (
                    final["env"]["NEEDS"]
                    .replace("${{ steps.plan.outcome }}", selector)
                    .replace("${{ steps.published-plugins-check.outcome }}", check)
                )
                result = subprocess.run(
                    [
                        sys.executable,
                        str(ROOT / ".github/ci/check-plan.py"),
                        "aggregate",
                        "--scope",
                        "published",
                    ],
                    env={
                        **os.environ,
                        "PLAN": json.dumps(selection) if selection is not None else "",
                        "NEEDS": needs,
                    },
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(
                    result.returncode == 0, succeeds, result.stdout + result.stderr
                )

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
                if scope == "checks":
                    self.assertEqual(set(jobs), expected | {"select", "check"})
                    self.assertEqual(set(jobs["check"]["needs"]), expected | {"select"})
                    self.assertEqual(
                        set(jobs["select"]["outputs"]), set(ci.LANES) | {"plan"}
                    )
                    aggregate_job = jobs["check"]
                    selection_steps = jobs["select"]["steps"]
                else:
                    self.assertEqual(set(jobs), {"acceptance"})
                    aggregate_job = jobs["acceptance"]
                    selection_steps = aggregate_job["steps"]
                    self.assertNotIn("needs", aggregate_job)
                    self.assertEqual(
                        aggregate_job["name"], "Published plugin acceptance result"
                    )
                self.assertEqual(aggregate_job["if"], "always()")
                self.assertIn(
                    f"aggregate --scope {scope}", aggregate_job["steps"][-1]["run"]
                )
                self.assertEqual(workflow["permissions"], {"contents": "read"})
                self.assertEqual(selection_steps[0]["with"]["fetch-depth"], 2)
                select = next(
                    step for step in selection_steps if step.get("id") == "plan"
                )
                self.assertNotIn("github.event_name == 'push'", select["env"]["MODE"])
                self.assertIn("github.event_name == 'schedule'", select["env"]["MODE"])
                self.assertIn("--fetch-base", select["run"])
                if scope == "checks":
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
