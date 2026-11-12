"""The handheld path and the points of the first review: copies, secrets, choices."""

import hashlib
import json
import os
from pathlib import Path
import re
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


def korrid_picture_paths(source: str) -> tuple[list[tuple[set[bytes], str]], str | None]:
    """The chip table of korrid's platform_from_path: branches in order, then the default."""
    body = source.split("fn platform_from_path", 1)[1].split("\n}\n", 1)[0]
    table = body.split("let mut values", 1)[1]
    branches: list[tuple[set[bytes], str]] = []
    chips: set[bytes] = set()
    default = None
    for token in re.finditer(r'b"([^"]+)"|^\s*"([^"]+)"\s*$', table, re.MULTILINE):
        if token.group(1):
            chips.add(token.group(1).encode())
        elif chips:
            branches.append((chips, token.group(2)))
            chips = set()
        else:
            default = token.group(2)
    return branches, default


class KorridCopy(unittest.TestCase):
    def test_picture_path_table_equals_korrids(self):
        branches, default = korrid_picture_paths(KORRID_VIEWER.read_text())
        self.assertGreaterEqual(len(branches), 1, "cannot read korrid's chip table any more")
        self.assertEqual(default, viewer.PICTURE_PATH_DEFAULT)
        self.assertEqual(
            [(set(chips), name) for chips, name in viewer.PICTURE_PATHS], branches
        )
        for chips, name in branches:
            for chip in chips:
                self.assertEqual(viewer.picture_path(b"vendor,board\0" + chip + b"\0"), name)
        self.assertEqual(viewer.picture_path(b"vendor,board\0vendor,chip\0"), default)
        self.assertEqual(viewer.picture_path(b""), default)

    def test_table_reader_sees_a_changed_korrid(self):
        source = KORRID_VIEWER.read_text()
        changed = source.replace('b"rockchip,rk3566"', 'b"rockchip,rk3568"')
        self.assertNotEqual(source, changed)
        self.assertNotEqual(korrid_picture_paths(source), korrid_picture_paths(changed))


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
