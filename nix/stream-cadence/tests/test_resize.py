"""The sender fixture changes its picture size in the middle of a stream."""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import sender  # noqa: E402
import test_private_sender  # noqa: E402

TESTS = Path(__file__).resolve().parent


class Resize(test_private_sender.Machine):
    def setUp(self):
        super().setUp()
        program = self.peers / "motion"
        program.write_text(
            f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "fixture-peer.py"} {self.peers} motion "$@"\n'
        )
        program.chmod(0o755)

    def started(self, session: sender.Session) -> None:
        self.prepare(session)
        session.handle({"operation": "start", "motion": str(self.peers / "motion")})

    def sizes(self) -> list[str]:
        return (self.peers / "motion.log").read_text().split()

    def test_the_output_takes_the_new_mode_and_the_source_starts_again_at_it(self):
        session = self.session()
        self.started(session)
        first = session.processes["source"].pid
        resized = session.handle({"operation": "resize", "width": 480, "height": 640})
        self.assertEqual((resized["display"]["rect"]["width"], resized["display"]["rect"]["height"]), (480, 640))
        # The same refresh as before: the steady mode of the stream's rate.
        self.assertEqual(resized["display"]["mode"]["refresh"], 58823)
        self.assertEqual(session.facts["display"], resized["display"])
        self.assertEqual(self.sizes(), ["640x480", "480x640"])
        self.assertNotEqual(resized["source_pid"], first)
        self.assertTrue(session.alive("source"))
        # The run goes on: a sample reads the source as running.
        self.assertIn("journal", session.handle({"operation": "sample"}))
        self.assertTrue(session.close()["pass"])

    def test_a_resize_before_the_source_started_is_refused(self):
        session = self.session()
        self.prepare(session)
        with self.assertRaisesRegex(RuntimeError, "a resize follows start"):
            session.handle({"operation": "resize", "width": 480, "height": 640})
        self.assertFalse((self.peers / "output-mode").exists())

    def test_a_size_outside_the_output_limits_is_refused(self):
        session = self.session()
        self.started(session)
        for width, height in ((0, 640), (480, 20000)):
            with self.assertRaisesRegex(ValueError, "1 to 16384 pixels"):
                session.handle({"operation": "resize", "width": width, "height": height})
        self.assertEqual(self.sizes(), ["640x480"])


if __name__ == "__main__":
    unittest.main()
