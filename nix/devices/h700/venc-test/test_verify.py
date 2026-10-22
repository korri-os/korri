"""Focused verifier tests using actual FFmpeg encoding and decoding."""

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import numpy as np

from verify import psnr_frames


@unittest.skipUnless(shutil.which("ffmpeg"), "FFmpeg required")
class VerifyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "fixture.h264"
        self.width, self.height, self.frames = 64, 48, 4
        self.encode()

    def encode(self, offset=0):
        x = np.arange(self.width)[None, :]
        y = np.arange(self.height)[:, None]
        frames = []
        for n in range(self.frames):
            luma = (16 + ((x + y + (n + offset) * 4) % 220)).astype(np.uint8)
            frames.append(
                luma.tobytes() + bytes([128]) * (self.width * self.height // 2)
            )
        subprocess.run(
            [
                "ffmpeg",
                "-v",
                "error",
                "-y",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "nv12",
                "-s",
                "64x48",
                "-r",
                "60",
                "-i",
                "-",
                "-c:v",
                "libx264",
                "-qp",
                "18",
                "-bf",
                "0",
                "-g",
                "1",
                "-f",
                "h264",
                str(self.path),
            ],
            input=b"".join(frames),
            check=True,
        )

    def test_accepts_matching_motion(self):
        scores = psnr_frames(self.path, 64, 48, 4, 30)
        self.assertEqual(len(scores), 4)
        self.assertGreater(min(scores), 30)

    def test_rejects_missing_frame(self):
        with self.assertRaises(AssertionError):
            psnr_frames(self.path, 64, 48, 5, 30)

    def test_rejects_extra_frame(self):
        with self.assertRaises(AssertionError):
            psnr_frames(self.path, 64, 48, 3, 30)

    def test_rejects_wrong_motion(self):
        self.encode(offset=25)
        with self.assertRaisesRegex(AssertionError, "low PSNR"):
            psnr_frames(self.path, 64, 48, 4, 30)

    def test_rejects_invalid_stream(self):
        self.path.write_bytes(b"not H.264")
        with self.assertRaises(AssertionError):
            psnr_frames(self.path, 64, 48, 4, 30)


if __name__ == "__main__":
    unittest.main()
