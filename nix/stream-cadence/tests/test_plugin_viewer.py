"""The product's viewer in the fixture: a private pair on one device shows nothing on its display."""

import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cli  # noqa: E402
from cli import (  # noqa: E402
    COPIED_PRODUCT,
    DISPLAY,
    FIXTURE,
    PLUGIN_VIEWER,
    STAND_IN,
    Refusal,
    copied_viewer_refusal,
    parser,
    product_package,
    renderer_refusal,
    signer_names,
    viewer_place,
    viewer_ran,
)
import device  # noqa: E402
import heat  # noqa: E402
import machine  # noqa: E402
from verdict import judge, product_changes, viewers  # noqa: E402
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
        with self.assertRaisesRegex(RuntimeError, "board with a device tree.*viewer fixture does not start"):
            self.prepare(session, plugin_viewer=True)
        self.assertIsNone(session.root)

    def test_a_display_that_cannot_be_read_does_not_stop_the_agent(self):
        def unread(_shell: dict) -> dict:
            raise ValueError("need exactly one active compositor output")

        self.assertEqual(viewer.own_display({}, unread), (None, "need exactly one active compositor output"))
        self.assertEqual(viewer.own_display({}, lambda _shell: {"name": "DP-1"}), ({"name": "DP-1"}, None))


class FixtureRenderer(PluginViewerInTheFixture):
    """The viewer fixture's compositor draws on the device's graphics card; pixman only by option."""

    def test_gles2_is_the_default_of_the_fixtures_compositor(self):
        # A measure of a viewer must not carry Mesa's software rasterizer.
        session = self.session()
        prepared = self.prepare(session)
        self.assertEqual(prepared["renderer"], "gles2")
        environment = session.fixture_environment()
        self.assertEqual(environment["WLR_RENDERER"], "gles2")
        self.assertEqual(environment["WLR_BACKENDS"], "headless")
        self.assertEqual(environment["WLR_LIBINPUT_NO_DEVICES"], "1")
        # No software renderer behind the name gles2.
        self.assertNotIn("WLR_RENDERER_ALLOW_SOFTWARE", environment)
        self.assertEqual(viewer.RENDERER_DEFAULT, "gles2")
        self.assertIsNone(parser().parse_args(["view", "--sender", "a", "--viewer", "b"]).viewer_renderer)

    def test_pixman_is_the_explicit_way_to_the_old_behaviour(self):
        session = self.session()
        prepared = self.prepare(session, renderer="pixman")
        self.assertEqual(prepared["renderer"], "pixman")
        self.assertEqual(session.fixture_environment()["WLR_RENDERER"], "pixman")
        self.assertEqual(session.groups, [])
        args = parser().parse_args(["view", "--sender", "a", "--viewer", "b", "--viewer-renderer", "pixman"])
        self.assertEqual(args.viewer_renderer, "pixman")

    def test_only_the_groups_of_a_render_node_are_kept_for_gles2(self):
        # The compositor's user is in audio, video, render and input; a render node belongs to render.
        self.assertEqual(viewer.render_groups(member={17, 26, 303, 174}, nodes={303}), [303])
        self.assertEqual(viewer.render_groups(member={17}, nodes={303}), [])

    def test_gles2_that_cannot_start_is_refused_with_the_cause_and_the_option(self):
        # A compositor that finds no graphics card ends, as sway does. Nothing falls back.
        failing = Path(self.directory.name) / "no-card"
        failing.write_text("#!/bin/sh\necho '[ERROR] [render/egl.c:312] Failed to create EGL context' >&2\nexit 1\n")
        failing.chmod(0o755)
        session = viewer.Session(str(failing))
        self.addCleanup(session.close)
        with self.assertRaises(RuntimeError) as refused:
            self.prepare(session)
        words = str(refused.exception)
        self.assertRegex(words, "did not start with the gles2 renderer")
        self.assertIn("Failed to create EGL context", words)
        self.assertIn("render node", words)
        self.assertIn("--viewer-renderer pixman", words)
        self.assertEqual(session.processes.get("viewer"), None)
        # With pixman the same failure is not blamed on the graphics card.
        other = viewer.Session(str(failing))
        self.addCleanup(other.close)
        with self.assertRaises(RuntimeError) as plain:
            self.prepare(other, renderer="pixman")
        self.assertNotIn("--viewer-renderer", str(plain.exception))

    def test_an_unknown_renderer_and_a_renderer_for_the_devices_display_are_refused(self):
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "renderer"):
            self.prepare(session, renderer="vulkan")
        self.assertIsNone(session.root)
        for asked in ("gles2", "pixman"):
            self.assertRegex(renderer_refusal(asked, on_display=True), "--viewer-renderer.*fixture")
            self.assertIsNone(renderer_refusal(asked, on_display=False))
        # No option on the device's own display: nothing to refuse.
        self.assertIsNone(renderer_refusal(None, on_display=True))
        self.assertIsNone(renderer_refusal(None, on_display=False))

    def test_the_renderer_is_a_condition_of_a_verdict(self):
        def result(renderer: str | None) -> list[dict]:
            window = {
                "latency_ms": {"p50": 20.0, "p95": 22.0}, "capture_misses": 0,
                "repeated_frames": 0, "late_frames": 0, "torn_frames": 0, "undecodable_frames": 0,
            }
            facts = {"valid": True, "model": "host", "viewer": {"model": "host"}, "max_capture_gaps": 0}
            if renderer:
                facts["viewer_fixture"] = {"renderer": renderer}
            return [
                {"discovered": dict(facts), "source_counter": {"windows": [window]}, "viewer": {"startup_ms": 900.0}}
                for _ in range(5)
            ]

        with self.assertRaisesRegex(ValueError, "viewer_renderer") as refused:
            judge(result("pixman"), result("gles2"), 5)
        self.assertIn("software rasterizer", str(refused.exception))
        # A result from before the fact ran with pixman: the fixture had no other renderer.
        # So the landed baseline of that time does not compare with a result of today's default.
        self.assertEqual(len(judge(result(None), result("pixman"), 5)), 15)
        with self.assertRaisesRegex(ValueError, "viewer_renderer"):
            judge(result(None), result("gles2"), 5)
        self.assertEqual(len(judge(result("gles2"), result("gles2"), 5)), 15)


class CopiedProductViewer(PluginViewerInTheFixture):
    """A product viewer package that the tool copies to a device with no plugin, as aka is."""

    def package(self) -> Path:
        store = Path(self.directory.name) / "store"
        package = store / "ycag-moonlight-embedded-korri-2.7.1-korri"
        (package / "bin").mkdir(parents=True)
        (package / "bin" / "moonlight").write_text("#!/bin/sh\n")
        (package / "share" / "moonlight").mkdir(parents=True)
        (package / "share" / "moonlight" / "korri-inputplumber-gamecontrollerdb.txt").write_text("map\n")
        previous, cli.NIX_STORE = cli.NIX_STORE, store
        self.addCleanup(setattr, cli, "NIX_STORE", previous)
        return package

    def test_the_package_gives_the_plugins_binary_and_its_mapping(self):
        package = self.package()
        self.assertEqual(
            product_package(str(package)),
            {
                "package": str(package),
                "moonlight": str(package / "bin" / "moonlight"),
                "mapping": str(package / "share" / "moonlight" / "korri-inputplumber-gamecontrollerdb.txt"),
            },
        )
        # A trailing slash names the same package.
        self.assertEqual(product_package(str(package) + "/")["package"], str(package))

    def test_a_path_that_is_no_product_viewer_package_is_refused_before_any_device(self):
        package = self.package()
        with self.assertRaisesRegex(Refusal, "--viewer-package.*store path"):
            product_package(str(package / "bin"))
        with self.assertRaisesRegex(Refusal, "--viewer-package.*store path"):
            product_package(self.directory.name)
        (package / "share" / "moonlight" / "korri-inputplumber-gamecontrollerdb.txt").unlink()
        with self.assertRaisesRegex(Refusal, "korri-inputplumber-gamecontrollerdb.txt"):
            product_package(str(package))
        (package / "bin" / "moonlight").unlink()
        with self.assertRaisesRegex(Refusal, "bin/moonlight"):
            product_package(str(package))

    def test_view_refuses_a_bad_package_before_it_asks_any_device(self):
        self.package()
        asked: list = []

        def no_device(*command, **options):
            asked.append(command)
            raise AssertionError("a device was asked")

        for name in ("execute", "tools"):
            previous = getattr(cli, name)
            setattr(cli, name, no_device)
            self.addCleanup(setattr, cli, name, previous)
        previous_run = cli.subprocess.run
        cli.subprocess.run = no_device
        self.addCleanup(setattr, cli.subprocess, "run", previous_run)
        args = parser().parse_args(
            ["view", "--sender", "root@aka", "--sender-fixture", "--viewer", "root@aka",
             "--viewer-package", self.directory.name, "--out", self.directory.name + "/out"]
        )
        with self.assertRaisesRegex(Refusal, "--viewer-package.*store path"):
            cli.view(args)
        self.assertEqual(asked, [])

    def test_it_is_for_a_device_with_no_plugin(self):
        self.assertIsNone(copied_viewer_refusal(plugin=False))
        self.assertRegex(copied_viewer_refusal(plugin=True), "--viewer-package.*plugin.*installed")

    def test_the_result_says_copied_product_package_and_where_it_came_from(self):
        package = self.package()
        copied = dict(product_package(str(package)), copied_from="buildhost", signed_by=["korri-plugins-1"])
        ran = viewer_ran(None, "", FIXTURE, copied=copied)
        self.assertEqual(
            ran,
            {
                "kind": COPIED_PRODUCT,
                "place": FIXTURE,
                "moonlight": str(package / "bin" / "moonlight"),
                "release_version": None,
                "package": str(package),
                "copied_from": "buildhost",
                "signed_by": ["korri-plugins-1"],
            },
        )
        # Not the installed plugin, and not the stand-in.
        self.assertNotIn(ran["kind"], (PLUGIN_VIEWER, STAND_IN))

    def test_signers_are_read_from_nix_path_info(self):
        path = "/nix/store/ycag-moonlight-embedded-korri-2.7.1-korri"
        new = json.dumps({path: {"signatures": ["korri-plugins-1:Pgbb==", "cache.nixos.org-1:abc="]}})
        old = json.dumps([{"path": path, "signatures": ["korri-plugins-1:Pgbb=="]}])
        self.assertEqual(signer_names(new), ["cache.nixos.org-1", "korri-plugins-1"])
        self.assertEqual(signer_names(old), ["korri-plugins-1"])
        self.assertEqual(signer_names(json.dumps({path: {}})), [])

    def test_the_agent_starts_a_copied_package_as_it_starts_the_plugins_binary(self):
        package = self.package()
        session = self.session()
        self.assertIsNone(session.facts["plugin"])
        self.prepare(session, plugin_viewer=True, viewer_package=str(package))
        environment = session.viewer_environment()
        self.assertEqual(environment["KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT"], "1")
        self.assertEqual(environment["SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS"], "1")
        self.assertEqual(environment["KORRI_MOONLIGHT_SERVER_CERTIFICATE"], str(session.root / "server.pem"))
        self.assertEqual(environment["SDL_VIDEODRIVER"], "wayland")
        self.assertNotIn("DISPLAY", environment)

    def test_the_agent_refuses_a_package_that_is_not_on_the_device(self):
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "is not on this device"):
            self.prepare(session, plugin_viewer=True, viewer_package="/nix/store/none-moonlight-embedded-korri")
        self.assertIsNone(session.root)

    def test_verdict_names_a_copied_package_and_knows_the_same_binary(self):
        def result(ran: dict) -> list[dict]:
            window = {
                "latency_ms": {"p50": 20.0, "p95": 22.0}, "capture_misses": 0,
                "repeated_frames": 0, "late_frames": 0, "torn_frames": 0, "undecodable_frames": 0,
            }
            facts = {
                "valid": True, "model": "host", "viewer": {"model": "host"}, "max_capture_gaps": 0,
                "viewer_ran": ran, "viewer_fixture": {"renderer": "gles2"},
            }
            return [
                {"discovered": dict(facts), "source_counter": {"windows": [window]}, "viewer": {"startup_ms": 900.0}}
                for _ in range(5)
            ]

        copied = viewer_ran(
            None, "", FIXTURE,
            copied={"package": "/nix/store/bbbb-moonlight-embedded-korri-2.7.1-korri", "moonlight": PLUGIN["moonlight"],
                    "mapping": PLUGIN["mapping"], "copied_from": "buildhost", "signed_by": ["korri-plugins-1"]},
        )
        stand_in = viewer_ran(None, "/nix/store/cccc-moonlight-embedded-2.7.1", FIXTURE)
        lines = viewers(result(copied), result(stand_in))
        self.assertRegex(
            lines[0],
            "^baseline viewer: copied product package \\(.*bbbb-moonlight-embedded-korri.*copied from buildhost, signed by korri-plugins-1\\), in the fixture",
        )
        self.assertRegex(lines[2], "different viewers")
        # The same binary, installed as a plugin on one device and copied to the other.
        installed = viewer_ran(PLUGIN, "", FIXTURE)
        self.assertRegex(viewers(result(copied), result(installed))[2], "same viewer binary")
        self.assertEqual(len(judge(result(copied), result(stand_in), 5)), 15)


class ProductUnits(unittest.TestCase):
    """The product's own processes, read at the start and at the end of a run."""

    SHOW = (
        "Id=korri-compositor.service\nMainPID=391963\nExecMainStartTimestampMonotonic=1996941782522\n\n"
        "Id=korri-shell.service\nMainPID=919859\nExecMainStartTimestampMonotonic=2091468004537\n\n"
        "Id=korrid.service\nMainPID=919983\nExecMainStartTimestampMonotonic=2091469746682\n\n"
        "Id=korri-oneshot.service\nMainPID=0\nExecMainStartTimestampMonotonic=5\n"
    )

    def test_units_are_the_ones_that_the_device_has_without_the_games(self):
        listing = (
            "korri-compositor.service loaded active running Korri compositor\n"
            "korri-game-0123456789abcdef.service loaded active running [systemd-run] /nix/store/x secret-argument\n"
            "korri-shell.service loaded active running Korri shell\n"
            "korrid.service loaded active running korrid\n"
        )
        self.assertEqual(
            device.unit_names(listing), ["korri-compositor.service", "korri-shell.service", "korrid.service"]
        )
        self.assertEqual(
            device.unit_processes(self.SHOW),
            {
                "korri-compositor.service": {"pid": 391963, "started": 1996941782522},
                "korri-shell.service": {"pid": 919859, "started": 2091468004537},
                "korrid.service": {"pid": 919983, "started": 2091469746682},
                "korri-oneshot.service": {"pid": 0, "started": 5},
            },
        )

    def test_a_unit_that_restarted_appeared_or_went_is_named(self):
        before = device.unit_processes(self.SHOW)
        self.assertEqual(device.changed_units(before, dict(before)), [])
        after = dict(before)
        # ibuki at 17:05:50 UTC: a bundle switch restarted three units; the compositor kept its PID.
        after["korri-shell.service"] = {"pid": 1000001, "started": 2099999999999}
        after["korrid.service"] = {"pid": 1000002, "started": 2099999999998}
        del after["korri-oneshot.service"]
        after["korri-sunshine.service"] = {"pid": 1000003, "started": 2099999999997}
        self.assertEqual(
            device.changed_units(before, after),
            ["korri-oneshot.service (gone)", "korri-shell.service (restarted)", "korri-sunshine.service (new)", "korrid.service (restarted)"],
        )
        # Units that could not be read at one end say so; nothing is guessed.
        self.assertEqual(device.changed_units(None, after), ["the product's units could not be read"])

    def test_verdict_says_in_a_line_of_its_own_which_runs_had_a_change(self):
        def result(changed: list[str] | None) -> list[dict]:
            window = {
                "latency_ms": {"p50": 20.0, "p95": 22.0}, "capture_misses": 0,
                "repeated_frames": 0, "late_frames": 0, "torn_frames": 0, "undecodable_frames": 0,
            }
            runs = []
            for n in range(5):
                facts: dict = {"valid": True, "model": "host", "viewer": {"model": "host"}, "max_capture_gaps": 0}
                if changed is not None:
                    hit = changed if n == 2 else []
                    facts["product_units"] = {"sender": {"changed": hit}, "viewer": {"changed": hit}}
                runs.append(
                    {"label": f"run-{n + 1}", "discovered": facts, "source_counter": {"windows": [window]}, "viewer": {"startup_ms": 900.0}}
                )
            return runs

        quiet = result([])
        switched = result(["korri-shell.service (restarted)", "korrid.service (restarted)"])
        self.assertEqual(product_changes(quiet, quiet), [])
        lines = product_changes(quiet, switched)
        self.assertEqual(len(lines), 1)
        self.assertRegex(
            lines[0],
            "^candidate: the product changed during run-3: korri-shell.service \\(restarted\\), korrid.service \\(restarted\\)",
        )
        # The run stays valid and is judged; the line is what tells.
        self.assertEqual(len(judge(quiet, switched, 5)), 15)
        # A result from before the record says nothing about its product.
        self.assertEqual(product_changes(result(None), quiet), ["baseline: the product's units were not recorded."])


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

    def test_a_viewer_whose_opengl_fell_back_to_software_is_not_silent(self):
        def ran(nodes: list[str] | None) -> dict:
            return dict(viewer_ran(PLUGIN, "", FIXTURE), render_nodes=nodes)

        card, software = self.result(ran(["/dev/dri/renderD128"])), self.result(ran([]))
        lines = viewers(card, software)
        self.assertRegex(lines[0], "OpenGL on /dev/dri/renderD128")
        self.assertRegex(lines[1], "no render node open")
        self.assertRegex(lines[2], "same viewer binary.*OpenGL differs")
        self.assertRegex(viewers(card, card)[2], "same viewer")
        self.assertNotIn("differs", viewers(card, card)[2])
        # One run of a result that fell back makes the result one of two viewers.
        with self.assertRaisesRegex(ValueError, "more than one viewer.*no render node open"):
            judge(card[:4] + software[:1], card, 5)
        # A result from before the read says nothing about it.
        self.assertNotIn("OpenGL", viewers(self.result(viewer_ran(PLUGIN, "", FIXTURE)), card)[0])

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
