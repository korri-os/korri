"""The sender fixture's tone: a private sound server, only when asked for."""

import json
from pathlib import Path
import re
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import cli  # noqa: E402
from cli import Refusal, parser  # noqa: E402
import sender  # noqa: E402
import test_private_sender  # noqa: E402
from test_net import run  # noqa: E402
from verdict import conditions, judge  # noqa: E402

TESTS = Path(__file__).resolve().parent


class Tone(test_private_sender.Machine):
    def setUp(self):
        super().setUp()
        program = self.peers / "pulseaudio"
        program.write_text(
            f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "fixture-peer.py"} {self.peers} pulseaudio "$@"\n'
        )
        program.chmod(0o755)

    def prepare_with_tone(self, session: sender.Session, **extra) -> dict:
        return session.handle(
            {
                "operation": "prepare",
                "sway": str(self.peers / "sway"),
                "sunshine": str(self.peers / "sunshine"),
                "width": 640,
                "height": 480,
                "fps": 60.024,
                "pulseaudio": str(self.peers / "pulseaudio"),
                **extra,
            }
        )

    def started(self) -> list[list[str]]:
        return [json.loads(line) for line in (self.peers / "arguments.log").read_text().splitlines()]

    def environments(self) -> dict[str, dict]:
        rows = [json.loads(line) for line in (self.peers / "environment.log").read_text().splitlines()]
        return {role: sound for role, sound in rows}

    def test_without_a_tone_nothing_of_sound_starts_and_sunshine_gets_no_server(self):
        session = self.session()
        facts = self.prepare(session)["facts"]
        self.assertIsNone(facts["tone"])
        self.assertNotIn("sound", session.processes)
        self.assertNotIn("pulseaudio", [row[0] for row in self.started()])
        self.assertNotIn("PULSE_SERVER", self.environments()["sunshine"])
        self.assertTrue(session.close()["pass"])

    def test_a_tone_is_a_null_sink_with_a_sine_in_a_private_server_before_sunshine(self):
        session = self.session()
        facts = self.prepare_with_tone(session, tone_hz=1000)["facts"]
        self.assertEqual(facts["tone"], {"hz": 1000, "sink": "korri-tone"})
        assert session.root is not None
        root = session.root
        roles = [row[0] for row in self.started()]
        self.assertLess(roles.index("pulseaudio"), roles.index("sunshine"))
        arguments = next(row[1:] for row in self.started() if row[0] == "pulseaudio")
        for flag in ("-n", "--disallow-module-loading", "--system=no", "--daemonize=no", "--exit-idle-time=-1"):
            self.assertIn(flag, arguments)
        # The script loads one socket in the run's rt, a null sink and a sine:
        # no module that opens a sound card, Bluetooth or a bus.
        script = Path(arguments[arguments.index("-F") + 1])
        self.assertEqual(script.parent, root)
        modules = re.findall(r"^load-module (\S+)", script.read_text(), re.MULTILINE)
        self.assertEqual(modules, ["module-native-protocol-unix", "module-null-sink", "module-sine"])
        self.assertIn(f"socket={root / 'rt' / 'pulse-native'} ", script.read_text())
        self.assertIn("module-sine sink=korri-tone frequency=1000\n", script.read_text())
        self.assertIn("set-default-sink korri-tone\n", script.read_text())
        # The server reaches no bus; Sunshine reaches the server and starts none.
        sound, sunshine = self.environments()["pulseaudio"], self.environments()["sunshine"]
        for name in ("DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS"):
            self.assertTrue(sound[name].startswith(f"unix:path={root / 'rt'}/"), sound)
        self.assertEqual(sunshine["PULSE_SERVER"], f"unix:{root / 'rt' / 'pulse-native'}")
        self.assertEqual(Path(sunshine["PULSE_CLIENTCONFIG"]).read_text(), "autospawn = no\n")
        process = session.processes["sound"]
        session.check()
        restore = session.close()
        self.assertTrue(restore["pass"], restore)
        self.assertIsNotNone(process.poll(), "the sound server ends with the fixture")
        self.assertFalse(root.exists())

    def test_a_sound_server_that_does_not_start_stops_the_fixture_and_leaves_nothing(self):
        (self.peers / "pulseaudio-fails").touch()
        session = self.session()
        with self.assertRaisesRegex(RuntimeError, "sound server did not start.*told to fail"):
            self.prepare_with_tone(session)
        self.assertNotIn("sunshine", [row[0] for row in self.started()])
        self.assertTrue(session.close()["pass"])

    def test_a_sound_server_that_stops_during_the_run_ends_it(self):
        session = self.session()
        self.prepare_with_tone(session)
        session.processes["sound"].kill()
        session.processes["sound"].wait(timeout=5)
        with self.assertRaisesRegex(RuntimeError, "fixture's sound stopped"):
            session.check()
        self.assertTrue(session.close()["pass"])

    def test_a_tone_outside_hearing_is_refused_before_anything_starts(self):
        session = self.session()
        with self.assertRaisesRegex(ValueError, "between 20 and 20000 Hz"):
            self.prepare_with_tone(session, tone_hz=5)
        self.assertIsNone(session.root)


def toned(hz: int | None) -> dict:
    record = run()
    record["discovered"]["tone"] = None if hz is None else {"hz": hz, "sink": sender.TONE_SINK}
    return record


class ViewTone(unittest.TestCase):
    """view --tone: the stream's sound in a measure of the private pair (#391 step F)."""

    def test_the_option_names_a_tone_in_hearing(self):
        base = ["view", "--sender", "a", "--viewer", "b"]
        self.assertEqual(parser().parse_args([*base, "--tone", "1000"]).tone, 1000)
        self.assertIsNone(parser().parse_args(base).tone)
        for refused in ("10", "20001", "loud"):
            with self.assertRaises(SystemExit):
                parser().parse_args([*base, "--tone", refused])

    def test_view_refuses_a_tone_without_the_sender_fixture_before_any_device(self):
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
        args = parser().parse_args(["view", "--sender", "root@aka", "--viewer", "root@aka", "--tone", "1000"])
        with self.assertRaisesRegex(Refusal, "--tone.*--sender-fixture"):
            cli.view(args)
        self.assertEqual(asked, [])

    def test_a_tone_keeps_two_results_apart(self):
        # A result from before the tone, or with none, streamed no sound.
        self.assertIsNone(conditions(run())["tone"])
        self.assertIsNone(conditions(toned(None))["tone"])
        self.assertEqual(conditions(toned(1000))["tone"], 1000)
        with self.assertRaisesRegex(ValueError, "tone") as refused:
            judge([run() for _ in range(5)], [toned(1000) for _ in range(5)], 5)
        self.assertIn("1000 Hz; no sound", str(refused.exception))
        with self.assertRaisesRegex(ValueError, "tone"):
            judge([toned(440) for _ in range(5)], [toned(1000) for _ in range(5)], 5)
        self.assertEqual(len(judge([toned(1000) for _ in range(5)], [toned(1000) for _ in range(5)], 5)), 16)


if __name__ == "__main__":
    unittest.main()
