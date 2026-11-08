"""Reject damaged or incompatible real output ELFs."""

import contextlib
import io
import os
import shutil
import struct
import tempfile
import unittest
from pathlib import Path

from abi_check import check


class AbiTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "artifacts"
        shutil.copytree(os.environ["VIEWER_ARTIFACTS"], self.root)
        for directory in (self.root, self.root / "bin", self.root / "lib"):
            directory.chmod(0o700)
        self.binary = self.root / "bin/webos-viewer-test"
        self.binary.chmod(0o600)

    def test_valid(self) -> None:
        with contextlib.redirect_stdout(io.StringIO()):
            check(self.root)

    def test_reject_machine_and_hard_float(self) -> None:
        original = self.binary.read_bytes()
        for offset, value, format in [(18, 62, "<H"), (36, 0x5000400, "<I")]:
            content = bytearray(original)
            struct.pack_into(format, content, offset, value)
            self.binary.write_bytes(content)
            with self.assertRaisesRegex(ValueError, "soft-float"):
                check(self.root)

    def test_reject_new_libc(self) -> None:
        content = self.binary.read_bytes()
        self.assertIn(b"GLIBC_2.4\0", content)
        self.binary.write_bytes(content.replace(b"GLIBC_2.4\0", b"GLIBC_9.9\0"))
        with self.assertRaisesRegex(ValueError, "GLIBC_9.9"):
            check(self.root)

    def test_reject_missing_module(self) -> None:
        (self.root / "lib/ss4s-pulse.so").unlink()
        with self.assertRaisesRegex(ValueError, "missing artifact"):
            check(self.root)
