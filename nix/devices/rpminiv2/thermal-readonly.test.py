#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3
"""Exercise the actual read-only snapshot against a controlled sysfs tree."""
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("thermal-readonly.sh")


class ThermalSnapshotTest(unittest.TestCase):
    def test_reports_named_zones_and_fan_without_writing_controls(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sys = root / "sys"
            proc = root / "proc"
            readings = {
                sys / "class/thermal/thermal_zone7/type": "cpu7-top-thermal\n",
                sys / "class/thermal/thermal_zone7/temp": "95400\n",
                sys / "class/thermal/thermal_zone7/trip_point_0_temp": "90000\n",
                sys / "class/thermal/thermal_zone7/trip_point_0_type": "passive\n",
                sys / "class/thermal/cooling_device4/type": "pwm-fan\n",
                sys / "class/thermal/cooling_device4/cur_state": "1\n",
                sys / "class/hwmon/hwmon0/name": "pwmfan\n",
                sys / "class/hwmon/hwmon0/pwm1": "70\n",
                sys / "class/hwmon/hwmon0/pwm1_enable": "2\n",
                sys / "devices/system/cpu/cpufreq/policy7/scaling_governor": "performance\n",
                sys / "devices/system/cpu/cpufreq/policy7/scaling_cur_freq": "844800\n",
                sys / "class/power_supply/BAT0/type": "Battery\n",
                sys / "class/power_supply/BAT0/temp": "400\n",
                proc / "uptime": "10.00 0.00\n",
                proc / "loadavg": "1.03 1.07 1.07 1/100 42\n",
                proc / "pressure/cpu": "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n",
            }
            for path, data in readings.items():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(data)

            result = subprocess.run(
                ["sh", str(SCRIPT), str(sys), str(proc)],
                text=True,
                capture_output=True,
                check=True,
                timeout=10,
            )
            self.assertIn("cpu7-top-thermal", result.stdout)
            self.assertIn("thermal_zones=1", result.stdout)
            self.assertIn("trip_point_0_temp=90000", result.stdout)
            self.assertIn("pwm1=70", result.stdout)
            self.assertIn("scaling_governor=performance", result.stdout)
            self.assertIn("BAT0/temp=400", result.stdout)
            self.assertEqual({path: path.read_text() for path in readings}, readings)
            self.assertNotIn("fan1_input", result.stdout)

    def test_missing_optional_sensor_is_reported_without_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = subprocess.run(
                ["sh", str(SCRIPT), str(root), str(root)],
                text=True,
                capture_output=True,
                check=True,
                timeout=10,
            )
            self.assertIn("thermal_zones=0", result.stdout)


if __name__ == "__main__":
    unittest.main()
