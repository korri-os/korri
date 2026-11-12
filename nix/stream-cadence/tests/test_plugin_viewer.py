"""The product's viewer in the fixture: a private pair on one device shows nothing on its display."""

from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cli import DISPLAY, FIXTURE, PLUGIN_VIEWER, STAND_IN, viewer_place, viewer_ran  # noqa: E402
import heat  # noqa: E402
import machine  # noqa: E402
from verdict import judge, viewers  # noqa: E402
import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
PLUGIN = {
    "package": "/nix/store/aaaa-korri-plugin",
    "release_version": "build-0123456789ab-test",
    "moonlight": "/nix/store/bbbb-moonlight-embedded-korri-2.7.1-korri/bin/moonlight",
    "mapping": "/nix/store/bbbb-moonlight-embedded-korri-2.7.1-korri/share/moonlight/map.txt",
}


class WhereTheViewerRuns(unittest.TestCase):
    def test_a_private_pair_on_one_device_never_uses_that_devices_display(self):
        # ibuki: the plugin is installed, and its display is the family's TV.
        self.assertEqual(viewer_place(plugin=True, sender_fixture=True, same_device=True), FIXTURE)
        # No plugin: the fixture with the stand-in, as on aka.
        self.assertEqual(viewer_place(plugin=False, sender_fixture=True, same_device=True), FIXTURE)
        self.assertEqual(viewer_place(plugin=False, sender_fixture=False, same_device=True), FIXTURE)

    def test_the_devices_own_display_stays_for_a_viewer_on_another_device_than_the_sender(self):
        self.assertEqual(viewer_place(plugin=True, sender_fixture=True, same_device=False), DISPLAY)
        # A product sender is no private pair: the installed viewer shows on the display.
        self.assertEqual(viewer_place(plugin=True, sender_fixture=False, same_device=True), DISPLAY)
        self.assertEqual(viewer_place(plugin=True, sender_fixture=False, same_device=False), DISPLAY)

    def test_the_result_says_which_viewer_ran_and_where(self):
        ran = viewer_ran(PLUGIN, "", FIXTURE)
        self.assertEqual(
            ran,
            {
                "kind": PLUGIN_VIEWER,
                "place": FIXTURE,
                "moonlight": PLUGIN["moonlight"],
                "release_version": "build-0123456789ab-test",
                "package": "/nix/store/aaaa-korri-plugin",
            },
        )
        stand_in = viewer_ran(None, "/nix/store/cccc-moonlight-embedded-2.7.1", FIXTURE)
        self.assertEqual(
            stand_in,
            {
                "kind": STAND_IN,
                "place": FIXTURE,
                "moonlight": "/nix/store/cccc-moonlight-embedded-2.7.1/bin/moonlight",
                "release_version": None,
                "package": None,
            },
        )


class PluginViewerInTheFixture(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        peer = root / "peer"
        peer.write_text(f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "host-peer.py"} "$@"\n')
        peer.chmod(0o755)
        self.peer = peer
        previous = (heat.THERMAL, viewer.COMPATIBLE)
        heat.THERMAL, viewer.COMPATIBLE = root / "thermal", root / "compatible"
        self.addCleanup(setattr, heat, "THERMAL", previous[0])
        self.addCleanup(setattr, viewer, "COMPATIBLE", previous[1])
        machine.power_supplies(self, root)

    def session(self) -> viewer.Session:
        made = viewer.Session(str(self.peer))
        self.addCleanup(made.close)
        return made

    def prepare(self, session: viewer.Session, **extra) -> dict:
        return session.handle(
            {
                "operation": "prepare", "fixture": True, "width": 640, "height": 480,
                "refresh_hz": 50.0, "certificate": None, "key": None,
                "server_certificate": "public", **extra,
            }
        )

    def test_plugin_binary_gets_korrids_environment_on_the_fixtures_wayland_display(self):
        session = self.session()
        session.facts["plugin"] = dict(PLUGIN)
        self.prepare(session, plugin_viewer=True)
        environment = session.viewer_environment()
        # What korrid's linux_viewer gives the viewer.
        self.assertEqual(environment["KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT"], "1")
        self.assertEqual(environment["SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS"], "1")
        self.assertEqual(environment["KORRI_MOONLIGHT_SERVER_CERTIFICATE"], str(session.root / "server.pem"))
        # The fixture's own compositor, and nothing that names the device's display.
        self.assertEqual(environment["WAYLAND_DISPLAY"], viewer.DISPLAY)
        self.assertEqual(environment["XDG_RUNTIME_DIR"], str(session.root / "rt"))
        self.assertEqual(environment["SDL_VIDEODRIVER"], "wayland")
        self.assertEqual(environment["SDL_AUDIODRIVER"], "dummy")
        self.assertNotIn("DISPLAY", environment)
        self.assertEqual(environment["HOME"], str(session.root / "home"))
        self.assertTrue(session.fixture)

    def test_stand_in_keeps_its_environment(self):
        session = self.session()
        self.prepare(session)
        environment = session.viewer_environment()
        self.assertNotIn("KORRI_MOONLIGHT_SERVER_CERTIFICATE", environment)
        self.assertNotIn("DISPLAY", environment)
        self.assertEqual(environment["SDL_VIDEODRIVER"], "wayland")

    def test_plugin_viewer_is_refused_where_the_agent_found_no_plugin(self):
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "no enabled viewer plugin"):
            self.prepare(session, plugin_viewer=True)
        self.assertIsNone(session.root)

    def test_plugin_viewer_gets_no_fixture_on_a_hardware_picture_path(self):
        viewer.COMPATIBLE.write_bytes(b"vendor,board\0rockchip,rk3566\0")
        zone = heat.THERMAL / "thermal_zone0"
        zone.mkdir(parents=True)
        (zone / "type").write_text("cpu-thermal\n")
        (zone / "trip_point_0_type").write_text("critical\n")
        (zone / "trip_point_0_temp").write_text("95000\n")
        (zone / "temp").write_text("40000\n")
        session = self.session()
        session.facts["plugin"] = dict(PLUGIN)
        with self.assertRaisesRegex(RuntimeError, "hardware picture path.*no fixture"):
            self.prepare(session, plugin_viewer=True)
        self.assertIsNone(session.root)

    def test_a_display_that_cannot_be_read_does_not_stop_the_agent(self):
        def unread(_shell: dict) -> dict:
            raise ValueError("need exactly one active compositor output")

        self.assertEqual(viewer.own_display({}, unread), (None, "need exactly one active compositor output"))
        self.assertEqual(viewer.own_display({}, lambda _shell: {"name": "DP-1"}), ({"name": "DP-1"}, None))


class IbukisThermalZones(unittest.TestCase):
    """ibuki, an Intel NUC, as its kernel shows the zones: placeholders must not refuse a start."""

    ZONES = (
        ("pch_cannonlake", 38000, [("critical", 120000)]),
        ("acpitz", -263200, []),
        ("acpitz", 27800, [("critical", 119000), ("active", 71000), ("active", 55000), ("active", 40000)]),
        ("iwlwifi_1", 36000, [("passive", -274000)] * 8),
        ("x86_pkg_temp", 40000, [("passive", -274000)] * 2),
    )

    def test_placeholder_trips_give_no_limit_and_a_cool_device_starts(self):
        with tempfile.TemporaryDirectory() as temporary:
            previous, heat.THERMAL = heat.THERMAL, Path(temporary)
            self.addCleanup(setattr, heat, "THERMAL", previous)
            for number, (kind, temp, trips) in enumerate(self.ZONES):
                zone = heat.THERMAL / f"thermal_zone{number}"
                zone.mkdir()
                (zone / "type").write_text(kind + "\n")
                (zone / "temp").write_text(f"{temp}\n")
                for n, (trip, value) in enumerate(trips):
                    (zone / f"trip_point_{n}_type").write_text(trip + "\n")
                    (zone / f"trip_point_{n}_temp").write_text(f"{value}\n")
            zones = heat.thermal_zones()
            limits = {zone["zone"]: heat.thermal_limits(zone) for zone in zones}
            self.assertEqual(
                limits,
                {
                    "thermal_zone0": {"critical": 120000, "start_below": 100000, "end_at": 110000},
                    "thermal_zone1": None,
                    "thermal_zone2": {"critical": 119000, "start_below": 99000, "end_at": 109000},
                    "thermal_zone3": None,
                    "thermal_zone4": None,
                },
            )
            self.assertIsNone(heat.no_start(zones, limits, hardware_path=False))
            self.assertIsNone(heat.must_end(zones, limits))


class WhichViewerAVerdictCompares(unittest.TestCase):
    def result(self, ran: dict | None, **facts) -> list[dict]:
        window = {
            "latency_ms": {"p50": 20.0, "p95": 22.0}, "capture_misses": 0,
            "repeated_frames": 0, "late_frames": 0, "torn_frames": 0, "undecodable_frames": 0,
        }
        known = {"valid": True, "model": "host", "viewer": {"model": "host"}, "max_capture_gaps": 0}
        if ran is not None:
            known["viewer_ran"] = ran
        return [
            {"discovered": {**known, **facts}, "source_counter": {"windows": [window]}, "viewer": {"startup_ms": 900.0}}
            for _ in range(5)
        ]

    def test_verdict_says_which_viewer_each_result_ran(self):
        plugin = self.result(viewer_ran(PLUGIN, "", FIXTURE))
        stand_in = self.result(viewer_ran(None, "/nix/store/cccc-moonlight-embedded-2.7.1", FIXTURE))
        # The viewer is what a comparison changes on purpose: it is compared, and it is said.
        self.assertEqual(len(judge(plugin, stand_in, 5)), 15)
        lines = viewers(plugin, stand_in)
        self.assertRegex(lines[0], "^baseline viewer: plugin build-0123456789ab-test .*bbbb-moonlight-embedded-korri.*in the fixture")
        self.assertRegex(lines[1], "^candidate viewer: stand-in .*cccc-moonlight-embedded-2.7.1.*in the fixture")
        self.assertRegex(lines[2], "different viewers")
        same = viewers(plugin, plugin)
        self.assertEqual(len(same), 3)
        self.assertRegex(same[2], "same viewer")

    def test_one_result_has_one_viewer(self):
        plugin = self.result(viewer_ran(PLUGIN, "", FIXTURE))
        stand_in = self.result(viewer_ran(None, "/nix/store/cccc-moonlight-embedded-2.7.1", FIXTURE))
        mixed = plugin[:4] + stand_in[:1]
        with self.assertRaisesRegex(ValueError, "more than one viewer"):
            judge(mixed, plugin, 5)

    def test_the_fixture_and_the_devices_display_are_two_conditions(self):
        fixture = self.result(viewer_ran(PLUGIN, "", FIXTURE))
        display = self.result(viewer_ran(PLUGIN, "", DISPLAY))
        with self.assertRaisesRegex(ValueError, "viewer_place") as refused:
            judge(fixture, display, 5)
        self.assertIn("fixture", str(refused.exception))
        self.assertIn("device's own display", str(refused.exception))

    def test_a_result_from_before_the_fact_is_read_from_what_it_has(self):
        # Before: a device with the plugin ran it on its display; any other device
        # ran the stand-in in the fixture.
        old_plugin = self.result(None, viewer={"model": "host", "plugin": dict(PLUGIN)})
        old_stand_in = self.result(
            None, viewer_command=["/nix/store/cccc-moonlight-embedded-2.7.1/bin/moonlight", "stream"]
        )
        self.assertRegex(viewers(old_plugin, old_plugin)[0], "plugin build-0123456789ab-test .*on the device's own display")
        self.assertRegex(viewers(old_stand_in, old_stand_in)[0], "stand-in .*cccc-moonlight-embedded-2.7.1.*in the fixture")
        self.assertEqual(len(judge(old_stand_in, self.result(viewer_ran(None, "/nix/store/cccc-moonlight-embedded-2.7.1", FIXTURE)), 5)), 15)


if __name__ == "__main__":
    unittest.main()
