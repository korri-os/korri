"""The second review's guard points: no software viewer on a handheld, and a guard that cannot be blind."""

from pathlib import Path
import sys
import tempfile
import time
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


def hardware_session(case: Device) -> viewer.Session:
    """A session on a hardware-path chip with a run directory, as after a prepare.

    No plugin exists on a build host, so the test makes the run directory itself
    and starts stand-in viewers; the guard's rules are the real ones.
    """
    session = viewer.Session(str(case.peer))
    session.root = Path(tempfile.mkdtemp(prefix="cadence-guard-test-", dir=case.directory.name))
    (session.root / "rt").mkdir()
    session.fixture = True  # no product compositor to ask for focus
    return session


def says(line: str) -> list[str]:
    """A stand-in viewer that logs one line and stays."""
    return ["sh", "-c", f'echo "{line}"; exec sleep 60']


YES = "v4l2m2m: submit: first frame from decoder format=drm_prime (DRM_PRIME=YES)"
NO = "v4l2m2m: submit: first frame from decoder format=nv12 (DRM_PRIME=NO)"


class EachStartHasItsOwnLog(Device):
    def test_decode_state_of_the_measured_start_is_not_the_locating_starts(self):
        session = hardware_session(self)
        try:
            session.handle({"operation": "start", "command": says(YES)})
            for _ in range(50):
                if session.handle({"operation": "sample"})["hardware_decode"]:
                    break
                time.sleep(0.1)
            self.assertIs(session.handle({"operation": "sample"})["hardware_decode"], True)
            session.handle({"operation": "stop"})
            # The measured start falls back to software decode.
            session.handle({"operation": "start", "command": says(NO)})
            running = session.processes["viewer"]
            self.assertIsNotNone(running.wait(timeout=5))
            with self.assertRaisesRegex(RuntimeError, "hardware decode did not start"):
                session.handle({"operation": "sample"})
            shown = session.handle({"operation": "evidence"})
            self.assertIn("DRM_PRIME=NO", shown["viewer_log"])
            self.assertNotIn("DRM_PRIME=YES", shown["viewer_log"])
            self.assertIn("DRM_PRIME=YES", shown["viewer_earlier_log"])
        finally:
            self.assertTrue(session.close()["pass"])

    def test_a_measured_start_with_no_decode_line_has_no_decode_state(self):
        session = hardware_session(self)
        try:
            session.handle({"operation": "start", "command": says(YES)})
            time.sleep(0.5)
            session.handle({"operation": "stop"})
            session.handle({"operation": "start", "command": says("Connecting...")})
            time.sleep(0.5)
            # The validity gate needs True; the locating start's line must not supply it.
            self.assertIsNone(session.handle({"operation": "sample"})["hardware_decode"])
        finally:
            self.assertTrue(session.close()["pass"])


if __name__ == "__main__":
    unittest.main()
