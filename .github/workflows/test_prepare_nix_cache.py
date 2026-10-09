#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "prepare_nix_cache", Path(__file__).with_name("prepare-nix-cache.py")
)
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class BuildRemnantsTest(unittest.TestCase):
    def test_refuses_cleanup_outside_a_disposable_github_hosted_runner(self):
        result = subprocess.run(
            [sys.executable, str(Path(__file__).with_name("prepare-nix-cache.py"))],
            env={**os.environ, "GITHUB_ACTIONS": "false"},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("only on a disposable GitHub-hosted runner", result.stderr)

    def test_keeps_registered_paths_and_selects_only_interrupted_build_remnants(self):
        with tempfile.TemporaryDirectory() as directory:
            store = Path(directory)
            output = store / "registered-output"
            output.mkdir()
            (output / "flake.lock").write_text("keep nested files")
            registered_lock = store / "registered-output.lock"
            registered_lock.write_text("a real output")
            lock = store / "unfinished-output.lock"
            lock.touch(mode=0o600)
            sandbox = store / "unfinished-output.drv.chroot"
            sandbox.mkdir(mode=0o700)
            unrelated = store / "unregistered-other-file"
            unrelated.touch()
            registered = {str(output), str(registered_lock)}

            self.assertEqual(
                set(prepare.build_remnants(store, registered)), {lock, sandbox}
            )
            self.assertTrue((output / "flake.lock").exists())
            self.assertTrue(registered_lock.exists())
            self.assertTrue(unrelated.exists())


if __name__ == "__main__":
    unittest.main()
