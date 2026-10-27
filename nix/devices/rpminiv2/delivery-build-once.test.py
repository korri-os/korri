#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p 'python3.withPackages (ps: [ ps.pyyaml ])'
"""Exercise the actual device workflow command with controlled subprocesses."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import yaml


workflow = yaml.safe_load(Path(sys.argv.pop(1)).read_text())
steps = workflow["jobs"]["devices"]["steps"]
publish_steps = [
    step["run"]
    for step in steps
    if ".#korri-nix-cache-publish" in step.get("run", "")
]
assert len(publish_steps) == 1, "expected exactly one device publication command"
COMMAND = publish_steps[0]


class DeviceBuildWorkflowTest(unittest.TestCase):
    def execute(self, device, exit_code):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            calls = root / "calls.jsonl"
            nix = root / "nix"
            nix.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['CALLS'], 'a') as out:\n"
                "    out.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if sys.argv[1] == 'build':\n"
                "    print('controlled build result', file=sys.stderr)\n"
                "    sys.exit(int(os.environ['BUILD_EXIT']))\n"
                "if sys.argv[1] != 'run':\n"
                "    sys.exit(90)\n"
            )
            nix.chmod(0o755)
            # The old wrapper's sleeps must not make a deliberately red
            # regression test take three minutes. This is a real command whose
            # configured behavior is an immediate successful exit.
            sleep = root / "sleep"
            sleep.write_text(f"#!{shutil.which('bash')}\nexit 0\n")
            sleep.chmod(0o755)
            command = root / "workflow-command.sh"
            command.write_text(
                "#!/usr/bin/env nix-shell\n"
                "#! nix-shell -i bash -p bash coreutils\n"
                "set -euo pipefail\n" + COMMAND
            )
            env = os.environ | {
                "PATH": f"{root}:{os.environ['PATH']}",
                "CALLS": str(calls),
                "BUILD_EXIT": str(exit_code),
                "DEVICE": device,
                "RUN_NUMBER": "1",
                "JOB_INDEX": "0",
            }
            result = subprocess.run(
                ["bash", str(command)],
                env=env,
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
            captured = (
                [json.loads(line) for line in calls.read_text().splitlines()]
                if calls.exists()
                else []
            )
            return result, captured

    def test_failed_build_runs_once_and_never_publishes(self):
        for device in ("rg353m", "odin2portal"):
            with self.subTest(device=device):
                result, calls = self.execute(device, 17)
                self.assertEqual(result.returncode, 17, result.stderr)
                self.assertEqual([call[0] for call in calls], ["build"], result.stderr)

    def test_successful_build_runs_once_and_retains_all_output_selection(self):
        for device in ("rg353m", "odin2portal"):
            with self.subTest(device=device):
                result, calls = self.execute(device, 0)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual([call[0] for call in calls], ["build", "run"])
                self.assertIn(
                    f".#nixosConfigurations.{device}.config.system.build.toplevel",
                    calls[0],
                )
                self.assertIn(
                    f".#nixosConfigurations.{device}.config.boot.kernelPackages.kernel^*",
                    calls[0],
                )
                self.assertIn(".#korri-nix-cache-publish", calls[1])
                self.assertIn(device, calls[1])
                if device == "odin2portal":
                    self.assertIn(".#packages.aarch64-linux.korrid.cargoArtifacts^*", calls[0])
                    self.assertIn("packages.aarch64-linux.korrid.cargoArtifacts", calls[1])


if __name__ == "__main__":
    unittest.main()
