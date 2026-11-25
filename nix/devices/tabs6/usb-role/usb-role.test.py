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
import time
import threading
import unittest
from pathlib import Path

SCRIPT = Path(sys.argv.pop(1)).resolve()

I2CGET = """#!/usr/bin/env bash
# i2cget -y BUS ADDR REG b
set -eu
[ "$2" = 7 ] || { echo 'wrong bus' >&2; exit 1; }
f="$TABS6_TEST_REGS/$4"
[ ! -e "$TABS6_TEST_REGS/invalid" ] || { echo garbage; exit 0; }
[ ! -e "$TABS6_TEST_REGS/fail-read-$4" ] || exit 1
[ -e "$TABS6_TEST_REGS/offline" ] && { echo "Error: Read failed" >&2; exit 1; }
printf '0x%02x\\n' "$(cat "$f")"
"""
I2CSET = """#!/usr/bin/env bash
# i2cset -y BUS ADDR REG VALUE b
set -eu
[ -e "$TABS6_TEST_REGS/offline" ] && { echo "Error: Write failed" >&2; exit 1; }
[ "$2" = 7 ] || { echo 'wrong bus' >&2; exit 1; }
[ ! -e "$TABS6_TEST_REGS/fail-write-$4" ] || exit 1
[ ! -e "$TABS6_TEST_REGS/drop-write-$4" ] || exit 0
if [ "$4" = 0x14 ] && [ -e "$TABS6_TEST_REGS/change-on-config" ]; then
    case "$(cat "$TABS6_TEST_REGS/change-on-config")" in
    power) echo 1 > "$TABS6_TEST_REGS/muic" ;;
    detach) echo '0x00 0x44 0xff 0x88' > "$TABS6_TEST_REGS/func" ;;
    esac
fi
printf '%s %s role=%s\n' "$4" "$5" "$(cat "$TABS6_SYSFS"/class/usb_role/*/role)" >> "$TABS6_TEST_REGS/order"
echo $(( $5 )) > "$TABS6_TEST_REGS/$4"
echo "$4 $(( $5 ))" >> "$TABS6_TEST_REGS/writes"
"""
SYSTEMCTL = """#!/usr/bin/env bash
echo "$*" >> "$TABS6_TEST_REGS/systemctl"
if [ "$*" = "is-active --quiet tabs6-typec-kernel-policy.service" ]; then
    [ ! -e "$TABS6_TEST_REGS/policy-not-loaded" ] || exit 3
fi
case "$*" in
"stop --no-block usb-gadget.service"|"stop usb-gadget.service")
    echo failed > "$TABS6_TEST_REGS/gadget-state" ;;
"reset-failed usb-gadget.service")
    echo inactive > "$TABS6_TEST_REGS/gadget-state" ;;
esac
"""

I2CTRANSFER = """#!/usr/bin/env bash
set -eu
[ ! -e "$TABS6_TEST_REGS/offline" ] || exit 1
if [ "$(cat "$TABS6_SYSFS"/class/usb_role/*/role)" = host ]; then
    [ ! -e "$TABS6_TEST_REGS/fail-after-switch" ] || exit 1
    if [ -e "$TABS6_TEST_REGS/cntl-after-switch" ]; then
        cp "$TABS6_TEST_REGS/cntl-after-switch" "$TABS6_TEST_REGS/0x0c"
    fi
fi
[ ! -e "$TABS6_TEST_REGS/fail-transfer-$3-$4-${5:-}" ] || exit 1
[ ! -e "$TABS6_TEST_REGS/short-transfer" ] || { echo 0x00; exit 0; }
[ ! -e "$TABS6_TEST_REGS/slow-transfer" ] || sleep 5
case "$*" in
'-y 6 w2@0x33 0x00 0x08 r1') echo 0x00 ;;
'-y 6 w2@0x33 0x00 0x20 r4') cat "$TABS6_TEST_REGS/func" ;;
'-y 7 w1@0x25 0x15 r1') printf '0x%02x\n' "$(cat "$TABS6_TEST_REGS/muic")" ;;
'-y 7 w1@0x49 0x08 r1') printf '0x%02x\n' "$(cat "$TABS6_TEST_REGS/charger")" ;;
'-y 7 w1@0x25 0x0a r3') cat "$TABS6_TEST_REGS/devices" ;;
'-y 7 w1@0x49 0x20 r1') printf '0x%02x\n' "$(cat "$TABS6_TEST_REGS/0x20")" ;;
'-y 7 w1@0x49 0x0c r1') printf '0x%02x\n' "$(cat "$TABS6_TEST_REGS/0x0c")" ;;
*) echo 'forbidden transfer' >&2; exit 1 ;;
esac
"""

MODPROBE = """#!/usr/bin/env bash
echo "$*" >> "$TABS6_TEST_REGS/modprobe"
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
            ("i2ctransfer", I2CTRANSFER),
            ("modprobe", MODPROBE),
        ):
            p = self.bin / name
            p.write_text(text.replace("#!/usr/bin/env bash", f"#!{shutil.which('bash')}"))
            p.chmod(0o755)
        # The live values the port read back at rest (USB_HOST.md 2), with
        # other bits set so read-modify-write must keep them.
        self.reg(CNTL, 0xF5)
        (self.regs / "func").write_text("0x0e 0x24 0xff 0x8f\n")
        self.reg("muic", 0)
        self.reg("charger", 0x22)
        (self.regs / "devices").write_text("0x00 0x00 0x00\n")
        for bus, controller in ((6, "i2c@c94000"), (7, "i2c@890000")):
            node = self.sys / "firmware/devicetree/base/soc" / controller
            node.mkdir(parents=True)
            adapter = self.sys / "bus/i2c/devices" / f"i2c-{bus}"
            adapter.mkdir(parents=True)
            (adapter / "of_node").symlink_to(node)
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
            "TABS6_USB_ROLE_LOCK": str(self.tmp / "lock"),
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


    def charging_kishi(self):
        (self.regs / "func").write_text("0x15 0x21 0xff 0x9d\n")
        self.reg("muic", 1)
        self.reg("charger", 0x29)
        self.gauge("Charging")







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

    def test_early_boot_does_not_depend_on_tmpfiles_creating_the_lock_directory(self):
        lock = self.tmp / "run/lock/tabs6-usb-role.lock"
        self.reg(CNTL, 0xF7)
        r = self.run_role("boot-vbus", TABS6_USB_ROLE_LOCK=lock)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.reg(CNTL) & 7, 5)
        self.assertTrue(lock.exists())

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















    def kernel_client(self, bound=True, driver_loaded=True):
        driver = self.sys / "bus/i2c/drivers/s2mm005-read-only"
        client = self.sys / "bus/i2c/devices/6-0033"
        (client / "of_node").mkdir(parents=True)
        (client / "of_node/compatible").write_bytes(b"samsung,s2mm005-read-only\0")
        if driver_loaded:
            driver.mkdir(parents=True)
        if bound:
            (client / "driver").symlink_to(driver, target_is_directory=True)
        return client



    def test_user_space_never_enables_boost(self):
        for command in ("host", "device", "sink-host"):
            r = self.run_role(command)
            self.assertEqual(r.returncode, 2, command)
        self.assertEqual(self.writes(), [])
        self.assertEqual(self.role(), "device")

    def test_boot_vbus_reports_an_unverified_off(self):
        self.reg(CNTL, 0xF7)
        (self.regs / f"drop-write-{CNTL}").touch()
        r = self.run_role("boot-vbus", TABS6_USB_ROLE_WAIT_S=1)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("VBUS off not verified", r.stderr)
        self.assertIn("VBUS left as it is", r.stdout)

    def test_duplicate_or_missing_adapter_denies_writes(self):
        adapter = self.sys / "bus/i2c/devices/i2c-7"
        link = adapter / "of_node"
        node = link.resolve()
        link.unlink()
        self.reg(CNTL, 0xF7)
        for duplicate in (False, True):
            with self.subTest(duplicate=duplicate):
                if duplicate:
                    link.symlink_to(node)
                    other = adapter.parent / "i2c-8"
                    other.mkdir()
                    (other / "of_node").symlink_to(node)
                r = self.run_role("boot-vbus", TABS6_USB_ROLE_WAIT_S=1)
                self.assertIn("could not reach", r.stdout)
                self.assertEqual(self.writes(), [])

    def test_boot_role_writes_device_before_the_module_and_never_after(self):
        self.kernel_client(bound=False, driver_loaded=False)
        self.set_role("host")
        r = self.run_role("boot-role")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.role(), "device")
        driver = self.sys / "bus/i2c/drivers/s2mm005-read-only"
        driver.mkdir(parents=True)
        (self.sys / "bus/i2c/devices/6-0033/driver").symlink_to(driver, target_is_directory=True)
        self.set_role("host")
        r = self.run_role("boot-role")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("kernel owns", r.stderr)
        self.assertEqual(self.role(), "host")
        self.assertEqual(self.writes(), [])

    def test_host_gadget_cleanup_never_writes_role_or_chip_registers(self):
        self.set_role("host")
        self.assertEqual(self.run_role("gadget-stop").returncode, 0)
        self.assertEqual(self.systemctl(), ["stop usb-gadget.service", "reset-failed usb-gadget.service"])
        self.assertEqual(self.role(), "host")
        self.assertEqual(self.writes(), [])
        self.assertEqual((self.regs / "gadget-state").read_text().strip(), "inactive")

    def test_detach_restarts_only_the_gadget_never_input_providers(self):
        self.set_role("device")
        self.reg(CNTL, 5)
        r = self.run_role("gadget-start")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.systemctl(), ["restart --no-block usb-gadget.service"])
        self.assertEqual(self.writes(), [])
        self.assertEqual(self.role(), "device")

    def test_real_role_watcher_cycles_the_gadget_and_never_restarts_providers(self):
        helper = self.bin / "tabs6-usb-role"
        helper.write_text(f'#!{shutil.which("bash")}\nexec bash {SCRIPT} "$@"\n')
        helper.chmod(0o755)
        self.set_role("host")
        self.reg(CNTL, 5)
        stop = ["stop usb-gadget.service", "reset-failed usb-gadget.service"]
        start = ["restart --no-block usb-gadget.service"]
        def wait_for(expected):
            deadline = time.monotonic() + 2
            while self.systemctl() != expected:
                self.assertIsNone(proc.poll(), (self.tmp / "watch.log").read_text())
                self.assertLess(time.monotonic(), deadline, self.systemctl())
                time.sleep(0.01)
        with (self.tmp / "watch.log").open("w") as log:
            proc = subprocess.Popen([os.environ["TABS6_TEST_WATCH"]],
                                    env=self.env(TABS6_USB_ROLE_HELPER=helper),
                                    stdout=log, stderr=subprocess.STDOUT)
            try:
                wait_for(stop)
                # A transient unreadable role must not erase the last host.
                role_path = self.sys / "class/usb_role/a600000.usb-role-switch/role"
                role_path.unlink()
                time.sleep(0.15)
                self.assertIsNone(proc.poll())
                # host -> device -> host -> device: gadget only, in order.
                self.set_role("device")
                wait_for(stop + start)
                self.set_role("host")
                wait_for(stop + start + stop)
                self.set_role("device")
                wait_for(stop + start + stop + start)
                time.sleep(0.15)
                self.assertFalse(any("inputplumber" in c or "korri-inputd" in c
                                     for c in self.systemctl()), self.systemctl())
                self.assertEqual(self.role(), "device")
                self.assertEqual(self.writes(), [])
            finally:
                proc.terminate()
                proc.wait(timeout=5)

if __name__ == "__main__":
    unittest.main(verbosity=2)
