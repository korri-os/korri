"""The functional picture verdict must not turn missing evidence into a pass."""

import ast
import gzip
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "picture_proof", Path(__file__).with_name("picture.py")
)
picture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(picture)

LOG = "Platform V4L2 M2M + SDL NV12\nv4l2m2m: setup: decoder=h264_v4l2m2m presenter=sdl-nv12 1280x720\n"


def shots(ids=(10, 20, 30), seconds=(5, 37, 70)):
    return [{"seconds": at, "frame": {"id": ident}} for ident, at in zip(ids, seconds)]


class PictureEvidence(unittest.TestCase):
    def test_the_device_monitor_is_valid_prebuilt_python_source(self):
        ast.parse(picture.MONITOR)

    def test_sender_log_compression_preserves_the_exact_evidence(self):
        payload = "private fixture log\n" * 100
        result = {"sender_last_sample": {"journal": payload}}
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder)
            picture.save_sender_logs(result, out)
            reference = result["sender_last_sample"]["journal"]
            self.assertEqual(
                gzip.decompress((out / reference["file"]).read_bytes()),
                payload.encode(),
            )
            self.assertEqual(
                reference["uncompressed_sha256"],
                hashlib.sha256(payload.encode()).hexdigest(),
            )
            picture.save_sender_logs(result, out)
            self.assertEqual(result["sender_last_sample"]["journal"], reference)

    def test_sunshine_stream_keys_are_not_retained_in_the_compressed_log(self):
        key = "0" * 32
        result = {"sender_last_sample": {"journal": "Debug: rikey -- " + key + "\n"}}
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder)
            picture.save_sender_logs(result, out)
            reference = result["sender_last_sample"]["journal"]
            saved = gzip.decompress((out / reference["file"]).read_bytes()).decode()
            self.assertNotIn(key, saved)
            self.assertIn("rikey -- [redacted]", saved)
            self.assertEqual(reference["transient_stream_key_redactions"], 1)

    def test_60_seconds_of_readable_rising_pictures_and_the_real_decoder(self):
        self.assertTrue(all(picture.picture_checks(shots(), LOG).values()))

    def test_software_decode_does_not_pass_as_hardware(self):
        self.assertFalse(
            picture.picture_checks(shots(), "Using FFmpeg decoder: h264")[
                "hardware_path"
            ]
        )

    def test_a_short_run_does_not_pass(self):
        self.assertFalse(
            picture.picture_checks(shots(seconds=(5, 30, 64)), LOG)[
                "at_least_60_seconds"
            ]
        )

    def test_unreadable_missing_or_frozen_pictures_do_not_pass(self):
        unreadable = shots()
        unreadable[1]["frame"] = {"unreadable": "no bar"}
        for evidence in (
            unreadable,
            shots()[:2],
            shots(ids=(10, 10, 10)),
            shots(ids=(30, 20, 10)),
        ):
            with self.subTest(evidence=evidence):
                self.assertFalse(all(picture.picture_checks(evidence, LOG).values()))


if __name__ == "__main__":
    unittest.main()
