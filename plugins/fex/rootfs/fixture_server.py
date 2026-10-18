"""A local HTTP fixture server for prepare-program tests. No internet."""
import http.server
import threading
import time


class QuietHTTPServer(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def handle_error(self, request, client_address):
        pass  # Clients abort transfers on purpose in these tests.


class FixtureServer:
    """Serve fixture bytes over local HTTP.

    Knobs: `ranges` honours Range requests, `truncate` drops the next full
    response after that many bytes, and `delay` holds each response.
    """

    def __init__(self, files, ranges=True, truncate=None, delay=0):
        self.files = files
        self.ranges = ranges
        self.truncate = truncate
        self.delay = delay
        self.requests = []
        self.httpd = QuietHTTPServer(("127.0.0.1", 0), self.handler())
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def url(self, name):
        return f"http://127.0.0.1:{self.httpd.server_port}/{name}"

    def stop(self):
        self.httpd.shutdown()
        self.httpd.server_close()

    def handler(self):
        server = self

        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *_):
                pass

            def do_GET(self):
                requested = self.headers.get("Range")
                server.requests.append((self.path.lstrip("/"), requested))
                time.sleep(server.delay)
                data = server.files.get(self.path.lstrip("/"))
                if data is None:
                    self.send_error(404)
                    return
                start = 0
                if requested and server.ranges:
                    start = int(requested.removeprefix("bytes=").split("-")[0])
                    if start >= len(data):
                        self.send_response(416)
                        self.send_header("Content-Range", f"bytes */{len(data)}")
                        self.send_header("Content-Length", "0")
                        self.end_headers()
                        return
                    self.send_response(206)
                    self.send_header("Content-Range", f"bytes {start}-{len(data) - 1}/{len(data)}")
                else:
                    self.send_response(200)
                body = data[start:]
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                if server.truncate is not None and start == 0:
                    cut, server.truncate = server.truncate, None
                    self.wfile.write(body[:cut])
                    self.wfile.flush()
                    self.close_connection = True
                    return
                self.wfile.write(body)

        return Handler
