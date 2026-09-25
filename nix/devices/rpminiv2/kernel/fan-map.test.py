#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 dtc
"""Check compiled product fan policy and unchanged recovery DTB; not a device test."""

import hashlib
import os
from pathlib import Path
import subprocess
import unittest

PRODUCT = Path(os.environ["RP_MINIV2_PRODUCT_DTB"])
RECOVERY = Path(os.environ["RP_MINIV2_RECOVERY_DTB"])
FAN = "/pwm-fan"
ZONE = "/thermal-zones/cluster1-thermal"
TRIPS = (45000, 55000, 62000, 68000, 73000, 78000)
STATES = (1, 2, 3, 4, 6, 8)


def property_value(dtb, node, name, value_type="x"):
    result = subprocess.run(
        ["fdtget", "-t", value_type, str(dtb), node, name],
        capture_output=True,
        text=True,
        check=True,
        timeout=15,
    )
    if value_type == "s":
        return result.stdout.strip()
    return [int(value, 16) for value in result.stdout.split()]


class ProductFanMap(unittest.TestCase):
    def test_compiled_moderate_policy(self):
        self.assertEqual(
            property_value(PRODUCT, FAN, "cooling-levels"),
            [0, 51, 77, 102, 128, 153, 179, 204, 255],
        )
        self.assertEqual(property_value(PRODUCT, FAN, "pwms")[1:], [3, 50000])
        fan_handle = property_value(PRODUCT, FAN, "phandle")[0]
        for index, (temperature, state) in enumerate(zip(TRIPS, STATES)):
            with self.subTest(trip=index):
                trip = f"{ZONE}/trips/fan-active{index}"
                cooling_map = f"{ZONE}/cooling-maps/fan-map{index}"
                self.assertEqual(property_value(PRODUCT, trip, "temperature"), [temperature])
                self.assertEqual(property_value(PRODUCT, trip, "hysteresis"), [5000])
                self.assertEqual(property_value(PRODUCT, trip, "type", "s"), "active")
                self.assertEqual(
                    property_value(PRODUCT, cooling_map, "trip"),
                    property_value(PRODUCT, trip, "phandle"),
                )
                self.assertEqual(
                    property_value(PRODUCT, cooling_map, "cooling-device"),
                    [fan_handle, state, state],
                )
        missing_tach = subprocess.run(
            ["fdtget", str(PRODUCT), FAN, "interrupts"],
            capture_output=True,
            check=False,
            timeout=15,
        )
        self.assertNotEqual(missing_tach.returncode, 0)

    def test_recovery_dtb_remains_the_hardware_proven_artifact(self):
        self.assertEqual(
            hashlib.sha256(RECOVERY.read_bytes()).hexdigest(),
            "f9e32c33e14f3d974c461c674435a7002c73ec4243e96e3aef158620a730eee4",
        )
        self.assertEqual(
            property_value(RECOVERY, FAN, "cooling-levels"), [0, 32, 64, 128, 255]
        )
        self.assertEqual(property_value(RECOVERY, FAN, "pwms")[1:], [3, 100000])


if __name__ == "__main__":
    unittest.main()
