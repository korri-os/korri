#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p 'python3.withPackages (p: [ p.libfdt p.pyelftools ])' dtc stdenv.cc
"""Deterministic metadata/ELF/DTB tests, plus optional real-kernel mutation tests."""

import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
SCRIPT = Path(__file__).with_name("dt-driver-audit.py")
spec = importlib.util.spec_from_file_location("dt_driver_audit", SCRIPT)
audit = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = audit
spec.loader.exec_module(audit)

HARDWARE = {
    "touch": "focaltech,ft5452",
    "led": "leds-group-multicolor",
    "rtc": "qcom,pm8941-rtc",
    "temperature": "qcom,spmi-temp-alarm",
}

# These tests compile real ELF OF_DECLARE and non-exported match table records.
# They test the documented 64-bit layout, not an emulated target kernel boot.
ELF_SOURCE = r'''
struct of_id { char name[32], type[32], compatible[128]; const void *data; };
void probe(void) {}
struct reserved_mem_ops { void (*validate)(void), (*fixup)(void), (*init)(void),
                               (*device_init)(void), (*release)(void); };
static const struct reserved_mem_ops ops = { .init = probe };
static const struct of_id __of_table_clock __attribute__((used)) = {
    .compatible = "fixed-clock", .data = probe
};
static const struct of_id __of_table_memory __attribute__((used)) = {
    .compatible = "shared-dma-pool", .data = &ops
};
static const struct of_id qcom_pcie_match[] __attribute__((used)) = {
    { .compatible = "qcom,pcie-sm8250" }, {}
};
void syscon_node_to_regmap(void) {}
'''


class CoverageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        cls.elf = cls.root / "vmlinux"
        subprocess.run(
            ["cc", "-x", "c", "-nostdlib", "-static", "-no-pie", "-Wl,-e,probe", "-o", str(cls.elf), "-"],
            input=ELF_SOURCE, text=True, check=True, capture_output=True,
        )
        cls.early, cls.functions = audit.read_elf(cls.elf)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def setUp(self):
        self.workspace = tempfile.TemporaryDirectory(dir=self.root)
        self.addCleanup(self.workspace.cleanup)
        self.directory = Path(self.workspace.name)
        self.modules = self.directory / "modules"
        self.modules.mkdir()
        (self.modules / "modules.builtin.modinfo").write_bytes(b"")
        (self.modules / "modules.alias").write_text("")

    def dtb(self, body):
        path = self.directory / "test.dtb"
        subprocess.run(
            ["dtc", "-q", "-I", "dts", "-O", "dtb", "-o", str(path), "-"],
            input='/dts-v1/; / { compatible = "retroidpocket,rpminiv2", "qcom,sm8250"; ' + body + " };",
            check=True, text=True, capture_output=True,
        )
        return path

    def rows(self, body):
        nodes = audit.read_dtb(self.dtb(body))
        return {r[1]: r for r in audit.audit(nodes, audit.read_aliases(self.modules), self.early, self.functions)}

    def builtin(self, owner, compatible):
        with (self.modules / "modules.builtin.modinfo").open("ab") as stream:
            stream.write(f"{owner}.alias=of:N*T*C{compatible}C*\0{owner}.alias=of:N*T*C{compatible}\0".encode())

    def module(self, owner, compatible, packaged=True):
        with (self.modules / "modules.alias").open("a") as stream:
            stream.write(f"alias of:N*T*C{compatible}C* {owner}\nalias of:N*T*C{compatible} {owner}\n")
        if packaged:
            # Metadata fixture: only packaging/existence is checked by read_aliases.
            (self.modules / f"{owner.replace('_', '-')}.ko").write_bytes(b"module fixture")

    def test_missing_touch_led_rtc_temperature_fail_cli(self):
        for name, compatible in HARDWARE.items():
            with self.subTest(hardware=name):
                dtb = self.dtb(f'{name} {{ compatible = "{compatible}"; }};')
                result = subprocess.run(
                    [sys.executable, str(SCRIPT), "--dtb", str(dtb), "--modules", str(self.modules), "--vmlinux", str(self.elf)],
                    capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(f"MISSING\t/{name}\t{compatible}\t", result.stdout)

    def test_builtin_and_packaged_module_cover_each_hardware(self):
        for name, compatible in HARDWARE.items():
            with self.subTest(hardware=name):
                body = f'{name} {{ compatible = "{compatible}"; }};'
                self.builtin(name, compatible)
                self.assertIn("builtin:" + name, self.rows(body)["/" + name][3])
                (self.modules / "modules.builtin.modinfo").write_bytes(b"")
                self.module(name, compatible)
                self.assertIn("module:", self.rows(body)["/" + name][3])
                (self.modules / f"{name}.ko").unlink()
                self.assertEqual(self.rows(body)["/" + name][0], "MISSING")

    def test_disabled_ancestor_overrides_enabled_child(self):
        for status in ("disabled", "reserved", "fail", "fail-sss"):
            with self.subTest(status=status):
                rows = self.rows(f'''bus {{ status = "{status}";
                    child {{ compatible = "focaltech,ft5452"; status = "okay"; }};
                    nested {{ child {{ compatible = "qcom,pm8941-rtc"; }}; }};
                }};''')
                self.assertEqual(rows["/bus/child"][0], "DISABLED")
                self.assertEqual(rows["/bus/nested/child"][0], "DISABLED")

    def test_absent_okay_and_ok_status_are_enabled(self):
        for status in ('', 'status = "okay";', 'status = "ok";'):
            with self.subTest(status=status):
                rows = self.rows(f'bus {{ {status} child {{ compatible = "qcom,pm8941-rtc"; }}; }};')
                self.assertEqual(rows["/bus/child"][0], "MISSING")

    def test_disabled_node_and_untyped_parent(self):
        rows = self.rows('bus { child { compatible = "qcom,pm8941-rtc"; status = "disabled"; }; };')
        self.assertEqual(rows["/bus/child"][0], "DISABLED")
        self.assertNotIn("/bus", rows)

    def test_builtin_fallback_compatible_and_name_type_constraints(self):
        self.builtin("rtc", "qcom,pm8941-rtc")
        row = self.rows('rtc@6000 { compatible = "qcom,new-rtc", "qcom,pm8941-rtc"; };')["/rtc@6000"]
        self.assertEqual(row[0], "DRIVER")
        (self.modules / "modules.builtin.modinfo").write_bytes(b"named.alias=of:NrtcTclockCqcom,pm8941-rtc\0")
        rows = self.rows('rtc@6000 { device_type = "clock"; compatible = "qcom,pm8941-rtc"; }; other { compatible = "qcom,pm8941-rtc"; };')
        self.assertEqual(rows["/rtc@6000"][0], "DRIVER")
        self.assertEqual(rows["/other"][0], "MISSING")

    def test_soundwire_native_alias(self):
        (self.modules / "modules.builtin.modinfo").write_bytes(b"wcd.alias=sdw:m0217p010Dv*c*\0")
        rows = self.rows('codec { compatible = "sdw20217010d00"; }; wrong { compatible = "sdw20217ffff00"; };')
        self.assertEqual(rows["/codec"][0], "DRIVER")
        self.assertEqual(rows["/wrong"][0], "MISSING")

    def test_compiled_early_and_unexported_tables(self):
        rows = self.rows('clock { compatible = "fixed-clock"; }; memory { compatible = "shared-dma-pool"; }; pcie { compatible = "qcom,pcie-sm8250"; };')
        for path, symbol in (("/clock", "__of_table_clock"), ("/memory", "__of_table_memory"), ("/pcie", "qcom_pcie_match")):
            self.assertEqual(rows[path][0], "DRIVER")
            self.assertIn(symbol, rows[path][3])
        nodes = audit.read_dtb(self.directory / "test.dtb")
        self.assertEqual(sum(r[0] == "MISSING" for r in audit.audit(nodes, [], [], set())), 3)

    def test_opps_are_referenced_data_not_blanket_children(self):
        rows = self.rows('''consumer { operating-points-v2 = <&opp>; };
            opp: opp-table { compatible = "operating-points-v2"; };
            orphan { compatible = "operating-points-v2"; };
            unknown { compatible = "new,hardware"; child { compatible = "focaltech,ft5452"; }; };
        ''')
        self.assertEqual(rows["/opp-table"][0], "DATA")
        for path in ("/orphan", "/unknown", "/unknown/child"):
            self.assertEqual(rows[path][0], "MISSING")

    def test_reviewed_exception_does_not_cover_new_path_or_child(self):
        rows = self.rows('''battery { compatible = "simple-battery";
            child { compatible = "focaltech,ft5452"; }; };
            other { compatible = "simple-battery"; };''')
        self.assertEqual(rows["/battery"][0], "REVIEWED")
        self.assertEqual(rows["/battery/child"][0], "MISSING")
        self.assertEqual(rows["/other"][0], "MISSING")

    def test_reviewed_path_with_changed_compatible_fails(self):
        self.assertEqual(self.rows('battery { compatible = "new,battery"; };')["/battery"][0], "MISSING")

    def test_syscon_requires_compiled_consumer(self):
        self.rows('soc@0 { syscon@1fc0000 { compatible = "qcom,sm8250-tcsr", "syscon"; }; };')
        nodes = audit.read_dtb(self.directory / "test.dtb")
        for functions, expected in ((self.functions, "DRIVER"), (set(), "MISSING")):
            rows = {r[1]: r for r in audit.audit(nodes, [], [], functions)}
            self.assertEqual(rows["/soc@0/syscon@1fc0000"][0], expected)

    def test_missing_metadata_is_input_error(self):
        dtb = self.dtb('rtc { compatible = "qcom,pm8941-rtc"; };')
        (self.modules / "modules.builtin.modinfo").unlink()
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--dtb", str(dtb), "--modules", str(self.modules), "--vmlinux", str(self.elf)],
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("input error", result.stderr)


@unittest.skipUnless(os.environ.get("RP_MINIV2_AUDIT_DTB"), "set RP_MINIV2_AUDIT_{DTB,MODULES,VMLINUX} for real-kernel mutation tests")
class RealKernelMutations(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.nodes = audit.read_dtb(os.environ["RP_MINIV2_AUDIT_DTB"])
        cls.aliases = audit.read_aliases(os.environ["RP_MINIV2_AUDIT_MODULES"])
        cls.early, cls.functions = audit.read_elf(os.environ["RP_MINIV2_AUDIT_VMLINUX"])

    def test_remove_driver_evidence_from_real_compiled_kernel(self):
        for name, compatible in HARDWARE.items():
            with self.subTest(hardware=name):
                nodes = [n for n in self.nodes if n.enabled and compatible in n.compatible]
                self.assertTrue(nodes, f"real product DTB must contain enabled {compatible}")
                owners = {owner for n in nodes for owner in audit.driver_matches(n, self.aliases, self.early)}
                self.assertTrue(owners, f"{compatible} is already missing before mutation")
                aliases = [(pattern, owner) for pattern, owner in self.aliases if owner not in owners]
                early = [entry for entry in self.early if entry[3] not in owners]
                rows = {r[1]: r for r in audit.audit(self.nodes, aliases, early, self.functions)}
                for node in nodes:
                    self.assertEqual(rows[node.path][0], "MISSING", node.path)

    def test_real_usb_disabled_ancestor(self):
        child = next(n for n in self.nodes if n.path == "/soc@0/usb@a8f8800/usb@a800000")
        self.assertFalse(child.enabled)
        rows = {r[1]: r for r in audit.audit(self.nodes, self.aliases, self.early, self.functions)}
        self.assertEqual(rows[child.path][0], "DISABLED")


if __name__ == "__main__":
    unittest.main()
