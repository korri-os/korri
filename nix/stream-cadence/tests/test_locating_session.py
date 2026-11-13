"""The measured start begins only when the sender has ended the locating start's session (#467)."""

from pathlib import Path
import sys
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cli  # noqa: E402
import test_private_sender  # noqa: E402


class LocatingSessionEnd(test_private_sender.Machine):
    def sender(self):
        """A real sender fixture agent with the stand-in Sunshine, streaming its source."""
        session = self.session()
        self.prepare(session)
        motion = self.peers / "motion"
        motion.write_text('#!/bin/sh\necho "PRESENT 1"\nexec sleep 60\n')
        motion.chmod(0o755)
        session.handle({"operation": "start", "motion": str(motion)})
        return session

    def stream_session(self, seconds: float) -> None:
        """Sunshine keeps a session::audio thread, as after a viewer that left before its audio ping."""
        (self.peers / "stream-session").write_text(str(seconds))
        for _ in range(500):
            if (self.peers / "stream-session-started").exists():
                return
            time.sleep(0.01)
        self.fail("the stand-in Sunshine started no session thread")

    def test_waits_until_sunshine_has_ended_the_session(self):
        session = self.sender()
        self.stream_session(1.0)
        began = time.monotonic()
        waited = cli.wait_for_session_end(lambda: session.handle({"operation": "sample"}))
        self.assertGreaterEqual(time.monotonic() - began, 0.8)
        self.assertGreaterEqual(waited["seconds"], 0.8)
        self.assertEqual(waited["threads"], ["session::audio"])
        names = [row["name"] for row in session.handle({"operation": "sample"})["threads"]]
        self.assertNotIn("session::audio", names)

    def test_a_sender_with_no_session_needs_no_wait(self):
        session = self.sender()
        waited = cli.wait_for_session_end(lambda: session.handle({"operation": "sample"}))
        self.assertEqual(waited["threads"], [])
        self.assertLess(waited["seconds"], 0.5)

    def test_a_session_that_does_not_end_is_refused_at_the_limit(self):
        session = self.sender()
        self.stream_session(60)
        elapsed = [0.0]

        def sleep(seconds):
            elapsed[0] += seconds

        with self.assertRaisesRegex(cli.Refusal, r"session::audio.*20 seconds"):
            cli.wait_for_session_end(
                lambda: session.handle({"operation": "sample"}),
                sleep=sleep, clock=lambda: elapsed[0],
            )


if __name__ == "__main__":
    unittest.main()
