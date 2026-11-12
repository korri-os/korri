"""The sender fixture beside a product: no root programs, no input device, no game."""

import os
from pathlib import Path
import pwd
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import sender  # noqa: E402

TESTS = Path(__file__).resolve().parent


class Machine(unittest.TestCase):
    """Stand-in programs and device nodes that the test owns."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name)
        self.peers, dri, self.inputs = root / "bin", root / "dri", root / "input"
        for path in (self.peers, dri, self.inputs):
            path.mkdir()
        (dri / "renderD1").touch()
        (self.inputs / "event0").touch()
        (self.peers / "hardware-nodes").write_text(str(dri / "renderD1"))
        for role in ("sway", "swaymsg", "sunshine"):
            program = self.peers / role
            program.write_text(
                f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "fixture-peer.py"} {self.peers} {role} "$@"\n'
            )
            program.chmod(0o755)
        self.uinput = root / "uinput"
        self.real = (sender.RENDER_NODES, sender.INPUT_DEVICES, sender.UINPUT)
        sender.RENDER_NODES, sender.INPUT_DEVICES, sender.UINPUT = dri, self.inputs, self.uinput

    def tearDown(self):
        sender.RENDER_NODES, sender.INPUT_DEVICES, sender.UINPUT = self.real
        self.directory.cleanup()

    def session(self) -> sender.Session:
        made = sender.Session()
        self.addCleanup(made.close)
        return made

    def prepare(self, session: sender.Session) -> dict:
        return session.handle(
            {
                "operation": "prepare",
                "sway": str(self.peers / "sway"),
                "sunshine": str(self.peers / "sunshine"),
                "width": 640,
                "height": 480,
                "fps": 60.024,
            }
        )


class NoInputDevice(Machine):
    def test_fixture_does_not_start_where_its_sunshine_can_open_uinput(self):
        self.uinput.touch()
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "can open .*uinput.*does not start"):
            self.prepare(session)
        # Nothing was started and no directory was made.
        self.assertIsNone(session.root)
        self.assertFalse((self.peers / "arguments.log").exists())

    def test_uinput_access_of_a_dropped_user_is_read_from_the_node(self):
        self.uinput.touch()
        node = self.uinput.stat()
        user = {"user": "viewer", "uid": node.st_uid + 1, "gid": node.st_gid + 1, "groups": []}
        self.uinput.chmod(0o660)
        self.assertFalse(sender.opens_uinput(user))
        # A supplementary group that owns the node opens it; so does a world-writable node.
        self.assertTrue(sender.opens_uinput(dict(user, groups=[node.st_gid])))
        self.assertTrue(sender.opens_uinput(dict(user, gid=node.st_gid)))
        self.assertTrue(sender.opens_uinput(dict(user, uid=node.st_uid)))
        self.uinput.chmod(0o666)
        self.assertTrue(sender.opens_uinput(user))
        self.uinput.unlink()
        self.assertFalse(sender.opens_uinput(user))
        self.assertFalse(sender.opens_uinput(None))

    def test_new_input_device_during_a_run_ends_it_and_fails_the_restore(self):
        session = self.session()
        facts = self.prepare(session)["facts"]
        self.assertEqual(facts["fixture_user"], pwd.getpwuid(os.geteuid()).pw_name)
        session.check()
        (self.inputs / "event17").touch()
        with self.assertRaisesRegex(RuntimeError, "New input devices appeared.*event17"):
            session.check()
        restore = session.close()
        self.assertEqual(restore["new_input_devices"], ["event17"])
        self.assertFalse(restore["pass"])
        self.assertTrue(restore["run_folder_removed"])

    def test_unchanged_input_devices_pass(self):
        session = self.session()
        self.prepare(session)
        restore = session.close()
        self.assertEqual(restore["new_input_devices"], [])
        self.assertTrue(restore["pass"], restore)

    def test_fixture_sunshine_takes_no_input(self):
        values = dict(
            line.split(" = ", 1)
            for line in sender.configuration(Path("/tmp/run"), 40123, "/dev/dri/renderD1").splitlines()
        )
        for key in ("mouse", "keyboard", "controller"):
            self.assertEqual(values[key], "disabled")


class NoRoot(Machine):
    def test_an_agent_that_is_not_root_keeps_its_user(self):
        if os.geteuid() == 0:
            self.skipTest("this test describes an ordinary user")
        self.assertIsNone(sender.korri_identity())
        session = self.session()
        self.assertIsNone(session.identity)
        self.prepare(session)
        for name in ("compositor", "sunshine"):
            status = Path(f"/proc/{session.processes[name].pid}/status").read_text()
            self.assertIn(f"Uid:\t{os.geteuid()}\t", status)


if __name__ == "__main__":
    unittest.main()
