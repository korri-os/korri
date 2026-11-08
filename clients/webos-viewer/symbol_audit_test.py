"""Audit actual ARM ELFs against the parent's original TV library copies."""

import contextlib
import io
import os
import shutil
import struct
import tempfile
import unittest
from pathlib import Path

from symbol_audit import audit, library


class SymbolAudit(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "bundle"
        shutil.copytree(os.environ["VIEWER_ARTIFACTS"], self.root)
        for path in self.root.rglob("*"):
            if path.is_file():
                path.chmod(0o600)
        self.evidence = Path(os.environ["TV_LIBRARY_EVIDENCE"])

    def inspect(self, evidence: Path | None = None) -> None:
        with contextlib.redirect_stdout(io.StringIO()):
            audit(self.root, evidence or self.evidence)

    def test_all_packaged_imports_resolve(self) -> None:
        self.inspect()

    def test_toolchain_hooks_remain_weak_undefined(self) -> None:
        hooks = {
            "_ITM_deregisterTMCloneTable",
            "_ITM_registerTMCloneTable",
            "__gmon_start__",
        }
        for relative in (
            "bin/webos-viewer-test",
            "lib/libss4s.so",
            "lib/ss4s-ndl-webos4.so",
        ):
            item = library(self.root / relative)
            self.assertTrue(hooks <= item.optional_imports)
            self.assertFalse(hooks & item.imports)
            self.assertFalse(item.zero_absolute_exports)

    def test_global_and_weak_zero_absolute_exports_fail(self) -> None:
        path = self.root / "lib/libss4s.so"
        original = path.read_bytes()
        section_table = struct.unpack_from("<I", original, 32)[0]
        section_size, section_count = struct.unpack_from("<HH", original, 46)
        for binding in (1, 2):  # ELF STB_GLOBAL / STB_WEAK
            data = bytearray(original)
            changed = False
            for index in range(section_count):
                section = section_table + index * section_size
                if struct.unpack_from("<I", data, section + 4)[0] != 11:  # SHT_DYNSYM
                    continue
                offset, size = struct.unpack_from("<II", data, section + 16)
                entry_size = struct.unpack_from("<I", data, section + 36)[0]
                for entry in range(offset, offset + size, entry_size):
                    info, visibility, section_index = struct.unpack_from(
                        "<BBH", data, entry + 12
                    )
                    if (
                        info >> 4 in (1, 2)
                        and visibility & 3 in (0, 3)
                        and section_index != 0
                    ):
                        struct.pack_into("<I", data, entry + 4, 0)
                        data[entry + 12] = binding << 4 | info & 15
                        struct.pack_into("<H", data, entry + 14, 0xFFF1)  # SHN_ABS
                        changed = True
                        break
                if changed:
                    break
            self.assertTrue(changed)
            path.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "zero-valued ABS export"):
                self.inspect()

    def test_missing_tv_soname_fails(self) -> None:
        evidence = Path(self.temporary.name) / "tvlibs"
        evidence.mkdir()
        for path in self.evidence.iterdir():
            if path.is_file() and not path.name.startswith("libNDL_directmedia"):
                shutil.copyfile(path, evidence / path.name)
        with self.assertRaisesRegex(ValueError, "missing NEEDED libNDL_directmedia"):
            self.inspect(evidence)

    def test_unresolved_import_fails(self) -> None:
        path = self.root / "bin/webos-viewer-test"
        content = path.read_bytes()
        self.assertIn(b"clock_gettime\0", content)
        path.write_bytes(content.replace(b"clock_gettime\0", b"clock_gettinx\0"))
        with self.assertRaisesRegex(ValueError, "undefined clock_gettinx"):
            self.inspect()

    def test_wrong_symbol_version_fails(self) -> None:
        path = self.root / "bin/webos-viewer-test"
        content = path.read_bytes()
        self.assertIn(b"GLIBC_2.4\0", content)
        path.write_bytes(content.replace(b"GLIBC_2.4\0", b"GLIBC_9.9\0"))
        with self.assertRaisesRegex(ValueError, r"undefined .*@GLIBC_9\.9"):
            self.inspect()


if __name__ == "__main__":
    unittest.main()
