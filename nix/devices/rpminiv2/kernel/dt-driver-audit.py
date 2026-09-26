#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p 'python3.withPackages (p: [ p.libfdt p.pyelftools ])'
"""Check compiled RP Mini V2 DT hardware coverage, not probe/runtime success.

Inputs must be outputs of the SAME kernel derivation. See dt-driver-audit.md.
No source Kconfig symbol-to-compatible map is used.
"""

import argparse
from collections import Counter
from dataclasses import dataclass
from fnmatch import fnmatchcase
from pathlib import Path
import re
import struct
import sys

import libfdt
from elftools.elf.elffile import ELFFile


@dataclass
class Node:
    path: str
    props: dict
    enabled: bool

    def strings(self, key):
        value = self.props.get(key, b"")
        return tuple(value.rstrip(b"\0").decode().split("\0")) if value else ()

    @property
    def compatible(self):
        return self.strings("compatible")


def read_dtb(path):
    fdt = libfdt.Fdt(Path(path).read_bytes())
    nodes = []

    def visit(offset, path, parent_enabled):
        props = {}
        prop = fdt.first_property_offset(offset, quiet=(libfdt.NOTFOUND,))
        while prop >= 0:
            value = fdt.get_property_by_offset(prop)
            props[value.name] = bytes(value)
            prop = fdt.next_property_offset(prop, quiet=(libfdt.NOTFOUND,))
        enabled = parent_enabled and props.get("status", b"okay\0") in (
            b"okay\0",
            b"ok\0",
        )
        nodes.append(Node(path, props, enabled))
        child = fdt.first_subnode(offset, quiet=(libfdt.NOTFOUND,))
        while child >= 0:
            visit(child, path.rstrip("/") + "/" + fdt.get_name(child), enabled)
            child = fdt.next_subnode(child, quiet=(libfdt.NOTFOUND,))

    visit(0, "/", True)
    return nodes


def module_name(path):
    return Path(path).name.split(".ko")[0].replace("-", "_")


def read_aliases(directory):
    """Use Kbuild builtin metadata and depmod aliases with an actual .ko present."""
    directory = Path(directory)
    packaged = {
        module_name(p): str(p.relative_to(directory))
        for p in directory.rglob("*.ko*")
        if p.is_file() and re.search(r"\.ko(?:\.(?:xz|gz|zst))?$", p.name)
    }
    aliases = []
    for entry in (directory / "modules.builtin.modinfo").read_bytes().split(b"\0"):
        owner, sep, alias = entry.decode().partition(".alias=")
        if sep and alias.startswith(("of:", "sdw:")):
            # Kbuild emits builtin-only aliases even for objects absent from
            # modules.builtin (e.g. msm_poweroff). modinfo itself is the evidence.
            aliases.append((alias, f"builtin:{owner}"))
    for line in (directory / "modules.alias").read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        tag, alias, owner = line.split()
        if tag != "alias":
            raise ValueError(f"invalid modules.alias line: {line}")
        # Stale aliases never prove coverage. Removing a .ko must fail the audit.
        if owner.replace("-", "_") in packaged:
            aliases.append((alias, "module:" + packaged[owner.replace("-", "_")]))
    return sorted(set(aliases))


# These Linux 7.2 consumers do not export MODULE_DEVICE_TABLE aliases. Read
# their linked OF tables, not a copied list of compatible strings or Kconfig.
# See the exact registration sites in dt-driver-audit.md.
NON_EXPORTED_TABLES = {"qcom_pcie_match", "fastrpc_match_table", "psci_of_match"}


def read_elf(path):
    """OF_DECLARE entries are compiled struct of_device_id, not module aliases.

    include/linux/of.h:_OF_DECLARE and include/linux/mod_devicetable.h define
    these records. Validate callbacks target compiled executable code. Linux 7.2
    RESERVEDMEM_OF_DECLARE instead points at struct reserved_mem_ops (five
    callbacks; include/linux/of_reserved_mem.h).
    """
    early = []
    functions = set()
    with Path(path).open("rb") as stream:
        elf = ELFFile(stream)
        if elf.elfclass != 64 or not elf.little_endian:
            raise ValueError("expected the ARM64 little-endian kernel ELF")
        symbols = elf.get_section_by_name(".symtab")
        if symbols is None:
            raise ValueError("vmlinux has no symbol table")
        executable = [
            (s["sh_addr"], s["sh_addr"] + s["sh_size"])
            for s in elf.iter_sections()
            if s["sh_flags"] & 4
        ]
        records = list(symbols.iter_symbols())
        objects = {
            s["st_value"]: s for s in records if s["st_info"]["type"] == "STT_OBJECT"
        }

        def is_code(address):
            return any(start <= address < end for start, end in executable)

        for symbol in records:
            if (
                symbol["st_info"]["type"] == "STT_FUNC"
                and symbol["st_shndx"] != "SHN_UNDEF"
            ):
                functions.add(symbol.name)
            if symbol.name in NON_EXPORTED_TABLES:
                if (
                    symbol["st_info"]["type"] != "STT_OBJECT"
                    or symbol["st_size"] % 200
                    or not isinstance(symbol["st_shndx"], int)
                ):
                    raise ValueError(f"unexpected OF match table layout: {symbol.name}")
                section = elf.get_section(symbol["st_shndx"])
                offset = symbol["st_value"] - section["sh_addr"]
                data = section.data()[offset : offset + symbol["st_size"]]
                if data[-200:] != bytes(200):
                    raise ValueError(f"OF match table has no terminator: {symbol.name}")
                for name, kind, compat, _ in struct.iter_unpack(
                    "<32s32s128sQ", data[:-200]
                ):
                    strings = [
                        v.split(b"\0", 1)[0].decode() for v in (name, kind, compat)
                    ]
                    if not strings[2]:
                        raise ValueError(
                            f"OF match table entry has no compatible: {symbol.name}"
                        )
                    early.append((*strings, "builtin:" + symbol.name))
                continue
            if not symbol.name.startswith("__of_table_"):
                continue
            if symbol["st_size"] != 200 or not isinstance(symbol["st_shndx"], int):
                raise ValueError(f"unexpected OF_DECLARE layout: {symbol.name}")
            section = elf.get_section(symbol["st_shndx"])
            offset = symbol["st_value"] - section["sh_addr"]
            name, kind, compat, callback = struct.unpack(
                "<32s32s128sQ", section.data()[offset : offset + 200]
            )
            if not is_code(callback):
                ops = objects.get(callback)
                if (
                    ops is None
                    or ops["st_size"] != 40
                    or not isinstance(ops["st_shndx"], int)
                ):
                    raise ValueError(
                        f"OF_DECLARE has no compiled callback/ops: {symbol.name}"
                    )
                ops_section = elf.get_section(ops["st_shndx"])
                ops_offset = callback - ops_section["sh_addr"]
                callbacks = struct.unpack(
                    "<5Q", ops_section.data()[ops_offset : ops_offset + 40]
                )
                if not callbacks[2] or not all(not p or is_code(p) for p in callbacks):
                    raise ValueError(
                        f"OF_DECLARE has invalid reserved_mem_ops: {symbol.name}"
                    )
            strings = [v.split(b"\0", 1)[0].decode() for v in (name, kind, compat)]
            early.append((*strings, "builtin:" + symbol.name))
    return early, functions


def modalias(node):
    # drivers/of/module.c:of_modalias (name omits the unit address).
    name = node.path.rsplit("/", 1)[-1].split("@", 1)[0]
    kind = next(iter(node.strings("device_type")), "(null)")
    return (f"of:N{name}T{kind}" + "".join("C" + c for c in node.compatible)).replace(
        " ", "_"
    )


def driver_matches(node, aliases, early):
    requests = [modalias(node)]
    # drivers/soundwire/slave.c:sdw_of_find_slaves and bus_type.c:sdw_slave_modalias.
    for compatible in node.compatible:
        sdw = re.fullmatch(
            r"sdw([0-9a-fA-F])([0-9a-fA-F]{4})([0-9a-fA-F]{4})([0-9a-fA-F]{2})",
            compatible,
        )
        if sdw:
            version, vendor, part, kind = [int(v, 16) for v in sdw.groups()]
            requests.append(f"sdw:m{vendor:04X}p{part:04X}v{version:02X}c{kind:02X}")
    matches = {
        owner
        for alias, owner in aliases
        if any(fnmatchcase(req, alias) for req in requests)
    }
    name = node.path.rsplit("/", 1)[-1].split("@", 1)[0]
    kind = next(iter(node.strings("device_type")), "")
    for match_name, match_kind, compat, owner in early:
        if (
            compat in node.compatible
            and (not match_name or match_name == name)
            and (not match_kind or match_kind == kind)
        ):
            matches.add(owner)
    return sorted(matches)


# Owner-reviewed hardware omissions. Exact paths AND compatible lists prevent
# a new child/device from inheriting an exception. No coresight/CPU wildcards.
REVIEWED = {
    ("/battery", ("simple-battery",)): "battery description; no independent driver",
    (
        "/chosen/framebuffer@9c000000",
        ("simple-framebuffer",),
    ): "firmware framebuffer; native DSI owns display",
    ("/pmu", ("arm,armv8-pmuv3",)): "CPU performance monitor",
    ("/timer", ("arm,armv8-timer",)): "architectural timer",
    ("/soc@0/timer@17c20000", ("arm,armv7-timer-mem",)): "architectural timer",
    (
        "/soc@0/clock-controller@ad00000",
        ("qcom,sm8250-camcc",),
    ): "camera clock controller",
    (
        "/soc@0/ufshc@1d84000",
        ("qcom,sm8250-ufshc", "qcom,ufshc", "jedec,ufs-2.0"),
    ): "UFS host deliberately not compiled; internal storage excluded",
    (
        "/soc@0/gmu@3d6a000",
        ("qcom,adreno-gmu-650.2", "qcom,adreno-gmu"),
    ): "GPU-managed GMU",
}
for address in ("0", "100", "200", "300", "400", "500", "600", "700"):
    REVIEWED[(f"/cpus/cpu@{address}", ("qcom,kryo485",))] = "CPU core"
for kind, addresses, compatible in (
    ("etf", ("6b05000",), "arm,coresight-tmc"),
    ("etr", ("6048000",), "arm,coresight-tmc"),
    (
        "etm",
        (
            "7040000",
            "7140000",
            "7240000",
            "7340000",
            "7440000",
            "7540000",
            "7640000",
            "7740000",
        ),
        "arm,coresight-etm4x",
    ),
    (
        "funnel",
        (
            "6005000",
            "6041000",
            "6042000",
            "6045000",
            "6b04000",
            "6c0b000",
            "6c2d000",
            "7800000",
            "7810000",
        ),
        "arm,coresight-dynamic-funnel",
    ),
    ("replicator", ("6046000", "6b06000"), "arm,coresight-dynamic-replicator"),
    ("tpda", ("6004000",), "qcom,coresight-tpda"),
    ("tpdm", ("684c000", "6c08000"), "qcom,coresight-tpdm"),
):
    for address in addresses:
        REVIEWED[(f"/soc@0/{kind}@{address}", (compatible, "arm,primecell"))] = (
            "CoreSight debug/trace"
        )


def data_node(node, nodes):
    """Only producer-defined descriptions, never a blanket exemption for children."""
    if node.path == "/" and node.compatible == (
        "retroidpocket,rpminiv2",
        "qcom,sm8250",
    ):
        return "board identity (board DTS root), not an independently probed device"
    # drivers/opp/of.c:_opp_of_get_opp_desc_node consumes these phandles.
    if node.compatible == ("operating-points-v2",) and node.props.get("phandle"):
        handle = node.props["phandle"]
        for consumer in nodes:
            refs = consumer.props.get("operating-points-v2", b"")
            if consumer.enabled and handle in [
                refs[i : i + 4] for i in range(0, len(refs), 4)
            ]:
                return (
                    f"OPP description referenced by {consumer.path} (drivers/opp/of.c)"
                )
    # drivers/base/cacheinfo.c:cache_setup_of_node reads CPU cache topology.
    caches = {
        f"/cpus/cpu@{address}/l2-cache"
        for address in ("0", "100", "200", "300", "400", "500", "600", "700")
    }
    caches.add("/cpus/cpu@0/l2-cache/l3-cache")
    if node.path in caches and node.compatible == ("cache",):
        return "CPU cache topology (drivers/base/cacheinfo.c)"
    idle = {
        "/cpus/idle-states/cpu-sleep-0-0": ("arm,idle-state",),
        "/cpus/idle-states/cpu-sleep-1-0": ("arm,idle-state",),
        "/cpus/domain-idle-states/cluster-sleep-0": ("domain-idle-state",),
    }
    if idle.get(node.path) == node.compatible:
        return "CPU idle description (drivers/cpuidle/dt_idle_states.c, drivers/pmdomain/core.c)"
    if (
        node.path == "/soc@0/spmi@c440000/pmic@2/typec@1500/connector"
        and node.compatible == ("usb-c-connector",)
    ):
        parent = next(n for n in nodes if n.path == node.path.rsplit("/", 1)[0])
        if parent.compatible == ("qcom,pm8150b-typec",):
            return "parent TCPM connector data (drivers/usb/typec/tcpm/qcom/qcom_pmic_typec.c)"
    return None


def audit(nodes, aliases, early, functions):
    rows = []
    for node in sorted(nodes, key=lambda n: n.path):
        if not node.compatible:
            continue
        if not node.enabled:
            status, evidence = "DISABLED", "node or ancestor status is not okay/ok"
        elif description := data_node(node, nodes):
            status, evidence = "DATA", description
        elif matches := driver_matches(node, aliases, early):
            status, evidence = "DRIVER", ",".join(matches)
        elif (node.path, node.compatible) in REVIEWED:
            status, evidence = "REVIEWED", REVIEWED[(node.path, node.compatible)]
        elif (
            node.path == "/soc@0/syscon@1fc0000"
            and node.compatible == ("qcom,sm8250-tcsr", "syscon")
            and "syscon_node_to_regmap" in functions
        ):
            # drivers/mfd/syscon.c creates regmaps on demand, no OF driver table.
            status, evidence = (
                "DRIVER",
                "builtin:syscon_node_to_regmap (drivers/mfd/syscon.c)",
            )
        else:
            status, evidence = (
                "MISSING",
                "no compiled OF declaration, builtin alias or packaged module alias",
            )
        rows.append((status, node.path, " ".join(node.compatible), evidence))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dtb", required=True, type=Path)
    parser.add_argument(
        "--modules",
        required=True,
        type=Path,
        help="lib/modules/<version> of the same kernel",
    )
    parser.add_argument("--vmlinux", required=True, type=Path)
    args = parser.parse_args()
    try:
        nodes = read_dtb(args.dtb)
        if not nodes or nodes[0].compatible != (
            "retroidpocket,rpminiv2",
            "qcom,sm8250",
        ):
            raise ValueError("not the RP Mini V2 product DTB")
        early, functions = read_elf(args.vmlinux)
        rows = audit(nodes, read_aliases(args.modules), early, functions)
        for row in rows:
            print("\t".join(row))
        counts = Counter(row[0] for row in rows)
        print(
            "DT driver coverage: "
            + ", ".join(f"{key}={value}" for key, value in sorted(counts.items())),
            file=sys.stderr,
        )
        return 1 if counts["MISSING"] else 0
    except (OSError, ValueError, libfdt.FdtException) as error:
        print(f"DT driver audit input error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
