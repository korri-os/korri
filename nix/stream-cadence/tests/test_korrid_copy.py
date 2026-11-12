"""viewer.py holds a copy of korrid's picture-path table. This test reads korrid's own file.

It runs in the source gate, which the app build depends on, so a change of
korrid's table fails `nix run .#stream-cadence` and not only the test build.
A stale copy that says "no hardware path" would also switch off the rules
that keep a software viewer away from a handheld.

It imports only the standard library and viewer.py.
"""

import os
from pathlib import Path
import re
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
# The build passes korrid's file in; a checkout has it at its own place.
KORRID_VIEWER = Path(
    os.environ.get(
        "CADENCE_KORRID_VIEWER",
        TESTS.parents[2] / "services/korrid/src/linux_viewer/production.rs",
    )
)


def korrid_picture_paths(source: str) -> dict:
    """What korrid's platform_from_path decides, read from its source.

    `branches` are the chip sets with their picture path, in order. `default`
    is the last arm. `early` are the returns before the table: a device tree
    that is absent or unreadable. `whole` says that each chip is compared as a
    whole value with `==`, as viewer.py compares it.
    """
    body = source.split("fn platform_from_path", 1)[1].split("\n}\n", 1)[0]
    head, table = body.split("let mut values", 1)
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
    return {
        "branches": branches,
        "default": default,
        "early": re.findall(r'return "([^"]+)"', head),
        "whole": len(re.findall(r'b"[^"]+"', table)) == len(re.findall(r'value == b"[^"]+"', table))
        and not re.search(r"starts_with|ends_with|contains\(|find\(", table),
    }


class KorridCopy(unittest.TestCase):
    def test_picture_path_table_equals_korrids(self):
        korrid = korrid_picture_paths(KORRID_VIEWER.read_text())
        self.assertGreaterEqual(
            len(korrid["branches"]), 1, "cannot read korrid's chip table any more"
        )
        self.assertEqual(korrid["default"], viewer.PICTURE_PATH_DEFAULT)
        self.assertEqual(
            [(set(chips), name) for chips, name in viewer.PICTURE_PATHS], korrid["branches"]
        )
        for chips, name in korrid["branches"]:
            for chip in chips:
                self.assertEqual(viewer.picture_path(b"vendor,board\0" + chip + b"\0"), name)
        self.assertEqual(viewer.picture_path(b"vendor,board\0vendor,chip\0"), korrid["default"])

    def test_a_device_tree_that_is_absent_gets_korrids_answer(self):
        korrid = korrid_picture_paths(KORRID_VIEWER.read_text())
        # korrid returns before its table for a file that is absent or unreadable.
        self.assertGreaterEqual(len(korrid["early"]), 2, "cannot read korrid's early returns")
        self.assertEqual(set(korrid["early"]), {viewer.PICTURE_PATH_DEFAULT})
        self.assertEqual(viewer.picture_path(b""), viewer.PICTURE_PATH_DEFAULT)

    def test_chips_are_compared_as_whole_values_on_both_sides(self):
        self.assertTrue(korrid_picture_paths(KORRID_VIEWER.read_text())["whole"])
        # A longer or a shorter name is another chip for viewer.py too.
        for chips, _ in viewer.PICTURE_PATHS:
            for chip in chips:
                self.assertEqual(
                    viewer.picture_path(chip + b"-next\0"), viewer.PICTURE_PATH_DEFAULT
                )
                self.assertEqual(
                    viewer.picture_path(b"x" + chip + b"\0"), viewer.PICTURE_PATH_DEFAULT
                )

    def test_reader_sees_each_kind_of_change_in_korrid(self):
        source = KORRID_VIEWER.read_text()
        same = korrid_picture_paths(source)
        changes = {
            "a chip": ('b"rockchip,rk3566"', 'b"rockchip,rk3568"'),
            "the answer for an absent device tree": (
                'ErrorKind::NotFound => return "x11"',
                'ErrorKind::NotFound => return "v4l2request"',
            ),
            "a prefix match": (
                'value == b"qcom,sm8550"',
                'value.starts_with(b"qcom,sm8550")',
            ),
        }
        for what, (old, new) in changes.items():
            with self.subTest(change=what):
                self.assertIn(old, source)
                self.assertNotEqual(korrid_picture_paths(source.replace(old, new)), same)


if __name__ == "__main__":
    unittest.main()
