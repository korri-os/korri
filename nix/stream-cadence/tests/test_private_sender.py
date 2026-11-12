"""The sender fixture beside a product: no root programs, no input device, no game."""

import os
from pathlib import Path
import pwd
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import heat  # noqa: E402
import machine  # noqa: E402
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
        self.addCleanup(setattr, sender, "UHID", sender.UHID)
        sender.UHID = root / "uhid"
        # No thermal zone and no device tree, whatever machine runs the tests.
        self.sensors = (heat.THERMAL, viewer.COMPATIBLE)
        heat.THERMAL, viewer.COMPATIBLE = root / "thermal", root / "compatible"
        self.supplies = machine.power_supplies(self, root)

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

    def test_access_to_a_device_node_is_tried_as_the_fixtures_user(self):
        if os.geteuid() == 0:
            self.skipTest("root opens every node; this test describes an ordinary user")
        session = self.session()
        # Mode bits are not asked: the node is opened, as ACLs and all else decide.
        self.uinput.touch()
        self.uinput.chmod(0o600)
        self.assertTrue(session.opens(self.uinput, os.O_WRONLY))
        self.uinput.chmod(0o400)
        self.assertFalse(session.opens(self.uinput, os.O_WRONLY))
        self.assertTrue(session.opens(self.uinput, os.O_RDONLY))
        self.uinput.chmod(0o000)
        self.assertFalse(session.opens(self.uinput, os.O_RDONLY))
        self.uinput.unlink()
        self.assertFalse(session.opens(self.uinput, os.O_WRONLY))

    def test_render_node_that_the_fixtures_user_cannot_open_is_not_offered(self):
        if os.geteuid() == 0:
            self.skipTest("root opens every node; this test describes an ordinary user")
        closed = sender.RENDER_NODES / "renderD0"
        closed.touch()
        closed.chmod(0o000)
        session = self.session()
        self.assertEqual(session.render_nodes(), [str(sender.RENDER_NODES / "renderD1")])

    def test_new_input_device_during_a_run_is_recorded_and_is_no_fault(self):
        # The fixture's user cannot open uinput, so the entry is not the fixture's:
        # a game's controller, or one that a person plugged in.
        session = self.session()
        facts = self.prepare(session)["facts"]
        self.assertEqual(facts["fixture_user"], pwd.getpwuid(os.geteuid()).pw_name)
        session.check()
        (self.inputs / "event17").touch()
        session.check()
        restore = session.close()
        self.assertEqual(restore["new_input_devices"], ["event17"])
        self.assertEqual(restore["gone_input_devices"], [])
        self.assertTrue(restore["pass"], restore)
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
        with self.assertRaisesRegex(RuntimeError, "hardware picture path.*does not start"):
            self.prepare(session)
        self.assertIsNone(session.root)


class NotOnABoard(Machine):
    def place(self) -> str:
        return self.directory.name

    def test_sender_fixture_never_starts_on_the_r36t_max(self):
        # Its chip is not in korrid's table, its kernel shows no power supply, and it is cool.
        machine.device_tree(self.place(), b"aislpc,r36t-max\0rockchip,rk3326\0")
        self.zone(50000)
        session = self.session()
        facts = session.handle({"operation": "discover"})
        self.assertFalse(facts["hardware_picture_path"])
        self.assertEqual(list(self.supplies.glob("*")), [])
        self.assertEqual(facts["board"], ["aislpc,r36t-max", "rockchip,rk3326"])
        with self.assertRaisesRegex(
            RuntimeError,
            "board with a device tree \\(aislpc,r36t-max, rockchip,rk3326\\).*runs only its installed viewer.*sender fixture does not start",
        ):
            self.prepare(session)
        self.assertIsNone(session.root)
        self.assertFalse((self.peers / "arguments.log").exists())

    def test_sender_fixture_starts_on_aka_whose_device_tree_names_no_board(self):
        # aka is x86_64: /proc/device-tree is there with `#address-cells`, `name`
        # and `#size-cells`, and no `compatible`.
        root = machine.device_tree(self.place(), None)
        self.assertEqual(sorted(entry.name for entry in root.iterdir()), ["#address-cells", "#size-cells", "name"])
        session = self.session()
        self.prepare(session)
        self.assertEqual(session.handle({"operation": "discover"})["board"], [])
        self.assertTrue(session.close()["pass"])

    def test_a_board_that_cannot_be_read_is_refused_and_the_refusal_says_so(self):
        root = machine.device_tree(self.place(), None)
        (root / "compatible").mkdir()
        session = self.session()
        self.assertIsNone(session.handle({"operation": "discover"})["board"])
        with self.assertRaisesRegex(RuntimeError, "board of this device could not be read.*sender fixture does not start"):
            self.prepare(session)
        self.assertIsNone(session.root)
        self.assertFalse((self.peers / "arguments.log").exists())

    def test_a_laptop_has_no_device_tree_and_hosts_the_fixture(self):
        machine.supply(self.supplies, "AC", "Mains")
        machine.supply(self.supplies, "BAT0", "Battery")
        session = self.session()
        self.assertEqual(session.handle({"operation": "discover"})["board"], [])
        self.prepare(session)
        self.assertTrue(session.close()["pass"])

    def test_a_hot_laptop_is_still_refused_by_the_heat_guard(self):
        machine.supply(self.supplies, "BAT0", "Battery")
        self.zone(72000)
        with self.assertRaisesRegex(RuntimeError, "The device is hot"):
            self.prepare(self.session())
        self.assertFalse((self.peers / "arguments.log").exists())


class NotOnAHandheld(Machine):
    def test_sender_fixture_never_starts_on_a_chip_with_a_hardware_picture_path(self):
        # Cool, with every trip point readable: the heat rule alone would let it start.
        self.zone(40000)
        viewer.COMPATIBLE.write_bytes(b"vendor,board\0rockchip,rk3566\0")
        session = self.session()
        self.assertTrue(session.handle({"operation": "discover"})["hardware_picture_path"])
        with self.assertRaisesRegex(RuntimeError, "hardware picture path.*sender fixture does not start"):
            self.prepare(session)
        self.assertIsNone(session.root)
        self.assertFalse((self.peers / "arguments.log").exists())


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

    def test_a_root_agent_takes_the_compositors_user_and_only_the_render_groups(self):
        # The rule that a root agent applies, on the facts of aka's compositor.
        compositor = {"user": "korri", "uid": 56774, "gid": 1000}
        taken = sender.fixture_identity(compositor, member={1000, 17, 26, 303}, nodes={303})
        self.assertEqual(taken, {"user": "korri", "uid": 56774, "gid": 1000, "groups": [303]})
        # No audio, video or input group comes along, and none that the user is not in.
        self.assertEqual(
            sender.fixture_identity(compositor, member={1000, 17}, nodes={303})["groups"], []
        )

    def test_a_compositor_that_runs_as_root_gives_no_user_to_drop_to(self):
        with self.assertRaisesRegex(RuntimeError, "runs as root.*does not start"):
            sender.fixture_identity(
                {"user": "root", "uid": 0, "gid": 0}, member={0}, nodes={303}
            )
        with self.assertRaisesRegex(RuntimeError, "runs as root"):
            viewer.not_root({"user": "root", "uid": 0, "gid": 0, "exe": "/bin/sway"})
        ordinary = {"user": "korri", "uid": 56774, "gid": 1000, "exe": "/bin/sway"}
        self.assertIs(viewer.not_root(ordinary), ordinary)


if __name__ == "__main__":
    unittest.main()
