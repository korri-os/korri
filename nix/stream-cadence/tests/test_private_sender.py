"""The sender fixture beside a product: no root programs, no input device, no game."""

import os
from pathlib import Path
import pwd
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import heat  # noqa: E402
import sender  # noqa: E402
import viewer  # noqa: E402

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
        # No thermal zone and no device tree, whatever machine runs the tests.
        self.sensors = (heat.THERMAL, viewer.COMPATIBLE)
        heat.THERMAL, viewer.COMPATIBLE = root / "thermal", root / "compatible"

    def tearDown(self):
        sender.RENDER_NODES, sender.INPUT_DEVICES, sender.UINPUT = self.real
        heat.THERMAL, viewer.COMPATIBLE = self.sensors
        self.directory.cleanup()

    def zone(self, millidegrees: int | None) -> Path:
        """One thermal zone with the RG353M's trip points and this temperature."""
        zone = heat.THERMAL / "thermal_zone0"
        zone.mkdir(parents=True, exist_ok=True)
        (zone / "type").write_text("cpu-thermal\n")
        for n, (kind, value) in enumerate(
            [("passive", 70000), ("passive", 75000), ("critical", 95000)]
        ):
            (zone / f"trip_point_{n}_type").write_text(kind + "\n")
            (zone / f"trip_point_{n}_temp").write_text(f"{value}\n")
        fresh = heat.THERMAL / "fresh-temp"
        fresh.write_text("\n" if millidegrees is None else f"{millidegrees}\n")
        fresh.replace(zone / "temp")
        return zone

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


class SenderHeatGuard(Machine):
    def test_fixture_does_not_start_on_a_hot_host(self):
        self.zone(72222)
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "device is hot.*72.2.*70.0"):
            self.prepare(session)
        self.assertIsNone(session.root)
        self.assertFalse((self.peers / "arguments.log").exists())

    def test_fixture_does_not_start_when_the_guard_cannot_read(self):
        self.zone(None)
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "cannot read.*does not start"):
            self.prepare(session)
        self.assertIsNone(session.root)

    def test_running_fixture_is_stopped_near_the_critical_trip(self):
        self.zone(50000)
        session = self.session()
        facts = self.prepare(session)["facts"]
        self.assertTrue(facts["heat_guarded"])
        self.assertEqual(facts["thermal"][0]["limits"]["end_at"], 85000)
        session.check()
        sunshine, compositor = session.processes["sunshine"], session.processes["compositor"]
        self.zone(86000)
        # The guard's own thread stops the fixture; no request is needed for it.
        self.assertIsNotNone(sunshine.wait(timeout=5))
        self.assertIsNotNone(compositor.wait(timeout=5))
        with self.assertRaisesRegex(RuntimeError, "cpu-thermal reads 86.0.*ends at 85.0"):
            session.check()
        self.assertTrue(session.close()["pass"])

    def test_running_fixture_is_stopped_when_the_guard_goes_blind(self):
        self.zone(50000)
        session = self.session()
        self.prepare(session)
        sunshine = session.processes["sunshine"]
        self.zone(None)
        self.assertIsNotNone(sunshine.wait(timeout=5))
        with self.assertRaisesRegex(RuntimeError, "cannot read.*run ends"):
            session.check()

    def test_host_with_no_zone_and_no_hardware_picture_path_runs_unguarded(self):
        session = self.session()
        facts = self.prepare(session)["facts"]
        self.assertFalse(facts["heat_guarded"])
        self.assertEqual(facts["thermal"], [])
        session.check()
        self.assertTrue(session.close()["pass"])

    def test_hardware_path_device_with_no_zone_does_not_host_the_fixture(self):
        viewer.COMPATIBLE.write_bytes(b"vendor,board\0rockchip,rk3566\0")
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "no thermal zone with a critical trip point"):
            self.prepare(session)
        self.assertIsNone(session.root)


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
