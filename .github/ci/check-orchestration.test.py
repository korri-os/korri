#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pyyaml typeshare bash gnused diffutils
"""Exercise the treaty check and validation workflow's actual orchestration."""

import hashlib
import importlib.util
import json
import os
import shlex
import shutil
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


class InputdProfileTest(unittest.TestCase):
    def run_profile(self, args=(), fail_command=""):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "services/inputd/deploy").mkdir(parents=True)
            commands = root / "commands"
            commands.mkdir()
            log = root / "calls.jsonl"
            adapter = root / "command.py"
            adapter.write_text('''import json, os, pathlib, sys
call = sys.argv[1:]
with pathlib.Path(os.environ["COMMAND_LOG"]).open("a") as stream:
    stream.write(json.dumps(call) + "\\n")
if " ".join(call) == os.environ.get("FAIL_COMMAND"):
    sys.exit(7)
if call[:2] == ["nix", "eval"]:
    print("x86_64-linux")
''')
            for name in ("cargo", "shellcheck", "nix", "device-check.sh", "test-device-check.sh", "test-device-bitmap.sh"):
                path = commands / name if not name.endswith(".sh") else root / "services/inputd/deploy" / name
                path.write_text(
                    f"#!{shutil.which('bash')}\nexec {shlex.quote(sys.executable)} {shlex.quote(str(adapter))} {name} \"$@\"\n"
                )
                path.chmod(0o755)
            result = subprocess.run(
                ["bash", str(ROOT / "services/inputd/check-in-shell.sh"), *args],
                env={**os.environ, "KORRI_ROOT": str(root), "PATH": f"{commands}:{os.environ['PATH']}",
                     "COMMAND_LOG": str(log), "FAIL_COMMAND":
                     f"nix run {root}#inputd-plugins-prepare" if fail_command == "signed-preparation" else fail_command},
                capture_output=True, text=True, check=False,
            )
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            return result, calls

    def test_fast_retains_all_rust_wasm_and_shell_commands_without_nix(self):
        result, calls = self.run_profile(["--fast"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [
            ["cargo", "fmt", "--check"],
            ["cargo", "fmt", "--manifest-path", "core/Cargo.toml", "--check"],
            ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
            ["cargo", "clippy", "--manifest-path", "core/Cargo.toml", "--all-targets", "--", "-D", "warnings"],
            ["cargo", "test", "--all-targets"],
            ["cargo", "build", "--bin", "korri-ledger-proof"],
            ["cargo", "test", "--manifest-path", "core/Cargo.toml", "--all-targets"],
            ["cargo", "check", "--manifest-path", "core/Cargo.toml", "--target", "wasm32-unknown-unknown"],
            ["shellcheck", "services/inputd/deploy/device-check.sh", "services/inputd/deploy/test-device-check.sh", "services/inputd/deploy/test-device-bitmap.sh"],
            ["test-device-bitmap.sh"], ["test-device-check.sh"],
        ])

    def test_default_still_runs_fast_then_signed_preparation_and_all_eight_checks(self):
        result, calls = self.run_profile()
        fast, fast_calls = self.run_profile(["--fast"])
        self.assertEqual((result.returncode, fast.returncode), (0, 0), result.stderr)
        self.assertEqual(calls[:len(fast_calls)], fast_calls)
        self.assertEqual(calls[len(fast_calls)], ["nix", "run", calls[len(fast_calls)][2]])
        self.assertTrue(calls[len(fast_calls)][2].endswith("#inputd-plugins-prepare"))
        self.assertEqual(calls[-1], ["nix", "build", "--no-link", *[
            f".#checks.x86_64-linux.{check}" for check in (
                "inputplumber-korri-package", "korri-input-module", "korrid-linux-device-module",
                "korri-linux-host-module", "korri-sunshine-plugin-native", "korri-input-seat-core",
                "korri-input-seat-receiver", "korri-inputd-package",
            )
        ]])

    def test_full_signed_preparation_failure_prevents_module_evaluation(self):
        result, calls = self.run_profile(fail_command="signed-preparation")
        self.assertEqual(result.returncode, 7)
        self.assertEqual(calls[-1][:2], ["nix", "run"])
        self.assertTrue(calls[-1][2].endswith("#inputd-plugins-prepare"))
        self.assertFalse(any(call[:2] == ["nix", "build"] for call in calls))

    def test_invalid_profile_fails_before_any_checks(self):
        for args in (["--typo"], ["--fast", "--fast"]):
            with self.subTest(args=args):
                result, calls = self.run_profile(args)
                self.assertEqual(result.returncode, 2)
                self.assertIn("usage:", result.stderr)
                self.assertEqual(calls, [])

    def test_fast_failure_does_not_reach_module_preparation(self):
        result, calls = self.run_profile(fail_command="cargo test --all-targets")
        self.assertEqual(result.returncode, 7)
        self.assertEqual(calls[-1], ["cargo", "test", "--all-targets"])
        self.assertFalse(any(call[0] == "nix" for call in calls))


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
            self.assertEqual(ci.affected([helper]), {"policy", "cache"})

    def test_explicit_full_schedule_and_ci_full_label_control_the_same_profile(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/korri-checks.yml").read_text())
        triggers = workflow.get("on", workflow.get(True))
        self.assertEqual(triggers["workflow_dispatch"]["inputs"]["mode"]["default"], "full")
        self.assertIn("schedule", triggers)
        selection = next(step for step in workflow["jobs"]["select"]["steps"] if step.get("id") == "plan")
        expression = selection["env"]["MODE"]
        self.assertIn("github.event_name == 'workflow_dispatch' && inputs.mode", expression)
        self.assertIn("github.event_name == 'schedule'", expression)
        self.assertIn("contains(github.event.pull_request.labels.*.name, 'ci-full')", expression)
        self.assertIn("'full' || 'affected'", expression)

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
        full = "fromJSON(needs.select.outputs.plan).mode == 'full'"
        self.assertEqual(steps[kvm]["if"], full)
        self.assertEqual(steps[run]["if"], full)
        fast = next(step for step in steps if step.get("run") == "nix run .#inputd-check -- --fast")
        self.assertEqual(fast["if"], "fromJSON(needs.select.outputs.plan).mode != 'full'")
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
        self.assertIn("inputd-check", task)
        self.assertNotIn("--fast", task)
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

    def test_architecture_lint_apps_declare_the_node_launcher_runtime(self):
        tasks = (ROOT / "nix/tasks.nix").read_text()
        for surface in ["pico", "zone"]:
            with self.subTest(surface=surface):
                task = tasks.split(f"    {surface}-check = {{", 1)[1].split(
                    "\n    };", 1
                )[0]
                inputs = task.split("runtimeInputs = [", 1)[1].split("];", 1)[0]
                self.assertIn("pkgs.bun", inputs)
                self.assertIn("pkgs.nodejs", inputs)

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

    def test_published_acceptance_preserves_the_frozen_main_base_byte_for_byte(self):
        source = (ROOT / ".github/workflows/published-plugin-checks.yml").read_bytes()
        # Actual blob at 7e4bf0500c6dab2c0568230906ddb197dbbf69cf, not moving main.
        self.assertEqual(hashlib.sha256(source).hexdigest(),
                         "ece70c4b1b8d02013eb4c4e08cf552a30b6e2f37c3765f9b0cd40ae9d32ceb22")
        workflow = yaml.safe_load(source)
        self.assertEqual(set(workflow.get("on", workflow.get(True))),
                         {"workflow_dispatch", "pull_request", "push"})
        self.assertEqual(set(workflow["jobs"]), {"acceptance"})
        job = workflow["jobs"]["acceptance"]
        self.assertNotIn("if", job)
        self.assertNotIn("needs", job)
        self.assertEqual(job["timeout-minutes"], 60)
        steps = job["steps"]
        self.assertFalse(any(step.get("id") == "plan" for step in steps))
        self.assertFalse(any(step.get("continue-on-error", False) for step in steps))
        for step in steps[:-2]:
            self.assertNotIn("if", step)
        by_id = {step["id"]: step for step in steps if "id" in step}
        check = by_id["published-plugins-check"]
        self.assertEqual(check["run"], "nix run .#korri-published-plugins-check")
        self.assertEqual(check["timeout-minutes"], 40)
        prepare = by_id["prepare-nix-store"]
        self.assertEqual(prepare["timeout-minutes"], 3)
        self.assertIn("always()", prepare["if"])
        self.assertIn("steps.nix-store.outcome == 'success'", prepare["if"])
        self.assertIn("steps.nix-store.outputs.hit-primary-key != 'true'", prepare["if"])
        self.assertIn("steps.prepare-nix-store.outcome == 'success'", steps[-1]["if"])
        self.assertIn("github.run_id, github.run_attempt", steps[-1]["with"]["primary-key"])

    def test_static_jobs_and_aggregate_dependencies_cover_the_host_plan(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/korri-checks.yml").read_text())
        jobs = workflow["jobs"]
        expected = set(ci.LANES)
        self.assertEqual(set(jobs), expected | {"select", "check"})
        self.assertEqual(set(jobs["check"]["needs"]), expected | {"select"})
        self.assertEqual(set(jobs["select"]["outputs"]), expected | {"plan"})
        self.assertEqual(jobs["check"]["if"], "always()")
        self.assertEqual(jobs["check"]["steps"][-1]["run"],
                         "python3 .github/ci/check-plan.py aggregate")
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        selection_steps = jobs["select"]["steps"]
        self.assertEqual(selection_steps[0]["with"]["fetch-depth"], 2)
        select = next(step for step in selection_steps if step.get("id") == "plan")
        self.assertNotIn("github.event_name == 'push'", select["env"]["MODE"])
        self.assertIn("github.event_name == 'schedule'", select["env"]["MODE"])
        self.assertIn("--fetch-base", select["run"])
        for lane in expected:
            self.assertEqual(jobs[lane]["needs"], "select")
            self.assertEqual(jobs[lane]["if"], f"needs.select.outputs.{lane} == 'true'")
        for lane in ["portal", "fmt", "treaty"]:
            self.assertFalse(any("cache-nix-action" in step.get("uses", "")
                                 for step in jobs[lane]["steps"]))
        for job in jobs.values():
            checkout = [step for step in job["steps"]
                        if step.get("uses", "").startswith("actions/checkout@")]
            self.assertEqual(len(checkout), 1)
            self.assertEqual(checkout[0]["with"]["ref"], "${{ inputs.source_sha || github.sha }}")
            self.assertFalse(checkout[0]["with"]["persist-credentials"])
            guards = [step for step in job["steps"] if "EXPECTED_SOURCE_SHA" in step.get("env", {})]
            self.assertEqual(len(guards), 1)
            self.assertIn('test "$(git rev-parse HEAD)" = "$EXPECTED_SOURCE_SHA"', guards[0]["run"])


if __name__ == "__main__":
    unittest.main()
