import contextlib
import io
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cli import Remote, compare, parser, run_case


class InterruptionTests(unittest.TestCase):
    def test_close_does_not_mistake_interrupted_sample_for_restore(self):
        with tempfile.TemporaryDirectory() as temporary:
            remote = Remote(
                [sys.executable, str(Path(__file__).with_name("reply-peer.py"))],
                Path(temporary) / "peer.log",
            )

            def interrupt(_signum, _frame):
                raise InterruptedError("cancel pending sample")

            previous = signal.signal(signal.SIGALRM, interrupt)
            try:
                signal.setitimer(signal.ITIMER_REAL, 0.03)
                with self.assertRaises(InterruptedError):
                    remote.request("sample")
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)
                signal.signal(signal.SIGALRM, previous)
            restore = remote.close()
            self.assertTrue(restore["pass"])
            self.assertEqual(remote.process.returncode, 0)

    def test_signals_during_each_cleanup_step_finish_revocation_and_folder_removal(
        self,
    ):
        for operation in ["eof", "close"]:
            for step in ["source", "certificate", "folder"]:
                for signum in [signal.SIGINT, signal.SIGTERM]:
                    with self.subTest(operation=operation, step=step, signum=signum):
                        self.check_cleanup_signal(operation, step, signum)

    def check_cleanup_signal(self, operation, step, signum):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            grant = root / "grant"
            grant.write_text("public-certificate")
            received = []
            with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as server:
                server.bind(str(root / "control.sock"))
                server.listen(1)
                server.settimeout(2)

                def serve():
                    try:
                        connection, _ = server.accept()
                    except TimeoutError:
                        return
                    with connection:
                        received.append(json.loads(connection.recv(16384)))
                        grant.unlink()
                        connection.sendall(
                            json.dumps({"status": "ok", "changed": True}).encode()
                        )

                worker = threading.Thread(target=serve)
                worker.start()
                try:
                    result = subprocess.run(
                        [
                            sys.executable,
                            str(Path(__file__).with_name("cleanup-peer.py")),
                            str(root),
                            step,
                            str(signum),
                        ],
                        input=""
                        if operation == "eof"
                        else json.dumps({"request_id": 1, "operation": "close"}) + "\n",
                        text=True,
                        capture_output=True,
                        timeout=10,
                    )
                finally:
                    worker.join(timeout=3)
                source_pid = int((root / "source-pid").read_text())
                try:
                    os.kill(source_pid, 0)
                except ProcessLookupError:
                    source_alive = False
                else:
                    source_alive = True
                    os.kill(source_pid, signal.SIGKILL)
                self.assertFalse(source_alive, "cleanup left its source process alive")
                self.assertEqual(result.returncode, 128 + signum, result.stderr)
                self.assertEqual((root / "signalled").read_text(), str(signum))
                self.assertEqual(len(received), 1, result.stderr)
                self.assertEqual(received[0]["operation"], "revoke")
                self.assertFalse(grant.exists())
                self.assertFalse((root / "run-data").exists())

    def test_refused_run_records_imported_paths_in_discovered_facts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            remote = Remote(
                [sys.executable, str(Path(__file__).with_name("reply-peer.py"))],
                root / "peer.log",
            )
            args = parser().parse_args(["run", "--device", "example.test"])
            args.imported_store_paths = [
                "/nix/store/transport-python3-1.0",
                "/nix/store/transport-stream-cadence-motion",
                "/nix/store/transport-source",
            ]
            with contextlib.redirect_stdout(io.StringIO()):
                result = run_case(
                    args, remote, "unused-motion", root, "public-certificate", None, 0
                )
            saved = json.loads((root / "result.json").read_text())
            self.assertEqual(
                saved["discovered"]["imported_store_paths"], args.imported_store_paths
            )
            self.assertEqual(result, saved)
            self.assertFalse(saved["discovered"]["valid"])
            self.assertTrue(saved["discovered"]["restore"]["pass"])

    def test_empty_results_do_not_pass_an_explicit_limit(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "empty.json"
            path.write_text(json.dumps([]))
            args = parser().parse_args(
                ["compare", str(path), str(path), "--max-torn-frames", "0"]
            )
            self.assertEqual(compare(args), 1)
