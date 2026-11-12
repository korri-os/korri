"""The second review's guard points: no software viewer on a handheld, and a guard that cannot be blind."""

from pathlib import Path
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import heat  # noqa: E402
import machine  # noqa: E402
import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
# The trip points that the RG353M reported on 2026-11-11.
RG353M = [("passive", 70000), ("passive", 75000), ("critical", 95000)]


class Device(unittest.TestCase):
    """A session on sysfs-shaped files: one thermal zone, and a chip the test chooses."""

    chip = b"vendor,board\0rockchip,rk3566\0"

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name)
        self.zone = root / "thermal" / "thermal_zone0"
        self.zone.mkdir(parents=True)
        (self.zone / "type").write_text("cpu-thermal\n")
        for n, (kind, value) in enumerate(RG353M):
            (self.zone / f"trip_point_{n}_type").write_text(kind + "\n")
            (self.zone / f"trip_point_{n}_temp").write_text(f"{value}\n")
        self.read(50000)
        (root / "compatible").write_bytes(self.chip)
        self.peer = root / "peer"
        self.peer.write_text(f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "host-peer.py"} "$@"\n')
        self.peer.chmod(0o755)
        self.previous = (heat.THERMAL, viewer.COMPATIBLE)
        heat.THERMAL, viewer.COMPATIBLE = root / "thermal", root / "compatible"
        self.supplies = machine.power_supplies(self, root)

    def tearDown(self):
        heat.THERMAL, viewer.COMPATIBLE = self.previous
        self.directory.cleanup()

    def session(self) -> viewer.Session:
        """A session that is closed even when its test fails: no run directory stays behind."""
        made = viewer.Session(str(self.peer))
        self.addCleanup(made.close)
        return made

    def read(self, millidegrees: int) -> None:
        # In one step, as sysfs answers: the guard's thread must never see half a write.
        fresh = self.zone.with_name("fresh-temp")
        fresh.write_text(f"{millidegrees}\n")
        fresh.replace(self.zone / "temp")

    def prepare(self, session: viewer.Session, **extra) -> dict:
        return session.handle(
            {
                "operation": "prepare", "fixture": True, "width": 640, "height": 480,
                "refresh_hz": 50.0, "certificate": None, "key": None,
                "server_certificate": "public", **extra,
            }
        )


class NoSoftwareViewerOnAHandheld(Device):
    def test_fixture_viewer_is_refused_on_a_chip_with_a_hardware_picture_path(self):
        session = self.session()
        facts = session.handle({"operation": "discover"})
        self.assertEqual(facts["platform"], "v4l2request")
        self.assertTrue(facts["hardware_picture_path"])
        with self.assertRaisesRegex(RuntimeError, "hardware picture path.*software viewer"):
            self.prepare(session)
        # Nothing was started and no directory was made.
        self.assertIsNone(session.root)
        self.assertEqual(session.processes, {})
        self.assertTrue(session.close()["pass"])

    def test_refusal_says_why_the_plugin_was_not_usable(self):
        session = self.session()
        session.facts["plugin_missing"] = "korri-plugin status ended with 1: korrid did not answer"
        with self.assertRaisesRegex(RuntimeError, "korrid did not answer"):
            self.prepare(session)

    def test_a_chip_with_no_hardware_path_keeps_the_fixture_viewer(self):
        viewer.COMPATIBLE.write_bytes(b"vendor,board\0vendor,chip\0")
        session = self.session()
        try:
            self.assertFalse(session.handle({"operation": "discover"})["hardware_picture_path"])
            self.assertTrue(Path(self.prepare(session)["run_folder"]).is_dir())
        finally:
            self.assertTrue(session.close()["pass"])


def hardware_session(case: Device) -> viewer.Session:
    """A session on a hardware-path chip with a run directory, as after a prepare.

    No plugin exists on a build host, so the test makes the run directory itself
    and starts stand-in viewers; the guard's rules are the real ones.
    """
    session = case.session()
    session.root = Path(tempfile.mkdtemp(prefix="cadence-guard-test-", dir=case.directory.name))
    (session.root / "rt").mkdir()
    (session.root / "home").mkdir()
    session.fixture = True  # no product compositor to ask for focus
    return session


def says(line: str) -> list[str]:
    """A stand-in viewer that logs one line and stays."""
    return ["sh", "-c", f'echo "{line}"; exec sleep 60']


YES = "v4l2m2m: submit: first frame from decoder format=drm_prime (DRM_PRIME=YES)"
NO = "v4l2m2m: submit: first frame from decoder format=nv12 (DRM_PRIME=NO)"


class EachStartHasItsOwnLog(Device):
    def test_decode_state_of_the_measured_start_is_not_the_locating_starts(self):
        session = hardware_session(self)
        try:
            session.handle({"operation": "start", "command": says(YES)})
            for _ in range(50):
                if session.handle({"operation": "sample"})["hardware_decode"]:
                    break
                time.sleep(0.1)
            self.assertIs(session.handle({"operation": "sample"})["hardware_decode"], True)
            session.handle({"operation": "stop"})
            # The measured start falls back to software decode.
            session.handle({"operation": "start", "command": says(NO)})
            running = session.processes["viewer"]
            self.assertIsNotNone(running.wait(timeout=5))
            with self.assertRaisesRegex(RuntimeError, "hardware decode did not start"):
                session.handle({"operation": "sample"})
            shown = session.handle({"operation": "evidence"})
            self.assertIn("DRM_PRIME=NO", shown["viewer_log"])
            self.assertNotIn("DRM_PRIME=YES", shown["viewer_log"])
            self.assertIn("DRM_PRIME=YES", shown["viewer_earlier_log"])
        finally:
            self.assertTrue(session.close()["pass"])

    def test_a_later_line_that_says_no_ends_the_run(self):
        session = hardware_session(self)
        try:
            # Hardware decode starts, then the viewer falls back while it runs.
            session.handle(
                {"operation": "start", "command": ["sh", "-c", f'echo "{YES}"; sleep 1; echo "{NO}"; exec sleep 60']}
            )
            running = session.processes["viewer"]
            self.assertIsNotNone(running.wait(timeout=6))
            with self.assertRaisesRegex(RuntimeError, "hardware decode did not start"):
                session.handle({"operation": "sample"})
        finally:
            self.assertTrue(session.close()["pass"])

    def test_a_no_far_down_a_long_log_is_seen(self):
        self.assertIs(viewer.decode_started(YES + "\n"), True)
        self.assertIs(viewer.decode_started(YES + "\n" + "x" * 100_000 + "\n" + NO + "\n"), False)
        self.assertIs(viewer.decode_started(NO + "\n" + YES + "\n"), False)
        # What was seen in an earlier part of the log is kept.
        self.assertIs(viewer.decode_started("more\n", known=True), True)
        self.assertIs(viewer.decode_started(YES + "\n", known=False), False)
        session = hardware_session(self)
        try:
            session.handle(
                {
                    "operation": "start",
                    "command": [
                        "sh", "-c",
                        f'echo "{YES}"; head -c 100000 /dev/zero | tr "\\0" x; echo; echo "{NO}"; exec sleep 60',
                    ],
                }
            )
            self.assertIsNotNone(session.processes["viewer"].wait(timeout=6))
        finally:
            self.assertTrue(session.close()["pass"])

    def test_a_measured_start_with_no_decode_line_has_no_decode_state(self):
        session = hardware_session(self)
        try:
            session.handle({"operation": "start", "command": says(YES)})
            time.sleep(0.5)
            session.handle({"operation": "stop"})
            session.handle({"operation": "start", "command": says("Connecting...")})
            time.sleep(0.5)
            # The validity gate needs True; the locating start's line must not supply it.
            self.assertIsNone(session.handle({"operation": "sample"})["hardware_decode"])
        finally:
            self.assertTrue(session.close()["pass"])


class DesktopChip(Device):
    chip = b"vendor,board\0vendor,chip\0"

    def running(self, session: viewer.Session):
        self.prepare(session)
        session.handle({"operation": "start", "command": ["sleep", "60"]})
        session.handle({"operation": "sample"})
        return session.processes["viewer"]


class AGuardThatCannotReadStopsTheRun(DesktopChip):
    def ended(self, break_it, words: str) -> None:
        session = self.session()
        try:
            process = self.running(session)
            break_it()
            # The guard's own thread stops the viewer.
            self.assertIsNotNone(process.wait(timeout=5))
            with self.assertRaisesRegex(RuntimeError, words):
                session.handle({"operation": "sample"})
        finally:
            self.assertTrue(session.close()["pass"])

    def test_temperature_that_is_no_number(self):
        self.ended(lambda: (self.zone / "temp").write_text("\n"), "cannot read.*cpu-thermal")

    def test_temperature_file_that_cannot_be_read(self):
        def broken():
            (self.zone / "temp").unlink()
            (self.zone / "temp").mkdir()

        self.ended(broken, "cannot read.*cpu-thermal")

    def test_zone_that_is_gone(self):
        def gone():
            for path in self.zone.iterdir():
                path.unlink()
            self.zone.rmdir()

        self.ended(gone, "cannot read.*thermal_zone0")

    def test_trip_point_that_cannot_be_read_does_not_kill_the_guard(self):
        def broken():
            (self.zone / "trip_point_2_type").unlink()
            (self.zone / "trip_point_2_type").mkdir()

        self.ended(broken, "cannot read.*cpu-thermal")

    def test_run_does_not_start_when_the_guard_cannot_read(self):
        (self.zone / "temp").write_text("not a number\n")
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "cannot read.*does not start"):
            self.prepare(session)
        self.assertIsNone(session.root)
        self.assertTrue(session.close()["pass"])

    def test_zone_with_no_critical_trip_may_be_unreadable(self):
        # Recorded and never judged: its temperature decides nothing.
        other = self.zone.with_name("thermal_zone1")
        other.mkdir()
        (other / "type").write_text("board-sensor\n")
        (other / "temp").write_text("\n")
        session = self.session()
        try:
            self.prepare(session)
            sample = session.handle({"operation": "sample"})
            self.assertIn(None, [zone["temp"] for zone in sample["thermal"]])
        finally:
            self.assertTrue(session.close()["pass"])


def trips(*values: tuple[str, int]) -> dict:
    return {
        "zone": "thermal_zone0", "type": "cpu-thermal", "temp": 50000, "unreadable": None,
        "trips": [{"type": kind, "temp": value} for kind, value in values],
    }


class TestMarginOnlyLowersTheEnd(DesktopChip):
    def test_a_margin_below_the_tools_own_is_not_taken(self):
        for zone in (trips(*RG353M), trips(("critical", 110000))):
            normal = heat.thermal_limits(zone)
            for margin in (0, 1, 9999):
                with self.subTest(margin=margin, critical=normal["critical"]):
                    self.assertEqual(heat.thermal_limits(zone, end_margin=margin), normal)
            self.assertLess(
                heat.thermal_limits(zone, end_margin=30000)["end_at"], normal["end_at"]
            )

    def test_the_agent_never_ends_later_than_its_own_rule(self):
        session = self.session()
        try:
            self.prepare(session, test_end_margin=1)
            self.assertEqual(session.limits["thermal_zone0"]["end_at"], 85000)
        finally:
            self.assertTrue(session.close()["pass"])


class AStopAfterTheLastSampleIsReported(DesktopChip):
    def test_stop_says_that_the_guard_ended_the_viewer(self):
        session = self.session()
        try:
            process = self.running(session)
            self.read(86000)
            self.assertIsNotNone(process.wait(timeout=5))
            # No sample comes any more; the tool asks the viewer to stop.
            with self.assertRaisesRegex(RuntimeError, "reads 86.0.*ends at 85.0"):
                session.handle({"operation": "stop"})
        finally:
            self.assertTrue(session.close()["pass"])

    def test_stop_of_a_viewer_that_the_guard_left_alone_is_plain(self):
        session = self.session()
        try:
            self.running(session)
            self.assertIn("exit", session.handle({"operation": "stop"}))
        finally:
            self.assertTrue(session.close()["pass"])


class HeatIsCheckedBeforeAnythingStarts(DesktopChip):
    def test_check_refuses_a_hot_or_unreadable_device_and_makes_nothing(self):
        session = self.session()
        self.assertEqual(session.handle({"operation": "check"}), {})
        self.read(72222)
        with self.assertRaisesRegex(RuntimeError, "device is hot.*72.2"):
            session.handle({"operation": "check"})
        (self.zone / "temp").write_text("\n")
        with self.assertRaisesRegex(RuntimeError, "cannot read.*does not start"):
            session.handle({"operation": "check"})
        self.assertIsNone(session.root)
        self.assertTrue(session.close()["pass"])


class ABusyViewerOnAHardwarePath(Device):
    """The CPU rule on a real process. The limit is set to half a CPU, so one busy loop is over it."""

    def setUp(self):
        super().setUp()
        self.rate, viewer.BUSY_RATE = viewer.BUSY_RATE, 0.5

    def tearDown(self):
        viewer.BUSY_RATE = self.rate
        super().tearDown()

    def test_viewer_that_spins_is_ended_with_its_cpu_rate(self):
        session = hardware_session(self)
        try:
            session.handle({"operation": "start", "command": ["sh", "-c", "while :; do :; done"]})
            process = session.processes["viewer"]
            self.assertIsNotNone(process.wait(timeout=10))
            with self.assertRaisesRegex(RuntimeError, "CPU seconds each second"):
                session.handle({"operation": "sample"})
        finally:
            self.assertTrue(session.close()["pass"])

    def test_viewer_that_idles_runs_on(self):
        session = hardware_session(self)
        try:
            session.handle({"operation": "start", "command": says(YES)})
            time.sleep(viewer.BUSY_SECONDS + 1)
            sample = session.handle({"operation": "sample"})
            self.assertIsNone(sample["viewer_exit"])
            self.assertIs(sample["hardware_decode"], True)
        finally:
            self.assertTrue(session.close()["pass"])


class NoFixtureOnADeviceTree(DesktopChip):
    """The R36T Max: its chip is not in korrid's table, and its kernel shows no power supply.

    `nix/devices/r36tmax/dts/rk3326-aislpc-r36t-max.dts` gives the compatible
    value; the device tree leaves the charger node out. The zone has the
    passive trip at 70 °C that the kernel shows there.
    """

    chip = b"aislpc,r36t-max\0rockchip,rk3326\0"

    def test_fixture_viewer_is_refused_on_the_r36t_max(self):
        machine.device_tree()
        self.read(50000)
        session = self.session()
        facts = session.handle({"operation": "discover"})
        self.assertFalse(facts["hardware_picture_path"])
        self.assertEqual(list(self.supplies.glob("*")), [])
        self.assertIs(facts["device_tree"], True)
        with self.assertRaisesRegex(
            RuntimeError, "device tree.*runs only its installed viewer.*viewer fixture does not start"
        ):
            self.prepare(session)
        self.assertIsNone(session.root)
        self.assertEqual(session.processes, {})

    def test_a_laptop_has_no_device_tree_and_hosts_the_fixture(self):
        # Its battery does not decide. Its fans and thermal zones are what the heat guard is for.
        machine.supply(self.supplies, "AC", "Mains")
        machine.supply(self.supplies, "BAT0", "Battery")
        session = self.session()
        self.assertIs(session.handle({"operation": "discover"})["device_tree"], False)
        self.assertTrue(Path(self.prepare(session)["run_folder"]).is_dir())

    def test_a_hot_laptop_is_still_refused_by_the_heat_guard(self):
        machine.supply(self.supplies, "BAT0", "Battery")
        self.read(72000)
        with self.assertRaisesRegex(RuntimeError, "The device is hot"):
            self.prepare(self.session())

    def test_a_mains_board_with_a_device_tree_is_refused_too(self):
        # The accepted cost of the rule: an ARM builder is not told from a handheld.
        machine.supply(self.supplies, "usb", "USB")
        machine.device_tree()
        with self.assertRaisesRegex(RuntimeError, "device tree"):
            self.prepare(self.session())


class ADeviceWithNoThermalZone(Device):
    def empty(self) -> None:
        for path in self.zone.iterdir():
            path.unlink()
        self.zone.rmdir()

    def test_hardware_path_device_with_no_zone_does_not_run(self):
        self.empty()
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "no thermal zone with a critical trip point"):
            self.prepare(session)
        self.assertIsNone(session.root)

    def test_hardware_path_device_whose_zones_have_no_critical_trip_does_not_run(self):
        (self.zone / "trip_point_2_type").write_text("passive\n")
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "no thermal zone with a critical trip point"):
            self.prepare(session)

    def test_other_device_with_no_zone_runs_unguarded_and_says_so(self):
        self.empty()
        viewer.COMPATIBLE.write_bytes(b"vendor,board\0vendor,chip\0")
        session = self.session()
        try:
            facts = session.handle({"operation": "discover"})
            self.assertEqual(facts["thermal"], [])
            self.assertFalse(facts["heat_guarded"])
            self.prepare(session)
            self.assertEqual(session.handle({"operation": "sample"})["thermal"], [])
        finally:
            self.assertTrue(session.close()["pass"])


if __name__ == "__main__":
    unittest.main()
