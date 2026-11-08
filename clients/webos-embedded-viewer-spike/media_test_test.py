"""Native CLI/ss4s tests. No ARM execution or vendor media is used."""

import os
import re
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path


class MediaCLI(unittest.TestCase):
    binary = os.environ["MEDIA_TEST_BINARY"]
    stream = os.environ["MEDIA_TEST_STREAM"]
    command = [binary] + (
        ["media-test"] if os.environ.get("MEDIA_TEST_MULTICALL") else []
    )

    def run_cli(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [*self.command, *args],
            capture_output=True,
            text=True,
            check=False,
            timeout=20,
        )

    def test_bundled_stream_has_300_complete_access_units(self) -> None:
        result = self.run_cli("--check-stream", self.stream)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("300 access units", result.stderr)

    def test_invalid_and_truncated_streams_fail_before_module_load(self) -> None:
        original = Path(self.stream).read_bytes()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "invalid.h264"
            for data in (b"", b"not annex b", original[:200], original[:-50000]):
                path.write_bytes(data)
                result = self.run_cli("--check-stream", str(path))
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn("module load begin", result.stderr)

    def test_duration_is_bounded_before_loading_media(self) -> None:
        for duration in ("0", "26", "-1", "1x", "", "99999999999999999999"):
            result = self.run_cli(duration, self.stream)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertNotIn("module load begin", result.stderr)

    def test_reference_sink_is_paced_and_closes(self) -> None:
        start = time.monotonic()
        result = self.run_cli("1", self.stream)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertGreaterEqual(time.monotonic() - start, 1)
        first = re.search(
            r"^\[(\d+\.\d+)\] stage: first frame fed", result.stderr, re.MULTILINE
        )
        close = re.search(
            r"^\[(\d+\.\d+)\] stage: close begin", result.stderr, re.MULTILINE
        )
        self.assertIsNotNone(first)
        self.assertIsNotNone(close)
        assert first is not None and close is not None
        # Exclude the reference sink's 500 ms open and close calls.
        self.assertGreaterEqual(float(close[1]) - float(first[1]), 0.95)
        self.assertLess(float(close[1]) - float(first[1]), 1.5)
        self.assertIn("frames=30", result.stderr)
        self.assertIn("first frame fed", result.stderr)
        self.assertIn("close complete", result.stderr)

    def test_stream_repeats_at_the_first_idr_without_reopening(self) -> None:
        result = self.run_cli("11", self.stream)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("frames=330", result.stderr)
        self.assertEqual(result.stderr.count("open accepted"), 1)

    def test_wrong_app_id_fails_without_loading_a_module(self) -> None:
        result = subprocess.run(
            [*self.command, "1", self.stream],
            env={**os.environ, "APPID": "another.app"},
            capture_output=True,
            text=True,
            check=False,
            timeout=5,
        )
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertNotIn("module load begin", result.stderr)

    def test_missing_file_fails_without_loading_a_module(self) -> None:
        result = self.run_cli("1", self.stream + ".missing")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertNotIn("module load begin", result.stderr)

    def test_signals_close_the_reference_sink(self) -> None:
        for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGALRM):
            with tempfile.TemporaryFile(mode="w+") as log:
                process = subprocess.Popen(
                    [*self.command, "10", self.stream], stderr=log, text=True
                )
                try:
                    deadline = time.monotonic() + 5
                    while time.monotonic() < deadline:
                        log.seek(0)
                        if "first frame fed" in log.read():
                            break
                        time.sleep(0.02)
                    else:
                        self.fail("first frame was not fed")
                    process.send_signal(sig)
                    self.assertEqual(process.wait(timeout=5), 128 + sig)
                    log.seek(0)
                    self.assertIn("close complete", log.read())
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()


if __name__ == "__main__":
    unittest.main()
