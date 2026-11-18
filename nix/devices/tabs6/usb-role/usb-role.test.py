#!/usr/bin/env python3
"""tabs6-usb-role against a temporary sysfs tree and an SM5705 register file.

i2cget, i2cset and systemctl are small real programs on PATH that read and
write files in the test directory; nothing touches real hardware. Run with the
script path as the first argument (check.nix does this).
"""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(sys.argv.pop(1)).resolve()

I2CGET = """#!/usr/bin/env bash
# i2cget -y BUS ADDR REG b
set -eu
f="$TABS6_TEST_REGS/$4"
[ -e "$TABS6_TEST_REGS/offline" ] && { echo "Error: Read failed" >&2; exit 1; }
printf '0x%02x\\n' "$(cat "$f")"
"""
I2CSET = """#!/usr/bin/env bash
# i2cset -y BUS ADDR REG VALUE b
set -eu
[ -e "$TABS6_TEST_REGS/offline" ] && { echo "Error: Write failed" >&2; exit 1; }
echo $(( $5 )) > "$TABS6_TEST_REGS/$4"
echo "$4 $(( $5 ))" >> "$TABS6_TEST_REGS/writes"
"""
SYSTEMCTL = """#!/usr/bin/env bash
echo "$*" >> "$TABS6_TEST_REGS/systemctl"
"""

CNTL, CHGCNTL6, FLEDCNTL6 = "0x0c", "0x14", "0x20"


class UsbRole(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.sys = self.tmp / "sys"
        self.regs = self.tmp / "regs"
        self.bin = self.tmp / "bin"
        for d in (self.regs, self.bin):
            d.mkdir()
        for name, text in (
            ("i2cget", I2CGET),
            ("i2cset", I2CSET),
            ("systemctl", SYSTEMCTL),
        ):
            p = self.bin / name
            p.write_text(text.replace("#!/usr/bin/env bash", f"#!{shutil.which('bash')}"))
            p.chmod(0o755)
        # The live values the port read back at rest (USB_HOST.md 2), with
        # other bits set so read-modify-write must keep them.
        self.reg(CNTL, 0xF5)
        self.reg(CHGCNTL6, 0xF8)
        self.reg(FLEDCNTL6, 0xA5)
        role = self.sys / "class/usb_role/a600000.usb-role-switch"
        role.mkdir(parents=True)
        (role / "role").write_text("device\n")
        self.gauge("Discharging")
        (self.sys / "class/udc/a600000.usb").mkdir(parents=True)
        self.udc("not attached")
        (self.sys / "bus/usb/devices").mkdir(parents=True)
        for hub in ("usb1", "usb2", "1-0:1.0", "2-0:1.0"):
            (self.sys / "bus/usb/devices" / hub).mkdir()
        self.i2c = self.tmp / "i2c-1"
        self.i2c.touch()

    def reg(self, name, value=None):
        f = self.regs / name
        if value is not None:
            f.write_text(f"{value}\n")
        return int(f.read_text())

    def gauge(self, status):
        d = self.sys / "class/power_supply/sm5705-fuelgauge"
        d.mkdir(parents=True, exist_ok=True)
        if status is None:
            (d / "status").unlink(missing_ok=True)
        else:
            (d / "status").write_text(status + "\n")

    def udc(self, state):
        (self.sys / "class/udc/a600000.usb/state").write_text(state + "\n")

    def role(self):
        return (self.sys / "class/usb_role/a600000.usb-role-switch/role").read_text().strip()

    def set_role(self, value):
        (self.sys / "class/usb_role/a600000.usb-role-switch/role").write_text(value + "\n")

    def env(self, **extra):
        env = {
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "TABS6_SYSFS": str(self.sys),
            "TABS6_TEST_REGS": str(self.regs),
            "TABS6_I2C_DEV": str(self.i2c),
            "TABS6_USB_GADGET_UNIT": "usb-gadget.service",
        }
        env.update({k: str(v) for k, v in extra.items()})
        return env

    def run_role(self, *args, **extra):
        return subprocess.run(
            ["bash", str(SCRIPT), *args],
            env=self.env(**extra),
            capture_output=True,
            text=True,
            timeout=30,
        )

    def writes(self):
        f = self.regs / "writes"
        return f.read_text().splitlines() if f.exists() else []

    def systemctl(self):
        f = self.regs / "systemctl"
        return f.read_text().splitlines() if f.exists() else []

    def assert_refused(self, reason):
        r = self.run_role("host")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn(reason, r.stderr)
        self.assertEqual(self.writes(), [])
        self.assertEqual(self.role(), "device")

    def test_host_powers_the_port_then_switches(self):
        r = self.run_role("host")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.reg(FLEDCNTL6), 0xAB)  # 5100 mV, high bits kept
        self.assertEqual(self.reg(CHGCNTL6), 0xF8)  # 900 mA in bits 3:2, kept
        self.assertEqual(self.reg(CNTL), 0xF7)  # USB_OTG, high bits kept
        # Boost before the role, as the port did.
        self.assertEqual([w.split()[0] for w in self.writes()], [FLEDCNTL6, CHGCNTL6, CNTL])
        self.assertEqual(self.role(), "host")

    def test_host_refused_while_charging(self):
        self.gauge("Charging")
        self.assert_refused("says Charging")

    def test_host_refused_on_full_or_not_charging(self):
        for status in ("Full", "Not charging"):
            self.gauge(status)
            self.assert_refused(f"says {status}")

    def test_host_refused_without_a_gauge(self):
        self.gauge(None)
        self.assert_refused("fuel gauge is missing")

    def test_host_refused_while_a_host_uses_the_gadget(self):
        self.udc("configured")
        self.assert_refused("gadget configured")

    def test_device_turns_vbus_off_and_rebinds_the_gadget(self):
        self.run_role("host")
        r = self.run_role("device")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.role(), "device")
        self.assertEqual(self.reg(CNTL), 0xF5)
        self.assertEqual(self.reg(FLEDCNTL6), 0xA5)
        self.assertEqual(self.systemctl(), ["restart --no-block usb-gadget.service"])

    def test_status(self):
        (self.sys / "bus/usb/devices/1-1").mkdir()
        (self.sys / "bus/usb/devices/1-1:1.0").mkdir()
        r = self.run_role("status")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("role device, VBUS off, USB devices 1", r.stdout)

    def test_boot_returns_to_device_and_turns_the_boost_off(self):
        self.set_role("host")
        self.reg(CNTL, 0xF7)
        self.assertEqual(self.run_role("boot-role").returncode, 0)
        self.assertEqual(self.role(), "device")
        self.assertEqual(self.run_role("boot-vbus").returncode, 0)
        self.assertEqual(self.reg(CNTL), 0xF5)
        # Boot never restarts the gadget: its unit runs after boot-role.
        self.assertEqual(self.systemctl(), [])

    def test_boot_never_fails_on_missing_hardware(self):
        for d in (self.sys / "class/usb_role").iterdir():
            (d / "role").unlink()
        self.i2c.unlink()
        r = self.run_role("boot-role", TABS6_USB_ROLE_WAIT_S=1)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("no USB role switch", r.stdout)
        r = self.run_role("boot-vbus", TABS6_USB_ROLE_WAIT_S=1)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("could not reach the SM5705", r.stdout)
        self.i2c.touch()
        (self.regs / "offline").touch()
        r = self.run_role("boot-vbus", TABS6_USB_ROLE_WAIT_S=1)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("could not reach the SM5705", r.stdout)


if __name__ == "__main__":
    unittest.main(verbosity=2)
