"""Run the deploy command against recording subprocesses, never a device."""

import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(sys.argv.pop(1)).resolve()

# Configurable command processes implement the SSH observations and selector
# transitions. The deploy script itself owns ordering, selection and errors.
COMMAND = r'''
import json, os, pathlib, shlex, sys
root = pathlib.Path(os.environ["DEPLOY_TEST_ROOT"])
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with (root / "calls").open("a") as out:
    out.write(json.dumps([name, args]) + "\n")
if name != "ssh":
    sys.exit(0)
cmd = args[-1]
state = json.loads((root / "state").read_text())
if "korri-bundle-select switch " in cmd:
    words = shlex.split(cmd)
    requested = words[words.index("switch") + 1]
    # The real selector is a no-op when the requested bundle is active.
    if state["active"] != requested:
        state["active"] = requested
        state["running"] = requested if not state["bad_selection"] else "stale"
        (root / "state").write_text(json.dumps(state))
elif "switch-to-configuration" in cmd:
    sys.exit(state["activation_status"])
elif cmd == "systemctl reboot":
    state["boot"] = "new"
    (root / "state").write_text(json.dumps(state))
elif cmd == "cat /proc/sys/kernel/random/boot_id":
    print(state["boot"])
elif cmd == "readlink -f /run/booted-system":
    print(state["booted_system"])
elif cmd == "readlink -f /nix/var/nix/gcroots/korri-bundle/active":
    print(state["active"])
elif cmd.startswith('test "$(readlink -f /nix/var/nix/gcroots/korri-bundle/active)'):
    requested = shlex.split(cmd)[3]
    sys.exit(0 if state["active"] == state["running"] == requested else 1)
elif cmd.startswith("systemctl show tabs6-boot-good"):
    print("inactive success 123 ")
elif "journalctl" in cmd:
    print("blessed: candidate")
elif cmd.startswith("readlink -f "):
    print(cmd.removeprefix("readlink -f "))
'''


class DeployTest(unittest.TestCase):
    def deploy(self, *args, product=True, active="/old-bundle", running="/old-bundle",
               bad_selection=False, booted_system="/candidate-system", activation_status=0):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "bin").mkdir()
            (root / "seed").mkdir()
            (root / "key").write_text("fixture signing key")
            (root / "state").write_text(json.dumps({
                "boot": "old", "booted_system": booted_system,
                "active": active, "running": running,
                "bad_selection": bad_selection, "activation_status": activation_status,
            }))
            command = root / "command"
            command.write_text(f"#!{sys.executable}\n" + COMMAND)
            command.chmod(0o755)
            for tool in ["ssh", "nix", "rsync", "sleep"]:
                (root / "bin" / tool).symlink_to(command)
            variables = {
                "toplevel": "/candidate-system",
                "bundle": "/candidate-bundle" if product else "",
                "firmware": "/firmware", "placeholder": "false",
                "plugin_seed": str(root / "seed"), "sign_key": str(root / "key"),
                "default_target": "root@tablet", "default_port": "2222",
            }
            setup = "\n".join(f"{key}={shlex.quote(value)}" for key, value in variables.items())
            result = subprocess.run(
                ["bash", "-euo", "pipefail", "-c", setup + f"\nsource {shlex.quote(str(SCRIPT))}",
                 "deploy-over-ssh", *args],
                env={**os.environ, "PATH": str(root / "bin") + ":" + os.environ["PATH"],
                     "DEPLOY_TEST_ROOT": str(root)},
                capture_output=True, text=True, timeout=10,
            )
            calls = [json.loads(line) for line in (root / "calls").read_text().splitlines()]
            commands = [argv[-1] for name, argv in calls if name == "ssh"]
            return result, commands

    def test_product_without_flags_switches_system_and_bundle(self):
        result, commands = self.deploy()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any("switch-to-configuration switch" in cmd for cmd in commands))
        self.assertTrue(any("korri-bundle-select switch /candidate-bundle" in cmd for cmd in commands))

    def test_console_without_flags_still_stages_a_boot(self):
        result, commands = self.deploy(product=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any("switch-to-configuration boot" in cmd for cmd in commands))
        self.assertFalse(any("korri-bundle-select" in cmd for cmd in commands))

    def test_reboot_selects_after_booting_the_candidate_system(self):
        result, commands = self.deploy("--reboot")
        self.assertEqual(result.returncode, 0, result.stderr)
        install = next(i for i, cmd in enumerate(commands) if "switch-to-configuration boot" in cmd)
        reboot = commands.index("systemctl reboot")
        select = next(i for i, cmd in enumerate(commands) if "korri-bundle-select switch" in cmd)
        self.assertLess(install, reboot)
        self.assertLess(reboot, select)

    def test_reboot_rejects_a_stale_process_even_when_selector_matches(self):
        result, commands = self.deploy("--reboot", active="/candidate-bundle", running="stale")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("korrid does not run", result.stderr)
        self.assertTrue(any("korri-bundle-select switch /candidate-bundle" in cmd for cmd in commands))

    def test_switch_then_fallback_restores_the_original_bundle(self):
        result, commands = self.deploy("--switch", "--reboot", booted_system="/last-good-system")
        self.assertNotEqual(result.returncode, 0)
        selections = [cmd for cmd in commands if "korri-bundle-select switch" in cmd]
        self.assertIn("switch /candidate-bundle", selections[0])
        self.assertIn("switch /old-bundle", selections[-1])

    def test_activation_failure_is_not_reported_as_a_successful_deploy(self):
        result, commands = self.deploy(activation_status=4)
        self.assertEqual(result.returncode, 4)
        self.assertFalse(any("korri-bundle-select" in cmd for cmd in commands))

    def test_wrong_executable_after_selection_fails_the_deploy(self):
        result, _ = self.deploy("--switch", bad_selection=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("korrid does not run", result.stderr)


if __name__ == "__main__":
    unittest.main()
