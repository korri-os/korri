"""Real WPE process, loopback HTTP origins, credentials and sd_notify sockets."""

import contextlib
import http.server
import os
from pathlib import Path
import queue
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest

LAUNCHER = sys.argv.pop(1)


class Server(http.server.ThreadingHTTPServer):
    def __init__(self):
        self.pages = {}
        self.requests = queue.Queue()
        self.observed = []
        super().__init__(("127.0.0.1", 0), Handler)
        threading.Thread(target=self.serve_forever, daemon=True).start()

    @property
    def origin(self):
        return f"http://127.0.0.1:{self.server_port}"

    def wait(self, prefix, timeout=10):
        for path in self.observed:
            if path.startswith(prefix):
                return path
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            path = self.requests.get(timeout=max(0.01, until - time.monotonic()))
            self.observed.append(path)
            if path.startswith(prefix):
                return path
        raise AssertionError(f"missing request {prefix}")


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.server.requests.put(self.path)
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.end_headers()
        self.wfile.write(self.server.pages.get(self.path, "").encode())

    def log_message(self, *_args):
        pass


def page(script):
    return f"<!doctype html><script>window.onerror=(m)=>fetch('/error?'+encodeURIComponent(m));{script}</script>"


CONSUME = "KorriRpc.korridPort(); KorriRpc.korridCapability();"


class Acceptance(unittest.TestCase):
    def setUp(self):
        self.trusted, self.other = Server(), Server()
        self.addCleanup(self.trusted.server_close)
        self.addCleanup(self.other.server_close)
        self.addCleanup(self.trusted.shutdown)
        self.addCleanup(self.other.shutdown)

    @contextlib.contextmanager
    def launch(self, script="", *, abstract=False, env_override=None, setup=None):
        self.trusted.pages["/"] = page(script)
        with tempfile.TemporaryDirectory(prefix="korri-wpe-test-") as tmp:
            root = Path(tmp)
            credential = root / "KORRID_RPC_CAPABILITY"
            credential.write_text("acceptance-test-capability\n")
            credential.chmod(0o600)
            if setup:
                setup(credential)
            notify = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
            path = f"{tmp}/notify"
            address = (
                f"\0korri-wpe-{os.getpid()}-{time.monotonic_ns()}" if abstract else path
            )
            notify.bind(address)
            notify.settimeout(10)
            env = {
                "HOME": tmp,
                "XDG_RUNTIME_DIR": tmp,
                "WPE_PLATFORM": "headless",
                "CREDENTIALS_DIRECTORY": tmp,
                "KORRID_ADDRESS": "127.0.0.1:7887",
                "KORRID_PORTAL_ORIGIN": self.trusted.origin,
                "KORRI_WEB_SURFACE_URL": self.trusted.origin + "/",
                "NOTIFY_SOCKET": "@" + address[1:] if abstract else address,
                # These must not affect WebKit or reach any subprocess.
                "KORRID_RPC_CAPABILITY": "environment-must-not-be-used",
                "WEBKIT_INSPECTOR_SERVER": "127.0.0.1:9222",
                "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS": "1",
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=/must-not-inherit",
            }
            for name in (
                "GBM_BACKENDS_PATH",
                "LIBGL_DRIVERS_PATH",
                "__EGL_VENDOR_LIBRARY_FILENAMES",
            ):
                if name in os.environ:
                    env[name] = os.environ[name]
            env.update(env_override or {})
            process = subprocess.Popen(
                [LAUNCHER],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                start_new_session=True,
            )
            try:
                yield process, notify
            finally:
                # Production uses KillMode=control-group. The harness owns this
                # session and cleans all engine descendants even on assertion failure.
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=10)
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                out, err = process.communicate()
                self.assertNotIn(b"acceptance-test-capability", out + err)
                notify.close()

    def test_trusted_binding_permissions_navigation_and_popups(self):
        self.trusted.pages["/child"] = page("""
fetch('/child-result?binding=' + typeof window.KorriRpc);
window.webkit.messageHandlers.korriReady.postMessage('wrong-child-nonce');
""")
        script = f"""
const d=Object.getOwnPropertyDescriptor(window,'KorriRpc');
if (d.enumerable || d.writable || d.configurable || !Object.isFrozen(KorriRpc)) throw Error('descriptor');
if (KorriRpc.korridPort()!==7887 || KorriRpc.korridCapability()!=='acceptance-test-capability') throw Error('binding');
console.log(KorriRpc.korridCapability());
fetch('/binding-ok');
navigator.geolocation.getCurrentPosition(()=>fetch('/geo-granted'), e=>fetch('/geo-error?code='+e.code));
Notification.requestPermission().then(result=>fetch('/notification?result='+result));
const child=document.createElement('iframe');child.src='/child';document.documentElement.append(child);
const other=document.createElement('iframe');other.src='{self.other.origin}/frame';document.documentElement.append(other);
window.open('/popup');
setTimeout(()=>location.href='{self.other.origin}/escape',500);
setTimeout(()=>fetch('/retained'),1500);
"""
        with self.launch(script) as (process, notify):
            self.assertEqual(notify.recv(100), b"READY=1")
            self.trusted.wait("/binding-ok")
            self.trusted.wait("/geo-error?code=1")
            self.trusted.wait("/notification?result=denied")
            self.trusted.wait("/child-result?binding=undefined")
            self.trusted.wait("/retained")
            time.sleep(0.5)
            self.assertTrue(
                self.other.requests.empty(), "off-origin navigation reached HTTP server"
            )
            self.assertNotIn(
                "/popup", self.trusted.observed + list(self.trusted.requests.queue)
            )
            self.assertIsNone(process.poll())
            # The launcher has already initialized WPE's child processes.
            child_environments = []
            for entry in Path("/proc").iterdir():
                if not entry.name.isdigit() or int(entry.name) == process.pid:
                    continue
                try:
                    if int((entry / "stat").read_text().split()[5]) == process.pid:
                        child_environments.append((entry / "environ").read_bytes())
                except (FileNotFoundError, PermissionError, ProcessLookupError):
                    continue
            self.assertTrue(child_environments)
            for environment in child_environments:
                for forbidden in (
                    b"CREDENTIALS_DIRECTORY=",
                    b"KORRID_RPC_CAPABILITY=",
                    b"NOTIFY_SOCKET=",
                    b"WEBKIT_INSPECTOR_SERVER=",
                    b"WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=",
                    b"DBUS_SESSION_BUS_ADDRESS=",
                ):
                    self.assertNotIn(forbidden, environment)

    def test_abstract_notify(self):
        with self.launch("fetch('/started');" + CONSUME, abstract=True) as (
            process,
            notify,
        ):
            self.assertEqual(notify.recv(100), b"READY=1")
            self.assertIsNone(process.poll())

    def assert_deadline(self, script):
        start = time.monotonic()
        with self.launch(script) as (process, notify):
            notify.settimeout(0.1)
            self.assertEqual(process.wait(timeout=20), 1)
            elapsed = time.monotonic() - start
            self.assertGreaterEqual(elapsed, 14)
            self.assertLess(elapsed, 19)
            with self.assertRaises(socket.timeout):
                notify.recv(100)

    def test_ignored_binding_deadline(self):
        self.assert_deadline("fetch('/old-fixture')")

    def test_partial_binding_deadline(self):
        self.assert_deadline("KorriRpc.korridPort()")

    def test_wrong_nonce_deadline(self):
        self.assert_deadline(
            "window.webkit.messageHandlers.korriReady.postMessage('wrong-nonce')"
        )

    def test_child_cannot_report_ready(self):
        self.trusted.pages["/child"] = page("""
if(window.KorriRpc) { KorriRpc.korridPort(); KorriRpc.korridCapability(); }
window.webkit.messageHandlers.korriReady.postMessage('wrong-child-nonce');
""")
        self.assert_deadline("""
const frame=document.createElement('iframe');frame.src='/child';document.documentElement.append(frame);
""")

    def test_untrusted_initial_origin_is_rejected(self):
        with self.launch(
            CONSUME, env_override={"KORRI_WEB_SURFACE_URL": self.other.origin + "/"}
        ) as (process, _notify):
            self.assertEqual(process.wait(timeout=3), 1)
            self.assertTrue(self.other.requests.empty())

    def test_invalid_environment(self):
        cases = (
            {"KORRID_ADDRESS": "127.0.0.1:0"},
            {"KORRID_ADDRESS": "localhost:7887"},
            {"KORRID_PORTAL_ORIGIN": self.trusted.origin + "/"},
            {"KORRI_WEB_SURFACE_URL": self.trusted.origin + ".evil/"},
            {"KORRI_WEB_SURFACE_URL": "javascript:alert(1)"},
            {"HOME": "relative"},
            {"XDG_RUNTIME_DIR": "relative"},
            {"WPE_PLATFORM": "../../untrusted-module"},
            {"NOTIFY_SOCKET": "relative"},
        )
        for override in cases:
            with (
                self.subTest(override=override),
                self.launch(CONSUME, env_override=override) as (process, _notify),
            ):
                self.assertEqual(process.wait(timeout=3), 1)

    def test_notification_failure_fails_startup(self):
        with self.launch(
            CONSUME, env_override={"NOTIFY_SOCKET": "/does-not-exist/notify"}
        ) as (process, _notify):
            self.assertEqual(process.wait(timeout=10), 1)

    def test_invalid_credentials(self):
        for setup in (
            lambda path: path.chmod(0o640),
            lambda path: path.write_text("x" * 4097),
            lambda path: path.write_text("token\n\n"),
            lambda path: path.write_bytes(b"token\x00"),
            lambda path: (
                path.rename(path.with_suffix(".real")),
                path.symlink_to(path.with_suffix(".real")),
            ),
            lambda path: (path.unlink(), path.mkdir()),
        ):
            with (
                self.subTest(setup=setup),
                self.launch(CONSUME, setup=setup) as (process, _notify),
            ):
                self.assertEqual(process.wait(timeout=3), 1)


if __name__ == "__main__":
    unittest.main()
