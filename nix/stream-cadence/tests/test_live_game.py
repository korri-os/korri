"""The private pair beside a live game: only by an explicit option, and on record.

A stand-in `systemctl` on PATH answers `list-units` with the units that the
test names, so the real game check of both agents runs. Whether an agent is
on a Korri device needs root and a product compositor; the test says so
through the agent's own flag for it.
"""

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cli import Refusal, live_game_refusal, parser, view  # noqa: E402
import device  # noqa: E402
import heat  # noqa: E402
import machine  # noqa: E402
import sender  # noqa: E402
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


class WithoutTheOption(Device):
    def test_a_live_game_refuses_the_start_and_ends_a_run(self):
        self.units.write_text(LINE)
        self.assertEqual(device.live_games(), [GAME])
        with self.assertRaisesRegex(RuntimeError, "korri-game-\\* unit is active"):
            self.prepare_sender(self.sender_session())
        with self.assertRaisesRegex(RuntimeError, "korri-game-\\* unit is active"):
            self.prepare_viewer(self.viewer_session())
        # A game that starts while the pair runs ends it.
        self.units.write_text("")
        pair = self.sender_session()
        self.prepare_sender(pair)
        self.units.write_text(LINE)
        pair.last_game_check = 0.0
        with self.assertRaisesRegex(RuntimeError, "korri-game-\\* unit is active"):
            pair.check()
        watched = self.viewer_session()
        self.units.write_text("")
        self.prepare_viewer(watched)
        self.units.write_text(LINE)
        watched.last_game_check = 0.0
        with self.assertRaisesRegex(RuntimeError, "korri-game-\\* unit is active"):
            watched.handle({"operation": "sample"})


class WithTheOption(Device):
    def test_the_pair_runs_beside_a_game_and_the_result_names_the_units(self):
        self.units.write_text(LINE)
        pair = self.sender_session()
        built = self.prepare_sender(pair, allow_game=True)
        self.assertEqual(built["facts"]["live_games_at_start"], [GAME])
        pair.last_game_check = 0.0
        pair.check()
        watched = self.viewer_session()
        prepared = self.prepare_viewer(watched, allow_game=True)
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

    def test_every_other_rule_stays_on(self):
        self.units.write_text(LINE)
        # A battery, a hot host and an open uinput still refuse the sender fixture.
        machine.supply(device.POWER_SUPPLY, "BAT0", "Battery")
        with self.assertRaisesRegex(RuntimeError, "battery"):
            self.prepare_sender(self.sender_session(), allow_game=True)
        with self.assertRaisesRegex(RuntimeError, "battery"):
            self.prepare_viewer(self.viewer_session(), allow_game=True)
        for entry in (device.POWER_SUPPLY / "BAT0").iterdir():
            entry.unlink()
        (device.POWER_SUPPLY / "BAT0").rmdir()
        sender.UINPUT.touch()
        with self.assertRaisesRegex(RuntimeError, "uinput"):
            self.prepare_sender(self.sender_session(), allow_game=True)
        sender.UINPUT.unlink()
        # The input-device watch still ends a run.
        pair = self.sender_session()
        self.prepare_sender(pair, allow_game=True)
        (sender.INPUT_DEVICES / "event9").touch()
        with self.assertRaisesRegex(RuntimeError, "New input devices"):
            pair.check()

    def test_the_installed_viewer_never_runs_beside_a_game(self):
        watched = self.viewer_session()
        with self.assertRaisesRegex(RuntimeError, "only .* fixture"):
            watched.handle(
                {"operation": "prepare", "fixture": False, "server_certificate": "public", "allow_game": True}
            )
        self.assertIsNone(watched.root)


class TheOptionIsRefusedElsewhere(unittest.TestCase):
    plain = {"batteries": [], "hardware_picture_path": False, "plugin": None}

    def test_only_for_two_fixtures_on_a_mains_device_with_no_hardware_path(self):
        ok = dict(sender_fixture=True, sender=self.plain, viewer=self.plain)
        self.assertIsNone(live_game_refusal(**ok))
        cases = {
            "--sender-fixture": dict(ok, sender_fixture=False),
            "installed viewer": dict(ok, viewer=dict(self.plain, plugin={"moonlight": "/x"})),
            "viewer .* battery": dict(ok, viewer=dict(self.plain, batteries=["BAT0"])),
            "sender .* battery": dict(ok, sender=dict(self.plain, batteries=["BAT0"])),
            "viewer .* hardware picture path": dict(
                ok, viewer=dict(self.plain, hardware_picture_path=True)
            ),
            "sender .* hardware picture path": dict(
                ok, sender=dict(self.plain, hardware_picture_path=True)
            ),
        }
        for words, case in cases.items():
            with self.subTest(case=words):
                self.assertRegex(live_game_refusal(**case), words)

    def test_the_tool_refuses_the_option_without_the_sender_fixture_before_anything(self):
        args = parser().parse_args(
            ["view", "--sender", "operator@sender", "--viewer", "operator@viewer", "--allow-live-game"]
        )
        self.assertTrue(args.allow_live_game)
        with self.assertRaisesRegex(Refusal, "--allow-live-game.*--sender-fixture"):
            view(args)
        self.assertFalse(
            parser().parse_args(["view", "--sender", "a", "--viewer", "b"]).allow_live_game
        )


if __name__ == "__main__":
    unittest.main()
