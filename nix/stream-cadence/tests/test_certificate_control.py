"""The sender fixture's certificate control: korrid provisions a client at Korri's patched Sunshine.

Korri's Sunshine (patch 0020) takes one root-made SOCK_SEQPACKET socket by
systemd's socket activation, and korrid's broker provisions Moonlight client
certificates through it (services/korrid/src/host/moonlight_certificate.rs).
The fixture makes that socket as the product's socket unit does, for a
korrid that runs as a given user (#434 D10).
"""

import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import sender  # noqa: E402
import test_patched_sunshine  # noqa: E402
import test_private_sender  # noqa: E402


class ControlSocket(unittest.TestCase):
    def test_the_socket_is_made_as_the_products_socket_unit_makes_it(self):
        with tempfile.TemporaryDirectory() as parent:
            control = sender.CertificateControl.make(Path(parent), os.getgid())
            self.addCleanup(control.close)
            folder = control.path.parent
            # ListenSequentialPacket, SocketMode=0660, SocketGroup, DirectoryMode=0751.
            self.assertEqual(stat.S_IMODE(folder.stat().st_mode), 0o751)
            made = control.path.stat()
            self.assertTrue(stat.S_ISSOCK(made.st_mode))
            self.assertEqual(stat.S_IMODE(made.st_mode), 0o660)
            self.assertEqual(made.st_gid, os.getgid())
            self.assertEqual(control.listener.type, socket.SOCK_SEQPACKET)
            with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as client:
                client.connect(str(control.path))
            control.close()
            self.assertFalse(folder.exists())

    def test_sunshine_and_korrid_are_told_the_same_socket(self):
        group = os.getgid()
        with tempfile.TemporaryDirectory() as parent:
            control = sender.CertificateControl.make(Path(parent), group)
            self.addCleanup(control.close)
            self.assertEqual(
                control.sunshine(korrid_uid=4243, korrid_gid=4242),
                {
                    "KORRI_CERTIFICATE_CONTROL_UID": "4243",
                    "KORRI_CERTIFICATE_CONTROL_GID": "4242",
                    "KORRI_CERTIFICATE_CONTROL_OWNER_GID": str(group),
                    "KORRI_CERTIFICATE_CONTROL_MODE": "0660",
                    "KORRI_CERTIFICATE_CONTROL_PATH": str(control.path),
                },
            )
            # The listener's peer is the process that listened: this agent.
            self.assertEqual(
                control.korrid(),
                {
                    "KORRID_STREAM_CERTIFICATE_CONTROL_SOCKET": str(control.path),
                    "KORRID_STREAM_CERTIFICATE_CONTROL_GID": str(group),
                    "KORRID_STREAM_CERTIFICATE_CONTROL_PEER_UID": str(os.geteuid()),
                    "KORRID_STREAM_CERTIFICATE_CONTROL_PEER_GID": str(os.getegid()),
                },
            )


class SocketActivation(unittest.TestCase):
    def test_sunshine_gets_the_socket_as_descriptor_3_with_its_own_pid(self):
        with tempfile.TemporaryDirectory() as parent:
            control = sender.CertificateControl.make(Path(parent), os.getgid())
            self.addCleanup(control.close)
            probe = (
                "import json, os, socket; s = socket.socket(fileno=3); "
                "print(json.dumps([os.environ['LISTEN_PID'] == str(os.getpid()), "
                "os.environ['LISTEN_FDS'], os.environ['LISTEN_FDNAMES'], "
                "s.type == socket.SOCK_SEQPACKET, s.getsockname()]))"
            )
            command = control.activate([sys.executable, "-c", probe])
            done = subprocess.run(
                command,
                pass_fds=(control.listener.fileno(),),
                capture_output=True,
                text=True,
                timeout=30,
                check=True,
            )
            self.assertEqual(
                json.loads(done.stdout),
                [True, "1", "korri-certificate-control", True, str(control.path)],
            )


class Refusals(test_private_sender.Machine):
    def test_stock_sunshine_has_no_certificate_control(self):
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "certificate control.*0020"):
            session.handle(
                {
                    "operation": "prepare",
                    "sway": str(self.peers / "sway"),
                    "sunshine": str(self.peers / "sunshine"),
                    "width": 640,
                    "height": 480,
                    "fps": 60.024,
                    "certificate_control": {"uid": 59034, "gid": 59034},
                }
            )
        self.assertIsNone(session.root)

    def test_a_patch_stack_without_patch_0020_has_no_certificate_control(self):
        provenance = test_patched_sunshine.PROVENANCE.read_text()
        without = "\n".join(
            line for line in provenance.splitlines() if "0020-add-korrid-certificate-control" not in line
        )
        package = test_patched_sunshine.Package(self, without + "\n")
        self.assertIsNotNone(sender.patch_stack(package.path / "bin" / "sunshine"))
        with self.assertRaisesRegex(RuntimeError, "certificate control.*0020"):
            sender.certificate_control_allowed(sender.patch_stack(package.path / "bin" / "sunshine"))
        stack = sender.patch_stack(
            test_patched_sunshine.Package(self, provenance).path / "bin" / "sunshine"
        )
        sender.certificate_control_allowed(stack)


if __name__ == "__main__":
    unittest.main()
