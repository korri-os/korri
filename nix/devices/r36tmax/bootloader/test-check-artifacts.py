#!/usr/bin/env python3
"""Exercise rejection paths using altered copies of the real build outputs."""

from pathlib import Path
import runpy
import sys
import tempfile
import unittest

check = runpy.run_path(sys.argv[1])["check"]
build = Path(sys.argv[2]).resolve()
ddr = Path(sys.argv[3])
bl31 = Path(sys.argv[4])


class ArtifactRejectionTests(unittest.TestCase):
    def rejects(self, filename: str, data: bytes, message: str) -> None:
        with tempfile.TemporaryDirectory() as directory:
            changed = Path(directory)
            for original in build.iterdir():
                (changed / original.name).symlink_to(original)
            target = changed / filename
            target.unlink()
            target.write_bytes(data)
            with self.assertRaisesRegex(AssertionError, message):
                check(changed, ddr, bl31)

    def test_open_tpl_enabled(self):
        self.rejects(
            ".config",
            (build / ".config").read_bytes() + b"\nCONFIG_TPL=y\n",
            "Open TPL must not be built",
        )

    def test_changed_ddr_in_loader(self):
        data = bytearray((build / "idbloader.img").read_bytes())
        data[2048 + 100] ^= 1
        self.rejects("idbloader.img", data, "DDR bytes absent")

    def test_truncated_fit(self):
        self.rejects(
            "u-boot.itb", (build / "u-boot.itb").read_bytes()[:4096], "Truncated FIT"
        )

    def test_changed_fit_payload(self):
        data = bytearray((build / "u-boot.itb").read_bytes())
        # The first external payload starts immediately after the aligned FDT.
        import libfdt

        offset = libfdt.Fdt(data).totalsize()
        data[offset + 100] ^= 1
        self.rejects("u-boot.itb", data, "FIT payload differs")

    def test_changed_combined_loader(self):
        data = bytearray((build / "u-boot-rockchip.bin").read_bytes())
        data[2048 + 100] ^= 1
        self.rejects(
            "u-boot-rockchip.bin", data, "Combined image has a different loader"
        )


if __name__ == "__main__":
    unittest.main(argv=[sys.argv[0]])
