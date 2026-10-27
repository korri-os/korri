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
helper = Path(sys.argv.pop(1))
steps = workflow["jobs"]["devices"]["steps"]
publish_steps = [
    step["run"] for step in steps if ".#korri-nix-cache-publish" in step.get("run", "")
]
assert len(publish_steps) == 1, "expected exactly one device publication command"
COMMAND = publish_steps[0]
# The terminal format is recorded in run 39761993867's actual Nix output.
READ_ERROR = (
    "error: unable to download 'https://korri-os.github.io/nix-cache/"
    "zhir3wxi0bkvsz7ab21l1hw9wn7smw2j.narinfo': HTTP error 429\n"
)

# Real Nix renders a missing payload differently from an HTTP429 narinfo read.
NATIVE_PATH = "/nix/store/jzr3z7lvfq87w2zxmdj9p5a403zplg5p-korri-read-recovery-fixture"
NATIVE_COPY = f"copying path '{NATIVE_PATH}' from 'http://127.0.0.1:1'...\n"
NATIVE_MISSING = "error: file 'nar/payload.nar.zst' does not exist in binary cache 'http://127.0.0.1:1'\n"
NATIVE_REQUIRED = f"error: path '{NATIVE_PATH}' is required, but there is no substituter that can build it\n"


class DeviceBuildWorkflowTest(unittest.TestCase):
    def execute(self, device, results):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            calls = root / "calls.jsonl"
            installed_helper = root / "nix/cache/build-with-read-retry.py"
            installed_helper.parent.mkdir(parents=True)
            shutil.copyfile(helper, installed_helper)
            nix = root / "nix-command"
            nix.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "from pathlib import Path\n"
                "calls = Path(os.environ['CALLS'])\n"
                "with calls.open('a') as out:\n"
                "    out.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if sys.argv[1] == 'build':\n"
                "    count = sum(json.loads(line)[0] == 'build' for line in calls.read_text().splitlines())\n"
                "    results = json.loads(os.environ['BUILD_RESULTS'])\n"
                "    status, message = results[min(count - 1, len(results) - 1)]\n"
                "    print(message, file=sys.stderr, end='')\n"
                "    sys.exit(status)\n"
                "if sys.argv[1] != 'run':\n"
                "    sys.exit(90)\n"
            )
            nix.chmod(0o755)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            (bin_dir / "nix").symlink_to(nix)
            # Exercise the original 60/120-second backoff arguments without
            # waiting in regression tests. This is a real configured command.
            sleep = bin_dir / "sleep"
            sleep.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['CALLS'], 'a') as out:\n"
                "    out.write(json.dumps(['sleep', *sys.argv[1:]]) + '\\n')\n"
            )
            sleep.chmod(0o755)
            command = root / "workflow-command.sh"
            command.write_text(
                "#!/usr/bin/env nix-shell\n"
                "#! nix-shell -i bash -p bash coreutils\n"
                "set -euo pipefail\n" + COMMAND
            )
            env = os.environ | {
                "PATH": f"{bin_dir}:{os.environ['PATH']}",
                "CALLS": str(calls),
                "BUILD_RESULTS": json.dumps(results),
                "DEVICE": device,
                "RUN_NUMBER": "1",
                "JOB_INDEX": "0",
            }
            result = subprocess.run(
                ["bash", str(command)],
                cwd=root,
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
                result, calls = self.execute(
                    device, [(17, "controlled compiler failure")]
                )
                self.assertEqual(result.returncode, 17, result.stderr)
                self.assertEqual([call[0] for call in calls], ["build"], result.stderr)

    def test_successful_build_runs_once_and_retains_all_output_selection(self):
        for device in ("rg353m", "odin2portal"):
            with self.subTest(device=device):
                result, calls = self.execute(device, [(0, "")])
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
                    self.assertIn(
                        ".#packages.aarch64-linux.korrid.cargoArtifacts^*", calls[0]
                    )
                    self.assertIn(
                        "packages.aarch64-linux.korrid.cargoArtifacts", calls[1]
                    )

    def test_terminal_nar_reads_recover_before_publication(self):
        for extension in (".narinfo", ".nar.zst"):
            for status in (404, 429, 503):
                with self.subTest(extension=extension, status=status):
                    message = READ_ERROR.replace(".narinfo", extension).replace(
                        "429", str(status)
                    )
                    result, calls = self.execute("rg353m", [(1, message), (0, "")])
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(
                        [call[0] for call in calls], ["build", "sleep", "build", "run"]
                    )
                    self.assertEqual(calls[1], ["sleep", "60"])

    def test_native_missing_payload_recovers_only_for_its_copied_path(self):
        message = NATIVE_COPY + NATIVE_MISSING + NATIVE_REQUIRED
        result, calls = self.execute("rg353m", [(1, message), (0, "")])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            [call[0] for call in calls], ["build", "sleep", "build", "run"]
        )
        metadata_path = READ_ERROR.split("/")[-1].split(".narinfo")[0]
        metadata_required = NATIVE_REQUIRED.replace(
            NATIVE_PATH, f"/nix/store/{metadata_path}-fixture"
        )
        result, calls = self.execute(
            "rg353m", [(1, READ_ERROR + metadata_required), (0, "")]
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            [call[0] for call in calls], ["build", "sleep", "build", "run"]
        )
        for unrelated in (
            NATIVE_REQUIRED,
            NATIVE_COPY + NATIVE_REQUIRED,
            NATIVE_COPY + READ_ERROR + NATIVE_REQUIRED,
            NATIVE_COPY + NATIVE_MISSING.replace(":1'", ":2'") + NATIVE_REQUIRED,
            message + "error: Cannot build '/nix/store/example.drv'.\n",
            message + "error: path does not have enough signatures from trusted keys\n",
        ):
            with self.subTest(message=unrelated):
                result, calls = self.execute("rg353m", [(1, unrelated), (0, "")])
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual([call[0] for call in calls], ["build"], result.stderr)

    def test_terminal_nar_read_recovery_is_bounded(self):
        result, calls = self.execute("rg353m", [(1, READ_ERROR)])
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(
            [call[0] for call in calls], ["build", "sleep", "build", "sleep", "build"]
        )
        self.assertEqual(
            [call for call in calls if call[0] == "sleep"],
            [["sleep", "60"], ["sleep", "120"]],
        )

    def test_compiler_assertion_trust_and_unknown_errors_are_not_reads(self):
        failures = (
            "error: Cannot build '/nix/store/example.drv'. Reason: builder failed\n",
            "error: assertion '(requiredGate)' failed\n",
            "error: path does not have enough signatures from trusted keys\n",
            "error: hash mismatch in fixed-output derivation\n",
            "error: attribute 'missing' missing\n",
            READ_ERROR.replace("429", "403"),
            READ_ERROR.replace(".narinfo", ".tar.gz"),
            READ_ERROR + "error: Cannot build '/nix/store/example.drv'.\n",
            READ_ERROR.replace("error:", "warning:") + "error: compilation failed\n",
            "unrecognized failure containing HTTP error 404\n",
        )
        for message in failures:
            with self.subTest(message=message):
                result, calls = self.execute("rg353m", [(1, message), (0, "")])
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual([call[0] for call in calls], ["build"], result.stderr)


if __name__ == "__main__":
    unittest.main()
