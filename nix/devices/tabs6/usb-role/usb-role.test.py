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
"""

I2CTRANSFER = """#!/usr/bin/env bash
set -eu
[ ! -e "$TABS6_TEST_REGS/offline" ] || exit 1
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
            "TABS6_TYPEC_AUTO": os.environ["TABS6_TEST_AUTO"],
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

    def assert_refused(self, reason):
        r = self.run_role("host")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn(reason, r.stderr)
        self.assertFalse(any(w == "0x0c 247" for w in self.writes()))
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
        self.assertEqual(self.systemctl(), ["stop tabs6-typec-auto.service", "stop tabs6-typec-auto.service", "restart --no-block usb-gadget.service"])

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

    def test_off_precedes_role_device(self):
        self.assertEqual(self.run_role("host").returncode, 0)
        self.assertEqual(self.run_role("device").returncode, 0)
        order = (self.regs / "order").read_text().splitlines()
        self.assertIn("0x0c 245 role=host", order)

    def test_power_or_detach_between_checks_never_enables_boost(self):
        for change in ("power", "detach"):
            with self.subTest(change=change):
                self.reg("muic", 0)
                (self.regs / "func").write_text("0x0e 0x24 0xff 0x8f\n")
                (self.regs / "change-on-config").write_text(change)
                r = self.run_role("host")
                self.assertNotEqual(r.returncode, 0)
                self.assertNotIn("0x0c 247", self.writes())
                self.assertEqual(self.role(), "device")

    def test_each_write_failure_cleans_up_without_retrying_enable(self):
        for register in (FLEDCNTL6, CHGCNTL6, CNTL):
            with self.subTest(register=register):
                failure = self.regs / f"fail-write-{register}"
                failure.touch()
                r = self.run_role("host")
                self.assertNotEqual(r.returncode, 0)
                self.assertNotIn("0x0c 247", self.writes())
                self.assertEqual(self.role(), "device")
                failure.unlink()

    def test_readback_must_confirm_off_even_if_role_can_be_restored(self):
        self.reg(CNTL, 0xF7)
        self.set_role("host")
        (self.regs / f"drop-write-{CNTL}").touch()
        r = self.run_role("device")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("VBUS off not verified", r.stderr)
        self.assertEqual(self.role(), "device")
        self.assertNotIn("VBUS off (readback verified)", r.stdout)

    def test_each_policy_read_failure_or_short_read_denies_host(self):
        for name in ("w2@0x33-0x00-0x08", "w2@0x33-0x00-0x20",
                     "w1@0x25-0x15-r1", "w1@0x49-0x08-r1", "w1@0x49-0x0c-r1",
                     "w1@0x49-0x20-r1", "w1@0x25-0x0a-r3"):
            with self.subTest(name=name):
                failure = self.regs / f"fail-transfer-{name}"
                failure.touch()
                r = self.run_role("host")
                self.assertNotEqual(r.returncode, 0)
                self.assertNotIn("0x0c 247", self.writes())
                failure.unlink()
        (self.regs / "short-transfer").touch()
        self.assertNotEqual(self.run_role("host").returncode, 0)
        self.assertNotIn("0x0c 247", self.writes())

    def test_manual_host_preserves_the_measured_passive_uart_classification(self):
        # After the live reboot, the unplugged Kishi reports DEVT1_UART.
        # This is not one of the charger's/PC's power signatures.
        (self.regs / "devices").write_text("0x08 0x00 0x00\n")
        r = self.run_role("host")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.role(), "host")
        self.assertEqual(self.reg(CNTL) & 7, 7)

    def test_muic_charger_and_pc_signatures_never_enable_boost(self):
        for value in (0x04, 0x10, 0x20, 0x40):
            with self.subTest(value=value):
                (self.regs / "devices").write_text(f"0x{value:02x} 0x00 0x00\n")
                self.assertNotEqual(self.run_role("host").returncode, 0)
                self.assertNotIn("0x0c 247", self.writes())

    def test_incoming_power_denied_even_with_source_flags_and_discharging(self):
        for register, value in (("muic", 1), ("charger", 0x21)):
            with self.subTest(register=register):
                self.reg(register, value)
                self.assertNotEqual(self.run_role("host").returncode, 0)
                self.assertNotIn("0x0c 247", self.writes())
                self.reg(register, 0 if register == "muic" else 0x22)

    def test_configuration_read_errors_never_enable_boost(self):
        for register in (FLEDCNTL6, CHGCNTL6, CNTL):
            with self.subTest(register=register):
                failure = self.regs / f"fail-read-{register}"
                failure.touch()
                self.assertNotEqual(self.run_role("host").returncode, 0)
                self.assertNotIn("0x0c 247", self.writes())
                self.assertEqual(self.role(), "device")
                failure.unlink()

    def test_duplicate_or_missing_adapter_denies_reads_and_writes(self):
        adapter = self.sys / "bus/i2c/devices/i2c-7"
        link = adapter / "of_node"
        node = link.resolve()
        link.unlink()
        for duplicate in (False, True):
            with self.subTest(duplicate=duplicate):
                if duplicate:
                    link.symlink_to(node)
                    other = adapter.parent / "i2c-8"
                    other.mkdir()
                    (other / "of_node").symlink_to(node)
                self.assertNotEqual(self.run_role("host").returncode, 0)
                self.assertEqual(self.writes(), [])
                self.assertEqual(self.role(), "device")

    def test_automatic_gauge_policy_uses_non_charging_with_all_other_checks(self):
        for status in ("Full", "Not charging", "Unknown"):
            self.gauge(status)
            r = self.run_role("host", TABS6_AUTOMATIC=1)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(self.run_role("device", TABS6_AUTOMATIC=1).returncode, 0)

    def test_empty_or_unreadable_gauge_denies_host(self):
        for status in ("", None):
            self.gauge(status)
            self.assertNotEqual(self.run_role("host").returncode, 0)
            self.assertNotIn("0x0c 247", self.writes())

    def test_a_hung_status_command_times_out_without_enable(self):
        (self.regs / "slow-transfer").touch()
        start = time.monotonic()
        r = self.run_role("host")
        self.assertNotEqual(r.returncode, 0)
        self.assertLess(time.monotonic() - start, 1.0)
        self.assertNotIn("0x0c 247", self.writes())

    def test_invalid_i2c_result_and_unavailable_bus_still_select_device(self):
        for condition in ("invalid", "offline"):
            self.set_role("host")
            (self.regs / condition).touch()
            r = self.run_role("device")
            self.assertNotEqual(r.returncode, 0)
            self.assertEqual(self.role(), "device")
            (self.regs / condition).unlink()

    def test_real_daemon_replays_kishi_charger_and_fault_transitions(self):
        helper = self.bin / "tabs6-usb-role"
        helper.write_text(f"#!{shutil.which('bash')}\nexec bash {SCRIPT} \"$@\"\n")
        helper.chmod(0o755)
        env = self.env(TABS6_USB_ROLE_HELPER=helper)
        trace = [dict(part.split("=", 1) for part in line.split())
                 for line in Path(os.environ["TABS6_TEST_TRACE"]).read_text().splitlines()]
        def replay(index):
            row = trace[index]
            func = " ".join("0x" + row["func"][i:i+2] for i in range(0, 8, 2))
            (self.regs / "func").write_text(func + "\n")
            self.reg("muic", int(row["muic_vbus"], 16))
            self.reg("charger", int(row["chg_status"], 16))
            self.gauge(row["gauge"].strip('"'))
        with (self.tmp / "daemon.log").open("w+") as log:
            proc = subprocess.Popen([os.environ["TABS6_TEST_AUTO"]], env=env,
                                    stdout=log, stderr=subprocess.STDOUT)
            def wait_off(limit=1):
                start = time.monotonic()
                while (self.role() != "device" or self.reg(CNTL) & 7 != 5) and time.monotonic() - start < limit:
                    self.assertIsNone(proc.poll(), (self.tmp / "daemon.log").read_text())
                    time.sleep(0.01)
                self.assertEqual(self.role(), "device", (self.tmp / "daemon.log").read_text())
                self.assertEqual(self.reg(CNTL) & 7, 5)
                return time.monotonic() - start
            try:
                # Even a permanently attached source-qualified Kishi must
                # not grant automatic HOST until direction is proven.
                time.sleep(1.3)
                self.assertIsNone(proc.poll())
                self.assertEqual(self.role(), "device")
                self.assertNotIn("0x0c 247", self.writes())
                self.assertIn("mode=observe automatic_host=blocked", (self.tmp / "daemon.log").read_text())
                self.assertIn("staying_device=true", (self.tmp / "daemon.log").read_text())
                (self.regs / "devices").write_text("0x08 0x00 0x00\n")
                time.sleep(1.0)
                self.assertEqual(self.role(), "device")
                self.assertNotIn("0x0c 247", self.writes())
                for index in (5, 6, 7, 8, 9, 11, 12):
                    replay(index)
                    time.sleep(0.32)
                    self.assertEqual(self.role(), "device")
                    self.assertNotIn("0x0c 247", self.writes())
                # Unexpected boost or a late role completion must be undone,
                # never inherited as permission by observe mode.
                self.set_role("host")
                self.reg(CNTL, 0xF7)
                off_time = wait_off()
                self.assertLess(off_time, 1.0)
                (self.regs / "fail-transfer-w2@0x33-0x00-0x20").touch()
                self.set_role("host")
                self.assertLess(wait_off(), 1.0)
                self.assertNotIn("0x0c 247", self.writes())
                print(f"daemon observe: unexpected boost off/device={off_time:.3f}s; never enabled HOST")
            finally:
                proc.terminate()
                proc.wait(timeout=5)


if __name__ == "__main__":
    unittest.main(verbosity=2)
