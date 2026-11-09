"""Actual Opus callbacks and private Pulse null-sink monitor; no device/host."""

import array
import os
import re
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

BINARY = os.environ["AUDIO_TEST_BINARY"]


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(3)


class AudioTests(unittest.TestCase):
    def test_real_opus_frames_feed_and_bounded_diagnostics(self):
        result = subprocess.run([BINARY], capture_output=True, timeout=30, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        lines = result.stderr.decode().splitlines()
        init = [line for line in lines if "stage=audio_init " in line]
        reports = [line for line in lines if "stage=audio " in line]
        self.assertEqual(len(init), 5)
        self.assertEqual(len(reports), 4 * 30)
        self.assertTrue(all(len(line) <= 512 for line in lines))
        for index in range(4):
            connection = reports[index * 30 : (index + 1) * 30]
            seconds = []
            for line in connection:
                match = re.search(r"second=(\d+)", line)
                assert match is not None
                seconds.append(int(match.group(1)))
            self.assertEqual(seconds, list(range(1, 31)))
            self.assertRegex(connection[0], r"peak=[1-9][0-9]*")
        self.assertNotIn("-----BEGIN", result.stderr.decode())

    def test_pcm_reaches_real_pa_simple_write_and_pulse_monitor(self):
        with tempfile.TemporaryDirectory(prefix="korri-audio-") as directory:
            root = Path(directory)
            socket = root / "pulse.sock"
            (root / "config").mkdir(mode=0o700)
            (root / "runtime").mkdir(mode=0o700)
            environment = dict(
                os.environ,
                PULSE_SERVER="unix:" + str(socket),
                HOME=str(root),
                XDG_CONFIG_HOME=str(root / "config"),
                XDG_RUNTIME_DIR=str(root / "runtime"),
            )
            with (root / "daemon.log").open("wb") as daemon_log:
                daemon = subprocess.Popen(
                    [
                        "pulseaudio",
                        "-n",
                        "--daemonize=no",
                        "--use-pid-file=no",
                        "--exit-idle-time=-1",
                        "--disable-shm=yes",
                        "--load=module-native-protocol-unix socket="
                        + str(socket)
                        + " auth-anonymous=1",
                        "--load=module-null-sink sink_name=korri_audio_test rate=48000 channels=2 format=s16le",
                    ],
                    env=environment,
                    stdout=daemon_log,
                    stderr=subprocess.STDOUT,
                )
                try:
                    deadline = time.monotonic() + 10
                    while not socket.exists():
                        self.assertIsNone(
                            daemon.poll(), (root / "daemon.log").read_text()
                        )
                        self.assertLess(time.monotonic(), deadline)
                        time.sleep(0.02)
                    with (root / "monitor.pcm").open("w+b") as capture:
                        recorder = subprocess.Popen(
                            [
                                "parec",
                                "--raw",
                                "--format=s16le",
                                "--rate=48000",
                                "--channels=2",
                                "--device=korri_audio_test.monitor",
                                "--latency-msec=20",
                            ],
                            env=environment,
                            stdout=capture,
                            stderr=subprocess.DEVNULL,
                        )
                        try:
                            time.sleep(0.2)
                            result = subprocess.run(
                                [BINARY, "pulse"],
                                env=environment,
                                capture_output=True,
                                timeout=20,
                                check=False,
                            )
                            self.assertEqual(result.returncode, 0, result.stderr)
                            time.sleep(0.1)
                        finally:
                            stop(recorder)
                        capture.seek(0)
                        pcm = array.array("h", capture.read(2 * 1024 * 1024))
                    self.assertGreater(len(pcm), 48000)
                    peak = max(abs(value) for value in pcm)
                    rms = (sum(value * value for value in pcm) / len(pcm)) ** 0.5
                    self.assertGreater(peak, 1000)
                    self.assertGreater(rms, 100)
                    print(
                        f"Private Pulse monitor: samples={len(pcm)} peak={peak} RMS={rms:.1f}"
                    )
                finally:
                    stop(daemon)


if __name__ == "__main__":
    unittest.main()
