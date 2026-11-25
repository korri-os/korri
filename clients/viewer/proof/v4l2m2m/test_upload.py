"""Exercise the existing RPC upload transport against a real Unix HTTP server."""

import http.server
import importlib.util
import json
from pathlib import Path
import socketserver
import tempfile
import threading
import unittest

spec = importlib.util.spec_from_file_location(
    "proof_upload", Path(__file__).with_name("upload.py")
)
upload = importlib.util.module_from_spec(spec)
spec.loader.exec_module(upload)


class UploadTransport(unittest.TestCase):
    def exchange(self, response, status=200):
        observed = []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                observed.append(
                    (
                        self.path,
                        json.loads(
                            self.rfile.read(int(self.headers["Content-Length"]))
                        ),
                    )
                )
                body = json.dumps(response).encode()
                self.send_response(status)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *args):
                pass

        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "rpc.sock"
            with socketserver.UnixStreamServer(str(path), Handler) as server:
                thread = threading.Thread(target=server.handle_request)
                thread.start()
                try:
                    result = upload.request(path, "begin", {"sha256": "ab", "size": 1})
                finally:
                    thread.join(timeout=5)
            self.assertEqual(
                observed,
                [
                    (
                        "/rpc",
                        {
                            "_tag": "system.plugins.archive.begin",
                            "payload": {"sha256": "ab", "size": 1},
                        },
                    )
                ],
            )
            return result

    def test_success_preserves_the_existing_response_payload(self):
        self.assertEqual(
            self.exchange(
                {
                    "_tag": "system.plugins.archive.begin",
                    "outcome": {
                        "_tag": "Ok",
                        "payload": {"uploadId": "existing-producer-id"},
                    },
                }
            ),
            {"uploadId": "existing-producer-id"},
        )

    def test_refusal_stops_without_retry(self):
        with self.assertRaisesRegex(RuntimeError, "PluginUploadBusy"):
            self.exchange(
                {
                    "_tag": "system.plugins.archive.begin",
                    "outcome": {
                        "_tag": "Err",
                        "payload": {"code": "PluginUploadBusy", "message": "busy"},
                    },
                }
            )

    def test_a_different_response_tag_is_not_accepted(self):
        with self.assertRaises(RuntimeError):
            self.exchange(
                {
                    "_tag": "system.plugins.archive.finish",
                    "outcome": {
                        "_tag": "Ok",
                        "payload": {},
                    },
                }
            )

    def test_http_failure_is_not_an_rpc_success(self):
        with self.assertRaisesRegex(RuntimeError, "HTTP 403"):
            self.exchange({}, status=403)


if __name__ == "__main__":
    unittest.main()
