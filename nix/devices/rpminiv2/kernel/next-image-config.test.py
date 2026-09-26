#!/usr/bin/env python3
"""Check resolved product Kconfig against the Mini V2 ROCKNIX source baseline."""

import argparse
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent
FEATURES = {
    "K1": "PCI PCIE_QCOM PCIE_QCOM_COMMON PHY_QCOM_QMP_PCIE CFG80211 MAC80211 ATH11K ATH11K_PCI RFKILL",
    "K2": "BT BT_HCIUART BT_HCIUART_SERDEV BT_HCIUART_QCA BT_QCA BT_LE BT_HIDP",
    "K3": "HID_PLAYSTATION HID_SONY HID_NINTENDO JOYSTICK_XPAD JOYSTICK_XPAD_FF",
    "K4": "CHARGER_QCOM_SMB5 BATTERY_QCOM_FG",
    "K5": "MEDIA_SUPPORT V4L_MEM2MEM_DRIVERS V4L2_MEM2MEM_DEV VIDEO_QCOM_VENUS",
    "K6": "LEDS_HTR3212 LEDS_TRIGGERS LEDS_TRIGGER_TIMER LEDS_TRIGGER_HEARTBEAT LEDS_TRIGGER_CPU LEDS_TRIGGER_DEFAULT_ON LEDS_TRIGGER_PANIC",
    "K7": "ZRAM ZRAM_DEF_COMP_LZORLE",
    "K8": "SCSI BLK_DEV_SD USB_STORAGE EXFAT_FS NTFS3_FS",
    "K9": "USB_USBNET USB_RTL8152",
    "K10": "TUN WIREGUARD",
    "K11": "WATCHDOG QCOM_WDT PSTORE PSTORE_RAM",
}


def settings(path):
    result = {}
    for line in path.read_text().splitlines():
        if line.startswith("CONFIG_"):
            name, value = line.split("=", 1)
            result[name] = value
        elif line.startswith("# CONFIG_") and line.endswith(" is not set"):
            result[line.split()[1]] = "n"
    return result


class NextImageConfig(unittest.TestCase):
    def test_rocknix_values_survive_kernel_resolution(self):
        baseline = settings(HERE / "config")
        product = settings(HERE / "config-korri")
        resolved = settings(Path(RESOLVED))
        for slice_name, names in FEATURES.items():
            for name in names.split():
                symbol = f"CONFIG_{name}"
                with self.subTest(slice=slice_name, symbol=symbol):
                    self.assertIn(baseline.get(symbol), ("y", "m"))
                    self.assertEqual(product.get(symbol), baseline[symbol])
                    self.assertEqual(resolved.get(symbol), baseline[symbol])

    def test_recovery_config_is_still_a_separate_profile(self):
        recovery = settings(HERE / "config-tty-trim")
        self.assertEqual(recovery["CONFIG_PCI"], "n")
        self.assertEqual(recovery["CONFIG_BT"], "n")
        self.assertEqual(recovery["CONFIG_MEDIA_SUPPORT"], "n")
        self.assertEqual(recovery["CONFIG_WATCHDOG"], "n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("resolved_config", help="the built kernel dev output's .config")
    args = parser.parse_args()
    RESOLVED = args.resolved_config
    unittest.main(argv=[__file__])
