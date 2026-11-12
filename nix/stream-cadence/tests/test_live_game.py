"""The private pair beside a live game: it runs, and the result says so.

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

from cli import parser  # noqa: E402
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
        # The input-device watch still ends a run.
        pair = self.sender_session()
        self.prepare_sender(pair)
        (sender.INPUT_DEVICES / "event9").touch()
        with self.assertRaisesRegex(RuntimeError, "New input devices"):
            pair.check()


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
