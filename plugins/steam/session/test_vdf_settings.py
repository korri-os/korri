"""Exercise the shipped CLI against Steam-shaped files, without Steam or network."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

HELPER = Path(__file__).with_name("vdf-settings.py")
FIXTURES = Path(__file__).with_name("fixtures")


class SteamSettingsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.config = self.home / "config/config.vdf"
        self.config.parent.mkdir()
        self.config.write_bytes((FIXTURES / "config.vdf").read_bytes())

    def run_helper(self, appid="401710"):
        return subprocess.run(
            [os.environ.get("PYTHON", "python3"), str(HELPER), str(self.home), appid],
            text=True, capture_output=True, check=False,
        )

    def test_preserves_existing_content_and_adds_compat_mapping(self):
        result = self.run_helper()
        self.assertEqual(result.returncode, 0, result.stderr)
        config = self.config.read_text()
        self.assertIn('// Steam-owned comment must survive', config)
        self.assertIn('"Unrelated" "keep\\\\this\\\"value"', config)
        self.assertIn('"999" { "name" "other-proton" }', config)
        self.assertIn('"401710"', config)
        self.assertIn('"0"', config)
        self.assertEqual(config.count('"proton-cachyos-11.0-20260601-slr-arm64"'), 2)
        self.assertIn('"SteamController_XBoxSupport"\t\t"0"', config)
        self.assertIn('userdata seeds skipped', result.stderr)

    def test_preserves_quoted_braces_conditionals_and_crlf(self):
        content = b'"Braces" "{"\r\n"Platform" "keep" [$LINUX]\r\n'
        self.config.write_bytes(content)
        result = self.run_helper()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.config.read_bytes().startswith(content))
        first = self.config.read_bytes()
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertEqual(self.config.read_bytes(), first)

    def test_second_run_is_byte_identical(self):
        self.assertEqual(self.run_helper().returncode, 0)
        first = self.config.read_bytes()
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertEqual(first, self.config.read_bytes())

    def test_merges_localconfig_for_existing_login_only(self):
        local = self.home / "userdata/123/config/localconfig.vdf"
        local.parent.mkdir(parents=True)
        local.write_bytes((FIXTURES / "localconfig.vdf").read_bytes())
        self.assertEqual(self.run_helper().returncode, 0)
        content = local.read_text()
        self.assertIn('"PersonaName" "Keep Me"', content)
        self.assertIn('"401710_eula_2"\t\t"1"', content)
        self.assertIn('"Deck_ConfiguratorInterstitialsVersionSeen_Intro"\t\t"99"', content)
        self.assertNotIn('VersionSeen_GamepadRequired', content)
        first = local.read_bytes()
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertEqual(first, local.read_bytes())
        self.assertFalse((self.home / "userdata/0").exists())

    def test_rejects_malformed_vdf_without_overwriting_it(self):
        content = '"InstallConfigStore" { "incomplete"'
        self.config.write_text(content)
        result = self.run_helper()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.config.read_text(), content)

    def test_rejects_non_numeric_appid(self):
        before = self.config.read_bytes()
        self.assertEqual(self.run_helper("401710;false").returncode, 2)
        self.assertEqual(self.config.read_bytes(), before)

    def test_creates_config_and_preserves_private_mode(self):
        self.config.unlink()
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertEqual(self.config.stat().st_mode & 0o777, 0o600)
        self.config.chmod(0o640)
        self.assertEqual(self.run_helper("123").returncode, 0)
        self.assertEqual(self.config.stat().st_mode & 0o777, 0o640)


if __name__ == "__main__":
    unittest.main()
