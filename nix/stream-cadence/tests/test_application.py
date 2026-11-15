"""The sender fixture's one application: `Desktop`, or korrid's `Korri Stream` on request."""

import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import sender  # noqa: E402
import test_private_sender  # noqa: E402


class Application(test_private_sender.Machine):
    def prepare_with(self, session: sender.Session, **extra) -> dict:
        return session.handle(
            {
                "operation": "prepare",
                "sway": str(self.peers / "sway"),
                "sunshine": str(self.peers / "sunshine"),
                "width": 640,
                "height": 480,
                "fps": 60.024,
                **extra,
            }
        )

    def applications(self, session: sender.Session) -> list[str]:
        assert session.root is not None
        listed = json.loads((session.home() / "apps.json").read_text())
        return [app["name"] for app in listed["apps"]]

    def test_without_a_request_the_one_application_is_desktop(self):
        session = self.session()
        facts = self.prepare_with(session)["facts"]
        self.assertEqual(self.applications(session), ["Desktop"])
        self.assertEqual(facts["sender_fixture"]["application"], "Desktop")
        self.assertTrue(session.close()["pass"])

    def test_korrid_asks_for_korri_stream_and_that_is_the_one_application(self):
        # services/korrid/src/linux_viewer/production.rs: korrid's viewer asks
        # for this application; the Android app's holder proof asks for it.
        session = self.session()
        facts = self.prepare_with(session, application="Korri Stream")["facts"]
        self.assertEqual(self.applications(session), ["Korri Stream"])
        self.assertEqual(facts["sender_fixture"]["application"], "Korri Stream")
        self.assertTrue(session.close()["pass"])

    def test_another_application_is_refused_before_anything_starts(self):
        session = self.session()
        for name in ("Steam", "", "Korri Stream\n"):
            with self.assertRaisesRegex(ValueError, "Desktop or Korri Stream"):
                self.prepare_with(session, application=name)
        self.assertIsNone(session.root)
        self.assertFalse((self.peers / "arguments.log").exists())


if __name__ == "__main__":
    unittest.main()
