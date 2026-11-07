"""Exercise real readelf on built ARM files and corrupted disposable copies."""

import contextlib
import io
import os
import shutil
import tempfile
import unittest
from pathlib import Path

from check_elf import inspect


class ArtifactChecks(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "artifacts"
        shutil.copytree(os.environ["SPIKE_ARTIFACTS"], self.root)
        for path in self.root.rglob("*"):
            if path.is_file() and not path.is_symlink():
                path.chmod(0o600)

    def inspect(self) -> None:
        with contextlib.redirect_stdout(io.StringIO()):
            inspect(self.root)

    def test_real_built_arm_artifacts_pass(self) -> None:
        self.inspect()

    def test_missing_protocol_library_fails(self) -> None:
        (self.root / "lib/libmoonlight-common-c.so").unlink()
        with self.assertRaisesRegex(ValueError, "missing linked"):
            self.inspect()

    def test_wrong_machine_fails(self) -> None:
        binary = self.root / "bin/media-link-probe"
        data = bytearray(binary.read_bytes())
        data[18:20] = (3).to_bytes(2, "little")  # ELF e_machine: x86, not ARM.
        binary.write_bytes(data)
        with self.assertRaisesRegex(ValueError, "wrong ARM"):
            self.inspect()

    def test_too_new_glibc_fails(self) -> None:
        binary = self.root / "bin/media-link-probe"
        original = binary.read_bytes()
        self.assertIn(b"GLIBC_2.4\0", original)
        binary.write_bytes(original.replace(b"GLIBC_2.4\0", b"GLIBC_9.9\0"))
        with self.assertRaisesRegex(ValueError, "unsupported glibc"):
            self.inspect()


if __name__ == "__main__":
    unittest.main()
