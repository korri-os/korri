#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3
"""Read-only audit of rg42t.dts against the carried panel sources. No builds/I/O to devices."""

import argparse
import hashlib
from pathlib import Path
import re


# Exact supplied Linux 6.12.63 extracts, not a newer distribution checkout.
SOURCES = {
    "drivers/gpu/drm/panel/panel-sitronix-st7703.c":
        "9fd768489ed122f093ed0e1e1ab908e8c307662f19c299fb11f41e90b97154ac",
    "drivers/gpu/drm/drm_mipi_dsi.c":
        "bc7f54962e425ba491cae54224349625d0bd2454952f3b7552772fc4208c5f52",
    "drivers/gpu/drm/bridge/synopsys/dw-mipi-dsi.c":
        "b7f49aa6d126b186413475c78fe4c7f8be18f71df1395bed8d206a41835ce01d",
    "drivers/mfd/rk8xx-i2c.c":
        "91fa25721d4fcec4d1c05122c36ed995134896c19ef624dd21d26668168cffe4",
    "drivers/regulator/rk808-regulator.c":
        "51ecb00c2fdd85a40cfed00e0fe61c2d29818b1b2ae798a6d5ab74e51c1247e3",
    "include/drm/drm_mipi_dsi.h":
        "b73fdbe6ea05d53f682c6b941d10ca7979f505c3ca2ac13ee7706d9d643ae816",
    "include/linux/mfd/rk808.h":
        "df8d67a5597d4734a666163be66511ac9d71c51acf2df936910b892636d3942d",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def one(pattern, source):
    matches = re.findall(pattern, source, re.S)
    require(len(matches) == 1, f"Expected one match: {pattern}")
    return matches[0]


def apply_patch(base, patch):
    """Apply this one-file unified diff in memory, with no offsets or fuzz."""
    lines = patch.splitlines(keepends=True)
    target = "drivers/gpu/drm/panel/panel-sitronix-st7703.c"
    require(lines[:2] == [f"--- a/{target}\n", f"+++ b/{target}\n"], "Patch target")
    original = base.splitlines(keepends=True)
    result, cursor, index = [], 0, 2
    while index < len(lines):
        header = re.fullmatch(r"@@ -(\d+),(\d+) \+(\d+),(\d+) @@\n", lines[index])
        require(header is not None, "Malformed hunk header")
        old_start, old_count, new_start, new_count = map(int, header.groups())
        index += 1
        old, new = [], []
        while index < len(lines) and not lines[index].startswith("@@"):
            line = lines[index]
            require(line[0] in " +-", "Malformed hunk body")
            if line[0] in " -":
                old.append(line[1:])
            if line[0] in " +":
                new.append(line[1:])
            index += 1
        require((len(old), len(new)) == (old_count, new_count), "Hunk counts")
        start = old_start - 1
        require(start >= cursor and original[start:start + old_count] == old, "Base context")
        result.extend(original[cursor:start])
        require(len(result) == new_start - 1, "New hunk position")
        result.extend(new)
        cursor = start + old_count
    result.extend(original[cursor:])
    return "".join(result)


def vendor_commands(vendor):
    stream = bytes.fromhex(one(r"panel-init-sequence\s*=\s*\[([^]]+)\]", vendor))
    commands, offset = [], 0
    while offset < len(stream):
        require(offset + 3 <= len(stream), "Truncated vendor header")
        kind, wait, size = stream[offset:offset + 3]
        offset += 3
        require(size > 0 and offset + size <= len(stream), "Truncated vendor payload")
        commands.append((kind, wait, stream[offset:offset + size]))
        offset += size
    require(len(commands) == 24, "Vendor command count")
    return commands


def candidate_commands(base, patched):
    # Resolve macros from the exact *base C*, not a hand-copied opcode table.
    constants = dict(re.findall(r"#define\s+(ST7703_CMD_\w+)\s+(0x[0-9a-fA-F]+)", base))
    body = one(r"static void r36tmax_init_sequence\([^)]*\)\s*\{(.*?)\n\}", patched)
    commands = []
    for args in re.findall(r"mipi_dsi_dcs_write_seq_multi\(dsi_ctx,\s*(.*?)\);", body, re.S):
        values = []
        for token in args.split(","):
            token = token.strip()
            values.append(int(constants.get(token, token), 16))
        commands.append(bytes(values))
    require(len(commands) == 22, "Candidate register command count")
    return commands


def compare(vendor, dts, base, patched):
    commands = vendor_commands(vendor)
    generic = [(bytes.fromhex(data), int(wait or "0", 16)) for data, wait in
               re.findall(r'"I seq=([0-9a-f]+)(?: wait=([0-9a-f]+))?"', dts)]
    candidate = candidate_commands(base, patched)
    require(len(generic) == 24, "Generic description command count")
    for index, (kind, wait, payload) in enumerate(commands):
        require(payload == generic[index][0], f"Generic payload mismatch at {index}")
        if index < 22:
            require(payload == candidate[index], f"Candidate payload mismatch at {index}")
            require(wait == generic[index][1] == 0, f"Register wait at {index}")
        # DSI wire types; exact drm_mipi_dsi.c selects the corresponding DCS
        # SHORT_WRITE / SHORT_WRITE_PARAM / LONG_WRITE symbols by buffer length.
        require(kind == {1: 0x05, 2: 0x15}.get(len(payload), 0x39), f"Packet type at {index}")
    require(commands[-2:] == [(5, 250, b"\x11"), (5, 50, b"\x29")], "Vendor tail")
    require(generic[-2:] == [(b"\x11", 592), (b"\x29", 80)], "Generic hexadecimal waits")
    return commands


def check_dispatch(sources, generic):
    core = sources["drivers/gpu/drm/drm_mipi_dsi.c"]
    header = sources["include/drm/drm_mipi_dsi.h"]
    host = sources["drivers/gpu/drm/bridge/synopsys/dw-mipi-dsi.c"]
    for fragment in ["case 1:\n\t\tmsg.type = MIPI_DSI_DCS_SHORT_WRITE;",
                     "case 2:\n\t\tmsg.type = MIPI_DSI_DCS_SHORT_WRITE_PARAM;",
                     "default:\n\t\tmsg.type = MIPI_DSI_DCS_LONG_WRITE;",
                     "msg->flags |= MIPI_DSI_MSG_USE_LPM;"]:
        require(fragment in core, f"Core dispatch: {fragment}")
    require("mipi_dsi_dcs_write_buffer_multi(ctx, d, ARRAY_SIZE(d))" in header, "Multi dispatch")
    require("ret = mipi_dsi_dcs_write_buffer(dsi, data, len);" in core, "Multi buffer dispatch")
    require("val |= CMD_MODE_ALL_LP;" in host and "val |= ENABLE_LOW_POWER_CMD;" in host,
            "Host LP command configuration")
    require("item->wait = simple_strtoul(val, NULL, 16);" in generic, "Wait radix changed")
    require("mipi_dsi_dcs_write_buffer(dsi, iseq->data, iseq->len)" in generic, "Generic dispatch")


def self_test(vendor, dts, base, patched):
    # Mutations stay in memory; use the real parsers and comparison contract.
    cases = [
        (vendor.replace("b9 f1 12 83", "b9 f0 12 83", 1), dts, base, patched),
        (vendor.replace("39 00 04 b9", "15 00 04 b9", 1), dts, base, patched),
        (vendor, dts.replace("wait=250", "wait=fa"), base, patched),
        (vendor, dts, base.replace("ST7703_CMD_SETEXTC\t 0xB9", "ST7703_CMD_SETEXTC\t 0xB0"), patched),
    ]
    for args in cases:
        try:
            compare(*args)
        except ValueError:
            continue
        raise ValueError("A mutation escaped detection")
    print(f"PASS {len(cases)} negative controls (payload, packet type, wait radix, base opcode)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--linux-source", type=Path, required=True)
    parser.add_argument("--vendor-dts", type=Path, required=True)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    sources = {}
    for name, digest in SOURCES.items():
        data = (args.linux_source / name).read_bytes()
        require(hashlib.sha256(data).hexdigest() == digest, f"Not the audited source: {name}")
        sources[name] = data.decode()
    directory = Path(__file__).resolve().parent
    dts = (directory / "rk3326-aislpc-r36t-max.dts").read_text()
    generic = (directory / "drivers/panel-generic-dsi.c").read_text()
    base = sources["drivers/gpu/drm/panel/panel-sitronix-st7703.c"]
    patched = apply_patch(base, (directory / "panel-st7703-r36t-max.patch").read_text())
    vendor = args.vendor_dts.read_text()
    require('compatible = "rocknix,generic-dsi";' in dts, "Selected compatible changed")
    check_dispatch(sources, generic)
    commands = compare(vendor, dts, base, patched)
    backlight = one(r"\n\tbacklight \{(.*?)\n\t\};", vendor)
    require("power-supply" not in backlight, "Vendor backlight supply changed")
    print("PASS exact hashes (7 Linux files), strict in-memory patch application, DCS/LPM dispatch")
    print("cmd bytes type vendor-wait-ms")
    for kind, wait, payload in commands:
        print(f"{payload[0]:02x}  {len(payload):2d}    {kind:02x}   {wait}")
    print("PASS 22 register payloads: vendor = generic = candidate; all 24 vendor packet types match")
    print("PASS vendor waits 250/50 ms; generic waits 592/80 ms; vendor backlight has no supply")
    print(f"Vendor SHA256: {hashlib.sha256(vendor.encode()).hexdigest()}")
    if args.self_test:
        self_test(vendor, dts, base, patched)


if __name__ == "__main__":
    main()
