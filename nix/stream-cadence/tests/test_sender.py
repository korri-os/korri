"""The private sender and the host that runs the tool as a target."""

import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cli import (  # noqa: E402
    BASE,
    LOCAL,
    QUEUE,
    Refusal,
    Remote,
    endpoint,
    fixture_rate,
    pair_measure,
    pair_rate,
    panel_refusal,
    parser,
    queue_measure_refusal,
    rate_refusal,
    view,
    viewer_notes,
    viewer_stream_host,
)
from discovery import session_log  # noqa: E402
import sender  # noqa: E402
import heat  # noqa: E402
import machine  # noqa: E402
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


class TwoMeasures(unittest.TestCase):
    """A private pair has a base measure, the default, and a queue measure, by one option."""

    def test_base_measure_is_the_default_and_its_sender_is_the_slower_side(self):
        rate = fixture_rate(None, 60.0, queue=False)
        self.assertEqual(rate, 60.0)
        # A 17 ms sender output against the viewer fixture's 16 ms output: no queue
        # can build, and the sender adds no dropped frame.
        self.assertEqual(1_000_000 // sender.steady_refresh_mhz(rate), 17)
        self.assertEqual(1_000_000 // round(60.0 * 1000), 16)
        self.assertFalse(parser().parse_args(["view", "--sender", "a", "--viewer", "b"]).queue_measure)
        # A requested rate is kept as it is.
        self.assertEqual(fixture_rate(30.0, 60.0, queue=False), 30.0)

    def test_queue_measure_is_one_named_option_and_pair_rate_knows_the_number(self):
        args = parser().parse_args(
            ["view", "--sender", "a", "--viewer", "b", "--sender-fixture", "--queue-measure"]
        )
        self.assertTrue(args.queue_measure)
        rate = fixture_rate(None, 60.0, queue=True)
        self.assertEqual(rate, pair_rate(60.0))
        self.assertEqual(1_000_000 // sender.steady_refresh_mhz(rate), 15)

    def test_queue_measure_is_refused_where_it_is_not_a_private_pair(self):
        ok = dict(sender_fixture=True, requested_fps=None, installed_viewer=False)
        self.assertIsNone(queue_measure_refusal(**ok))
        cases = {
            "--sender-fixture": dict(ok, sender_fixture=False),
            "--fps": dict(ok, requested_fps=60.0),
            "installed viewer": dict(ok, installed_viewer=True),
        }
        for words, case in cases.items():
            with self.subTest(case=words):
                self.assertRegex(queue_measure_refusal(**case), "--queue-measure.*" + words)
        # Before any device is asked: with a product sender, and with a rate of its own.
        for extra in ([], ["--sender-fixture", "--fps", "67"]):
            args = parser().parse_args(
                ["view", "--sender", "operator@sender", "--viewer", "operator@viewer", "--queue-measure", *extra]
            )
            with self.assertRaisesRegex(Refusal, "--queue-measure"):
                view(args)

    def test_the_two_output_periods_name_the_measure_and_the_option_does_not(self):
        def measure(fps: float | None, refresh: float, queue: bool = False) -> str:
            # The sender's mode, as its agent sets it for the rate that the tool asks.
            mode = sender.steady_refresh_mhz(fixture_rate(fps, refresh, queue=queue)) / 1000
            return pair_measure(mode, refresh)

        self.assertEqual(measure(None, 60.0), BASE)
        self.assertEqual(measure(None, 60.0, queue=True), QUEUE)
        # A rate that makes the sender the faster side is the queue measure, however it was asked.
        self.assertEqual(measure(67.0, 60.0), QUEUE)
        self.assertEqual(measure(63.0, 58.823), QUEUE)
        self.assertEqual(measure(120.0, 60.0), QUEUE)
        # A slower sender is the base measure.
        self.assertEqual(measure(30.0, 60.0), BASE)
        self.assertEqual(measure(60.0, 90.0), BASE)
        # The modes that the tries on aka had (README).
        self.assertEqual(pair_measure(58.823, 60.0), BASE)
        self.assertEqual(pair_measure(66.666, 60.0), QUEUE)
        self.assertEqual(pair_measure(62.5, 58.823), QUEUE)

    def test_a_pair_with_equal_periods_is_refused(self):
        # 16 ms against 16 ms, and 17 ms against 17 ms: the queue fills in 4 s or in 50 s.
        for mode, refresh in ((62.5, 60.0), (58.823, 58.823), (62.5, 62.5)):
            with self.subTest(sender=mode, viewer=refresh):
                with self.assertRaisesRegex(Refusal, "equal.*no steady measure"):
                    pair_measure(mode, refresh)
        with self.assertRaisesRegex(Refusal, "no steady measure"):
            pair_measure(sender.steady_refresh_mhz(63.0) / 1000, 60.0)
        with self.assertRaisesRegex(Refusal, "no steady measure"):
            pair_measure(sender.steady_refresh_mhz(60.0) / 1000, 58.823)

    def test_the_equal_pair_refusal_says_what_the_user_can_do(self):
        # --fps made the pair equal, and the default rate would not: leave it out.
        with self.assertRaisesRegex(Refusal, "Leave --fps out"):
            pair_measure(sender.steady_refresh_mhz(63.0) / 1000, 60.0)
        # A viewer refresh with a whole-millisecond period: the default rate is the equal pair.
        for refresh, limit in ((50.0, "50"), (100.0, "100"), (125.0, "125"), (62.5, "62.5")):
            with self.subTest(refresh=refresh):
                default = sender.steady_refresh_mhz(fixture_rate(None, refresh, queue=False)) / 1000
                with self.assertRaises(Refusal) as refused:
                    pair_measure(default, refresh)
                words = str(refused.exception)
                self.assertNotIn("Leave --fps out", words)
                self.assertIn(f"--fps below {limit} ", words)
                self.assertIn("--queue-measure", words)
                # The advice holds: such a rate gives the base pair.
                below = float(limit) - 0.01
                self.assertEqual(pair_measure(sender.steady_refresh_mhz(below) / 1000, refresh), BASE)
                self.assertEqual(
                    pair_measure(sender.steady_refresh_mhz(pair_rate(refresh)) / 1000, refresh), QUEUE
                )

    def test_a_sender_fixture_faster_than_a_real_panel_is_refused(self):
        # An installed viewer shows on a panel with its own clock. The default asks
        # the sender for the panel's rate, and its whole-millisecond period is longer.
        self.assertIsNone(panel_refusal(sender.steady_refresh_mhz(60.0) / 1000, 60.0))
        self.assertIsNone(panel_refusal(sender.steady_refresh_mhz(59.98) / 1000, 59.98))
        self.assertIsNone(panel_refusal(sender.steady_refresh_mhz(30.0) / 1000, 60.0))
        # Equal on paper: the sender's own work makes its real period the longer one.
        self.assertIsNone(panel_refusal(sender.steady_refresh_mhz(50.0) / 1000, 50.0))
        # --fps above the panel's rate: the result would be `base` with the sender's
        # dropped frames in it, and verdict would judge them as the viewer's.
        for fps, panel in ((63.0, 60.0), (67.0, 60.0), (120.0, 59.98), (62.5, 60.0)):
            with self.subTest(fps=fps, panel=panel):
                self.assertRegex(
                    panel_refusal(sender.steady_refresh_mhz(fps) / 1000, panel),
                    "--fps.*faster than the panel.*sender's",
                )

    def test_a_rate_with_no_headless_mode_is_refused_before_any_device_is_asked(self):
        self.assertIsNone(rate_refusal("--viewer-refresh", 60.0))
        self.assertIsNone(rate_refusal("--fps", 1.0))
        self.assertIsNone(rate_refusal("--fps", 1000.0))
        for value in (0.0004, 0.0009, 0.99, 1000.5, 1001.0, 5000.0):
            with self.subTest(value=value):
                self.assertRegex(rate_refusal("--viewer-refresh", value), "--viewer-refresh.*from 1 to 1000")
        cases = (
            ["--viewer-refresh", "0.0004"],
            ["--sender-fixture", "--viewer-refresh", "1001", "--queue-measure"],
            ["--sender-fixture", "--fps", "0.0005"],
            ["--sender-fixture", "--fps", "2000"],
        )
        for extra in cases:
            with self.subTest(options=extra):
                args = parser().parse_args(
                    ["view", "--sender", "operator@sender", "--viewer", "operator@viewer", *extra]
                )
                with self.assertRaisesRegex(Refusal, "from 1 to 1000"):
                    view(args)

    def test_queue_result_says_that_its_dropped_frames_are_not_the_viewers(self):
        self.assertNotIn("dropped_frames", viewer_notes(BASE))
        self.assertRegex(viewer_notes(QUEUE)["dropped_frames"], "sender.*not the viewer's")
        # What no measure has is in both.
        self.assertEqual(set(viewer_notes(QUEUE)) - set(viewer_notes(BASE)), {"dropped_frames"})


class PairRate(unittest.TestCase):
    """The rate of the queue measure: frames arrive faster than the viewer can show them."""

    def test_sender_output_is_one_millisecond_faster_than_the_viewer_fixtures_output(self):
        for refresh in (50, 58.823, 59.94, 60, 90, 120, 144):
            with self.subTest(refresh=refresh):
                # wlroots arms the viewer fixture's frame timer with this integer division.
                viewer_ms = 1_000_000 // round(refresh * 1000)
                rate = pair_rate(refresh)
                sender_ms = 1_000_000 // sender.steady_refresh_mhz(rate)
                self.assertEqual(sender_ms, viewer_ms - 1)
                # Sunshine takes every frame: the stream's period is not above the output's.
                self.assertLessEqual(1000 / rate, sender_ms)
        # At 60 the viewer fixture's output is a 16 ms output. A 17 ms sender starves
        # it, and a 16 ms sender is as fast as it within the machine's load.
        self.assertEqual(pair_rate(60), 67)
        self.assertEqual(sender.steady_refresh_mhz(pair_rate(60)), 66666)
        self.assertEqual(sender.steady_refresh_mhz(pair_rate(58.823)), 62500)


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
        (root / "home").mkdir()
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

    @staticmethod
    def ended(pid: int) -> bool:
        """A killed process has ended when it is gone, or a zombie that its new parent has not reaped yet.

        The stray is an orphan. On a loaded host its entry can stay in /proc
        for a moment after the kill, and that is not the agent's to end. The
        agent's own check (`survivors`) no longer sees it then.
        """
        for _ in range(50):
            try:
                state = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[0]
            except OSError:
                return True
            if state == "Z":
                return True
            time.sleep(0.1)
        return False

    def test_close_ends_what_descends_from_the_fixture_and_nothing_else(self):
        root = Path(tempfile.mkdtemp(prefix="cadence-sender-test-"))
        session = self.session(root)
        # A child of the fixture leaves a child of its own behind, in the
        # fixture's session; the session's tracked process is only the parent.
        session.spawn(
            "compositor",
            ["sh", "-c", "sleep 60 & echo $! > left-behind; exec sleep 61"],
            session.environment(),
        )
        for _ in range(50):
            if (root / "home" / "left-behind").exists() and (root / "home" / "left-behind").read_text().strip():
                break
            time.sleep(0.1)
        left_behind = int((root / "home" / "left-behind").read_text())
        # An operator's shell in the run directory, and a process that only
        # carries the directory in its environment: the agent started neither.
        operator = subprocess.Popen(["sleep", "60"], cwd=root, start_new_session=True)
        other = subprocess.Popen(
            ["sleep", "60"],
            cwd="/",
            env={"HOME": str(root), "PATH": os.environ["PATH"]},
            start_new_session=True,
        )
        try:
            restore = session.close()
            self.assertEqual(restore["stray_processes_killed"], [left_behind])
            # The agent's own check saw it end; the wait below cannot hide a stray.
            self.assertEqual(restore["remaining_processes"], [])
            self.assertTrue(self.ended(left_behind))
            # Listed and alive: the agent kills only what it can prove its own.
            self.assertEqual(
                sorted(row["pid"] for row in restore["other_processes"]),
                sorted([operator.pid, other.pid]),
            )
            self.assertIsNone(operator.poll())
            self.assertIsNone(other.poll())
            # They belong to the fixture's user, so the check cannot call the run clean.
            self.assertFalse(restore["pass"])
            self.assertTrue(restore["run_folder_removed"])
        finally:
            for process in (operator, other):
                if process.poll() is None:
                    process.kill()
                    process.wait()

    def test_process_of_another_user_is_listed_and_does_not_fail_the_check(self):
        mine = os.geteuid()
        rows = [
            {"pid": 11, "uid": mine, "session": 7},
            {"pid": 12, "uid": mine, "session": 99},
            {"pid": 13, "uid": mine + 1, "session": 7},
            {"pid": 14, "uid": mine + 1, "session": 99},
        ]
        ours, others = sender.sort_matches(rows, mine, {7})
        self.assertEqual([row["pid"] for row in ours], [11])
        self.assertEqual([row["pid"] for row in others], [12, 13, 14])
        # Only a leftover of the fixture's own user can be a fixture process.
        self.assertTrue(sender.fails_restore(others[:1], mine))
        self.assertFalse(sender.fails_restore(others[1:], mine))

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


class NoSensors(unittest.TestCase):
    """A host with no thermal zone and no device tree, whatever machine runs the tests."""

    def setUp(self):
        self.sysfs = tempfile.TemporaryDirectory()
        self.real = (heat.THERMAL, viewer.COMPATIBLE)
        heat.THERMAL = Path(self.sysfs.name) / "thermal"
        viewer.COMPATIBLE = Path(self.sysfs.name) / "compatible"
        machine.power_supplies(self, self.sysfs.name)

    def tearDown(self):
        heat.THERMAL, viewer.COMPATIBLE = self.real
        self.sysfs.cleanup()


class HostViewer(NoSensors):
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


class LocalTarget(NoSensors):
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
