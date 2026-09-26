#!/usr/bin/env python3
"""Require exact resolved Kconfig equality with ROCKNIX plus the reviewed delta."""

import argparse
import importlib.util
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "config_policy", HERE / "derive-config-korri.py"
)
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


class ConfigPolicy(unittest.TestCase):
    def test_delta_changes_only_named_settings(self):
        baseline = "CONFIG_TOUCH=y\nCONFIG_NET=m\n# CONFIG_UFS is not set\n"
        self.assertEqual(
            policy.derive(baseline, "CONFIG_NET=y\nCONFIG_RTC=y\n"),
            "CONFIG_TOUCH=y\nCONFIG_NET=y\n# CONFIG_UFS is not set\nCONFIG_RTC=y\n",
        )

    def test_missing_and_disabled_are_equivalent(self):
        self.assertEqual(policy.differences({"CONFIG_UFS": "n"}, {}), [])

    def test_any_unreviewed_difference_fails(self):
        expected = {"CONFIG_TOUCH": "y", "CONFIG_RTC": "m"}
        for actual in (
            {"CONFIG_RTC": "m"},
            {"CONFIG_TOUCH": "m", "CONFIG_RTC": "m"},
            {**expected, "CONFIG_UFS": "y"},
            {**expected, "CONFIG_NEW_DRIVER": "m"},
        ):
            with self.subTest(actual=actual):
                self.assertTrue(policy.differences(expected, actual))

    def test_duplicate_or_malformed_settings_fail(self):
        for text in (
            "CONFIG_A=y\nCONFIG_A=m\n",
            "CONFIG_A\n",
            "# CONFIG_A broken\n",
            " CONFIG_NFT_LOG=y\n",
            "CONFG_NFT_LOG=y\n",
            " # CONFIG_NFT_LOG is not set\n",
        ):
            with self.subTest(text=text), self.assertRaises(ValueError):
                policy.settings(text)


class NextImageConfig(unittest.TestCase):
    def test_checked_in_product_is_derived_from_rocknix(self):
        self.assertEqual((SOURCE_DIR / "config-korri").read_text(), EXPECTED_TEXT)

    def test_every_resolved_value_matches_baseline_plus_delta(self):
        changes = policy.differences(
            policy.settings(EXPECTED_TEXT), policy.settings(Path(RESOLVED).read_text())
        )
        self.assertEqual(
            changes, [], "Unreviewed Kconfig changes:\n" + "\n".join(changes)
        )

    def test_internal_ufs_is_unavailable_and_rtc_is_retained(self):
        resolved = policy.settings(Path(RESOLVED).read_text())
        for name in ("SCSI_UFSHCD", "SCSI_UFSHCD_PLATFORM", "SCSI_UFS_QCOM"):
            self.assertEqual(resolved.get(f"CONFIG_{name}", "n"), "n")
        self.assertEqual(resolved["CONFIG_RTC_DRV_PM8XXX"], "y")

    def test_recovery_config_remains_a_separate_profile(self):
        recovery = policy.settings((SOURCE_DIR / "config-tty-trim").read_text())
        for name in ("PCI", "BT", "MEDIA_SUPPORT", "WATCHDOG"):
            self.assertEqual(recovery[f"CONFIG_{name}"], "n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("resolved_config", help="the built kernel dev output's .config")
    parser.add_argument("--source-directory", type=Path, default=HERE)
    args = parser.parse_args()
    RESOLVED = args.resolved_config
    SOURCE_DIR = args.source_directory
    EXPECTED_TEXT = policy.derive(
        (SOURCE_DIR / "config").read_text(),
        (SOURCE_DIR / "config-korri.delta").read_text(),
    )
    unittest.main(argv=[__file__])
