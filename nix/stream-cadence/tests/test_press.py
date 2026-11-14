"""press.c against a real X server: Xvfb with a client of Moonlight's window shape.

The tests derivation builds press.c and tests/key-window.c and names them in
CADENCE_PRESS and CADENCE_KEY_WINDOW; Xvfb comes from its inputs.
"""

import os
from pathlib import Path
import subprocess
import sys
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import input_measure  # noqa: E402


def program(name: str) -> str:
    path = os.environ.get(name)
    if not path:
        raise AssertionError(f"{name} is not set: run these tests through the tests output of default.nix.")
    return path


class XServer:
    """A private Xvfb that picks its own display number."""

    def __init__(self) -> None:
        read, write = os.pipe()
        self.process = subprocess.Popen(
            ["Xvfb", "-displayfd", str(write), "-nolisten", "tcp", "-screen", "0", "640x480x24"],
            pass_fds=(write,),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        os.close(write)
        with os.fdopen(read) as numbers:
            number = numbers.readline().strip()
        if not number:
            self.process.kill()
            raise AssertionError("Xvfb named no display")
        self.environment = dict(os.environ, DISPLAY=":" + number)

    def close(self) -> None:
        self.process.terminate()
        self.process.wait(timeout=10)


def stop(process: subprocess.Popen) -> None:
    process.kill()
    process.wait(timeout=10)
    if process.stdout is not None:
        process.stdout.close()


class Press(unittest.TestCase):
    def setUp(self) -> None:
        self.server = XServer()
        self.addCleanup(self.server.close)

    def window(self, shape: str) -> tuple[subprocess.Popen, int]:
        client = subprocess.Popen(
            [program("CADENCE_KEY_WINDOW"), "1000", shape],
            env=self.server.environment,
            stdout=subprocess.PIPE,
            text=True,
        )
        self.addCleanup(stop, client)
        assert client.stdout is not None
        ready = client.stdout.readline().split()
        self.assertEqual(ready[0], "ready")
        return client, int(ready[1])

    def press(self, pid: int, *arguments: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [program("CADENCE_PRESS"), str(pid), *arguments],
            env=self.server.environment,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def test_each_press_and_release_reaches_the_viewers_window_after_its_time(self):
        client, window = self.window("keys")
        result = self.press(client.pid, "3", "10", "60", "20", "7")
        self.assertEqual(result.returncode, 0, result.stderr)
        log = input_measure.press_log(result.stdout)
        self.assertTrue(log["done"])
        self.assertEqual(log["target"]["window"], window)
        self.assertEqual(log["target"]["owner_pid"], client.pid)
        measured = [row for row in log["events"] if row["index"] >= 0]
        self.assertEqual([row["kind"] for row in measured], ["press", "release"] * 3)
        self.assertTrue(all(row["focused"] for row in log["events"]))
        time.sleep(0.3)
        client.kill()
        client.wait(timeout=10)
        assert client.stdout is not None
        keys = [line.split() for line in client.stdout.read().splitlines()]
        # The warm-up may arrive or not (Xwayland drops it); the measured six arrive last.
        received = keys[-6:]
        self.assertGreaterEqual(len(keys), 6)
        self.assertEqual([row[1] for row in received], ["press", "release"] * 3)
        for sent, got in zip(measured, received):
            self.assertEqual(int(got[2]), log["target"]["keycode"])
            # A device event from the server, not a client's synthetic event.
            self.assertEqual(got[4], "0")
            self.assertGreaterEqual(int(got[3]), sent["realtime_ns"])

    def test_a_process_with_no_window_gets_no_press(self):
        self.window("keys")
        other = subprocess.Popen(["sleep", "30"])
        self.addCleanup(stop, other)
        result = self.press(other.pid, "1", "10", "60", "0", "1")
        self.assertEqual(result.returncode, 5)
        self.assertIn("no viewable window that asks for key presses", result.stderr)

    def test_a_window_that_takes_no_keys_gets_no_press(self):
        client, _ = self.window("nokeys")
        result = self.press(client.pid, "1", "10", "60", "0", "1")
        self.assertEqual(result.returncode, 5)

    def test_two_windows_that_take_keys_are_no_single_target(self):
        client, _ = self.window("two")
        result = self.press(client.pid, "1", "10", "60", "0", "1")
        self.assertEqual(result.returncode, 5)
        self.assertIn("more than one window", result.stderr)

    def test_a_hold_as_long_as_the_gap_is_refused(self):
        result = self.press(1, "1", "60", "60", "0", "1")
        self.assertEqual(result.returncode, 2)

    def test_a_hold_that_auto_repeat_can_lengthen_is_refused(self):
        # An X server repeats a held key after its repeat delay (660 ms by default).
        result = self.press(1, "1", "500", "2000", "0", "1")
        self.assertEqual(result.returncode, 2)
        self.assertIn("auto-repeat", result.stderr)


if __name__ == "__main__":
    unittest.main()
