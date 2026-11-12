"""The private sender and the host that runs the tool as a target."""

import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cli import (  # noqa: E402
    LOCAL,
    Refusal,
    Remote,
    endpoint,
    parser,
    view,
    viewer_stream_host,
)
from discovery import session_log  # noqa: E402
import sender  # noqa: E402
import viewer  # noqa: E402

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
    def test_port_base_has_the_whole_span_free_and_leaves_sunshines_default_span(self):
        base = sender.port_base()
        self.assertTrue(sender.span_free(base))
        self.assertGreaterEqual(abs(base - sender.SUNSHINE_DEFAULT_BASE), len(sender.PORT_SPAN))
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
                detached=True,
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


class HostViewer(unittest.TestCase):
    """A host with no Korri: the calling user, a supplied sway, no Korri unit."""

    def program(self, directory: Path) -> str:
        path = directory / "peer"
        path.write_text(f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "host-peer.py"} "$@"\n')
        path.chmod(0o755)
        return str(path)

    def test_fixture_runs_as_the_caller_and_pairs_by_pin(self):
        with tempfile.TemporaryDirectory() as temporary:
            peer = self.program(Path(temporary))
            session = viewer.Session(peer)
            try:
                facts = session.handle({"operation": "discover"})
                self.assertIsNone(facts["compositor"])
                self.assertIsNone(facts["plugin"])
                prepared = session.handle(
                    {
                        "operation": "prepare",
                        "fixture": True,
                        "width": 640,
                        "height": 480,
                        "refresh_hz": 50.0,
                        "certificate": None,
                        "key": None,
                        "server_certificate": "public",
                    }
                )
                root, keys = Path(prepared["run_folder"]), Path(prepared["keydir"])
                self.assertTrue((root / "rt" / viewer.DISPLAY).is_socket())
                self.assertEqual(root.stat().st_uid, os.getuid())
                self.assertEqual(list(keys.iterdir()), [])
                command = [peer, "pair", "-keydir", str(keys), "-port", "1", "sender"]
                self.assertEqual(
                    session.handle({"operation": "pair", "command": command}), {"pin": "4321"}
                )
                (keys / "admitted").touch()
                words = session.handle({"operation": "paired"})["log"]
                self.assertIn("Succesfully paired", words)
                self.assertNotIn("4321", words)
                (keys / "admitted").unlink()
                (keys / "refused").touch()
                session.handle({"operation": "pair", "command": command})
                with self.assertRaisesRegex(RuntimeError, "did not pair") as refused:
                    session.handle({"operation": "paired"})
                # Moonlight's words reach the result; the PIN stays in the run directory.
                self.assertIn("Failed to pair to server", str(refused.exception))
                self.assertNotIn("4321", str(refused.exception))
            finally:
                restore = session.close()
            self.assertTrue(restore["pass"], restore)
            self.assertEqual(restore["pids_unchanged"], {})
            self.assertFalse(root.exists())

    def test_host_has_no_installed_viewer(self):
        session = viewer.Session("/unused/sway")
        with self.assertRaisesRegex(RuntimeError, "only the fixture"):
            session.handle(
                {"operation": "prepare", "fixture": False, "server_certificate": "public"}
            )
        self.assertTrue(session.close()["pass"])

    def test_pairing_pin_is_read_from_moonlights_own_words(self):
        self.assertEqual(
            viewer.pairing_pin("Please enter the following PIN on the target PC: 0042\n"), "0042"
        )
        self.assertIsNone(viewer.pairing_pin("Failed to pair to server: 12345\n"))


class LocalTarget(unittest.TestCase):
    def test_local_names_this_host_and_other_values_stay_ssh_targets(self):
        self.assertEqual(endpoint(LOCAL, ["UserKnownHostsFile=/x"]), (None, []))
        target, options = endpoint("operator@device:2222", [])
        self.assertEqual(target, "operator@device")
        self.assertEqual(options[-2:], ["-p", "2222"])

    def test_remote_viewer_reaches_a_local_sender_at_the_far_end_of_its_ssh_link(self):
        def chosen(given=None, **facts) -> str:
            values = dict(
                same_clock=False,
                sender_is_this_host=True,
                controller_address="link-address",
                sender_host="sender",
                loopback="loopback",
            )
            return viewer_stream_host(given, **{**values, **facts})

        self.assertEqual(chosen(), "link-address")
        self.assertEqual(chosen("given"), "given")
        self.assertEqual(chosen(same_clock=True), "loopback")
        self.assertEqual(chosen(sender_is_this_host=False), "sender")
        # Never the viewer's own loopback: with no SSH link the tool asks.
        with self.assertRaisesRegex(Refusal, "--viewer-stream-host"):
            chosen(controller_address=None)

    def test_viewer_agent_reports_the_far_end_of_its_ssh_connection(self):
        previous = os.environ.get("SSH_CONNECTION")
        try:
            os.environ["SSH_CONNECTION"] = "192.0.2.7 50000 192.0.2.1 22"
            self.assertEqual(viewer.Session("/unused/sway").facts["controller_address"], "192.0.2.7")
            del os.environ["SSH_CONNECTION"]
            self.assertIsNone(viewer.Session("/unused/sway").facts["controller_address"])
        finally:
            if previous is not None:
                os.environ["SSH_CONNECTION"] = previous

    def test_local_sender_needs_the_fixture(self):
        args = parser().parse_args(["view", "--sender", LOCAL, "--viewer", LOCAL])
        self.assertFalse(args.sender_fixture)
        with self.assertRaisesRegex(Refusal, "--sender-fixture"):
            view(args)


if __name__ == "__main__":
    unittest.main()
