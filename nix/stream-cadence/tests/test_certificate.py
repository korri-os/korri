import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]


class CertificateSocketTests(unittest.TestCase):
    def test_native_seqpacket_request_and_peer_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "control.sock"
            server = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
            server.bind(str(path))
            server.listen(1)
            received = []

            def serve():
                connection, _ = server.accept()
                with connection:
                    received.append(json.loads(connection.recv(16384)))
                    connection.sendall(
                        json.dumps(
                            {
                                "status": "ok",
                                "changed": True,
                                "serverCertificate": "public-certificate",
                            }
                        ).encode()
                    )

            worker = threading.Thread(target=serve)
            worker.start()
            control = {
                "path": str(path),
                "peer_uid": os.geteuid(),
                "peer_gid": os.getegid(),
            }
            request = {
                "operation": "revoke",
                "hostUuid": "test-host",
                "certificate": "public-client-certificate",
            }
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "device.py"),
                    "certificate",
                    json.dumps(control),
                ],
                input=json.dumps(request),
                text=True,
                capture_output=True,
                timeout=10,
            )
            worker.join(timeout=10)
            server.close()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(received, [request])
            self.assertTrue(json.loads(result.stdout)["changed"])

    def test_wrong_peer_is_refused_without_sending_certificate(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "control.sock"
            with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as server:
                server.bind(str(path))
                server.listen(1)
                control = {
                    "path": str(path),
                    "peer_uid": os.geteuid() + 1,
                    "peer_gid": os.getegid(),
                }
                result = subprocess.run(
                    [
                        sys.executable,
                        str(ROOT / "device.py"),
                        "certificate",
                        json.dumps(control),
                    ],
                    input="private request must not be sent",
                    text=True,
                    capture_output=True,
                    timeout=10,
                )
                connection, _ = server.accept()
                with connection:
                    self.assertEqual(connection.recv(16384), b"")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("peer differs", result.stderr)
                self.assertNotIn("private request", result.stderr)
