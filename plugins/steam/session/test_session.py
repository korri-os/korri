"""Run the real session state machine with filesystem IPC and real processes."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

HERE = Path(__file__).parent
RESOURCES = HERE.parent / "packages/steam-korri/resources"


class SteamSessionTests(unittest.TestCase):
    def run_session(self, behavior):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        home = root / "steam"
        (home / "userdata/123/config").mkdir(parents=True)
        (home / "steamrtarm64").mkdir()
        client = home / "steamrtarm64/steam"
        client.touch()
        client.chmod(0o755)
        (root / "bin").mkdir()
        runner = root / "bin/steam-arm64-fhs"
        runner.write_text(f"#!{sys.executable}\n" + (HERE / "fixtures/session-client.py").read_text())
        runner.chmod(0o755)
        settings = root / "bin/steam-vdf-settings"
        settings.write_text(f"#!{sys.executable}\n" + (HERE / "vdf-settings.py").read_text())
        settings.chmod(0o755)
        shutil.copytree(RESOURCES, root / "share")
        script = (HERE / "steam-session.sh").read_text()
        for placeholder in ("rootfs", "fhs", "helpers", "proton", "resources", "settings"):
            script = script.replace(f"@{placeholder}@", str(root))
        path = root / "session.sh"
        path.write_text(script)
        started = time.monotonic()
        result = subprocess.run(
            ["bash", str(path), "401710"], timeout=30, capture_output=True, text=True,
            env={**os.environ, "STEAM_HOME": str(home), "HOME": str(root), "CLIENT_BEHAVIOR": behavior},
        )
        return result, home, time.monotonic() - started

    def test_game_exit_waits_for_process_and_stops_steam(self):
        result, home, duration = self.run_session("game")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertGreaterEqual(duration, 2)
        self.assertIn("AppID 401710 exited", result.stderr)
        self.assertTrue((home / "shutdown").exists())
        self.assertEqual((home / "forwards").read_text(), "1")

    def test_restart_42_before_game_reforwards_appid(self):
        result, home, _ = self.run_session("restart")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("update restart (42)", result.stderr)
        self.assertEqual((home / "forwards").read_text(), "2")
        self.assertTrue((home / "shutdown").exists())

    def test_update_blocks_forward_even_when_a_ready_line_is_present(self):
        result, home, _ = self.run_session("update")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("startup update observed", result.stderr)
        self.assertTrue((home / "update-complete").exists())

    def test_forward_failure_is_classified_and_stops_steam(self):
        result, home, _ = self.run_session("forward-error")
        self.assertEqual(result.returncode, 126, result.stderr)
        self.assertIn("AppID forward failed", result.stderr)
        self.assertTrue((home / "shutdown").exists())


if __name__ == "__main__":
    unittest.main()
