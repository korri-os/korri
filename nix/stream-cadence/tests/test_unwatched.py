"""A test pattern goes only on a display that the device marks unwatched (CONTEXT.md)."""

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import device  # noqa: E402
from discovery import watched_display  # noqa: E402
import heat  # noqa: E402
import machine  # noqa: E402
import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
# KORRID_DISPLAYS as korrid has it in its environment on the two devices.
AKA = '[{"height":1080,"main":true,"mode":"1920x1080@60Hz","name":"HDMI-A-1","touch":null,"unwatched":true,"width":1920,"x":0,"y":0}]'
IBUKI = '[{"height":2160,"main":true,"mode":"3840x2160@60Hz","name":"DP-1","touch":null,"unwatched":false,"width":3840,"x":0,"y":0}]'


class TheFact(unittest.TestCase):
    def test_a_dummy_plug_that_the_device_marks_unwatched_may_show_the_pattern(self):
        self.assertIsNone(watched_display(AKA, "HDMI-A-1"))

    def test_a_display_that_is_not_marked_unwatched_is_refused(self):
        refused = watched_display(IBUKI, "DP-1")
        self.assertRegex(refused, "DP-1.*not marked unwatched")
        self.assertIn("a person may be looking at this display", refused)
        # The way to measure on such a device.
        self.assertIn("private pair", refused)
        self.assertIn("--sender-fixture", refused)

    def test_a_fact_that_cannot_be_read_is_refused_and_says_what_was_looked_for(self):
        cases = {
            "no KORRID_DISPLAYS": None,
            "not JSON": "DP-1:unwatched",
            "no list": '{"name":"DP-1","unwatched":true}',
            "another display": AKA,
            "no unwatched key": '[{"name":"DP-1","main":true}]',
            "unwatched is no boolean": '[{"name":"DP-1","unwatched":"yes"}]',
        }
        for name, facts in cases.items():
            with self.subTest(case=name):
                refused = watched_display(facts, "DP-1")
                self.assertRegex(refused, "KORRID_DISPLAYS.*korrid's environment")
                self.assertIn("DP-1", refused)
                self.assertIn("does not guess", refused)

    def test_two_entries_for_one_display_are_refused(self):
        # The first entry must not decide: [true, false] would let the pattern on.
        for values in ((True, False), (False, True), (True, True)):
            with self.subTest(values=values):
                facts = json.dumps([{"name": "DP-1", "unwatched": value} for value in values])
                refused = watched_display(facts, "DP-1")
                self.assertRegex(refused, "more than one entry")
                self.assertIn("does not guess", refused)
        # Two displays with their own names are no duplicate.
        two = json.dumps([{"name": "DP-1", "unwatched": True}, {"name": "DP-2", "unwatched": False}])
        self.assertIsNone(watched_display(two, "DP-1"))

    def test_only_true_is_unwatched(self):
        for value in (False, None, 0, 1, "true"):
            with self.subTest(value=value):
                facts = json.dumps([{"name": "eDP-1", "unwatched": value}])
                self.assertIsNotNone(watched_display(facts, "eDP-1"))


class ProductSender(unittest.TestCase):
    """device.py's Session: `run`, and `view` against a product sender."""

    def session(self, watched: str | None) -> device.Session:
        session = object.__new__(device.Session)
        session.root, session.source, session.source_log, session.revoke = None, None, None, None
        # A compositor that does not exist: an agent that went on would fail to call it.
        session.facts = {
            "watched_display": watched,
            "working_encoders": ["h264_vaapi"],
            "processes": {"compositor": {"exe": "/no-such/sway", "environment": {"SWAYSOCK": "/no-such"}}},
        }
        return session

    def test_nothing_wakes_or_starts_on_a_watched_display(self):
        refused = watched_display(IBUKI, "DP-1")
        for operation in ("wake", "provision", "start", "sample"):
            with self.subTest(operation=operation):
                session = self.session(refused)
                with self.assertRaisesRegex(RuntimeError, "a person may be looking at this display"):
                    session.handle({"operation": operation, "hostUuid": "x", "certificate": "x", "motion": "/no-such"})
                self.assertIsNone(session.root)
                self.assertIsNone(session.revoke)

    def test_an_unwatched_display_goes_on_to_the_compositor(self):
        session = self.session(None)
        # It reaches the call of the compositor's swaymsg, which this test has not.
        with self.assertRaises(OSError):
            session.handle({"operation": "wake"})


class InstalledViewer(unittest.TestCase):
    def test_the_installed_viewer_is_refused_on_a_watched_display_before_anything(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            peer = root / "peer"
            peer.write_text(f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "host-peer.py"} "$@"\n')
            peer.chmod(0o755)
            previous = (heat.THERMAL, viewer.COMPATIBLE)
            heat.THERMAL, viewer.COMPATIBLE = root / "thermal", root / "compatible"
            self.addCleanup(setattr, heat, "THERMAL", previous[0])
            self.addCleanup(setattr, viewer, "COMPATIBLE", previous[1])
            machine.power_supplies(self, root)
            session = viewer.Session(str(peer))
            self.addCleanup(session.close)
            self.assertIsNone(session.facts["watched_display"])
            session.facts["watched_display"] = watched_display(IBUKI, "DP-1")
            with self.assertRaisesRegex(RuntimeError, "a person may be looking at this display"):
                session.handle(
                    {"operation": "prepare", "fixture": False, "server_certificate": "public"}
                )
            self.assertIsNone(session.root)
            # A display that the agent could not read is not an unwatched display:
            # a device with two outputs must not reach the wake.
            session.facts["watched_display"] = None
            session.facts["display"] = None
            session.facts["display_unread"] = "need exactly one active compositor output"
            session.compositor = {"uid": os.getuid(), "gid": os.getgid(), "user": "tester"}
            with self.assertRaisesRegex(RuntimeError, "own display could not be read.*exactly one active"):
                session.handle(
                    {"operation": "prepare", "fixture": False, "server_certificate": "public"}
                )
            self.assertIsNone(session.root)
            session.compositor = None
            # The fixture uses no display of the device: the fact does not stop it.
            prepared = session.handle(
                {
                    "operation": "prepare", "fixture": True, "width": 640, "height": 480,
                    "refresh_hz": 50.0, "certificate": None, "key": None, "server_certificate": "public",
                }
            )
            self.assertTrue(Path(prepared["run_folder"]).is_dir())


if __name__ == "__main__":
    unittest.main()
