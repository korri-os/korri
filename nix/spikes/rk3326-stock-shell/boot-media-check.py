#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 bash
"""Exercise the recorder's media selection against real filesystem links."""
import pathlib
import subprocess
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent


class BootMediaTests(unittest.TestCase):
    def select(self, controller='ff370000.mmc', partition='mmcblk1p1', label=True):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            dev = root / 'dev'
            sys = root / 'sys'
            (dev / 'disk/by-label').mkdir(parents=True)
            (dev / partition).touch()
            if label:
                (dev / 'disk/by-label/NIXOS_BOOT').symlink_to(dev / partition)
            node = sys / f'devices/platform/{controller}/mmc_host/mmc1/mmc1:0001/block/mmcblk1/{partition}'
            node.mkdir(parents=True)
            (sys / 'class/block').mkdir(parents=True)
            (sys / 'class/block' / partition).symlink_to(node)
            result = subprocess.run([
                'bash', str(HERE / 'check-boot-media.sh'),
                str(HERE / 'payload/boot-media.sh'), str(dev), str(sys),
            ], text=True, capture_output=True)
            return result.returncode, result.stdout.strip(), str(dev / partition)

    def test_accepts_label_on_sd_first_partition(self):
        code, output, expected = self.select()
        self.assertEqual(code, 0)
        self.assertEqual(output, expected)

    def test_rejects_emmc_with_same_label(self):
        code, output, _ = self.select(controller='ff390000.mmc')
        self.assertNotEqual(code, 0)
        self.assertEqual(output, '')

    def test_missing_label_does_not_scan_other_partitions(self):
        code, output, _ = self.select(label=False)
        self.assertNotEqual(code, 0)
        self.assertEqual(output, '')

    def test_rejects_root_partition(self):
        code, output, _ = self.select(partition='mmcblk1p2')
        self.assertNotEqual(code, 0)
        self.assertEqual(output, '')

    def test_rejects_usb_partition(self):
        code, output, _ = self.select(controller='usb', partition='sda1')
        self.assertNotEqual(code, 0)
        self.assertEqual(output, '')


if __name__ == '__main__':
    unittest.main()
