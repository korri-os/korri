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
TOOL = "proton-cachyos-11.0-20260601-slr-arm64"


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
        bash = shutil.which("bash")
        self.assertIsNotNone(bash)
        for name in ("steam-arm64-bootstrap", "steam-arm64-seed"):
            script = (NATIVE / "scripts" / name).read_text()
            # Production installation uses patchShebangs as well. A Nix build
            # sandbox deliberately has no /usr/bin/env for the child bootstrap.
            script = script.replace("#!/usr/bin/env bash", f"#!{bash}", 1)
            script = script.replace("@vdfSettings@", str(self.root))
            (self.bin / name).write_text(script)
            (self.bin / name).chmod(0o755)
        settings = self.bin / "steam-vdf-settings"
        source = (HERE / "vdf-settings.py").read_text().replace("@protonToolName@", TOOL)
        settings.write_text(f"#!{sys.executable}\n" + source)
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

    def registry_merge_is_preserved_and_idempotent(self, helper):
        registry = self.dot / "registry.vdf"
        registry.write_bytes((HERE / "fixtures/registry.vdf").read_bytes())
        registry.chmod(0o640)
        result = self.run_helper(helper)
        self.assertEqual(result.returncode, 0, result.stderr)
        content = registry.read_text()
        for original in (
            '// Existing registry comments and user choices must survive seeding.',
            '"Unrelated" "keep this value"',
            '"SteamPID" "12345"',
            '"ClientLauncherType" "2"',
            '"language" "german"',
            '"SourceModInstallPath" "/games/custom-mods"',
            '"AlreadyRetriedOfflineMode" "1"',
        ):
            self.assertIn(original, content)
        for key in ("SteamDeckOOBEComplete", "CompletedOOBEStage1", "CompletedOOBE"):
            self.assertIn(f'"{key}" "1"', content)
        self.assertEqual(registry.stat().st_mode & 0o777, 0o640)
        first = registry.read_bytes()
        result = self.run_helper(helper)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(registry.read_bytes(), first)

    def test_bootstrap_merges_only_oobe_flags_and_is_idempotent(self):
        self.registry_merge_is_preserved_and_idempotent("steam-arm64-bootstrap")

    def test_seed_preserves_existing_registry_and_is_idempotent(self):
        self.registry_merge_is_preserved_and_idempotent("steam-arm64-seed")

    def test_bootstrap_copies_registry_fixture_only_when_absent(self):
        result = self.run_helper("steam-arm64-bootstrap")
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = (NATIVE / "resources/registry.vdf").read_bytes()
        registry = self.dot / "registry.vdf"
        self.assertEqual(registry.read_bytes(), expected)
        result = self.run_helper("steam-arm64-bootstrap")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(registry.read_bytes(), expected)

    def test_bootstrap_rejects_malformed_registry_without_overwriting_it(self):
        registry = self.dot / "registry.vdf"
        content = b'"Registry" { "incomplete"'
        registry.write_bytes(content)
        result = self.run_helper("steam-arm64-bootstrap")
        self.assertEqual(result.returncode, 126, result.stderr)
        self.assertEqual(registry.read_bytes(), content)

    def test_bootstrap_dry_run_does_not_change_registry(self):
        registry = self.dot / "registry.vdf"
        content = (HERE / "fixtures/registry.vdf").read_bytes()
        registry.write_bytes(content)
        result = self.run_helper("steam-arm64-bootstrap", "--dry-run")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(registry.read_bytes(), content)

    def test_bootstrap_does_not_remove_an_existing_compatibility_tool(self):
        result = self.run_helper("steam-arm64-bootstrap")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.existing_tool.read_text(), "leave this existing tool untouched\n")

    def test_seed_does_not_remove_an_existing_compatibility_tool(self):
        result = self.run_helper("steam-arm64-seed")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.existing_tool.read_text(), "leave this existing tool untouched\n")
        # The session links the prepared Proton tool on every launch; the seed does not.
        self.assertEqual(list((self.home / "compatibilitytools.d").iterdir()), [self.existing_tool])


if __name__ == "__main__":
    unittest.main()
