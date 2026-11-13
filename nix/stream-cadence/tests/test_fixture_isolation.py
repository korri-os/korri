"""Native Unix socket evidence joins a viewer FD to its actual X server."""
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from fixture_namespace import unix_peers, x_connection, input_nodes_open  # noqa: E402


class SocketEvidence(unittest.TestCase):
    def test_real_connected_socket_reports_its_peer_inode_and_name(self):
        with tempfile.TemporaryDirectory() as temporary:
            address = str(Path(temporary) / 'X0')
            with socket.socket(socket.AF_UNIX) as server, socket.socket(socket.AF_UNIX) as client:
                server.bind(address)
                server.listen(1)
                client.connect(address)
                accepted, _ = server.accept()
                with accepted:
                    inode = lambda fd: int(os.readlink(f'/proc/self/fd/{fd}')[8:-1])
                    peers = unix_peers()
                    self.assertEqual(peers[inode(client.fileno())]['peer'], inode(accepted.fileno()))
                    self.assertEqual(peers[inode(accepted.fileno())]['name'], address)

    def test_private_xwayland_is_identified_by_its_live_compositor_socket(self):
        from fixture_namespace import fixture_xwayland
        with tempfile.TemporaryDirectory() as temporary:
            program = Path(temporary) / "Xwayland"
            shutil.copyfile(shutil.which("sleep"), program)
            program.chmod(0o755)
            compositor, server = socket.socketpair()
            with compositor, server:
                child = subprocess.Popen(["sleep", "60"], executable=str(program), pass_fds=(server.fileno(),))
                try:
                    server.close()
                    scope = Path(temporary) / "proc"
                    scope.mkdir()
                    (scope / str(child.pid)).symlink_to(f"/proc/{child.pid}")
                    _, servers = fixture_xwayland(os.getpid(), unix_peers(), scope)
                    self.assertIn(child.pid, servers)
                    # An arbitrary process with no end of this socket is not its compositor.
                    _, unrelated = fixture_xwayland(child.pid, unix_peers(), scope)
                    self.assertNotIn(child.pid, unrelated)
                finally:
                    child.terminate()
                    child.wait(timeout=5)

    def test_connection_must_have_a_private_xwayland_peer_and_no_other_x_server(self):
        sockets = {10: {'peer': 20, 'name': ''}, 20: {'peer': 10, 'name': '@/tmp/.X11-unix/X0'}}
        self.assertTrue(x_connection({10}, {20}, sockets)['pass'])
        self.assertFalse(x_connection({10}, {99}, sockets)['pass'])
        self.assertFalse(x_connection({10}, {20}, {})['pass'])
        sockets.update({11: {'peer': 21, 'name': ''}, 21: {'peer': 11, 'name': '/tmp/.X11-unix/X1'}})
        self.assertFalse(x_connection({10, 11}, {20}, sockets)['pass'])

    def test_input_evidence_includes_every_input_node_and_fails_closed_when_unreadable(self):
        with tempfile.TemporaryDirectory() as temporary:
            proc = Path(temporary)
            fd = proc / '1/fd'
            fd.mkdir(parents=True)
            (fd / '0').symlink_to('/dev/null')
            (fd / '1').symlink_to('/dev/input/event17')
            self.assertEqual(input_nodes_open(1, proc), ['/dev/input/event17'])
            self.assertIsNone(input_nodes_open(2, proc))


class RunEvidence(unittest.TestCase):
    def test_a_run_needs_a_proved_x_connection_and_no_input_node(self):
        from cli import isolation_record
        good = {'pass': True, 'input_nodes': [], 'x': {'connected': True}}
        startup = {'pass': False, 'input_nodes': [], 'x': {'connected': False}}
        bad = {'pass': False, 'input_nodes': [], 'x': {'connected': True}}
        opened = {**good, 'input_nodes': ['/dev/input/event17']}
        for rows, expected in (([], False), ([{}], False), ([startup], False), ([startup, good], True), ([good, startup], False), ([good, bad], False), ([good, opened], False)):
            with self.subTest(rows=rows):
                samples = [{'isolation': row} if row else {} for row in rows]
                self.assertEqual(isolation_record(samples)['pass'], expected)
