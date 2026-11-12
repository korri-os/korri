"""The private pair beside a live game: it runs, and the result says so.

A stand-in `systemctl` on PATH answers `list-units` with the units that the
test names, so the real game check of both agents runs. Whether an agent is
on a Korri device needs root and a product compositor; the test says so
through the agent's own flag for it.
"""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cli import parser  # noqa: E402
import heat  # noqa: E402
import machine  # noqa: E402
import sender  # noqa: E402
from verdict import judge  # noqa: E402
import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
GAME = "korri-game-0123456789abcdef.service"
# As systemd prints it: the unit, its state, and the command with its arguments.
LINE = f"{GAME} loaded active running [systemd-run] /nix/store/x-korrid/bin/korrid plugin-launch secret-argument\n"


class Device(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        place = Path(self.directory.name)
        self.units = place / "units"
        self.units.write_text("")
        tools = place / "tools"
        tools.mkdir()
        (tools / "systemctl").write_text(f'#!/bin/sh\nexec cat "{self.units}"\n')
        (tools / "systemctl").chmod(0o755)
        path = os.environ["PATH"]
        os.environ["PATH"] = f"{tools}:{path}"
        self.addCleanup(os.environ.__setitem__, "PATH", path)
        machine.power_supplies(self, place)
        real = (
            heat.THERMAL, viewer.COMPATIBLE, sender.RENDER_NODES, sender.INPUT_DEVICES, sender.UINPUT,
        )
        self.addCleanup(self.restore, real)
        self.peers, dri, inputs = place / "bin", place / "dri", place / "input"
        for made in (self.peers, dri, inputs):
            made.mkdir()
        (dri / "renderD1").touch()
        (self.peers / "hardware-nodes").write_text(str(dri / "renderD1"))
        for role in ("sway", "swaymsg", "sunshine"):
            program = self.peers / role
            program.write_text(
                f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "fixture-peer.py"} {self.peers} {role} "$@"\n'
            )
            program.chmod(0o755)
        (self.peers / "viewer-sway").write_text(
            f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "host-peer.py"} "$@"\n'
        )
        (self.peers / "viewer-sway").chmod(0o755)
        heat.THERMAL, viewer.COMPATIBLE = place / "thermal", place / "compatible"
        sender.RENDER_NODES, sender.INPUT_DEVICES, sender.UINPUT = dri, inputs, place / "uinput"
        self.addCleanup(setattr, sender, "UHID", sender.UHID)
        sender.UHID = place / "uhid"

    @staticmethod
    def restore(real) -> None:
        (
            heat.THERMAL, viewer.COMPATIBLE, sender.RENDER_NODES, sender.INPUT_DEVICES, sender.UINPUT,
        ) = real

    def sender_session(self) -> sender.Session:
        session = sender.Session()
        self.addCleanup(session.close)
        # What a root agent on a Korri device sets by itself.
        session.watch_games = True
        return session

    def viewer_session(self) -> viewer.Session:
        session = viewer.Session(str(self.peers / "viewer-sway"))
        self.addCleanup(session.close)
        session.watch_games = True
        return session

    def prepare_sender(self, session: sender.Session, **extra) -> dict:
        return session.handle(
            {
                "operation": "prepare", "sway": str(self.peers / "sway"),
                "sunshine": str(self.peers / "sunshine"), "width": 640, "height": 480,
                "fps": 60.024, **extra,
            }
        )

    def prepare_viewer(self, session: viewer.Session, **extra) -> dict:
        return session.handle(
            {
                "operation": "prepare", "fixture": True, "width": 640, "height": 480,
                "refresh_hz": 50.0, "certificate": None, "key": None,
                "server_certificate": "public", **extra,
            }
        )


class APrivatePairBesideAGame(Device):
    """A private pair does not use the device's display, so a game does not stop it."""

    def test_the_pair_runs_beside_a_game_and_the_result_names_the_units(self):
        self.units.write_text(LINE)
        pair = self.sender_session()
        built = self.prepare_sender(pair)
        self.assertEqual(built["facts"]["live_games_at_start"], [GAME])
        pair.check()
        watched = self.viewer_session()
        prepared = self.prepare_viewer(watched, private_pair=True)
        self.assertEqual(prepared["live_games"], [GAME])
        watched.last_game_check = 0.0
        watched.handle({"operation": "sample"})
        # The game ends before the pair does: the end is on record too.
        self.units.write_text("")
        closed = [pair.close(), watched.close()]
        self.assertEqual([row["live_games"] for row in closed], [[], []])
        self.assertTrue(all(row["pass"] for row in closed), closed)
        # Unit names only: no command line of the game is kept.
        self.assertNotIn("secret-argument", json.dumps([built, prepared, closed]))

    def test_a_pair_with_no_game_records_that_too(self):
        pair = self.sender_session()
        self.assertEqual(self.prepare_sender(pair)["facts"]["live_games_at_start"], [])
        watched = self.viewer_session()
        self.assertEqual(self.prepare_viewer(watched, private_pair=True)["live_games"], [])
        self.assertEqual(pair.close()["live_games"], [])
        self.assertEqual(watched.close()["live_games"], [])

    def test_every_other_rule_stays_on(self):
        self.units.write_text(LINE)
        sender.UINPUT.touch()
        with self.assertRaisesRegex(RuntimeError, "uinput"):
            self.prepare_sender(self.sender_session())
        sender.UINPUT.unlink()
        # The heat guard still refuses a hot host.
        zone = heat.THERMAL / "thermal_zone0"
        zone.mkdir(parents=True)
        (zone / "type").write_text("cpu-thermal\n")
        (zone / "trip_point_0_type").write_text("critical\n")
        (zone / "trip_point_0_temp").write_text("95000\n")
        (zone / "temp").write_text("90000\n")
        with self.assertRaisesRegex(RuntimeError, "The device is hot"):
            self.prepare_sender(self.sender_session())


class AGameThatStartsDuringARun(Device):
    """A game's controllers add entries under /dev/input. They are a record, not a fault."""

    def pair(self) -> sender.Session:
        (sender.INPUT_DEVICES / "event0").touch()
        (sender.INPUT_DEVICES / "js0").touch()
        session = self.sender_session()
        built = self.prepare_sender(session)
        # The fixture's user cannot open uinput: the probe was made and it failed.
        self.assertEqual(built["facts"]["input_rule"], "record")
        return session

    def test_new_input_entries_do_not_end_the_run_and_do_not_fail_the_restore(self):
        session = self.pair()
        session.check()
        # A game starts: korrid's input stack makes its virtual devices.
        for name in ("event17", "event18", "mouse2", "mouse3"):
            (sender.INPUT_DEVICES / name).touch()
        session.check()
        # A controller is unplugged, and one of the game's devices goes again.
        (sender.INPUT_DEVICES / "js0").unlink()
        (sender.INPUT_DEVICES / "mouse3").unlink()
        session.check()
        restore = session.close()
        self.assertTrue(restore["pass"], restore)
        self.assertEqual(restore["errors"], [])
        # Everything that appeared or disappeared at any moment of the run is kept.
        self.assertEqual(restore["new_input_devices"], ["event17", "event18", "mouse2", "mouse3"])
        self.assertEqual(restore["gone_input_devices"], ["js0", "mouse3"])
        self.assertEqual(restore["input_rule"], "record")

    def test_the_fixture_still_does_not_start_where_its_user_can_open_uinput(self):
        sender.UINPUT.touch()
        with self.assertRaisesRegex(RuntimeError, "can open .*uinput.*does not start"):
            self.prepare_sender(self.sender_session())

    def test_the_fixture_does_not_start_where_its_user_can_open_uhid(self):
        # Sunshine makes its DualSense pad through /dev/uhid, not through uinput.
        sender.UHID.touch()
        with self.assertRaisesRegex(RuntimeError, "can open .*uhid.*does not start"):
            self.prepare_sender(self.sender_session())

    def test_a_uhid_probe_that_could_not_be_made_keeps_the_strict_rule(self):
        session = self.sender_session()
        session.probe = lambda node, how: None if node == sender.UHID else node != sender.UINPUT
        built = self.prepare_sender(session)
        self.assertRegex(built["facts"]["input_rule"], "^strict: .*uhid.*could not be tried")

    def test_a_probe_that_could_not_be_made_keeps_the_strict_rule_and_says_so(self):
        session = self.sender_session()
        # The child that tries the node as the fixture's user did not run.
        session.probe = lambda node, how: None if node == sender.UINPUT else node != sender.UHID
        built = self.prepare_sender(session)
        self.assertRegex(built["facts"]["input_rule"], "^strict: .*uinput.*could not be tried")
        (sender.INPUT_DEVICES / "event17").touch()
        with self.assertRaisesRegex(RuntimeError, "New input devices appeared.*event17"):
            session.check()
        restore = session.close()
        self.assertFalse(restore["pass"])
        self.assertEqual(restore["errors"], ["new input devices: event17"])
        self.assertRegex(restore["input_rule"], "^strict")

    def test_the_probe_tells_cannot_open_from_could_not_try(self):
        session = self.sender_session()
        node = sender.UINPUT
        self.assertIs(session.probe(node, os.O_WRONLY), False)
        node.touch()
        self.assertIs(session.probe(node, os.O_WRONLY), True)
        real = sender.subprocess.run
        self.addCleanup(setattr, sender.subprocess, "run", real)

        def broken(*_args, **_kwargs):
            raise OSError("no child")

        sender.subprocess.run = broken
        self.assertIsNone(session.probe(node, os.O_WRONLY))
        # Python ends with 1 for any uncaught exception and with 0 for a script that
        # did nothing: neither is the child's answer.
        for code in (0, 1, 2, -9):
            with self.subTest(code=code):
                sender.subprocess.run = lambda *a, code=code, **k: subprocess.CompletedProcess(a, code)
                self.assertIsNone(session.probe(node, os.O_WRONLY))
        sender.subprocess.run = lambda *a, **k: subprocess.CompletedProcess(a, sender.PROBE_OPENED)
        self.assertIs(session.probe(node, os.O_WRONLY), True)
        sender.subprocess.run = lambda *a, **k: subprocess.CompletedProcess(a, sender.PROBE_REFUSED)
        self.assertIs(session.probe(node, os.O_WRONLY), False)
        self.assertNotIn(sender.PROBE_OPENED, (0, 1, 2))
        self.assertNotIn(sender.PROBE_REFUSED, (0, 1, 2))
        sender.subprocess.run = lambda *a, **k: subprocess.CompletedProcess(a, 1)
        # A render node is offered only when the probe says yes.
        self.assertFalse(session.opens(node, os.O_WRONLY))

    def test_an_input_entry_during_a_run_makes_the_result_beside_a_game(self):
        def result(**live) -> list[dict]:
            window = {
                "latency_ms": {"p50": 20.0, "p95": 22.0}, "capture_misses": 0,
                "repeated_frames": 0, "late_frames": 0, "torn_frames": 0, "undecodable_frames": 0,
            }
            moments = {"sender_at_start": [], "viewer_at_start": [], "sender_at_end": [], "viewer_at_end": []}
            facts = {
                "valid": True, "model": "host", "viewer": {"model": "host"}, "max_capture_gaps": 0,
                "live_game": {**moments, "input_devices_appeared": [], "input_devices_disappeared": [], **live},
            }
            return [
                {"discovered": dict(facts), "source_counter": {"windows": [window]}, "viewer": {"startup_ms": 900.0}}
                for _ in range(5)
            ]

        free = result()
        # A game that started and ended between the two reads of the units.
        between = result(input_devices_appeared=["event17", "mouse2"], input_devices_disappeared=["event17", "mouse2"])
        with self.assertRaisesRegex(ValueError, "beside_a_game"):
            judge(free, between, 5)
        with self.assertRaisesRegex(ValueError, "beside_a_game"):
            judge(free, result(input_devices_disappeared=["js0"]), 5)
        self.assertEqual(len(judge(between, result(sender_at_start=[GAME]), 5)), 15)
        self.assertEqual(len(judge(free, free, 5)), 15)


class WhatUsesTheDisplayStillRefuses(Device):
    def test_fixture_viewer_of_a_product_sender_refuses_and_ends_for_a_game(self):
        # Not a private pair: the stream comes from the product Sunshine.
        self.units.write_text(LINE)
        with self.assertRaisesRegex(RuntimeError, "korri-game-\\* unit is active"):
            self.prepare_viewer(self.viewer_session())
        self.units.write_text("")
        watched = self.viewer_session()
        self.prepare_viewer(watched)
        self.units.write_text(LINE)
        watched.last_game_check = 0.0
        with self.assertRaisesRegex(RuntimeError, "korri-game-\\* unit is active"):
            watched.handle({"operation": "sample"})

    def test_installed_viewer_refuses_for_a_game_also_with_a_private_sender(self):
        self.units.write_text(LINE)
        watched = self.viewer_session()
        with self.assertRaisesRegex(RuntimeError, "korri-game-\\* unit is active"):
            watched.handle(
                {"operation": "prepare", "fixture": False, "server_certificate": "public", "private_pair": True}
            )
        self.assertIsNone(watched.root)

    def test_no_option_switches_the_refusal_off(self):
        with self.assertRaises(SystemExit):
            parser().parse_args(["view", "--sender", "a", "--viewer", "b", "--allow-live-game"])


if __name__ == "__main__":
    unittest.main()
