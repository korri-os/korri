#!/usr/bin/env python3

import importlib.util
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("shared-preferences-snapshot.py")
SPEC = importlib.util.spec_from_file_location("shared_preferences_snapshot", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class SharedPreferencesSnapshotTest(unittest.TestCase):
    def test_preserves_file_key_presence_and_all_supported_types(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "main.xml").write_text(
                """<?xml version='1.0' encoding='utf-8' standalone='yes' ?>
<map>
  <boolean name="enabled" value="true" />
  <int name="count" value="7" />
  <long name="epoch" value="8" />
  <float name="ratio" value="1.5" />
  <string name="empty"></string>
  <set name="names"><string>z</string><string>a</string></set>
</map>
""",
                encoding="utf-8",
            )

            self.assertEqual(
                MODULE.snapshot(root),
                {
                    "main.xml:count": {"type": "int", "value": 7},
                    "main.xml:empty": {"type": "string", "value": ""},
                    "main.xml:enabled": {"type": "boolean", "value": True},
                    "main.xml:epoch": {"type": "long", "value": 8},
                    "main.xml:names": {"type": "string-set", "value": ["a", "z"]},
                    "main.xml:ratio": {"type": "float", "value": 1.5},
                },
            )

    def test_absent_directory_is_distinct_from_present_default_key(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            absent = root / "absent"
            present = root / "present"
            present.mkdir()
            (present / "main.xml").write_text(
                '<map><boolean name="enabled" value="false" /></map>',
                encoding="utf-8",
            )

            self.assertEqual(MODULE.snapshot(absent), {})
            self.assertEqual(
                MODULE.snapshot(present),
                {"main.xml:enabled": {"type": "boolean", "value": False}},
            )

    def test_rejects_unknown_files_and_types(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "unexpected").write_text("x", encoding="utf-8")
            with self.assertRaises(ValueError):
                MODULE.snapshot(root)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "main.xml").write_text(
                '<map><double name="ratio" value="1.5" /></map>', encoding="utf-8"
            )
            with self.assertRaises(ValueError):
                MODULE.snapshot(root)


if __name__ == "__main__":
    unittest.main()
