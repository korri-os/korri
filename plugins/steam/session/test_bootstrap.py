"""Exercise the native seed/bootstrap scripts against offline Steam homes."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).parent
NATIVE = HERE.parent / "packages/steam-korri"


class SteamBootstrapTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.home = self.root / "steam"
        self.dot = self.root / "dot"
        self.home.mkdir()
        self.dot.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        resources = self.root / "share/steam-rocknix-bootstrap/resources"
        shutil.copytree(NATIVE / "resources", resources)
        for name in ("steam-arm64-bootstrap", "steam-arm64-seed"):
            script = (NATIVE / "scripts" / name).read_text()
            script = script.replace("@korriProtonCachyosArm64@", str(self.root / "proton"))
            script = script.replace("@vdfSettings@", str(self.root))
            (self.bin / name).write_text(script)
            (self.bin / name).chmod(0o755)
        settings = self.bin / "steam-vdf-settings"
        settings.write_text(f"#!{sys.executable}\n" + (HERE / "vdf-settings.py").read_text())
        settings.chmod(0o755)
        # Offline fetches fail explicitly, so tests cannot contact Valve.
        curl = self.bin / "curl"
        curl.write_text(f"#!{sys.executable}\nraise SystemExit(1)\n")
        curl.chmod(0o755)
        self.env = {
            **os.environ,
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "STEAM_HOME": str(self.home),
            "STEAM_GAMES_ROOT": str(self.home),
            "STEAM_DOT": str(self.dot),
        }
        runtime = self.home / "steam-runtime-steamrt-arm64/steamrt3c_platform_test/files/lib/aarch64-linux-gnu"
        runtime.mkdir(parents=True)
        (runtime / "libibus-1.0.so.5.0").touch()
        client = self.home / "steamrtarm64/steam"
        client.parent.mkdir()
        client.touch()
        client.chmod(0o755)
        self.existing_tool = self.home / "compatibilitytools.d/Proton11ARM"
        self.existing_tool.parent.mkdir()
        self.existing_tool.write_text("leave this existing tool untouched\n")

    def run_helper(self, name, mode="--apply"):
        return subprocess.run(
            ["bash", str(self.bin / name), mode], env=self.env,
            text=True, capture_output=True, check=False, timeout=10,
        )

    def test_bootstrap_does_not_remove_an_existing_compatibility_tool(self):
        result = self.run_helper("steam-arm64-bootstrap")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.existing_tool.read_text(), "leave this existing tool untouched\n")

    def test_seed_does_not_remove_an_existing_compatibility_tool(self):
        result = self.run_helper("steam-arm64-seed")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.existing_tool.read_text(), "leave this existing tool untouched\n")


if __name__ == "__main__":
    unittest.main()
