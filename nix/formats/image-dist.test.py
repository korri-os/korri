#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 zstd
"""Exercise image staging and verification with real files and compression."""

import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

COMMAND = Path(__file__).with_name("image-dist.py")
REVISION = "a" * 40


class ImageDistributionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.result = self.root / "result"
        (self.result / "sd-image").mkdir(parents=True)
        self.destination = self.root / "dist"

    def run_command(self, *arguments):
        return subprocess.run(
            [sys.executable, str(COMMAND), *map(str, arguments)],
            text=True,
            capture_output=True,
            timeout=30,
        )

    def image(self, compressed=True):
        path = self.result / "sd-image" / "nixos-rg353m.img"
        path.write_bytes(b"image fixture\n" * 128)
        if compressed:
            subprocess.run(["zstd", "-q", "--rm", str(path)], check=True)
            path = path.with_suffix(".img.zst")
        return path

    def stage(self):
        return self.run_command("stage", self.result, self.destination, REVISION)

    def test_stages_only_image_checksum_and_revision(self):
        image = self.image()
        staged = self.stage()
        self.assertEqual(staged.returncode, 0, staged.stderr)
        self.assertEqual(
            {p.name for p in self.destination.iterdir()},
            {image.name, image.name + ".sha256", "korri-revision.txt"},
        )
        self.assertEqual(
            (self.destination / image.name).read_bytes(), image.read_bytes()
        )
        expected = (
            hashlib.sha256(image.read_bytes()).hexdigest() + "  " + image.name + "\n"
        )
        self.assertEqual(
            (self.destination / (image.name + ".sha256")).read_text(), expected
        )
        self.assertEqual(
            (self.destination / "korri-revision.txt").read_text(), REVISION + "\n"
        )
        verified = self.run_command("verify", self.destination, REVISION, "--release")
        self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertEqual(verified.stdout.strip(), image.name)

    def test_supports_existing_uncompressed_sd_output(self):
        image = self.image(compressed=False)
        staged = self.stage()
        self.assertEqual(staged.returncode, 0, staged.stderr)
        self.assertEqual(
            (self.destination / image.name).read_bytes(), image.read_bytes()
        )

    def test_rejects_missing_or_multiple_images(self):
        self.assertNotEqual(self.stage().returncode, 0)
        self.image()
        self.image(compressed=False)
        self.assertNotEqual(self.stage().returncode, 0)
        self.assertFalse(self.destination.exists())

    def test_rejects_empty_image(self):
        image = self.image(compressed=False)
        image.write_bytes(b"")
        self.assertNotEqual(self.stage().returncode, 0)
        self.assertFalse(self.destination.exists())

    def test_rejects_symlink_image(self):
        image = self.image(compressed=False)
        target = self.root / "outside.img"
        image.rename(target)
        image.symlink_to(target)
        self.assertNotEqual(self.stage().returncode, 0)

    def test_rejects_corrupt_compression_without_partial_output(self):
        image = self.image()
        image.write_bytes(b"not zstd")
        self.assertNotEqual(self.stage().returncode, 0)
        self.assertFalse(self.destination.exists())
        self.assertEqual(list(self.root.glob(".image-dist-*")), [])

    def test_does_not_overwrite_existing_destination(self):
        self.image()
        self.destination.mkdir()
        marker = self.destination / "keep"
        marker.write_text("unchanged")
        self.assertNotEqual(self.stage().returncode, 0)
        self.assertEqual(marker.read_text(), "unchanged")

    def test_rejects_wrong_revision_checksum_and_extra_files(self):
        image = self.image()
        self.assertEqual(self.stage().returncode, 0)
        self.assertNotEqual(
            self.run_command("verify", self.destination, "b" * 40).returncode, 0
        )
        checksum = self.destination / (image.name + ".sha256")
        original = checksum.read_text()
        checksum.write_text("0" * 64 + "  " + image.name + "\n")
        self.assertNotEqual(
            self.run_command("verify", self.destination, REVISION).returncode, 0
        )
        checksum.write_text(original)
        (self.destination / "unexpected").write_text("not a release asset")
        self.assertNotEqual(
            self.run_command("verify", self.destination, REVISION).returncode, 0
        )

    def test_rejects_symlink_and_oversized_metadata(self):
        image = self.image()
        self.assertEqual(self.stage().returncode, 0)
        revision = self.destination / "korri-revision.txt"
        outside = self.root / "outside-revision"
        revision.rename(outside)
        revision.symlink_to(outside)
        self.assertNotEqual(
            self.run_command("verify", self.destination, REVISION).returncode, 0
        )
        revision.unlink()
        revision.write_bytes(outside.read_bytes())
        (self.destination / (image.name + ".sha256")).write_bytes(b"x" * 4097)
        self.assertNotEqual(
            self.run_command("verify", self.destination, REVISION).returncode, 0
        )

    def test_checks_release_size_before_reading_large_file(self):
        image = self.image(compressed=False)
        self.assertEqual(self.stage().returncode, 0)
        with (self.destination / image.name).open("r+b") as output:
            output.truncate(2 * 1024**3)
        result = self.run_command("verify", self.destination, REVISION, "--release")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("release asset limit", result.stderr)

    def test_rejects_invalid_revision(self):
        self.image()
        result = self.run_command(
            "stage", self.result, self.destination, "not-a-commit"
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.destination.exists())


if __name__ == "__main__":
    unittest.main()
