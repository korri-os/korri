"""Run the real unpack program against small generated ZIPs, never the game."""
import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile

HERE = Path(__file__).parent
SHIPPED_CONTENTS = HERE / "contents"
PLUGIN_SOURCE = HERE.parent / "plugin.ts"

FILES = {
    "data.pck": b"GDPC fixture pack\n",
    "MegaMan_RocknRoll": b"\x7fELF fixture executable\n",
    "readme.txt": b"fixture readme\n",
}


def sha256(content):
    return hashlib.sha256(content).hexdigest()


def write_zip(path, entries):
    """Entries are (name, content) or (ZipInfo, content), stored like the real
    ZIP: MS-DOS attributes and no Unix modes unless a ZipInfo sets them."""
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive, warnings.catch_warnings():
        # Some fixtures repeat a name on purpose.
        warnings.simplefilter("ignore")
        for name, content in entries:
            info = name if isinstance(name, zipfile.ZipInfo) else zipfile.ZipInfo(name)
            if not isinstance(name, zipfile.ZipInfo):
                info.create_system = 0
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, content)
    return path


class UnpackTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # korrid creates the work directory inside the contribution directory.
        self.contribution = self.root / "cache/mega-man-rock-n-roll/game"
        self.work = self.contribution / ".tmp-fixture"
        self.work.mkdir(parents=True)
        self.contents = self.root / "contents"
        self.write_contents(FILES["data.pck"], FILES["MegaMan_RocknRoll"])
        self.zip = write_zip(self.root / "game.zip", list(FILES.items()))

    def write_contents(self, data, executable):
        self.contents.write_text(
            "# fixture\n"
            f"0644 sha256:{sha256(data)} data.pck\n"
            f"0755 sha256:{sha256(executable)} MegaMan_RocknRoll\n"
        )

    def run_unpack(self, *arguments, contents=None):
        return subprocess.run(
            [sys.executable, str(HERE / "unpack.py"), str(contents or self.contents),
             *map(str, arguments)],
            capture_output=True, text=True, timeout=60)

    def assert_failed(self, result, message):
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertIn(message, result.stderr)
        self.assertEqual(sorted(p.name for p in self.contribution.iterdir()), [".tmp-fixture"])
        self.assertEqual(sorted(p.name for p in self.root.iterdir()),
                         ["cache", "contents", "game.zip"])

    def test_extracts_the_zip_into_the_work_directory_with_modes(self):
        result = self.run_unpack(self.work, self.zip)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertEqual(sorted(p.name for p in self.work.iterdir()), sorted(FILES))
        for name, content in FILES.items():
            self.assertEqual((self.work / name).read_bytes(), content)
        modes = {name: stat.S_IMODE((self.work / name).stat().st_mode) for name in FILES}
        self.assertEqual(modes, {"data.pck": 0o644, "MegaMan_RocknRoll": 0o755, "readme.txt": 0o644})
        self.assertTrue(os.access(self.work / "MegaMan_RocknRoll", os.X_OK))
        # korrid flips the tree read-only and renames it; the program writes
        # nowhere else.
        self.assertEqual(sorted(p.name for p in self.contribution.iterdir()), [".tmp-fixture"])

    def test_extracts_nested_directories(self):
        write_zip(self.zip, [*FILES.items(), ("extras/", b""), ("extras/notes/a.txt", b"a\n")])
        result = self.run_unpack(self.work, self.zip)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.work / "extras/notes/a.txt").read_bytes(), b"a\n")

    def test_hash_mismatch_fails(self):
        self.write_contents(b"another pack\n", FILES["MegaMan_RocknRoll"])
        result = self.run_unpack(self.work, self.zip)
        self.assert_failed(result, f"data.pck has sha256:{sha256(FILES['data.pck'])}, expected")

    def test_missing_listed_file_fails(self):
        write_zip(self.zip, [("data.pck", FILES["data.pck"]), ("readme.txt", b"x\n")])
        self.assert_failed(self.run_unpack(self.work, self.zip), "archive is missing MegaMan_RocknRoll")

    def test_entries_that_leave_the_work_directory_fail(self):
        for name in ("../escape", "/escape", "a/../../escape", "..\\escape", "./data.pck", "a//b"):
            with self.subTest(name=name):
                write_zip(self.zip, [*FILES.items(), (name, b"escaped\n")])
                self.assert_failed(self.run_unpack(self.work, self.zip), "leaves the work directory")
                # The archive is refused before any entry is written.
                self.assertEqual(list(self.work.iterdir()), [])

    def test_symlink_and_special_entries_fail(self):
        for mode in (stat.S_IFLNK | 0o777, stat.S_IFIFO | 0o644, stat.S_IFCHR | 0o644):
            with self.subTest(mode=oct(mode)):
                link = zipfile.ZipInfo("data-link")
                link.create_system = 3
                link.external_attr = mode << 16
                write_zip(self.zip, [*FILES.items(), (link, b"/etc/passwd")])
                self.assert_failed(self.run_unpack(self.work, self.zip), "is not a regular file")
                self.assertEqual(list(self.work.iterdir()), [])

    def test_duplicate_entry_fails(self):
        write_zip(self.zip, [*FILES.items(), ("readme.txt", b"again\n")])
        self.assert_failed(self.run_unpack(self.work, self.zip), "appears twice")

    def test_usage_errors_exit_1_without_stdout(self):
        not_zip = self.root / "not.zip"
        not_zip.write_bytes(b"not a zip\n")
        for arguments, message in (
            ([], "usage"),
            ([self.work], "usage"),
            ([self.work, self.zip, self.zip], "usage"),
            (["relative", self.zip], "not an absolute directory"),
            ([self.root / "missing", self.zip], "not an absolute directory"),
            ([self.work, self.root / "missing.zip"], "not a file"),
            ([self.work, not_zip], "cannot read"),
        ):
            with self.subTest(arguments=arguments):
                result = self.run_unpack(*arguments)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertIn(message, result.stderr)
                self.assertEqual(os.listdir(self.work), [])
        not_zip.unlink()

    def test_shipped_contents_rejects_a_zip_that_is_not_the_game(self):
        result = self.run_unpack(self.work, self.zip, contents=SHIPPED_CONTENTS)
        self.assert_failed(result, "data.pck has sha256:")

    def test_shipped_contents_names_the_release_hash_in_plugin_ts(self):
        listed = dict(
            (line.split()[2], line.split()[1])
            for line in SHIPPED_CONTENTS.read_text().splitlines()
            if line and not line.startswith("#")
        )
        self.assertEqual(sorted(listed), ["MegaMan_RocknRoll", "data.pck"])
        source = PLUGIN_SOURCE.read_text()
        release = re.search(r'const dataPck =\s*"(sha256:[0-9a-f]{64})"', source)
        self.assertIsNotNone(release, "plugin.ts names the data.pck hash once as dataPck")
        self.assertEqual(listed["data.pck"], release.group(1))


if __name__ == "__main__":
    unittest.main()
