"""Run the real session state machine with filesystem IPC and real processes."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

HERE = Path(__file__).parent
TOOL = "proton-cachyos-11.0-20260601-slr-arm64"


def program(path, source):
    path.write_text(f"#!{sys.executable}\n" + source)
    path.chmod(0o755)


class SteamSessionTests(unittest.TestCase):
    def run_session(self, behavior, prepare="ready", seeded=True, setup=None, installed=True):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.root = root
        home = root / "steam"
        (home / "userdata/123/config").mkdir(parents=True)
        if installed:
            (home / "steamapps").mkdir()
            (home / "steamapps/appmanifest_401710.acf").write_text(
                '"AppState"\n{\n\t"appid"\t\t"401710"\n\t"StateFlags"\t\t"4"\n}\n')
        if setup:
            setup(home)
        if seeded:
            (home / "steamrtarm64").mkdir()
            client = home / "steamrtarm64/steam"
            client.touch()
            client.chmod(0o755)
        (root / "bin").mkdir()
        (root / "cache").mkdir()
        (root / "fex-config").mkdir()
        program(root / "bin/steam-arm64-fhs", (HERE / "fixtures/session-client.py").read_text())
        program(root / "bin/steam-arm64-seed", (HERE / "fixtures/seed.py").read_text())
        for name in ("fex-prepare", "cachyos-prepare"):
            program(root / "bin" / name, (HERE / "fixtures/prepare.py").read_text())
        program(root / "bin/steam-vdf-settings",
                (HERE / "vdf-settings.py").read_text().replace("@protonToolName@", TOOL))
        script = (HERE / "steam-session.sh").read_text()
        for placeholder in ("fhs", "helpers", "settings"):
            script = script.replace(f"@{placeholder}@", str(root))
        script = script.replace("@protonToolName@", TOOL)
        path = root / "session.sh"
        path.write_text(script)
        started = time.monotonic()
        result = subprocess.run(
            ["bash", str(path), "401710"], timeout=60, capture_output=True, text=True,
            env={
                **os.environ, "STEAM_HOME": str(home), "HOME": str(root), "CLIENT_BEHAVIOR": behavior,
                "STEAM_BETA": "steamdeck_stable",
                "PREPARE_BEHAVIOR": prepare,
                "KORRI_CACHE_ROOT": str(root / "cache"),
                "KORRI_FEX_PREPARE": str(root / "bin/fex-prepare"),
                "KORRI_FEX_CONFIG": str(root / "fex-config"),
                "KORRI_PROTON_CACHYOS_PREPARE": str(root / "bin/cachyos-prepare"),
            },
        )
        return result, home, time.monotonic() - started

    def test_prepares_run_before_seeding_and_configure_fex_and_proton(self):
        result, home, _ = self.run_session("game", seeded=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        rootfs = self.root / "cache/fex/rootfs/fixture"
        proton = self.root / "cache/proton/cachyos/fixture"
        # Prepare diagnostics reach the session's stderr; only the last stdout line counts.
        self.assertIn("fex-prepare: fixture ready", result.stderr)
        seed = json.loads((home / "seed.json").read_text())
        self.assertEqual(seed, {"rootfs": True, "proton": True})
        client = json.loads((home / "client-env.json").read_text())
        self.assertEqual(client["FEX_ROOTFS"], str(rootfs))
        self.assertEqual(client["FEX_APP_CONFIG_LOCATION"], str(self.root / "fex-config") + "/")
        self.assertEqual(os.readlink(home / "compatibilitytools.d" / TOOL), str(proton))
        self.assertIn(f'"{TOOL}"', (home / "config/config.vdf").read_text())

    def test_prepare_failure_exits_126_before_seeding_or_starting_steam(self):
        for failing, label in (("fex-prepare", "FEX rootfs"), ("cachyos-prepare", "Proton-CachyOS")):
            with self.subTest(failing=failing):
                result, home, _ = self.run_session("game", prepare=f"fail:{failing}", seeded=False)
                self.assertEqual(result.returncode, 126, result.stderr)
                self.assertIn(f"runtime download failed: {label} (prepare exited with status 1)",
                              result.stderr)
                self.assertFalse((home / "seed.json").exists())
                self.assertFalse((home / "config/config.vdf").exists())
                self.assertFalse((home / "logs/session_log.txt").exists())

    def test_prepare_without_a_ready_directory_exits_126(self):
        result, home, _ = self.run_session("game", prepare="no-directory")
        self.assertEqual(result.returncode, 126, result.stderr)
        self.assertIn("runtime download failed: FEX rootfs (prepare exited with status 0)",
                      result.stderr)
        self.assertFalse((home / "logs/session_log.txt").exists())

    def test_game_exit_waits_for_process_and_stops_steam(self):
        result, home, duration = self.run_session("game")
        self.assertEqual(result.returncode, 0, result.stderr)
        # An already seeded home still gets the link to the prepared tool.
        self.assertFalse((home / "seed.json").exists())
        self.assertEqual(os.readlink(home / "compatibilitytools.d" / TOOL),
                         str(self.root / "cache/proton/cachyos/fixture"))
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

    def test_forward_exit_42_restarts_and_reforwards_appid(self):
        result, home, _ = self.run_session("forward-restart")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((home / "forwards").read_text(), "2")
        self.assertIn("re-forwarding AppID (1/3)", result.stderr)
        self.assertTrue((home / "shutdown").exists())

    def test_forward_error_recovers_when_the_client_also_restarted(self):
        result, home, _ = self.run_session("forward-client-restart")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((home / "forwards").read_text(), "2")
        self.assertTrue((home / "shutdown").exists())

    def test_forward_restarts_stop_after_three_reforwards(self):
        result, home, _ = self.run_session("forward-restart-always")
        self.assertEqual(result.returncode, 126, result.stderr)
        self.assertEqual((home / "forwards").read_text(), "4")
        self.assertIn("restart re-forward limit (3) exceeded", result.stderr)
        self.assertTrue((home / "shutdown").exists())

    def test_update_blocks_forward_even_when_a_ready_line_is_present(self):
        result, home, _ = self.run_session("update")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("startup update observed", result.stderr)
        self.assertTrue((home / "update-complete").exists())

    def test_dropped_forward_is_sent_again(self):
        result, home, duration = self.run_session("drop-first")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("did not act on AppID 401710; forwarding again (1/3)", result.stderr)
        self.assertEqual((home / "forwards").read_text(), "2")
        self.assertGreaterEqual(duration, 20)

    def test_uninstalled_app_waits_for_the_install(self):
        result, _, _ = self.run_session("game", installed=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("AppID 401710 is not installed; waiting up to 3600 s", result.stderr)

    def test_channel_and_installed_manifest_survive_the_client_deleting_them(self):
        manifest = '"linuxarm64"\n{\n\t"version"\t\t"1791918634"\n}\n'

        def installed_client(home):
            (home / "package").mkdir()
            (home / "package/beta").write_text("\n")
            (home / "package/steam_client_steamdeck_stable_linuxarm64.installed").write_text("x\n")
            (home / "package/steam_client_steamdeck_stable_linuxarm64.manifest").write_text(manifest)

        result, home, _ = self.run_session("game", setup=installed_client)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((home / "beta-seen").read_text(), "steamdeck_stable\n")
        self.assertEqual((home / "manifest-seen").read_text(), manifest)
        # The client deleted it during the session; the saved copy remains.
        package = home / "package/steam_client_steamdeck_stable_linuxarm64.manifest"
        self.assertFalse(package.exists())
        saved = home / ".korri/steam_client_steamdeck_stable_linuxarm64.manifest"
        self.assertEqual(saved.read_text(), manifest)

        def deleted_manifest(next_home):
            installed_client(next_home)
            (next_home / "package/steam_client_steamdeck_stable_linuxarm64.manifest").unlink()
            (next_home / ".korri").mkdir()
            (next_home / ".korri/steam_client_steamdeck_stable_linuxarm64.manifest").write_text(manifest)

        result, home, _ = self.run_session("game", setup=deleted_manifest)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("restoring the installed Steam client manifest", result.stderr)
        self.assertEqual((home / "manifest-seen").read_text(), manifest)

    def test_setgid_is_cleared_from_the_steam_home(self):
        def setgid_tree(home):
            (home / "steamapps/common").mkdir(parents=True, exist_ok=True)
            for directory in (home, home / "steamapps", home / "steamapps/common"):
                directory.chmod(0o2770)

        result, home, _ = self.run_session("game", setup=setgid_tree)
        self.assertEqual(result.returncode, 0, result.stderr)
        for directory in (home, home / "steamapps", home / "steamapps/common"):
            self.assertFalse(directory.stat().st_mode & 0o2000, directory)

    def test_forward_failure_is_classified_and_stops_steam(self):
        result, home, _ = self.run_session("forward-error")
        self.assertEqual(result.returncode, 126, result.stderr)
        self.assertIn("AppID forward failed", result.stderr)
        self.assertTrue((home / "shutdown").exists())


if __name__ == "__main__":
    unittest.main()
