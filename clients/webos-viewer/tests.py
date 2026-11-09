"""Real CLI/filesystem and loopback TLS tests. Never contact a real host."""

import os
import socket
import ssl
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path

BINARY = os.environ["VIEWER_BINARY"]


def run(*args):
    return subprocess.run(
        [BINARY, *map(str, args)], capture_output=True, timeout=30, check=False
    )


class ViewerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.identity = self.root / "identity"

    def tearDown(self):
        self.temp.cleanup()

    def bootstrap(self, directory=None):
        result = run("identity", directory or self.identity)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(result.stdout.startswith(b"-----BEGIN CERTIFICATE-----\n"))
        self.assertNotIn(b"PRIVATE KEY", result.stdout + result.stderr)
        return result.stdout

    def test_stock_identity_is_persistent_and_tightened(self):
        certificate = self.bootstrap()
        self.assertTrue(
            (self.identity / "key.pem")
            .read_bytes()
            .startswith(b"-----BEGIN PRIVATE KEY-----\n")
        )
        self.assertEqual(
            {p.name for p in self.identity.iterdir()},
            {"client.pem", "key.pem", "uniqueid.dat"},
        )
        for path in [self.identity, *self.identity.iterdir()]:
            path.chmod(0o777)
        self.assertEqual(self.bootstrap(), certificate)
        self.assertEqual(self.identity.stat().st_mode & 0o7777, 0o700)
        for path in self.identity.iterdir():
            self.assertEqual(path.stat().st_mode & 0o7777, 0o600)
        self.assertRegex(
            (self.identity / "uniqueid.dat").read_text(), r"^[0-9A-F]{16}$"
        )
        subprocess.run(
            [
                "openssl",
                "x509",
                "-in",
                str(self.identity / "client.pem"),
                "-noout",
                "-checkend",
                "86400",
            ],
            check=True,
            capture_output=True,
        )

    def test_refuse_partial_corrupt_symlink_hardlink_and_foreign_identity(self):
        self.bootstrap()
        key = self.identity / "key.pem"
        original = key.read_bytes()
        key.write_bytes(b"not a key")
        self.assertNotEqual(run("identity", self.identity).returncode, 0)
        self.assertEqual(key.read_bytes(), b"not a key")
        key.write_bytes(original)
        key.rename(self.root / "key")
        self.assertNotEqual(run("identity", self.identity).returncode, 0)
        key.symlink_to(self.root / "key")
        self.assertNotEqual(run("identity", self.identity).returncode, 0)
        key.unlink()
        os.link(self.root / "key", key)
        self.assertNotEqual(run("identity", self.identity).returncode, 0)
        key.unlink()
        key.write_bytes(original)
        other = self.root / "other"
        self.bootstrap(other)
        key.write_bytes((other / "key.pem").read_bytes())
        self.assertNotEqual(run("identity", self.identity).returncode, 0)
        if os.geteuid() == 0:
            os.chown(key, 12345, -1)
            self.assertNotEqual(run("identity", self.identity).returncode, 0)

    def test_refuse_symlink_ancestor_and_unknown_entries_before_chmod(self):
        self.bootstrap()
        alias = self.root / "alias"
        alias.symlink_to(self.identity, target_is_directory=True)
        self.assertNotEqual(run("identity", alias).returncode, 0)
        link = self.identity / "unexpected"
        link.symlink_to(self.root)
        self.identity.chmod(0o777)
        self.assertNotEqual(run("identity", self.identity).returncode, 0)
        self.assertEqual(self.identity.stat().st_mode & 0o777, 0o777)

    def request(
        self,
        response,
        wrong_pin=False,
        action="info",
        require_client=True,
        extra=(),
        server_change=None,
    ):
        self.bootstrap()
        server = self.root / "server"
        self.bootstrap(server)
        original_pin = self.root / "server-pin.pem"
        original_pin.write_bytes((server / "client.pem").read_bytes())
        if server_change:
            subprocess.run(
                [
                    "openssl",
                    "x509",
                    "-in",
                    str(original_pin),
                    "-signkey",
                    str(server / "key.pem"),
                    "-not_before",
                    "20200101000000Z",
                    "-not_after",
                    "20210101000000Z"
                    if server_change == "expired"
                    else "20300101000000Z",
                    "-set_serial",
                    "42",
                    "-out",
                    str(server / "client.pem"),
                ],
                check=True,
                capture_output=True,
            )
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(server / "client.pem", server / "key.pem")
        if require_client:
            context.verify_mode = ssl.CERT_REQUIRED
            context.load_verify_locations(self.identity / "client.pem")
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(15)
        port = listener.getsockname()[1]
        requests = []
        errors = []

        def serve():
            try:
                with listener:
                    for reply in response if isinstance(response, list) else [response]:
                        connection, _ = listener.accept()
                        with context.wrap_socket(connection, server_side=True) as tls:
                            request = b""
                            while b"\r\n\r\n" not in request:
                                block = tls.recv(8192)
                                if not block:
                                    return
                                request += block
                            requests.append(request)
                            tls.sendall(reply)
            except (ssl.SSLError, BrokenPipeError, ConnectionResetError):
                pass  # expected for pin/response rejection
            except OSError as error:
                errors.append(error)

        thread = threading.Thread(target=serve)
        thread.start()
        pin = self.identity / "client.pem" if wrong_pin else original_pin
        if server_change == "expired":
            pin = server / "client.pem"
        result = run(action, self.identity, "127.0.0.1", port, pin, *extra)
        thread.join(20)
        self.assertFalse(thread.is_alive())
        self.assertFalse(errors, errors)
        return result, requests

    @staticmethod
    def serverinfo(game=0):
        return (
            b'<root status_code="200"><uniqueid>test-uuid</uniqueid>'
            b"<appversion>7.1.431.0</appversion><PairStatus>1</PairStatus>"
            b"<currentgame>" + str(game).encode() + b"</currentgame>"
            b"<state>SUNSHINE_SERVER_FREE</state></root>"
        )

    def test_authenticated_applist_resolves_one_transport_without_launch(self):
        apps = b'<root status_code="200"><App><ID>123</ID><AppTitle>Korri Stream</AppTitle></App></root>'
        result, requests = self.request(
            [self.response(self.serverinfo()), self.response(apps)], action="resolve"
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b"123\n")
        self.assertEqual(len(requests), 2)
        self.assertTrue(requests[1].startswith(b"GET /applist?"))
        self.assertNotIn(b"/cancel", b"".join(requests))

    def test_launch_uses_authenticated_session_material_and_bounds_url(self):
        from urllib.parse import parse_qs, urlsplit

        session = b'<root status_code="200"><gamesession>1</gamesession><sessionUrl0>rtspenc://127.0.0.1:48010</sessionUrl0></root>'
        result, requests = self.request(
            [self.response(self.serverinfo()), self.response(session)],
            action="launch-check",
            extra=(123,),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(requests), 2)
        query = urlsplit(requests[1].split(b" ")[1].decode())
        self.assertEqual(query.path, "/launch")
        parameters = parse_qs(query.query)
        self.assertEqual(parameters["appid"], ["123"])
        self.assertEqual(parameters["mode"], ["1920x1080x60"])
        self.assertRegex(parameters["rikey"][0], r"^[0-9A-F]{32}$")
        self.assertLessEqual(int(parameters["rikeyid"][0]), 2**32 - 1)
        self.assertNotIn(parameters["rikey"][0].encode(), result.stdout + result.stderr)
        self.assertIn("corever", parameters)
        self.assertRegex(
            parameters["uuid"][0],
            r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
        )

    def test_resume_uses_same_authenticated_transport(self):
        session = b'<root status_code="200"><resume>1</resume><sessionUrl0>rtspenc://127.0.0.1:48010</sessionUrl0></root>'
        result, requests = self.request(
            [self.response(self.serverinfo(123)), self.response(session)],
            action="resume-check",
            extra=(123,),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(requests[1].startswith(b"GET /resume?uniqueid="))

    def test_occupied_transport_refuses_launch_without_cancel(self):
        result, requests = self.request(
            self.response(self.serverinfo(42)), action="launch-check", extra=(123,)
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(requests), 1)

    def test_directory_lock_prevents_concurrent_identity_access(self):
        import fcntl

        self.bootstrap()
        directory = os.open(self.identity, os.O_RDONLY)
        try:
            fcntl.flock(directory, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.assertNotEqual(run("identity", self.identity).returncode, 0)
        finally:
            os.close(directory)

    @staticmethod
    def response(body):
        return (
            b"HTTP/1.1 200 OK\r\nContent-Length: "
            + str(len(body)).encode()
            + b"\r\nConnection: close\r\n\r\n"
            + body
        )

    def test_verified_tls_requires_exact_pin_and_client_certificate(self):
        body = (
            b'<root status_code="200"><uniqueid>test-uuid</uniqueid>'
            b"<appversion>7.1.431.0</appversion><PairStatus>1</PairStatus>"
            b"<currentgame>0</currentgame><state>SUNSHINE_SERVER_FREE</state></root>"
        )
        result, requests = self.request(self.response(body))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(requests), 1)
        self.assertTrue(requests[0].startswith(b"GET /serverinfo?uniqueid="))
        result, requests = self.request(self.response(body), wrong_pin=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(
            requests, []
        )  # no HTTP, no session material before verification

    def test_same_key_different_certificate_and_expired_pin_are_rejected(self):
        for change in ("reissued", "expired"):
            result, requests = self.request(
                self.response(self.serverinfo()), server_change=change
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(requests, [])

    def test_foreign_directory_is_rejected_without_changes(self):
        if os.geteuid() != 0:
            self.assertNotEqual(run("identity", "/etc").returncode, 0)

    def test_protocol_and_response_bounds_fail_closed(self):
        for body in [
            b'<root status_code="401"/>',
            b'<!DOCTYPE root [<!ENTITY x "unsafe">]><root status_code="200">&x;</root>',
            b'<root status_code="200"><appversion>7</appversion></root>',
            b'<root status_code="200"><uniqueid>a</uniqueid><uniqueid>b</uniqueid></root>',
        ]:
            result, _ = self.request(self.response(body))
            self.assertNotEqual(result.returncode, 0, body)
        result, _ = self.request(b"HTTP/1.1 200 OK\r\nContent-Length: 9999999\r\n\r\n")
        self.assertNotEqual(result.returncode, 0)
        for response in (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nx",
            b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n" + b"x" * 140000,
        ):
            result, _ = self.request(response)
            self.assertNotEqual(result.returncode, 0)
        result, _ = self.request(
            b"HTTP/1.1 302 Found\r\nLocation: https://example.org/\r\n\r\n"
        )
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
