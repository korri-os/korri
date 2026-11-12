"""The handheld path and the points of the first review: copies, secrets, choices."""

import hashlib
import http.server
import json
import os
from pathlib import Path
import re
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
from verdict import differences  # noqa: E402
import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
FIXTURES = TESTS / "fixtures" / "sender"
# The build passes korrid's file in; a checkout has it at its own place.
KORRID_VIEWER = Path(
    os.environ.get(
        "CADENCE_KORRID_VIEWER",
        TESTS.parents[2] / "services/korrid/src/linux_viewer/production.rs",
    )
)


def korrid_picture_paths(source: str) -> tuple[list[tuple[set[bytes], str]], str | None]:
    """The chip table of korrid's platform_from_path: branches in order, then the default."""
    body = source.split("fn platform_from_path", 1)[1].split("\n}\n", 1)[0]
    table = body.split("let mut values", 1)[1]
    branches: list[tuple[set[bytes], str]] = []
    chips: set[bytes] = set()
    default = None
    for token in re.finditer(r'b"([^"]+)"|^\s*"([^"]+)"\s*$', table, re.MULTILINE):
        if token.group(1):
            chips.add(token.group(1).encode())
        elif chips:
            branches.append((chips, token.group(2)))
            chips = set()
        else:
            default = token.group(2)
    return branches, default


class KorridCopy(unittest.TestCase):
    def test_picture_path_table_equals_korrids(self):
        branches, default = korrid_picture_paths(KORRID_VIEWER.read_text())
        self.assertGreaterEqual(len(branches), 1, "cannot read korrid's chip table any more")
        self.assertEqual(default, viewer.PICTURE_PATH_DEFAULT)
        self.assertEqual(
            [(set(chips), name) for chips, name in viewer.PICTURE_PATHS], branches
        )
        for chips, name in branches:
            for chip in chips:
                self.assertEqual(viewer.picture_path(b"vendor,board\0" + chip + b"\0"), name)
        self.assertEqual(viewer.picture_path(b"vendor,board\0vendor,chip\0"), default)
        self.assertEqual(viewer.picture_path(b""), default)

    def test_table_reader_sees_a_changed_korrid(self):
        source = KORRID_VIEWER.read_text()
        changed = source.replace('b"rockchip,rk3566"', 'b"rockchip,rk3568"')
        self.assertNotEqual(source, changed)
        self.assertNotEqual(korrid_picture_paths(source), korrid_picture_paths(changed))


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
        (session.root / "sunshine.crt").write_text(
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
                stored = json.loads((Path(built["facts"]["sender_fixture"]["run_folder"]) / "credentials.json").read_text())
            finally:
                sender.RENDER_NODES = previous
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
        previous = sender.RENDER_NODES
        with tempfile.TemporaryDirectory() as temporary:
            sender.RENDER_NODES = Path(temporary)
            session = sender.Session()
            try:
                with self.assertRaisesRegex(RuntimeError, "no render node"):
                    session.handle(
                        {"operation": "prepare", "sway": "/unused", "sunshine": "/unused",
                         "width": 640, "height": 480, "fps": 60}
                    )
            finally:
                sender.RENDER_NODES = previous
            self.assertIsNone(session.root)
            self.assertTrue(session.close()["pass"])


if __name__ == "__main__":
    unittest.main()
