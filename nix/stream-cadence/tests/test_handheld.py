"""The handheld path and the points of the first review: copies, secrets, choices."""

import hashlib
import json
import os
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import sender  # noqa: E402
import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
FIXTURES = TESTS / "fixtures" / "sender"
# The build passes korrid's file in; a checkout has it at its own place.
KORRID_VIEWER = Path(
    os.environ.get(
        "CADENCE_KORRID_VIEWER",
        TESTS.parents[2] / "services/korrid/src/linux_viewer/production.rs",
    )
)


class PairingWords(unittest.TestCase):
    def test_pin_never_leaves_in_moonlights_words(self):
        words = "Please enter the following PIN on the target PC: 4321\nFailed to pair to server\n"
        self.assertEqual(viewer.pairing_pin(words), "4321")
        self.assertNotIn("4321", viewer.without_pin(words))
        self.assertIn("Failed to pair to server", viewer.without_pin(words))


class Login(unittest.TestCase):
    def test_credentials_file_is_what_sunshine_itself_writes(self):
        # What the stock Sunshine 2026.516 wrote with --creds for this made-up
        # login. A salted hash of a test value; it opens nothing.
        real = {
            "username": "cadence-test",
            "salt": "gXniE8pDJsl0kLz2",  # gitleaks:allow
            "password": "D5F5AE1C29C578A905F778D00CEEBD7A20EFC44598C6F1442A904190A6D57629",  # gitleaks:allow
        }
        ours = json.loads(sender.credentials("cadence-test", "example-password"))
        self.assertEqual(ours.keys(), real.keys())
        self.assertEqual(ours["username"], real["username"])
        self.assertEqual(len(ours["salt"]), len(real["salt"]))
        # The same rule gives Sunshine's own hash for Sunshine's own salt.
        digest = hashlib.sha256(("example-password" + real["salt"]).encode()).digest()
        self.assertEqual(digest[::-1].hex().upper(), real["password"])
        digest = hashlib.sha256(("example-password" + ours["salt"]).encode()).digest()
        self.assertEqual(digest[::-1].hex().upper(), ours["password"])


if __name__ == "__main__":
    unittest.main()
