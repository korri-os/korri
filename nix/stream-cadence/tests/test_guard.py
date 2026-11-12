"""The second review's guard points: no software viewer on a handheld, and a guard that cannot be blind."""

from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
# The trip points that the RG353M reported on 2026-11-11.
RG353M = [("passive", 70000), ("passive", 75000), ("critical", 95000)]


class Device(unittest.TestCase):
    """A session on sysfs-shaped files: one thermal zone, and a chip the test chooses."""

    chip = b"vendor,board\0rockchip,rk3566\0"

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name)
        self.zone = root / "thermal" / "thermal_zone0"
        self.zone.mkdir(parents=True)
        (self.zone / "type").write_text("cpu-thermal\n")
        for n, (kind, value) in enumerate(RG353M):
            (self.zone / f"trip_point_{n}_type").write_text(kind + "\n")
            (self.zone / f"trip_point_{n}_temp").write_text(f"{value}\n")
        self.read(50000)
        (root / "compatible").write_bytes(self.chip)
        self.peer = root / "peer"
        self.peer.write_text(f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "host-peer.py"} "$@"\n')
        self.peer.chmod(0o755)
        self.previous = (viewer.THERMAL, viewer.COMPATIBLE)
        viewer.THERMAL, viewer.COMPATIBLE = root / "thermal", root / "compatible"

    def tearDown(self):
        viewer.THERMAL, viewer.COMPATIBLE = self.previous
        self.directory.cleanup()

    def read(self, millidegrees: int) -> None:
        (self.zone / "temp").write_text(f"{millidegrees}\n")

    def prepare(self, session: viewer.Session, **extra) -> dict:
        return session.handle(
            {
                "operation": "prepare", "fixture": True, "width": 640, "height": 480,
                "refresh_hz": 50.0, "certificate": None, "key": None,
                "server_certificate": "public", **extra,
            }
        )


class NoSoftwareViewerOnAHandheld(Device):
    def test_fixture_viewer_is_refused_on_a_chip_with_a_hardware_picture_path(self):
        session = viewer.Session(str(self.peer))
        facts = session.handle({"operation": "discover"})
        self.assertEqual(facts["platform"], "v4l2request")
        self.assertTrue(facts["hardware_picture_path"])
        with self.assertRaisesRegex(RuntimeError, "hardware picture path.*software viewer"):
            self.prepare(session)
        # Nothing was started and no directory was made.
        self.assertIsNone(session.root)
        self.assertEqual(session.processes, {})
        self.assertTrue(session.close()["pass"])

    def test_refusal_says_why_the_plugin_was_not_usable(self):
        session = viewer.Session(str(self.peer))
        session.facts["plugin_missing"] = "korri-plugin status ended with 1: korrid did not answer"
        with self.assertRaisesRegex(RuntimeError, "korrid did not answer"):
            self.prepare(session)

    def test_a_chip_with_no_hardware_path_keeps_the_fixture_viewer(self):
        viewer.COMPATIBLE.write_bytes(b"vendor,board\0vendor,chip\0")
        session = viewer.Session(str(self.peer))
        try:
            self.assertFalse(session.handle({"operation": "discover"})["hardware_picture_path"])
            self.assertTrue(Path(self.prepare(session)["run_folder"]).is_dir())
        finally:
            self.assertTrue(session.close()["pass"])


if __name__ == "__main__":
    unittest.main()
