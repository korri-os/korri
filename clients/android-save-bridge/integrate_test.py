#!/usr/bin/env python3
"""Tests for integrate.py. Run: python3 integrate_test.py"""
import tempfile
import unittest
from pathlib import Path

from integrate import integrate

MANIFEST = """<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="x">
    <application android:label="Game">
        <activity android:name=".Main" />
    </application>
</manifest>
"""


class IntegrateTest(unittest.TestCase):
    def module(self):
        root = Path(tempfile.mkdtemp())
        (root / "src/main").mkdir(parents=True)
        (root / "src/main/AndroidManifest.xml").write_text(MANIFEST)
        return root

    def test_adds_provider_permission_and_folders_once(self):
        root = self.module()
        integrate(root, "save=v190/save")
        text = (root / "src/main/AndroidManifest.xml").read_text()
        self.assertIn('android:protectionLevel="signature"', text)
        self.assertIn("KorriSaveBridgeProvider", text)
        self.assertIn('android:value="save=v190/save"', text)
        self.assertTrue((root / "src/main/java/dev/korri/savebridge/Paths.java").is_file())
        with self.assertRaises(SystemExit):
            integrate(root, "save=v190/save")

    def test_internal_storage_folders(self):
        root = self.module()
        integrate(root, "save=internal:saves;data=v1/x")
        self.assertIn('android:value="save=internal:saves;data=v1/x"',
                      (root / "src/main/AndroidManifest.xml").read_text())

    def test_refuses_bad_folder_maps(self):
        for bad in ["", "save", "Save=x", "save=../x", "save=a b", "save=x;", "save=other:x"]:
            with self.assertRaises(SystemExit, msg=bad):
                integrate(self.module(), bad)


if __name__ == "__main__":
    unittest.main()
