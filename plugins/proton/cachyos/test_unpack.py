"""Run the real cachyos-unpack source against a small synthetic release."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).parent
TOOL = "proton-cachyos-fixture-arm64"
UPSTREAM_VDF = '"compatibilitytools" { "compat_tools" { "upstream" { "install_path" "." } } }\n'


def release(scratch, files=None):
    """A tar.xz with one top-level dist directory, like the CachyOS release."""
    dist = scratch / "proton-cachyos-fixture"
    contents = files if files is not None else {
        "proton": "#!/usr/bin/env python3\n",
        "toolmanifest.vdf": '"manifest"\n{\n  "version" "2"\n  "require_tool_appid" "4185400"\n'
                            '  "commandline" "/proton %verb%"\n}\n',
        "compatibilitytool.vdf": UPSTREAM_VDF,
        "files/lib/wine.so": "native\n",
    }
    for name, content in contents.items():
        path = dist / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    archive = scratch / "proton.tar.xz"
    subprocess.run(["tar", "--xz", "-cf", str(archive), "-C", str(scratch), dist.name], check=True)
    return archive


class CachyosUnpackTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # korrid creates the work directory inside the contribution directory.
        self.contribution = self.root / "cache/proton/cachyos"
        self.work = self.contribution / ".tmp-fixture"
        self.work.mkdir(parents=True)
        self.archive = release(self.root / "release")
        vdf = self.root / "compatibilitytool.vdf"
        vdf.write_text((HERE / "compatibilitytool.vdf").read_text().replace("@toolName@", TOOL))
        self.script = self.root / "cachyos-unpack"
        self.script.write_text((HERE / "unpack.sh").read_text().replace("@compatibilitytool@", str(vdf)))

    def run_unpack(self, *arguments):
        return subprocess.run(["bash", str(self.script), *map(str, arguments)],
                              capture_output=True, text=True, timeout=60)

    def test_unpacks_the_tool_directory_into_the_work_directory(self):
        result = self.run_unpack(self.work, self.archive)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertTrue((self.work / "proton").is_file())
        self.assertEqual((self.work / "files/lib/wine.so").read_text(), "native\n")
        manifest = (self.work / "toolmanifest.vdf").read_text()
        self.assertNotIn("require_tool_appid", manifest)
        self.assertIn('"commandline" "/proton %verb%"', manifest)
        # Korri's registration replaces the release's own file.
        vdf = (self.work / "compatibilitytool.vdf").read_text()
        self.assertIn(f'"{TOOL}"', vdf)
        self.assertIn('"install_path" "."', vdf)
        self.assertFalse((self.work / "toolmanifest.vdf.tmp").exists())
        # korrid flips the tree read-only and renames it; the program leaves
        # it writable and writes nowhere else.
        self.assertTrue((self.work / "proton").stat().st_mode & 0o200)
        self.assertEqual(sorted(p.name for p in self.contribution.iterdir()), [".tmp-fixture"])

    def test_release_without_proton_fails(self):
        self.archive = release(self.root / "broken", files={"toolmanifest.vdf": '"manifest" {}\n'})
        result = self.run_unpack(self.work, self.archive)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("no proton or toolmanifest.vdf", result.stderr)

    def test_usage_errors_exit_1_without_stdout(self):
        for arguments in (
            [],
            [self.work],
            [self.work, self.archive, "extra"],
            ["relative", self.archive],
            [self.root / "missing", self.archive],
            [self.work, self.root / "missing.tar.xz"],
        ):
            with self.subTest(arguments=arguments):
                result = self.run_unpack(*arguments)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertEqual(os.listdir(self.work), [])


if __name__ == "__main__":
    unittest.main()
