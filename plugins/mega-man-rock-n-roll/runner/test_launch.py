"""Run the real launcher with a recording program named FEX on its PATH."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).parent

# Records what the launcher handed FEX, then exits with a chosen status.
RECORDER = f"""#!{sys.executable}
import json, os, sys
with open(os.environ["RECORD"], "w") as record:
    json.dump({{"argv": sys.argv[1:], "cwd": os.getcwd(), "env": dict(os.environ)}}, record)
sys.exit(int(os.environ.get("RECORDER_STATUS", "0")))
"""


class LaunchTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.game = root / "cache/mega-man-rock-n-roll/game/1.3"
        self.game.mkdir(parents=True)
        (self.game / "data.pck").write_bytes(b"pack\n")
        (self.game / "MegaMan_RocknRoll").write_bytes(b"\x7fELF\n")
        (self.game / "MegaMan_RocknRoll").chmod(0o755)
        self.home = root / "accounts/a/mega-man-rock-n-roll"
        self.home.mkdir(parents=True)
        self.config = root / "fex/share/fex-emu"
        self.config.mkdir(parents=True)
        self.rootfs = root / "cache/fex/rootfs/archlinux"
        self.rootfs.mkdir(parents=True)
        tools = root / "bin"
        tools.mkdir()
        (tools / "FEX").write_text(RECORDER)
        (tools / "FEX").chmod(0o755)
        self.record = root / "record.json"
        self.env = {
            "PATH": f"{tools}:{os.environ['PATH']}",
            "RECORD": str(self.record),
            "HOME": str(self.home),
            "KORRI_FEX_CONFIG": str(self.config),
            "KORRI_FEX_ROOTFS": str(self.rootfs),
            "DISPLAY": ":1",
        }

    def run_launch(self, *arguments, env=None):
        return subprocess.run(["bash", str(HERE / "fex-launch.sh"), *map(str, arguments)],
                              env=env or self.env, cwd="/", capture_output=True, text=True,
                              timeout=60)

    def test_runs_the_game_executable_under_fex_from_the_game_directory(self):
        result = self.run_launch(self.game)
        self.assertEqual(result.returncode, 0, result.stderr)
        record = json.loads(self.record.read_text())
        self.assertEqual(record["argv"], [str(self.game / "MegaMan_RocknRoll")])
        self.assertEqual(record["cwd"], str(self.game))
        env = record["env"]
        self.assertEqual(env["FEX_ROOTFS"], str(self.rootfs))
        self.assertEqual(env["FEX_APP_CONFIG_LOCATION"], f"{self.config}/")
        self.assertEqual(env["FEX_APP_DATA_LOCATION"], f"{self.home}/.fex-emu/")
        self.assertEqual(env["HOME"], str(self.home))
        self.assertEqual(env["DISPLAY"], ":1")
        self.assertNotIn("WAYLAND_DISPLAY", env)

    def test_returns_the_game_status(self):
        result = self.run_launch(self.game, env=self.env | {"RECORDER_STATUS": "7"})
        self.assertEqual(result.returncode, 7, result.stderr)

    def test_missing_or_wrong_inputs_exit_2_before_fex(self):
        missing = self.game.parent / "missing"
        cases = [
            ([], {}, "usage"),
            ([self.game, self.game], {}, "usage"),
            (["relative"], {}, "not an absolute directory"),
            ([missing], {}, "not an absolute directory"),
            ([self.game], {"HOME": ""}, "HOME must be an absolute path"),
            ([self.game], {"KORRI_FEX_CONFIG": "relative"}, "KORRI_FEX_CONFIG must be an absolute path"),
            ([self.game], {"KORRI_FEX_ROOTFS": str(missing)}, "KORRI_FEX_ROOTFS is not a directory"),
        ]
        for arguments, changes, message in cases:
            with self.subTest(arguments=arguments, changes=changes):
                result = self.run_launch(*arguments, env=self.env | changes)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(message, result.stderr)
                self.assertFalse(self.record.exists())
        for name in ("data.pck", "MegaMan_RocknRoll"):
            with self.subTest(missing=name):
                (self.game / name).rename(self.game / f"{name}.away")
                result = self.run_launch(self.game)
                (self.game / f"{name}.away").rename(self.game / name)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(f"game directory is missing {name}", result.stderr)
                self.assertFalse(self.record.exists())


if __name__ == "__main__":
    unittest.main()
