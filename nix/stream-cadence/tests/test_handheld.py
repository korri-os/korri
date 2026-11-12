"""The handheld path and the points of the first review: copies, secrets, choices."""

from collections import deque
import hashlib
import http.server
import json
from pathlib import Path
import ssl
import subprocess
import sys
import tempfile
import threading
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bar import Clean, Unreadable  # noqa: E402
from cli import plugin_enabled  # noqa: E402
from score import first_measured_frame  # noqa: E402
import sender  # noqa: E402
from verdict import differences, judge  # noqa: E402
import heat  # noqa: E402
import machine  # noqa: E402
import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
FIXTURES = TESTS / "fixtures" / "sender"


class InstalledViewer(unittest.TestCase):
    def test_only_an_enabled_plugin_counts_as_installed(self):
        enabled = json.dumps({"id": "viewer", "desired": {"state": "Enabled"}})
        self.assertTrue(plugin_enabled(0, enabled))
        self.assertFalse(plugin_enabled(0, json.dumps({"desired": {"state": "Disabled"}})))
        self.assertFalse(plugin_enabled(0, json.dumps({"id": "viewer"})))
        self.assertFalse(plugin_enabled(1, enabled))
        self.assertFalse(plugin_enabled(0, "not json"))


class PairingWords(unittest.TestCase):
    def test_pin_never_leaves_in_moonlights_words(self):
        words = "Please enter the following PIN on the target PC: 4321\nFailed to pair to server\n"
        self.assertEqual(viewer.pairing_pin(words), "4321")
        self.assertNotIn("4321", viewer.without_pin(words))
        self.assertIn("Failed to pair to server", viewer.without_pin(words))


class FirstFrame(unittest.TestCase):
    def test_first_frame_is_the_first_id_presented_after_the_measured_start(self):
        presented = {ident: {"present_ns": ident * 1000} for ident in range(1, 9)}
        # A stale picture of the locating start, a blank output, then the stream.
        frames = [Clean(3), Unreadable("blank"), Clean(3), Clean(6), Clean(7)]
        self.assertEqual(first_measured_frame(frames, presented, 5500, lambda ns: ns), 3)
        # The sender's clock is ahead of the viewer's by 2000: ID 4 is already new.
        frames = [Clean(3), Clean(4), Clean(5)]
        self.assertEqual(first_measured_frame(frames, presented, 5500, lambda ns: ns + 2000), 1)
        self.assertIsNone(first_measured_frame(frames, presented, 9000, lambda ns: ns))
        # An ID that the sender never presented decides nothing.
        self.assertIsNone(first_measured_frame([Clean(99)], presented, 0, lambda ns: ns))


class Conditions(unittest.TestCase):
    def result(self, **facts) -> list[dict]:
        window = {
            "latency_ms": {"p50": 20.0, "p95": 22.0}, "capture_misses": 0,
            "repeated_frames": 0, "late_frames": 0, "torn_frames": 0,
            "undecodable_frames": 0,
        }
        known = {"valid": True, "model": "host A", "viewer": {"model": "host A"}, "max_capture_gaps": 0}
        return [
            {
                "discovered": {**known, **facts},
                "source_counter": {"windows": [window]},
                "viewer": {"startup_ms": 900.0},
            }
            for _ in range(5)
        ]

    def test_base_measure_and_queue_measure_refuse_each_other_and_name_the_cause(self):
        def pair(measure: str | None, fps: float, refresh: int) -> list[dict]:
            facts = {"requested_fps": fps, "display": {"mode": {"refresh": refresh}}}
            return self.result(**facts, **({} if measure is None else {"measure": measure}))

        base, queue = pair("base", 60.0, 58823), pair("queue", 67.0, 66666)
        for old, new in ((base, queue), (queue, base)):
            with self.assertRaisesRegex(ValueError, "measure") as refused:
                judge(old, new, 5)
            words = str(refused.exception)
            self.assertIn("base measure", words)
            self.assertIn("queue measure", words)
            self.assertIn("--queue-measure", words)
            # The measure is the cause; the rate and the mode follow from it.
            self.assertNotIn("same sender, request and devices", words)
            self.assertNotIn("recorded before", words)
        self.assertEqual(len(judge(queue, queue, 5)), 15)
        # A result from before the tool named its measure is a base result.
        self.assertEqual(len(judge(pair(None, 60.0, 58823), base, 5)), 15)
        # The same rate asked with --fps is not the queue measure: the result says which it is.
        with self.assertRaisesRegex(ValueError, "queue measure"):
            judge(pair("base", 67.0, 66666), queue, 5)

    def test_dropped_frames_of_a_queue_result_are_not_judged(self):
        queue = self.result(measure="queue")
        for record in queue:
            record["viewer"]["not_measured"] = {"dropped_frames": "the sender's floor"}
            record["source_counter"]["windows"][0]["capture_misses"] = 200
        rows = {row["metric"]: row for row in judge(queue, queue, 5)}
        self.assertEqual(rows["dropped_frames"]["verdict"], "not measured")
        self.assertEqual(rows["repeated_frames"]["verdict"], "equal")
        base = {row["metric"]: row for row in judge(self.result(), self.result(), 5)}
        self.assertEqual(base["dropped_frames"]["verdict"], "equal")

    def test_a_raised_gap_limit_is_another_condition(self):
        with self.assertRaisesRegex(ValueError, "max_capture_gaps") as refused:
            judge(self.result(), self.result(max_capture_gaps=3), 5)
        self.assertIn("same sender, request and devices", str(refused.exception))
        self.assertEqual(len(judge(self.result(max_capture_gaps=3), self.result(max_capture_gaps=3), 5)), 15)

    def test_a_result_beside_a_game_is_not_compared_with_one_without(self):
        game = "korri-game-0123456789abcdef.service"

        def live(**units) -> dict:
            return {
                "live_game": {
                    "sender_at_start": [], "viewer_at_start": [], "sender_at_end": [], "viewer_at_end": [], **units,
                }
            }

        free = self.result(**live())
        beside = self.result(**live(sender_at_start=[game], viewer_at_start=[game]))
        with self.assertRaisesRegex(ValueError, "beside_a_game") as refused:
            judge(free, beside, 5)
        self.assertIn("while a game ran on the device", str(refused.exception))
        self.assertNotIn("recorded before", str(refused.exception))
        self.assertEqual(len(judge(beside, beside, 5)), 15)
        # A game at any moment of a run counts: here only at its end, on one side.
        with self.assertRaisesRegex(ValueError, "beside_a_game"):
            judge(free, self.result(**live(sender_at_end=[game])), 5)
        # One run of five beside a game is enough to refuse the set.
        mixed = free[:4] + beside[:1]
        with self.assertRaisesRegex(ValueError, "beside_a_game"):
            judge(mixed, mixed, 5)
        # A result from before the record was kept ran with no game: the tool refused beside one.
        self.assertEqual(len(judge(self.result(), free, 5)), 15)
        # Only the lists of units decide; the trial's results also hold the old key `allowed`.
        self.assertEqual(len(judge(self.result(**live(allowed=True)), free, 5)), 15)

    def test_a_product_sender_has_no_render_node_and_predates_nothing_for_it(self):
        product = self.result()
        fixture = self.result(sender_fixture={"render_node": "/dev/dri/renderD128"})
        with self.assertRaisesRegex(ValueError, "sender_render_node") as refused:
            judge(product, fixture, 5)
        # One has a fixture and one has not: recording again changes nothing.
        self.assertNotIn("recorded before", str(refused.exception))
        self.assertIn("same sender, request and devices", str(refused.exception))

    def test_refusal_says_when_a_result_predates_a_condition(self):
        def run(viewer_model) -> dict:
            window = {
                "latency_ms": {"p50": 20.0, "p95": 22.0}, "capture_misses": 0,
                "repeated_frames": 0, "late_frames": 0, "torn_frames": 0,
                "undecodable_frames": 0,
            }
            seat = {} if viewer_model is None else {"model": viewer_model}
            return {
                "discovered": {"valid": True, "model": "host A", "viewer": seat},
                "source_counter": {"windows": [window]},
                "viewer": {"startup_ms": 900.0},
            }

        stored = [run(None) for _ in range(5)]
        fresh = [run("host A") for _ in range(5)]
        with self.assertRaises(ValueError) as refused:
            judge(stored, fresh, 5)
        words = str(refused.exception)
        self.assertIn("viewer_model", words)
        # The cause is the age of one result, not another sender or request.
        self.assertIn("recorded before the tool kept viewer_model", words)
        self.assertNotIn("same sender and request", words)
        # Two results that both have the condition and differ get the plain advice.
        other = [run("host B") for _ in range(5)]
        with self.assertRaisesRegex(ValueError, "same sender, request and devices") as differ:
            judge(fresh, other, 5)
        self.assertNotIn("recorded before", str(differ.exception))

    def test_another_host_render_node_or_viewer_device_is_another_condition(self):
        def record(**changed) -> dict:
            facts = {
                "model": "host A",
                "sender_fixture": {"render_node": "/dev/dri/renderD128"},
                "viewer": {"model": "handheld A"},
            }
            facts.update(changed)
            return {"discovered": facts}

        self.assertEqual(differences([record()], [record()]), [])
        self.assertEqual(differences([record()], [record(model="host B")]), ["sender_model"])
        self.assertEqual(
            differences([record()], [record(viewer={"model": "handheld B"})]), ["viewer_model"]
        )
        self.assertEqual(
            differences([record()], [record(sender_fixture={"render_node": "/dev/dri/renderD129"})]),
            ["sender_render_node"],
        )
        # A product-sender result from before these facts has none of them.
        old = {"discovered": {"model": "host A", "viewer": {"plugin": None}}}
        self.assertEqual(differences([old], [old]), [])


class Login(unittest.TestCase):
    def test_credentials_file_is_what_sunshine_itself_writes(self):
        # What the stock Sunshine 2026.516 wrote with --creds for this made-up
        # login. A salted hash of a test value; it opens nothing.
        real = {
            "username": "cadence-test",
            "salt": "gXniE8pDJsl0kLz2",  # gitleaks:allow
            "password": "D5F5AE1C29C578A905F778D00CEEBD7A20EFC44598C6F1442A904190A6D57629",  # gitleaks:allow
        }
        ours = json.loads(sender.credentials("cadence-test", "example-password"))
        self.assertEqual(ours.keys(), real.keys())
        self.assertEqual(ours["username"], real["username"])
        self.assertEqual(len(ours["salt"]), len(real["salt"]))
        # The same rule gives Sunshine's own hash for Sunshine's own salt.
        digest = hashlib.sha256(("example-password" + real["salt"]).encode()).digest()
        self.assertEqual(digest[::-1].hex().upper(), real["password"])
        digest = hashlib.sha256(("example-password" + ours["salt"]).encode()).digest()
        self.assertEqual(digest[::-1].hex().upper(), ours["password"])


class PinPost(unittest.TestCase):
    """pin() against a real HTTPS server that answers as Sunshine's web UI does."""

    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory()
        root = Path(cls.directory.name)
        for name in ("server", "other"):
            subprocess.run(
                [
                    "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "2",
                    "-subj", "/CN=Sunshine Gamestream Host",
                    "-keyout", str(root / f"{name}.key"), "-out", str(root / f"{name}.crt"),
                ],
                check=True,
                capture_output=True,
            )
        cls.requests: list[dict] = []

        class WebUi(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                cls.requests.append(
                    {"path": self.path, "authorization": self.headers["Authorization"], **body}
                )
                answer = json.dumps({"status": body["pin"] == "1234"}).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(answer)))
                self.end_headers()
                self.wfile.write(answer)

            def log_message(self, *_values):
                pass

        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), WebUi)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(root / "server.crt", root / "server.key")
        cls.server.socket = context.wrap_socket(cls.server.socket, server_side=True)
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.directory.cleanup()

    def session(self, certificate: str) -> sender.Session:
        session = sender.Session()
        session.root = Path(tempfile.mkdtemp(prefix="cadence-pin-test-", dir=self.directory.name))
        session.login = ("operator-of-the-run", "secret-of-the-run")
        session.facts["sunshine_port"] = self.server.server_address[1] - sender.WEB_UI_OFFSET
        (session.root / "home").mkdir()
        (session.root / "home" / "sunshine.crt").write_text(
            (Path(self.directory.name) / certificate).read_text()
        )
        return session

    def test_pin_goes_to_the_web_ui_with_the_runs_login(self):
        session = self.session("server.crt")
        self.assertEqual(session.pin("1234"), {})
        sent = self.requests[-1]
        self.assertEqual((sent["path"], sent["pin"]), ("/api/pin", "1234"))
        self.assertTrue(sent["authorization"].startswith("Basic "))
        with self.assertRaisesRegex(RuntimeError, "refused the pairing PIN"):
            session.pin("9999")
        with self.assertRaisesRegex(ValueError, "four digits"):
            session.pin("12345")

    def test_pin_is_not_posted_to_a_server_with_another_certificate(self):
        session = self.session("other.crt")
        before = len(self.requests)
        with self.assertRaises(OSError):
            session.pin("1234")
        self.assertEqual(len(self.requests), before)


# The trip points that the two bench handhelds reported on 2026-11-11, and one
# zone of the build host with its placeholder trips.
RG353M = [("passive", 70000), ("passive", 75000), ("critical", 95000)]
RG35XX_PRO = [("passive", 60000), ("passive", 70000), ("critical", 110000)]
ACPI_ZONE = [
    ("passive", -274000), ("passive", -274000), ("critical", 126950),
    ("hot", 126950), ("passive", 126950),
]


def zone(trips: list[tuple[str, int]], temp: int = 50000) -> dict:
    return {
        "zone": "thermal_zone0",
        "type": "cpu-thermal",
        "temp": temp,
        "trips": [{"type": kind, "temp": value} for kind, value in trips],
    }


class HeatLimits(unittest.TestCase):
    def test_limits_come_from_the_devices_own_trip_points(self):
        self.assertEqual(
            heat.thermal_limits(zone(RG353M)),
            {"critical": 95000, "start_below": 70000, "end_at": 85000},
        )
        # Half-way from the last passive trip to the critical one is the lower limit here.
        self.assertEqual(
            heat.thermal_limits(zone(RG35XX_PRO)),
            {"critical": 110000, "start_below": 60000, "end_at": 90000},
        )
        # Placeholder trips count as absent: only margins below the critical trip remain.
        self.assertEqual(
            heat.thermal_limits(zone(ACPI_ZONE)),
            {"critical": 126950, "start_below": 106950, "end_at": 116950},
        )
        self.assertIsNone(heat.thermal_limits(zone([("passive", 70000)])))
        self.assertIsNone(heat.thermal_limits(zone([])))

    def test_a_test_margin_moves_only_the_end_limit(self):
        limits = heat.thermal_limits(zone(RG353M), end_margin=40000)
        self.assertEqual(limits["end_at"], 55000)
        self.assertEqual(limits["start_below"], 70000)

    def test_too_hot_names_the_zone_the_reading_and_the_devices_trip(self):
        limits = {"thermal_zone0": heat.thermal_limits(zone(RG353M))}
        self.assertIsNone(heat.too_hot([zone(RG353M, 69999)], limits, "start_below"))
        refused = heat.too_hot([zone(RG353M, 72222)], limits, "start_below")
        self.assertIn("cpu-thermal reads 72.2", refused)
        self.assertIn("does not start at or above 70.0", refused)
        self.assertIsNone(heat.too_hot([zone(RG353M, 84999)], limits, "end_at"))
        self.assertIn("ends at 85.0", heat.too_hot([zone(RG353M, 85000)], limits, "end_at"))
        # A zone with no critical trip point is recorded and never judged.
        self.assertIsNone(heat.too_hot([zone([], 99000)], {"thermal_zone0": None}, "end_at"))

    def test_busy_viewer_is_seen_after_the_whole_time_and_not_at_startup(self):
        # The short run on the RG353M: a start-up burst, then 0.15.
        fine: deque[tuple[float, int]] = deque()
        rates = [0.66, 0.19, 0.92, 0.38, 0.17, 0.16, 0.14, 0.17]
        spent = 0.0
        seen = []
        for n, rate in enumerate(rates):
            spent += rate * 0.55 * 100
            fine.append((n * 0.55, round(spent)))
            seen.append(viewer.busy(fine, 100))
        self.assertIsNone(seen[0])
        self.assertTrue(all(value is None or value < viewer.BUSY_RATE for value in seen))
        # The long run that ended with the device off the link.
        hot: deque[tuple[float, int]] = deque()
        spent = 0.0
        over = None
        for n, rate in enumerate([0.75, 0.89, 0.93, 0.04, 1.06, 1.39, 1.31, 1.38, 1.25, 1.42]):
            spent += rate * 0.6 * 100
            hot.append((n * 0.6, round(spent)))
            load = viewer.busy(hot, 100)
            if over is None and load is not None and load > viewer.BUSY_RATE:
                over = n * 0.6
        self.assertIsNotNone(over)
        self.assertLess(over, 5.0)

    def test_hardware_decode_is_read_from_the_viewers_own_first_frame_line(self):
        # The line of the RG353M's installed viewer in the short run.
        started = "v4l2m2m: submit: first frame from decoder format=drm_prime (DRM_PRIME=YES)\n"
        self.assertIs(viewer.decode_started("Connecting...\n" + started), True)
        self.assertIs(
            viewer.decode_started(started.replace("drm_prime (DRM_PRIME=YES", "nv12 (DRM_PRIME=NO")),
            False,
        )
        self.assertIsNone(viewer.decode_started("Connecting...\n"))


class HeatGuard(unittest.TestCase):
    """The guard on a real session: real sysfs-shaped files, a real viewer process."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name)
        self.thermal = root / "thermal" / "thermal_zone0"
        self.thermal.mkdir(parents=True)
        (self.thermal / "type").write_text("cpu-thermal\n")
        for n, (kind, value) in enumerate(RG353M):
            (self.thermal / f"trip_point_{n}_type").write_text(kind + "\n")
            (self.thermal / f"trip_point_{n}_temp").write_text(f"{value}\n")
        self.read(50000)
        self.peer = root / "peer"
        self.peer.write_text(f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "host-peer.py"} "$@"\n')
        self.peer.chmod(0o755)
        self.previous = (heat.THERMAL, viewer.COMPATIBLE)
        heat.THERMAL, viewer.COMPATIBLE = root / "thermal", root / "no-device-tree"
        self.supplies = machine.power_supplies(self, root)

    def tearDown(self):
        heat.THERMAL, viewer.COMPATIBLE = self.previous
        self.directory.cleanup()

    def session(self) -> viewer.Session:
        """A session that is closed even when its test fails: no run directory stays behind."""
        made = viewer.Session(str(self.peer))
        self.addCleanup(made.close)
        return made

    def read(self, millidegrees: int) -> None:
        # In one step, as sysfs answers: the guard's thread must never see half a write.
        fresh = self.thermal.with_name("fresh-temp")
        fresh.write_text(f"{millidegrees}\n")
        fresh.replace(self.thermal / "temp")

    def prepare(self, session: viewer.Session, **extra) -> dict:
        return session.handle(
            {
                "operation": "prepare", "fixture": True, "width": 640, "height": 480,
                "refresh_hz": 50.0, "certificate": None, "key": None,
                "server_certificate": "public", **extra,
            }
        )

    def test_zones_and_limits_are_discovered_and_each_sample_has_the_temperature(self):
        session = self.session()
        try:
            found = session.handle({"operation": "discover"})["thermal"]
            self.assertEqual(found[0]["type"], "cpu-thermal")
            self.assertEqual(found[0]["limits"]["end_at"], 85000)
            self.assertEqual(len(found[0]["trips"]), 3)
            self.assertFalse(self.prepare(session)["hardware_path"])
            self.read(61234)
            sample = session.handle({"operation": "sample"})
            self.assertEqual(
                sample["thermal"],
                [{"zone": "thermal_zone0", "type": "cpu-thermal", "temp": 61234}],
            )
            self.assertIsNone(sample["hardware_decode"])
        finally:
            self.assertTrue(session.close()["pass"])

    def test_run_does_not_start_on_a_hot_device(self):
        self.read(72222)
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "device is hot.*72.2.*70.0"):
            self.prepare(session)
        # Nothing was started and no directory was made.
        self.assertIsNone(session.root)
        self.assertEqual(session.processes, {})
        self.assertTrue(session.close()["pass"])

    def test_guard_ends_a_running_viewer_at_once_and_the_run_cleans_up(self):
        session = self.session()
        try:
            root = Path(self.prepare(session)["run_folder"])
            process = session.processes
            session.handle({"operation": "start", "command": ["sleep", "60"]})
            running = process["viewer"]
            session.handle({"operation": "sample"})
            self.read(86000)
            # The guard's own thread stops the viewer; no request is needed for it.
            self.assertIsNotNone(running.wait(timeout=5))
            with self.assertRaisesRegex(RuntimeError, "cpu-thermal reads 86.0.*ends at 85.0"):
                session.handle({"operation": "sample"})
            with self.assertRaisesRegex(RuntimeError, "ends at 85.0"):
                session.handle({"operation": "start", "command": ["sleep", "60"]})
        finally:
            restore = session.close()
        self.assertTrue(restore["pass"], restore)
        self.assertFalse(root.exists())

    def test_a_low_test_limit_ends_a_run_on_a_cool_device(self):
        session = self.session()
        try:
            # 50 degrees is cool; a margin of 50 below the critical 95 puts the end at 45.
            self.prepare(session, test_end_margin=50000)
            session.handle({"operation": "start", "command": ["sleep", "60"]})
            running = session.processes["viewer"]
            self.assertIsNotNone(running.wait(timeout=5))
            with self.assertRaisesRegex(RuntimeError, "reads 50.0.*ends at 45.0"):
                session.handle({"operation": "sample"})
        finally:
            self.assertTrue(session.close()["pass"])


class RenderNodeChoice(unittest.TestCase):
    """prepare() with stand-in programs: real processes, ports, files and cleanup."""

    def prepared(self, nodes: list[str], hardware: list[str]) -> tuple[dict, dict, list[list[str]]]:
        with tempfile.TemporaryDirectory() as temporary:
            peers, dri = Path(temporary) / "bin", Path(temporary) / "dri"
            peers.mkdir()
            dri.mkdir()
            for node in nodes:
                (dri / node).touch()
            (peers / "hardware-nodes").write_text("\n".join(str(dri / node) for node in hardware))
            for role in ("sway", "swaymsg", "sunshine"):
                program = peers / role
                program.write_text(
                    f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "fixture-peer.py"} {peers} {role} "$@"\n'
                )
                program.chmod(0o755)
            previous, sender.RENDER_NODES = sender.RENDER_NODES, dri
            # No thermal zone and no device tree, whatever machine runs the tests.
            sensors = (heat.THERMAL, viewer.COMPATIBLE)
            heat.THERMAL, viewer.COMPATIBLE = Path(temporary) / "thermal", Path(temporary) / "dt"
            machine.power_supplies(self, temporary)
            session = sender.Session()
            try:
                built = session.handle(
                    {
                        "operation": "prepare",
                        "sway": str(peers / "sway"),
                        "sunshine": str(peers / "sunshine"),
                        "width": 640,
                        "height": 480,
                        "fps": 60.024,
                    }
                )
                login = session.login
                stored = json.loads((Path(built["facts"]["sender_fixture"]["run_folder"]) / "home" / "credentials.json").read_text())
            finally:
                sender.RENDER_NODES = previous
                heat.THERMAL, viewer.COMPATIBLE = sensors
                restore = session.close()
            started = [
                json.loads(line) for line in (peers / "arguments.log").read_text().splitlines()
            ]
            self.assertEqual(stored["username"], login[0])
            for arguments in started:
                for secret in login:
                    self.assertNotIn(secret, " ".join(arguments))
            return built["facts"], restore, started

    def test_first_node_with_a_hardware_encoder_wins(self):
        facts, restore, started = self.prepared(["renderD1", "renderD2", "renderD3"], ["renderD2", "renderD3"])
        fixture = facts["sender_fixture"]
        self.assertTrue(fixture["render_node"].endswith("/renderD2"))
        self.assertTrue(fixture["hardware_encoder"])
        self.assertEqual(len(fixture["attempts"]), 2)
        # Why the first node was left: Sunshine's own words about its encoders.
        self.assertTrue(any("Encoder [" in line for line in fixture["attempts"][0]["encoder_log"]))
        self.assertEqual(facts["display"]["mode"], {"width": 640, "height": 480, "refresh": 58823})
        self.assertEqual(facts["sunshine_version"], "2026.516.143833")
        self.assertEqual(len(facts["working_encoders"]), 2)
        self.assertEqual([row[0] for row in started].count("sunshine"), 2)
        self.assertTrue(restore["pass"], restore)
        self.assertEqual(restore["stray_processes_killed"], [])

    def test_with_no_hardware_encoder_the_first_node_that_encodes_is_started_again(self):
        facts, restore, started = self.prepared(["renderD1", "renderD2"], [])
        fixture = facts["sender_fixture"]
        self.assertTrue(fixture["render_node"].endswith("/renderD1"))
        self.assertFalse(fixture["hardware_encoder"])
        self.assertEqual(len(fixture["attempts"]), 3)
        self.assertEqual(len(facts["working_encoders"]), 1)
        self.assertTrue(restore["pass"], restore)

    def test_no_render_node_is_refused_before_anything_starts(self):
        previous, sensors = sender.RENDER_NODES, (heat.THERMAL, viewer.COMPATIBLE)
        with tempfile.TemporaryDirectory() as temporary:
            sender.RENDER_NODES = Path(temporary)
            heat.THERMAL, viewer.COMPATIBLE = Path(temporary) / "thermal", Path(temporary) / "dt"
            machine.power_supplies(self, temporary)
            session = sender.Session()
            try:
                with self.assertRaisesRegex(RuntimeError, "no render node"):
                    session.handle(
                        {"operation": "prepare", "sway": "/unused", "sunshine": "/unused",
                         "width": 640, "height": 480, "fps": 60}
                    )
            finally:
                sender.RENDER_NODES = previous
                heat.THERMAL, viewer.COMPATIBLE = sensors
            self.assertIsNone(session.root)
            self.assertTrue(session.close()["pass"])


if __name__ == "__main__":
    unittest.main()
