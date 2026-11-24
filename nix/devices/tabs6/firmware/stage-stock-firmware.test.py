"""Host integration test: real tar archives, Nix store and indirect GC roots.

Run outside the Nix build sandbox (needs access to the host's Nix daemon).
Only tiny, non-proprietary fixtures enter the store; no GC is run.
"""

import io
import os
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("stage-stock-firmware.sh")
MEMBERS = ("NON-HLOS.bin.lz4", "modem.bin.lz4", "vendor.img.ext4.lz4")


class FirmwareStaging(unittest.TestCase):
    def test_staging_roots_each_member_and_can_be_repeated(self):
        with tempfile.TemporaryDirectory(prefix="tabs6-firmware-test-") as tmp:
            root = Path(tmp)
            for prefix, members in (("BL", MEMBERS[:1]), ("AP", MEMBERS[1:])):
                with tarfile.open(
                    root / f"{prefix}_T860XXS5DWH1_fixture.tar.md5", "w"
                ) as archive:
                    for name in members:
                        data = f"non-proprietary tabs6 staging test: {name}\n".encode()
                        member = tarfile.TarInfo(name)
                        member.size = len(data)
                        archive.addfile(member, io.BytesIO(data))
            env = dict(os.environ, XDG_STATE_HOME=str(root / "state"))
            for _ in range(2):
                result = subprocess.run(
                    ["bash", "-euo", "pipefail", str(SCRIPT), str(root)],
                    env=env,
                    check=True,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(len(result.stdout.splitlines()), 3)
                for name, store_path in zip(MEMBERS, result.stdout.splitlines()):
                    link = root / "state/korri/gcroots/tabs6-firmware" / name
                    self.assertTrue(link.is_symlink(), f"missing GC root: {link}")
                    self.assertEqual(link.resolve(), Path(store_path))
                    roots = subprocess.check_output(
                        ["nix-store", "--query", "--roots", store_path],
                        text=True,
                        stderr=subprocess.PIPE,
                    )
                    self.assertIn(str(link), roots)

    def test_missing_archives_fail_before_creating_roots(self):
        with tempfile.TemporaryDirectory(prefix="tabs6-firmware-test-") as tmp:
            env = dict(os.environ, XDG_STATE_HOME=str(Path(tmp) / "state"))
            result = subprocess.run(
                ["bash", "-euo", "pipefail", str(SCRIPT), tmp],
                env=env,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("no BL_T860XXS5DWH1 tar", result.stderr)
            self.assertFalse((Path(tmp) / "state").exists())


if __name__ == "__main__":
    unittest.main()
