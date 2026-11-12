"""Native Unix socket evidence joins a viewer FD to its actual X server."""
import os
from pathlib import Path
import socket
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
