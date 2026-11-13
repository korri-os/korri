"""CPU package sensors join the existing thermal facts and heat decisions."""

from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import heat  # noqa: E402
import viewer  # noqa: E402


class HwmonHeat(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        previous = heat.THERMAL, viewer.COMPATIBLE
        heat.THERMAL, viewer.COMPATIBLE = self.root / "thermal", self.root / "compatible"
        self.addCleanup(setattr, heat, "THERMAL", previous[0])
        self.addCleanup(setattr, viewer, "COMPATIBLE", previous[1])

    def sensor(self, driver, label, temperature=50000, number=1, chip=0, **limits):
        device = self.root / "hwmon" / f"hwmon{chip}"
        device.mkdir(parents=True, exist_ok=True)
        (device / "name").write_text(driver + "\n")
        stem = device / f"temp{number}"
        Path(f"{stem}_label").write_text(label + "\n")
        Path(f"{stem}_input").write_text(f"{temperature}\n")
        for kind, value in limits.items():
            Path(f"{stem}_{kind}").write_text(f"{value}\n")
        return stem

    def readings(self):
        zones = heat.thermal_zones()
        self.assertTrue(zones, "CPU package sensor must be discovered without thermal zones")
        return zones, {zone["zone"]: heat.thermal_limits(zone) for zone in zones}

    def test_k10temp_without_limits_starts_below_85_and_ends_at_100(self):
        stem = self.sensor("k10temp", "Tctl", 84999)
        zones, limits = self.readings()
        self.assertEqual(zones[0]["type"], "k10temp Tctl")
        self.assertEqual(next(iter(limits.values())), {
            "critical": None, "start_below": 85000, "end_at": 100000,
        })
        self.assertIsNone(heat.no_start(zones, limits, False))
        for value, start_hot, end_hot in [(85000, True, False), (95000, True, False), (99999, True, False), (100000, True, True)]:
            with self.subTest(value=value):
                Path(f"{stem}_input").write_text(str(value))
                current = heat.thermal_zones()
                self.assertEqual(heat.no_start(current, limits, False) is not None, start_hot)
                self.assertEqual(heat.must_end(current, limits) is not None, end_hot)
        self.assertNotIn("critical trip point", heat.must_end(current, limits))
        self.assertEqual(heat.thermal_limits(zones[0], 30000)["end_at"], 80000)
        self.assertEqual(heat.thermal_limits(zones[0], 1000)["end_at"], 100000)

    def test_amd_uses_tdie_when_present_and_ignores_ccd(self):
        for driver in ["k10temp", "zenpower"]:
            with self.subTest(driver=driver):
                self.sensor(driver, "Tctl", 99000)
                self.sensor(driver, "Tdie", 50000, number=2)
                self.sensor(driver, "Tccd1", 99000, number=3)
                zones, limits = self.readings()
                self.assertEqual(len(zones), 1)
                self.assertEqual(zones[0]["type"], f"{driver} Tdie")
                self.assertIsNone(heat.no_start(zones, limits, False))

    def test_coretemp_uses_package_label_and_critical_before_max(self):
        self.sensor("coretemp", "Core 0", 99000, crit=100000)
        self.sensor("coretemp", "Package id 0", 79999, number=2, crit=100000, max=80000)
        zones, limits = self.readings()
        self.assertEqual(len(zones), 1)
        self.assertEqual(zones[0]["type"], "coretemp Package id 0")
        self.assertEqual(zones[0]["trips"], [{"type": "critical", "temp": 100000}])
        self.assertEqual(next(iter(limits.values())), {
            "critical": 100000, "start_below": 80000, "end_at": 90000,
        })
        self.assertIsNone(heat.no_start(zones, limits, False))

    def test_max_is_used_when_critical_is_absent_or_placeholder(self):
        for reported in [{"max": 105000}, {"crit": 0, "max": 105000}]:
            with self.subTest(reported=reported):
                self.sensor("coretemp", "Package id 0", **reported)
                zones, limits = self.readings()
                self.assertEqual(next(iter(limits.values()))["critical"], 105000)
                self.assertEqual(zones[0]["trips"], [{"type": "critical", "temp": 105000}])

    def test_reported_amd_limit_overrides_fixed_rule(self):
        self.sensor("k10temp", "Tctl", 85000, crit=110000)
        zones, limits = self.readings()
        self.assertEqual(next(iter(limits.values())), {
            "critical": 110000, "start_below": 90000, "end_at": 100000,
        })
        self.assertIsNone(heat.no_start(zones, limits, False))

    def test_coretemp_without_reported_limit_stays_unguarded(self):
        self.sensor("coretemp", "Package id 0")
        zones, limits = self.readings()
        self.assertIsNone(next(iter(limits.values())))
        self.assertIsNone(heat.no_start(zones, limits, False))

    def test_unknown_driver_and_non_package_labels_are_not_judged(self):
        self.sensor("nvme", "Composite", 99000, crit=100000)
        self.sensor("coretemp", "Core 0", 99000, chip=1, crit=100000)
        self.assertEqual(heat.thermal_zones(), [])
        self.assertIsNone(heat.no_start([], {}, False))

    def test_broken_sensor_or_limit_blinds_guard_and_disappearance_ends_run(self):
        stem = self.sensor("k10temp", "Tctl", crit=105000)
        zones, limits = self.readings()
        Path(f"{stem}_crit").write_text("broken")
        self.assertIn("cannot read", heat.no_start(heat.thermal_zones(), limits, False))
        Path(f"{stem}_crit").write_text("105000")
        Path(f"{stem}_input").write_text("broken")
        self.assertIn("cannot read", heat.must_end(heat.thermal_zones(), limits))
        Path(f"{stem}_label").unlink()
        self.assertIn("gone", heat.must_end(heat.thermal_zones(), limits))

    def test_unreadable_label_of_known_cpu_driver_refuses_start(self):
        stem = self.sensor("k10temp", "Tctl")
        Path(f"{stem}_label").unlink()
        Path(f"{stem}_label").mkdir()  # sysfs read fails, without depending on the test user's permissions
        zones = heat.thermal_zones()
        limits = {zone["zone"]: heat.thermal_limits(zone) for zone in zones}
        reason = heat.no_start(zones, limits, False)
        self.assertIsNotNone(reason)
        self.assertIn("cannot read", reason)

    def test_zone_limits_stay_unchanged_alongside_hwmon(self):
        zone = heat.THERMAL / "thermal_zone0"
        zone.mkdir(parents=True)
        (zone / "type").write_text("cpu-thermal")
        (zone / "temp").write_text("69000")
        for index, (kind, value) in enumerate([("passive", 70000), ("passive", 75000), ("critical", 95000)]):
            (zone / f"trip_point_{index}_type").write_text(kind)
            (zone / f"trip_point_{index}_temp").write_text(str(value))
        self.sensor("k10temp", "Tctl")
        zones, limits = self.readings()
        self.assertEqual(len(zones), 2)
        self.assertEqual(limits["thermal_zone0"], {"critical": 95000, "start_below": 70000, "end_at": 85000})

    def test_viewer_records_package_facts_samples_and_hot_start_refusal(self):
        stem = self.sensor("k10temp", "Tctl", 50000)
        peer = self.root / "peer"
        peer.write_text(f'#!/bin/sh\nexec {sys.executable} -B {Path(__file__).with_name("host-peer.py")} "$@"\n')
        peer.chmod(0o755)
        session = viewer.Session(str(peer))
        self.addCleanup(session.close)
        facts = session.handle({"operation": "discover"})
        self.assertTrue(facts["heat_guarded"])
        self.assertEqual(facts["thermal"][0]["temp"], 50000)
        self.assertEqual(facts["thermal"][0]["limits"]["start_below"], 85000)
        prepare = {
            "operation": "prepare", "fixture": True, "width": 640, "height": 480,
            "refresh_hz": 50.0, "certificate": None, "key": None, "server_certificate": "public",
        }
        session.handle(prepare)
        Path(f"{stem}_input").write_text("61234")
        self.assertEqual(session.handle({"operation": "sample"})["thermal"][0]["temp"], 61234)
        session.handle({"operation": "start", "command": ["sleep", "60"]})
        process = session.processes["viewer"]
        fresh = self.root / "reading"
        fresh.write_text("100000")
        fresh.replace(Path(f"{stem}_input"))
        self.assertIsNotNone(process.wait(timeout=5))
        with self.assertRaisesRegex(RuntimeError, "k10temp Tctl reads 100.0.*ends at 100.0"):
            session.handle({"operation": "sample"})
        self.assertTrue(session.close()["pass"])
        Path(f"{stem}_input").write_text("95500")
        hot = viewer.Session(str(peer))
        self.addCleanup(hot.close)
        with self.assertRaisesRegex(RuntimeError, "k10temp Tctl reads 95.5.*85.0"):
            hot.handle(prepare)
        self.assertIsNone(hot.root)
