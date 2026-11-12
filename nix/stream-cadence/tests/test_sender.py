"""The private sender."""

import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cli import Refusal, Remote  # noqa: E402
from discovery import session_log  # noqa: E402
import sender  # noqa: E402

TESTS = Path(__file__).resolve().parent
FIXTURES = TESTS / "fixtures" / "sender"


class SteadyMode(unittest.TestCase):
    def test_headless_frame_period_is_never_shorter_than_the_streams(self):
        for rate in (24, 30, 50, 59.94, 60, 90, 120, 144):
            with self.subTest(rate=rate):
                # wlroots arms its frame timer with this integer division.
                period_ms = 1_000_000 // sender.steady_refresh_mhz(rate)
                self.assertGreaterEqual(period_ms, 1000 / rate)
                self.assertLess(period_ms - 1, 1000 / rate)
        self.assertEqual(sender.steady_refresh_mhz(60), 58823)
        self.assertEqual(sender.steady_refresh_mhz(50), 50000)


class Ports(unittest.TestCase):
    def test_port_base_has_the_whole_span_free_and_leaves_the_product_span(self):
        base = sender.port_base()
        self.assertTrue(sender.span_free(base))
        self.assertGreaterEqual(abs(base - sender.PRODUCT_BASE), len(sender.PORT_SPAN))
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as taken:
            taken.bind(("", base + sender.PORT_SPAN[-1]))
            self.assertFalse(sender.span_free(base))
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as taken:
            taken.bind(("", base + sender.PORT_SPAN[0]))
            self.assertFalse(sender.span_free(base))


class SunshineContract(unittest.TestCase):
    def test_configuration_stays_in_the_run_directory_and_names_no_encoder(self):
        root = Path("/tmp/run")
        values = dict(
            line.split(" = ", 1)
            for line in sender.configuration(root, 40123, "/dev/dri/renderD999").splitlines()
        )
        self.assertEqual(values["port"], "40123")
        self.assertEqual(values["adapter_name"], "/dev/dri/renderD999")
        self.assertEqual(values["origin_web_ui_allowed"], "pc")
        self.assertNotIn("encoder", values)
        for key in ("file_apps", "file_state", "credentials_file", "pkey", "cert", "log_path"):
            self.assertTrue(values[key].startswith(str(root) + "/"), key)

    def test_startup_log_with_a_hardware_encoder(self):
        found = sender.startup((FIXTURES / "sunshine-startup-hardware.log").read_text())
        self.assertEqual(found["sunshine_version"], "2026.516.143833")
        self.assertEqual(len(found["working_encoders"]), 2)
        self.assertTrue(found["hardware_encoder"])

    def test_startup_log_that_fell_back_to_the_cpu_encoder(self):
        log = (FIXTURES / "sunshine-startup-software.log").read_text()
        found = sender.startup(log)
        self.assertEqual(len(found["working_encoders"]), 1)
        self.assertFalse(found["hardware_encoder"])
        self.assertFalse(sender.startup("")["hardware_encoder"])
        # The log of a stream names what verdict compares as conditions.
        self.assertEqual(session_log(log)["capture_backend"], "Wayland's protocol")


class Cleanup(unittest.TestCase):
    def session(self, root: Path) -> sender.Session:
        session = sender.Session()
        session.root = root
        (root / "rt").mkdir()
        return session

    def test_close_ends_tracked_processes_and_removes_the_run_directory(self):
        root = Path(tempfile.mkdtemp(prefix="cadence-sender-test-"))
        session = self.session(root)
        tracked = session.spawn("compositor", ["sleep", "60"], session.environment())
        self.assertEqual(sender.survivors(root), [tracked.pid])
        restore = session.close()
        self.assertTrue(restore["pass"], restore)
        self.assertEqual(restore["stray_processes_killed"], [])
        self.assertTrue(restore["run_folder_removed"])
        self.assertFalse(root.exists())
        self.assertIsNotNone(tracked.poll())

    def test_close_finds_and_ends_a_process_that_the_session_did_not_track(self):
        root = Path(tempfile.mkdtemp(prefix="cadence-sender-test-"))
        session = self.session(root)
        # One hides in another session with only its working directory there;
        # the other carries the run directory only in its environment.
        by_cwd = subprocess.Popen(["sleep", "60"], cwd=root, start_new_session=True)
        by_environment = subprocess.Popen(
            ["sleep", "60"],
            cwd="/",
            env={"HOME": str(root), "PATH": os.environ["PATH"]},
            start_new_session=True,
        )
        try:
            restore = session.close()
            self.assertEqual(
                restore["stray_processes_killed"], sorted([by_cwd.pid, by_environment.pid])
            )
            self.assertEqual(restore["remaining_processes"], [])
            self.assertTrue(restore["pass"], restore)
            self.assertEqual(by_cwd.wait(timeout=5), -9)
            self.assertEqual(by_environment.wait(timeout=5), -9)
        finally:
            for process in (by_cwd, by_environment):
                if process.poll() is None:
                    process.kill()
                    process.wait()

    def test_agent_answers_the_device_protocol_and_restores_without_a_fixture(self):
        with tempfile.TemporaryDirectory() as temporary:
            remote = Remote(
                [sys.executable, "-B", str(TESTS.parent / "sender.py")],
                Path(temporary) / "agent.log",
            )
            facts = remote.request("discover")
            self.assertRegex(facts["boot_id"], r"^[0-9a-f-]{36}$")
            self.assertIsNone(facts["display"])
            self.assertIn("device_ns", remote.request("clock"))
            with self.assertRaisesRegex(Refusal, "not prepared"):
                remote.request("start", motion="unused")
            restore = remote.close()
            self.assertTrue(restore["pass"], restore)
            self.assertEqual(remote.process.returncode, 0)


if __name__ == "__main__":
    unittest.main()
